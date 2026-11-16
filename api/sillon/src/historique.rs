use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    bibliotheque::{COLONNES_ARTISTE, artiste_public, curseur, lire_curseur},
    etat::AppState,
    file::{self, SOURCES},
    garde::Personne,
    http::{bd, horodatage, introuvable, invalide, json, ok},
    playlists::{champ_uuid, objet, pistes_json},
};

pub const SEUIL_SECONDES: f64 = 30.0;
pub const DUREE_MAX: f64 = 86_400.0;

type Params = Query<HashMap<String, String>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fin {
    Terminee,
    Sautee,
    Interrompue,
}

impl Fin {
    pub fn nom(self) -> &'static str {
        match self {
            Fin::Terminee => "completed",
            Fin::Sautee => "skipped",
            Fin::Interrompue => "interrupted",
        }
    }
}

pub fn qualifier(ecoute: f64, duree: Option<f64>, sortie: &str) -> Option<(Fin, bool)> {
    let fin_atteinte = duree.is_some_and(|d| ecoute >= d - 1.0);
    let fin = match sortie {
        "end" => Fin::Terminee,
        _ if fin_atteinte => Fin::Terminee,
        "skip" => Fin::Sautee,
        "stop" => Fin::Interrompue,
        _ => return None,
    };
    let compte = ecoute >= SEUIL_SECONDES || duree.is_some_and(|d| d > 0.0 && ecoute >= d / 2.0);
    Some((fin, compte))
}

fn limite(q: &HashMap<String, String>, defaut: i64, max: i64) -> Result<i64, Response> {
    match q.get("limit").filter(|v| !v.is_empty()) {
        None => Ok(defaut),
        Some(v) => v
            .parse::<i64>()
            .ok()
            .filter(|n| (1..=max).contains(n))
            .ok_or_else(|| invalide("limite invalide")),
    }
}

pub async fn enregistrer(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let id = champ_uuid(&corps, "id")?.unwrap_or_else(Uuid::new_v4);
    let piste = champ_uuid(&corps, "track")?.ok_or_else(|| invalide("track requis"))?;
    let duree: Option<f64> = sqlx::query_scalar("SELECT duree FROM pistes WHERE id = $1")
        .bind(piste)
        .fetch_optional(state.pool())
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let plafond = duree.map_or(DUREE_MAX, |d| (d * 1.05 + 5.0).min(DUREE_MAX));
    let ecoute = corps
        .get("played")
        .and_then(Value::as_f64)
        .filter(|e| e.is_finite() && (0.0..=plafond).contains(e))
        .ok_or_else(|| invalide("played : durée écoutée invalide"))?;
    let (fin, compte) = qualifier(
        ecoute,
        duree,
        corps.get("ended_by").and_then(Value::as_str).unwrap_or(""),
    )
    .ok_or_else(|| invalide("ended_by : end, skip ou stop"))?;
    let maintenant = Utc::now();
    let debut = match corps.get("started_at").filter(|v| !v.is_null()) {
        None => maintenant - Duration::milliseconds((ecoute * 1000.0) as i64),
        Some(v) => v
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|d| d.with_timezone(&Utc))
            .filter(|d| {
                *d <= maintenant + Duration::minutes(1) && *d >= maintenant - Duration::days(30)
            })
            .ok_or_else(|| invalide("started_at invalide"))?,
    };
    let (source_type, source_id) = match corps.get("source").filter(|v| !v.is_null()) {
        None => (None, None),
        Some(Value::Object(s)) => (
            Some(
                s.get("type")
                    .and_then(Value::as_str)
                    .filter(|g| SOURCES.contains(g))
                    .ok_or_else(|| invalide("source : type inconnu"))?
                    .to_string(),
            ),
            champ_uuid(s, "id")?,
        ),
        Some(_) => return Err(invalide("source : objet attendu")),
    };
    let suggeree = match champ_uuid(&corps, "item")? {
        Some(e) => file::element(state.pool(), p.sub(), e)
            .await
            .map_err(bd)?
            .unwrap_or(false),
        None => false,
    };
    sqlx::query(
        "INSERT INTO ecoutes (id, sub, piste_id, debut, duree_ecoutee, duree_piste, fin, compte, source_type, source_id, suggeree)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(p.sub())
    .bind(piste)
    .bind(debut)
    .bind(ecoute)
    .bind(duree)
    .bind(fin.nom())
    .bind(compte)
    .bind(source_type)
    .bind(source_id)
    .bind(suggeree)
    .execute(state.pool())
    .await
    .map_err(bd)?;
    Ok(json(
        StatusCode::CREATED,
        &json!({"id": id, "ended": fin.nom(), "counted": compte, "suggested": suggeree}),
    ))
}

