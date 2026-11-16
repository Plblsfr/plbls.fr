use std::collections::{HashMap, HashSet};

use axum::{
    body::{Body, to_bytes},
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::{
    bibliotheque::{COLONNES_PISTE, JOINTURES_PISTE, piste_publique, uuid},
    etat::AppState,
    garde::Personne,
    http::{bd, erreur, horodatage, introuvable, invalide, json, ok, sans_contenu},
    ordre,
};

pub const PLAYLISTS_MAX: i64 = 500;
pub const PISTES_MAX: usize = 10_000;
pub const NOM_MAX: usize = 100;
pub const DESCRIPTION_MAX: usize = 1000;
const CORPS_MAX: usize = 1024 * 1024;

pub async fn objet(corps: Body) -> Result<Map<String, Value>, Response> {
    let octets = to_bytes(corps, CORPS_MAX)
        .await
        .map_err(|_| invalide("corps trop gros ou illisible"))?;
    if octets.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(&octets) {
        Ok(Value::Object(o)) => Ok(o),
        _ => Err(invalide("objet JSON attendu")),
    }
}

pub fn plafond(message: &str) -> Response {
    erreur(StatusCode::UNPROCESSABLE_ENTITY, "limit_reached", message)
}

pub fn texte(
    corps: &Map<String, Value>,
    nom: &str,
    max: usize,
    vide: bool,
) -> Result<Option<String>, Response> {
    match corps.get(nom) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => {
            let t = t.trim();
            if t.is_empty() && !vide {
                return Err(invalide(&format!("{nom} vide")));
            }
            if t.chars().count() > max {
                return Err(invalide(&format!(
                    "{nom} trop long ({max} caractères au plus)"
                )));
            }
            if t.chars().any(|c| c.is_control() && c != '\n') {
                return Err(invalide(&format!("{nom} invalide")));
            }
            Ok(Some(t.to_string()))
        }
        Some(_) => Err(invalide(&format!("{nom} doit être un texte"))),
    }
}

pub fn champ_uuid(corps: &Map<String, Value>, nom: &str) -> Result<Option<Uuid>, Response> {
    match corps.get(nom) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(Some)
            .ok_or_else(|| invalide(&format!("{nom} : identifiant invalide"))),
    }
}

pub fn entier(corps: &Map<String, Value>, nom: &str) -> Result<Option<usize>, Response> {
    match corps.get(nom) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| invalide(&format!("{nom} : entier positif attendu"))),
    }
}

pub fn booleen(corps: &Map<String, Value>, nom: &str) -> Result<Option<bool>, Response> {
    match corps.get(nom) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(invalide(&format!("{nom} : booléen attendu"))),
    }
}

pub async fn pistes_json(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Value>, sqlx::Error> {
    let uniques: Vec<Uuid> = ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_PISTE} {JOINTURES_PISTE} WHERE p.id = ANY($1)"
    )))
    .bind(&uniques)
    .fetch_all(pool)
    .await?;
    let par_id: HashMap<Uuid, Value> = lignes
        .iter()
        .map(|l| (l.get::<Uuid, _>("id"), piste_publique(l)))
        .collect();
    Ok(ids
        .iter()
        .map(|id| par_id.get(id).cloned().unwrap_or(Value::Null))
        .collect())
}

