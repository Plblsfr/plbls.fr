use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    Forbidden,
    NotFound(&'static str),
    Conflict(String),
    Internal(String),
    Upstream(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match e.as_database_error().and_then(|d| d.code()).as_deref() {
            Some("23505") => ApiError::Conflict("existe déjà".into()),
            Some("23503") => ApiError::BadRequest("référence inconnue".into()),
            Some("23514") => ApiError::BadRequest("valeur invalide".into()),
            Some("22P02") => ApiError::BadRequest("identifiant invalide".into()),
            _ => ApiError::Internal(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "identité absente ou invalide".into(),
            ),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "accès refusé".into()),
            ApiError::NotFound(what) => (StatusCode::NOT_FOUND, format!("{what} introuvable")),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::Upstream(m) => (StatusCode::BAD_GATEWAY, m),
            ApiError::Internal(m) => {
                tracing::error!("erreur interne : {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, "erreur interne".into())
            }
        };
        (status, Json(json!({"error": msg}))).into_response()
    }
}
