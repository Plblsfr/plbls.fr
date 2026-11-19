use std::collections::HashMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

use estompe_plateforme::{Coffre, Erreur, OuvertureUrl, Resultat};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{AbortHandle, JoinSet};

use crate::config::{
    emetteur_keycloak, valider_adresse, CLIENT_KEYCLOAK, EMETTEUR_KEYCLOAK, PORTEES_OIDC,
};
use crate::journal::{Journal, Niveau};

pub const CLE_COFFRE: &str = "refresh-token";
pub const DELAI_NAVIGATEUR: Duration = Duration::from_secs(5 * 60);
pub const MARGE_RENOUVELLEMENT: Duration = Duration::from_secs(90);
pub const DELAI_REQUETE: Duration = Duration::from_secs(30);
pub const CHEMIN_REDIRECTION: &str = "/callback";

const DELAI_LECTURE: Duration = Duration::from_secs(10);
const TETE_MAX: usize = 16 * 1024;
const CONNEXIONS_MAX: usize = 16;
const REPRISE_MIN: Duration = Duration::from_secs(5);
const REPRISE_MAX: Duration = Duration::from_secs(60);
const SOURCE_JOURNAL: &str = "connexion";
const PAGE_REUSSIE: &str = "Connexion réussie. Tu peux fermer cet onglet et revenir à Estompe.";
const PAGE_INTERROMPUE: &str =
    "Connexion interrompue. Tu peux fermer cet onglet et revenir à Estompe.";
const PAGE_INATTENDUE: &str = "Réponse de connexion inattendue.";
const PAGE_INTROUVABLE: &str = "Introuvable.";
const ALPHABET_BASE64URL: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAuth {
    pub emetteur: String,
    pub client_id: String,
    pub portees: String,
    pub delai_navigateur: Duration,
    pub marge_renouvellement: Duration,
    pub delai_requete: Duration,
    pub mandataire_systeme: bool,
}

impl Default for ConfigAuth {
    fn default() -> Self {
        Self {
            emetteur: EMETTEUR_KEYCLOAK.to_string(),
            client_id: CLIENT_KEYCLOAK.to_string(),
            portees: PORTEES_OIDC.to_string(),
            delai_navigateur: DELAI_NAVIGATEUR,
            marge_renouvellement: MARGE_RENOUVELLEMENT,
            delai_requete: DELAI_REQUETE,
            mandataire_systeme: true,
        }
    }
}

