use axum::{
    body::{Body, to_bytes},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Map, Value, json};

pub const CORPS_MAX: usize = 64 * 1024;

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

pub fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub fn introuvable() -> Response {
    erreur(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

pub fn invalide(message: &str) -> Response {
    erreur(StatusCode::BAD_REQUEST, "invalid", message)
}

pub fn interne() -> Response {
    erreur(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "erreur interne",
    )
}

pub fn indisponible(code: &str, message: &str) -> Response {
    erreur(StatusCode::BAD_GATEWAY, code, message)
}

pub fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    interne()
}

pub async fn objet(corps: Body) -> Result<Map<String, Value>, Response> {
    let octets = to_bytes(corps, CORPS_MAX)
        .await
        .map_err(|_| invalide("corps trop gros ou illisible"))?;
    if octets.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(&octets) {
        Ok(Value::Object(o)) => Ok(o),
        _ => Err(invalide("objet JSON attendu")),
    }
}

pub fn horodatage(d: &chrono::DateTime<chrono::Utc>) -> String {
    d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn jetons_egaux(attendu: &str, donne: &str) -> bool {
    use subtle::ConstantTimeEq;
    !attendu.is_empty()
        && attendu.len() == donne.len()
        && bool::from(attendu.as_bytes().ct_eq(donne.as_bytes()))
}
