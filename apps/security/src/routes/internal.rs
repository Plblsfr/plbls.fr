//! Routes internes (jamais exposées par le reverse proxy public), protégées par un token statique
//! (`GATEWAY__INTERNAL__TOKEN`, 404 si vide) pour les micro-services du réseau Docker.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;

use super::mcp::bearer_matches;
use crate::AppState;

const DEFAULT_MAX: u32 = 20;

pub fn router() -> Router<AppState> {
    Router::new().route("/internal/users", get(users))
}

#[derive(Deserialize)]
struct Params {
    /// Début d'e-mail, non vide (pas d'export de tous les comptes).
    email: String,
    #[serde(default)]
    first: u32,
    max: Option<u32>,
}

/// `GET /internal/users?email=jean&first=0&max=20` → `{users: [{id, email}], first, max, next}`.
/// `next` = valeur de `first` de la page suivante, `null` en dernière page. Une page peut contenir moins de
/// `max` comptes (ceux dont seul le nom correspond sont écartés) : on s'arrête quand `next` est `null`.
async fn users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(p): Query<Params>,
) -> Response {
    let token = state.settings.internal.token.as_bytes();
    if token.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !bearer_matches(&headers, token) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    let prefix = p.email.trim().to_lowercase();
    if prefix.is_empty() {
        return (StatusCode::BAD_REQUEST, "email requis").into_response();
    }
    let max = p
        .max
        .unwrap_or(DEFAULT_MAX)
        .clamp(1, state.settings.internal.max_page_size.max(1));
    let found = match state.oidc.search_users(&prefix, p.first, max).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("recherche de comptes: {e:#}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let next = (found.len() as u32 == max).then(|| p.first + max);
    let users: Vec<_> = found
        .into_iter()
        .filter_map(|u| Some((u.id, u.email?)))
        .filter(|(_, email)| email.to_lowercase().starts_with(&prefix))
        .map(|(id, email)| json!({"id": id, "email": email}))
        .collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"users": users, "first": p.first, "max": max, "next": next})),
    )
        .into_response()
}