pub async fn pistes_de_source(
    pool: &PgPool,
    sub: &str,
    genre: &str,
    id: Option<Uuid>,
) -> Result<(Vec<Uuid>, String), Response> {
    let requis = || id.ok_or_else(|| invalide("source : identifiant requis"));
    let (nom_sql, pistes_sql) = match genre {
        "album" => (
            "SELECT titre FROM albums WHERE id = $1 AND $2::text IS NOT NULL",
            "SELECT id FROM pistes WHERE album_id = $1 AND $2::text IS NOT NULL ORDER BY disque, numero NULLS LAST, cle, id",
        ),
        "artist" => (
            "SELECT nom FROM artistes WHERE id = $1 AND $2::text IS NOT NULL",
            "SELECT p.id FROM pistes p JOIN albums al ON al.id = p.album_id
             WHERE p.artiste_id = $1 AND p.present AND $2::text IS NOT NULL
             ORDER BY al.sortie NULLS LAST, al.cle, al.id, p.disque, p.numero NULLS LAST, p.cle, p.id",
        ),
        "playlist" => (
            "SELECT nom FROM playlists WHERE id = $1 AND sub = $2",
            "SELECT pp.piste_id FROM playlists_pistes pp JOIN playlists pl ON pl.id = pp.playlist_id
             WHERE pl.id = $1 AND pl.sub = $2 ORDER BY pp.position",
        ),
        "likes" => {
            let pistes = sqlx::query_scalar(
                "SELECT cible FROM likes WHERE sub = $1 AND type = 'track' ORDER BY cree_le DESC, cible",
            )
            .bind(sub)
            .fetch_all(pool)
            .await
            .map_err(bd)?;
            return Ok((pistes, "Titres likés".into()));
        }
        _ => return Err(invalide("source inconnue")),
    };
    let id = requis()?;
    let nom: String = sqlx::query_scalar(nom_sql)
        .bind(id)
        .bind(sub)
        .fetch_optional(pool)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let pistes = sqlx::query_scalar(pistes_sql)
        .bind(id)
        .bind(sub)
        .fetch_all(pool)
        .await
        .map_err(bd)?;
    Ok((pistes, nom))
}

pub async fn contenu(
    pool: &PgPool,
    sub: &str,
    corps: &Map<String, Value>,
) -> Result<Vec<Uuid>, Response> {
    let donnes: Vec<&str> = ["tracks", "album", "playlist", "artist"]
        .into_iter()
        .filter(|k| corps.get(*k).is_some_and(|v| !v.is_null()))
        .collect();
    if donnes.len() != 1 {
        return Err(invalide(
            "un seul contenu attendu : tracks, album, playlist ou artist",
        ));
    }
    let pistes = match donnes[0] {
        "tracks" => {
            let liste = corps["tracks"]
                .as_array()
                .ok_or_else(|| invalide("tracks : liste attendue"))?;
            if liste.len() > PISTES_MAX {
                return Err(plafond(&format!("{PISTES_MAX} pistes au plus")));
            }
            let ids = liste
                .iter()
                .map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Option<Vec<Uuid>>>()
                .ok_or_else(|| invalide("tracks : identifiant invalide"))?;
            let connues: i64 = sqlx::query_scalar("SELECT count(*) FROM pistes WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_one(pool)
                .await
                .map_err(bd)?;
            if usize::try_from(connues).unwrap_or(0) != ids.iter().collect::<HashSet<_>>().len() {
                return Err(invalide("tracks : piste inconnue"));
            }
            ids
        }
        genre => {
            let id = champ_uuid(corps, genre)?;
            pistes_de_source(pool, sub, genre, id).await?.0
        }
    };
    Ok(pistes)
}

const COLONNES_PLAYLIST: &str = "pl.id, pl.nom, pl.description, pl.cree_le, pl.modifie_le,
    (SELECT count(*) FROM playlists_pistes pp WHERE pp.playlist_id = pl.id) AS nombre,
    (SELECT sum(p.duree) FROM playlists_pistes pp JOIN pistes p ON p.id = pp.piste_id WHERE pp.playlist_id = pl.id) AS duree,
    ARRAY(SELECT p.album_id FROM playlists_pistes pp JOIN pistes p ON p.id = pp.piste_id JOIN albums al ON al.id = p.album_id
          WHERE pp.playlist_id = pl.id AND al.pochette IS NOT NULL
          GROUP BY p.album_id ORDER BY min(pp.position) LIMIT 4) AS pochettes";

fn playlist_publique(l: &PgRow) -> Value {
    json!({
        "id": l.get::<Uuid, _>("id"),
        "name": l.get::<String, _>("nom"),
        "description": l.get::<String, _>("description"),
        "track_count": l.get::<i64, _>("nombre"),
        "duration": l.get::<Option<f64>, _>("duree"),
        "cover_albums": l.get::<Vec<Uuid>, _>("pochettes"),
        "created_at": horodatage(&l.get::<DateTime<Utc>, _>("cree_le")),
        "updated_at": horodatage(&l.get::<DateTime<Utc>, _>("modifie_le")),
    })
}

async fn detail(pool: &PgPool, sub: &str, id: Uuid) -> Result<Value, Response> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_PLAYLIST} FROM playlists pl WHERE pl.id = $1 AND pl.sub = $2"
    )))
    .bind(id)
    .bind(sub)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let elements = sqlx::query(
        "SELECT id, piste_id, position, ajoute_le FROM playlists_pistes WHERE playlist_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let ids: Vec<Uuid> = elements.iter().map(|e| e.get("piste_id")).collect();
    let pistes = pistes_json(pool, &ids).await.map_err(bd)?;
    let mut v = playlist_publique(&l);
    v["items"] = elements
        .iter()
        .zip(pistes)
        .map(|(e, piste)| {
            json!({
                "id": e.get::<Uuid, _>("id"),
                "position": e.get::<i32, _>("position"),
                "added_at": horodatage(&e.get::<DateTime<Utc>, _>("ajoute_le")),
                "track": piste,
            })
        })
        .collect();
    Ok(v)
}

