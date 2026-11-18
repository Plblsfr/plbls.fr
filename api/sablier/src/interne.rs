use axum::{extract::State, http::StatusCode, response::Response};
use serde_json::json;

use crate::{
    etat::AppState,
    garde::Personne,
    http::{erreur, ok},
};

pub async fn moi(State(state): State<AppState>, p: Personne) -> Response {
    ok(&json!({
        "sub": p.identite.sub,
        "name": p.identite.nom,
        "email": p.identite.email,
        "permissions": p.droits,
        "sections": p.droits.sections(),
        "heartbeat_seconds": state.config().battement.as_secs(),
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
