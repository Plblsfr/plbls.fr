pub mod decision;
pub mod fcm;
pub mod webpush;

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use decision::{Evenement, Lieu, Notification, Reglages, decider};
use estompe_core::{
    api::ActiveUser,
    http::{champ, champ_texte, decode, internal, write_err, write_json},
    messages::{Conv, EvenementMessage, participants},
    openapi,
    state::{AppState, Notifieur},
};
use fcm::Fcm;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tokio::sync::{Semaphore, mpsc};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;
use webpush::{Abonnement, Vapid};

const PREFIXE_FCM: &str = "fcm:";
const PREFIXE_PC: &str = "desktop:";
const NIVEAUX_MAX: usize = 1000;
const FILE_MAX: usize = 256;
const DELAI_TRAITEMENT: Duration = Duration::from_secs(30);
pub const ABONNEMENTS_MAX: i64 = 10;
pub const ENVOIS_SIMULTANES: usize = 8;

pub type Futur<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct Appareil {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub kind: String,
}

pub trait Envoyeur: Send + Sync {
    fn envoyer<'a>(
        &'a self,
        appareil: &'a Appareil,
        notification: &'a Notification,
    ) -> Futur<'a, Result<bool, String>>;
}

pub struct EnvoyeurWeb(pub Vapid);

impl Envoyeur for EnvoyeurWeb {
    fn envoyer<'a>(
        &'a self,
        appareil: &'a Appareil,
        n: &'a Notification,
    ) -> Futur<'a, Result<bool, String>> {
        Box::pin(async move {
            let charge = serde_json::to_vec(n).map_err(|e| e.to_string())?;
            let cible = Abonnement {
                endpoint: &appareil.endpoint,
                p256dh: &appareil.p256dh,
                auth: &appareil.auth,
                urgent: n.kind == "dm" || n.kind == "mention",
            };
            webpush::envoyer(&self.0, &cible, &charge).await
        })
    }
}

pub struct EnvoyeurFcm(pub Fcm);

impl Envoyeur for EnvoyeurFcm {
    fn envoyer<'a>(
        &'a self,
        appareil: &'a Appareil,
        n: &'a Notification,
    ) -> Futur<'a, Result<bool, String>> {
        Box::pin(async move {
            let charge = serde_json::to_vec(n).map_err(|e| e.to_string())?;
            let jeton = appareil
                .endpoint
                .strip_prefix(PREFIXE_FCM)
                .unwrap_or(&appareil.endpoint);
            self.0.envoyer(jeton, &charge).await
        })
    }
}

fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    internal()
}

pub async fn enregistrer(
    pool: &PgPool,
    user: &str,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
) -> Result<(), sqlx::Error> {
    let genre = match endpoint.split_once(':') {
        Some(("fcm", _)) => "fcm",
        Some(("desktop", _)) => "desktop",
        _ => "web",
    };
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO push_subscriptions (endpoint, user_id, p256dh, auth, kind) VALUES ($1,$2::uuid,$3,$4,$5)
         ON CONFLICT (endpoint) DO UPDATE SET user_id=EXCLUDED.user_id, p256dh=EXCLUDED.p256dh, auth=EXCLUDED.auth, kind=EXCLUDED.kind, created_at=now()",
    )
    .bind(endpoint)
    .bind(user)
    .bind(p256dh)
    .bind(auth)
    .bind(genre)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM push_subscriptions WHERE endpoint IN (
           SELECT endpoint FROM push_subscriptions WHERE user_id=$1::uuid ORDER BY created_at DESC, endpoint DESC OFFSET $2)",
    )
    .bind(user)
    .bind(ABONNEMENTS_MAX)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

pub async fn supprimer(pool: &PgPool, user: &str, endpoint: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM push_subscriptions WHERE endpoint=$1 AND user_id=$2::uuid")
        .bind(endpoint)
        .bind(user)
        .execute(pool)
        .await
        .map(|_| ())
}

async fn retirer_expire(pool: &PgPool, endpoint: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM push_subscriptions WHERE endpoint=$1")
        .bind(endpoint)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn appareils(pool: &PgPool, user: &str) -> Result<Vec<Appareil>, sqlx::Error> {
    let lignes = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT endpoint, p256dh, auth, kind FROM push_subscriptions WHERE user_id=$1::uuid",
    )
    .bind(user)
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|(endpoint, p256dh, auth, kind)| Appareil {
            endpoint,
            p256dh,
            auth,
            kind,
        })
        .collect())
}