async fn verrouiller(
    tx: &mut Transaction<'_, Postgres>,
    sub: &str,
    id: Uuid,
) -> Result<(), Response> {
    sqlx::query("UPDATE playlists SET modifie_le = now() WHERE id = $1 AND sub = $2 RETURNING id")
        .bind(id)
        .bind(sub)
        .fetch_optional(&mut **tx)
        .await
        .map_err(bd)?
        .map(|_| ())
        .ok_or_else(introuvable)
}

pub async fn lister(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_PLAYLIST} FROM playlists pl WHERE pl.sub = $1 ORDER BY pl.modifie_le DESC, pl.id"
    )))
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(playlist_publique).collect::<Vec<_>>(),
    })))
}

pub async fn creer(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let nom = texte(&corps, "name", NOM_MAX, false)?.ok_or_else(|| invalide("name requis"))?;
    let description = texte(&corps, "description", DESCRIPTION_MAX, true)?.unwrap_or_default();
    let mut tx = state.pool().begin().await.map_err(bd)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('playlists:' || $1))")
        .bind(p.sub())
        .execute(&mut *tx)
        .await
        .map_err(bd)?;
    let nombre: i64 = sqlx::query_scalar("SELECT count(*) FROM playlists WHERE sub = $1")
        .bind(p.sub())
        .fetch_one(&mut *tx)
        .await
        .map_err(bd)?;
    if nombre >= PLAYLISTS_MAX {
        return Err(plafond(&format!("{PLAYLISTS_MAX} playlists au plus")));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlists (sub, nom, description) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(p.sub())
    .bind(&nom)
    .bind(&description)
    .fetch_one(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(json(
        StatusCode::CREATED,
        &detail(state.pool(), p.sub(), id).await?,
    ))
}

pub async fn lire(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    Ok(ok(&detail(state.pool(), p.sub(), uuid(&id)?).await?))
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps = objet(corps).await?;
    let nom = texte(&corps, "name", NOM_MAX, false)?;
    let description = texte(&corps, "description", DESCRIPTION_MAX, true)?;
    sqlx::query(
        "UPDATE playlists SET nom = coalesce($3, nom), description = coalesce($4, description), modifie_le = now()
         WHERE id = $1 AND sub = $2 RETURNING id",
    )
    .bind(id)
    .bind(p.sub())
    .bind(nom)
    .bind(description)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    Ok(ok(&detail(state.pool(), p.sub(), id).await?))
}

pub async fn supprimer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let r = sqlx::query("DELETE FROM playlists WHERE id = $1 AND sub = $2")
        .bind(id)
        .bind(p.sub())
        .execute(state.pool())
        .await
        .map_err(bd)?;
    if r.rows_affected() == 0 {
        return Err(introuvable());
    }
    Ok(sans_contenu())
}

