use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}

#[utoipa::path(get, path = "/health", tag = "système", security(()),
    summary = "Santé : base joignable, état du cache Redis",
    responses((status = 200), (status = 503)))]
async fn health(State(s): State<AppState>) -> (StatusCode, Json<Value>) {
    let redis = match s.cache.ping().await {
        None => "désactivé",
        Some(true) => "ok",
        Some(false) => "injoignable",
    };
    match sqlx::query("SELECT 1").execute(&s.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({"status": "ok", "redis": redis})),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "db indisponible", "redis": redis})),
        ),
    }
}
