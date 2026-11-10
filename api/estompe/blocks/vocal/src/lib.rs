#![allow(clippy::result_large_err)]

mod livekit;

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use axum::{
    body::{Body, to_bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use estompe_core::{
    api::ActiveUser,
    audit,
    http::{champ, champ_texte, decode, internal, write_err, write_json},
    messages::{Conv, EvenementMessage, SYSTEME},
    openapi,
    roles::{self, Access, Perm},
    servers::{self, pool, uuid_valide},
    state::{AppState, ParticipantsVocal, Vocal, VocalInjoignable},
};
use livekit::{Erreur, Lk, Mod, Participant, capacite, salle, salon_de};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::{HttpMethod, path::Operation};
use utoipa_axum::router::OpenApiRouter;

mod deplacement;
use deplacement::*;
mod jeton;
use jeton::*;
mod moderation;
use moderation::*;
mod outils;
use outils::*;
mod salons;
use salons::*;
mod webhook;
use webhook::*;

const CORPS_WEBHOOK_MAX: usize = 1 << 16;
const SESSION_MAX: i64 = 12 * 3600;
const HORODATAGE: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.US";

type Params = Path<HashMap<String, String>>;

struct Salon {
    id: String,
    serveur: String,
    limite: i32,
    perms: Perm,
}

struct Cible {
    salon: Salon,
    acces: Access,
    acces_cible: Access,
    uid: String,
}

struct Accroche(AppState);

impl Vocal for Accroche {
    fn sortir_du_serveur<'a>(
        &'a self,
        serveur: &'a str,
        membre: &'a str,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let Ok(pool) = self.0.pool() else { return };
            if let Ok(salons) = salons_vocaux(pool, serveur).await {
                evincer(&Lk::new(&self.0.config().livekit), &salons, membre).await;
            }
        })
    }

    fn participants<'a>(
        &'a self,
        salon: &'a str,
    ) -> Pin<Box<dyn Future<Output = ParticipantsVocal> + Send + 'a>> {
        Box::pin(async move {
            match Lk::new(&self.0.config().livekit).participants(salon).await {
                Ok(ps) => Ok(ps.iter().map(|p| json!(p)).collect()),
                Err(Erreur::Absent) => Ok(Vec::new()),
                Err(_) => Err(VocalInjoignable),
            }
        })
    }
}

pub fn brancher(state: &AppState) {
    state.definir_vocal(Arc::new(Accroche(state.clone())));
    state.sur_droits_modifies(|s, serveur, membre| async move {
        synchroniser(&s, &serveur, &membre).await;
    });
    state.sur_acces_retire(|s, user| async move {
        let Ok(pool) = s.pool() else { return };
        if let Ok(salons) =
            sqlx::query_scalar::<_, String>("SELECT id::text FROM channels WHERE kind = ANY($1)")
                .bind(&servers::TYPES_VOCAUX[..])
                .fetch_all(pool)
                .await
        {
            evincer(&Lk::new(&s.config().livekit), &salons, &user).await;
        }
    });
}

fn operation(resume: &str, reponses: &[(&str, &str)], chemin: &str) -> Operation {
    openapi::operation("vocal", resume, reponses, chemin)
}

pub fn routes() -> OpenApiRouter<AppState> {
    let mut webhook_op = operation(
        "Webhook LiveKit (signé), relayé en évènement `voice.state`",
        &[("204", "ok"), ("401", "bad_signature")],
        "/voice/webhook",
    );
    webhook_op.security = Some(vec![]);
    let r = OpenApiRouter::new()
        .route("/voice/{cid}/token", post(jeton))
        .route("/servers/{id}/voice", get(presents))
        .route("/servers/{id}/voice/stats", get(statistiques))
        .route("/voice/{cid}/participants/{uid}/mod", put(moderer))
        .route("/servers/{id}/voice/moderation-log", get(journal))
        .route("/voice/webhook", post(webhook))
        .route("/voice/{cid}/participants/{uid}/move", put(deplacer))
        .route(
            "/voice/{cid}/participants/{uid}/disconnect",
            post(deconnecter),
        );
    let documentees = [
        (
            "/voice/{cid}/token",
            HttpMethod::Post,
            operation(
                "Jeton pour rejoindre un salon vocal avec le client LiveKit",
                &[
                    ("200", "ok"),
                    ("400", "not_voice_channel"),
                    ("403", "forbidden"),
                    ("404", "not_found"),
                    (
                        "409",
                        "voice_full : limite du salon atteinte (10 participants au plus)",
                    ),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/token",
            ),
        ),
        (
            "/servers/{id}/voice",
            HttpMethod::Get,
            operation(
                "Participants présents dans chaque salon vocal",
                &[
                    ("200", "ok"),
                    ("404", "not_found"),
                    ("502", "voice_unavailable"),
                ],
                "/servers/{id}/voice",
            ),
        ),
        (
            "/servers/{id}/voice/stats",
            HttpMethod::Get,
            operation(
                "Temps passé en vocal (secondes) par membre dans ce serveur, session en cours comprise",
                &[("200", "ok"), ("404", "not_found")],
                "/servers/{id}/voice/stats",
            ),
        ),
        (
            "/voice/{cid}/participants/{uid}/mod",
            HttpMethod::Put,
            operation(
                "Couper ou rétablir le micro et/ou le son d'un participant pour tout le salon (EST-102)",
                &[
                    ("204", "ok"),
                    ("400", "corps invalide"),
                    ("403", "forbidden"),
                    ("409", "not_in_voice"),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/participants/{uid}/mod",
            ),
        ),
        (
            "/servers/{id}/voice/moderation-log",
            HttpMethod::Get,
            operation(
                "Journal des coupures de micro ou de son (100 dernières, les plus récentes d'abord)",
                &[("200", "ok"), ("403", "forbidden"), ("404", "not_found")],
                "/servers/{id}/voice/moderation-log",
            ),
        ),
        ("/voice/webhook", HttpMethod::Post, webhook_op),
        (
            "/voice/{cid}/participants/{uid}/move",
            HttpMethod::Put,
            operation(
                "Déplacer un participant vers un autre salon vocal du serveur",
                &[
                    ("204", "ok"),
                    ("400", "invalid_channel"),
                    ("403", "forbidden"),
                    ("404", "not_found"),
                    ("409", "`not_in_voice` ou `cannot_join`"),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/participants/{uid}/move",
            ),
        ),
        (
            "/voice/{cid}/participants/{uid}/disconnect",
            HttpMethod::Post,
            operation(
                "Déconnecter un participant du vocal sans l'expulser du serveur",
                &[
                    ("204", "ok"),
                    ("403", "forbidden"),
                    ("404", "not_found"),
                    ("409", "not_in_voice"),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/participants/{uid}/disconnect",
            ),
        ),
    ];
    documentees.into_iter().fold(r, |r, (chemin, methode, op)| {
        openapi::documenter(r, chemin, methode, op)
    })
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes())
}

#[cfg(test)]
mod tests;