pub async fn recentes(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Params,
) -> Result<Response, Response> {
    let n = limite(&q, 50, 200)?;
    let apres = match lire_curseur(&q)? {
        None => None,
        Some((t, id)) => Some((
            DateTime::parse_from_rfc3339(&t)
                .map_err(|_| invalide("curseur invalide"))?
                .with_timezone(&Utc),
            id,
        )),
    };
    let lignes = sqlx::query(
        "SELECT id, piste_id, debut, duree_ecoutee, fin, compte, source_type, source_id, suggeree FROM ecoutes
         WHERE sub = $1 AND ($2::timestamptz IS NULL OR (debut, id) < ($2, $3))
         ORDER BY debut DESC, id DESC LIMIT $4",
    )
    .bind(p.sub())
    .bind(apres.map(|a| a.0))
    .bind(apres.map(|a| a.1).unwrap_or_default())
    .bind(n + 1)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let n = usize::try_from(n).unwrap_or(0);
    let suite = (lignes.len() > n).then(|| {
        let l = &lignes[n - 1];
        curseur(
            &l.get::<DateTime<Utc>, _>("debut")
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            l.get("id"),
        )
    });
    let lignes = &lignes[..lignes.len().min(n)];
    let ids: Vec<Uuid> = lignes.iter().map(|l| l.get("piste_id")).collect();
    let pistes = pistes_json(state.pool(), &ids).await.map_err(bd)?;
    let items: Vec<Value> = lignes
        .iter()
        .zip(pistes)
        .map(|(l, piste)| {
            json!({
                "id": l.get::<Uuid, _>("id"),
                "track": piste,
                "started_at": horodatage(&l.get::<DateTime<Utc>, _>("debut")),
                "played": l.get::<f64, _>("duree_ecoutee"),
                "ended": l.get::<String, _>("fin"),
                "counted": l.get::<bool, _>("compte"),
                "source": l.get::<Option<String>, _>("source_type").map(|t| json!({
                    "type": t, "id": l.get::<Option<Uuid>, _>("source_id"),
                })),
                "suggested": l.get::<bool, _>("suggeree"),
            })
        })
        .collect();
    Ok(ok(&json!({ "items": items, "next_cursor": suite })))
}

fn periode(q: &HashMap<String, String>) -> Result<Option<DateTime<Utc>>, Response> {
    let jours = match q.get("period").map(String::as_str) {
        None | Some("") | Some("month") => 30,
        Some("week") => 7,
        Some("year") => 365,
        Some("all") => return Ok(None),
        Some(_) => return Err(invalide("period : week, month, year ou all")),
    };
    Ok(Some(Utc::now() - Duration::days(jours)))
}

pub async fn top_pistes(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Params,
) -> Result<Response, Response> {
    let depuis = periode(&q)?;
    let n = limite(&q, 20, 100)?;
    let lignes = sqlx::query(
        "SELECT piste_id, count(*) FROM ecoutes WHERE sub = $1 AND compte AND ($2::timestamptz IS NULL OR debut >= $2)
         GROUP BY piste_id ORDER BY count(*) DESC, max(debut) DESC, piste_id LIMIT $3",
    )
    .bind(p.sub())
    .bind(depuis)
    .bind(n)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let ids: Vec<Uuid> = lignes.iter().map(|l| l.get(0)).collect();
    let pistes = pistes_json(state.pool(), &ids).await.map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().zip(pistes).map(|(l, piste)| json!({
            "track": piste, "plays": l.get::<i64, _>(1),
        })).collect::<Vec<_>>(),
    })))
}

pub async fn top_artistes(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Params,
) -> Result<Response, Response> {
    let depuis = periode(&q)?;
    let n = limite(&q, 20, 100)?;
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "WITH t AS (
            SELECT pi.artiste_id, count(*) AS ecoutes, max(e.debut) AS derniere FROM ecoutes e JOIN pistes pi ON pi.id = e.piste_id
            WHERE e.sub = $1 AND e.compte AND ($2::timestamptz IS NULL OR e.debut >= $2) GROUP BY pi.artiste_id
         )
         SELECT {COLONNES_ARTISTE}, t.ecoutes FROM t JOIN artistes ar ON ar.id = t.artiste_id
         ORDER BY t.ecoutes DESC, t.derniere DESC, ar.id LIMIT $3"
    )))
    .bind(p.sub())
    .bind(depuis)
    .bind(n)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| json!({
            "artist": artiste_public(l), "plays": l.get::<i64, _>("ecoutes"),
        })).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saut_ecoute_et_interruption() {
        assert_eq!(
            qualifier(10.0, Some(200.0), "skip"),
            Some((Fin::Sautee, false))
        );
        assert_eq!(
            qualifier(200.0, Some(200.0), "end"),
            Some((Fin::Terminee, true))
        );
        assert_eq!(
            qualifier(199.5, Some(200.0), "skip"),
            Some((Fin::Terminee, true))
        );
        assert_eq!(
            qualifier(45.0, Some(200.0), "skip"),
            Some((Fin::Sautee, true))
        );
        assert_eq!(
            qualifier(25.0, Some(40.0), "stop"),
            Some((Fin::Interrompue, true))
        );
        assert_eq!(
            qualifier(10.0, None, "stop"),
            Some((Fin::Interrompue, false))
        );
        assert_eq!(qualifier(10.0, None, "pause"), None);
    }
}

