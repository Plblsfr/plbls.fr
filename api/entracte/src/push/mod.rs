pub mod fcm;
pub mod webpush;

use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    config::Config,
    etat::AppState,
    garde::Personne,
    http::{self, bd, erreur, horodatage, invalide, objet, ok},
};

const PREFIXE_FCM: &str = "fcm:";

pub struct Push {
    web: Option<webpush::Vapid>,
    fcm: Option<fcm::Fcm>,
}

impl Push {
    pub fn nouveau(cfg: &Config, _http: reqwest::Client) -> Self {
        let fcm = match fcm::Fcm::depuis_base64(&cfg.fcm) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("{e} : notifications Android coupées");
                None
            }
        };
        Self {
            web: webpush::Vapid::nouveau(&cfg.vapid_publique, &cfg.vapid_privee, &cfg.vapid_sujet),
            fcm,
        }
    }

    pub fn web_actif(&self) -> bool {
        self.web.is_some()
    }

    pub fn fcm_actif(&self) -> bool {
        self.fcm.is_some()
    }

    #[cfg(test)]
    pub fn avec_fcm(&mut self, f: fcm::Fcm) {
        self.fcm = Some(f);
    }
}

fn sans_push() -> Response {
    erreur(
        StatusCode::NOT_IMPLEMENTED,
        "no_push",
        "notifications push non configurées",
    )
}

pub async fn cle(State(state): State<AppState>) -> Response {
    match &state.push().web {
        Some(v) => ok(&json!({ "key": v.public })),
        None => sans_push(),
    }
}

fn texte(o: &serde_json::Map<String, Value>, cle: &str) -> Option<String> {
    o.get(cle)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

async fn enregistrer(
    state: &AppState,
    sub: &str,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
    genre: &str,
) -> Result<(), Response> {
    sqlx::query(
        "INSERT INTO abonnements_push (endpoint, sub, p256dh, auth, genre) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (endpoint) DO UPDATE SET sub = EXCLUDED.sub, p256dh = EXCLUDED.p256dh, auth = EXCLUDED.auth, genre = EXCLUDED.genre",
    )
    .bind(endpoint)
    .bind(sub)
    .bind(p256dh)
    .bind(auth)
    .bind(genre)
    .execute(state.pool())
    .await
    .map(|_| ())
    .map_err(bd)
}

async fn supprimer(state: &AppState, sub: &str, endpoint: &str) -> Result<(), Response> {
    sqlx::query("DELETE FROM abonnements_push WHERE endpoint = $1 AND sub = $2")
        .bind(endpoint)
        .bind(sub)
        .execute(state.pool())
        .await
        .map(|_| ())
        .map_err(bd)
}

fn endpoint_valide(e: &str) -> bool {
    let schema_ok = |s: &str| s == "https" || (cfg!(test) && s == "http");
    e.len() <= 1000
        && url::Url::parse(e).is_ok_and(|u| schema_ok(u.scheme()) && u.host_str().is_some())
}

pub async fn abonner(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    if state.push().web.is_none() {
        return Err(sans_push());
    }
    let o = objet(corps).await?;
    let endpoint = texte(&o, "endpoint")
        .filter(|e| endpoint_valide(e))
        .ok_or_else(|| invalide("endpoint invalide"))?;
    let cles = o
        .get("keys")
        .and_then(Value::as_object)
        .ok_or_else(|| invalide("clés manquantes"))?;
    let (Some(p256dh), Some(auth)) = (texte(cles, "p256dh"), texte(cles, "auth")) else {
        return Err(invalide("clés manquantes"));
    };
    if p256dh.len() > 200 || auth.len() > 100 {
        return Err(invalide("clés invalides"));
    }
    enregistrer(&state, p.sub(), &endpoint, &p256dh, &auth, "web").await?;
    Ok(http::sans_contenu())
}

pub async fn desabonner(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let o = objet(corps).await?;
    let endpoint = texte(&o, "endpoint").ok_or_else(|| invalide("endpoint manquant"))?;
    supprimer(&state, p.sub(), &endpoint).await?;
    Ok(http::sans_contenu())
}

pub async fn abonner_fcm(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    if state.push().fcm.is_none() {
        return Err(sans_push());
    }
    let o = objet(corps).await?;
    let jeton = texte(&o, "token")
        .filter(|t| t.len() <= 4096)
        .ok_or_else(|| invalide("jeton invalide"))?;
    if let Some(ancien) = texte(&o, "previous") {
        supprimer(&state, p.sub(), &format!("{PREFIXE_FCM}{ancien}")).await?;
    }
    enregistrer(
        &state,
        p.sub(),
        &format!("{PREFIXE_FCM}{jeton}"),
        "",
        "",
        "fcm",
    )
    .await?;
    Ok(http::sans_contenu())
}

pub async fn desabonner_fcm(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let o = objet(corps).await?;
    let jeton = texte(&o, "token").ok_or_else(|| invalide("jeton manquant"))?;
    supprimer(&state, p.sub(), &format!("{PREFIXE_FCM}{jeton}")).await?;
    Ok(http::sans_contenu())
}

pub async fn notifications(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let apres: i64 = match q.get("after").filter(|v| !v.is_empty()) {
        None => 0,
        Some(v) => v.parse().map_err(|_| invalide("after invalide"))?,
    };
    let lignes = sqlx::query(
        "SELECT id, genre, demande_id, titre, corps, cree_le FROM notifications WHERE sub = $1 AND id > $2
         ORDER BY id DESC LIMIT 50",
    )
    .bind(p.sub())
    .bind(apres)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let dernier: Option<i64> =
        sqlx::query_scalar("SELECT max(id) FROM notifications WHERE sub = $1")
            .bind(p.sub())
            .fetch_one(state.pool())
            .await
            .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| json!({
            "id": l.get::<i64, _>(0),
            "kind": l.get::<String, _>(1),
            "request_id": l.get::<Option<Uuid>, _>(2),
            "title": l.get::<String, _>(3),
            "body": l.get::<String, _>(4),
            "created_at": horodatage(&l.get::<DateTime<Utc>, _>(5)),
        })).collect::<Vec<_>>(),
        "last_id": dernier.unwrap_or(0),
    })))
}

