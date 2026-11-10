use std::collections::{HashMap, HashSet};

use axum::{
    body::{Body, to_bytes},
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use rand::Rng;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    http::{CORPS_MAX, champ, champ_texte, decode, internal, premier_objet, write_err, write_json},
    openapi,
    roles::{self, Access, Perm},
    state::{AppState, DbIndisponible},
    users::User,
};

mod acces_serveur;
pub use acces_serveur::*;
mod gestion_salons;
use gestion_salons::*;
mod invitations;
use invitations::*;
mod outils;
pub use outils::*;
mod serveurs;
use serveurs::*;

pub const MAX_SALONS: i64 = 50;
pub const MAX_ROLES: usize = 50;
pub const MAX_PARTICIPANTS_VOCAL: i64 = 10;

const ALPHABET_CODE: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
const TYPES: [&str; 5] = ["text", "voice", "videos", "music", "cinema"];
pub const TYPES_VOCAUX: [&str; 2] = ["voice", "cinema"];

pub fn cinema_disponible(state: &AppState) -> bool {
    let actif = |nom: &str| state.blocs_actifs().iter().any(|b| b == nom);
    actif("films") || (actif("regarder-ensemble") && state.config().entracte_actif())
}

pub fn est_vocal(kind: &str) -> bool {
    TYPES_VOCAUX.contains(&kind)
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/servers", post(creer).get(lister))
        .route("/servers/{id}", get(voir).patch(renommer).delete(supprimer))
        .route("/servers/{id}/leave", post(quitter))
        .route("/servers/{id}/channels", post(creer_salon))
        .route(
            "/servers/{id}/channels/order",
            put(ordonner).patch(renommer_salon).delete(supprimer_salon),
        )
        .route(
            "/servers/{id}/channels/{cid}",
            patch(renommer_salon).delete(supprimer_salon),
        )
        .route(
            "/servers/{id}/invites",
            get(lister_invitations).post(creer_invitation),
        )
        .route("/servers/{id}/invites/{code}", delete(revoquer))
        .route("/server-invites/{code}/join", post(rejoindre))
        .merge(crate::gestion_roles::router())
        .merge(crate::moderation::router())
        .merge(crate::messages::router())
        .merge(crate::sondages::router())
        .merge(crate::mentions::router())
        .merge(crate::recherche::router())
        .merge(crate::etat_lecture::router());
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("serveurs", resume, reponses, chemin);
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
        HttpMethod::Post,
        "/servers",
        "Créer un serveur (avec un salon texte « général » et un salon vocal « vocal »)",
        &[("201", "Créé"), ("400", "Requête invalide")],
    ),
    (
        HttpMethod::Get,
        "/servers",
        "Serveurs de l'utilisateur",
        &[("200", "ok")],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}",
        "Détail d'un serveur (membre uniquement)",
        &[
            ("200", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/servers/{id}",
        "Renommer (permission `manage_server`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}",
        "Supprimer (créateur)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/leave",
        "Quitter un serveur",
        &[
            ("204", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "Conflit"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/channels",
        "Créer un salon (permission `manage_channels` ; 50 max par serveur)",
        &[
            ("201", "Créé"),
            ("400", "Requête invalide"),
            ("403", "Refusé"),
            ("409", "Conflit"),
        ],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/channels/order",
        "Réordonner les salons d'un type (permission `manage_channels`)",
        &[
            ("204", "ok"),
            ("400", "Requête invalide"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/servers/{id}/channels/{cid}",
        "Renommer un salon (permission `manage_channels`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "Conflit"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/channels/{cid}",
        "Supprimer un salon et ses messages (permission `manage_channels`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/invites",
        "Liens d'invitation actifs (permission `create_invites`)",
        &[("200", "ok")],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/invites",
        "Créer un lien d'invitation, réutilisable jusqu'à révocation, épuisement ou expiration (permission `create_invites`)",
        &[("201", "Créé"), ("400", "Requête invalide")],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/invites/{code}",
        "Révoquer un lien (permission `create_invites`)",
        &[
            ("204", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/server-invites/{code}/join",
        "Rejoindre un serveur avec un code (sans effet si déjà membre)",
        &[
            ("200", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
];

pub struct Serveur {
    pub id: String,
    name: String,
    pub(crate) owner_id: String,
    icon_version: i64,
}

impl Serveur {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "owner_id": self.owner_id,
            "icon_version": self.icon_version,
        })
    }
}

type LigneServeur = (String, String, String, i64);

impl From<LigneServeur> for Serveur {
    fn from((id, name, owner_id, icon_version): LigneServeur) -> Self {
        Serveur {
            id,
            name,
            owner_id,
            icon_version,
        }
    }
}

struct Salon {
    id: String,
    name: String,
    kind: String,
    position: i32,
    slowmode: i32,
    locked: bool,
    topic: String,
    user_limit: i32,
}

impl Salon {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "kind": self.kind,
            "position": self.position,
            "slowmode": self.slowmode,
            "locked": self.locked,
            "topic": self.topic,
            "user_limit": self.user_limit,
        })
    }
}

struct NouveauSalon {
    name: String,
    kind: String,
    topic: String,
    slowmode: i32,
    user_limit: i32,
    after: String,
    prive: bool,
    roles: Vec<String>,
    membres: Vec<String>,
    createur: String,
}

#[cfg(test)]
pub(crate) mod tests;
