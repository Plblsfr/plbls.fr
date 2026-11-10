use std::collections::{HashMap, HashSet};

use axum::{
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::{get, post, put},
};
use regex::{Regex, RegexSet};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use std::sync::OnceLock;
use std::time::Duration;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    audit, emoji,
    gestion_roles::premier_parametre,
    http::{decode, write_err, write_json},
    openapi,
    roles::{self, Perm},
    servers::{
        Params, bd, horodatage, mauvais, non_trouve, param, pool, rfc3339, sans_contenu,
        uuid_valide,
    },
    sondages,
    state::AppState,
    users::User,
};

mod dms;
use dms::*;
mod edition;
pub(crate) use edition::*;
mod envoi;
pub(crate) use envoi::*;
mod epingles;
use epingles::*;
mod lecture;
pub use lecture::*;
mod mentionner;
pub use mentionner::*;
mod reactions;
use reactions::*;
mod saisie;
use saisie::*;

pub const SYSTEME: &str = "00000000-0000-0000-0000-000000000001";
const LONGUEUR_MAX: usize = 4000;
const PIECES_JOINTES_MAX: usize = 10;
const REACTIONS_MAX: i64 = 20;
const ENVOIS: usize = 20;
const FENETRE_ENVOI: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Debug, Default)]
pub struct Conv {
    pub channel_id: Option<String>,
    pub server_id: Option<String>,
    pub dm_id: Option<String>,
    pub kind: String,
    pub slowmode: i32,
    pub user_limit: i32,
    pub perms: Perm,
}

impl Conv {
    pub fn peut(&self, p: Perm) -> bool {
        self.channel_id.is_none() || roles::contient(self.perms, p)
    }
}

