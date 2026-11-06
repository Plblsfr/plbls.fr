//! Lecture des permissions : ce que consomment les micro-services et le frontend.

use axum::{
    Json,
    extract::{Path, Query, State},
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
    OpenApiRouter::new()
        .routes(routes!(me))
        .routes(routes!(user_permissions))
        .routes(routes!(check))
}

#[derive(Deserialize, IntoParams)]
struct AppFilter {
    /// Limiter à une application (clé).
    app: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct Permissions {
    /// `sub` Keycloak.
    user_id: String,
    /// Noms complets `application:permission`, triés.
    #[schema(example = json!(["chat:message.delete"]))]
    permissions: Vec<String>,
}

async fn load(state: &AppState, user_id: &str, app: Option<&str>) -> ApiResult<Permissions> {
    let permissions = sqlx::query_scalar(
        "SELECT DISTINCT a.key || ':' || p.key
         FROM assignments s
         JOIN role_permissions rp ON rp.role_id = s.role_id
         JOIN permissions p ON p.id = rp.permission_id
         JOIN apps a ON a.id = p.app_id
         WHERE s.user_id = $1 AND ($2::text IS NULL OR a.key = $2)
         ORDER BY 1",
    )
    .bind(user_id)
    .bind(app)
    .fetch_all(&state.db)
    .await?;
    Ok(Permissions {
        user_id: user_id.to_string(),
        permissions,
    })
}

/// Permissions de l'utilisateur connecté.
#[utoipa::path(get, path = "/v1/me/permissions", tag = "lecture", params(AppFilter), responses(
    (status = 200, body = Permissions), (status = 400, body = ErrorBody, description = "Appelant de type service"),
    (status = 401, body = ErrorBody)))]
async fn me(
    State(state): State<AppState>,
    caller: Caller,
    Query(f): Query<AppFilter>,
) -> ApiResult<Json<Permissions>> {
    let Caller::User { id, .. } = caller else {
        return Err(ApiError::BadRequest(
            "route réservée aux utilisateurs, voir /v1/users/{user_id}/permissions".into(),
        ));
    };
    Ok(Json(load(&state, &id, f.app.as_deref()).await?))
}

/// Permissions d'un compte (soi-même, un service ou un admin).
#[utoipa::path(get, path = "/v1/users/{user_id}/permissions", tag = "lecture", params(AppFilter), responses(
    (status = 200, body = Permissions), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn user_permissions(
    State(state): State<AppState>,
    caller: Caller,
    Path(user_id): Path<String>,
    Query(f): Query<AppFilter>,
) -> ApiResult<Json<Permissions>> {
    if !caller.can_read(&user_id, &state).await? {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(load(&state, &user_id, f.app.as_deref()).await?))
}

#[derive(Deserialize, ToSchema)]
struct CheckRequest {
    /// Par défaut : l'appelant (utilisateur).
    user_id: Option<String>,
    /// `application:permission`, ex. `chat:message.delete`
    #[schema(example = "chat:message.delete")]
    permission: String,
}

#[derive(Serialize, ToSchema)]
struct CheckResponse {
    allowed: bool,
}

/// Un compte a-t-il cette permission ?
///
/// Sans `user_id`, vérifie l'utilisateur appelant ; un service doit fournir `user_id`.
#[utoipa::path(post, path = "/v1/check", tag = "lecture", request_body = CheckRequest, responses(
    (status = 200, body = CheckResponse), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    (status = 403, body = ErrorBody)))]
async fn check(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<CheckRequest>,
) -> ApiResult<Json<CheckResponse>> {
    let user_id = match (&req.user_id, &caller) {
        (Some(u), _) => u.clone(),
        (None, Caller::User { id, .. }) => id.clone(),
        (None, Caller::Service(_)) => {
            return Err(ApiError::BadRequest(
                "user_id requis pour un service".into(),
            ));
        }
    };
    if !caller.can_read(&user_id, &state).await? {
        return Err(ApiError::Forbidden);
    }
    let (app, perm) = req.permission.split_once(':').ok_or_else(|| {
        ApiError::BadRequest("permission attendue sous la forme application:permission".into())
    })?;
    let allowed = sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM assignments s
           JOIN role_permissions rp ON rp.role_id = s.role_id
           JOIN permissions p ON p.id = rp.permission_id
           JOIN apps a ON a.id = p.app_id
           WHERE s.user_id = $1 AND a.key = $2 AND p.key = $3)",
    )
    .bind(&user_id)
    .bind(app)
    .bind(perm)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(CheckResponse { allowed }))
}
