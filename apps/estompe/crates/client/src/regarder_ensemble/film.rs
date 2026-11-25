use std::sync::Arc;
use std::time::Instant;

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::http::{encoder_composant, Client, Corps, ErreurApi, OptionsAppel};
use crate::lecteur::{GenrePiste, Lecteur, Phase, Source};

use super::fournisseurs::{etat_de, plages_de, pret_de, EtatLecture, LecteurSeance, Plage};

pub const ENREGISTRER_TOUTES_LES_MS: i64 = 15_000;
pub const ATTENTE_DIRECT_MS: i64 = 10_000;
pub const MESSAGE_DEMARRAGE: &str = "La lecture n’a pas pu démarrer.";
pub const MESSAGE_ILLISIBLE: &str = "Cette vidéo ne peut pas être lue sur cet appareil.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Qualite {
    #[default]
    #[serde(rename = "original")]
    Originale,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
}

impl Qualite {
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Originale => "Originale",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeFlux {
    #[default]
    Direct,
    Hls,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PisteFilm {
    pub n: i64,
    pub label: Option<String>,
    pub language: Option<String>,
    pub forced: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
}

impl PisteFilm {
    pub fn nom(&self) -> String {
        self.label
            .clone()
            .or_else(|| self.language.clone())
            .unwrap_or_else(|| format!("Piste {}", self.n + 1))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Intro {
    pub debut: f64,
    pub fin: f64,
}

impl Intro {
    pub fn contient(&self, position: f64) -> bool {
        self.fin > self.debut && position >= self.debut && position < self.fin
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoFlux {
    pub codec: String,
    pub copy: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FluxFilm {
    pub stream_id: String,
    pub mode: ModeFlux,
    pub url: String,
    pub duration: f64,
    pub audio: Vec<PisteFilm>,
    pub subtitles: Vec<PisteFilm>,
    pub offset: Option<f64>,
    pub quality: Option<Qualite>,
    pub qualities: Option<Vec<Qualite>>,
    pub intro: Option<Intro>,
    pub video: Option<VideoFlux>,
    pub cache: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Support {
    pub containers: Vec<String>,
    pub video: Vec<String>,
    pub audio: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DemandeLecture {
    pub audio: i64,
    pub quality: Qualite,
    pub start: f64,
    pub support: Support,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrefAudio {
    #[default]
    Auto,
    Vf,
    Vff,
    Vfq,
    En,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrefSousTitres {
    #[default]
    #[serde(rename = "none")]
    Aucun,
    #[serde(rename = "forced")]
    Forces,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "en")]
    En,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrefTampon {
    #[default]
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "all")]
    Tout,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reseau {
    #[default]
    Inconnu,
    Filaire,
    Wifi,
    Mobile,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreferencesFilm {
    pub audio: PrefAudio,
    pub sous_titres: PrefSousTitres,
    pub tampon: PrefTampon,
    pub wifi_seulement: bool,
    pub qualite: Qualite,
    pub codecs_refuses: Vec<String>,
}

impl PreferencesFilm {
    pub fn qualite_proposee(&self, proposees: Option<&[Qualite]>) -> Qualite {
        match proposees {
            Some(q) if !q.contains(&self.qualite) => Qualite::Originale,
            _ => self.qualite,
        }
    }
}

pub fn tampon_effectif(prefs: &PreferencesFilm, reseau: Reseau) -> PrefTampon {
    if prefs.wifi_seulement && reseau == Reseau::Mobile {
        PrefTampon::Normal
    } else {
        prefs.tampon
    }
}

pub fn doit_enregistrer(enregistre_a: Option<i64>, maintenant: i64) -> bool {
    enregistre_a.is_none_or(|t| maintenant - t >= ENREGISTRER_TOUTES_LES_MS)
}

pub fn besoin_de_rouvrir(cible: f64, decalage: f64, pret: f64) -> bool {
    cible < decalage - 0.5 || cible > decalage + pret + 5.0
}

const CONTENEURS: [&str; 3] = ["mp4", "webm", "mkv"];
const CODECS_VIDEO: [&str; 5] = ["h264", "hevc", "hevc10", "vp9", "av1"];
const CODECS_AUDIO: [&str; 6] = ["aac", "mp3", "opus", "flac", "ac3", "eac3"];

pub fn support(refuses: &[String]) -> Support {
    let liste = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
    Support {
        containers: liste(&CONTENEURS),
        video: CODECS_VIDEO
            .iter()
            .filter(|c| !refuses.iter().any(|r| c.starts_with(r.as_str())))
            .map(|c| c.to_string())
            .collect(),
        audio: liste(&CODECS_AUDIO),
    }
}

fn caractere_de_mot(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn apparait(texte: &str, motif: &str, debut_de_mot: bool, fin_de_mot: bool) -> bool {
    texte.match_indices(motif).any(|(i, m)| {
        let avant = texte[..i].chars().next_back();
        let apres = texte[i + m.len()..].chars().next();
        (!debut_de_mot || !avant.is_some_and(caractere_de_mot))
            && (!fin_de_mot || !apres.is_some_and(caractere_de_mot))
    })
}

fn mot(texte: &str, motif: &str) -> bool {
    apparait(texte, motif, true, true)
}

fn debut(texte: &str, motif: &str) -> bool {
    apparait(texte, motif, true, false)
}

fn code_de(langue: &str, codes: &[&str]) -> bool {
    let l = langue.to_ascii_lowercase();
    codes
        .iter()
        .any(|c| l == *c || l.strip_prefix(c).is_some_and(|r| r.starts_with('-')))
}

pub fn famille_piste(p: &PisteFilm) -> Option<String> {
    let label = p.label.as_deref().unwrap_or("").to_lowercase();
    let fr = match &p.language {
        Some(l) => code_de(l, &["fr", "fre", "fra"]),
        None => [
            "vff",
            "vfq",
            "vfi",
            "vf2",
            "vf",
            "vostfr",
            "truefrench",
            "french",
            "francais",
            "français",
        ]
        .iter()
        .any(|m| debut(&label, m)),
    };
    if fr {
        return Some("fr".into());
    }
    let en = match &p.language {
        Some(l) => code_de(l, &["en", "eng"]),
        None => ["english", "anglais"].iter().any(|m| debut(&label, m)),
    };
    if en {
        return Some("en".into());
    }
    p.language.as_ref().map(|l| {
        l.to_lowercase()
            .split('-')
            .next()
            .unwrap_or_default()
            .to_string()
    })
}

pub fn est_forcee(p: &PisteFilm) -> bool {
    p.forced
        || p.label
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("forc")
}

fn texte_piste(p: &PisteFilm) -> String {
    format!(
        "{} {}",
        p.label.as_deref().unwrap_or(""),
        p.language.as_deref().unwrap_or("")
    )
    .to_lowercase()
}

fn vff(t: &str) -> bool {
    mot(t, "vff") || t.contains("truefrench") || mot(t, "france") || mot(t, "fr-fr")
}

fn vfq(t: &str) -> bool {
    mot(t, "vfq")
        || t.contains("québ")
        || t.contains("queb")
        || t.contains("canad")
        || mot(t, "fr-ca")
}

fn a_part(label: &str) -> bool {
    let l = label.to_lowercase();
    l.contains("comment")
        || l.contains("audio desc")
        || l.contains("audiodesc")
        || l.contains("descripti")
}

fn malentendants(label: &str) -> bool {
    let l = label.to_lowercase();
    mot(&l, "sdh") || l.contains("malentendant") || mot(&l, "cc")
}

fn meilleure(pistes: &[PisteFilm], score: impl Fn(&PisteFilm) -> i64) -> Option<&PisteFilm> {
    let mut haut = None;
    let mut max = 0;
    for p in pistes {
        let s = score(p);
        if s > max {
            max = s;
            haut = Some(p);
        }
    }
    haut
}

pub fn choisir_piste_audio(pistes: &[PisteFilm], pref: PrefAudio) -> i64 {
    if pref == PrefAudio::Auto {
        return 0;
    }
    let famille = if pref == PrefAudio::En { "en" } else { "fr" };
    let voulue: Option<fn(&str) -> bool> = match pref {
        PrefAudio::Vff => Some(vff),
        PrefAudio::Vfq => Some(vfq),
        _ => None,
    };
    let autre: Option<fn(&str) -> bool> = match pref {
        PrefAudio::Vff => Some(vfq),
        PrefAudio::Vfq => Some(vff),
        _ => None,
    };
    meilleure(pistes, |p| {
        if famille_piste(p).as_deref() != Some(famille) {
            return 0;
        }
        let t = texte_piste(p);
        let variante = if voulue.is_some_and(|f| f(&t)) {
            2
        } else if autre.is_some_and(|f| f(&t)) {
            0
        } else {
            1
        };
        1 + variante
            + if a_part(p.label.as_deref().unwrap_or("")) {
                0
            } else {
                3
            }
    })
    .map_or(0, |p| p.n)
}

pub fn choisir_piste_sous_titres(
    pistes: &[PisteFilm],
    pref: PrefSousTitres,
    audio: Option<&PisteFilm>,
) -> Option<i64> {
    let voulue = match pref {
        PrefSousTitres::Aucun => return None,
        PrefSousTitres::Forces => {
            let famille = audio.and_then(famille_piste);
            return meilleure(pistes, |p| {
                i64::from(
                    est_forcee(p)
                        && famille
                            .as_ref()
                            .is_none_or(|f| famille_piste(p).as_ref() == Some(f)),
                )
            })
            .map(|p| p.n);
        }
        PrefSousTitres::Fr => "fr",
        PrefSousTitres::En => "en",
    };
    meilleure(pistes, |p| {
        if famille_piste(p).as_deref() != Some(voulue) {
            return 0;
        }
        1 + if est_forcee(p) { 0 } else { 2 }
            + if malentendants(p.label.as_deref().unwrap_or("")) {
                0
            } else {
                1
            }
    })
    .map(|p| p.n)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PistesFilm {
    pub audio: Vec<PisteFilm>,
    pub sous_titres: Vec<PisteFilm>,
    pub audio_n: i64,
    pub sous_titres_n: Option<i64>,
    pub qualites: Vec<Qualite>,
    pub qualite: Qualite,
    pub intro: Option<Intro>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DemandeFilm {
    Detail {
        tour: u64,
        reference: String,
    },
    Ouvrir {
        tour: u64,
        reference: String,
        demande: DemandeLecture,
    },
    Fermer(String),
    Progression {
        reference: String,
        position: f64,
        duree: f64,
    },
    Depot(Option<String>),
    FermerDepot,
    Preferences(PreferencesFilm),
    Reveiller,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReponseFilm {
    Detail {
        tour: u64,
        audio: Vec<PisteFilm>,
    },
    Flux {
        tour: u64,
        resultat: Result<Box<FluxFilm>, ErreurApi>,
    },
}

pub struct LecteurFilm {
    lecteur: Arc<Lecteur>,
    base: String,
    jeton: Option<String>,
    relais: Option<String>,
    prefs: PreferencesFilm,
    reference: String,
    audio: i64,
    sous_titres: Option<i64>,
    qualite: Qualite,
    flux: Option<FluxFilm>,
    decalage: f64,
    forcer_hls: bool,
    reessaye: bool,
    parti: bool,
    tour: u64,
    enregistre_a: Option<i64>,
    position: f64,
    guet: Option<i64>,
    audio_en_attente: bool,
    sous_titres_en_attente: bool,
    sous_titres_a_appliquer: bool,
    ouverture: (f64, bool),
    depot: Option<Option<String>>,
    demandes: Vec<DemandeFilm>,
    erreurs: Vec<String>,
    pistes: Option<PistesFilm>,
    maintenant: i64,
    dernier: EtatLecture,
    phase: Phase,
}

impl LecteurFilm {
    pub fn nouveau(lecteur: Arc<Lecteur>, base: impl Into<String>, prefs: PreferencesFilm) -> Self {
        Self {
            lecteur,
            base: base.into(),
            jeton: None,
            relais: None,
            qualite: prefs.qualite,
            prefs,
            reference: String::new(),
            audio: 0,
            sous_titres: None,
            flux: None,
            decalage: 0.0,
            forcer_hls: false,
            reessaye: false,
            parti: false,
            tour: 0,
            enregistre_a: None,
            position: 0.0,
            guet: None,
            audio_en_attente: false,
            sous_titres_en_attente: false,
            sous_titres_a_appliquer: false,
            ouverture: (0.0, false),
            depot: None,
            demandes: Vec::new(),
            erreurs: Vec::new(),
            pistes: None,
            maintenant: 0,
            dernier: EtatLecture::NonDemarre,
            phase: Phase::Vide,
        }
    }

    pub fn avec_jeton(mut self, jeton: impl Into<String>) -> Self {
        self.jeton = Some(jeton.into());
        self
    }

    pub fn avec_relais(mut self, relais: impl Into<String>) -> Self {
        self.relais = Some(relais.into());
        self
    }

    pub fn preferences(&self) -> &PreferencesFilm {
        &self.prefs
    }

    pub fn pistes(&self) -> Option<&PistesFilm> {
        self.pistes.as_ref()
    }

    pub fn flux(&self) -> Option<&FluxFilm> {
        self.flux.as_ref()
    }

    pub fn decalage(&self) -> f64 {
        self.decalage
    }

    pub fn lecteur(&self) -> &Arc<Lecteur> {
        &self.lecteur
    }

    fn en_pause(&self) -> bool {
        self.lecteur.etat().en_pause
    }

    fn total(&self) -> f64 {
        match &self.flux {
            Some(f) if f.duration > 0.0 => f.duration,
            _ => self.lecteur.etat().duree.map_or(0.0, |d| d + self.decalage),
        }
    }

    fn enregistrer(&mut self) {
        let duree = self.total();
        if self.reference.is_empty() || self.position <= 0.0 || !duree.is_finite() || duree <= 0.0 {
            return;
        }
        self.enregistre_a = Some(self.maintenant);
        self.demandes.push(DemandeFilm::Progression {
            reference: self.reference.clone(),
            position: self.position,
            duree,
        });
    }

    fn fermer(&mut self) {
        if let Some(f) = self.flux.take() {
            self.demandes.push(DemandeFilm::Fermer(f.stream_id));
        }
    }

    fn ouvrir(&mut self, depart: f64, lecture: bool) {
        self.tour += 1;
        self.guet = None;
        self.fermer();
        self.ouverture = (depart, lecture);
        if self.audio_en_attente {
            if self.prefs.audio == PrefAudio::Auto {
                self.audio = 0;
                self.audio_en_attente = false;
            } else {
                self.demandes.push(DemandeFilm::Detail {
                    tour: self.tour,
                    reference: self.reference.clone(),
                });
                return;
            }
        }
        self.demander();
    }

    fn demander(&mut self) {
        self.demandes.push(DemandeFilm::Ouvrir {
            tour: self.tour,
            reference: self.reference.clone(),
            demande: DemandeLecture {
                audio: self.audio,
                quality: self.qualite,
                start: self.ouverture.0,
                support: if self.forcer_hls {
                    Support::default()
                } else {
                    support(&self.prefs.codecs_refuses)
                },
            },
        });
    }

    fn repli(&mut self, message: &str, media: bool) {
        self.guet = None;
        if self.parti {
            return;
        }
        let copie = self
            .flux
            .as_ref()
            .filter(|f| f.mode == ModeFlux::Hls)
            .and_then(|f| f.video.as_ref())
            .filter(|v| v.copy)
            .map(|v| v.codec.clone());
        if let Some(codec) = copie.filter(|c| {
            !self.reference.is_empty()
                && media
                && c != "h264"
                && !self.prefs.codecs_refuses.contains(c)
        }) {
            self.prefs.codecs_refuses.push(codec);
            self.demandes
                .push(DemandeFilm::Preferences(self.prefs.clone()));
            self.ouvrir(self.position, true);
            return;
        }
        if self.reference.is_empty() || self.reessaye {
            self.erreurs.push(message.to_string());
            return;
        }
        self.reessaye = true;
        self.forcer_hls = true;
        self.ouvrir(self.position, true);
    }

    fn source(&self, flux: &FluxFilm, depart: f64, lecture: bool) -> Source {
        let par_relais = flux.mode == ModeFlux::Hls && self.relais.is_some();
        let base = match (&self.relais, par_relais) {
            (Some(r), true) => r.as_str(),
            _ => self.base.as_str(),
        };
        let sous_titres = flux
            .subtitles
            .iter()
            .map(|s| format!("{}{}", self.base, s.url))
            .collect();
        let mut source = Source::adresse(format!("{base}{}", flux.url))
            .a_partir_de(depart)
            .avec_sous_titres(sous_titres, self.decalage);
        if let (Some(j), false) = (&self.jeton, par_relais) {
            source = source.avec_jeton(j.clone());
        }
        if lecture {
            source
        } else {
            source.en_pause()
        }
    }

    fn appliquer_sous_titres(&mut self) {
        let e = self.lecteur.etat();
        if e.phase != Phase::Pret {
            return;
        }
        self.sous_titres_a_appliquer = false;
        let piste = self.sous_titres.and_then(|n| {
            let rang = self
                .flux
                .as_ref()
                .and_then(|f| f.subtitles.iter().position(|s| s.n == n))
                .unwrap_or(n as usize);
            e.pistes_de(GenrePiste::SousTitres)
                .filter(|p| p.externe)
                .nth(rang)
                .map(|p| p.id)
        });
        self.lecteur.choisir_sous_titres(piste);
    }

    fn flux_recu(&mut self, flux: FluxFilm) {
        if flux.mode == ModeFlux::Hls {
            let cle = flux.cache.clone();
            if self.depot.is_none() || cle.is_none() || self.depot.as_ref() != Some(&cle) {
                self.demandes.push(DemandeFilm::Depot(cle.clone()));
                self.depot = Some(cle);
            }
        }
        let (depart, lecture) = self.ouverture;
        if flux.mode == ModeFlux::Direct && lecture {
            self.guet = Some(self.maintenant + ATTENTE_DIRECT_MS);
        }
        self.decalage = flux.offset.unwrap_or(0.0);
        self.position = depart;
        self.enregistre_a = Some(self.maintenant);
        if self.sous_titres_en_attente {
            let audio = flux.audio.iter().find(|a| a.n == self.audio);
            self.sous_titres =
                choisir_piste_sous_titres(&flux.subtitles, self.prefs.sous_titres, audio);
            self.sous_titres_en_attente = false;
        }
        self.pistes = Some(PistesFilm {
            audio: flux.audio.clone(),
            sous_titres: flux.subtitles.clone(),
            audio_n: self.audio,
            sous_titres_n: self.sous_titres,
            qualites: flux.qualities.clone().unwrap_or_default(),
            qualite: flux.quality.unwrap_or(self.qualite),
            intro: flux.intro,
        });
        let source = self.source(&flux, (depart - self.decalage).max(0.0), lecture);
        self.flux = Some(flux);
        self.sous_titres_a_appliquer = true;
        self.phase = Phase::Chargement;
        self.lecteur.charger(source);
    }
}

impl LecteurSeance for LecteurFilm {
    fn charger(&mut self, reference: &str, depart: f64, lecture: bool) {
        self.enregistrer();
        self.reference = reference.to_string();
        self.position = depart;
        self.audio = 0;
        self.sous_titres = None;
        self.audio_en_attente = true;
        self.sous_titres_en_attente = true;
        self.decalage = 0.0;
        self.qualite = self.prefs.qualite;
        self.forcer_hls = false;
        self.reessaye = false;
        self.ouvrir(depart, lecture);
    }

    fn lire(&mut self) {
        self.lecteur.lire();
    }

    fn pause(&mut self) {
        self.lecteur.pause();
    }

    fn aller(&mut self, position: f64) {
        let pret = self.lecteur.etat().duree.unwrap_or(0.0);
        let rouvrir = self.flux.as_ref().is_some_and(|f| {
            f.mode == ModeFlux::Hls
                && f.offset.is_some()
                && besoin_de_rouvrir(position, self.decalage, pret)
        });
        if rouvrir {
            self.position = position;
            let lecture = !self.en_pause();
            self.ouvrir(position, lecture);
            return;
        }
        self.lecteur.aller((position - self.decalage).max(0.0));
    }

    fn temps(&self) -> f64 {
        self.lecteur.position() + self.decalage
    }

    fn vitesse(&self) -> f64 {
        self.lecteur.etat().vitesse
    }

    fn regler_vitesse(&mut self, vitesse: f64) {
        self.lecteur.definir_vitesse(vitesse);
    }

    fn etat(&self) -> EtatLecture {
        etat_de(&self.lecteur.etat())
    }

    fn pret(&self) -> Option<bool> {
        Some(pret_de(&self.lecteur.etat(), Instant::now()))
    }

    fn duree(&self) -> f64 {
        self.total()
    }

    fn plages(&self) -> Vec<Plage> {
        plages_de(&self.lecteur.etat(), Instant::now())
            .into_iter()
            .map(|(a, b)| (a + self.decalage, b + self.decalage))
            .collect()
    }

    fn regler_volume(&mut self, volume: u8) {
        self.lecteur.definir_son(volume, volume == 0);
    }

    fn choisir_audio(&mut self, piste: i64) {
        self.audio = piste;
        self.audio_en_attente = false;
        let (t, lecture) = (self.temps(), !self.en_pause());
        self.ouvrir(t, lecture);
    }

    fn choisir_sous_titres(&mut self, piste: Option<i64>) {
        self.sous_titres = piste;
        self.sous_titres_en_attente = false;
        if let Some(p) = &mut self.pistes {
            p.sous_titres_n = piste;
        }
        self.appliquer_sous_titres();
    }

    fn choisir_qualite(&mut self, qualite: Qualite) {
        self.qualite = qualite;
        self.prefs.qualite = qualite;
        self.demandes
            .push(DemandeFilm::Preferences(self.prefs.clone()));
        let (t, lecture) = (self.temps(), !self.en_pause());
        self.ouvrir(t, lecture);
    }

    fn regler_tampon(&mut self, tampon: PrefTampon) {
        self.prefs.tampon = tampon;
        self.demandes
            .push(DemandeFilm::Preferences(self.prefs.clone()));
        self.demandes.push(DemandeFilm::Reveiller);
    }

    fn regler_wifi_seulement(&mut self, actif: bool) {
        self.prefs.wifi_seulement = actif;
        self.demandes
            .push(DemandeFilm::Preferences(self.prefs.clone()));
        self.demandes.push(DemandeFilm::Reveiller);
    }

    fn suivre(&mut self, maintenant_ms: i64) -> Option<EtatLecture> {
        self.maintenant = maintenant_ms;
        if self.parti {
            return None;
        }
        let e = self.lecteur.etat();
        let phase = std::mem::replace(&mut self.phase, e.phase);
        if e.phase == Phase::Erreur && phase != Phase::Erreur {
            self.repli(
                "Cette vidéo ne peut pas être lue (format ou accès refusé par le site qui l'héberge).",
                false,
            );
        }
        if e.phase == Phase::Pret && phase != Phase::Pret {
            if let Some(f) = &self.flux {
                let sans_image = !e.a_de_la_video();
                let copie = f.video.as_ref().is_some_and(|v| v.copy);
                match f.mode {
                    ModeFlux::Direct if sans_image => self.repli(MESSAGE_ILLISIBLE, false),
                    ModeFlux::Hls if copie && sans_image => self.repli(MESSAGE_ILLISIBLE, true),
                    _ => {}
                }
            }
        }
        if self.sous_titres_a_appliquer {
            self.appliquer_sous_titres();
        }
        let st = etat_de(&self.lecteur.etat());
        let precedent = std::mem::replace(&mut self.dernier, st);
        if st == EtatLecture::Lecture {
            self.reessaye = false;
            self.guet = None;
        }
        let e = self.lecteur.etat();
        if e.phase == Phase::Pret {
            let t = e.position_a(Instant::now());
            if t > 0.0 {
                self.position = t + self.decalage;
            }
        }
        let mis_en_pause = st == EtatLecture::Pause
            && matches!(precedent, EtatLecture::Lecture | EtatLecture::Tampon)
            && !e.recherche;
        if self.flux.is_some()
            && (mis_en_pause
                || (st == EtatLecture::Lecture
                    && doit_enregistrer(self.enregistre_a, maintenant_ms)))
        {
            self.enregistrer();
        }
        if self.guet.is_some_and(|t| maintenant_ms >= t) {
            let sans_image = e.phase != Phase::Pret || e.taille_video().is_none();
            self.guet = None;
            if self
                .flux
                .as_ref()
                .is_some_and(|f| f.mode == ModeFlux::Direct)
                && sans_image
            {
                self.repli(
                    &format!(
                        "aucune image après {} s en lecture directe",
                        ATTENTE_DIRECT_MS / 1000
                    ),
                    false,
                );
            }
        }
        (precedent != st).then_some(st)
    }

    fn erreur(&mut self) -> Option<String> {
        (!self.erreurs.is_empty()).then(|| self.erreurs.remove(0))
    }

    fn demandes(&mut self) -> Vec<DemandeFilm> {
        std::mem::take(&mut self.demandes)
    }

    fn repondre(&mut self, reponse: ReponseFilm, maintenant_ms: i64) {
        self.maintenant = maintenant_ms;
        match reponse {
            ReponseFilm::Detail { tour, audio } => {
                if self.parti || tour != self.tour {
                    return;
                }
                if self.audio_en_attente {
                    self.audio = choisir_piste_audio(&audio, self.prefs.audio);
                    self.audio_en_attente = false;
                }
                self.demander();
            }
            ReponseFilm::Flux { tour, resultat } => {
                if self.parti || tour != self.tour {
                    if let Ok(f) = resultat {
                        self.demandes.push(DemandeFilm::Fermer(f.stream_id));
                    }
                    return;
                }
                match resultat {
                    Err(e) if e.statut() == 400 && self.qualite != Qualite::Originale => {
                        self.qualite = Qualite::Originale;
                        self.demander();
                    }
                    Err(e) => {
                        let message = match e.message() {
                            "" => MESSAGE_DEMARRAGE,
                            m => m,
                        };
                        self.erreurs.push(message.to_string());
                    }
                    Ok(flux) => self.flux_recu(*flux),
                }
            }
        }
    }

    fn detruire(&mut self) {
        self.parti = true;
        self.guet = None;
        self.enregistrer();
        self.fermer();
        self.lecteur.arreter();
        self.demandes.push(DemandeFilm::FermerDepot);
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct DetailFilm {
    audio: Vec<PisteFilm>,
}

impl Client {
    pub async fn lecture_film(
        &self,
        titre: &str,
        demande: &DemandeLecture,
    ) -> Result<FluxFilm, ErreurApi> {
        self.post(
            &format!("/titles/{}/playback", encoder_composant(titre)),
            demande,
        )
        .await
    }

    pub async fn pistes_audio_film(&self, titre: &str) -> Result<Vec<PisteFilm>, ErreurApi> {
        let d: DetailFilm = self
            .get(&format!("/titles/{}", encoder_composant(titre)))
            .await?;
        Ok(d.audio)
    }

    pub async fn progression_film(
        &self,
        titre: &str,
        position: f64,
        duree: f64,
    ) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::PUT,
            &format!("/titles/{}/progress", encoder_composant(titre)),
            &Corps::json(&serde_json::json!({"position": position, "duration": duree}))?,
        )
        .await
    }

    pub async fn fermer_flux_film(&self, flux: &str) -> Result<(), ErreurApi> {
        self.appeler(
            Method::DELETE,
            &format!("/streams/{}", encoder_composant(flux)),
            &Corps::Aucun,
            OptionsAppel {
                discret: true,
                ..OptionsAppel::default()
            },
        )
        .await
        .map(|_| ())
    }

    pub async fn executer_demande_film(&self, demande: DemandeFilm) -> Option<ReponseFilm> {
        match demande {
            DemandeFilm::Detail { tour, reference } => Some(ReponseFilm::Detail {
                tour,
                audio: self.pistes_audio_film(&reference).await.unwrap_or_default(),
            }),
            DemandeFilm::Ouvrir {
                tour,
                reference,
                demande,
            } => Some(ReponseFilm::Flux {
                tour,
                resultat: self.lecture_film(&reference, &demande).await.map(Box::new),
            }),
            DemandeFilm::Fermer(flux) => {
                let _ = self.fermer_flux_film(&flux).await;
                None
            }
            DemandeFilm::Progression {
                reference,
                position,
                duree,
            } => {
                let _ = self.progression_film(&reference, position, duree).await;
                None
            }
            DemandeFilm::Depot(_)
            | DemandeFilm::FermerDepot
            | DemandeFilm::Preferences(_)
            | DemandeFilm::Reveiller => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Refus;
    use crate::lecteur::{CommandeMoteur, MediaFactice, MoteurFactice};
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use serde_json::json;

    const BASE: &str = "https://entracte.test";

    fn flux(n: u32, mode: ModeFlux) -> FluxFilm {
        let fin = if mode == ModeFlux::Direct {
            "direct"
        } else {
            "hls.m3u8"
        };
        FluxFilm {
            stream_id: format!("s{n}"),
            mode,
            url: format!("/stream/j{n}/{fin}"),
            duration: 3000.0,
            ..FluxFilm::default()
        }
    }

    fn piste(n: i64, label: Option<&str>, langue: Option<&str>) -> PisteFilm {
        PisteFilm {
            n,
            label: label.map(str::to_string),
            language: langue.map(str::to_string),
            ..PisteFilm::default()
        }
    }

    struct Banc {
        f: LecteurFilm,
        m: MoteurFactice,
        demandes: Vec<DemandeFilm>,
        maintenant: i64,
    }

    impl Banc {
        fn nouveau(prefs: PreferencesFilm) -> Self {
            let (lecteur, m) = Lecteur::factice();
            Self {
                f: LecteurFilm::nouveau(Arc::new(lecteur), BASE, prefs).avec_jeton("jeton"),
                m,
                demandes: Vec::new(),
                maintenant: 1_000_000,
            }
        }

        fn media(&self, f: &FluxFilm, media: MediaFactice) {
            self.m.ajouter_media(&format!("{BASE}{}", f.url), media);
        }

        fn ramasser(&mut self) -> Vec<DemandeFilm> {
            let nouvelles = self.f.demandes();
            self.demandes.extend(nouvelles.clone());
            nouvelles
        }

        fn ouvertures(&self) -> Vec<(u64, DemandeLecture)> {
            self.demandes
                .iter()
                .filter_map(|d| match d {
                    DemandeFilm::Ouvrir { tour, demande, .. } => Some((*tour, demande.clone())),
                    _ => None,
                })
                .collect()
        }

        fn derniere(&self) -> (u64, DemandeLecture) {
            self.ouvertures().pop().expect("aucune ouverture")
        }

        fn servir(&mut self, f: FluxFilm) {
            self.ramasser();
            let (tour, _) = self.derniere();
            self.f.repondre(
                ReponseFilm::Flux {
                    tour,
                    resultat: Ok(Box::new(f)),
                },
                self.maintenant,
            );
            self.suivre();
        }

        fn suivre(&mut self) -> Option<EtatLecture> {
            let r = self.f.suivre(self.maintenant);
            self.ramasser();
            r
        }

        fn chargee(&self) -> Source {
            self.m
                .commandes()
                .into_iter()
                .rev()
                .find_map(|c| match c {
                    CommandeMoteur::Charger(s) => Some(s),
                    _ => None,
                })
                .expect("aucun chargement")
        }

        fn fermes(&self) -> Vec<String> {
            self.demandes
                .iter()
                .filter_map(|d| match d {
                    DemandeFilm::Fermer(s) => Some(s.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    #[test]
    fn l_intro_du_flux_est_transmise_sans_deplacer_la_lecture() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = FluxFilm {
            intro: Some(Intro {
                debut: 30.0,
                fin: 90.0,
            }),
            ..flux(1, ModeFlux::Direct)
        };
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("e1", 45.0, true);
        b.servir(f);
        assert_eq!(
            b.f.pistes().unwrap().intro,
            Some(Intro {
                debut: 30.0,
                fin: 90.0
            })
        );
        assert!(b.f.temps() < 90.0);
        assert!(!b
            .m
            .commandes()
            .iter()
            .any(|c| matches!(c, CommandeMoteur::Aller(_))));
        assert!(Intro {
            debut: 30.0,
            fin: 90.0
        }
        .contient(45.0));
    }

    #[test]
    fn une_intro_absente_est_annoncee_vide() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Direct);
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("e1", 0.0, true);
        b.servir(f);
        assert_eq!(b.f.pistes().unwrap().intro, None);
    }

    #[test]
    fn demande_un_flux_signe_annonce_les_pistes_et_ferme_le_flux_en_quittant() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = FluxFilm {
            stream_id: "f1".into(),
            audio: vec![
                piste(0, Some("Français"), Some("fra")),
                piste(1, Some("Anglais"), Some("eng")),
            ],
            subtitles: vec![PisteFilm {
                url: "/films/stream/t/sub/0".into(),
                ..piste(0, Some("Français"), Some("fra"))
            }],
            ..flux(1, ModeFlux::Direct)
        };
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("e1", 0.0, true);
        b.ramasser();
        let (_, d) = b.derniere();
        assert_eq!(d.audio, 0);
        assert!(d.support.video.contains(&"hevc".to_string()));
        b.servir(f);
        let p = b.f.pistes().unwrap();
        assert_eq!((p.audio.len(), p.sous_titres.len(), p.audio_n), (2, 1, 0));
        let s = b.chargee();
        assert_eq!(s.adresse, format!("{BASE}/stream/j1/direct"));
        assert_eq!(s.jeton.as_deref(), Some("jeton"));
        assert_eq!(s.sous_titres, [format!("{BASE}/films/stream/t/sub/0")]);
        b.f.detruire();
        b.ramasser();
        assert_eq!(b.fermes(), ["f1"]);
        assert!(b.demandes.contains(&DemandeFilm::FermerDepot));
    }

    #[test]
    fn une_piste_audio_choisie_rouvre_le_flux_propre_au_spectateur() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = FluxFilm {
            stream_id: "f1".into(),
            ..flux(1, ModeFlux::Direct)
        };
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("e1", 0.0, true);
        b.servir(f);
        b.f.choisir_audio(1);
        b.ramasser();
        let (_, d) = b.derniere();
        assert_eq!((d.audio, b.ouvertures().len()), (1, 2));
        assert_eq!(b.fermes(), ["f1"]);
    }

    #[test]
    fn une_panne_rouvre_une_fois_en_transcodage_a_la_meme_position_puis_s_affiche() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let direct = flux(1, ModeFlux::Direct);
        b.m.ajouter_echec(&format!("{BASE}{}", direct.url), "HTTP 500");
        b.f.charger("e1", 120.0, true);
        b.servir(direct);
        let (_, d) = b.derniere();
        assert_eq!(b.ouvertures().len(), 2);
        assert_eq!(
            (d.start, d.support, d.audio),
            (120.0, Support::default(), 0)
        );
        assert_eq!(b.f.erreur(), None);
        let hls = flux(2, ModeFlux::Hls);
        b.m.ajouter_echec(&format!("{BASE}{}", hls.url), "HTTP 500");
        b.servir(hls);
        assert!(b.f.erreur().is_some());
        assert_eq!(b.f.erreur(), None);
        assert_eq!(b.ouvertures().len(), 2);
    }

    #[test]
    fn changer_de_palier_rouvre_a_la_meme_position_et_le_memorise() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = FluxFilm {
            offset: Some(0.0),
            quality: Some(Qualite::Originale),
            qualities: Some(vec![Qualite::Originale, Qualite::P720]),
            ..flux(1, ModeFlux::Hls)
        };
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("e1", 0.0, true);
        b.ramasser();
        let (_, d) = b.derniere();
        assert_eq!((d.quality, d.start), (Qualite::Originale, 0.0));
        b.servir(f);
        let p = b.f.pistes().unwrap();
        assert_eq!(p.qualites, [Qualite::Originale, Qualite::P720]);
        b.m.emettre(crate::lecteur::Evenement::Position(42.0));
        b.f.choisir_qualite(Qualite::P720);
        b.ramasser();
        let (_, d) = b.derniere();
        assert_eq!(d.quality, Qualite::P720);
        assert!((d.start - 42.0).abs() < 0.1);
        assert!(b
            .demandes
            .iter()
            .any(|x| matches!(x, DemandeFilm::Preferences(p) if p.qualite == Qualite::P720)));
        assert_eq!(b.f.preferences().qualite, Qualite::P720);
        let suivant = Banc::nouveau(b.f.preferences().clone());
        assert_eq!(suivant.f.qualite, Qualite::P720);
        assert_eq!(
            b.f.preferences()
                .qualite_proposee(Some(&[Qualite::Originale])),
            Qualite::Originale
        );
    }

    #[test]
    fn un_palier_refuse_par_le_serveur_revient_a_l_original() {
        let mut b = Banc::nouveau(PreferencesFilm {
            qualite: Qualite::P480,
            ..PreferencesFilm::default()
        });
        b.f.charger("e1", 0.0, true);
        b.ramasser();
        let (tour, d) = b.derniere();
        assert_eq!(d.quality, Qualite::P480);
        let refus = Refus {
            statut: 400,
            code: "bad".into(),
            message: "palier".into(),
            donnees: Default::default(),
        };
        b.f.repondre(
            ReponseFilm::Flux {
                tour,
                resultat: Err(ErreurApi::Autre(refus)),
            },
            0,
        );
        b.ramasser();
        assert_eq!(b.derniere().1.quality, Qualite::Originale);
        assert_eq!(b.f.erreur(), None);
    }

    #[test]
    fn un_flux_hls_commencant_a_la_reprise_donne_la_position_reelle_et_un_saut_hors_flux_le_rouvre()
    {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = FluxFilm {
            offset: Some(600.0),
            ..flux(1, ModeFlux::Hls)
        };
        b.media(&f, MediaFactice::video(1200.0));
        b.f.charger("e1", 600.0, true);
        b.servir(f);
        assert_eq!(b.chargee().depart, 0.0);
        b.m.emettre(crate::lecteur::Evenement::Position(10.0));
        assert!((b.f.temps() - 610.0).abs() < 0.1);
        b.f.aller(700.0);
        assert!(b.m.commandes().contains(&CommandeMoteur::Aller(100.0)));
        b.f.aller(100.0);
        b.ramasser();
        assert_eq!(b.derniere().1.start, 100.0);
        assert_eq!(b.ouvertures().len(), 2);
    }

    #[test]
    fn la_progression_est_enregistree_a_la_pause_et_en_quittant() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Direct);
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("e1", 120.0, true);
        b.servir(f);
        b.f.pause();
        b.suivre();
        let progressions: Vec<_> = b
            .demandes
            .iter()
            .filter_map(|d| match d {
                DemandeFilm::Progression {
                    reference,
                    position,
                    duree,
                } => Some((reference.clone(), *position, *duree)),
                _ => None,
            })
            .collect();
        assert_eq!(progressions.len(), 1);
        assert_eq!(
            (progressions[0].0.as_str(), progressions[0].2),
            ("e1", 3000.0)
        );
        assert!((progressions[0].1 - 120.0).abs() < 0.1);
        b.f.detruire();
        b.ramasser();
        let n = b
            .demandes
            .iter()
            .filter(|d| matches!(d, DemandeFilm::Progression { .. }))
            .count();
        assert_eq!(n, 2);
        assert!(doit_enregistrer(None, 0));
        assert!(!doit_enregistrer(Some(0), 14_999));
        assert!(doit_enregistrer(Some(0), 15_000));
    }

    #[test]
    fn un_refus_du_serveur_est_montre_au_spectateur() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        b.f.charger("e1", 0.0, true);
        b.ramasser();
        let (tour, _) = b.derniere();
        let refus = Refus {
            statut: 503,
            code: "serveur_charge".into(),
            message: "le serveur est trop chargé".into(),
            donnees: Default::default(),
        };
        b.f.repondre(
            ReponseFilm::Flux {
                tour,
                resultat: Err(ErreurApi::Serveur(refus)),
            },
            0,
        );
        assert_eq!(b.f.erreur().as_deref(), Some("le serveur est trop chargé"));
        b.f.repondre(
            ReponseFilm::Flux {
                tour,
                resultat: Err(ErreurApi::RequeteInvalide(String::new())),
            },
            0,
        );
        assert_eq!(b.f.erreur().as_deref(), Some(MESSAGE_DEMARRAGE));
    }

    #[test]
    fn libmpv_annonce_ses_codecs_moins_ceux_que_l_appareil_a_refuses() {
        let s = support(&[]);
        assert_eq!(s.containers, ["mp4", "webm", "mkv"]);
        assert_eq!(s.video, ["h264", "hevc", "hevc10", "vp9", "av1"]);
        assert_eq!(support(&["hevc".into()]).video, ["h264", "vp9", "av1"]);
    }

    #[test]
    fn rouvre_avec_la_piste_choisie_a_la_position_demandee_meme_avant_le_chargement() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f1 = flux(1, ModeFlux::Direct);
        let f2 = flux(2, ModeFlux::Direct);
        b.media(&f1, MediaFactice::video(3000.0));
        b.media(&f2, MediaFactice::video(3000.0));
        b.f.charger("f", 600.0, true);
        b.servir(f1);
        b.f.choisir_audio(1);
        b.ramasser();
        assert_eq!(b.derniere().1.audio, 1);
        b.servir(f2);
        let s = b.chargee();
        assert_eq!(s.adresse, format!("{BASE}/stream/j2/direct"));
        assert!((s.depart - 600.0).abs() < 0.1);
        assert!(!s.en_pause);
    }

    #[test]
    fn ouvre_le_titre_avec_la_langue_et_les_sous_titres_choisis_par_defaut() {
        let mut b = Banc::nouveau(PreferencesFilm {
            audio: PrefAudio::Vff,
            sous_titres: PrefSousTitres::Forces,
            ..PreferencesFilm::default()
        });
        let audio = vec![
            piste(0, Some("English"), Some("eng")),
            piste(1, Some("VFF"), Some("fre")),
        ];
        let sous_titres = vec![
            PisteFilm {
                url: "/s/0".into(),
                ..piste(0, Some("English"), Some("eng"))
            },
            PisteFilm {
                url: "/s/1".into(),
                forced: true,
                ..piste(1, Some("Forcés"), Some("fre"))
            },
        ];
        b.f.charger("f", 0.0, true);
        b.ramasser();
        assert!(b.ouvertures().is_empty());
        let tour = match &b.demandes[0] {
            DemandeFilm::Detail { tour, reference } if reference == "f" => *tour,
            autre => panic!("{autre:?}"),
        };
        b.f.repondre(
            ReponseFilm::Detail {
                tour,
                audio: audio.clone(),
            },
            0,
        );
        b.ramasser();
        assert_eq!(b.ouvertures().len(), 1);
        assert_eq!(b.derniere().1.audio, 1);
        let f = FluxFilm {
            audio,
            subtitles: sous_titres,
            ..flux(1, ModeFlux::Direct)
        };
        b.media(&f, MediaFactice::video(3000.0));
        b.servir(f);
        let p = b.f.pistes().unwrap();
        assert_eq!((p.audio_n, p.sous_titres_n), (1, Some(1)));
        let e = b.f.lecteur().etat();
        let externes: Vec<i64> = e
            .pistes_de(GenrePiste::SousTitres)
            .filter(|p| p.externe)
            .map(|p| p.id)
            .collect();
        assert_eq!(externes.len(), 2);
        assert_eq!(
            b.m.commandes().last(),
            Some(&CommandeMoteur::SousTitres(Some(externes[1])))
        );
        b.f.choisir_sous_titres(None);
        assert_eq!(
            b.m.commandes().last(),
            Some(&CommandeMoteur::SousTitres(None))
        );
    }

    #[test]
    fn la_piste_choisie_a_la_main_pendant_l_ouverture_est_gardee() {
        let mut b = Banc::nouveau(PreferencesFilm {
            audio: PrefAudio::Vff,
            ..PreferencesFilm::default()
        });
        b.f.charger("f", 0.0, true);
        b.f.choisir_audio(2);
        b.ramasser();
        let ancien = match &b.demandes[0] {
            DemandeFilm::Detail { tour, .. } => *tour,
            autre => panic!("{autre:?}"),
        };
        b.f.repondre(
            ReponseFilm::Detail {
                tour: ancien,
                audio: vec![piste(1, Some("VFF"), Some("fre"))],
            },
            0,
        );
        b.ramasser();
        assert_eq!(b.ouvertures().len(), 1);
        assert_eq!(b.derniere().1.audio, 2);
    }

    #[test]
    fn choix_des_pistes_selon_les_preferences() {
        let pistes = vec![
            piste(0, Some("Anglais"), Some("eng")),
            piste(1, Some("Français (VFQ)"), Some("fre")),
            piste(2, Some("Français (VFF)"), Some("fre")),
            piste(3, Some("Commentaires"), Some("fre")),
        ];
        assert_eq!(choisir_piste_audio(&pistes, PrefAudio::Auto), 0);
        assert_eq!(choisir_piste_audio(&pistes, PrefAudio::Vff), 2);
        assert_eq!(choisir_piste_audio(&pistes, PrefAudio::Vfq), 1);
        assert_eq!(choisir_piste_audio(&pistes, PrefAudio::En), 0);
        assert_eq!(
            choisir_piste_audio(&[piste(5, Some("TrueFrench"), None)], PrefAudio::Vf),
            5
        );
        let st = vec![
            piste(0, Some("Français SDH"), Some("fre")),
            piste(1, Some("Français"), Some("fre")),
            PisteFilm {
                forced: true,
                ..piste(2, Some("Forcés"), Some("fre"))
            },
        ];
        assert_eq!(
            choisir_piste_sous_titres(&st, PrefSousTitres::Fr, None),
            Some(1)
        );
        assert_eq!(
            choisir_piste_sous_titres(&st, PrefSousTitres::En, None),
            None
        );
        assert_eq!(
            choisir_piste_sous_titres(&st, PrefSousTitres::Aucun, None),
            None
        );
        assert_eq!(
            choisir_piste_sous_titres(&st, PrefSousTitres::Forces, Some(&pistes[0])),
            None
        );
        assert_eq!(
            famille_piste(&piste(0, None, Some("fr-CA"))).as_deref(),
            Some("fr")
        );
        assert_eq!(
            famille_piste(&piste(0, None, Some("pt-BR"))).as_deref(),
            Some("pt")
        );
        assert_eq!(
            famille_piste(&piste(0, Some("vostfr"), None)).as_deref(),
            Some("fr")
        );
        assert_eq!(famille_piste(&piste(0, Some("inconnu"), None)), None);
    }

    #[test]
    fn rouvre_en_hls_une_lecture_directe_sans_piste_video() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Direct);
        b.media(&f, MediaFactice::son(3000.0));
        b.f.charger("f", 0.0, true);
        b.servir(f);
        assert_eq!(b.ouvertures().len(), 2);
        assert_eq!(b.derniere().1.support, Support::default());
    }

    #[test]
    fn garde_la_lecture_directe_quand_l_image_est_decodee() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Direct);
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("f", 0.0, true);
        b.servir(f);
        b.maintenant += ATTENTE_DIRECT_MS + 1;
        b.suivre();
        assert_eq!(b.ouvertures().len(), 1);
    }

    #[test]
    fn rouvre_en_hls_une_lecture_directe_restee_sans_image_apres_le_delai() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Direct);
        b.media(&f, MediaFactice::video(3000.0));
        b.m.suspendre_reponses(true);
        b.f.charger("f", 0.0, true);
        b.servir(f);
        b.maintenant += ATTENTE_DIRECT_MS - 1;
        b.suivre();
        assert_eq!(b.ouvertures().len(), 1);
        b.maintenant += 2;
        b.suivre();
        assert_eq!(b.ouvertures().len(), 2);
        assert_eq!(b.derniere().1.support, Support::default());
    }

    #[test]
    fn un_flux_hls_est_lu_par_libmpv_sans_lecteur_a_part() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Hls);
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("f", 0.0, true);
        b.servir(f);
        assert_eq!(b.chargee().adresse, format!("{BASE}/stream/j1/hls.m3u8"));
        assert_eq!(b.f.etat(), EtatLecture::Lecture);
    }

    fn copie(n: u32, copy: bool) -> FluxFilm {
        FluxFilm {
            video: Some(VideoFlux {
                codec: "hevc".into(),
                copy,
            }),
            ..flux(n, ModeFlux::Hls)
        }
    }

    #[test]
    fn une_video_recopiee_illisible_rouvre_un_flux_reencode_et_le_codec_n_est_plus_demande() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = copie(1, true);
        b.media(&f, MediaFactice::son(3000.0));
        b.f.charger("f", 120.0, true);
        b.ramasser();
        assert!(b.derniere().1.support.video.contains(&"hevc10".to_string()));
        b.servir(f);
        let (_, d) = b.derniere();
        assert_eq!((d.start, d.quality), (120.0, Qualite::Originale));
        assert_eq!(d.support.video, ["h264", "vp9", "av1"]);
        assert_eq!(b.f.preferences().codecs_refuses, ["hevc"]);
        assert_eq!(b.f.erreur(), None);
        let mut suivant = Banc::nouveau(b.f.preferences().clone());
        suivant.f.charger("g", 0.0, true);
        suivant.ramasser();
        assert!(!suivant
            .derniere()
            .1
            .support
            .video
            .contains(&"hevc".to_string()));
    }

    #[test]
    fn le_codec_n_est_pas_refuse_pour_une_erreur_reseau_ni_une_video_reencodee() {
        for (copy, panne) in [(true, true), (false, false)] {
            let mut b = Banc::nouveau(PreferencesFilm::default());
            let f = copie(1, copy);
            if panne {
                b.m.ajouter_echec(&format!("{BASE}{}", f.url), "HTTP 502");
            } else {
                b.media(&f, MediaFactice::son(3000.0));
            }
            b.f.charger("f", 0.0, true);
            let url = format!("{BASE}{}", f.url);
            b.servir(f);
            if !panne {
                b.m.ajouter_echec(&url, "HTTP 502");
                b.m.emettre(crate::lecteur::Evenement::Erreur("décodage".into()));
                b.suivre();
            }
            assert_eq!(b.ouvertures().len(), 2);
            assert!(b.f.preferences().codecs_refuses.is_empty());
            assert!(b.derniere().1.support.video.is_empty());
            assert_eq!(b.f.erreur(), None);
        }
    }

    #[test]
    fn le_depot_est_garde_pour_la_meme_cle_et_change_sinon() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let avec = |n: u32, cle: Option<&str>| FluxFilm {
            cache: cle.map(str::to_string),
            ..flux(n, ModeFlux::Hls)
        };
        for n in 1..=4 {
            b.media(&avec(n, None), MediaFactice::video(3000.0));
        }
        b.f.charger("f", 0.0, false);
        b.servir(avec(1, Some("f-480p-0-g")));
        b.f.choisir_audio(0);
        b.servir(avec(2, Some("f-480p-0-g")));
        b.f.choisir_qualite(Qualite::P720);
        b.servir(avec(3, Some("f-720p-0-g")));
        b.f.choisir_audio(0);
        b.servir(avec(4, None));
        let depots: Vec<_> = b
            .demandes
            .iter()
            .filter_map(|d| match d {
                DemandeFilm::Depot(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            depots,
            [
                Some("f-480p-0-g".to_string()),
                Some("f-720p-0-g".to_string()),
                None
            ]
        );
    }

    #[test]
    fn changer_le_reglage_de_prechargement_ne_rouvre_pas_le_flux() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = flux(1, ModeFlux::Hls);
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("f", 0.0, false);
        b.servir(f);
        b.f.regler_tampon(PrefTampon::Long);
        b.f.regler_wifi_seulement(true);
        let d = b.ramasser();
        assert_eq!(b.ouvertures().len(), 1);
        assert!(d.contains(&DemandeFilm::Reveiller));
        assert_eq!(b.f.preferences().tampon, PrefTampon::Long);
        assert!(b.f.preferences().wifi_seulement);
        assert_eq!(
            tampon_effectif(b.f.preferences(), Reseau::Mobile),
            PrefTampon::Normal
        );
        assert_eq!(
            tampon_effectif(b.f.preferences(), Reseau::Wifi),
            PrefTampon::Long
        );
        assert_eq!(
            tampon_effectif(b.f.preferences(), Reseau::Inconnu),
            PrefTampon::Long
        );
    }

    #[test]
    fn les_plages_d_un_flux_hls_sont_ramenees_au_temps_du_titre() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        let f = FluxFilm {
            offset: Some(1200.0),
            ..flux(1, ModeFlux::Hls)
        };
        b.media(&f, MediaFactice::video(3000.0));
        b.f.charger("f", 1203.0, false);
        b.servir(f);
        assert_eq!(b.chargee().depart, 3.0);
        assert!(b.f.plages().is_empty());
        b.m.emettre(crate::lecteur::Evenement::Position(0.0));
        b.m.emettre(crate::lecteur::Evenement::CacheJusqua(Some(64.0)));
        assert_eq!(b.f.plages(), [(1200.0, 1264.0)]);
        assert!(besoin_de_rouvrir(1100.0, 1200.0, 64.0));
        assert!(!besoin_de_rouvrir(1260.0, 1200.0, 64.0));
        assert!(besoin_de_rouvrir(1300.0, 1200.0, 64.0));
    }