impl ConfigAuth {
    pub fn depuis_environnement() -> Self {
        Self {
            emetteur: emetteur_keycloak(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parcours {
    Connexion,
    Inscription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurAuth {
    Annulee,
    NavigateurIndisponible(String),
    DelaiDepasse,
    Refusee { code: String, message: String },
    SessionExpiree,
    Reseau(String),
    Serveur(u16),
    ReponseInvalide(String),
    Systeme(String),
}

impl ErreurAuth {
    pub fn exige_connexion(&self) -> bool {
        matches!(self, Self::SessionExpiree)
    }

    pub fn est_passagere(&self) -> bool {
        matches!(self, Self::Reseau(_) | Self::Serveur(_))
    }
}

impl fmt::Display for ErreurAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Annulee => write!(f, "connexion annulée"),
            Self::NavigateurIndisponible(detail) => {
                write!(f, "impossible d'ouvrir le navigateur : {detail}")
            }
            Self::DelaiDepasse => write!(f, "délai de connexion dépassé"),
            Self::Refusee { code, message } => write!(f, "connexion refusée ({code}) : {message}"),
            Self::SessionExpiree => write!(f, "session expirée : reconnecte-toi"),
            Self::Reseau(detail) => write!(f, "Keycloak injoignable : {detail}"),
            Self::Serveur(statut) => write!(f, "Keycloak répond {statut}"),
            Self::ReponseInvalide(detail) => write!(f, "réponse de Keycloak illisible : {detail}"),
            Self::Systeme(detail) => write!(f, "connexion impossible : {detail}"),
        }
    }
}

impl std::error::Error for ErreurAuth {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EtatSession {
    Deconnecte,
    AttenteNavigateur,
    Connecte,
    Erreur(ErreurAuth),
}

#[derive(Deserialize)]
struct ReponseJetons {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

enum ErreurJetons {
    Refus {
        statut: u16,
        code: String,
        message: String,
    },
    Autre(ErreurAuth),
}

impl ErreurJetons {
    fn pour_la_connexion(self) -> ErreurAuth {
        match self {
            Self::Refus { statut, .. } if statut >= 500 => ErreurAuth::Serveur(statut),
            Self::Refus { code, message, .. } => ErreurAuth::Refusee { code, message },
            Self::Autre(erreur) => erreur,
        }
    }
}

struct Acces {
    valeur: String,
    expire_a: Instant,
    renouveler_a: Instant,
}

struct Points {
    autorisation: Url,
    inscription: Url,
    jetons: Url,
    deconnexion: Url,
}

struct Interne {
    config: ConfigAuth,
    points: Points,
    http: reqwest::Client,
    coffre: Arc<dyn Coffre + Send + Sync>,
    secours: Mutex<Option<String>>,
    journal: OnceLock<Arc<Journal>>,
    acces: Mutex<Option<Acces>>,
    etat: watch::Sender<EtatSession>,
    verrou: tokio::sync::Mutex<()>,
    annulation: Mutex<Option<oneshot::Sender<()>>>,
    generation: AtomicU64,
    tache: Mutex<Option<AbortHandle>>,
}

fn verrouiller<T>(verrou: &Mutex<T>) -> MutexGuard<'_, T> {
    verrou.lock().unwrap_or_else(PoisonError::into_inner)
}

fn reseau(erreur: reqwest::Error) -> ErreurAuth {
    ErreurAuth::Reseau(erreur.without_url().to_string())
}

fn base64url(octets: &[u8]) -> String {
    let mut sortie = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let octet = |i: usize| u32::from(bloc.get(i).copied().unwrap_or(0));
        let n = (octet(0) << 16) | (octet(1) << 8) | octet(2);
        for i in 0..=bloc.len() {
            sortie.push(char::from(
                ALPHABET_BASE64URL[((n >> (18 - 6 * i)) & 63) as usize],
            ));
        }
    }
    sortie
}

fn aleatoire(taille: usize) -> Result<String, ErreurAuth> {
    let mut octets = vec![0u8; taille];
    getrandom::fill(&mut octets).map_err(|e| ErreurAuth::Systeme(e.to_string()))?;
    Ok(base64url(&octets))
}

pub fn defi_s256(verificateur: &str) -> String {
    base64url(&Sha256::digest(verificateur.as_bytes()))
}

impl Interne {
    fn noter(&self, niveau: Niveau, message: &str) {
        if let Some(journal) = self.journal.get() {
            journal.ecrire(niveau, SOURCE_JOURNAL, message);
        }
    }

    fn changer_etat(&self, etat: EtatSession) {
        self.etat.send_replace(etat);
    }

    fn jeton(&self) -> Option<String> {
        verrouiller(&self.acces)
            .as_ref()
            .filter(|acces| Instant::now() < acces.expire_a)
            .map(|acces| acces.valeur.clone())
    }

    fn acces_valide(&self) -> bool {
        self.jeton().is_some()
    }

    async fn au_coffre<T: Send + 'static>(
        &self,
        action: impl FnOnce(&dyn Coffre) -> Resultat<T> + Send + 'static,
    ) -> Resultat<T> {
        let coffre = self.coffre.clone();
        tokio::task::spawn_blocking(move || action(&*coffre))
            .await
            .unwrap_or_else(|e| Err(Erreur::Echec(e.to_string())))
    }

    async fn lire_refresh(&self) -> Option<String> {
        match self.au_coffre(|coffre| coffre.lire(CLE_COFFRE)).await {
            Ok(Some(valeur)) if !valeur.is_empty() => Some(valeur),
            Ok(_) => verrouiller(&self.secours).clone(),
            Err(erreur) => {
                self.noter(
                    Niveau::Avertissement,
                    &format!("coffre illisible : {erreur}"),
                );
                verrouiller(&self.secours).clone()
            }
        }
    }

    async fn ecrire_refresh(&self, valeur: String) {
        let copie = valeur.clone();
        match self
            .au_coffre(move |coffre| coffre.ecrire(CLE_COFFRE, &copie))
            .await
        {
            Ok(()) => *verrouiller(&self.secours) = None,
            Err(erreur) => {
                self.noter(
                    Niveau::Avertissement,
                    &format!(
                        "coffre indisponible ({erreur}) : session gardée en mémoire seulement"
                    ),
                );
                *verrouiller(&self.secours) = Some(valeur);
            }
        }
    }

    async fn effacer_refresh(&self) {
        *verrouiller(&self.secours) = None;
        if let Err(erreur) = self.au_coffre(|coffre| coffre.effacer(CLE_COFFRE)).await {
            self.noter(
                Niveau::Avertissement,
                &format!("coffre : effacement impossible ({erreur})"),
            );
        }
    }

    fn url_autorisation(
        &self,
        parcours: Parcours,
        redirection: &str,
        etat: &str,
        defi: &str,
    ) -> Url {
        let mut url = match parcours {
            Parcours::Connexion => self.points.autorisation.clone(),
            Parcours::Inscription => self.points.inscription.clone(),
        };
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("response_type", "code")
            .append_pair("scope", &self.config.portees)
            .append_pair("redirect_uri", redirection)
            .append_pair("state", etat)
            .append_pair("code_challenge", defi)
            .append_pair("code_challenge_method", "S256");
        url
    }

    async fn poster(
        &self,
        url: &Url,
        parametres: &[(&str, &str)],
    ) -> Result<Vec<u8>, ErreurJetons> {
        let corps = {
            let mut corps = form_urlencoded::Serializer::new(String::new());
            corps.append_pair("client_id", &self.config.client_id);
            for (cle, valeur) in parametres {
                corps.append_pair(cle, valeur);
            }
            corps.finish()
        };
        let reponse = self
            .http
            .post(url.clone())
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, "application/json")
            .body(corps)
            .send()
            .await
            .map_err(|e| ErreurJetons::Autre(reseau(e)))?;
        let statut = reponse.status();
        let octets = reponse
            .bytes()
            .await
            .map_err(|e| ErreurJetons::Autre(reseau(e)))?;
        self.noter(
            if statut.is_success() {
                Niveau::Info
            } else {
                Niveau::Avertissement
            },
            &format!("POST {} → {}", url.path(), statut.as_u16()),
        );
        if statut.is_success() {
            return Ok(octets.to_vec());
        }
        let donnees = serde_json::from_slice::<Value>(&octets).unwrap_or(Value::Null);
        let texte = |cle: &str| donnees.get(cle).and_then(Value::as_str).map(str::to_string);
        let code = texte("error").unwrap_or_else(|| format!("http_{}", statut.as_u16()));
        let message = texte("error_description").unwrap_or_else(|| code.clone());
        Err(ErreurJetons::Refus {
            statut: statut.as_u16(),
            code,
            message,
        })
    }

    async fn demander_jetons(
        &self,
        parametres: &[(&str, &str)],
    ) -> Result<ReponseJetons, ErreurJetons> {
        let octets = self.poster(&self.points.jetons, parametres).await?;
        serde_json::from_slice(&octets)
            .map_err(|e| ErreurJetons::Autre(ErreurAuth::ReponseInvalide(e.to_string())))
    }

    async fn accepter(self: &Arc<Self>, reponse: ReponseJetons) -> Result<(), ErreurAuth> {
        if reponse.access_token.is_empty() {
            return Err(ErreurAuth::ReponseInvalide(
                "jeton d'accès vide".to_string(),
            ));
        }
        if let Some(refresh) = reponse.refresh_token.filter(|r| !r.is_empty()) {
            self.ecrire_refresh(refresh).await;
        }
        let maintenant = Instant::now();
        let duree = Duration::from_secs(reponse.expires_in);
        let avance = duree
            .saturating_sub(self.config.marge_renouvellement)
            .max(duree / 2);
        *verrouiller(&self.acces) = Some(Acces {
            valeur: reponse.access_token,
            expire_a: maintenant + duree,
            renouveler_a: maintenant + avance,
        });
        self.noter(
            Niveau::Info,
            &format!("jetons obtenus, valables {} s", reponse.expires_in),
        );
        self.changer_etat(EtatSession::Connecte);
        self.lancer_renouvellement();
        Ok(())
    }

    fn lancer_renouvellement(self: &Arc<Self>) {
        let mut tache = verrouiller(&self.tache);
        if tache.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        let faible = Arc::downgrade(self);
        *tache = Some(tokio::spawn(boucle_renouvellement(faible)).abort_handle());
    }

    fn arreter_renouvellement(&self) {
        if let Some(tache) = verrouiller(&self.tache).take() {
            tache.abort();
        }
    }

    fn perdre_session(&self) {
        *verrouiller(&self.acces) = None;
        self.arreter_renouvellement();
        self.changer_etat(EtatSession::Erreur(ErreurAuth::SessionExpiree));
    }