pub async fn ajouter(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps = objet(corps).await?;
    let doublons_permis = booleen(&corps, "allow_duplicates")?.unwrap_or(false);
    let pistes = contenu(state.pool(), p.sub(), &corps).await?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    verrouiller(&mut tx, p.sub(), id).await?;
    let mut deja: HashSet<Uuid> =
        sqlx::query_scalar("SELECT piste_id FROM playlists_pistes WHERE playlist_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .map_err(bd)?
            .into_iter()
            .collect();
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM playlists_pistes WHERE playlist_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(bd)?;
    let total = usize::try_from(total).unwrap_or(0);
    let mut doublons = Vec::new();
    let mut ajout = Vec::new();
    for piste in pistes {
        if !deja.insert(piste) {
            doublons.push(piste);
            if !doublons_permis {
                continue;
            }
        }
        ajout.push(piste);
    }
    if total + ajout.len() > PISTES_MAX {
        return Err(plafond(&format!(
            "{PISTES_MAX} pistes par playlist au plus"
        )));
    }
    sqlx::query(
        "INSERT INTO playlists_pistes (playlist_id, piste_id, position)
         SELECT $1, piste, $3 + rang - 1 FROM unnest($2::uuid[]) WITH ORDINALITY AS a(piste, rang)",
    )
    .bind(id)
    .bind(&ajout)
    .bind(i32::try_from(total).unwrap_or(i32::MAX))
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    let mut v = detail(state.pool(), p.sub(), id).await?;
    v["added"] = json!(ajout.len());
    v["duplicates"] = json!(doublons);
    Ok(ok(&v))
}

pub async fn retirer(
    State(state): State<AppState>,
    p: Personne,
    Path((id, element)): Path<(String, String)>,
) -> Result<Response, Response> {
    let (id, element) = (uuid(&id)?, uuid(&element)?);
    let mut tx = state.pool().begin().await.map_err(bd)?;
    verrouiller(&mut tx, p.sub(), id).await?;
    let position: i32 = sqlx::query_scalar(
        "DELETE FROM playlists_pistes WHERE playlist_id = $1 AND id = $2 RETURNING position",
    )
    .bind(id)
    .bind(element)
    .fetch_optional(&mut *tx)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    sqlx::query(
        "UPDATE playlists_pistes SET position = position - 1 WHERE playlist_id = $1 AND position > $2",
    )
    .bind(id)
    .bind(position)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&detail(state.pool(), p.sub(), id).await?))
}

pub async fn deplacer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps = objet(corps).await?;
    let depuis = entier(&corps, "from")?.ok_or_else(|| invalide("from requis"))?;
    let nombre = entier(&corps, "count")?.unwrap_or(1);
    let vers = entier(&corps, "to")?.ok_or_else(|| invalide("to requis"))?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    verrouiller(&mut tx, p.sub(), id).await?;
    let mut ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM playlists_pistes WHERE playlist_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(bd)?;
    if !ordre::deplacer(&mut ids, depuis, nombre, vers) {
        return Err(invalide("plage hors de la playlist"));
    }
    sqlx::query(
        "UPDATE playlists_pistes pp SET position = a.rang - 1
         FROM unnest($2::uuid[]) WITH ORDINALITY AS a(id, rang)
         WHERE pp.playlist_id = $1 AND pp.id = a.id",
    )
    .bind(id)
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&detail(state.pool(), p.sub(), id).await?))
}

