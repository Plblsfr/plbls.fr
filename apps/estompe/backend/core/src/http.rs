use axum::{
    body::{Body, to_bytes},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};

pub const CORPS_MAX: usize = 1 << 16;

pub fn text_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
        ],
        format!("{message}\n"),
    )
        .into_response()
}

pub fn write_json(status: StatusCode, value: &Value) -> Response {
    (
        status,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        format!("{value}\n"),
    )
        .into_response()
}

pub fn write_err(status: StatusCode, code: &str, message: &str) -> Response {
    write_json(status, &json!({ "error": code, "message": message }))
}

pub fn internal() -> Response {
    write_err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "erreur interne",
    )
}

pub fn premier_objet(octets: &[u8]) -> Option<Map<String, Value>> {
    let premier = serde_json::Deserializer::from_slice(octets)
        .into_iter::<Value>()
        .next()?
        .ok()?;
    match premier {
        Value::Object(m) => Some(m),
        Value::Null => Some(Map::new()),
        _ => None,
    }
}

pub fn champ<'a>(objet: &'a Map<String, Value>, nom: &str) -> Option<&'a Value> {
    objet
        .iter()
        .rev()
        .find(|(k, _)| k.eq_ignore_ascii_case(nom))
        .map(|(_, v)| v)
}

pub fn champ_texte(objet: &Map<String, Value>, nom: &str) -> Option<String> {
    match champ(objet, nom) {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

pub async fn decode(body: Body) -> Result<Map<String, Value>, Response> {
    let invalide = || write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    let octets = to_bytes(body, CORPS_MAX).await.map_err(|_| invalide())?;
    premier_objet(&octets).ok_or_else(invalide)
}

pub async fn lire_corps(mut reponse: reqwest::Response) -> Option<Vec<u8>> {
    let mut corps = Vec::new();
    while let Some(morceau) = reponse.chunk().await.ok()? {
        corps.extend_from_slice(&morceau);
        if corps.len() > CORPS_MAX {
            return None;
        }
    }
    Some(corps)
}