pub async fn reglages(pool: &PgPool, user: &str) -> Result<Reglages, sqlx::Error> {
    let brut: Value = sqlx::query_scalar("SELECT notify_settings FROM users WHERE id=$1::uuid")
        .bind(user)
        .fetch_one(pool)
        .await?;
    let objet = match brut {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    Reglages::depuis_objet(objet).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

async fn enregistrer_reglages(
    pool: &PgPool,
    user: &str,
    valeur: &Reglages,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET notify_settings=$2 WHERE id=$1::uuid")
        .bind(user)
        .bind(serde_json::to_value(valeur).unwrap_or(Value::Null))
        .execute(pool)
        .await
        .map(|_| ())
}

async fn statut(pool: &PgPool, user: &str) -> String {
    sqlx::query_scalar("SELECT status FROM users WHERE id=$1::uuid")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "online".to_string())
}

async fn nom_du_salon(pool: &PgPool, salon: &str) -> String {
    sqlx::query_scalar("SELECT name FROM channels WHERE id=$1::uuid")
        .bind(salon)
        .fetch_one(pool)
        .await
        .unwrap_or_default()
}

pub struct Service {
    etat: AppState,
    envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>>,
    file: mpsc::Sender<Evenement>,
    envois: Arc<Semaphore>,
}

impl Service {
    pub fn publier(&self, e: Evenement) {
        if self.file.try_send(e).is_err() {
            tracing::warn!("notify : file pleine, évènement perdu");
        }
    }

    async fn traiter(&self, e: Evenement) {
        let Ok(pool) = self.etat.pool() else {
            return;
        };
        let destinataires = match &e.a {
            Some(a) => a.clone(),
            None => {
                let conv = Conv {
                    channel_id: e.lieu.salon.clone(),
                    server_id: e.lieu.serveur.clone(),
                    dm_id: e.lieu.dm.clone(),
                    kind: String::new(),
                    slowmode: 0,
                    user_limit: 0,
                    perms: 0,
                };
                match participants(pool, &conv).await {
                    Ok(p) => p,
                    Err(err) => {
                        tracing::warn!("notify : destinataires : {err}");
                        return;
                    }
                }
            }
        };
        let ou = match &e.lieu.salon {
            Some(salon) => nom_du_salon(pool, salon).await,
            None => String::new(),
        };
        for id in destinataires {
            if id == e.auteur_id() || statut(pool, &id).await == "dnd" {
                continue;
            }
            let reglages = match reglages(pool, &id).await {
                Ok(r) => r,
                Err(err) => {
                    tracing::warn!("notify : réglages de {id} : {err}");
                    continue;
                }
            };
            if let Some(n) = decider(&reglages, &e, &id, &ou)
                && !self.en_direct(&id, &n)
            {
                self.envoyer(pool, &id, &n).await;
            }
        }
    }

    fn en_direct(&self, user: &str, n: &Notification) -> bool {
        let hub = self.etat.hub();
        if !hub.online(user) {
            return false;
        }
        hub.publish(
            &[user.to_string()],
            "notification",
            &serde_json::to_value(n).unwrap_or(Value::Null),
        );
        true
    }

    async fn envoyer(&self, pool: &PgPool, user: &str, n: &Notification) {
        let appareils = match appareils(pool, user).await {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!("notify : appareils de {user} : {err}");
                return;
            }
        };
        let n = Arc::new(n.clone());
        for a in appareils {
            if let Some(envoyeur) = self.envoyeurs.get(a.kind.as_str()) {
                self.expedier(pool, user, envoyeur, a, n.clone());
            }
        }
    }

    fn expedier(
        &self,
        pool: &PgPool,
        user: &str,
        envoyeur: &Arc<dyn Envoyeur>,
        a: Appareil,
        n: Arc<Notification>,
    ) {
        let (pool, user, envoyeur, envois) = (
            pool.clone(),
            user.to_string(),
            envoyeur.clone(),
            self.envois.clone(),
        );
        tokio::spawn(async move {
            let Ok(_jeton) = envois.acquire_owned().await else {
                return;
            };
            match tokio::time::timeout(DELAI_TRAITEMENT, envoyeur.envoyer(&a, &n)).await {
                Ok(Ok(true)) => {
                    let _ = retirer_expire(&pool, &a.endpoint).await;
                    tracing::info!("notify : appareil {} retiré ({})", a.kind, n.tag);
                }
                Ok(Ok(false)) => tracing::info!("notify : envoyé {} à {user} ({})", a.kind, n.tag),
                Ok(Err(err)) => tracing::warn!("notify : échec {} pour {user} : {err}", a.kind),
                Err(_) => tracing::warn!("notify : délai dépassé {} pour {user}", a.kind),
            }
        });
    }

    async fn marquer_lu(&self, user: &str, conv: &str, dernier: &str) {
        let Some(envoyeur) = self.envoyeurs.get("fcm") else {
            return;
        };
        let Ok(pool) = self.etat.pool() else {
            return;
        };
        let appareils = match appareils(pool, user).await {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!("notify : appareils de {user} : {err}");
                return;
            }
        };
        let n = Arc::new(Notification {
            tag: format!("read-{conv}"),
            conv: conv.to_string(),
            read: dernier.to_string(),
            ..Default::default()
        });
        for a in appareils.into_iter().filter(|a| a.kind == "fcm") {
            self.expedier(pool, user, envoyeur, a, n.clone());
        }
    }
}

struct Passerelle(Arc<Service>);

impl Notifieur for Passerelle {
    fn publier<'a>(&'a self, e: &'a EvenementMessage) -> Futur<'a, ()> {
        self.0.publier(Evenement {
            lieu: Lieu {
                salon: e.conv.channel_id.clone(),
                serveur: e.conv.server_id.clone(),
                dm: e.conv.dm_id.clone(),
            },
            message: e.message.clone(),
            mentionnes: e.mentionnes.clone(),
            groupe: e.groupe.clone(),
            a: (!e.a.is_empty()).then(|| e.a.clone()),
            vocal: e.vocal.clone(),
            rappel: e.rappel.clone(),
        });
        Box::pin(async {})
    }

    fn retirer<'a>(&'a self, user: &'a str, conv: &'a str, dernier: &'a str) -> Futur<'a, ()> {
        let service = self.0.clone();
        let (user, conv, dernier) = (user.to_string(), conv.to_string(), dernier.to_string());
        tokio::spawn(async move {
            let _ =
                tokio::time::timeout(DELAI_TRAITEMENT, service.marquer_lu(&user, &conv, &dernier))
                    .await;
        });
        Box::pin(async {})
    }
}

