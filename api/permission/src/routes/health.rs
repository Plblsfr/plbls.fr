use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}

/// Santé : la base de données est-elle joignable ?
#[utoipa::path(get, path = "/health", tag = "système", security(()), responses((status = 200), (status = 503)))]
async fn health(State(s): State<AppState>) -> (StatusCode, Json<Value>) {
    match sqlx::query("SELECT 1").execute(&s.db).await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ok"}))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "db indisponible"})),
        ),
    }
}
