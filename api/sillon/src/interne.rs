use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    droits::Droit,
    etat::AppState,
    garde::{Personne, interdit},
    http::{bd, erreur, introuvable, invalide, ok},
    identite::Identite,
};

const APERCU_ALBUM: &str =
    "SELECT al.titre, ar.nom, al.id AS album, al.pochette IS NOT NULL AS image
    FROM albums al JOIN artistes ar ON ar.id = al.artiste_id WHERE al.id = $1";
const APERCU_PISTE: &str = "SELECT p.titre, ar.nom, al.id AS album, al.pochette IS NOT NULL AS image
    FROM pistes p JOIN albums al ON al.id = p.album_id JOIN artistes ar ON ar.id = p.artiste_id WHERE p.id = $1";
const APERCU_ARTISTE: &str = "SELECT ar.nom AS titre, ar.nom, ar.image IS NOT NULL AS image FROM artistes ar WHERE ar.id = $1";

pub async fn apercu(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let (genre, sql) = match q.get("type").map(String::as_str) {
        Some("album") => ("album", APERCU_ALBUM),
        Some("track") => ("track", APERCU_PISTE),
        Some("artist") => ("artist", APERCU_ARTISTE),
        Some("playlist") => return Err(introuvable()),
        _ => return Err(invalide("type inconnu")),
    };
    let sub = q
        .get("sub")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .ok_or_else(|| invalide("sub requis"))?;
    let id = q
        .get("id")
        .and_then(|i| Uuid::parse_str(i).ok())
        .ok_or_else(introuvable)?;
    let identite = Identite {
        sub: sub.to_string(),
        nom: String::new(),
        email: String::new(),
    };
    let droits = state.droits_de(&identite).await.map_err(|_| {
        erreur(
            StatusCode::SERVICE_UNAVAILABLE,
            "aegis_unavailable",
            "service de droits injoignable",
        )
    })?;
    if !droits.a(Droit::Acces) {
        return Err(interdit(Droit::Acces));
    }
    let l = sqlx::query(sql)
        .bind(id)
        .fetch_optional(state.pool())
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let album = (genre != "artist").then(|| l.get::<Uuid, _>("album"));
    let cover = match album {
        _ if !l.get::<bool, _>("image") => Value::Null,
        Some(a) => json!({ "kind": "album", "id": a }),
        None => json!({ "kind": "artist", "id": id }),
    };
    Ok(ok(&json!({
        "type": genre,
        "id": id,
        "title": l.get::<String, _>("titre"),
        "artist": l.get::<String, _>("nom"),
        "album": album,
        "cover": cover,
    })))
}

pub async fn moi(p: Personne) -> Response {
    ok(&json!({
        "sub": p.identite.sub,
        "name": p.identite.nom,
        "email": p.identite.email,
        "permissions": p.droits,
        "sections": p.droits.sections(),
    }))
}

pub async fn sante() -> Response {
    ok(&json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn pret(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(state.pool())
        .await
    {
        Ok(_) => ok(&json!({ "status": "ready" })),
        Err(_) => erreur(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "base injoignable",
        ),
    }
}