#[cfg(test)]
pub(crate) mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use sqlx::PgPool;

    use crate::testutil::{Env, appel_sur, env};

    pub(crate) async fn catalogue(pool: &PgPool, n: i32) -> Vec<String> {
        let artiste: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO artistes (mbid, nom, nom_tri, present) VALUES (gen_random_uuid()::text, 'Artiste', 'Artiste', true) RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let mut albums = Vec::new();
        for (titre, pochette) in [
            ("Premier", Some("v1")),
            ("Second", None),
            ("Troisième", Some("v3")),
        ] {
            let id: uuid::Uuid = sqlx::query_scalar(
                "INSERT INTO albums (mbid, artiste_id, titre, pochette, present) VALUES (gen_random_uuid()::text, $1, $2, $3, true) RETURNING id",
            )
            .bind(artiste)
            .bind(titre)
            .bind(pochette)
            .fetch_one(pool)
            .await
            .unwrap();
            albums.push(id);
        }
        sqlx::query(
            "INSERT INTO pistes (album_id, artiste_id, mbid_enregistrement, titre, numero, duree, present)
             SELECT ($1::uuid[])[1 + i % 3], $2, 'rec-' || i, 'Piste ' || i, i, 200, true FROM generate_series(1, $3) AS i",
        )
        .bind(&albums)
        .bind(artiste)
        .bind(n)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query_scalar("SELECT id::text FROM pistes ORDER BY numero")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    pub(crate) async fn creer(e: &Env, qui: &str, nom: &str) -> String {
        let r = e
            .appel(Some(qui), "POST", "/playlists", Some(json!({"name": nom})))
            .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        r.json()["id"].as_str().unwrap().to_string()
    }

    pub(crate) fn pistes_de(v: &Value) -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["track"]["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn playlist_creee_remplie_doublons_signales_et_pochette() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 6).await;
        let id = creer(&e, "alice", "  Été  ").await;
        let chemin = format!("/playlists/{id}/tracks");
        let r = e
            .appel(
                Some("alice"),
                "POST",
                &chemin,
                Some(json!({"tracks": [p[0], p[1], p[0]]})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        let v = r.json();
        assert_eq!(v["name"], "Été");
        assert_eq!(
            (v["added"].clone(), v["duplicates"].clone()),
            (json!(2), json!([p[0]]))
        );
        assert_eq!(pistes_de(&v), [p[0].clone(), p[1].clone()]);
        let v = e
            .appel(
                Some("alice"),
                "POST",
                &chemin,
                Some(json!({"tracks": [p[1], p[2]], "allow_duplicates": true})),
            )
            .await
            .json();
        assert_eq!(
            (v["added"].clone(), v["duplicates"].clone()),
            (json!(2), json!([p[1]]))
        );
        assert_eq!(v["track_count"], 4);
        assert_eq!(v["duration"], 800.0);
        let positions: Vec<i64> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["position"].as_i64().unwrap())
            .collect();
        assert_eq!(positions, [0, 1, 2, 3]);
        let albums: Vec<Value> = [1, 3]
            .iter()
            .map(|i| v["items"][*i]["track"]["album"]["id"].clone())
            .collect();
        assert_eq!(
            v["cover_albums"],
            json!(albums),
            "albums avec pochette, dans l'ordre"
        );

        let autre = creer(&e, "alice", "Autre").await;
        let v = e
            .appel(
                Some("alice"),
                "POST",
                &format!("/playlists/{autre}/tracks"),
                Some(json!({"playlist": id})),
            )
            .await
            .json();
        assert_eq!(pistes_de(&v), [p[0].clone(), p[1].clone(), p[2].clone()]);
        assert_eq!(v["duplicates"], json!([p[1]]));
        let album = v["items"][0]["track"]["album"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let v = e
            .appel(
                Some("alice"),
                "POST",
                &format!("/playlists/{autre}/tracks"),
                Some(json!({"album": album})),
            )
            .await
            .json();
        assert_eq!(v["track_count"], 4);
        for corps in [
            json!({}),
            json!({"tracks": [p[0]], "album": album}),
            json!({"tracks": ["x"]}),
            json!({"tracks": [uuid::Uuid::new_v4()]}),
        ] {
            let r = e
                .appel(Some("alice"), "POST", &chemin, Some(corps.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
        }

        let element = e.get("alice", &format!("/playlists/{id}")).await.json()["items"][1]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let v = e
            .appel(
                Some("alice"),
                "DELETE",
                &format!("/playlists/{id}/tracks/{element}"),
                None,
            )
            .await
            .json();
        assert_eq!(pistes_de(&v), [p[0].clone(), p[1].clone(), p[2].clone()]);
        let positions: Vec<i64> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["position"].as_i64().unwrap())
            .collect();
        assert_eq!(positions, [0, 1, 2]);
        let v = e
            .appel(
                Some("alice"),
                "POST",
                &format!("/playlists/{id}/move"),
                Some(json!({"from": 1, "count": 2, "to": 0})),
            )
            .await
            .json();
        assert_eq!(pistes_de(&v), [p[1].clone(), p[2].clone(), p[0].clone()]);
        let r = e
            .appel(
                Some("alice"),
                "POST",
                &format!("/playlists/{id}/move"),
                Some(json!({"from": 2, "count": 2, "to": 0})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);

        let v = e
            .appel(
                Some("alice"),
                "PATCH",
                &format!("/playlists/{id}"),
                Some(json!({"description": "Pour l'été"})),
            )
            .await
            .json();
        assert_eq!(
            (v["name"].clone(), v["description"].clone()),
            (json!("Été"), json!("Pour l'été"))
        );
        let liste = e.get("alice", "/playlists").await.json();
        let noms: Vec<&str> = liste["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(noms, ["Été", "Autre"]);
        assert!(
            e.get("bruno", "/playlists").await.json()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            e.appel(Some("alice"), "DELETE", &format!("/playlists/{id}"), None)
                .await
                .statut,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            e.get("alice", &format!("/playlists/{id}")).await.statut,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn playlist_d_une_autre_personne_ni_lue_ni_modifiee() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 3).await;
        let id = creer(&e, "alice", "Privée").await;
        let v = e
            .appel(
                Some("alice"),
                "POST",
                &format!("/playlists/{id}/tracks"),
                Some(json!({"tracks": [p[0]]})),
            )
            .await
            .json();
        let element = v["items"][0]["id"].as_str().unwrap().to_string();
        let sienne = creer(&e, "bruno", "Sienne").await;
        let appels = [
            ("GET", format!("/playlists/{id}"), None),
            (
                "PATCH",
                format!("/playlists/{id}"),
                Some(json!({"name": "Volée"})),
            ),
            ("DELETE", format!("/playlists/{id}"), None),
            (
                "POST",
                format!("/playlists/{id}/tracks"),
                Some(json!({"tracks": [p[1]]})),
            ),
            ("DELETE", format!("/playlists/{id}/tracks/{element}"), None),
            (
                "POST",
                format!("/playlists/{id}/move"),
                Some(json!({"from": 0, "to": 0})),
            ),
            (
                "POST",
                format!("/playlists/{sienne}/tracks"),
                Some(json!({"playlist": id})),
            ),
            (
                "PUT",
                "/queue".to_string(),
                Some(json!({"revision": 0, "source": {"type": "playlist", "id": id}})),
            ),
            (
                "POST",
                "/queue/tracks".to_string(),
                Some(json!({"revision": 0, "playlist": id})),
            ),
        ];
        for (m, chemin, corps) in appels {
            let r = e.appel(Some("bruno"), m, &chemin, corps).await;
            assert_eq!(
                r.statut,
                StatusCode::NOT_FOUND,
                "{m} {chemin} : {}",
                r.texte()
            );
        }
        let v = e.get("alice", &format!("/playlists/{id}")).await.json();
        assert_eq!(
            (v["name"].clone(), pistes_de(&v)),
            (json!("Privée"), vec![p[0].clone()])
        );
    }

    #[tokio::test]
    async fn deplacements_et_ajouts_concurrents_sans_trou_ni_doublon() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 30).await;
        let id = creer(&e, "alice", "Concurrence").await;
        let chemin = format!("/playlists/{id}/tracks");
        e.appel(
            Some("alice"),
            "POST",
            &chemin,
            Some(json!({"tracks": p[..20]})),
        )
        .await;
        let mut taches = Vec::new();
        for i in 0..40usize {
            let app = e.app.clone();
            let (id, ajout) = (id.clone(), p[20 + i % 10].clone());
            taches.push(tokio::spawn(async move {
                let (chemin, corps) = if i % 4 == 0 {
                    (
                        format!("/playlists/{id}/tracks"),
                        json!({"tracks": [ajout], "allow_duplicates": true}),
                    )
                } else {
                    (
                        format!("/playlists/{id}/move"),
                        json!({"from": (i * 7) % 15, "count": 1 + i % 5, "to": (i * 3) % 15}),
                    )
                };
                appel_sur(&app, Some("alice"), "POST", &chemin, Some(corps), &[])
                    .await
                    .statut
            }));
        }
        for t in taches {
            assert_eq!(t.await.unwrap(), StatusCode::OK);
        }
        let v = e.get("alice", &format!("/playlists/{id}")).await.json();
        let positions: Vec<i64> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["position"].as_i64().unwrap())
            .collect();
        assert_eq!(positions, (0..30).collect::<Vec<i64>>());
        let mut vues = pistes_de(&v);
        vues.sort();
        let mut attendues: Vec<String> = p[..20].to_vec();
        attendues.extend((0..40).step_by(4).map(|i| p[20 + i % 10].clone()));
        attendues.sort();
        assert_eq!(vues, attendues);
    }

    #[tokio::test]
    async fn plafonds_refuses_avec_une_erreur_lisible() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 2).await;
        for corps in [
            json!({"name": "a".repeat(101)}),
            json!({"name": "  "}),
            json!({"name": "x", "description": "d".repeat(1001)}),
            json!({"name": 3}),
        ] {
            let r = e
                .appel(Some("alice"), "POST", "/playlists", Some(corps.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
            assert_eq!(r.json()["error"], "invalid");
            assert!(
                r.json()["message"].as_str().unwrap().len() > 5,
                "{}",
                r.texte()
            );
        }
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/playlists",
                Some(json!({"name": "a".repeat(101)})),
            )
            .await;
        assert_eq!(
            r.json()["message"],
            "name trop long (100 caractères au plus)"
        );
        let id = creer(&e, "alice", "Pleine").await;
        sqlx::query(
            "INSERT INTO playlists_pistes (playlist_id, piste_id, position) SELECT $1, $2, i FROM generate_series(0, 9999) AS i",
        )
        .bind(uuid::Uuid::parse_str(&id).unwrap())
        .bind(uuid::Uuid::parse_str(&p[0]).unwrap())
        .execute(e.state.pool())
        .await
        .unwrap();
        let r = e
            .appel(
                Some("alice"),
                "POST",
                &format!("/playlists/{id}/tracks"),
                Some(json!({"tracks": [p[1]]})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.texte());
        assert_eq!(
            r.json(),
            json!({"error": "limit_reached", "message": "10000 pistes par playlist au plus"})
        );
        let v = e.get("alice", &format!("/playlists/{id}")).await.json();
        assert_eq!(v["track_count"], 10_000);
        sqlx::query("INSERT INTO playlists (sub, nom) SELECT 'sub-alice', 'P' || i FROM generate_series(1, 498) AS i")
            .execute(e.state.pool())
            .await
            .unwrap();
        creer(&e, "alice", "Dernière").await;
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/playlists",
                Some(json!({"name": "De trop"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(r.json()["message"], "500 playlists au plus");
        creer(&e, "bruno", "Pour Bruno").await;
        let trop: Vec<String> = (0..10_001)
            .map(|_| uuid::Uuid::new_v4().to_string())
            .collect();
        let r = e
            .appel(
                Some("alice"),
                "PUT",
                "/queue",
                Some(json!({"revision": 0, "tracks": trop})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(r.json()["error"], "limit_reached");
    }

    #[tokio::test]
    async fn piste_retiree_du_catalogue_reste_marquee_absente_dans_playlist_file_likes_et_historique()
     {
        let Some(b) = crate::tests_api::bibliotheque().await else {
            return;
        };
        b.synchro().await;
        let lemonade = b.album("Lemonade").await;
        let album = lemonade["id"].as_str().unwrap().to_string();
        let formation = crate::tests_api::piste(&lemonade, "Formation")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let e = &b.e;
        let id = creer(e, "alice", "Lemonade").await;
        e.appel(
            Some("alice"),
            "POST",
            &format!("/playlists/{id}/tracks"),
            Some(json!({"album": album})),
        )
        .await;
        let file = e
            .appel(
                Some("alice"),
                "PUT",
                "/queue",
                Some(json!({"revision": 0, "source": {"type": "album", "id": album}})),
            )
            .await
            .json();
        assert_eq!(
            file["items"][0]["track"]["id"],
            formation.as_str(),
            "{file}"
        );
        e.appel(
            Some("alice"),
            "PUT",
            &format!("/likes/tracks/{formation}"),
            None,
        )
        .await;
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/history",
                Some(json!({"track": formation, "played": 2.0, "ended_by": "end"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::CREATED);

        std::fs::remove_file(b.chemin("Beyonce/Lemonade/01 Formation.flac")).unwrap();
        {
            let mut m = b.monde.lock().unwrap();
            m.pistes.retain(|p| p["title"] != "Formation");
            m.fichiers
                .retain(|f| f["id"] != crate::tests_api::LIDARR + 200);
        }
        b.synchro().await;

        let v = e.get("alice", &format!("/playlists/{id}")).await.json();
        assert_eq!(v["items"][0]["track"]["id"], formation.as_str());
        assert_eq!(v["items"][0]["track"]["available"], false);
        assert_eq!(v["track_count"], 3);
        let f = e.get("alice", "/queue").await.json();
        assert_eq!(f["items"].as_array().unwrap().len(), 3);
        assert_eq!(
            (
                f["items"][0]["track"]["id"].clone(),
                f["items"][0]["track"]["available"].clone()
            ),
            (json!(formation), json!(false))
        );
        let r = e
            .appel(
                Some("alice"),
                "PATCH",
                "/queue",
                Some(json!({"revision": f["revision"], "index": 2})),
            )
            .await;
        let r = e
            .appel(
                Some("alice"),
                "PATCH",
                "/queue",
                Some(json!({"revision": r.json()["revision"], "step": 1, "repeat": "all"})),
            )
            .await;
        assert_eq!(
            r.json()["index"],
            1,
            "la piste absente est passée : {}",
            r.texte()
        );
        let l = e.get("alice", "/likes/tracks").await.json();
        assert_eq!(l["items"][0]["track"]["available"], false);
        let h = e.get("alice", "/history").await.json();
        assert_eq!(h["items"][0]["track"]["id"], formation.as_str());
        assert_eq!(h["items"][0]["track"]["available"], false);
        b.synchro().await;
        assert_eq!(
            e.get("alice", &format!("/playlists/{id}")).await.json()["track_count"],
            3
        );
    }
}
