use std::path::PathBuf;

use estompe_plateforme::touche;
use serde::{Deserialize, Serialize};

use super::automate::Etat;
use super::tampon::Format;

pub use estompe_plateforme::CodecVideo as Codec;

pub const CADENCES: [u32; 4] = [30, 60, 120, 144];
pub const DUREE_MIN_S: u32 = 15;
pub const DUREE_MAX_S: u32 = 300;
pub const QUOTA_MIN: u64 = 1024 * 1024 * 1024;
pub const QUOTA_DEFAUT: u64 = 20 * QUOTA_MIN;
pub const TOUCHE_DEFAUT: &str = "F10";
pub const LONGUEUR_MAX_DOSSIER: usize = 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Resolution {
    #[serde(rename = "720p")]
    R720,
    #[default]
    #[serde(rename = "1080p")]
    R1080,
    #[serde(rename = "1440p")]
    R1440,
    #[serde(rename = "4k")]
    R4k,
    #[serde(rename = "native")]
    Native,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Qualite {
    Basse,
    Moyenne,
    #[default]
    Haute,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ReglagesReplay {
    pub actif: bool,
    pub resolution: Resolution,
    pub ips: u32,
    pub duree_s: u32,
    pub qualite: Qualite,
    pub micro: bool,
    pub curseur: bool,
    pub touche: String,
    pub dossier: Option<String>,
    pub quota_octets: u64,
}

impl Default for ReglagesReplay {
    fn default() -> Self {
        Self {
            actif: false,
            resolution: Resolution::R1080,
            ips: 60,
            duree_s: 30,
            qualite: Qualite::Haute,
            micro: false,
            curseur: false,
            touche: TOUCHE_DEFAUT.into(),
            dossier: None,
            quota_octets: QUOTA_DEFAUT,
        }
    }
}

impl ReglagesReplay {
    pub fn borne(self) -> Self {
        let ips = CADENCES
            .iter()
            .rev()
            .copied()
            .find(|c| *c <= self.ips)
            .unwrap_or(CADENCES[0]);
        let dossier = self.dossier.map(|d| d.trim().to_string()).filter(|d| {
            !d.is_empty()
                && d.chars().count() <= LONGUEUR_MAX_DOSSIER
                && std::path::Path::new(d).is_absolute()
        });
        let touche = if touche(&self.touche).is_some() {
            self.touche
        } else {
            TOUCHE_DEFAUT.into()
        };
        Self {
            ips,
            duree_s: self.duree_s.clamp(DUREE_MIN_S, DUREE_MAX_S),
            quota_octets: self.quota_octets.max(QUOTA_MIN),
            touche,
            dossier,
            ..self
        }
    }

    pub fn meme_capture(&self, autre: &Self) -> bool {
        self.resolution == autre.resolution
            && self.ips == autre.ips
            && self.duree_s == autre.duree_s
            && self.qualite == autre.qualite
            && self.micro == autre.micro
            && self.curseur == autre.curseur
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disponibilite {
    pub disponible: bool,
    pub raison: Option<String>,
    pub encodeur: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtatReplay {
    pub etat: Etat,
    pub jeu: Option<String>,
    pub format: Option<Format>,
    pub encodeur: Option<String>,
    pub raccourci: Option<String>,
}

impl Default for EtatReplay {
    fn default() -> Self {
        Self {
            etat: Etat::Inactif,
            jeu: None,
            format: None,
            encodeur: None,
            raccourci: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipEnregistre {
    pub chemin: PathBuf,
    pub nom: String,
    pub jeu: String,
    pub duree_s: f64,
    pub taille_octets: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErreurReplay {
    pub code: &'static str,
    pub message: String,
}

impl ErreurReplay {
    pub fn nouvelle(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvenementReplay {
    Clip(ClipEnregistre),
    Erreur(ErreurReplay),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub chemin: PathBuf,
    pub nom: String,
    pub jeu: Option<String>,
    pub date: String,
    pub duree_s: f64,
    pub taille_octets: u64,
    pub vignette: Option<PathBuf>,
    pub codec: Option<Codec>,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub pistes_audio: u32,
    pub origine: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stockage {
    pub utilise_octets: u64,
    pub quota_octets: u64,
    pub depasse: bool,
    pub plus_anciens: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_reglages_sont_bornes() {
        let r = ReglagesReplay {
            ips: 75,
            duree_s: 3,
            quota_octets: 10,
            touche: "Inconnue".into(),
            dossier: Some("relatif".into()),
            ..ReglagesReplay::default()
        }
        .borne();
        assert_eq!(
            (r.ips, r.duree_s, r.quota_octets),
            (60, DUREE_MIN_S, QUOTA_MIN)
        );
        assert_eq!((r.touche.as_str(), r.dossier), (TOUCHE_DEFAUT, None));
        let r = ReglagesReplay {
            ips: 10,
            duree_s: 1000,
            touche: "F9".into(),
            ..ReglagesReplay::default()
        }
        .borne();
        assert_eq!(
            (r.ips, r.duree_s, r.touche.as_str()),
            (30, DUREE_MAX_S, "F9")
        );
        assert_eq!(ReglagesReplay::default().borne(), ReglagesReplay::default());
    }

    #[test]
    fn se_lisent_meme_incomplets() {
        let r: ReglagesReplay =
            serde_json::from_str(r#"{"actif":true,"resolution":"4k","qualite":"basse"}"#).unwrap();
        assert!(r.actif);
        assert_eq!(
            (r.resolution, r.qualite, r.ips),
            (Resolution::R4k, Qualite::Basse, 60)
        );
        let a = ReglagesReplay::default();
        assert!(a.meme_capture(&ReglagesReplay {
            touche: "F9".into(),
            dossier: Some("/c".into()),
            actif: true,
            ..a.clone()
        }));
        assert!(!a.meme_capture(&ReglagesReplay {
            micro: true,
            ..a.clone()
        }));
    }
}