pub fn installer_avec(
    etat: &AppState,
    envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>>,
) -> Arc<Service> {
    let (file, mut recu) = mpsc::channel(FILE_MAX);
    let service = Arc::new(Service {
        etat: etat.clone(),
        envoyeurs,
        file,
        envois: Arc::new(Semaphore::new(ENVOIS_SIMULTANES)),
    });
    let tache = service.clone();
    tokio::spawn(async move {
        while let Some(e) = recu.recv().await {
            let _ = tokio::time::timeout(DELAI_TRAITEMENT, tache.traiter(e)).await;
        }
    });
    etat.definir_notifieur(Arc::new(Passerelle(service.clone())));
    service
}

pub fn installer(etat: &AppState) -> Result<Arc<Service>, String> {
    let c = etat.config();
    let mut envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>> = HashMap::new();
    if let Some(v) = vapid(etat) {
        envoyeurs.insert("web", Arc::new(EnvoyeurWeb(v)));
    }
    if let Some(f) = Fcm::depuis_base64(&c.fcm_service_account)? {
        envoyeurs.insert("fcm", Arc::new(EnvoyeurFcm(f)));
    }
    Ok(installer_avec(etat, envoyeurs))
}

fn vapid(etat: &AppState) -> Option<Vapid> {
    let c = etat.config();
    Vapid::nouveau(&c.vapid_public, &c.vapid_private, &c.vapid_subject)
}

fn fcm_configure(etat: &AppState) -> bool {
    !etat.config().fcm_service_account.trim().is_empty()
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    routes(router)
}

fn routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    let doc = |resume: &str, reponses: &[(&str, &str)]| {
        openapi::operation("comptes", resume, reponses, "")
    };
    let r = router
        .route("/push/key", get(cle))
        .route("/push/subscription", put(abonner).delete(desabonner))
        .route("/push/test", post(essai))
        .route("/push/fcm", put(abonner_fcm).delete(desabonner_fcm))
        .route("/push/desktop", put(abonner_pc).delete(desabonner_pc))
        .route(
            "/me/notify-settings",
            get(lire_reglages).put(ecrire_reglages),
        );
    let r = openapi::documenter(
        r,
        "/push/key",
        HttpMethod::Get,
        doc(
            "Clé publique VAPID pour s'abonner aux notifications push",
            &[("200", "ok"), ("501", "no_push (VAPID non configuré)")],
        ),
    );
    let r = openapi::documenter(
        r,
        "/push/subscription",
        HttpMethod::Put,
        doc(
            "Enregistrer l'abonnement push de ce navigateur (PushSubscription.toJSON())",
            &[
                ("204", "ok"),
                ("400", "invalid_subscription"),
                ("501", "no_push"),
            ],
        ),
    );
    let r = openapi::documenter(
        r,
        "/push/subscription",
        HttpMethod::Delete,
        doc(
            "Supprimer l'abonnement push de ce navigateur",
            &[("204", "ok")],
        ),
    );
    let r = openapi::documenter(
        r,
        "/push/test",
        HttpMethod::Post,
        doc(
            "Envoyer une notification de test aux abonnements Web Push du compte",
            &[
                ("204", "ok"),
                ("404", "no_subscription"),
                ("501", "no_push"),
                ("502", "push_failed"),
            ],
        ),
    );
    let r = openapi::documenter(
        r,
        "/push/fcm",
        HttpMethod::Put,
        doc(
            "Enregistrer le jeton FCM de l'app Android",
            &[
                ("204", "ok"),
                ("400", "invalid_subscription"),
                ("501", "no_push"),
            ],
        ),
    );
    let r = openapi::documenter(
        r,
        "/push/fcm",
        HttpMethod::Delete,
        doc("Supprimer le jeton FCM de cet appareil", &[("204", "ok")]),
    );
    let r = openapi::documenter(
        r,
        "/push/desktop",
        HttpMethod::Put,
        doc(
            "Enregistrer l'installation de l'app Windows",
            &[("204", "ok"), ("400", "invalid_subscription")],
        ),
    );
    let r = openapi::documenter(
        r,
        "/push/desktop",
        HttpMethod::Delete,
        doc(
            "Retirer cette installation (à la déconnexion)",
            &[("204", "ok")],
        ),
    );
    let r = openapi::documenter(
        r,
        "/me/notify-settings",
        HttpMethod::Get,
        doc(
            "Réglages de notifications (identiques sur tous les appareils)",
            &[("200", "ok")],
        ),
    );
    openapi::documenter(
        r,
        "/me/notify-settings",
        HttpMethod::Put,
        doc(
            "Remplacer les réglages de notifications",
            &[("204", "ok"), ("400", "invalid_settings")],
        ),
    )
}

fn sans_push() -> Response {
    write_err(
        StatusCode::NOT_IMPLEMENTED,
        "no_push",
        "notifications push indisponibles",
    )
}

fn abonnement_invalide(message: &str) -> Response {
    write_err(StatusCode::BAD_REQUEST, "invalid_subscription", message)
}

fn json_invalide() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

async fn cle(State(etat): State<AppState>, ActiveUser(_): ActiveUser) -> Response {
    match vapid(&etat) {
        Some(v) => write_json(StatusCode::OK, &json!({ "public_key": v.public })),
        None => sans_push(),
    }
}