    #[test]
    fn un_flux_arrive_trop_tard_est_ferme_aussitot() {
        let mut b = Banc::nouveau(PreferencesFilm::default());
        b.f.charger("f", 0.0, true);
        b.ramasser();
        let (ancien, _) = b.derniere();
        b.f.charger("g", 0.0, true);
        b.ramasser();
        b.f.repondre(
            ReponseFilm::Flux {
                tour: ancien,
                resultat: Ok(Box::new(FluxFilm {
                    stream_id: "vieux".into(),
                    ..flux(9, ModeFlux::Direct)
                })),
            },
            0,
        );
        b.ramasser();
        assert_eq!(b.fermes(), ["vieux"]);
        assert!(b.f.flux().is_none());
    }

    #[tokio::test]
    async fn les_appels_d_entracte_partent_aux_bons_chemins() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(200, json!({"stream_id": "s1", "mode": "hls", "url": "/stream/t/hls.m3u8", "duration": 60, "audio": [], "subtitles": [], "offset": 0, "cache": "c"})),
            Reponse::json(200, json!({"audio": [{"n": 1, "label": "VFF", "language": "fre"}], "subtitles": []})),
            Reponse::vide(204),
            Reponse::vide(204),
        ])
        .await;
        let client = serveur.client();
        let demande = DemandeLecture {
            audio: 1,
            quality: Qualite::P720,
            start: 12.0,
            support: support(&[]),
        };
        let Some(ReponseFilm::Flux { resultat, .. }) = client
            .executer_demande_film(DemandeFilm::Ouvrir {
                tour: 1,
                reference: "t 1".into(),
                demande,
            })
            .await
        else {
            panic!()
        };
        let f = resultat.unwrap();
        assert_eq!((f.mode, f.cache.as_deref()), (ModeFlux::Hls, Some("c")));
        let Some(ReponseFilm::Detail { audio, .. }) = client
            .executer_demande_film(DemandeFilm::Detail {
                tour: 1,
                reference: "t".into(),
            })
            .await
        else {
            panic!()
        };
        assert_eq!(audio[0].n, 1);
        client
            .executer_demande_film(DemandeFilm::Progression {
                reference: "t".into(),
                position: 12.5,
                duree: 60.0,
            })
            .await;
        client
            .executer_demande_film(DemandeFilm::Fermer("s1".into()))
            .await;
        let r = serveur.requetes();
        assert_eq!(r[0].ligne(), ("/titles/t%201/playback", "POST"));
        assert_eq!(
            r[0].json(),
            json!({"audio": 1, "quality": "720p", "start": 12.0, "support": {"containers": ["mp4", "webm", "mkv"], "video": ["h264", "hevc", "hevc10", "vp9", "av1"], "audio": ["aac", "mp3", "opus", "flac", "ac3", "eac3"]}})
        );
        assert_eq!(r[1].ligne(), ("/titles/t", "GET"));
        assert_eq!(r[2].ligne(), ("/titles/t/progress", "PUT"));
        assert_eq!(r[2].json(), json!({"position": 12.5, "duration": 60.0}));
        assert_eq!(r[3].ligne(), ("/streams/s1", "DELETE"));
    }
}
