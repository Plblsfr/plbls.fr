use axum::{body::Bytes, extract::State, response::Response};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    etat::AppState,
    garde::Personne,
    http::{bd, invalide, ok},
    transcodage::Qualite,
};

pub const QUALITE_PAR_DEFAUT: Qualite = Qualite::Originale;

pub async fn qualite_de(pool: &PgPool, sub: &str) -> Result<Qualite, sqlx::Error> {
    let q: Option<String> = sqlx::query_scalar("SELECT qualite FROM preferences WHERE sub = $1")
        .bind(sub)
        .fetch_optional(pool)
        .await?;
    Ok(q.as_deref()
        .and_then(Qualite::depuis)
        .unwrap_or(QUALITE_PAR_DEFAUT))
}

fn reponse(q: Qualite) -> Response {
    ok(&json!({
        "quality": q.nom(),
        "qualities": Qualite::TOUTES.map(Qualite::nom),
    }))
}

pub async fn lire(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    Ok(reponse(
        qualite_de(state.pool(), p.sub()).await.map_err(bd)?,
    ))
}

pub async fn ecrire(
    State(state): State<AppState>,
    p: Personne,
    corps: Bytes,
) -> Result<Response, Response> {
    let v: Value = serde_json::from_slice(&corps).map_err(|_| invalide("JSON attendu"))?;
    let q = v["quality"]
        .as_str()
        .and_then(Qualite::depuis)
        .ok_or_else(|| invalide("qualité invalide : originale, haute, normale ou basse"))?;
    sqlx::query(
        "INSERT INTO preferences (sub, qualite) VALUES ($1, $2)
         ON CONFLICT (sub) DO UPDATE SET qualite = EXCLUDED.qualite, modifie_le = now()",
    )
    .bind(p.sub())
    .bind(q.nom())
    .execute(state.pool())
    .await
    .map_err(bd)?;
    Ok(reponse(q))
}