async fn abonner(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    if vapid(&etat).is_none() {
        return sans_push();
    }
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(endpoint) = champ_texte(&objet, "endpoint") else {
        return json_invalide();
    };
    let cles = match champ(&objet, "keys") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(m)) => m.clone(),
        Some(_) => return json_invalide(),
    };
    let (Some(p256dh), Some(auth)) = (champ_texte(&cles, "p256dh"), champ_texte(&cles, "auth"))
    else {
        return json_invalide();
    };
    if !webpush::endpoint_accepte(&endpoint)
        || endpoint.len() > 2048
        || p256dh.is_empty()
        || auth.is_empty()
        || p256dh.len() > 256
        || auth.len() > 256
    {
        return abonnement_invalide("abonnement invalide");
    }
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    match enregistrer(pool, &user.id, &endpoint, &p256dh, &auth).await {
        Ok(()) => sans_contenu(),
        Err(e) => bd(e),
    }
}

async fn desabonner(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    supprimer_identifiant(&etat, &user.id, corps, "endpoint", "").await
}

async fn supprimer_identifiant(
    etat: &AppState,
    user: &str,
    corps: Body,
    champ_nom: &str,
    prefixe: &str,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(valeur) = champ_texte(&objet, champ_nom) else {
        return json_invalide();
    };
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    match supprimer(pool, user, &format!("{prefixe}{valeur}")).await {
        Ok(()) => sans_contenu(),
        Err(e) => bd(e),
    }
}

async fn abonner_fcm(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    if !fcm_configure(&etat) {
        return sans_push();
    }
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let (Some(jeton), Some(precedent)) = (
        champ_texte(&objet, "token"),
        champ_texte(&objet, "previous"),
    ) else {
        return json_invalide();
    };
    if jeton.is_empty() || jeton.len() > 512 || precedent.len() > 512 {
        return abonnement_invalide("jeton invalide");
    }
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    if let Err(e) = enregistrer(pool, &user.id, &format!("{PREFIXE_FCM}{jeton}"), "", "").await {
        return bd(e);
    }
    if !precedent.is_empty()
        && precedent != jeton
        && let Err(e) = supprimer(pool, &user.id, &format!("{PREFIXE_FCM}{precedent}")).await
    {
        return bd(e);
    }
    sans_contenu()
}

async fn desabonner_fcm(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    supprimer_identifiant(&etat, &user.id, corps, "token", PREFIXE_FCM).await
}

async fn abonner_pc(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(id) = champ_texte(&objet, "id") else {
        return json_invalide();
    };
    if id.is_empty() || id.len() > 128 {
        return abonnement_invalide("identifiant invalide");
    }
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    match enregistrer(pool, &user.id, &format!("{PREFIXE_PC}{id}"), "", "").await {
        Ok(()) => sans_contenu(),
        Err(e) => bd(e),
    }
}

async fn desabonner_pc(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    supprimer_identifiant(&etat, &user.id, corps, "id", PREFIXE_PC).await
}

async fn essai(State(etat): State<AppState>, ActiveUser(user): ActiveUser) -> Response {
    let Some(vapid) = vapid(&etat) else {
        return sans_push();
    };
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    let abonnements = match appareils(pool, &user.id).await {
        Ok(a) => a,
        Err(e) => return bd(e),
    };
    let n = Notification {
        title: "Estompe".into(),
        body: "Notification de test : tout fonctionne.".into(),
        tag: "test".into(),
        url: "/#/".into(),
        kind: "dm".into(),
        ..Default::default()
    };
    let envoyeur = EnvoyeurWeb(vapid);
    let (mut envoyes, mut derniere) = (0, None);
    for a in abonnements.iter().filter(|a| a.kind == "web") {
        match envoyeur.envoyer(a, &n).await {
            Ok(true) => {
                let _ = supprimer(pool, &user.id, &a.endpoint).await;
            }
            Err(e) => derniere = Some(e),
            Ok(false) => envoyes += 1,
        }
    }
    match (envoyes, derniere) {
        (n, _) if n > 0 => sans_contenu(),
        (_, Some(e)) => write_err(StatusCode::BAD_GATEWAY, "push_failed", &e),
        _ => write_err(
            StatusCode::NOT_FOUND,
            "no_subscription",
            "aucun abonnement push pour ce compte",
        ),
    }
}

async fn lire_reglages(State(etat): State<AppState>, ActiveUser(user): ActiveUser) -> Response {
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    match reglages(pool, &user.id).await {
        Ok(r) => write_json(
            StatusCode::OK,
            &serde_json::to_value(r).unwrap_or(Value::Null),
        ),
        Err(e) => bd(e),
    }
}

