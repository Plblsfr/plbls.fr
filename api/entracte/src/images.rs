use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::Response,
};
use sqlx::Row;

use crate::{etat::AppState, http, tmdb::Tmdb};

pub const LARGEURS: [u32; 3] = [342, 500, 1280];

fn cle(chemin: &str, largeur: u32) -> String {
    format!("{largeur}{chemin}")
}

pub async fn lire(
    state: &AppState,
    chemin: &str,
    largeur: u32,
) -> Result<Option<(String, Vec<u8>)>, Response> {
    let c = cle(chemin, largeur);
    if let Some(l) = sqlx::query("SELECT mime, octets FROM images WHERE cle = $1")
        .bind(&c)
        .fetch_optional(state.pool())
        .await
        .map_err(http::bd)?
    {
        return Ok(Some((l.get(0), l.get(1))));
    }
    match Tmdb::new(state.config(), state.http())
        .image(chemin, largeur)
        .await
    {
        Ok((mime, octets)) => {
            sqlx::query(
                "INSERT INTO images (cle, mime, octets) VALUES ($1, $2, $3) ON CONFLICT (cle) DO NOTHING",
            )
            .bind(&c)
            .bind(&mime)
            .bind(&octets)
            .execute(state.pool())
            .await
            .map_err(http::bd)?;
            Ok(Some((mime, octets)))
        }
        Err(crate::tmdb::Erreur::Introuvable) => Ok(None),
        Err(e) => {
            tracing::warn!("image : {e}");
            Err(http::indisponible(
                "tmdb_unavailable",
                "images injoignables",
            ))
        }
    }
}

pub async fn precharger(state: &AppState, chemin: &str) {
    if let Err(r) = lire(state, chemin, LARGEURS[0]).await {
        tracing::debug!("préchargement d'affiche : HTTP {}", r.status());
    }
}

pub fn reponse(mime: String, octets: Vec<u8>) -> Response {
    let mut r = Response::new(Body::from(octets));
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("image/jpeg")),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}
