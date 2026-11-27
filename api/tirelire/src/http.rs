use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

pub fn json<T: Serialize + ?Sized>(statut: StatusCode, corps: &T) -> Response {
    let mut r = (statut, serde_json::to_vec(corps).unwrap_or_default()).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

pub fn ok<T: Serialize + ?Sized>(corps: &T) -> Response {
    json(StatusCode::OK, corps)
}

pub fn erreur(statut: StatusCode, code: &str, message: &str) -> Response {
    json(statut, &json!({ "error": code, "message": message }))
}

pub fn introuvable() -> Response {
    erreur(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

pub fn interne() -> Response {
    erreur(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "erreur interne",
    )
}

pub fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub fn invalide(message: &str) -> Response {
    erreur(StatusCode::BAD_REQUEST, "invalid", message)
}

pub fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    interne()
}

pub fn corps<T: DeserializeOwned>(octets: &[u8], message: &str) -> Result<T, Response> {
    serde_json::from_slice(octets).map_err(|_| invalide(message))
}

pub fn cree<T: Serialize + ?Sized>(corps: &T) -> Response {
    json(StatusCode::CREATED, corps)
}

pub fn conflit(message: &str) -> Response {
    erreur(StatusCode::CONFLICT, "conflict", message)
}

pub fn identifiant(texte: &str) -> Result<i64, Response> {
    texte
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(introuvable)
}

pub fn facultatif<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}