async fn ecrire_reglages(
    State(etat): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Ok(nouveaux) = Reglages::depuis_objet(objet) else {
        return json_invalide();
    };
    if nouveaux.levels.len() > NIVEAUX_MAX {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_settings",
            "trop de réglages",
        );
    }
    if nouveaux.ignore_group.len() > NIVEAUX_MAX
        || nouveaux.ignore_group.iter().any(|id| id.len() > 64)
    {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_settings",
            "trop de réglages",
        );
    }
    if nouveaux
        .levels
        .iter()
        .any(|(id, n)| id.len() > 64 || !matches!(n.as_str(), "all" | "mentions" | "none"))
    {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_settings",
            "niveau : all, mentions ou none",
        );
    }
    let Ok(pool) = etat.pool() else {
        return internal();
    };
    if let Err(e) = enregistrer_reglages(pool, &user.id, &nouveaux).await {
        return bd(e);
    }
    etat.publier(
        std::slice::from_ref(&user.id),
        "notify_settings.updated",
        &serde_json::to_value(&nouveaux).unwrap_or(Value::Null),
    );
    sans_contenu()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, header},
    };
    use estompe_core::{
        db::migrate_embarquees,
        identity::Cred,
        testutil::{self, BasePostgres, Faux, faux, faux_aegis, faux_avec_entetes},
    };
    use std::sync::Mutex;
    use tower::ServiceExt;

    struct Env {
        app: Router,
        etat: AppState,
        pool: PgPool,
        ids: HashMap<&'static str, String>,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env(extra: &[(&str, &str)]) -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux_aegis(|_, _| true).await;
        let (pg, url) = testutil::postgres().await?;
        let pool = PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        let mut ids = HashMap::new();
        for nom in ["alice", "bob"] {
            let id: String = sqlx::query_scalar(
                "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
            )
            .bind(format!("sub-{nom}"))
            .bind(nom)
            .fetch_one(&pool)
            .await
            .unwrap();
            ids.insert(nom, id);
        }
        let mut config = vec![
            ("GATEWAY_URL", gateway.url.as_str()),
            ("AEGIS_URL", permission.url.as_str()),
        ];
        config.extend_from_slice(extra);
        let etat = AppState::new(testutil::config_avec(&config), vec![]);
        etat.set_pool(pool.clone());
        let app = Router::from(mount(OpenApiRouter::new())).with_state(etat.clone());
        Some(Env {
            app,
            etat,
            pool,
            ids,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn appel(
        app: &Router,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: &str,
    ) -> (StatusCode, Value) {
        let rep = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(methode)
                    .uri(chemin)
                    .header(header::AUTHORIZATION, format!("Bearer {nom}"))
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let s = rep.status();
        let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (s, serde_json::from_slice(&c).unwrap_or(Value::Null))
    }

    async fn attendre<F: Future<Output = bool>>(mut condition: impl FnMut() -> F) {
        for _ in 0..500 {
            if condition().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition non atteinte");
    }

    async fn nombre_abonnements(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM push_subscriptions")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[derive(Default)]
    struct FauxEnvoyeur {
        recus: Mutex<Vec<(String, Notification)>>,
        expire: Mutex<bool>,
        bloquer: Mutex<Option<Arc<Semaphore>>>,
    }

    impl FauxEnvoyeur {
        fn nombre(&self) -> usize {
            self.recus.lock().unwrap().len()
        }
    }

    impl Envoyeur for Arc<FauxEnvoyeur> {
        fn envoyer<'a>(
            &'a self,
            appareil: &'a Appareil,
            n: &'a Notification,
        ) -> Futur<'a, Result<bool, String>> {
            self.recus
                .lock()
                .unwrap()
                .push((appareil.endpoint.clone(), n.clone()));
            let expire = *self.expire.lock().unwrap();
            let bloquer = self.bloquer.lock().unwrap().clone();
            Box::pin(async move {
                if let Some(b) = bloquer {
                    b.acquire().await.unwrap().forget();
                }
                Ok(expire)
            })
        }
    }

    fn message(id: &str, contenu: &str) -> Value {
        json!({ "id": id, "author": { "id": "auteur", "pseudo": "alice" }, "content": contenu, "attachments": [] })
    }

    fn evenement_mp(dm: &str, id: &str, contenu: &str) -> EvenementMessage {
        EvenementMessage {
            conv: Conv {
                channel_id: None,
                server_id: None,
                dm_id: Some(dm.to_string()),
                kind: "dm".into(),
                slowmode: 0,
                user_limit: 0,
                perms: 0,
            },
            message: message(id, contenu),
            mentionnes: Vec::new(),
            a: Vec::new(),
            ..Default::default()
        }
    }

    async fn creer_mp(e: &Env) -> String {
        let (a, b) = (&e.ids["alice"], &e.ids["bob"]);
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        sqlx::query_scalar(
            "INSERT INTO dm_conversations (user_a, user_b) VALUES ($1::uuid, $2::uuid) RETURNING id::text",
        )
        .bind(a)
        .bind(b)
        .fetch_one(&e.pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn reglages_de_notifications() {
        let Some(e) = env(&[]).await else { return };
        let (s, v) = appel(&e.app, "bob", "GET", "/me/notify-settings", "").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            v,
            json!({ "levels": {}, "types": { "dm": true, "mention": true, "channel": true, "voice": true }, "preview": true, "sound": true })
        );
        for corps in [r#"{"levels":{"x":"loud"}}"#, r#"{"sound":"oui"}"#] {
            assert_eq!(
                appel(&e.app, "bob", "PUT", "/me/notify-settings", corps)
                    .await
                    .0,
                StatusCode::BAD_REQUEST,
                "{corps}"
            );
        }
        let trop: serde_json::Map<String, Value> =
            (0..1001).map(|i| (format!("s{i}"), json!("all"))).collect();
        let (s, v) = appel(
            &e.app,
            "bob",
            "PUT",
            "/me/notify-settings",
            &json!({ "levels": trop }).to_string(),
        )
        .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_settings"))
        );
        let (_conn, mut recu, _) = e.etat.hub().register(
            &e.ids["bob"],
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        let corps = r#"{"levels":{"srv":"none"},"types":{"dm":true,"mention":true,"channel":false,"voice":false},"preview":false,"sound":false}"#;
        assert_eq!(
            appel(&e.app, "bob", "PUT", "/me/notify-settings", corps)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        let evenement: Value = serde_json::from_str(&recu.recv().await.unwrap()).unwrap();
        assert_eq!(evenement["type"], "notify_settings.updated");
        let (_, v) = appel(&e.app, "bob", "GET", "/me/notify-settings", "").await;
        assert_eq!(v["levels"]["srv"], "none");
        assert_eq!(
            (
                v["types"]["channel"].as_bool(),
                v["preview"].as_bool(),
                v["sound"].as_bool()
            ),
            (Some(false), Some(false), Some(false))
        );
        assert_eq!(evenement["data"], v);
        let (_, a) = appel(&e.app, "alice", "GET", "/me/notify-settings", "").await;
        assert_eq!(a["preview"], true);
        assert_eq!(a["levels"], json!({}));
    }

    #[tokio::test]
    async fn abonnements_web_push_fcm_et_pc() {
        let Some(e) = env(&[
            ("VAPID_PUBLIC_KEY", "pub"),
            ("VAPID_PRIVATE_KEY", "priv"),
            ("FCM_SERVICE_ACCOUNT", "x"),
        ])
        .await
        else {
            return;
        };
        let (s, v) = appel(&e.app, "bob", "GET", "/push/key", "").await;
        assert_eq!((s, v["public_key"].as_str()), (StatusCode::OK, Some("pub")));
        let put = |corps: &'static str| appel(&e.app, "bob", "PUT", "/push/subscription", corps);
        for corps in [
            r#"{"endpoint":"http://x/insecure","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://push.example/1","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://169.254.169.254/latest","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://10.0.0.5/x","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://[::1]/x","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://estompe-api:8080/x","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://fcm.googleapis.com.evil.example/x","keys":{"p256dh":"a","auth":"b"}}"#,
            r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/1","keys":{"p256dh":"","auth":"b"}}"#,
            r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/1"}"#,
        ] {
            let (s, v) = put(corps).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_subscription")),
                "{corps}"
            );
        }
        assert_eq!(
            put(r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/1","keys":{"p256dh":"a","auth":"b"}}"#)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        let appareils_bob = || appareils(&e.pool, &e.ids["bob"]);
        let liste = appareils_bob().await.unwrap();
        assert_eq!((liste.len(), liste[0].kind.as_str()), (1, "web"));
        assert_eq!(
            appel(
                &e.app,
                "alice",
                "PUT",
                "/push/subscription",
                r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/1","keys":{"p256dh":"c","auth":"d"}}"#
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert!(appareils_bob().await.unwrap().is_empty());
        assert_eq!(
            appareils(&e.pool, &e.ids["alice"]).await.unwrap()[0].p256dh,
            "c"
        );

        let (s, _) = appel(&e.app, "bob", "PUT", "/push/fcm", r#"{"token":""}"#).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        appel(&e.app, "bob", "PUT", "/push/fcm", r#"{"token":"old"}"#).await;
        appel(
            &e.app,
            "bob",
            "PUT",
            "/push/fcm",
            r#"{"token":"new","previous":"old"}"#,
        )
        .await;
        assert_eq!(
            appel(&e.app, "bob", "PUT", "/push/desktop", r#"{"id":""}"#)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        appel(&e.app, "bob", "PUT", "/push/desktop", r#"{"id":"pc1"}"#).await;
        let mut genres: Vec<_> = appareils_bob()
            .await
            .unwrap()
            .into_iter()
            .map(|a| (a.endpoint, a.kind))
            .collect();
        genres.sort();
        assert_eq!(
            genres,
            [
                ("desktop:pc1".to_string(), "desktop".to_string()),
                ("fcm:new".to_string(), "fcm".to_string())
            ]
        );
        for (chemin, corps) in [
            ("/push/desktop", r#"{"id":"pc1"}"#),
            ("/push/fcm", r#"{"token":"new"}"#),
        ] {
            assert_eq!(
                appel(&e.app, "bob", "DELETE", chemin, corps).await.0,
                StatusCode::NO_CONTENT
            );
        }
        assert!(appareils_bob().await.unwrap().is_empty());
        assert_eq!(
            appel(
                &e.app,
                "alice",
                "DELETE",
                "/push/subscription",
                r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/1"}"#
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(nombre_abonnements(&e.pool).await, 0);
    }

    #[tokio::test]
    async fn push_indisponible_sans_configuration() {
        let Some(e) = env(&[]).await else { return };
        for (methode, chemin, corps) in [
            ("GET", "/push/key", ""),
            ("PUT", "/push/subscription", "{}"),
            ("POST", "/push/test", ""),
            ("PUT", "/push/fcm", r#"{"token":"x"}"#),
        ] {
            let (s, v) = appel(&e.app, "bob", methode, chemin, corps).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::NOT_IMPLEMENTED, Some("no_push")),
                "{chemin}"
            );
        }
        assert_eq!(
            appel(&e.app, "bob", "PUT", "/push/desktop", r#"{"id":"pc"}"#)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn notification_de_test() {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &rng,
        )
        .unwrap();
        let paire = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8.as_ref(),
            &rng,
        )
        .unwrap();
        let fin = pkcs8.as_ref().len() - 65 - 5;
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use ring::signature::KeyPair;
        let privee = URL_SAFE_NO_PAD.encode(&pkcs8.as_ref()[fin - 32..fin]);
        let publique = URL_SAFE_NO_PAD.encode(paire.public_key().as_ref());
        let rng2 = ring::rand::SystemRandom::new();
        let recepteur =
            ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::ECDH_P256, &rng2)
                .unwrap();
        let p256dh = URL_SAFE_NO_PAD.encode(recepteur.compute_public_key().unwrap().as_ref());
        let auth = URL_SAFE_NO_PAD.encode([3u8; 16]);

        let Some(e) = env(&[
            ("VAPID_PUBLIC_KEY", publique.as_str()),
            ("VAPID_PRIVATE_KEY", privee.as_str()),
        ])
        .await
        else {
            return;
        };
        let statut = Arc::new(std::sync::atomic::AtomicU16::new(201));
        let s = statut.clone();
        let push = faux_avec_entetes(move |_| {
            (
                s.load(std::sync::atomic::Ordering::SeqCst),
                vec![],
                b"refus".to_vec(),
            )
        })
        .await;
        let bob = e.ids["bob"].clone();
        let (s, _) = appel(&e.app, "bob", "POST", "/push/test", "").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        enregistrer(&e.pool, &bob, &push.url, &p256dh, &auth)
            .await
            .unwrap();
        assert_eq!(
            appel(&e.app, "bob", "POST", "/push/test", "").await.0,
            StatusCode::NO_CONTENT
        );
        let requetes = push.requetes();
        assert_eq!(requetes.len(), 1);
        assert_eq!(requetes[0].entete("content-encoding"), "aes128gcm");
        assert_eq!(requetes[0].entete("urgency"), "high");
        statut.store(403, std::sync::atomic::Ordering::SeqCst);
        let (s, v) = appel(&e.app, "bob", "POST", "/push/test", "").await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_GATEWAY, Some("push_failed"))
        );
        assert!(v["message"].as_str().unwrap().contains("403"));
        statut.store(410, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            appel(&e.app, "bob", "POST", "/push/test", "").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(nombre_abonnements(&e.pool).await, 0);
        assert_eq!(
            appel(&e.app, "bob", "POST", "/push/test", "").await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn service_decide_et_envoie() {
        let Some(e) = env(&[]).await else { return };
        let (bob, alice) = (e.ids["bob"].clone(), e.ids["alice"].clone());
        let dm = creer_mp(&e).await;
        let faux_fcm = Arc::new(FauxEnvoyeur::default());
        let mut envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>> = HashMap::new();
        envoyeurs.insert("fcm", Arc::new(faux_fcm.clone()));
        installer_avec(&e.etat, envoyeurs);
        enregistrer(&e.pool, &bob, "fcm:tok1", "", "")
            .await
            .unwrap();
        enregistrer(&e.pool, &bob, "desktop:pc", "", "")
            .await
            .unwrap();
        enregistrer(&e.pool, &alice, "fcm:atok", "", "")
            .await
            .unwrap();
        let etiquettes = |endpoint: &str| -> Vec<String> {
            faux_fcm
                .recus
                .lock()
                .unwrap()
                .iter()
                .filter(|(e, _)| e == endpoint)
                .map(|(_, n)| n.tag.clone())
                .collect()
        };

        e.etat.notifier(&evenement_mp(&dm, "7", "psst")).await;
        attendre(|| async { etiquettes("fcm:tok1") == ["msg-7"] }).await;
        {
            let recus = faux_fcm.recus.lock().unwrap();
            let n = &recus.iter().find(|(e, _)| e == "fcm:tok1").unwrap().1;
            assert_eq!(
                (
                    n.title.as_str(),
                    n.body.as_str(),
                    n.url.as_str(),
                    n.conv.as_str()
                ),
                (
                    "alice",
                    "psst",
                    &format!("/#/dm/{dm}/7")[..],
                    &format!("d:{dm}")[..]
                )
            );
        }

        sqlx::query("UPDATE users SET status='dnd' WHERE id=$1::uuid")
            .bind(&bob)
            .execute(&e.pool)
            .await
            .unwrap();
        e.etat
            .notifier(&evenement_mp(&dm, "8", "ne pas déranger"))
            .await;
        e.etat.notifier(&evenement_mp(&dm, "9", "sentinelle")).await;
        attendre(|| async { etiquettes("fcm:atok").contains(&"msg-9".to_string()) }).await;
        assert_eq!(etiquettes("fcm:tok1"), ["msg-7"]);

        sqlx::query("UPDATE users SET status='online', notify_settings='{\"types\":{\"dm\":false}}' WHERE id=$1::uuid")
            .bind(&bob)
            .execute(&e.pool)
            .await
            .unwrap();
        e.etat
            .notifier(&evenement_mp(&dm, "10", "type coupé"))
            .await;
        let mut mention = evenement_mp(&dm, "11", "@bob");
        mention.mentionnes = vec![bob.clone()];
        e.etat.notifier(&mention).await;
        attendre(|| async { etiquettes("fcm:tok1").len() == 2 }).await;
        assert_eq!(etiquettes("fcm:tok1"), ["msg-7", "msg-11"]);
    }

    #[tokio::test]
    async fn service_notifie_en_direct_ou_retire_l_appareil_expire() {
        let Some(e) = env(&[]).await else { return };
        let bob = e.ids["bob"].clone();
        let dm = creer_mp(&e).await;
        let faux_fcm = Arc::new(FauxEnvoyeur::default());
        let mut envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>> = HashMap::new();
        envoyeurs.insert("fcm", Arc::new(faux_fcm.clone()));
        installer_avec(&e.etat, envoyeurs);
        enregistrer(&e.pool, &bob, "fcm:tok1", "", "")
            .await
            .unwrap();

        let (conn, mut recu, _) = e.etat.hub().register(
            &bob,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        e.etat.notifier(&evenement_mp(&dm, "1", "en direct")).await;
        let direct: Value = serde_json::from_str(&recu.recv().await.unwrap()).unwrap();
        assert_eq!(direct["type"], "notification");
        assert_eq!(direct["data"]["tag"], "msg-1");
        e.etat.hub().unregister(&conn);

        *faux_fcm.expire.lock().unwrap() = true;
        e.etat.notifier(&evenement_mp(&dm, "2", "expiré")).await;
        attendre(|| async { nombre_abonnements(&e.pool).await == 0 }).await;
        assert_eq!(faux_fcm.nombre(), 1);
        assert_eq!(faux_fcm.recus.lock().unwrap()[0].1.tag, "msg-2");
    }

    #[tokio::test]
    async fn lecture_envoyee_aux_appareils_android() {
        let Some(e) = env(&[]).await else { return };
        let bob = e.ids["bob"].clone();
        let faux_fcm = Arc::new(FauxEnvoyeur::default());
        let mut envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>> = HashMap::new();
        envoyeurs.insert("fcm", Arc::new(faux_fcm.clone()));
        installer_avec(&e.etat, envoyeurs);
        enregistrer(&e.pool, &bob, "fcm:tok1", "", "")
            .await
            .unwrap();
        enregistrer(&e.pool, &bob, "desktop:pc", "", "")
            .await
            .unwrap();
        e.etat.retirer_notification(&bob, "c:abc", "42").await;
        attendre(|| async { faux_fcm.nombre() == 1 }).await;
        let recus = faux_fcm.recus.lock().unwrap();
        assert_eq!(recus[0].0, "fcm:tok1");
        let charge = serde_json::to_value(&recus[0].1).unwrap();
        assert_eq!(charge["conv"], "c:abc");
        assert_eq!(charge["read"], "42");
        assert_eq!(charge["tag"], "read-c:abc");
    }

    #[tokio::test]
    async fn plafond_remplace_le_plus_ancien_abonnement() {
        let Some(e) = env(&[]).await else { return };
        let (bob, alice) = (e.ids["bob"].clone(), e.ids["alice"].clone());
        let ep = |i: i64| format!("fcm:{i:02}");
        enregistrer(&e.pool, &alice, "fcm:alice", "", "")
            .await
            .unwrap();
        for i in 0..=ABONNEMENTS_MAX {
            enregistrer(&e.pool, &bob, &ep(i), "", "").await.unwrap();
        }
        let pool = &e.pool;
        let liste = |u: String| async move {
            appareils(pool, &u)
                .await
                .unwrap()
                .into_iter()
                .map(|a| a.endpoint)
                .collect::<Vec<_>>()
        };
        let restants = liste(bob.clone()).await;
        assert_eq!(restants.len() as i64, ABONNEMENTS_MAX);
        assert!(!restants.contains(&ep(0)), "{restants:?}");
        enregistrer(&e.pool, &bob, &ep(1), "", "").await.unwrap();
        enregistrer(&e.pool, &bob, &ep(20), "", "").await.unwrap();
        let restants = liste(bob.clone()).await;
        assert_eq!(restants.len() as i64, ABONNEMENTS_MAX);
        assert!(restants.contains(&ep(1)) && restants.contains(&ep(20)));
        assert!(!restants.contains(&ep(2)), "{restants:?}");
        assert_eq!(liste(alice).await, ["fcm:alice"]);
    }

    #[tokio::test]
    async fn envois_en_tache_de_fond_et_bornes() {
        let Some(e) = env(&[]).await else { return };
        let (bob, alice) = (e.ids["bob"].clone(), e.ids["alice"].clone());
        let dm = creer_mp(&e).await;
        let verrou = Arc::new(Semaphore::new(0));
        let faux_fcm = Arc::new(FauxEnvoyeur::default());
        *faux_fcm.bloquer.lock().unwrap() = Some(verrou.clone());
        let mut envoyeurs: HashMap<&'static str, Arc<dyn Envoyeur>> = HashMap::new();
        envoyeurs.insert("fcm", Arc::new(faux_fcm.clone()));
        installer_avec(&e.etat, envoyeurs);
        for i in 0..ABONNEMENTS_MAX {
            enregistrer(&e.pool, &bob, &format!("fcm:{i}"), "", "")
                .await
                .unwrap();
        }
        e.etat.notifier(&evenement_mp(&dm, "1", "bloqué")).await;
        attendre(|| async { faux_fcm.nombre() >= ENVOIS_SIMULTANES }).await;
        assert_eq!(faux_fcm.nombre(), ENVOIS_SIMULTANES);

        let (_conn, mut recu, _) = e.etat.hub().register(
            &alice,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        e.etat.notifier(&evenement_mp(&dm, "2", "en direct")).await;
        let direct = tokio::time::timeout(Duration::from_secs(5), recu.recv())
            .await
            .expect("file de notifications bloquée par les envois")
            .unwrap();
        let direct: Value = serde_json::from_str(&direct).unwrap();
        assert_eq!(direct["data"]["tag"], "msg-2");

        verrou.add_permits(1);
        attendre(|| async { faux_fcm.nombre() > ENVOIS_SIMULTANES }).await;
        assert_eq!(faux_fcm.nombre(), ENVOIS_SIMULTANES + 1);
    }
}
