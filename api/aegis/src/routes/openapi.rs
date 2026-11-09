use axum::{Router, extract::State, http::header, routing::get};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/openapi.json",
        get(|State(s): State<AppState>| async move {
            (
                [(header::CONTENT_TYPE, "application/json")],
                s.spec.to_string(),
            )
        }),
    )
}
