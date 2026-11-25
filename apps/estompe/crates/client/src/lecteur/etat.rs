use std::time::Instant;

use crate::format::formater_duree;

use super::pistes::{GenrePiste, Piste};
use super::reglages::{VITESSE_MAX, VITESSE_MIN, VOLUME_MAX};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Vide,
    Chargement,
    Pret,
    Erreur,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Evenement {
    Chargement,
    Charge,
    Arrete,
    Erreur(String),
    EnPause(bool),
    Position(f64),
    Duree(Option<f64>),
    Tampon(bool),
    Recherche(bool),
    CacheJusqua(Option<f64>),
    Volume(f64),
    Muet(bool),
    Vitesse(f64),
    Pistes(Vec<Piste>),
    PisteAudio(Option<i64>),
    SousTitres(Option<i64>),
    LargeurVideo(u32),
    HauteurVideo(u32),
    Fin(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EtatLecteur {
    pub phase: Phase,
    pub erreur: Option<String>,
    pub en_pause: bool,
    pub tampon: bool,
    pub recherche: bool,
    pub fin: bool,
    pub position: f64,
    pub horodatage: Option<Instant>,
    pub duree: Option<f64>,
    pub cache_jusqua: Option<f64>,
    pub volume: u8,
    pub muet: bool,
    pub vitesse: f64,
    pub pistes: Vec<Piste>,
    pub audio: Option<i64>,
    pub sous_titres: Option<i64>,
    pub largeur_video: u32,
    pub hauteur_video: u32,
    pub plein_ecran: bool,
    pub visible: bool,
}

impl Default for EtatLecteur {
    fn default() -> Self {
        Self {
            phase: Phase::Vide,
            erreur: None,
            en_pause: true,
            tampon: false,
            recherche: false,
            fin: false,
            position: 0.0,
            horodatage: None,
            duree: None,
            cache_jusqua: None,
            volume: VOLUME_MAX,
            muet: false,
            vitesse: 1.0,
            pistes: Vec::new(),
            audio: None,
            sous_titres: None,
            largeur_video: 0,
            hauteur_video: 0,
            plein_ecran: false,
            visible: true,
        }
    }
}

fn fini(valeur: f64) -> Option<f64> {
    valeur.is_finite().then_some(valeur)
}

impl EtatLecteur {
    pub fn appliquer(&mut self, evenement: Evenement, maintenant: Instant) {
        match evenement {
            Evenement::Chargement => {
                *self = Self {
                    phase: Phase::Chargement,
                    en_pause: self.en_pause,
                    volume: self.volume,
                    muet: self.muet,
                    vitesse: self.vitesse,
                    plein_ecran: self.plein_ecran,
                    visible: self.visible,
                    ..Self::default()
                };
            }
            Evenement::Charge => {
                self.phase = Phase::Pret;
                self.erreur = None;
                self.horodatage = Some(maintenant);
            }
            Evenement::Arrete => {
                if self.phase != Phase::Erreur {
                    self.phase = Phase::Vide;
                }
                self.horodatage = None;
            }
            Evenement::Erreur(message) => {
                self.phase = Phase::Erreur;
                self.erreur = Some(message);
                self.horodatage = None;
            }
            Evenement::EnPause(pause) => {
                self.figer(maintenant);
                self.en_pause = pause;
            }
            Evenement::Position(position) => {
                if let Some(p) = fini(position) {
                    self.position = p.max(0.0);
                    self.horodatage = Some(maintenant);
                }
            }
            Evenement::Duree(duree) => {
                self.duree = duree.and_then(fini).filter(|d| *d > 0.0);
            }
            Evenement::Tampon(actif) => {
                self.figer(maintenant);
                self.tampon = actif;
            }
            Evenement::Recherche(actif) => {
                self.figer(maintenant);
                self.recherche = actif;
            }
            Evenement::CacheJusqua(fin) => self.cache_jusqua = fin.and_then(fini),
            Evenement::Volume(volume) => {
                if let Some(v) = fini(volume) {
                    self.volume = v.round().clamp(0.0, f64::from(VOLUME_MAX)) as u8;
                }
            }
            Evenement::Muet(muet) => self.muet = muet,
            Evenement::Vitesse(vitesse) => {
                if let Some(v) = fini(vitesse) {
                    self.figer(maintenant);
                    self.vitesse = v.clamp(VITESSE_MIN, VITESSE_MAX);
                }
            }
            Evenement::Pistes(pistes) => self.pistes = pistes,
            Evenement::PisteAudio(id) => self.audio = id,
            Evenement::SousTitres(id) => self.sous_titres = id,
            Evenement::LargeurVideo(l) => self.largeur_video = l,
            Evenement::HauteurVideo(h) => self.hauteur_video = h,
            Evenement::Fin(fin) => {
                self.figer(maintenant);
                self.fin = fin;
                if let (true, Some(d)) = (fin, self.duree) {
                    self.position = d;
                }
            }
        }
    }

    fn figer(&mut self, maintenant: Instant) {
        if self.horodatage.is_some() {
            self.position = self.position_a(maintenant);
            self.horodatage = Some(maintenant);
        }
    }

    pub fn avance(&self) -> bool {
        self.phase == Phase::Pret && !self.en_pause && !self.tampon && !self.recherche && !self.fin
    }

    pub fn en_attente(&self) -> bool {
        self.phase == Phase::Chargement
            || (self.phase == Phase::Pret && (self.tampon || self.recherche))
    }

    pub fn position_a(&self, maintenant: Instant) -> f64 {
        let position = match self.horodatage {
            Some(t) if self.avance() => {
                self.position + maintenant.saturating_duration_since(t).as_secs_f64() * self.vitesse
            }
            _ => self.position,
        };
        match self.duree {
            Some(d) => position.min(d),
            None => position,
        }
    }

    pub fn taille_video(&self) -> Option<(u32, u32)> {
        (self.largeur_video > 0 && self.hauteur_video > 0)
            .then_some((self.largeur_video, self.hauteur_video))
    }

    pub fn a_de_la_video(&self) -> bool {
        self.pistes
            .iter()
            .any(|p| p.genre == GenrePiste::Video && !p.image_fixe)
    }

    pub fn pistes_de(&self, genre: GenrePiste) -> impl Iterator<Item = &Piste> {
        self.pistes.iter().filter(move |p| p.genre == genre)
    }

    pub fn temps(&self) -> String {
        format!(
            "{} / {}",
            formater_duree(self.position),
            formater_duree(self.duree.unwrap_or(0.0))
        )
    }

    pub fn volume_effectif(&self) -> u8 {
        if self.muet {
            0
        } else {
            self.volume
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn pret(t: Instant) -> EtatLecteur {
        let mut etat = EtatLecteur::default();
        etat.appliquer(Evenement::Chargement, t);
        etat.appliquer(Evenement::Duree(Some(60.0)), t);
        etat.appliquer(Evenement::Charge, t);
        etat.appliquer(Evenement::EnPause(false), t);
        etat.appliquer(Evenement::Position(10.0), t);
        etat
    }

    #[test]
    fn la_position_avance_avec_le_temps_et_la_vitesse_seulement_en_lecture() {
        let t = Instant::now();
        let mut etat = pret(t);
        assert!(etat.avance());
        assert!((etat.position_a(t + Duration::from_secs(2)) - 12.0).abs() < 1e-9);
        etat.appliquer(Evenement::Vitesse(2.0), t + Duration::from_secs(2));
        assert!((etat.position_a(t + Duration::from_secs(3)) - 14.0).abs() < 1e-9);
        etat.appliquer(Evenement::EnPause(true), t + Duration::from_secs(3));
        assert!((etat.position - 14.0).abs() < 1e-9);
        assert!((etat.position_a(t + Duration::from_secs(30)) - 14.0).abs() < 1e-9);
        assert!((etat.position_a(t + Duration::from_secs(300)) - 14.0).abs() < 1e-9);
    }

    #[test]
    fn la_position_extrapolee_ne_depasse_pas_la_duree_ni_n_avance_en_attente() {
        let t = Instant::now();
        let mut etat = pret(t);
        assert!((etat.position_a(t + Duration::from_secs(600)) - 60.0).abs() < 1e-9);
        assert_eq!(etat.temps(), "10 s / 1:00");
        etat.appliquer(Evenement::Tampon(true), t + Duration::from_secs(1));
        assert!(etat.en_attente());
        assert!((etat.position_a(t + Duration::from_secs(5)) - 11.0).abs() < 1e-9);
        etat.appliquer(Evenement::Tampon(false), t + Duration::from_secs(5));
        etat.appliquer(Evenement::Recherche(true), t + Duration::from_secs(5));
        assert!(!etat.avance());
        etat.appliquer(Evenement::Recherche(false), t + Duration::from_secs(6));
        etat.appliquer(Evenement::Fin(true), t + Duration::from_secs(6));
        assert!(!etat.avance());
        assert_eq!(etat.position, 60.0);
    }

    #[test]
    fn un_nouveau_chargement_garde_les_reglages_et_oublie_le_media() {
        let t = Instant::now();
        let mut etat = pret(t);
        etat.appliquer(Evenement::Volume(42.4), t);
        etat.appliquer(Evenement::Muet(true), t);
        etat.appliquer(Evenement::Vitesse(1.5), t);
        etat.appliquer(Evenement::SousTitres(Some(3)), t);
        etat.appliquer(Evenement::LargeurVideo(1920), t);
        etat.plein_ecran = true;
        etat.appliquer(Evenement::Chargement, t);
        assert_eq!(etat.phase, Phase::Chargement);
        assert!(etat.en_attente());
        assert_eq!((etat.volume, etat.muet, etat.vitesse), (42, true, 1.5));
        assert!(etat.plein_ecran);
        assert_eq!(
            (etat.position, etat.duree, etat.sous_titres),
            (0.0, None, None)
        );
        assert_eq!(etat.taille_video(), None);
        assert_eq!(etat.volume_effectif(), 0);
    }

    #[test]
    fn une_erreur_reste_affichee_apres_l_arret_et_les_valeurs_invalides_sont_ignorees() {
        let t = Instant::now();
        let mut etat = pret(t);
        etat.appliquer(Evenement::Erreur("HTTP 401".into()), t);
        etat.appliquer(Evenement::Arrete, t);
        assert_eq!(etat.phase, Phase::Erreur);
        assert_eq!(etat.erreur.as_deref(), Some("HTTP 401"));
        etat.appliquer(Evenement::Position(f64::NAN), t);
        etat.appliquer(Evenement::Duree(Some(f64::INFINITY)), t);
        etat.appliquer(Evenement::Volume(250.0), t);
        etat.appliquer(Evenement::Vitesse(40.0), t);
        assert_eq!(etat.position, 10.0);
        assert_eq!(etat.duree, None);
        assert_eq!(etat.volume, 100);
        assert_eq!(etat.vitesse, VITESSE_MAX);
        etat.appliquer(Evenement::Charge, t);
        assert_eq!((etat.phase, etat.erreur.clone()), (Phase::Pret, None));
    }
}
