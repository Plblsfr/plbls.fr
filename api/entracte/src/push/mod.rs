pub mod fcm;
pub mod webpush;

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    config::Config,
    etat::AppState,
    garde::Personne,
    http::{self, bd, erreur, horodatage, invalide, objet, ok},
};

const PREFIXE_FCM: &str = "fcm:";
pub const ABONNEMENTS_MAX: i64 = 10;
pub const ENVOIS_SIMULTANES: usize = 8;

pub struct Push {
    web: Option<webpush::Vapid>,
    fcm: Option<fcm::Fcm>,
    envois: Arc<Semaphore>,
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
            envois: Arc::new(Semaphore::new(ENVOIS_SIMULTANES)),
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
    let mut tx = state.pool().begin().await.map_err(bd)?;
    sqlx::query(
        "INSERT INTO abonnements_push (endpoint, sub, p256dh, auth, genre) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (endpoint) DO UPDATE SET sub = EXCLUDED.sub, p256dh = EXCLUDED.p256dh, auth = EXCLUDED.auth,
           genre = EXCLUDED.genre, cree_le = now()",
    )
    .bind(endpoint)
    .bind(sub)
    .bind(p256dh)
    .bind(auth)
    .bind(genre)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    sqlx::query(
        "DELETE FROM abonnements_push WHERE endpoint IN (
           SELECT endpoint FROM abonnements_push WHERE sub = $1 ORDER BY cree_le DESC, endpoint DESC OFFSET $2)",
    )
    .bind(sub)
    .bind(ABONNEMENTS_MAX)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)
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
    e.len() <= 1000 && webpush::endpoint_accepte(e)
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
        "url": "/#/films/requests",
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
    let octets: Arc<[u8]> = serde_json::to_vec(&message).unwrap_or_default().into();
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
        let appareils = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT endpoint, p256dh, auth, genre FROM abonnements_push WHERE sub = $1",
        )
        .bind(&sub)
        .fetch_all(state.pool())
        .await
        .unwrap_or_default();
        for appareil in appareils {
            expedier(state.clone(), appareil, octets.clone());
        }
    }
}