pub fn charge(demande: Uuid, genre: &str, titre: &str, corps: &str) -> Value {
    json!({
        "title": titre,
        "body": corps,
        "tag": format!("entracte-{demande}"),
        "url": "/#/entracte/requests",
        "kind": format!("entracte.{genre}"),
    })
}

pub async fn prevenir(state: &AppState, demande: Uuid, genre: &str, titre: &str, corps: &str) {
    let subs: Vec<String> =
        match sqlx::query_scalar("SELECT sub FROM demandeurs WHERE demande_id = $1")
            .bind(demande)
            .fetch_all(state.pool())
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("demandeurs : {e}");
                return;
            }
        };
    let message = charge(demande, genre, titre, corps);
    let octets = serde_json::to_vec(&message).unwrap_or_default();
    for sub in subs {
        let r = sqlx::query("INSERT INTO notifications (sub, genre, demande_id, titre, corps) VALUES ($1, $2, $3, $4, $5)")
            .bind(&sub)
            .bind(genre)
            .bind(demande)
            .bind(titre)
            .bind(corps)
            .execute(state.pool())
            .await;
        if let Err(e) = r {
            tracing::warn!("notification : {e}");
        }
        let appareils = sqlx::query(
            "SELECT endpoint, p256dh, auth, genre FROM abonnements_push WHERE sub = $1",
        )
        .bind(&sub)
        .fetch_all(state.pool())
        .await
        .unwrap_or_default();
        for a in appareils {
            let endpoint: String = a.get(0);
            let genre_appareil: String = a.get(3);
            let resultat = match (
                genre_appareil.as_str(),
                &state.push().web,
                &state.push().fcm,
            ) {
                ("web", Some(v), _) => {
                    let p256dh: String = a.get(1);
                    let auth: String = a.get(2);
                    let cible = webpush::Abonnement {
                        endpoint: &endpoint,
                        p256dh: &p256dh,
                        auth: &auth,
                        urgent: false,
                    };
                    webpush::envoyer(v, &cible, &octets).await
                }
                ("fcm", _, Some(f)) => {
                    f.envoyer(
                        endpoint.strip_prefix(PREFIXE_FCM).unwrap_or(&endpoint),
                        &octets,
                    )
                    .await
                }
                _ => continue,
            };
            match resultat {
                Ok(true) => {
                    tracing::info!(
                        "abonnement {genre_appareil} refusé par le service push : retiré"
                    );
                    let _ = sqlx::query("DELETE FROM abonnements_push WHERE endpoint = $1")
                        .bind(&endpoint)
                        .execute(state.pool())
                        .await;
                }
                Ok(false) => {}
                Err(e) => tracing::warn!("push {genre_appareil} : {e}"),
            }
        }
    }
}
