//! Recherche de comptes par e-mail, relayée à la route interne du gateway (`/internal/users`).

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::Caller,
    error::{ApiError, ApiResult, ErrorBody},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(search))
}

#[derive(Deserialize, IntoParams)]
struct SearchQuery {
    /// Début d'e-mail, insensible à la casse.
    email: String,
    /// Décalage de pagination (le `next` de la page précédente).
    first: Option<u32>,
    /// Taille de page (plafonnée par le gateway).
    max: Option<u32>,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct FoundUser {
    /// `sub` Keycloak.
    id: String,
    email: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct Found {
    users: Vec<FoundUser>,
    first: u32,
    max: u32,
    /// `null` = dernière page. Une page courte ne signale pas la fin.
    next: Option<u32>,
}

/// Cherche des comptes par début d'e-mail (admin avec la permission `permission:users.read`).
#[utoipa::path(get, path = "/v1/users", tag = "admin", params(SearchQuery), responses(
    (status = 200, body = Found), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    (status = 403, body = ErrorBody), (status = 404, body = ErrorBody, description = "Recherche non configurée"),
    (status = 502, body = ErrorBody, description = "Gateway ou Keycloak en erreur")))]
async fn search(
    State(state): State<AppState>,
    caller: Caller,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Found>> {
    caller.require_admin(&state).await?;
    if !caller.has(&state, "permission", "users.read").await? {
        return Err(ApiError::Forbidden);
    }
    let g = &state.settings.gateway;
    if g.url.is_empty() || g.internal_token.is_empty() {
        return Err(ApiError::NotFound(
            "recherche de comptes (gateway.url / gateway.internal_token non configurés)",
        ));
    }
    if q.email.trim().is_empty() {
        return Err(ApiError::BadRequest("email vide".into()));
    }
    let res = state
        .http
        .get(format!("{}/internal/users", g.url.trim_end_matches('/')))
        .bearer_auth(&g.internal_token)
        .query(&[("email", q.email.trim())])
        .query(&[("first", q.first), ("max", q.max)])
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("gateway injoignable : {e}")))?;
    if !res.status().is_success() {
        return Err(ApiError::Upstream(format!(
            "le gateway a répondu {}",
            res.status()
        )));
    }
    res.json()
        .await
        .map(Json)
        .map_err(|e| ApiError::Upstream(format!("réponse du gateway illisible : {e}")))
}