fn expedier(state: AppState, appareil: (String, String, String, String), octets: Arc<[u8]>) {
    let envois = state.push().envois.clone();
    tokio::spawn(async move {
        let Ok(_jeton) = envois.acquire_owned().await else {
            return;
        };
        let (endpoint, p256dh, auth, genre) = appareil;
        let resultat = match (genre.as_str(), &state.push().web, &state.push().fcm) {
            ("web", Some(v), _) => {
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
            _ => return,
        };
        match resultat {
            Ok(true) => {
                tracing::info!("abonnement {genre} refusé : retiré");
                let _ = sqlx::query("DELETE FROM abonnements_push WHERE endpoint = $1")
                    .bind(&endpoint)
                    .execute(state.pool())
                    .await;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("push {genre} : {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::postgres;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use ring::{
        agreement,
        rand::SystemRandom,
        signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
    };
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };
    use tokio::net::TcpListener;

    async fn etat() -> Option<(crate::testutil::BasePostgres, AppState)> {
        let (base, url) = postgres().await?;
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let paire =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
                .unwrap();
        let fin = pkcs8.as_ref().len() - 65 - 5;
        let mut cfg = Config::pour_tests(&url);
        cfg.vapid_privee = URL_SAFE_NO_PAD.encode(&pkcs8.as_ref()[fin - 32..fin]);
        cfg.vapid_publique = URL_SAFE_NO_PAD.encode(paire.public_key().as_ref());
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        crate::MIGRATIONS.run(&pool).await.unwrap();
        Some((base, AppState::new(cfg, pool)))
    }

    async fn endpoints(state: &AppState, sub: &str) -> Vec<String> {
        sqlx::query_scalar("SELECT endpoint FROM abonnements_push WHERE sub = $1 ORDER BY endpoint")
            .bind(sub)
            .fetch_all(state.pool())
            .await
            .unwrap()
    }

    async fn attendre(mut condition: impl FnMut() -> bool, quoi: &str) {
        let limite = Instant::now() + Duration::from_secs(20);
        while !condition() {
            assert!(Instant::now() < limite, "délai dépassé : {quoi}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    fn endpoint_hors_services_push_refuse() {
        assert!(endpoint_valide("https://fcm.googleapis.com/fcm/send/a"));
        assert!(endpoint_valide(
            "https://db5p.notify.windows.com/w/?token=a"
        ));
        for e in [
            "https://entracte:8080/admin",
            "https://169.254.169.254/latest",
            "https://10.0.0.2/x",
            "https://[fd00::1]/x",
            "http://fcm.googleapis.com/x",
            "ftp://x",
        ] {
            assert!(!endpoint_valide(e), "{e}");
        }
        assert!(!endpoint_valide(&format!(
            "https://fcm.googleapis.com/{}",
            "a".repeat(1000)
        )));
    }

    #[tokio::test]
    async fn plafond_remplace_le_plus_ancien_abonnement() {
        let Some((_base, state)) = etat().await else {
            return;
        };
        let ep = |i: i64| format!("https://fcm.googleapis.com/fcm/send/{i:02}");
        enregistrer(
            &state,
            "bob",
            "https://web.push.apple.com/b",
            "p",
            "a",
            "web",
        )
        .await
        .unwrap();
        for i in 0..=ABONNEMENTS_MAX {
            enregistrer(&state, "alice", &ep(i), "p", "a", "web")
                .await
                .unwrap();
        }
        let restants = endpoints(&state, "alice").await;
        assert_eq!(restants.len() as i64, ABONNEMENTS_MAX);
        assert!(!restants.contains(&ep(0)), "{restants:?}");
        enregistrer(&state, "alice", &ep(1), "p", "a", "web")
            .await
            .unwrap();
        enregistrer(&state, "alice", &ep(20), "p", "a", "web")
            .await
            .unwrap();
        let restants = endpoints(&state, "alice").await;
        assert_eq!(restants.len() as i64, ABONNEMENTS_MAX);
        assert!(restants.contains(&ep(1)) && restants.contains(&ep(20)));
        assert!(!restants.contains(&ep(2)), "{restants:?}");
        assert_eq!(endpoints(&state, "bob").await.len(), 1);
    }

    #[tokio::test]
    async fn prevenir_rend_la_main_avant_les_envois_bornes() {
        let Some((_base, state)) = etat().await else {
            return;
        };
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = ecoute.local_addr().unwrap();
        let ouvertes = Arc::new(AtomicUsize::new(0));
        let compte = ouvertes.clone();
        let _serveur = tokio::spawn(async move {
            let mut gardees = Vec::new();
            while let Ok((flux, _)) = ecoute.accept().await {
                compte.fetch_add(1, Ordering::SeqCst);
                gardees.push(flux);
            }
        });
        let rng = SystemRandom::new();
        let recepteur =
            agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let p256dh = URL_SAFE_NO_PAD.encode(recepteur.compute_public_key().unwrap().as_ref());
        let auth = URL_SAFE_NO_PAD.encode([5u8; 16]);
        let demande: Uuid = sqlx::query_scalar(
            "INSERT INTO demandes (genre, tmdb_id, titre) VALUES ('film', 1, 'Film') RETURNING id",
        )
        .fetch_one(state.pool())
        .await
        .unwrap();
        for sub in ["alice", "bob"] {
            sqlx::query("INSERT INTO demandeurs (demande_id, sub) VALUES ($1, $2)")
                .bind(demande)
                .bind(sub)
                .execute(state.pool())
                .await
                .unwrap();
            for i in 0..ABONNEMENTS_MAX {
                enregistrer(
                    &state,
                    sub,
                    &format!("http://{adresse}/{sub}/{i}"),
                    &p256dh,
                    &auth,
                    "web",
                )
                .await
                .unwrap();
            }
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            prevenir(&state, demande, "refusee", "Demande refusée", "motif"),
        )
        .await
        .expect("prevenir attend les services push");
        let notifications: i64 =
            sqlx::query_scalar("SELECT count(*) FROM notifications WHERE demande_id = $1")
                .bind(demande)
                .fetch_one(state.pool())
                .await
                .unwrap();
        assert_eq!(notifications, 2);
        attendre(
            || ouvertes.load(Ordering::SeqCst) >= ENVOIS_SIMULTANES,
            "envois lancés",
        )
        .await;
        assert_eq!(state.push().envois.available_permits(), 0);
        assert_eq!(ouvertes.load(Ordering::SeqCst), ENVOIS_SIMULTANES);
    }
}
