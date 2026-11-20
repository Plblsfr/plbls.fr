use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use reqwest::header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::config::valider_adresse;
use crate::journal::{Journal, Niveau};

pub const DELAI: Duration = Duration::from_secs(30);
pub const DELAI_ENVOI: Duration = Duration::from_secs(300);
pub const DELAI_TELECHARGEMENT: Duration = Duration::from_secs(1800);
pub const EXTENSION_PARTIELLE: &str = "partiel";
pub const ATTENTE_429_MAX: Duration = Duration::from_secs(30);
pub const ENTETE_PLATEFORME: &str = "X-Estompe-Platform";
pub const ENTETE_VERSION: &str = "X-Estompe-Version";

const SOURCE_JOURNAL: &str = "réseau";
const MESSAGE_RESEAU: &str = "Connexion au serveur impossible.";
const MESSAGE_DELAI: &str = "Le serveur ne répond pas.";

#[derive(Debug, Clone, PartialEq)]
pub struct Refus {
    pub statut: u16,
    pub code: String,
    pub message: String,
    pub donnees: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurApi {
    Reseau(String),
    DelaiDepasse,
    NonAutorise(Refus),
    Interdit(Refus),
    Introuvable(Refus),
    Conflit(Refus),
    MiseAJourRequise(Refus),
    Serveur(Refus),
    Autre(Refus),
    ReponseInvalide(String),
    RequeteInvalide(String),
}

impl ErreurApi {
    fn depuis_refus(refus: Refus) -> Self {
        match refus.statut {
            401 => Self::NonAutorise(refus),
            403 => Self::Interdit(refus),
            404 => Self::Introuvable(refus),
            409 => Self::Conflit(refus),
            426 => Self::MiseAJourRequise(refus),
            500..=599 => Self::Serveur(refus),
            _ => Self::Autre(refus),
        }
    }

    fn depuis_transport(erreur: reqwest::Error) -> Self {
        if erreur.is_timeout() {
            Self::DelaiDepasse
        } else {
            Self::Reseau(erreur.without_url().to_string())
        }
    }

    pub fn refus(&self) -> Option<&Refus> {
        match self {
            Self::NonAutorise(r)
            | Self::Interdit(r)
            | Self::Introuvable(r)
            | Self::Conflit(r)
            | Self::MiseAJourRequise(r)
            | Self::Serveur(r)
            | Self::Autre(r) => Some(r),
            _ => None,
        }
    }

    pub fn statut(&self) -> u16 {
        self.refus().map_or(0, |r| r.statut)
    }

    pub fn code(&self) -> &str {
        match self {
            Self::Reseau(_) => "network",
            Self::DelaiDepasse => "timeout",
            Self::ReponseInvalide(_) => "invalid_response",
            Self::RequeteInvalide(_) => "invalid_request",
            _ => self.refus().map_or("", |r| r.code.as_str()),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Reseau(_) => MESSAGE_RESEAU,
            Self::DelaiDepasse => MESSAGE_DELAI,
            Self::ReponseInvalide(detail) | Self::RequeteInvalide(detail) => detail,
            _ => self.refus().map_or("", |r| r.message.as_str()),
        }
    }
}

impl fmt::Display for ErreurApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reseau(detail) => write!(f, "{MESSAGE_RESEAU} ({detail})"),
            Self::ReponseInvalide(detail) => write!(f, "réponse illisible : {detail}"),
            Self::RequeteInvalide(detail) => write!(f, "requête invalide : {detail}"),
            _ => match self.refus() {
                Some(r) => write!(f, "{} {} : {}", r.statut, r.code, r.message),
                None => write!(f, "{}", self.message()),
            },
        }
    }
}

impl std::error::Error for ErreurApi {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OptionsAppel {
    pub discret: bool,
    pub delai: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partie {
    pub nom: String,
    pub fichier: Option<String>,
    pub type_mime: Option<String>,
    pub contenu: Bytes,
}

impl Partie {
    pub fn texte(nom: &str, valeur: &str) -> Self {
        Self {
            nom: nom.to_string(),
            fichier: None,
            type_mime: None,
            contenu: Bytes::copy_from_slice(valeur.as_bytes()),
        }
    }

