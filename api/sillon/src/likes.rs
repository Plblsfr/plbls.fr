use axum::{
    extract::{Path, State},
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    bibliotheque::{COLONNES_ALBUM, COLONNES_ARTISTE, album_public, artiste_public, uuid},
    etat::AppState,
    garde::Personne,
    http::{bd, horodatage, introuvable, ok, sans_contenu},
    playlists::{pistes_json, plafond},
};

pub const LIKES_MAX: i64 = 50_000;

fn genre(kind: &str) -> Result<(&'static str, &'static str), Response> {
    match kind {
        "tracks" => Ok(("track", "pistes")),
        "albums" => Ok(("album", "albums")),
        "artists" => Ok(("artist", "artistes")),
        _ => Err(introuvable()),
    }
}

pub async fn identifiants(
    State(state): State<AppState>,
    p: Personne,
) -> Result<Response, Response> {
    let lignes =
        sqlx::query("SELECT type, cible FROM likes WHERE sub = $1 ORDER BY cree_le DESC, cible")
            .bind(p.sub())
            .fetch_all(state.pool())
            .await
            .map_err(bd)?;
    let de = |t: &str| -> Vec<Uuid> {
        lignes
            .iter()
            .filter(|l| l.get::<String, _>(0) == t)
            .map(|l| l.get(1))
            .collect()
    };
    Ok(ok(&json!({
        "tracks": de("track"),
        "albums": de("album"),
        "artists": de("artist"),
    })))
}

pub async fn liste(
    State(state): State<AppState>,
    p: Personne,
    Path(kind): Path<String>,
) -> Result<Response, Response> {
    let (t, _) = genre(&kind)?;
    let pool = state.pool();
    let v = match t {
        "track" => {
            let lignes = sqlx::query(
                "SELECT cible, cree_le FROM likes WHERE sub = $1 AND type = 'track' ORDER BY cree_le DESC, cible",
            )
            .bind(p.sub())
            .fetch_all(pool)
            .await
            .map_err(bd)?;
            let ids: Vec<Uuid> = lignes.iter().map(|l| l.get(0)).collect();
            let pistes = pistes_json(pool, &ids).await.map_err(bd)?;
            let resume = sqlx::query(
                "SELECT sum(p.duree),
                   ARRAY(SELECT p2.album_id FROM likes l2 JOIN pistes p2 ON p2.id = l2.cible JOIN albums al ON al.id = p2.album_id
                         WHERE l2.sub = $1 AND l2.type = 'track' AND al.pochette IS NOT NULL
                         GROUP BY p2.album_id ORDER BY max(l2.cree_le) DESC LIMIT 4)
                 FROM likes l JOIN pistes p ON p.id = l.cible WHERE l.sub = $1 AND l.type = 'track'",
            )
            .bind(p.sub())
            .fetch_one(pool)
            .await
            .map_err(bd)?;
            json!({
                "name": "Titres likés",
                "track_count": ids.len(),
                "duration": resume.get::<Option<f64>, _>(0),
                "cover_albums": resume.get::<Vec<Uuid>, _>(1),
                "items": lignes.iter().zip(pistes).map(|(l, piste)| json!({
                    "liked_at": horodatage(&l.get::<DateTime<Utc>, _>(1)),
                    "track": piste,
                })).collect::<Vec<_>>(),
            })
        }
        "album" => {
            let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {COLONNES_ALBUM}, l.cree_le AS aime_le FROM likes l JOIN albums al ON al.id = l.cible
                 JOIN artistes ar ON ar.id = al.artiste_id WHERE l.sub = $1 AND l.type = 'album'
                 ORDER BY l.cree_le DESC, al.id"
            )))
            .bind(p.sub())
            .fetch_all(pool)
            .await
            .map_err(bd)?;
            json!({ "items": lignes.iter().map(|l| avec_date(album_public(l), l)).collect::<Vec<_>>() })
        }
        _ => {
            let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {COLONNES_ARTISTE}, l.cree_le AS aime_le FROM likes l JOIN artistes ar ON ar.id = l.cible
                 WHERE l.sub = $1 AND l.type = 'artist' ORDER BY l.cree_le DESC, ar.id"
            )))
            .bind(p.sub())
            .fetch_all(pool)
            .await
            .map_err(bd)?;
            json!({ "items": lignes.iter().map(|l| avec_date(artiste_public(l), l)).collect::<Vec<_>>() })
        }
    };
    Ok(ok(&v))
}

fn avec_date(mut v: Value, l: &sqlx::postgres::PgRow) -> Value {
    v["liked_at"] = json!(horodatage(&l.get::<DateTime<Utc>, _>("aime_le")));
    v
}

