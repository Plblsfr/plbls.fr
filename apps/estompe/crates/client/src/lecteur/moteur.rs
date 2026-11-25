use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::vocal::RecepteurVideo;

use super::etat::Evenement;

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub adresse: String,
    pub jeton: Option<String>,
    pub depart: f64,
    pub en_pause: bool,
}

impl Source {
    pub fn adresse(adresse: impl Into<String>) -> Self {
        Self {
            adresse: adresse.into(),
            jeton: None,
            depart: 0.0,
            en_pause: false,
        }
    }

    pub fn fichier(chemin: &Path) -> Self {
        Self::adresse(chemin.to_string_lossy())
    }

    pub fn avec_jeton(mut self, jeton: impl Into<String>) -> Self {
        self.jeton = Some(jeton.into());
        self
    }

    pub fn a_partir_de(mut self, secondes: f64) -> Self {
        self.depart = if secondes.is_finite() {
            secondes.max(0.0)
        } else {
            0.0
        };
        self
    }

    pub fn en_pause(mut self) -> Self {
        self.en_pause = true;
        self
    }

    pub fn est_distante(&self) -> bool {
        let a = self.adresse.to_ascii_lowercase();
        a.starts_with("https://") || a.starts_with("http://")
    }

    pub fn entetes_http(&self) -> Vec<String> {
        match &self.jeton {
            Some(jeton) if self.est_distante() && !jeton.contains(['\r', '\n']) => {
                vec![format!("Authorization: Bearer {jeton}")]
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurLecteur {
    Indisponible(String),
    Echec(String),
}

impl fmt::Display for ErreurLecteur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Indisponible(m) => write!(f, "lecteur indisponible : {m}"),
            Self::Echec(m) => write!(f, "lecteur : {m}"),
        }
    }
}

impl std::error::Error for ErreurLecteur {}

pub type Rapport = Arc<dyn Fn(Evenement) + Send + Sync>;

pub trait MoteurLecture: Send + Sync {
    fn charger(&self, source: &Source);
    fn arreter(&self);
    fn mettre_en_pause(&self, pause: bool);
    fn aller(&self, position: f64);
    fn regler_volume(&self, volume: u8, muet: bool);
    fn regler_vitesse(&self, vitesse: f64);
    fn choisir_audio(&self, piste: Option<i64>);
    fn choisir_sous_titres(&self, piste: Option<i64>);
    fn activer_video(&self, active: bool);
    fn choisir_sortie(&self, peripherique: &str);
    fn definir_zone(&self, zone: (u32, u32));
    fn images(&self) -> Arc<RecepteurVideo>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_jeton_part_en_en_tete_seulement_vers_http() {
        let distante = Source::adresse("https://media.plbls.fr/v.m3u8").avec_jeton("abc");
        assert_eq!(distante.entetes_http(), ["Authorization: Bearer abc"]);
        assert!(Source::adresse("HTTP://x/y")
            .avec_jeton("abc")
            .est_distante());
        assert!(Source::fichier(Path::new("/tmp/v.mp4"))
            .avec_jeton("abc")
            .entetes_http()
            .is_empty());
        assert!(Source::adresse("https://x/v.mp4").entetes_http().is_empty());
        assert!(Source::adresse("https://x/v.mp4")
            .avec_jeton("a\r\nX-Injecte: 1")
            .entetes_http()
            .is_empty());
        assert_eq!(Source::adresse("a").a_partir_de(-4.0).depart, 0.0);
        assert_eq!(Source::adresse("a").a_partir_de(f64::NAN).depart, 0.0);
        assert!(Source::adresse("a").en_pause().en_pause);
    }
}