    async fn renouveler(self: &Arc<Self>) -> Result<(), ErreurAuth> {
        let _garde = self.verrou.lock().await;
        let Some(refresh) = self.lire_refresh().await else {
            self.perdre_session();
            return Err(ErreurAuth::SessionExpiree);
        };
        let resultat = self
            .demander_jetons(&[("grant_type", "refresh_token"), ("refresh_token", &refresh)])
            .await;
        match resultat {
            Ok(reponse) => self.accepter(reponse).await,
            Err(ErreurJetons::Refus {
                statut: 400 | 401,
                code,
                ..
            }) => {
                self.noter(
                    Niveau::Avertissement,
                    &format!("renouvellement refusé ({code}) : retour à la connexion"),
                );
                self.effacer_refresh().await;
                self.perdre_session();
                Err(ErreurAuth::SessionExpiree)
            }
            Err(erreur) => {
                let erreur = match erreur {
                    ErreurJetons::Refus { statut, .. } => ErreurAuth::Serveur(statut),
                    ErreurJetons::Autre(erreur) => erreur,
                };
                self.noter(
                    Niveau::Avertissement,
                    &format!("renouvellement impossible : {erreur}"),
                );
                if !self.acces_valide() {
                    self.changer_etat(EtatSession::Erreur(erreur.clone()));
                }
                Err(erreur)
            }
        }
    }
}

async fn boucle_renouvellement(faible: Weak<Interne>) {
    let mut echecs = 0u32;
    loop {
        let attente = {
            let Some(interne) = faible.upgrade() else {
                return;
            };
            let acces = verrouiller(&interne.acces);
            match acces.as_ref() {
                Some(acces) => acces.renouveler_a.saturating_duration_since(Instant::now()),
                None => return,
            }
        };
        tokio::time::sleep(attente).await;
        let Some(interne) = faible.upgrade() else {
            return;
        };
        match interne.renouveler().await {
            Ok(()) => echecs = 0,
            Err(erreur) if erreur.exige_connexion() => return,
            Err(_) => {
                drop(interne);
                echecs = echecs.saturating_add(1);
                let pause = REPRISE_MIN
                    .saturating_mul(1 << echecs.min(5).saturating_sub(1))
                    .min(REPRISE_MAX);
                tokio::time::sleep(pause).await;
            }
        }
    }
}

enum Examen {
    Inconnue,
    EtatInvalide,
    Redirection(HashMap<String, String>),
}

fn examiner(cible: Option<&str>, attendu: &str) -> Examen {
    let Some((chemin, requete)) = cible.and_then(|c| c.split_once('?')) else {
        return Examen::Inconnue;
    };
    if chemin != CHEMIN_REDIRECTION {
        return Examen::Inconnue;
    }
    let parametres: HashMap<String, String> = form_urlencoded::parse(requete.as_bytes())
        .into_owned()
        .collect();
    if parametres.get("state").map(String::as_str) != Some(attendu) {
        return Examen::EtatInvalide;
    }
    Examen::Redirection(parametres)
}

async fn lire_cible(flux: &mut TcpStream) -> Option<String> {
    let mut tete = Vec::new();
    let mut tampon = [0u8; 2048];
    while !tete.windows(4).any(|fenetre| fenetre == b"\r\n\r\n") {
        if tete.len() >= TETE_MAX {
            return None;
        }
        let place = (TETE_MAX - tete.len()).min(tampon.len());
        let lus = flux.read(&mut tampon[..place]).await.ok()?;
        if lus == 0 {
            return None;
        }
        tete.extend_from_slice(&tampon[..lus]);
    }
    let texte = String::from_utf8_lossy(&tete);
    let mut morceaux = texte.lines().next()?.split(' ');
    let methode = morceaux.next()?;
    let cible = morceaux.next()?;
    let version = morceaux.next()?;
    (methode == "GET" && version.starts_with("HTTP/")).then(|| cible.to_string())
}

async fn repondre(flux: &mut TcpStream, statut: &str, texte: &str) {
    let corps = format!(
        "<!doctype html><meta charset=utf-8><title>Estompe</title><body style=\"font-family:system-ui;margin:3rem\">{texte}</body>"
    );
    let reponse = format!(
        "HTTP/1.1 {statut}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{corps}",
        corps.len()
    );
    let ecriture = async {
        let _ = flux.write_all(reponse.as_bytes()).await;
        let _ = flux.shutdown().await;
    };
    let _ = tokio::time::timeout(DELAI_LECTURE, ecriture).await;
}

async fn attendre_redirection(
    ecoute: TcpListener,
    attendu: &str,
) -> Result<HashMap<String, String>, ErreurAuth> {
    let (envoi, mut reception) = mpsc::channel::<(TcpStream, Option<String>)>(CONNEXIONS_MAX);
    let mut lectures = JoinSet::new();
    loop {
        while lectures.try_join_next().is_some() {}
        tokio::select! {
            acceptee = ecoute.accept() => {
                let (mut flux, _) = acceptee.map_err(|e| ErreurAuth::Reseau(e.to_string()))?;
                if lectures.len() >= CONNEXIONS_MAX {
                    continue;
                }
                let envoi = envoi.clone();
                lectures.spawn(async move {
                    let cible = tokio::time::timeout(DELAI_LECTURE, lire_cible(&mut flux))
                        .await
                        .ok()
                        .flatten();
                    let _ = envoi.send((flux, cible)).await;
                });
            }
            Some((mut flux, cible)) = reception.recv() => {
                match examiner(cible.as_deref(), attendu) {
                    Examen::Inconnue => repondre(&mut flux, "404 Not Found", PAGE_INTROUVABLE).await,
                    Examen::EtatInvalide => repondre(&mut flux, "400 Bad Request", PAGE_INATTENDUE).await,
                    Examen::Redirection(parametres) => {
                        let page = if parametres.contains_key("error") {
                            PAGE_INTERROMPUE
                        } else {
                            PAGE_REUSSIE
                        };
                        repondre(&mut flux, "200 OK", page).await;
                        return Ok(parametres);
                    }
                }
            }
        }
    }
}

pub struct Authentification {
    interne: Arc<Interne>,
}