pub async fn aimer(
    State(state): State<AppState>,
    p: Personne,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Response, Response> {
    let (t, table) = genre(&kind)?;
    let id = uuid(&id)?;
    let existe: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 FROM {table} WHERE id = $1)"
    )))
    .bind(id)
    .fetch_one(state.pool())
    .await
    .map_err(bd)?;
    if !existe {
        return Err(introuvable());
    }
    let nombre: i64 = sqlx::query_scalar("SELECT count(*) FROM likes WHERE sub = $1 AND type = $2")
        .bind(p.sub())
        .bind(t)
        .fetch_one(state.pool())
        .await
        .map_err(bd)?;
    if nombre >= LIKES_MAX {
        return Err(plafond(&format!("{LIKES_MAX} likes par type au plus")));
    }
    sqlx::query("INSERT INTO likes (sub, type, cible) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(p.sub())
        .bind(t)
        .bind(id)
        .execute(state.pool())
        .await
        .map_err(bd)?;
    Ok(sans_contenu())
}

pub async fn retirer(
    State(state): State<AppState>,
    p: Personne,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Response, Response> {
    let (t, _) = genre(&kind)?;
    sqlx::query("DELETE FROM likes WHERE sub = $1 AND type = $2 AND cible = $3")
        .bind(p.sub())
        .bind(t)
        .bind(uuid(&id)?)
        .execute(state.pool())
        .await
        .map_err(bd)?;
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        playlists::tests::{catalogue, pistes_de},
        testutil::env,
    };

    #[tokio::test]
    async fn likes_et_titres_likes_lisibles_comme_une_playlist() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 4).await;
        for piste in [&p[2], &p[0], &p[2]] {
            let r = e
                .appel(
                    Some("alice"),
                    "PUT",
                    &format!("/likes/tracks/{piste}"),
                    None,
                )
                .await;
            assert_eq!(r.statut, StatusCode::NO_CONTENT);
        }
        let piste = e.get("alice", &format!("/tracks/{}", p[0])).await.json();
        let (album, artiste) = (
            piste["album"]["id"].as_str().unwrap().to_string(),
            piste["artist"]["id"].as_str().unwrap().to_string(),
        );
        e.appel(
            Some("alice"),
            "PUT",
            &format!("/likes/albums/{album}"),
            None,
        )
        .await;
        e.appel(
            Some("alice"),
            "PUT",
            &format!("/likes/artists/{artiste}"),
            None,
        )
        .await;
        let ids = e.get("alice", "/likes").await.json();
        assert_eq!(
            ids,
            json!({"tracks": [p[0], p[2]], "albums": [album], "artists": [artiste]})
        );
        let t = e.get("alice", "/likes/tracks").await.json();
        assert_eq!(t["name"], "Titres likés");
        assert_eq!(t["track_count"], 2);
        assert_eq!(t["duration"], 400.0);
        assert_eq!(pistes_de(&t), [p[0].clone(), p[2].clone()]);
        assert!(t["items"][0]["liked_at"].is_string());
        assert_eq!(t["cover_albums"].as_array().unwrap().len(), 1);
        let a = e.get("alice", "/likes/albums").await.json();
        assert_eq!(a["items"][0]["id"], album.as_str());
        assert!(a["items"][0]["liked_at"].is_string());
        assert_eq!(
            e.get("alice", "/likes/artists").await.json()["items"][0]["name"],
            "Artiste"
        );
        let f = e
            .appel(
                Some("alice"),
                "PUT",
                "/queue",
                Some(json!({"revision": 0, "source": {"type": "likes"}, "endless": false})),
            )
            .await
            .json();
        assert_eq!(
            f["source"],
            json!({"type": "likes", "id": null, "name": "Titres likés"})
        );
        assert_eq!(pistes_de(&f), [p[0].clone(), p[2].clone()]);
        assert_eq!(
            e.appel(
                Some("alice"),
                "DELETE",
                &format!("/likes/tracks/{}", p[0]),
                None
            )
            .await
            .statut,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            e.get("alice", "/likes").await.json()["tracks"],
            json!([p[2]])
        );
        assert_eq!(
            e.get("bruno", "/likes").await.json(),
            json!({"tracks": [], "albums": [], "artists": []})
        );
        for (m, chemin) in [
            ("PUT", format!("/likes/tracks/{}", uuid::Uuid::new_v4())),
            ("PUT", format!("/likes/albums/{}", p[0])),
            ("PUT", format!("/likes/genres/{}", p[0])),
            ("GET", "/likes/genres".to_string()),
            ("PUT", "/likes/tracks/pas-un-uuid".to_string()),
        ] {
            assert_eq!(
                e.appel(Some("alice"), m, &chemin, None).await.statut,
                StatusCode::NOT_FOUND,
                "{m} {chemin}"
            );
        }
    }
}
