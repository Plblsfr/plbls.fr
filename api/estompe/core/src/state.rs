use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;

use crate::{
    catalogue,
    config::Config,
    http::text_error,
    hub::Hub,
    identity::{Identity, Resolver},
    messages::{Conv, EvenementMessage},
    metrics::Metrics,
    permission::{ADMIN_PERMISSION, Client, Droits, Indisponible, Profil},
    ratelimit::Limiteurs,
    roles::{self, Access, Perm},
    updates::{self, Policy},
};

pub const WS_RECHECK: Duration = Duration::from_secs(2 * 60);
pub const WS_PERM_GRACE: Duration = Duration::from_secs(10 * 60);
pub const WS_IDLE: Duration = Duration::from_secs(75);

#[derive(Clone, Copy)]
pub struct DureesWs {
    pub recheck: Duration,
    pub perm_grace: Duration,
    pub idle: Duration,
}

impl Default for DureesWs {
    fn default() -> Self {
        DureesWs {
            recheck: WS_RECHECK,
            perm_grace: WS_PERM_GRACE,
            idle: WS_IDLE,
        }
    }
}

pub type Horloge = Arc<dyn Fn() -> Instant + Send + Sync>;

pub type Callback =
    Arc<dyn Fn(AppState, String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub type RappelDroits =
    Arc<dyn Fn(AppState, String, String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub type RappelSortieVocal = Arc<
    dyn Fn(AppState, String, String, String) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

#[derive(Debug, PartialEq, Eq)]
pub struct VocalInjoignable;

pub type ParticipantsVocal = Result<Vec<serde_json::Value>, VocalInjoignable>;

pub trait Vocal: Send + Sync {
    fn sortir_du_serveur<'a>(
        &'a self,
        serveur: &'a str,
        membre: &'a str,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

    fn participants<'a>(
        &'a self,
        salon: &'a str,
    ) -> Pin<Box<dyn Future<Output = ParticipantsVocal> + Send + 'a>>;
}

pub trait PostVideo: Send + Sync {
    fn apres_message<'a>(
        &'a self,
        conv: &'a Conv,
        message: &'a serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilmResolu {
    pub id: String,
    pub titre: String,
    pub duree: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusFilm {
    Permission,
    Introuvable,
    Indisponible,
}

pub type FilmEtSuite = Result<(FilmResolu, Vec<FilmResolu>), RefusFilm>;

pub trait Films: Send + Sync {
    fn resoudre<'a>(
        &'a self,
        sub: &'a str,
        id: &'a str,
    ) -> Pin<Box<dyn Future<Output = FilmEtSuite> + Send + 'a>>;
}

pub trait Notifieur: Send + Sync {
    fn publier<'a>(
        &'a self,
        evenement: &'a EvenementMessage,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

    fn retirer<'a>(
        &'a self,
        _user: &'a str,
        _conv: &'a str,
        _last: &'a str,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

pub struct DbIndisponible;

impl IntoResponse for DbIndisponible {
    fn into_response(self) -> Response {
        text_error(StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
    }
}

struct Inner {
    config: Config,
    pool: OnceLock<PgPool>,
    identity: Resolver,
    permission: Client,
    blocs_actifs: Vec<String>,
    metrics: Metrics,
    limiteurs: Limiteurs,
    politique: Policy,
    demarrage: Instant,
    hub: Hub,
    ws: DureesWs,
    acces_retire: Mutex<Vec<Callback>>,
    droits_modifies: Mutex<Vec<RappelDroits>>,
    sorties_vocal: Mutex<Vec<RappelSortieVocal>>,
    vocal: Mutex<Option<Arc<dyn Vocal>>>,
    post_video: Mutex<Option<Arc<dyn PostVideo>>>,
    films: Mutex<Option<Arc<dyn Films>>>,
    notifieur: Mutex<Option<Arc<dyn Notifieur>>>,
    cle_liens: [u8; 32],
    stockage: tokio::sync::Mutex<()>,
    envois_video: AtomicI64,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub fn new(config: Config, blocs_actifs: Vec<String>) -> Self {
        Self::new_avec_politique(config, blocs_actifs, updates::charger())
    }

    pub(crate) fn new_avec_politique(
        config: Config,
        blocs_actifs: Vec<String>,
        politique: Policy,
    ) -> Self {
        Self::new_avec_ws(
            config,
            blocs_actifs,
            politique,
            DureesWs::default(),
            Arc::new(Instant::now),
        )
    }

    pub(crate) fn new_avec_ws(
        config: Config,
        blocs_actifs: Vec<String>,
        politique: Policy,
        ws: DureesWs,
        now: Horloge,
    ) -> Self {
        AppState(Arc::new(Inner {
            identity: Resolver::with_now(&config.gateway_url, now.clone()),
            permission: if config.aegis.actif() {
                Client::aegis(&config.aegis.url, &config.aegis.token, now.clone())
            } else {
                Client::permission(
                    &config.permission_url,
                    &config.permission_token,
                    &config.permission_name,
                    now.clone(),
                )
            },
            pool: OnceLock::new(),
            blocs_actifs,
            metrics: Metrics::default(),
            limiteurs: Limiteurs::default(),
            politique,
            demarrage: Instant::now(),
            hub: Hub::with_now(now),
            ws,
            acces_retire: Mutex::default(),
            droits_modifies: Mutex::default(),
            sorties_vocal: Mutex::default(),
            vocal: Mutex::default(),
            post_video: Mutex::default(),
            films: Mutex::default(),
            notifieur: Mutex::default(),
            cle_liens: rand::random(),
            stockage: tokio::sync::Mutex::new(()),
            envois_video: AtomicI64::new(0),
            config,
        }))
    }

    pub fn config(&self) -> &Config {
        &self.0.config
    }

    pub fn identity(&self) -> &Resolver {
        &self.0.identity
    }

    pub fn permissions(&self) -> &Client {
        &self.0.permission
    }

    pub async fn droits(&self, sub: &str) -> Result<Droits, Indisponible> {
        self.0.permission.droits(sub).await
    }

    pub async fn droits_de(&self, idn: &Identity) -> Result<Droits, Indisponible> {
        self.0
            .permission
            .droits_de(
                &idn.sub,
                Some(Profil {
                    email: &idn.email,
                    nom: &idn.username,
                }),
            )
            .await
    }

    pub async fn droit(&self, sub: &str, nom: &str) -> Result<bool, Indisponible> {
        Ok(self.droits(sub).await?.a(nom))
    }

    pub async fn acces(&self, sub: &str) -> Result<bool, Indisponible> {
        self.droit(sub, catalogue::ACCES).await
    }

    pub fn role_admin_permissions(&self) -> &str {
        if self.0.permission.aegis_actif() {
            &self.0.config.aegis.admin_role
        } else {
            &self.0.config.permission_admin_role
        }
    }

    pub async fn gere_permissions(&self, idn: &Identity) -> Result<bool, Indisponible> {
        if idn.roles.iter().any(|r| r == self.role_admin_permissions()) {
            return Ok(true);
        }
        let nom = if self.0.permission.aegis_actif() {
            catalogue::ADMIN_AEGIS
        } else {
            ADMIN_PERMISSION
        };
        Ok(self.droits_de(idn).await?.a(nom))
    }

    pub async fn acces_serveur(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> Result<Option<Access>, sqlx::Error> {
        let pool = self.pool().map_err(|_| sqlx::Error::PoolClosed)?;
        let Some(mut a) = roles::access_for(pool, server_id, user_id).await? else {
            return Ok(None);
        };
        a.retire = self.plafond(&a.sub, server_id).await;
        Ok(Some(a))
    }

    pub async fn plafond(&self, sub: &str, server_id: &str) -> Perm {
        let droits = self.droits(sub).await;
        catalogue::plafonnees()
            .filter(|e| match &droits {
                Ok(d) => !d.a_sur(&e.nom(), server_id),
                Err(_) => true,
            })
            .fold(0, |p, e| p | e.plafond)
    }

    pub fn blocs_actifs(&self) -> &[String] {
        &self.0.blocs_actifs
    }

    pub fn metrics(&self) -> &Metrics {
        &self.0.metrics
    }

    pub fn limiteurs(&self) -> &Limiteurs {
        &self.0.limiteurs
    }

    pub fn politique(&self) -> &Policy {
        &self.0.politique
    }

    pub fn hub(&self) -> &Hub {
        &self.0.hub
    }

    pub fn durees_ws(&self) -> DureesWs {
        self.0.ws
    }

    pub fn publier(&self, to: &[String], typ: &str, data: &serde_json::Value) {
        self.0.hub.publish(to, typ, data);
    }

    pub fn sur_acces_retire<F, Fut>(&self, rappel: F)
    where
        F: Fn(AppState, String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.0
            .acces_retire
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::new(move |s, id| Box::pin(rappel(s, id))));
    }

    pub async fn acces_retire(&self, user_id: &str) {
        let rappels = self
            .0
            .acces_retire
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for r in rappels {
            r(self.clone(), user_id.to_string()).await;
        }
    }

    pub fn sur_droits_modifies<F, Fut>(&self, rappel: F)
    where
        F: Fn(AppState, String, String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.0
            .droits_modifies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::new(move |s, serveur, membre| {
                Box::pin(rappel(s, serveur, membre))
            }));
    }

    pub async fn droits_modifies(&self, serveur: &str, membre: &str) {
        let rappels = self
            .0
            .droits_modifies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for r in rappels {
            r(self.clone(), serveur.to_string(), membre.to_string()).await;
        }
    }

    pub fn sur_sortie_salon_vocal<F, Fut>(&self, rappel: F)
    where
        F: Fn(AppState, String, String, String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.0
            .sorties_vocal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::new(move |s, serveur, salon, membre| {
                Box::pin(rappel(s, serveur, salon, membre))
            }));
    }

    pub async fn sortie_salon_vocal(&self, serveur: &str, salon: &str, membre: &str) {
        let rappels = self
            .0
            .sorties_vocal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for r in rappels {
            r(
                self.clone(),
                serveur.to_string(),
                salon.to_string(),
                membre.to_string(),
            )
            .await;
        }
    }

    pub fn definir_vocal(&self, vocal: Arc<dyn Vocal>) {
        *self.0.vocal.lock().unwrap_or_else(|e| e.into_inner()) = Some(vocal);
    }

    pub fn vocal_branche(&self) -> bool {
        self.0
            .vocal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    pub async fn sortir_du_vocal(&self, serveur: &str, membre: &str) {
        let vocal = self
            .0
            .vocal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(v) = vocal {
            v.sortir_du_serveur(serveur, membre).await;
        }
    }

    pub async fn participants_vocal(&self, salon: &str) -> Option<ParticipantsVocal> {
        let vocal = self
            .0
            .vocal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        Some(vocal.participants(salon).await)
    }

    pub fn definir_post_video(&self, hook: Arc<dyn PostVideo>) {
        *self.0.post_video.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    pub async fn apres_message_video(&self, conv: &Conv, message: &serde_json::Value) {
        let hook = self
            .0
            .post_video
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(h) = hook {
            h.apres_message(conv, message).await;
        }
    }

    pub fn definir_films(&self, films: Arc<dyn Films>) {
        *self.0.films.lock().unwrap_or_else(|e| e.into_inner()) = Some(films);
    }

    pub fn films(&self) -> Option<Arc<dyn Films>> {
        self.0
            .films
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn definir_notifieur(&self, notifieur: Arc<dyn Notifieur>) {
        *self.0.notifieur.lock().unwrap_or_else(|e| e.into_inner()) = Some(notifieur);
    }

    pub async fn notifier(&self, evenement: &EvenementMessage) {
        let notifieur = self
            .0
            .notifieur
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(n) = notifieur {
            n.publier(evenement).await;
        }
    }

    pub async fn retirer_notification(&self, user: &str, conv: &str, last: &str) {
        let notifieur = self
            .0
            .notifieur
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(n) = notifieur {
            n.retirer(user, conv, last).await;
        }
    }

    pub fn cle_liens(&self) -> &[u8; 32] {
        &self.0.cle_liens
    }

    pub async fn verrou_stockage(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.0.stockage.lock().await
    }

    pub fn reserver_envoi_video(&self, max: i64) -> bool {
        if self.0.envois_video.fetch_add(1, Ordering::SeqCst) + 1 > max {
            self.0.envois_video.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        true
    }

    pub fn liberer_envoi_video(&self) {
        self.0.envois_video.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn uptime(&self) -> u64 {
        self.0.demarrage.elapsed().as_secs()
    }

    pub fn set_pool(&self, pool: PgPool) {
        let _ = self.0.pool.set(pool);
    }

    pub fn pool(&self) -> Result<&PgPool, DbIndisponible> {
        self.0.pool.get().ok_or(DbIndisponible)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil;

    #[tokio::test]
    async fn rappels_de_droits_dans_l_ordre() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        etat.droits_modifies("s", "m").await;
        let notes = Arc::new(Mutex::new(Vec::<String>::new()));
        for nom in ["premier", "second"] {
            let notes = notes.clone();
            etat.sur_droits_modifies(move |_, serveur, membre| {
                let notes = notes.clone();
                async move {
                    notes
                        .lock()
                        .unwrap()
                        .push(format!("{nom}:{serveur}:{membre}"))
                }
            });
        }
        etat.droits_modifies("s", "m").await;
        etat.droits_modifies("s", "").await;
        assert_eq!(
            *notes.lock().unwrap(),
            ["premier:s:m", "second:s:m", "premier:s:", "second:s:"]
        );
    }

    #[tokio::test]
    async fn rappels_de_sortie_du_salon_vocal() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        etat.sortie_salon_vocal("s", "c", "m").await;
        let notes = Arc::new(Mutex::new(Vec::<String>::new()));
        for nom in ["premier", "second"] {
            let notes = notes.clone();
            etat.sur_sortie_salon_vocal(move |_, serveur, salon, membre| {
                let notes = notes.clone();
                async move {
                    notes
                        .lock()
                        .unwrap()
                        .push(format!("{nom}:{serveur}:{salon}:{membre}"))
                }
            });
        }
        etat.sortie_salon_vocal("s", "c", "m").await;
        assert_eq!(*notes.lock().unwrap(), ["premier:s:c:m", "second:s:c:m"]);
    }

    struct Faux(Mutex<Vec<(String, String)>>);

    impl Vocal for Faux {
        fn sortir_du_serveur<'a>(
            &'a self,
            serveur: &'a str,
            membre: &'a str,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.0
                    .lock()
                    .unwrap()
                    .push((serveur.to_string(), membre.to_string()));
            })
        }

        fn participants<'a>(
            &'a self,
            salon: &'a str,
        ) -> Pin<Box<dyn Future<Output = ParticipantsVocal> + Send + 'a>> {
            Box::pin(async move {
                match salon {
                    "plein" => Ok(vec![serde_json::json!({ "user_id": "u", "pseudo": "p" })]),
                    "panne" => Err(VocalInjoignable),
                    _ => Ok(Vec::new()),
                }
            })
        }
    }

    #[tokio::test]
    async fn vocal_facultatif() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        etat.sortir_du_vocal("s", "m").await;
        assert!(!etat.vocal_branche());
        let faux = Arc::new(Faux(Mutex::default()));
        etat.definir_vocal(faux.clone());
        assert!(etat.vocal_branche());
        etat.sortir_du_vocal("s", "m").await;
        assert_eq!(
            *faux.0.lock().unwrap(),
            [("s".to_string(), "m".to_string())]
        );
    }

    #[tokio::test]
    async fn participants_vocal_facultatifs() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        assert_eq!(etat.participants_vocal("plein").await, None);
        etat.definir_vocal(Arc::new(Faux(Mutex::default())));
        assert_eq!(etat.participants_vocal("vide").await, Some(Ok(vec![])));
        assert_eq!(
            etat.participants_vocal("plein").await,
            Some(Ok(vec![
                serde_json::json!({ "user_id": "u", "pseudo": "p" })
            ]))
        );
        assert_eq!(
            etat.participants_vocal("panne").await,
            Some(Err(VocalInjoignable))
        );
    }

    pub(crate) struct Enregistreur(pub(crate) Mutex<Vec<String>>);

    impl PostVideo for Enregistreur {
        fn apres_message<'a>(
            &'a self,
            conv: &'a Conv,
            message: &'a serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.0
                    .lock()
                    .unwrap()
                    .push(format!("{}:{}", conv.kind, message["content"]));
            })
        }
    }

    impl Notifieur for Enregistreur {
        fn publier<'a>(
            &'a self,
            evenement: &'a EvenementMessage,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                let mut tous = evenement.mentionnes.clone();
                tous.sort();
                self.0.lock().unwrap().push(format!(
                    "{}|{}|{}",
                    evenement.message["content"].as_str().unwrap_or_default(),
                    tous.len(),
                    evenement.a.len()
                ));
            })
        }

        fn retirer<'a>(
            &'a self,
            user: &'a str,
            conv: &'a str,
            last: &'a str,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.0
                    .lock()
                    .unwrap()
                    .push(format!("retirer:{user}:{conv}:{last}"));
            })
        }
    }

    fn conv_videos() -> Conv {
        Conv {
            channel_id: Some("c".into()),
            server_id: Some("s".into()),
            dm_id: None,
            kind: "videos".into(),
            slowmode: 0,
            user_limit: 0,
            perms: 0,
        }
    }

    #[tokio::test]
    async fn post_video_facultatif() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        let message = serde_json::json!({ "content": "7" });
        etat.apres_message_video(&conv_videos(), &message).await;
        let faux = Arc::new(Enregistreur(Mutex::default()));
        etat.definir_post_video(faux.clone());
        etat.apres_message_video(&conv_videos(), &message).await;
        assert_eq!(*faux.0.lock().unwrap(), ["videos:\"7\""]);
    }

    #[tokio::test]
    async fn notifieur_facultatif() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        let evenement = EvenementMessage {
            conv: conv_videos(),
            message: serde_json::json!({}),
            mentionnes: vec!["a".into(), "b".into()],
            a: vec![],
            ..Default::default()
        };
        etat.notifier(&evenement).await;
        let faux = Arc::new(Enregistreur(Mutex::default()));
        etat.definir_notifieur(faux.clone());
        etat.notifier(&evenement).await;
        assert_eq!(*faux.0.lock().unwrap(), ["|2|0"]);
    }

    #[tokio::test]
    async fn retirer_facultatif() {
        let etat = AppState::new(testutil::config_avec(&[]), vec![]);
        etat.retirer_notification("u", "c:1", "9").await;
        let faux = Arc::new(Enregistreur(Mutex::default()));
        etat.definir_notifieur(faux.clone());
        etat.retirer_notification("u", "c:1", "9").await;
        assert_eq!(*faux.0.lock().unwrap(), ["retirer:u:c:1:9"]);
    }
}
