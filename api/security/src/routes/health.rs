use axum::{Router, extract::State, http::StatusCode, routing::get};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn readyz(State(state): State<AppState>) -> StatusCode {
    match state.store.ping().await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!("redis injoignable: {e}");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
