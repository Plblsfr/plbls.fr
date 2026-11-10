use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    bibliotheque::uuid,
    droits::Droit,
    etat::AppState,
    garde::Personne,
    http::{bd, erreur, introuvable, invalide, ok},
    identite::Identite,
};

pub const SUITE_MAX: i64 = 50;

fn titre_affiche(l: &PgRow) -> String {
    let titre: String = l.get("titre");
    match (
        l.get::<Option<String>, _>("serie_titre"),
        l.get::<Option<i32>, _>("saison"),
        l.get::<Option<i32>, _>("episode"),
    ) {
        (Some(s), Some(n), Some(e)) => format!("{s} · S{n}E{e} · {titre}"),
        _ => titre,
    }
}

fn resolu(l: &PgRow) -> Value {
    json!({
        "id": l.get::<Uuid, _>("id"),
        "title": titre_affiche(l),
        "duration": l.get::<Option<f64>, _>("duree"),
    })
}

const SELECTION: &str =
    "SELECT t.id, t.titre, t.saison, t.episode, t.serie_id, t.duree, s.titre AS serie_titre
    FROM titres t LEFT JOIN titres s ON s.id = t.serie_id";

pub async fn titre(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let sub = q
        .get("sub")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .ok_or_else(|| invalide("sub requis"))?;
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{SELECTION} WHERE t.id = $1 AND t.genre <> 'serie' AND t.present
         AND EXISTS (SELECT 1 FROM fichiers f WHERE f.titre_id = t.id)"
    )))
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let identite = Identite {
        sub: sub.to_string(),
        nom: String::new(),
        email: String::new(),
    };
    let allowed = state
        .droits_de(&identite)
        .await
        .map_err(|_| {
            erreur(
                StatusCode::SERVICE_UNAVAILABLE,
                "aegis_unavailable",
                "service de droits injoignable",
            )
        })?
        .a(Droit::Acces);
    let suite: Vec<Value> = match l.get::<Option<Uuid>, _>("serie_id") {
        None => Vec::new(),
        Some(serie) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SELECTION} WHERE t.serie_id = $1 AND t.present AND (t.saison, t.episode) > ($2, $3)
             AND EXISTS (SELECT 1 FROM fichiers f WHERE f.titre_id = t.id)
             ORDER BY t.saison, t.episode LIMIT {SUITE_MAX}"
        )))
        .bind(serie)
        .bind(l.get::<Option<i32>, _>("saison"))
        .bind(l.get::<Option<i32>, _>("episode"))
        .fetch_all(state.pool())
        .await
        .map_err(bd)?
        .iter()
        .map(resolu)
        .collect(),
    };
    let mut v = resolu(&l);
    v["allowed"] = json!(allowed);
    v["next"] = json!(suite);
    Ok(ok(&v))
}

pub async fn moi(State(state): State<AppState>, p: Personne) -> Response {
    ok(&json!({
        "sub": p.identite.sub,
        "name": p.identite.nom,
        "email": p.identite.email,
        "permissions": p.droits,
        "push": { "web": state.push().web_actif(), "fcm": state.push().fcm_actif() },
    }))
}

pub async fn sante() -> Response {
    ok(&json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn pret(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(state.pool())
        .await
    {
        Ok(_) => ok(&json!({ "status": "ready" })),
        Err(_) => erreur(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "base injoignable",
        ),
    }
}