#[cfg(test)]
mod tests_api {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{playlists::tests::catalogue, testutil::env};

    #[tokio::test]
    async fn lecture_de_10_s_en_saut_lecture_complete_en_ecoute_toutes_deux_dans_l_historique() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 3).await;
        let f = e
            .appel(Some("alice"), "PUT", "/queue", Some(json!({"revision": 0, "tracks": [p[0], p[1]], "source": {"type": "search", "query": "piste"}})))
            .await
            .json();
        let element = f["items"][0]["id"].clone();
        let il_y_a =
            |s: i64| crate::http::horodatage(&(chrono::Utc::now() - chrono::Duration::seconds(s)));
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/history",
                Some(json!({"track": p[0], "played": 10.0, "ended_by": "skip", "item": element, "source": {"type": "search"}, "started_at": il_y_a(600)})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(
            (
                r.json()["ended"].clone(),
                r.json()["counted"].clone(),
                r.json()["suggested"].clone()
            ),
            (json!("skipped"), json!(false), json!(false))
        );
        let id = uuid::Uuid::new_v4();
        for _ in 0..2 {
            let r = e
                .appel(Some("alice"), "POST", "/history", Some(json!({"id": id, "track": p[1], "played": 200.0, "ended_by": "end", "started_at": il_y_a(400)})))
                .await;
            assert_eq!(
                (r.json()["ended"].clone(), r.json()["counted"].clone()),
                (json!("completed"), json!(true))
            );
        }
        e.appel(Some("alice"), "POST", "/history", Some(json!({"track": p[1], "played": 120.0, "ended_by": "stop", "started_at": il_y_a(150)}))).await;
        let h = e.get("alice", "/history").await.json();
        let vues: Vec<(String, String, bool)> = h["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    i["track"]["id"].as_str().unwrap().to_string(),
                    i["ended"].as_str().unwrap().to_string(),
                    i["counted"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            vues,
            [
                (p[1].clone(), "interrupted".to_string(), true),
                (p[1].clone(), "completed".into(), true),
                (p[0].clone(), "skipped".into(), false),
            ]
        );
        assert_eq!(
            h["items"][2]["source"],
            json!({"type": "search", "id": null})
        );
        assert_eq!(h["items"][2]["played"], 10.0);
        let page = e.get("alice", "/history?limit=2").await.json();
        let suite = page["next_cursor"].as_str().unwrap();
        let fin = e
            .get("alice", &format!("/history?limit=2&cursor={suite}"))
            .await
            .json();
        assert_eq!(fin["items"][0]["ended"], "skipped");
        assert!(fin["next_cursor"].is_null());
        let top = e
            .get("alice", "/history/top/tracks?period=week")
            .await
            .json();
        assert_eq!(
            top,
            json!({"items": [{"track": h["items"][0]["track"], "plays": 2}]})
        );
        let top = e
            .get("alice", "/history/top/artists?period=all")
            .await
            .json();
        assert_eq!(
            (
                top["items"][0]["artist"]["name"].clone(),
                top["items"][0]["plays"].clone()
            ),
            (json!("Artiste"), json!(2))
        );
        assert!(
            e.get("bruno", "/history").await.json()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for corps in [
            json!({"track": p[0], "played": 400.0, "ended_by": "end"}),
            json!({"track": p[0], "played": -1, "ended_by": "end"}),
            json!({"track": p[0], "played": 10, "ended_by": "pause"}),
            json!({"track": p[0], "played": 10, "ended_by": "skip", "started_at": "2001-01-01T00:00:00Z"}),
            json!({"track": p[0], "played": 10, "ended_by": "skip", "source": {"type": "radio"}}),
        ] {
            let r = e
                .appel(Some("alice"), "POST", "/history", Some(corps.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
        }
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/history",
                Some(json!({"track": uuid::Uuid::new_v4(), "played": 1, "ended_by": "stop"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NOT_FOUND);
        assert_eq!(
            e.get("alice", "/history/top/tracks?period=decade")
                .await
                .statut,
            StatusCode::BAD_REQUEST
        );
    }
}