impl Authentification {
    pub fn nouvelle(
        config: ConfigAuth,
        coffre: Arc<dyn Coffre + Send + Sync>,
    ) -> Result<Self, ErreurAuth> {
        let emetteur = valider_adresse(&config.emetteur)
            .map_err(|e| ErreurAuth::Systeme(format!("émetteur Keycloak : {e}")))?;
        let point = |chemin: &str| {
            Url::parse(&format!("{emetteur}/protocol/openid-connect/{chemin}"))
                .map_err(|e| ErreurAuth::Systeme(e.to_string()))
        };
        let points = Points {
            autorisation: point("auth")?,
            inscription: point("registrations")?,
            jetons: point("token")?,
            deconnexion: point("logout")?,
        };
        let mut constructeur = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(config.delai_requete);
        if !config.mandataire_systeme {
            constructeur = constructeur.no_proxy();
        }
        let http = constructeur
            .build()
            .map_err(|e| ErreurAuth::Systeme(e.to_string()))?;
        Ok(Self {
            interne: Arc::new(Interne {
                config,
                points,
                http,
                coffre,
                secours: Mutex::new(None),
                journal: OnceLock::new(),
                acces: Mutex::new(None),
                etat: watch::Sender::new(EtatSession::Deconnecte),
                verrou: tokio::sync::Mutex::new(()),
                annulation: Mutex::new(None),
                generation: AtomicU64::new(0),
                tache: Mutex::new(None),
            }),
        })
    }

    pub fn avec_journal(self, journal: Arc<Journal>) -> Self {
        let _ = self.interne.journal.set(journal);
        self
    }

    pub fn config(&self) -> &ConfigAuth {
        &self.interne.config
    }

    pub fn etat(&self) -> EtatSession {
        self.interne.etat.borrow().clone()
    }

    pub fn observer(&self) -> watch::Receiver<EtatSession> {
        self.interne.etat.subscribe()
    }

    pub fn jeton(&self) -> Option<String> {
        self.interne.jeton()
    }

    pub fn fournisseur_jeton(&self) -> impl Fn() -> Option<String> + Send + Sync + 'static {
        let interne = Arc::downgrade(&self.interne);
        move || interne.upgrade().and_then(|interne| interne.jeton())
    }

    pub fn demarrer_connexion(
        &self,
        navigateur: &dyn OuvertureUrl,
        parcours: Parcours,
    ) -> Result<Connexion, ErreurAuth> {
        let interne = &self.interne;
        let generation = interne.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (envoi, reception) = oneshot::channel();
        if let Some(precedente) = verrouiller(&interne.annulation).replace(envoi) {
            let _ = precedente.send(());
        }
        let preparation = (|| {
            let systeme = |e: std::io::Error| ErreurAuth::Systeme(e.to_string());
            let ecoute = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(systeme)?;
            ecoute.set_nonblocking(true).map_err(systeme)?;
            let port = ecoute.local_addr().map_err(systeme)?.port();
            let redirection = format!("http://127.0.0.1:{port}{CHEMIN_REDIRECTION}");
            let etat_attendu = aleatoire(16)?;
            let verificateur = aleatoire(48)?;
            let url = interne.url_autorisation(
                parcours,
                &redirection,
                &etat_attendu,
                &defi_s256(&verificateur),
            );
            if !navigateur.disponible() {
                return Err(ErreurAuth::NavigateurIndisponible(
                    "aucun navigateur sur ce système".to_string(),
                ));
            }
            navigateur
                .ouvrir(url.as_str())
                .map_err(|e| ErreurAuth::NavigateurIndisponible(e.to_string()))?;
            Ok((ecoute, redirection, etat_attendu, verificateur))
        })();
        match preparation {
            Ok((ecoute, redirection, etat_attendu, verificateur)) => {
                interne.noter(
                    Niveau::Info,
                    "navigateur ouvert, attente de la redirection sur 127.0.0.1",
                );
                interne.changer_etat(EtatSession::AttenteNavigateur);
                Ok(Connexion {
                    interne: interne.clone(),
                    ecoute: Some(ecoute),
                    redirection,
                    etat_attendu,
                    verificateur,
                    annulation: Some(reception),
                    generation,
                    terminee: false,
                })
            }
            Err(erreur) => {
                interne.noter(Niveau::Erreur, &erreur.to_string());
                interne.changer_etat(EtatSession::Erreur(erreur.clone()));
                Err(erreur)
            }
        }
    }

    pub fn annuler_connexion(&self) {
        if let Some(envoi) = verrouiller(&self.interne.annulation).take() {
            let _ = envoi.send(());
        }
    }

    pub async fn reprendre(&self) -> Result<bool, ErreurAuth> {
        if self.interne.lire_refresh().await.is_none() {
            self.interne.changer_etat(EtatSession::Deconnecte);
            return Ok(false);
        }
        self.interne.renouveler().await.map(|()| true)
    }

    pub async fn renouveler(&self) -> Result<(), ErreurAuth> {
        self.interne.renouveler().await
    }

    pub async fn deconnecter(&self) -> Result<(), ErreurAuth> {
        self.annuler_connexion();
        let interne = &self.interne;
        let _garde = interne.verrou.lock().await;
        interne.arreter_renouvellement();
        *verrouiller(&interne.acces) = None;
        let refresh = interne.lire_refresh().await;
        interne.effacer_refresh().await;
        interne.changer_etat(EtatSession::Deconnecte);
        interne.noter(Niveau::Info, "déconnexion");
        let Some(refresh) = refresh else {
            return Ok(());
        };
        match interne
            .poster(&interne.points.deconnexion, &[("refresh_token", &refresh)])
            .await
        {
            Ok(_) | Err(ErreurJetons::Refus { statut: 400, .. }) => Ok(()),
            Err(ErreurJetons::Refus { statut, .. }) => Err(ErreurAuth::Serveur(statut)),
            Err(ErreurJetons::Autre(erreur)) => Err(erreur),
        }
    }
}

impl Drop for Authentification {
    fn drop(&mut self) {
        self.interne.arreter_renouvellement();
        self.annuler_connexion();
    }
}

pub struct Connexion {
    interne: Arc<Interne>,
    ecoute: Option<std::net::TcpListener>,
    redirection: String,
    etat_attendu: String,
    verificateur: String,
    annulation: Option<oneshot::Receiver<()>>,
    generation: u64,
    terminee: bool,
}

impl Connexion {
    pub fn adresse_redirection(&self) -> &str {
        &self.redirection
    }

    fn est_courante(&self) -> bool {
        self.interne.generation.load(Ordering::SeqCst) == self.generation
    }

    pub async fn terminer(mut self) -> Result<(), ErreurAuth> {
        let resultat = self.derouler().await;
        self.terminee = true;
        if self.est_courante() {
            match &resultat {
                Ok(()) => {}
                Err(ErreurAuth::Annulee) => {
                    self.interne.noter(Niveau::Info, "connexion annulée");
                    self.interne.changer_etat(EtatSession::Deconnecte);
                }
                Err(erreur) => {
                    self.interne.noter(Niveau::Erreur, &erreur.to_string());
                    self.interne
                        .changer_etat(EtatSession::Erreur(erreur.clone()));
                }
            }
        }
        resultat
    }