#[derive(Clone, Debug, Default)]
pub struct EvenementMessage {
    pub conv: Conv,
    pub message: Value,
    pub mentionnes: Vec<String>,
    pub groupe: Vec<String>,
    pub a: Vec<String>,
    pub vocal: String,
    pub rappel: String,
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route(
            "/channels/{cid}/messages",
            get(lister_salon).post(envoyer_salon),
        )
        .route("/dms", get(lister_dms).post(ouvrir_dm))
        .route("/dms/{id}/messages", get(lister_dm).post(envoyer_dm))
        .route(
            "/messages/{id}",
            axum::routing::patch(modifier).delete(supprimer),
        )
        .route(
            "/messages/{id}/reactions/{emoji}",
            put(reagir).delete(retirer_reaction),
        )
        .route("/messages/{id}/pin", put(epingler).delete(desepingler))
        .route("/channels/{cid}/pins", get(epingles_salon))
        .route("/dms/{id}/pins", get(epingles_dm))
        .route("/channels/{cid}/typing", post(saisie_salon))
        .route("/dms/{id}/typing", post(saisie_dm));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("messages", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Get,
        "/channels/{cid}/messages",
        "Historique d'un salon texte, du plus ancien au plus récent, par pages",
        &[
            ("200", "ok — {messages, has_more}"),
            ("400", "`not_text_channel` ou paramètre invalide"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/messages",
        "Envoyer un message",
        &[
            ("201", "Créé"),
            ("400", "`invalid_message` ou `invalid_files`"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "`too_many_messages` : 20 messages / 10 s"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms",
        "Conversations privées, la plus récemment active d'abord",
        &[("200", "ok")],
    ),
    (
        HttpMethod::Post,
        "/dms",
        "Ouvrir (ou retrouver) la conversation privée avec un utilisateur",
        &[("200", "ok"), ("400", "self_dm"), ("404", "user_not_found")],
    ),
    (
        HttpMethod::Get,
        "/dms/{id}/messages",
        "Historique d'une conversation privée",
        &[
            ("200", "ok — {messages, has_more}"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/dms/{id}/messages",
        "Envoyer un message privé",
        &[
            ("201", "Créé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "too_many_messages"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/messages/{id}",
        "Modifier son message",
        &[
            ("200", "ok"),
            ("400", "Requête invalide"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}",
        "Supprimer son message",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Put,
        "/messages/{id}/reactions/{emoji}",
        "Ajouter sa réaction (idempotent)",
        &[
            ("204", "ok"),
            ("400", "invalid_emoji"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            (
                "409",
                "too_many_reactions : 20 emojis différents par message",
            ),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}/reactions/{emoji}",
        "Retirer sa réaction",
        &[
            ("204", "ok"),
            ("400", "invalid_emoji"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Put,
        "/messages/{id}/pin",
        "Épingler un message (l'auteur, ou « Gérer les messages »)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}/pin",
        "Désépingler un message (l'auteur, ou « Gérer les messages »)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/channels/{cid}/pins",
        "Messages épinglés d'un salon, le plus récemment épinglé d'abord (100 max)",
        &[
            ("200", "ok — {messages}"),
            ("400", "`not_text_channel`"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms/{id}/pins",
        "Messages épinglés d'une conversation privée (100 max)",
        &[
            ("200", "ok — {messages}"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/typing",
        "Signaler que l'on écrit dans un salon",
        &[
            ("204", "ok"),
            ("400", "`not_text_channel`"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/dms/{id}/typing",
        "Signaler que l'on écrit dans une conversation privée",
        &[
            ("204", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
];

fn introuvable() -> Response {
    non_trouve("introuvable")
}

async fn avec_droits(state: &AppState, conv: &mut Conv, user: &str) -> Result<(), Response> {
    let (Some(salon), Some(serveur)) = (conv.channel_id.clone(), conv.server_id.clone()) else {
        return Ok(());
    };
    let acces = state
        .acces_serveur(&serveur, user)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    conv.perms = acces.channel(&salon);
    if roles::contient(conv.perms, roles::VIEW_CHANNEL) {
        Ok(())
    } else {
        Err(introuvable())
    }
}

pub async fn conv_du_salon(state: &AppState, cid: &str, user: &str) -> Result<Conv, Response> {
    let conv = conv_de_tout_salon(state, cid, user).await?;
    if !["text", "videos", "music"].contains(&conv.kind.as_str()) {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "not_text_channel",
            "ce salon n'est pas un salon texte",
        ));
    }
    Ok(conv)
}

pub async fn conv_de_tout_salon(state: &AppState, cid: &str, user: &str) -> Result<Conv, Response> {
    let pool = state
        .pool()
        .map_err(axum::response::IntoResponse::into_response)?;
    if !uuid_valide(cid) {
        return Err(introuvable());
    }
    let (serveur, kind, slowmode, user_limit) = sqlx::query_as::<_, (String, String, i32, i32)>(
        "SELECT c.server_id::text, c.kind, c.slowmode, c.user_limit FROM channels c
         JOIN server_members m ON m.server_id = c.server_id WHERE c.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(cid)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let mut conv = Conv {
        channel_id: Some(cid.to_string()),
        server_id: Some(serveur),
        dm_id: None,
        kind,
        slowmode,
        user_limit,
        perms: 0,
    };
    avec_droits(state, &mut conv, user).await?;
    Ok(conv)
}

pub async fn conv_du_dm(pool: &PgPool, id: &str, user: &str) -> Result<Conv, Response> {
    if !uuid_valide(id) {
        return Err(introuvable());
    }
    let id: String = sqlx::query_scalar(
        "SELECT id::text FROM dm_conversations WHERE id=$1::uuid AND (user_a=$2::uuid OR user_b=$2::uuid)",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    Ok(Conv {
        channel_id: None,
        server_id: None,
        dm_id: Some(id),
        kind: String::new(),
        slowmode: 0,
        user_limit: 0,
        perms: 0,
    })
}

pub fn id_message(texte: &str) -> Result<i64, Response> {
    texte.parse::<i64>().map_err(|_| introuvable())
}

pub async fn conv_du_message(
    state: &AppState,
    id: i64,
    user: &str,
) -> Result<(Conv, String), Response> {
    let pool = state
        .pool()
        .map_err(axum::response::IntoResponse::into_response)?;
    let (salon, serveur, dm, kind, slowmode, auteur) = sqlx::query_as::<
        _,
        (Option<String>, Option<String>, Option<String>, String, i32, String),
    >(
        "SELECT m.channel_id::text, ch.server_id::text, m.dm_id::text, COALESCE(ch.kind,''), COALESCE(ch.slowmode,0), m.author_id::text
         FROM messages m
         LEFT JOIN channels ch ON ch.id = m.channel_id
         WHERE m.id=$1 AND (
             (m.channel_id IS NOT NULL AND EXISTS (SELECT 1 FROM server_members sm WHERE sm.server_id = ch.server_id AND sm.user_id=$2::uuid))
             OR (m.dm_id IS NOT NULL AND EXISTS (SELECT 1 FROM dm_conversations d WHERE d.id = m.dm_id AND (d.user_a=$2::uuid OR d.user_b=$2::uuid))))",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let mut conv = Conv {
        channel_id: salon,
        server_id: serveur,
        dm_id: dm,
        kind,
        slowmode,
        user_limit: 0,
        perms: 0,
    };
    avec_droits(state, &mut conv, user).await?;
    Ok((conv, auteur))
}

pub async fn participants(pool: &PgPool, conv: &Conv) -> Result<Vec<String>, sqlx::Error> {
    if let (Some(salon), Some(serveur)) = (&conv.channel_id, &conv.server_id) {
        return roles::voyeurs_du_salon(pool, serveur, salon).await;
    }
    let Some(dm) = &conv.dm_id else {
        return Ok(Vec::new());
    };
    sqlx::query_scalar(
        "SELECT user_a::text FROM dm_conversations WHERE id=$1::uuid UNION SELECT user_b::text FROM dm_conversations WHERE id=$1::uuid",
    )
    .bind(dm)
    .fetch_all(pool)
    .await
}

pub async fn publier(state: &AppState, pool: &PgPool, conv: &Conv, typ: &str, data: &Value) {
    if let Ok(to) = participants(pool, conv).await
        && !to.is_empty()
    {
        state.publier(&to, typ, data);
    }
}

#[cfg(test)]
pub(crate) mod tests;
