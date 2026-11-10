use std::{
    io::SeekFrom,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Multipart, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use hmac::{Hmac, KeyInit, Mac};
use http_body_util::Limited;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    api::ActiveUser,
    blob::{Disque, ErreurBlob, nouvelle_cle},
    config::Config,
    db,
    http::{internal, write_err, write_json},
    mp4, salons,
    servers::uuid_valide,
    sniff,
    state::AppState,
    users::User,
};

mod acces_fichier;
pub use acces_fichier::*;
mod envoi;
pub use envoi::*;
mod outils;
pub use outils::*;
mod purge;
pub use purge::*;
mod telechargement;
pub use telechargement::*;

pub const TAILLE_MAX: i64 = 10 << 20;
pub const TAILLE_VOCAL_MAX: usize = 5 << 20;
pub const MINIATURE_MAX: usize = 64 << 10;
pub const ENVOIS_VIDEO_MAX: i64 = 4;
pub const FICHIERS_EN_ATTENTE_MAX: i64 = 20;
pub const LIMITE_HEURE: i64 = 200 << 20;
pub const LIMITE_JOUR: i64 = 1 << 30;
pub const AGE_ORPHELINS: Duration = Duration::from_secs(3600);
pub const INTERVALLE_PURGE: Duration = Duration::from_secs(10 * 60);
const DUREE_VOCAL_MAX_MS: i64 = 5 * 60 * 1000;
const ONDE_MAX: usize = 100;
const DUREE_LIEN: u64 = 3600;
const MARGE_CORPS: usize = 64 << 10;
const AFFICHABLES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "video/mp4",
    "video/webm",
    "audio/mp4",
    "application/ogg",
];
const VOCAUX: &[&str] = &["video/webm", "application/ogg", "video/mp4", "audio/mp4"];
const EXTENSIONS_IMAGE: &[(&str, &str)] = &[
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(envoyer))
        .layer(DefaultBodyLimit::disable())
        .routes(routes!(telecharger))
        .routes(routes!(miniature))
        .routes(routes!(lien))
}

#[derive(Serialize, ToSchema)]
#[allow(dead_code)]
struct LienReponse {
    token: String,
}

pub struct Fichier {
    id: String,
    name: String,
    mime: String,
    key: String,
    expire: bool,
}

enum ErreurAcces {
    NonTrouve,
    Interne,
}

impl From<sqlx::Error> for ErreurAcces {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => ErreurAcces::NonTrouve,
            _ => ErreurAcces::Interne,
        }
    }
}

impl IntoResponse for ErreurAcces {
    fn into_response(self) -> Response {
        match self {
            ErreurAcces::NonTrouve => non_trouve(),
            ErreurAcces::Interne => internal(),
        }
    }
}

#[cfg(test)]
mod tests;