    async fn derouler(&mut self) -> Result<(), ErreurAuth> {
        let ecoute = self
            .ecoute
            .take()
            .ok_or_else(|| ErreurAuth::Systeme("connexion déjà terminée".to_string()))?;
        let ecoute =
            TcpListener::from_std(ecoute).map_err(|e| ErreurAuth::Systeme(e.to_string()))?;
        let annulation = self
            .annulation
            .take()
            .ok_or_else(|| ErreurAuth::Systeme("connexion déjà terminée".to_string()))?;
        let parametres = tokio::select! {
            resultat = attendre_redirection(ecoute, &self.etat_attendu) => resultat?,
            _ = annulation => return Err(ErreurAuth::Annulee),
            _ = tokio::time::sleep(self.interne.config.delai_navigateur) => {
                return Err(ErreurAuth::DelaiDepasse)
            }
        };
        self.interne
            .noter(Niveau::Info, "redirection reçue, écoute locale fermée");
        if let Some(code) = parametres.get("error") {
            if code == "access_denied" {
                return Err(ErreurAuth::Annulee);
            }
            return Err(ErreurAuth::Refusee {
                code: code.clone(),
                message: parametres.get("error_description").unwrap_or(code).clone(),
            });
        }
        let code = parametres
            .get("code")
            .filter(|c| !c.is_empty())
            .ok_or_else(|| ErreurAuth::ReponseInvalide("redirection sans code".to_string()))?;
        let _garde = self.interne.verrou.lock().await;
        let reponse = self
            .interne
            .demander_jetons(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("code_verifier", &self.verificateur),
                ("redirect_uri", &self.redirection),
            ])
            .await
            .map_err(ErreurJetons::pour_la_connexion)?;
        self.interne.accepter(reponse).await
    }
}