    pub fn fichier(nom: &str, fichier: &str, type_mime: &str, contenu: impl Into<Bytes>) -> Self {
        Self {
            nom: nom.to_string(),
            fichier: Some(fichier.to_string()),
            type_mime: Some(type_mime.to_string()),
            contenu: contenu.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Corps {
    Aucun,
    Json(Bytes),
    Brut { type_mime: String, octets: Bytes },
    Formulaire { frontiere: String, octets: Bytes },
}

static FRONTIERES: AtomicU64 = AtomicU64::new(0);

fn contient(octets: &[u8], motif: &[u8]) -> bool {
    octets.windows(motif.len()).any(|fenetre| fenetre == motif)
}

fn entre_guillemets(valeur: &str) -> String {
    valeur
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

impl Corps {
    pub fn json<C: Serialize + ?Sized>(corps: &C) -> Result<Self, ErreurApi> {
        serde_json::to_vec(corps)
            .map(|o| Self::Json(o.into()))
            .map_err(|e| ErreurApi::RequeteInvalide(e.to_string()))
    }

    pub fn brut(type_mime: &str, octets: impl Into<Bytes>) -> Self {
        Self::Brut {
            type_mime: type_mime.to_string(),
            octets: octets.into(),
        }
    }

    pub fn formulaire(parties: &[Partie]) -> Self {
        let instant = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let frontiere = loop {
            let essai = format!(
                "estompe-{instant:x}-{:x}",
                FRONTIERES.fetch_add(1, Ordering::Relaxed)
            );
            if !parties
                .iter()
                .any(|p| contient(&p.contenu, essai.as_bytes()))
            {
                break essai;
            }
        };
        let mut octets = Vec::new();
        for partie in parties {
            let mut tete = format!(
                "--{frontiere}\r\nContent-Disposition: form-data; name=\"{}\"",
                entre_guillemets(&partie.nom)
            );
            if let Some(fichier) = &partie.fichier {
                tete += &format!("; filename=\"{}\"", entre_guillemets(fichier));
            }
            if let Some(type_mime) = &partie.type_mime {
                tete += &format!("\r\nContent-Type: {type_mime}");
            }
            octets.extend_from_slice(tete.as_bytes());
            octets.extend_from_slice(b"\r\n\r\n");
            octets.extend_from_slice(&partie.contenu);
            octets.extend_from_slice(b"\r\n");
        }
        octets.extend_from_slice(format!("--{frontiere}--\r\n").as_bytes());
        Self::Formulaire {
            frontiere,
            octets: octets.into(),
        }
    }

    fn est_un_envoi(&self) -> bool {
        matches!(self, Self::Brut { .. } | Self::Formulaire { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fichier {
    pub octets: Bytes,
    pub nom: String,
    pub type_mime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reglages {
    pub delai: Duration,
    pub delai_envoi: Duration,
    pub attente_429_max: Duration,
    pub mandataire_systeme: bool,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            delai: DELAI,
            delai_envoi: DELAI_ENVOI,
            attente_429_max: ATTENTE_429_MAX,
            mandataire_systeme: true,
        }
    }
}

type FournisseurJeton = Arc<dyn Fn() -> Option<String> + Send + Sync>;
type Signal = Arc<dyn Fn() + Send + Sync>;
type SignalMessage = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    reglages: Reglages,
    jeton: Option<FournisseurJeton>,
    plateforme: Option<(String, String)>,
    perte_de_session: Option<Signal>,
    mise_a_jour_requise: Option<SignalMessage>,
    journal: Option<Arc<Journal>>,
}

impl Client {
    pub fn nouveau(adresse: &str) -> Result<Self, ErreurApi> {
        Self::avec_reglages(adresse, Reglages::default())
    }

    pub fn avec_reglages(adresse: &str, reglages: Reglages) -> Result<Self, ErreurApi> {
        let base =
            valider_adresse(adresse).map_err(|e| ErreurApi::RequeteInvalide(e.to_string()))?;
        let mut constructeur = reqwest::Client::builder();
        if !reglages.mandataire_systeme {
            constructeur = constructeur.no_proxy();
        }
        let http = constructeur
            .build()
            .map_err(|e| ErreurApi::RequeteInvalide(e.to_string()))?;
        Ok(Self {
            http,
            base,
            reglages,
            jeton: None,
            plateforme: None,
            perte_de_session: None,
            mise_a_jour_requise: None,
            journal: None,
        })
    }

    pub fn avec_jeton(
        mut self,
        fournisseur: impl Fn() -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.jeton = Some(Arc::new(fournisseur));
        self
    }

    pub fn avec_plateforme(mut self, plateforme: &str, version: &str) -> Self {
        self.plateforme = Some((plateforme.to_string(), version.to_string()));
        self
    }

    pub fn a_la_perte_de_session(mut self, signal: impl Fn() + Send + Sync + 'static) -> Self {
        self.perte_de_session = Some(Arc::new(signal));
        self
    }

    pub fn a_la_mise_a_jour_requise(
        mut self,
        signal: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        self.mise_a_jour_requise = Some(Arc::new(signal));
        self
    }

    pub fn avec_journal(mut self, journal: Arc<Journal>) -> Self {
        self.journal = Some(journal);
        self
    }

    pub fn adresse(&self) -> &str {
        &self.base
    }

    pub fn porte_un_jeton(&self) -> bool {
        self.jeton.is_some()
    }

    fn noter(&self, niveau: Niveau, message: &str) {
        if let Some(journal) = &self.journal {
            journal.ecrire(niveau, SOURCE_JOURNAL, message);
        }
    }

    fn transport(&self, methode: &Method, chemin: &str, erreur: reqwest::Error) -> ErreurApi {
        let erreur = ErreurApi::depuis_transport(erreur);
        self.noter(
            Niveau::Erreur,
            &format!("{methode} {} ✗ {erreur}", sans_parametres(chemin)),
        );
        erreur
    }

    async fn tenter(
        &self,
        methode: &Method,
        chemin: &str,
        corps: &Corps,
        options: OptionsAppel,
    ) -> Result<Response, ErreurApi> {
        let defaut = if corps.est_un_envoi() {
            self.reglages.delai_envoi
        } else {
            self.reglages.delai
        };
        let mut requete = self
            .http
            .request(methode.clone(), format!("{}{chemin}", self.base))
            .timeout(options.delai.unwrap_or(defaut));
        if let Some((plateforme, version)) = &self.plateforme {
            requete = requete
                .header(ENTETE_PLATEFORME, plateforme)
                .header(ENTETE_VERSION, version);
        }
        if let Some(jeton) = self
            .jeton
            .as_ref()
            .and_then(|f| f())
            .filter(|j| !j.is_empty())
        {
            requete = requete.header(AUTHORIZATION, format!("Bearer {jeton}"));
        }
        requete = match corps {
            Corps::Aucun => requete,
            Corps::Json(octets) => requete
                .header(CONTENT_TYPE, "application/json")
                .body(octets.clone()),
            Corps::Brut { type_mime, octets } => {
                requete.header(CONTENT_TYPE, type_mime).body(octets.clone())
            }
            Corps::Formulaire { frontiere, octets } => requete
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={frontiere}"),
                )
                .body(octets.clone()),
        };
        requete
            .send()
            .await
            .map_err(|e| self.transport(methode, chemin, e))
    }

    pub async fn appeler(
        &self,
        methode: Method,
        chemin: &str,
        corps: &Corps,
        options: OptionsAppel,
    ) -> Result<Response, ErreurApi> {
        let mut reponse = self.tenter(&methode, chemin, corps, options).await?;
        if reponse.status() == StatusCode::UNAUTHORIZED && !options.discret {
            reponse = self.tenter(&methode, chemin, corps, options).await?;
        }
        if reponse.status() == StatusCode::TOO_MANY_REQUESTS && methode == Method::GET {
            let annonce = reponse
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok());
            tokio::time::sleep(attente_apres_429(annonce).min(self.reglages.attente_429_max)).await;
            reponse = self.tenter(&methode, chemin, corps, options).await?;
        }
        let statut = reponse.status();
        let court = sans_parametres(chemin);
        let niveau = if statut.is_success() {
            Niveau::Info
        } else {
            Niveau::Avertissement
        };
        self.noter(niveau, &format!("{methode} {court} → {}", statut.as_u16()));
        if statut.is_success() {
            return Ok(reponse);
        }
        let donnees = match reponse
            .bytes()
            .await
            .ok()
            .and_then(|o| serde_json::from_slice::<Value>(&o).ok())
        {
            Some(Value::Object(objet)) => objet,
            _ => Map::new(),
        };
        let texte = |cle: &str| donnees.get(cle).and_then(Value::as_str).map(str::to_string);
        let refus = Refus {
            statut: statut.as_u16(),
            code: texte("error").unwrap_or_else(|| format!("http_{}", statut.as_u16())),
            message: texte("message")
                .unwrap_or_else(|| statut.canonical_reason().unwrap_or_default().to_string()),
            donnees,
        };
        self.noter(
            Niveau::Avertissement,
            &format!("{methode} {court} ✗ {} : {}", refus.code, refus.message),
        );
        if statut == StatusCode::UNAUTHORIZED && !options.discret {
            if let Some(signal) = &self.perte_de_session {
                signal();
            }
        }
        if statut == StatusCode::UPGRADE_REQUIRED {
            if let Some(signal) = &self.mise_a_jour_requise {
                signal(&refus.message);
            }
        }
        Err(ErreurApi::depuis_refus(refus))
    }

    pub async fn requete<T: DeserializeOwned>(
        &self,
        methode: Method,
        chemin: &str,
        corps: &Corps,
        options: OptionsAppel,
    ) -> Result<Option<T>, ErreurApi> {
        let reponse = self
            .appeler(methode.clone(), chemin, corps, options)
            .await?;
        let json = type_mime(&reponse).is_some_and(|t| t.contains("json"));
        if reponse.status() == StatusCode::NO_CONTENT || !json {
            return Ok(None);
        }
        let octets = reponse
            .bytes()
            .await
            .map_err(|e| self.transport(&methode, chemin, e))?;
        serde_json::from_slice(&octets)
            .map(Some)
            .map_err(|e| ErreurApi::ReponseInvalide(e.to_string()))
    }

    async fn json<T: DeserializeOwned>(
        &self,
        methode: Method,
        chemin: &str,
        corps: &Corps,
    ) -> Result<T, ErreurApi> {
        self.requete(methode, chemin, corps, OptionsAppel::default())
            .await?
            .ok_or_else(|| ErreurApi::ReponseInvalide("réponse sans corps JSON".to_string()))
    }

    pub async fn envoyer(
        &self,
        methode: Method,
        chemin: &str,
        corps: &Corps,
    ) -> Result<(), ErreurApi> {
        self.appeler(methode, chemin, corps, OptionsAppel::default())
            .await
            .map(|_| ())
    }

    pub async fn get<T: DeserializeOwned>(&self, chemin: &str) -> Result<T, ErreurApi> {
        self.json(Method::GET, chemin, &Corps::Aucun).await
    }

    pub async fn post<T: DeserializeOwned, C: Serialize + ?Sized>(
        &self,
        chemin: &str,
        corps: &C,
    ) -> Result<T, ErreurApi> {
        self.json(Method::POST, chemin, &Corps::json(corps)?).await
    }

    pub async fn put<T: DeserializeOwned, C: Serialize + ?Sized>(
        &self,
        chemin: &str,
        corps: &C,
    ) -> Result<T, ErreurApi> {
        self.json(Method::PUT, chemin, &Corps::json(corps)?).await
    }

    pub async fn patch<T: DeserializeOwned, C: Serialize + ?Sized>(
        &self,
        chemin: &str,
        corps: &C,
    ) -> Result<T, ErreurApi> {
        self.json(Method::PATCH, chemin, &Corps::json(corps)?).await
    }

    pub async fn delete(&self, chemin: &str) -> Result<(), ErreurApi> {
        self.envoyer(Method::DELETE, chemin, &Corps::Aucun).await
    }

    pub async fn televerser<T: DeserializeOwned>(
        &self,
        chemin: &str,
        parties: &[Partie],
    ) -> Result<T, ErreurApi> {
        self.json(Method::POST, chemin, &Corps::formulaire(parties))
            .await
    }

    pub async fn envoyer_image(
        &self,
        chemin: &str,
        type_mime: &str,
        image: impl Into<Bytes>,
    ) -> Result<(), ErreurApi> {
        self.envoyer(Method::PUT, chemin, &Corps::brut(type_mime, image))
            .await
    }

    pub async fn fichier(
        &self,
        chemin: &str,
        defaut: &str,
        options: OptionsAppel,
    ) -> Result<Fichier, ErreurApi> {
        let reponse = self
            .appeler(Method::GET, chemin, &Corps::Aucun, options)
            .await?;
        let disposition = reponse
            .headers()
            .get(CONTENT_DISPOSITION)
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned());
        let nom = nom_de_fichier(disposition.as_deref(), defaut);
        let type_mime = type_mime(&reponse);
        let octets = reponse
            .bytes()
            .await
            .map_err(|e| self.transport(&Method::GET, chemin, e))?;
        Ok(Fichier {
            octets,
            nom,
            type_mime,
        })
    }

    pub async fn octets(&self, chemin: &str) -> Result<Bytes, ErreurApi> {
        self.fichier(chemin, "", OptionsAppel::default())
            .await
            .map(|f| f.octets)
    }

    pub async fn telecharger(
        &self,
        chemin: &str,
        destination: &Path,
        mut progres: impl FnMut(u64, Option<u64>),
    ) -> Result<u64, ErreurApi> {
        let options = OptionsAppel {
            discret: false,
            delai: Some(DELAI_TELECHARGEMENT),
        };
        let mut reponse = self
            .appeler(Method::GET, chemin, &Corps::Aucun, options)
            .await?;
        let total = reponse.content_length();
        let mut nom_temporaire = destination.as_os_str().to_owned();
        nom_temporaire.push(".");
        nom_temporaire.push(EXTENSION_PARTIELLE);
        let temporaire = std::path::PathBuf::from(nom_temporaire);
        let ecrire = |e: std::io::Error| ErreurApi::RequeteInvalide(e.to_string());
        let mut fichier = File::create(&temporaire).map_err(ecrire)?;
        let mut recus = 0u64;
        progres(0, total);
        let resultat = loop {
            match reponse.chunk().await {
                Ok(Some(morceau)) => {
                    if let Err(e) = fichier.write_all(&morceau) {
                        break Err(ecrire(e));
                    }
                    recus += morceau.len() as u64;
                    progres(recus, total);
                }
                Ok(None) => break fichier.sync_all().map_err(ecrire),
                Err(e) => break Err(self.transport(&Method::GET, chemin, e)),
            }
        };
        drop(fichier);
        match resultat.and_then(|()| std::fs::rename(&temporaire, destination).map_err(ecrire)) {
            Ok(()) => Ok(recus),
            Err(erreur) => {
                let _ = std::fs::remove_file(&temporaire);
                Err(erreur)
            }
        }
    }

    pub async fn octets_externes(&self, url: &str, maximum: u64) -> Result<Bytes, ErreurApi> {
        let mut reponse = self
            .http
            .get(url)
            .timeout(self.reglages.delai)
            .send()
            .await
            .map_err(|e| self.transport(&Method::GET, url, e))?;
        if !reponse.status().is_success() {
            return Err(ErreurApi::depuis_refus(Refus {
                statut: reponse.status().as_u16(),
                code: format!("http_{}", reponse.status().as_u16()),
                message: String::new(),
                donnees: Map::new(),
            }));
        }
        if reponse.content_length().is_some_and(|n| n > maximum) {
            return Err(ErreurApi::ReponseInvalide("réponse trop grande".into()));
        }
        let mut octets = Vec::new();
        while let Some(morceau) = reponse
            .chunk()
            .await
            .map_err(|e| self.transport(&Method::GET, url, e))?
        {
            octets.extend_from_slice(&morceau);
            if octets.len() as u64 > maximum {
                return Err(ErreurApi::ReponseInvalide("réponse trop grande".into()));
            }
        }
        Ok(octets.into())
    }
}

fn type_mime(reponse: &Response) -> Option<String> {
    reponse
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn sans_parametres(chemin: &str) -> &str {
    chemin.split('?').next().unwrap_or(chemin)
}

pub fn attente_apres_429(retry_after: Option<&str>) -> Duration {
    let annonce = retry_after
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| *s != 0.0 && !s.is_nan());
    Duration::from_secs_f64(
        annonce
            .unwrap_or(1.0)
            .clamp(1.0, ATTENTE_429_MAX.as_secs_f64()),
    )
}

pub fn encoder_composant(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for octet in texte.bytes() {
        if octet.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&octet) {
            sortie.push(octet as char);
        } else {
            sortie += &format!("%{octet:02X}");
        }
    }
    sortie
}

fn decoder_pourcent(texte: &str) -> Option<String> {
    let octets = texte.as_bytes();
    let mut sortie = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' {
            let hexa = octets
                .get(i + 1..i + 3)
                .filter(|h| h.iter().all(u8::is_ascii_hexdigit))?;
            sortie.push(u8::from_str_radix(std::str::from_utf8(hexa).ok()?, 16).ok()?);
            i += 3;
        } else {
            sortie.push(octets[i]);
            i += 1;
        }
    }
    String::from_utf8(sortie).ok()
}

pub fn nom_de_fichier(disposition: Option<&str>, defaut: &str) -> String {
    const ETENDU: &str = "filename*=utf-8''";
    const SIMPLE: &str = "filename=";
    let Some(disposition) = disposition else {
        return defaut.to_string();
    };
    let minuscules = disposition.to_ascii_lowercase();
    for (debut, _) in minuscules.match_indices(ETENDU) {
        let valeur = disposition[debut + ETENDU.len()..]
            .split(';')
            .next()
            .unwrap_or_default();
        if !valeur.is_empty() {
            return decoder_pourcent(valeur).unwrap_or_else(|| defaut.to_string());
        }
    }
    for (debut, _) in minuscules.match_indices(SIMPLE) {
        let reste = &disposition[debut + SIMPLE.len()..];
        let valeur = reste
            .strip_prefix('"')
            .unwrap_or(reste)
            .split(['"', ';'])
            .next()
            .unwrap_or_default();
        if !valeur.is_empty() {
            return valeur.to_string();
        }
    }
    defaut.to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use serde_json::json;

    use super::*;
    use crate::journal::NOM_FICHIER;
    use crate::serveur_factice::{Comportement, Reponse, ServeurFactice};

    fn compteur() -> (Arc<AtomicUsize>, impl Fn() + Send + Sync + 'static) {
        let total = Arc::new(AtomicUsize::new(0));
        let partage = total.clone();
        (total, move || {
            partage.fetch_add(1, Ordering::SeqCst);
        })
    }

    fn non_autorise() -> Reponse {
        Reponse::json(401, json!({"error": "unauthorized", "message": "x"}))
    }

    #[test]
    fn lit_le_nom_annonce_par_content_disposition_sinon_le_nom_par_defaut() {
        assert_eq!(
            nom_de_fichier(
                Some("attachment; filename=\"sablier-export-2026-11-18.json\""),
                "x.json"
            ),
            "sablier-export-2026-11-18.json"
        );
        assert_eq!(
            nom_de_fichier(
                Some("attachment; filename*=UTF-8''r%C3%A9cap.json"),
                "x.json"
            ),
            "récap.json"
        );
        assert_eq!(nom_de_fichier(None, "x.json"), "x.json");
        assert_eq!(
            nom_de_fichier(Some("attachment; filename*=UTF-8''%E0%A4%A"), "x.json"),
            "x.json"
        );
        assert_eq!(
            nom_de_fichier(Some("attachment; FILENAME=a.txt; size=3"), "x.json"),
            "a.txt"
        );
        assert_eq!(
            nom_de_fichier(Some("attachment; filename=\"\""), "x.json"),
            "x.json"
        );
        assert_eq!(nom_de_fichier(Some("inline"), "x.json"), "x.json");
    }

    #[tokio::test]
    async fn le_telechargement_suit_sa_progression_et_ne_laisse_aucun_fichier_partiel() {
        let contenu = vec![7u8; 200_000];
        let serveur =
            ServeurFactice::demarrer([Reponse::brute(200, "application/octet-stream", &contenu)])
                .await;
        let dossier = tempfile::tempdir().unwrap();
        let destination = dossier.path().join("rapport.pdf");
        let mut etapes = Vec::new();
        let recus = serveur
            .client()
            .telecharger("/files/f1", &destination, |n, total| etapes.push((n, total)))
            .await
            .unwrap();
        assert_eq!(recus, 200_000);
        assert_eq!(std::fs::read(&destination).unwrap(), contenu);
        assert_eq!(etapes.first(), Some(&(0, Some(200_000))));
        assert_eq!(etapes.last(), Some(&(200_000, Some(200_000))));
        assert!(etapes.windows(2).all(|p| p[0].0 <= p[1].0));
        serveur.programmer([Reponse::json(404, json!({"error": "not_found"}))]);
        let manquant = dossier.path().join("absent.bin");
        assert!(serveur
            .client()
            .telecharger("/files/f2", &manquant, |_, _| {})
            .await
            .is_err());
        let restants: Vec<_> = std::fs::read_dir(dossier.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(restants, ["rapport.pdf"]);
    }

    #[tokio::test]
    async fn une_ressource_externe_est_bornee_et_ne_porte_pas_le_jeton() {
        let serveur =
            ServeurFactice::demarrer([Reponse::brute(200, "image/png", &[1u8; 1000])]).await;
        let client = serveur.client().avec_jeton(|| Some("secret".to_string()));
        let url = format!("{}/image.png", serveur.adresse());
        assert_eq!(client.octets_externes(&url, 4096).await.unwrap().len(), 1000);
        assert_eq!(serveur.requetes()[0].entete("authorization"), None);
        serveur.programmer([Reponse::brute(200, "image/png", &[1u8; 5000])]);
        assert!(client.octets_externes(&url, 4096).await.is_err());
        serveur.programmer([Reponse::vide(404)]);
        assert!(client.octets_externes(&url, 4096).await.is_err());
    }

    #[tokio::test]
    async fn rend_le_fichier_avec_son_nom_sans_deconnecter_en_mode_discret() {
        let serveur =
            ServeurFactice::demarrer([Reponse::brute(200, "application/json", b"{\"a\":1}")
                .avec_entete("Content-Disposition", "attachment; filename=\"e.json\"")])
            .await;
        let (pertes, signal) = compteur();
        let client = serveur
            .client()
            .avec_jeton(|| Some("jeton".to_string()))
            .a_la_perte_de_session(signal);
        let discret = OptionsAppel {
            discret: true,
            delai: None,
        };
        let fichier = client
            .fichier("/export", "defaut.json", discret)
            .await
            .unwrap();
        assert_eq!(fichier.nom, "e.json");
        assert_eq!(&fichier.octets[..], b"{\"a\":1}");
        assert_eq!(fichier.type_mime.as_deref(), Some("application/json"));
        let requetes = serveur.requetes();
        assert_eq!(requetes[0].ligne(), ("/export", "GET"));
        assert_eq!(requetes[0].entete("authorization"), Some("Bearer jeton"));
        serveur.programmer([non_autorise()]);
        let erreur = client
            .fichier("/export", "defaut.json", discret)
            .await
            .unwrap_err();
        assert!(matches!(erreur, ErreurApi::NonAutorise(_)));
        assert_eq!(pertes.load(Ordering::SeqCst), 0);
        assert_eq!(serveur.requetes().len(), 2);
    }

    #[tokio::test]
    async fn ajoute_le_bearer_fourni_a_chaque_tentative() {
        let serveur = ServeurFactice::demarrer([
            non_autorise(),
            Reponse::json(200, json!({"allowed": true})),
        ])
        .await;
        let appels = Arc::new(AtomicUsize::new(0));
        let partage = appels.clone();
        let client = serveur
            .client()
            .avec_jeton(move || Some(format!("jeton{}", partage.fetch_add(1, Ordering::SeqCst))));
        assert!(client.porte_un_jeton());
        let _: Value = client.get("/me").await.unwrap();
        let recus: Vec<Option<String>> = serveur
            .requetes()
            .iter()
            .map(|r| r.entete("authorization").map(str::to_string))
            .collect();
        assert_eq!(
            recus,
            [
                Some("Bearer jeton0".to_string()),
                Some("Bearer jeton1".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn n_ajoute_pas_d_en_tete_authorization_sans_jeton() {
        let serveur = ServeurFactice::demarrer([Reponse::json(200, json!([]))]).await;
        let _: Value = serveur.client().get("/servers").await.unwrap();
        let _: Value = serveur
            .client()
            .avec_jeton(|| None)
            .get("/servers")
            .await
            .unwrap();
        let _: Value = serveur
            .client()
            .avec_jeton(|| Some(String::new()))
            .get("/servers")
            .await
            .unwrap();
        let requetes = serveur.requetes();
        assert_eq!(requetes.len(), 3);
        assert!(requetes.iter().all(|r| r.entete("authorization").is_none()));
        assert!(requetes
            .iter()
            .all(|r| r.entete("content-type").is_none() && r.entete(ENTETE_PLATEFORME).is_none()));
        assert!(!serveur.client().porte_un_jeton());
    }

    #[tokio::test]
    async fn declare_sa_version_et_sa_plateforme_et_signale_le_refus_426() {
        let refus = json!({"error": "upgrade_required", "message": "faille corrigée", "min_version": "1.2.0"});
        let serveur = ServeurFactice::demarrer([Reponse::json(426, refus)]).await;
        let messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let partage = messages.clone();
        let client = serveur
            .client()
            .avec_plateforme("windows", "1.1.0")
            .a_la_mise_a_jour_requise(move |message| {
                partage.lock().unwrap().push(message.to_string())
            });
        let erreur = client.get::<Value>("/servers").await.unwrap_err();
        assert_eq!(erreur.statut(), 426);
        let ErreurApi::MiseAJourRequise(refus) = erreur else {
            panic!("{erreur:?}")
        };
        assert_eq!(refus.donnees["min_version"], "1.2.0");
        let requetes = serveur.requetes();
        assert_eq!(requetes.len(), 1);
        assert_eq!(requetes[0].entete(ENTETE_PLATEFORME), Some("windows"));
        assert_eq!(requetes[0].entete(ENTETE_VERSION), Some("1.1.0"));
        assert_eq!(*messages.lock().unwrap(), ["faille corrigée"]);
    }

    #[tokio::test]
    async fn transforme_les_erreurs_en_erreur_typee_avec_le_code_du_serveur() {
        let serveur = ServeurFactice::demarrer([Reponse::json(
            409,
            json!({"error": "pseudo_taken", "message": "pseudo déjà pris"}),
        )])
        .await;
        let client = serveur.client();
        let erreur = client
            .post::<Value, _>("/activate", &json!({"pseudo": "bob"}))
            .await
            .unwrap_err();
        assert!(matches!(erreur, ErreurApi::Conflit(_)));
        assert_eq!(
            (erreur.statut(), erreur.code(), erreur.message()),
            (409, "pseudo_taken", "pseudo déjà pris")
        );
        assert_eq!(erreur.to_string(), "409 pseudo_taken : pseudo déjà pris");
        type Attendu = fn(&ErreurApi) -> bool;
        let cas: [(Reponse, Attendu, &str, &str); 6] = [
            (
                Reponse::json(403, json!({"error": "forbidden"})),
                |e| matches!(e, ErreurApi::Interdit(_)),
                "forbidden",
                "Forbidden",
            ),
            (
                Reponse::vide(404),
                |e| matches!(e, ErreurApi::Introuvable(_)),
                "http_404",
                "Not Found",
            ),
            (
                Reponse::brute(500, "text/html", b"<h1>panne</h1>"),
                |e| matches!(e, ErreurApi::Serveur(_)),
                "http_500",
                "Internal Server Error",
            ),
            (
                Reponse::json(
                    503,
                    json!({"error": "permission_unavailable", "message": "aegis injoignable"}),
                ),
                |e| matches!(e, ErreurApi::Serveur(_)),
                "permission_unavailable",
                "aegis injoignable",
            ),
            (
                Reponse::json(400, json!(["pas", "un", "objet"])),
                |e| matches!(e, ErreurApi::Autre(_)),
                "http_400",
                "Bad Request",
            ),
            (
                Reponse::json(
                    429,
                    json!({"error": "rate_limited", "message": "doucement"}),
                ),
                |e| matches!(e, ErreurApi::Autre(_)),
                "rate_limited",
                "doucement",
            ),
        ];
        for (reponse, attendu, code, message) in cas {
            serveur.programmer([reponse]);
            let erreur = client.post::<Value, _>("/x", &json!({})).await.unwrap_err();
            assert!(attendu(&erreur), "{erreur:?}");
            assert_eq!((erreur.code(), erreur.message()), (code, message));
        }
    }

    #[tokio::test]
    async fn signale_les_401_apres_une_seconde_tentative() {
        let serveur = ServeurFactice::demarrer([non_autorise()]).await;
        let (pertes, signal) = compteur();
        let erreur = serveur
            .client()
            .a_la_perte_de_session(signal)
            .get::<Value>("/me")
            .await
            .unwrap_err();
        assert!(matches!(erreur, ErreurApi::NonAutorise(_)));
        assert_eq!((erreur.statut(), erreur.code()), (401, "unauthorized"));
        assert_eq!(pertes.load(Ordering::SeqCst), 1);
        assert_eq!(serveur.requetes().len(), 2);
    }

    #[tokio::test]
    async fn reessaie_un_401_isole_sans_signaler_la_perte_de_session() {
        let serveur = ServeurFactice::demarrer([
            non_autorise(),
            Reponse::json(200, json!({"allowed": true})),
        ])
        .await;
        let (pertes, signal) = compteur();
        let moi: Value = serveur
            .client()
            .a_la_perte_de_session(signal)
            .get("/me")
            .await
            .unwrap();
        assert_eq!(moi["allowed"], true);
        assert_eq!(pertes.load(Ordering::SeqCst), 0);
        assert_eq!(serveur.requetes().len(), 2);
    }

    #[tokio::test]
    async fn gere_les_204_et_les_reponses_sans_json() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let client = serveur.client();
        client
            .envoyer(Method::PUT, "/messages/12/pin", &Corps::Aucun)
            .await
            .unwrap();
        client.delete("/messages/12/pin").await.unwrap();
        let options = OptionsAppel::default();
        assert_eq!(
            client
                .requete::<Value>(Method::POST, "/push/test", &Corps::Aucun, options)
                .await
                .unwrap(),
            None
        );
        assert!(matches!(
            client.get::<Value>("/rien").await,
            Err(ErreurApi::ReponseInvalide(_))
        ));
        serveur.programmer([Reponse::brute(200, "text/plain", b"ok")]);
        assert_eq!(
            client
                .requete::<Value>(Method::GET, "/healthz", &Corps::Aucun, options)
                .await
                .unwrap(),
            None
        );
        serveur.programmer([Reponse::brute(200, "application/json", b"{\"a\":")]);
        let erreur = client.get::<Value>("/tronque").await.unwrap_err();
        assert!(matches!(erreur, ErreurApi::ReponseInvalide(_)));
        assert_eq!((erreur.statut(), erreur.code()), (0, "invalid_response"));
        let lignes = serveur.lignes();
        assert_eq!(
            lignes[0],
            ("/messages/12/pin".to_string(), "PUT".to_string())
        );
        assert_eq!(
            lignes[1],
            ("/messages/12/pin".to_string(), "DELETE".to_string())
        );
    }

    #[test]
    fn encode_un_composant_de_chemin_comme_encode_uri_component() {
        assert_eq!(encoder_composant("👍"), "%F0%9F%91%8D");
        assert_eq!(encoder_composant("AB/CD"), "AB%2FCD");
        assert_eq!(encoder_composant("a/b c"), "a%2Fb%20c");
        assert_eq!(encoder_composant("aZ09-_.!~*'()"), "aZ09-_.!~*'()");
        assert_eq!(encoder_composant("é?&=#+%"), "%C3%A9%3F%26%3D%23%2B%25");
    }

    #[tokio::test]
    async fn envoie_les_verbes_types_avec_un_corps_json() {
        let serveur = ServeurFactice::demarrer([Reponse::json(200, json!({"id": "r1"}))]).await;
        let client = serveur.client();
        let corps = json!({"name": "Amis"});
        let _: Value = client.post("/a", &corps).await.unwrap();
        let _: Value = client.put("/b", &corps).await.unwrap();
        let _: Value = client.patch("/c", &corps).await.unwrap();
        let requetes = serveur.requetes();
        let lignes: Vec<(&str, &str)> = requetes.iter().map(|r| r.ligne()).collect();
        assert_eq!(lignes, [("/a", "POST"), ("/b", "PUT"), ("/c", "PATCH")]);
        assert!(requetes
            .iter()
            .all(|r| r.entete("content-type") == Some("application/json") && r.json() == corps));
    }

    #[tokio::test]
    async fn encode_un_formulaire_multipart() {
        let serveur = ServeurFactice::demarrer([Reponse::json(201, json!({"id": "f1"}))]).await;
        let parties = [
            Partie::texte("kind", "voice"),
            Partie::fichier("file", "a \"b\".txt", "text/plain", &b"x\r\ny"[..]),
        ];
        let recu: Value = serveur
            .client()
            .televerser("/files", &parties)
            .await
            .unwrap();
        assert_eq!(recu["id"], "f1");
        let requete = &serveur.requetes()[0];
        let frontiere = requete
            .entete("content-type")
            .unwrap()
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();
        let attendu = format!(
            "--{frontiere}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\nvoice\r\n--{frontiere}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a %22b%22.txt\"\r\nContent-Type: text/plain\r\n\r\nx\r\ny\r\n--{frontiere}--\r\n"
        );
        assert_eq!(requete.texte(), attendu);
        let Corps::Formulaire {
            frontiere: autre, ..
        } = Corps::formulaire(&parties)
        else {
            panic!()
        };
        assert_ne!(autre, frontiere);
    }

    #[tokio::test]
    async fn borne_chaque_requete_dans_le_temps_et_signale_un_serveur_muet() {
        let defaut = Reglages::default();
        assert_eq!(
            (defaut.delai, defaut.delai_envoi),
            (Duration::from_secs(30), Duration::from_secs(300))
        );
        let serveur = ServeurFactice::demarrer([Comportement::Silence]).await;
        let court = Duration::from_millis(40);
        let presse = Client::avec_reglages(
            serveur.adresse(),
            Reglages {
                delai: court,
                delai_envoi: court,
                ..ServeurFactice::reglages()
            },
        )
        .unwrap();
        let erreur = presse.get::<Value>("/servers").await.unwrap_err();
        assert_eq!(erreur, ErreurApi::DelaiDepasse);
        assert_eq!(
            (erreur.statut(), erreur.code(), erreur.message()),
            (0, "timeout", "Le serveur ne répond pas.")
        );
        assert_eq!(
            presse.octets("/users/u1/avatar").await.unwrap_err(),
            ErreurApi::DelaiDepasse
        );
        assert_eq!(
            presse
                .envoyer_image("/me/avatar", "image/png", &b"x"[..])
                .await
                .unwrap_err(),
            ErreurApi::DelaiDepasse
        );
        let patient = serveur.client();
        let options = OptionsAppel {
            discret: false,
            delai: Some(court),
        };
        let erreur = patient
            .requete::<Value>(Method::GET, "/search?q=x", &Corps::Aucun, options)
            .await
            .unwrap_err();
        assert_eq!(erreur, ErreurApi::DelaiDepasse);
        assert_eq!(serveur.requetes().len(), 4);
    }

    #[tokio::test]
    async fn signale_un_serveur_injoignable() {
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = format!("http://{}", ecoute.local_addr().unwrap());
        drop(ecoute);
        let client = Client::avec_reglages(&adresse, ServeurFactice::reglages()).unwrap();
        let erreur = client.get::<Value>("/me").await.unwrap_err();
        assert!(matches!(erreur, ErreurApi::Reseau(_)), "{erreur:?}");
        assert_eq!(
            (erreur.statut(), erreur.code(), erreur.message()),
            (0, "network", "Connexion au serveur impossible.")
        );
    }

    #[tokio::test]
    async fn relance_une_lecture_apres_un_429_mais_jamais_une_action() {
        let limite = || {
            Reponse::json(429, json!({"error": "too_many_requests"}))
                .avec_entete("Retry-After", "7")
        };
        let serveur = ServeurFactice::demarrer([limite(), Reponse::json(200, json!([1]))]).await;
        let reglages = Reglages {
            attente_429_max: Duration::from_millis(1),
            ..ServeurFactice::reglages()
        };
        let client = Client::avec_reglages(serveur.adresse(), reglages).unwrap();
        let recu: Value = client.get("/gifs/trending").await.unwrap();
        assert_eq!(recu, json!([1]));
        assert_eq!(serveur.requetes().len(), 2);
        serveur.programmer([limite()]);
        let erreur = client
            .post::<Value, _>("/gifs/import", &json!({}))
            .await
            .unwrap_err();
        assert_eq!((erreur.statut(), erreur.code()), (429, "too_many_requests"));
        assert_eq!(serveur.requetes().len(), 3);
        let erreur = client.get::<Value>("/gifs/trending").await.unwrap_err();
        assert_eq!(erreur.statut(), 429);
        assert_eq!(serveur.requetes().len(), 5);
    }

    #[test]
    fn attend_le_delai_annonce_par_le_serveur_entre_une_et_trente_secondes() {
        assert_eq!(attente_apres_429(None), Duration::from_secs(1));
        assert_eq!(attente_apres_429(Some("7")), Duration::from_secs(7));
        assert_eq!(
            attente_apres_429(Some(" 2.5 ")),
            Duration::from_millis(2500)
        );
        assert_eq!(attente_apres_429(Some("0")), Duration::from_secs(1));
        assert_eq!(attente_apres_429(Some("-4")), Duration::from_secs(1));
        assert_eq!(attente_apres_429(Some("600")), Duration::from_secs(30));
        assert_eq!(
            attente_apres_429(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            Duration::from_secs(1)
        );
    }

    #[tokio::test]
    async fn journalise_les_appels_sans_parametres_ni_jeton() {
        let dossier = tempfile::tempdir().unwrap();
        let journal = Arc::new(Journal::ouvrir(dossier.path()));
        journal.activer(true);
        let serveur = ServeurFactice::demarrer([
            Reponse::json(200, json!([])),
            Reponse::json(
                403,
                json!({"error": "forbidden", "message": "refus pour token=abc123"}),
            ),
        ])
        .await;
        let client = serveur
            .client()
            .avec_jeton(|| Some("jeton-secret".to_string()))
            .avec_journal(journal.clone());
        let _: Value = client.get("/admin/users?q=alice").await.unwrap();
        let _ = client.get::<Value>("/admin/users?q=bob").await.unwrap_err();
        let contenu = std::fs::read_to_string(dossier.path().join(NOM_FICHIER)).unwrap();
        assert!(contenu.contains("GET /admin/users → 200"));
        assert!(contenu.contains("GET /admin/users → 403"));
        assert!(contenu.contains("GET /admin/users ✗ forbidden : refus pour token=[masqué]"));
        for absent in ["alice", "bob", "jeton-secret", "abc123"] {
            assert!(!contenu.contains(absent), "{absent} présent dans {contenu}");
        }
    }

    #[test]
    fn refuse_une_adresse_de_serveur_invalide() {
        assert!(matches!(
            Client::nouveau("ftp://chat.plbls.fr"),
            Err(ErreurApi::RequeteInvalide(_))
        ));
        assert_eq!(
            Client::nouveau("https://chat.plbls.fr/").unwrap().adresse(),
            "https://chat.plbls.fr"
        );
    }
}
