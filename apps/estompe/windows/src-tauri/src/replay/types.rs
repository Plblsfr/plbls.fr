use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Resolution {
    #[serde(rename = "720p")]
    R720,
    #[serde(rename = "1080p")]
    R1080,
    #[serde(rename = "1440p")]
    R1440,
    #[serde(rename = "4k")]
    R4k,
    #[serde(rename = "native")]
    Native,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Qualite {
    Basse,
    Moyenne,
    Haute,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Hevc,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Reglages {
    pub actif: bool,
    pub resolution: Resolution,
    pub ips: u32,
    pub duree_s: u32,
    pub qualite: Qualite,
    pub micro: bool,
    pub micro_libelle: Option<String>,
    pub micro_reduction: String,
    pub micro_gain: f32,
    pub curseur: bool,
    pub dossier: Option<String>,
    pub quota_octets: u64,
    pub jeux_ajoutes: Vec<String>,
    pub jeux_exclus: Vec<String>,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            actif: false,
            resolution: Resolution::R1080,
            ips: 60,
            duree_s: 30,
            qualite: Qualite::Haute,
            micro: false,
            micro_libelle: None,
            micro_reduction: "strong".into(),
            micro_gain: 1.0,
            curseur: false,
            dossier: None,
            quota_octets: 20 * 1024 * 1024 * 1024,
            jeux_ajoutes: Vec::new(),
            jeux_exclus: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Disponibilite {
    pub disponible: bool,
    pub raison: Option<String>,
    pub encodeur: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ChoixResolution {
    pub id: Resolution,
    pub disponible: bool,
    pub raison: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ChoixCadence {
    pub ips: u32,
    pub disponible: bool,
    pub raison: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub resolutions: Vec<ChoixResolution>,
    pub cadences: Vec<ChoixCadence>,
    pub duree_max_s: u32,
    pub raison_duree: Option<String>,
    pub codec: Codec,
    pub debit_kbps: u32,
    pub memoire_octets: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EtatPublic {
    pub etat: &'static str,
    pub jeu: Option<String>,
    pub raison: Option<String>,
    pub codec: Option<Codec>,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub micro_par_defaut: bool,
}

impl EtatPublic {
    pub fn simple(etat: &'static str, raison: Option<String>) -> Self {
        Self { etat, jeu: None, raison, codec: None, largeur: 0, hauteur: 0, ips: 0, micro_par_defaut: false }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipEnregistre {
    pub chemin: String,
    pub nom: String,
    pub jeu: String,
    pub duree_s: f64,
    pub taille_octets: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Erreur {
    pub code: &'static str,
    pub message: String,
}

impl Erreur {
    pub fn nouvelle(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub chemin: String,
    pub nom: String,
    pub jeu: Option<String>,
    pub date: String,
    pub duree_s: f64,
    pub taille_octets: u64,
    pub vignette: Option<String>,
    pub codec: Option<Codec>,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub pistes_audio: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stockage {
    pub utilise_octets: u64,
    pub quota_octets: u64,
    pub depasse: bool,
    pub plus_anciens: Vec<String>,
}