impl Drop for Connexion {
    fn drop(&mut self) {
        let en_attente = *self.interne.etat.borrow() == EtatSession::AttenteNavigateur;
        if !self.terminee && self.est_courante() && en_attente {
            self.interne.changer_etat(EtatSession::Deconnecte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;
    use crate::serveur_factice::{Reponse, Requete, ServeurFactice};
    use estompe_plateforme::CoffreMemoire;
    use serde_json::json;
    use std::path::Path;

    #[derive(Default)]
    struct NavigateurFactice {
        adresses: Mutex<Vec<String>>,
        refus: bool,
    }

    impl OuvertureUrl for NavigateurFactice {
        fn disponible(&self) -> bool {
            true
        }

        fn ouvrir(&self, url: &str) -> Resultat<()> {
            if self.refus {
                return Err(Erreur::Echec("aucun navigateur par défaut".to_string()));
            }
            verrouiller(&self.adresses).push(url.to_string());
            Ok(())
        }
    }

    impl NavigateurFactice {
        fn derniere(&self) -> Url {
            Url::parse(verrouiller(&self.adresses).last().unwrap()).unwrap()
        }
    }

    struct Banc {
        serveur: ServeurFactice,
        coffre: Arc<CoffreMemoire>,
        navigateur: NavigateurFactice,
        auth: Authentification,
    }

    fn config_pour(emetteur: String) -> ConfigAuth {
        ConfigAuth {
            emetteur,
            mandataire_systeme: false,
            ..ConfigAuth::default()
        }
    }

    async fn banc_avec(reponses: Vec<Reponse>, config: impl FnOnce(String) -> ConfigAuth) -> Banc {
        let serveur = ServeurFactice::demarrer(reponses).await;
        let coffre = Arc::new(CoffreMemoire::default());
        let auth = Authentification::nouvelle(
            config(format!("{}/realms/test", serveur.adresse())),
            coffre.clone(),
        )
        .unwrap();
        Banc {
            serveur,
            coffre,
            navigateur: NavigateurFactice::default(),
            auth,
        }
    }

    async fn banc(reponses: Vec<Reponse>) -> Banc {
        banc_avec(reponses, config_pour).await
    }

    fn jetons(acces: &str, refresh: &str, expire: u64) -> Reponse {
        Reponse::json(
            200,
            json!({"access_token": acces, "refresh_token": refresh, "expires_in": expire, "token_type": "Bearer"}),
        )
    }

    fn refus(statut: u16, code: &str) -> Reponse {
        Reponse::json(
            statut,
            json!({"error": code, "error_description": "Session not active"}),
        )
    }

    fn requete_url(url: &Url) -> HashMap<String, String> {
        url.query_pairs().into_owned().collect()
    }

    fn formulaire(requete: &Requete) -> HashMap<String, String> {
        form_urlencoded::parse(&requete.corps)
            .into_owned()
            .collect()
    }

    fn coffre(banc: &Banc) -> Option<String> {
        banc.coffre.lire(CLE_COFFRE).unwrap()
    }

    fn hote(redirection: &str) -> String {
        Url::parse(redirection)
            .map(|u| format!("{}:{}", u.host_str().unwrap(), u.port().unwrap()))
            .unwrap()
    }

    async fn visiter(redirection: &str, cible: &str) -> String {
        let mut flux = TcpStream::connect(hote(redirection)).await.unwrap();
        flux.write_all(format!("GET {cible} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut recu = Vec::new();
        flux.read_to_end(&mut recu).await.unwrap();
        String::from_utf8_lossy(&recu).into_owned()
    }

    async fn ecoute_fermee(redirection: &str) -> bool {
        TcpStream::connect(hote(redirection)).await.is_err()
    }

    async fn attendre(condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("condition non atteinte en 10 s");
    }

    async fn connecter(banc: &Banc, code: &str) -> Result<String, ErreurAuth> {
        let connexion = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)?;
        let redirection = connexion.adresse_redirection().to_string();
        let etat = requete_url(&banc.navigateur.derniere())["state"].clone();
        let attente = tokio::spawn(connexion.terminer());
        let page = visiter(&redirection, &format!("/callback?code={code}&state={etat}")).await;
        assert!(page.starts_with("HTTP/1.1 200"), "{page}");
        attente.await.unwrap().map(|()| redirection)
    }

    #[test]
    fn pkce_s256_conforme_a_la_rfc_7636_et_base64url_sans_remplissage() {
        assert_eq!(
            defi_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        let verificateur = aleatoire(48).unwrap();
        assert_eq!(verificateur.len(), 64);
        assert_ne!(verificateur, aleatoire(48).unwrap());
    }

    #[test]
    fn auth_configuration_par_defaut_celle_de_l_app_tauri() {
        let config = ConfigAuth::default();
        assert_eq!(config.emetteur, "https://auth.plbls.fr/realms/default");
        assert_eq!(config.client_id, "plbls-desktop");
        assert_eq!(config.portees, "openid");
        assert_eq!(crate::config::choisir_emetteur(None), EMETTEUR_KEYCLOAK);
        assert_eq!(
            crate::config::choisir_emetteur(Some("http://localhost:8180/realms/plbls/")),
            "http://localhost:8180/realms/plbls"
        );
        assert_eq!(
            crate::config::choisir_emetteur(Some("ftp://x")),
            EMETTEUR_KEYCLOAK
        );
        assert!(matches!(
            Authentification::nouvelle(
                config_pour("pas une adresse".into()),
                Arc::new(CoffreMemoire::default())
            ),
            Err(ErreurAuth::Systeme(_))
        ));
    }

    #[tokio::test]
    async fn c1_connexion_pkce_jetons_au_coffre_et_fournisseur_pour_le_client() {
        let banc = banc(vec![jetons("acces-1", "refresh-1", 300)]).await;
        let mut etats = banc.auth.observer();
        assert_eq!(*etats.borrow_and_update(), EtatSession::Deconnecte);
        let connexion = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
            .unwrap();
        assert!(etats.has_changed().unwrap());
        assert_eq!(*etats.borrow_and_update(), EtatSession::AttenteNavigateur);
        let url = banc.navigateur.derniere();
        assert_eq!(url.path(), "/realms/test/protocol/openid-connect/auth");
        let p = requete_url(&url);
        assert_eq!(p["client_id"], "plbls-desktop");
        assert_eq!(p["response_type"], "code");
        assert_eq!(p["scope"], "openid");
        assert_eq!(p["code_challenge_method"], "S256");
        assert_eq!(p["code_challenge"].len(), 43);
        assert_eq!(p["state"].len(), 22);
        assert_eq!(p["redirect_uri"], connexion.adresse_redirection());
        assert!(p["redirect_uri"].starts_with("http://127.0.0.1:"));
        assert!(p["redirect_uri"].ends_with("/callback"));
        let redirection = connexion.adresse_redirection().to_string();
        let attente = tokio::spawn(connexion.terminer());
        let page = visiter(
            &redirection,
            &format!("/callback?code=code-1&state={}", p["state"]),
        )
        .await;
        assert!(page.contains("Connexion réussie"), "{page}");
        attente.await.unwrap().unwrap();
        assert_eq!(banc.auth.etat(), EtatSession::Connecte);
        assert_eq!(*etats.borrow_and_update(), EtatSession::Connecte);
        assert_eq!(coffre(&banc).as_deref(), Some("refresh-1"));
        assert_eq!(banc.auth.jeton().as_deref(), Some("acces-1"));
        let requetes = banc.serveur.requetes();
        assert_eq!(requetes.len(), 1);
        assert_eq!(
            requetes[0].ligne(),
            ("/realms/test/protocol/openid-connect/token", "POST")
        );
        assert_eq!(
            requetes[0].entete("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        let corps = formulaire(&requetes[0]);
        assert_eq!(corps["grant_type"], "authorization_code");
        assert_eq!(corps["code"], "code-1");
        assert_eq!(corps["client_id"], "plbls-desktop");
        assert_eq!(corps["redirect_uri"], redirection);
        assert_eq!(corps["code_verifier"].len(), 64);
        assert_eq!(defi_s256(&corps["code_verifier"]), p["code_challenge"]);
        banc.serveur
            .programmer([Reponse::json(200, json!({"ok": true}))]);
        let client = banc
            .serveur
            .client()
            .avec_jeton(banc.auth.fournisseur_jeton());
        client.get::<Value>("/api/me").await.unwrap();
        assert_eq!(
            banc.serveur
                .requetes()
                .last()
                .unwrap()
                .entete("authorization"),
            Some("Bearer acces-1")
        );
    }

    #[tokio::test]
    async fn auth_inscription_ouvre_la_page_d_inscription_de_keycloak() {
        let banc = banc(vec![]).await;
        let connexion = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Inscription)
            .unwrap();
        assert_eq!(
            banc.navigateur.derniere().path(),
            "/realms/test/protocol/openid-connect/registrations"
        );
        drop(connexion);
        assert_eq!(banc.auth.etat(), EtatSession::Deconnecte);
    }

    #[tokio::test]
    async fn c1_renouvelle_avant_expiration_et_fait_tourner_le_refresh_token() {
        let banc = banc(vec![
            jetons("acces-1", "refresh-1", 1),
            jetons("acces-2", "refresh-2", 300),
        ])
        .await;
        connecter(&banc, "code-1").await.unwrap();
        let fournisseur = banc.auth.fournisseur_jeton();
        assert_eq!(fournisseur().as_deref(), Some("acces-1"));
        attendre(|| fournisseur().as_deref() == Some("acces-2")).await;
        assert_eq!(coffre(&banc).as_deref(), Some("refresh-2"));
        assert_eq!(banc.auth.etat(), EtatSession::Connecte);
        let requetes = banc.serveur.requetes();
        assert_eq!(requetes.len(), 2);
        let corps = formulaire(&requetes[1]);
        assert_eq!(corps["grant_type"], "refresh_token");
        assert_eq!(corps["refresh_token"], "refresh-1");
        assert_eq!(corps["client_id"], "plbls-desktop");
    }

    #[tokio::test]
    async fn c1_reprend_la_session_au_lancement_depuis_le_coffre() {
        let banc = banc(vec![jetons("acces-1", "refresh-1", 300)]).await;
        banc.coffre.ecrire(CLE_COFFRE, "refresh-0").unwrap();
        assert_eq!(banc.auth.reprendre().await, Ok(true));
        assert_eq!(banc.auth.etat(), EtatSession::Connecte);
        assert_eq!(banc.auth.jeton().as_deref(), Some("acces-1"));
        assert_eq!(coffre(&banc).as_deref(), Some("refresh-1"));
        assert_eq!(
            formulaire(&banc.serveur.requetes()[0])["refresh_token"],
            "refresh-0"
        );
    }

    #[tokio::test]
    async fn auth_sans_refresh_token_la_reprise_reste_deconnectee_sans_appel() {
        let banc = banc(vec![]).await;
        assert_eq!(banc.auth.reprendre().await, Ok(false));
        assert_eq!(banc.auth.etat(), EtatSession::Deconnecte);
        assert!(banc.serveur.requetes().is_empty());
        assert_eq!(banc.auth.jeton(), None);
    }

    #[tokio::test]
    async fn c1_refresh_refuse_efface_le_coffre_et_renvoie_a_la_connexion() {
        let banc = banc(vec![refus(400, "invalid_grant")]).await;
        banc.coffre.ecrire(CLE_COFFRE, "refresh-0").unwrap();
        let erreur = banc.auth.reprendre().await.unwrap_err();
        assert_eq!(erreur, ErreurAuth::SessionExpiree);
        assert!(erreur.exige_connexion());
        assert_eq!(
            banc.auth.etat(),
            EtatSession::Erreur(ErreurAuth::SessionExpiree)
        );
        assert_eq!(coffre(&banc), None);
        assert_eq!(banc.auth.jeton(), None);
    }

    #[tokio::test]
    async fn c1_refus_au_renouvellement_automatique_coupe_la_session() {
        let banc = banc(vec![
            jetons("acces-1", "refresh-1", 1),
            refus(401, "invalid_grant"),
        ])
        .await;
        connecter(&banc, "code-1").await.unwrap();
        let etats = banc.auth.observer();
        attendre(|| *etats.borrow() == EtatSession::Erreur(ErreurAuth::SessionExpiree)).await;
        assert_eq!(coffre(&banc), None);
        assert_eq!(banc.auth.jeton(), None);
    }

    #[tokio::test]
    async fn auth_une_panne_reseau_ou_de_keycloak_garde_le_refresh_token() {
        let ferme = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = ferme.local_addr().unwrap();
        drop(ferme);
        let coffre_memoire = Arc::new(CoffreMemoire::default());
        coffre_memoire.ecrire(CLE_COFFRE, "refresh-0").unwrap();
        let auth = Authentification::nouvelle(
            config_pour(format!("http://{adresse}/realms/test")),
            coffre_memoire.clone(),
        )
        .unwrap();
        let erreur = auth.reprendre().await.unwrap_err();
        assert!(matches!(erreur, ErreurAuth::Reseau(_)), "{erreur:?}");
        assert!(erreur.est_passagere() && !erreur.exige_connexion());
        assert_eq!(auth.etat(), EtatSession::Erreur(erreur));
        assert_eq!(
            coffre_memoire.lire(CLE_COFFRE).unwrap().as_deref(),
            Some("refresh-0")
        );

        let banc = banc(vec![Reponse::vide(503)]).await;
        banc.coffre.ecrire(CLE_COFFRE, "refresh-0").unwrap();
        assert_eq!(banc.auth.reprendre().await, Err(ErreurAuth::Serveur(503)));
        assert_eq!(coffre(&banc).as_deref(), Some("refresh-0"));
    }

    #[tokio::test]
    async fn c1_deconnexion_revoque_chez_keycloak_et_efface_le_coffre() {
        let banc = banc(vec![jetons("acces-1", "refresh-1", 300)]).await;
        connecter(&banc, "code-1").await.unwrap();
        banc.serveur.programmer([Reponse::vide(204)]);
        banc.auth.deconnecter().await.unwrap();
        let requetes = banc.serveur.requetes();
        let derniere = requetes.last().unwrap();
        assert_eq!(
            derniere.ligne(),
            ("/realms/test/protocol/openid-connect/logout", "POST")
        );
        let corps = formulaire(derniere);
        assert_eq!(corps["refresh_token"], "refresh-1");
        assert_eq!(corps["client_id"], "plbls-desktop");
        assert_eq!(coffre(&banc), None);
        assert_eq!(banc.auth.jeton(), None);
        assert_eq!(banc.auth.etat(), EtatSession::Deconnecte);
        assert_eq!(banc.auth.reprendre().await, Ok(false));
    }

    #[tokio::test]
    async fn auth_annulation_par_l_utilisateur_ferme_l_ecoute_sans_appeler_keycloak() {
        let banc = banc(vec![]).await;
        let connexion = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
            .unwrap();
        let redirection = connexion.adresse_redirection().to_string();
        let attente = tokio::spawn(connexion.terminer());
        banc.auth.annuler_connexion();
        assert_eq!(attente.await.unwrap(), Err(ErreurAuth::Annulee));
        assert_eq!(banc.auth.etat(), EtatSession::Deconnecte);
        assert!(ecoute_fermee(&redirection).await);
        assert!(banc.serveur.requetes().is_empty());
    }

    #[tokio::test]
    async fn auth_annulation_sur_la_page_de_keycloak_et_refus_types() {
        let banc = banc(vec![refus(400, "invalid_grant")]).await;
        for (parametres, attendu) in [
            ("error=access_denied", ErreurAuth::Annulee),
            (
                "error=temporarily_unavailable&error_description=Indisponible",
                ErreurAuth::Refusee {
                    code: "temporarily_unavailable".into(),
                    message: "Indisponible".into(),
                },
            ),
            (
                "code=code-perime",
                ErreurAuth::Refusee {
                    code: "invalid_grant".into(),
                    message: "Session not active".into(),
                },
            ),
        ] {
            let connexion = banc
                .auth
                .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
                .unwrap();
            let redirection = connexion.adresse_redirection().to_string();
            let etat = requete_url(&banc.navigateur.derniere())["state"].clone();
            let attente = tokio::spawn(connexion.terminer());
            let page = visiter(
                &redirection,
                &format!("/callback?{parametres}&state={etat}"),
            )
            .await;
            assert!(page.starts_with("HTTP/1.1 200"), "{page}");
            assert_eq!(attente.await.unwrap(), Err(attendu.clone()));
            let etat_final = if attendu == ErreurAuth::Annulee {
                EtatSession::Deconnecte
            } else {
                EtatSession::Erreur(attendu)
            };
            assert_eq!(banc.auth.etat(), etat_final);
        }
        assert_eq!(banc.serveur.requetes().len(), 1);
        assert_eq!(coffre(&banc), None);
    }

    #[tokio::test]
    async fn auth_delai_depasse_sans_redirection_ferme_l_ecoute() {
        let banc = banc_avec(vec![], |emetteur| ConfigAuth {
            delai_navigateur: Duration::from_millis(50),
            ..config_pour(emetteur)
        })
        .await;
        let connexion = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
            .unwrap();
        let redirection = connexion.adresse_redirection().to_string();
        assert_eq!(connexion.terminer().await, Err(ErreurAuth::DelaiDepasse));
        assert_eq!(
            banc.auth.etat(),
            EtatSession::Erreur(ErreurAuth::DelaiDepasse)
        );
        assert!(ecoute_fermee(&redirection).await);
    }

    #[tokio::test]
    async fn auth_navigateur_indisponible() {
        let mut banc = banc(vec![]).await;
        banc.navigateur.refus = true;
        let erreur = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
            .err()
            .unwrap();
        assert!(
            matches!(erreur, ErreurAuth::NavigateurIndisponible(_)),
            "{erreur:?}"
        );
        assert_eq!(banc.auth.etat(), EtatSession::Erreur(erreur));
    }

    #[tokio::test]
    async fn auth_une_nouvelle_connexion_annule_la_precedente() {
        let banc = banc(vec![jetons("acces-1", "refresh-1", 300)]).await;
        let premiere = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
            .unwrap();
        let attente = tokio::spawn(premiere.terminer());
        connecter(&banc, "code-1").await.unwrap();
        assert_eq!(attente.await.unwrap(), Err(ErreurAuth::Annulee));
        assert_eq!(banc.auth.etat(), EtatSession::Connecte);
    }

    #[tokio::test]
    async fn c3_le_loopback_n_accepte_qu_une_redirection_verifie_state_et_se_ferme() {
        let banc = banc(vec![jetons("acces-1", "refresh-1", 300)]).await;
        let connexion = banc
            .auth
            .demarrer_connexion(&banc.navigateur, Parcours::Connexion)
            .unwrap();
        let redirection = connexion.adresse_redirection().to_string();
        let etat = requete_url(&banc.navigateur.derniere())["state"].clone();
        let attente = tokio::spawn(connexion.terminer());
        assert!(visiter(&redirection, "/favicon.ico")
            .await
            .starts_with("HTTP/1.1 404"));
        assert!(visiter(&redirection, "/callback")
            .await
            .starts_with("HTTP/1.1 404"));
        for cible in [
            "/callback?code=code-pirate&state=faux".to_string(),
            "/callback?code=code-pirate".to_string(),
            format!("/callback?code=code-pirate&state={etat}x"),
        ] {
            let page = visiter(&redirection, &cible).await;
            assert!(page.starts_with("HTTP/1.1 400"), "{cible} : {page}");
        }
        let mut brut = TcpStream::connect(hote(&redirection)).await.unwrap();
        brut.write_all(
            format!("POST /callback?code=code-pirate&state={etat} HTTP/1.1\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
        let mut reponse = String::new();
        brut.read_to_string(&mut reponse).await.unwrap();
        assert!(reponse.starts_with("HTTP/1.1 404"), "{reponse}");
        assert_eq!(banc.auth.etat(), EtatSession::AttenteNavigateur);
        assert!(banc.serveur.requetes().is_empty());
        let page = visiter(&redirection, &format!("/callback?code=code-1&state={etat}")).await;
        assert!(
            page.starts_with("HTTP/1.1 200") && page.contains("Connexion réussie"),
            "{page}"
        );
        assert!(page.contains("Cache-Control: no-store"));
        attente.await.unwrap().unwrap();
        assert!(ecoute_fermee(&redirection).await);
        let requetes = banc.serveur.requetes();
        assert_eq!(requetes.len(), 1);
        assert_eq!(formulaire(&requetes[0])["code"], "code-1");
    }

    fn fichiers(dossier: &Path) -> Vec<std::path::PathBuf> {
        let mut trouves = Vec::new();
        for entree in std::fs::read_dir(dossier).unwrap() {
            let chemin = entree.unwrap().path();
            if chemin.is_dir() {
                trouves.extend(fichiers(&chemin));
            } else {
                trouves.push(chemin);
            }
        }
        trouves
    }

    #[tokio::test]
    async fn c2_aucun_jeton_dans_les_fichiers_de_l_app_ni_dans_le_journal() {
        let dossier = tempfile::tempdir().unwrap();
        let journal = Arc::new(Journal::ouvrir(&dossier.path().join("journaux")));
        journal.activer(true);
        Preferences::default()
            .enregistrer(&dossier.path().join("donnees"))
            .unwrap();
        let mut banc = banc(vec![
            jetons("acces-SECRET-1", "refresh-SECRET-1", 1),
            jetons("acces-SECRET-2", "refresh-SECRET-2", 300),
        ])
        .await;
        banc.auth = Authentification::nouvelle(banc.auth.config().clone(), banc.coffre.clone())
            .unwrap()
            .avec_journal(journal.clone());
        connecter(&banc, "code-SECRET").await.unwrap();
        let fournisseur = banc.auth.fournisseur_jeton();
        attendre(|| fournisseur().as_deref() == Some("acces-SECRET-2")).await;
        banc.serveur.programmer([Reponse::json(200, json!({}))]);
        let client = banc
            .serveur
            .client()
            .avec_jeton(banc.auth.fournisseur_jeton())
            .avec_journal(journal.clone());
        client
            .get::<Value>("/api/me?token=acces-SECRET-2")
            .await
            .unwrap();
        banc.serveur.programmer([refus(400, "invalid_grant")]);
        assert_eq!(
            banc.auth.renouveler().await,
            Err(ErreurAuth::SessionExpiree)
        );
        banc.coffre.ecrire(CLE_COFFRE, "refresh-SECRET-3").unwrap();
        banc.serveur.programmer([Reponse::vide(204)]);
        banc.auth.deconnecter().await.unwrap();

        let mut secrets: Vec<String> = ["acces-SECRET", "refresh-SECRET", "code-SECRET"]
            .map(String::from)
            .to_vec();
        for requete in banc.serveur.requetes() {
            let corps = formulaire(&requete);
            secrets.extend(corps.get("code_verifier").cloned());
        }
        secrets.extend(requete_url(&banc.navigateur.derniere()).remove("state"));
        assert_eq!(secrets.len(), 5);
        let contenu = journal.lire();
        assert!(contenu.contains("\"s\":\"connexion\""), "{contenu}");
        assert!(contenu.contains("jetons obtenus"), "{contenu}");
        assert!(contenu.contains("renouvellement refusé"), "{contenu}");
        let trouves = fichiers(dossier.path());
        assert!(trouves.iter().any(|f| f.ends_with("estompe-debug.log")));
        assert!(trouves.iter().any(|f| f.ends_with("preferences.json")));
        for fichier in trouves {
            let texte = String::from_utf8_lossy(&std::fs::read(&fichier).unwrap()).into_owned();
            for secret in &secrets {
                assert!(
                    !texte.contains(secret.as_str()),
                    "{secret} trouvé dans {}",
                    fichier.display()
                );
            }
        }
        for etat in [
            banc.auth.etat(),
            EtatSession::Erreur(ErreurAuth::SessionExpiree),
        ] {
            let affiche = format!("{etat:?}");
            assert!(secrets.iter().all(|s| !affiche.contains(s.as_str())));
        }
    }
}
