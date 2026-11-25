pub mod etat;
pub mod factice;
pub mod image;
pub mod moteur;
#[cfg(feature = "mpv")]
pub mod mpv;
pub mod pistes;
pub mod reglages;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::watch;

use crate::vocal::RecepteurVideo;

pub use etat::{EtatLecteur, Evenement, Phase};
pub use factice::{CommandeMoteur, MediaFactice, MoteurFactice};
pub use image::ImageLecteur;
pub use moteur::{ErreurLecteur, MoteurLecture, Rapport, Source};
pub use pistes::{nom_de_langue, pistes_de_mpv, GenrePiste, Piste};
pub use reglages::{
    borner_vitesse, libelle_vitesse, peripherique_mpv, position_bornee, raccourci, son_bascule,
    taille_de_rendu, vitesse_suivante, volume_ajuste, Raccourci, PAS_DEPLACEMENT,
    PAS_DEPLACEMENT_FILM, PAS_VOLUME, VITESSES, VITESSE_MAX, VITESSE_MIN, VOLUME_MAX,
};

pub const MESSAGE_ERREUR: &str = "La lecture n’a pas pu démarrer.";

pub struct Lecteur {
    moteur: Arc<dyn MoteurLecture>,
    etat: Arc<watch::Sender<EtatLecteur>>,
    pas: Mutex<f64>,
}

impl Lecteur {
    pub fn nouveau<M, F>(creer: F) -> Result<Self, ErreurLecteur>
    where
        M: MoteurLecture + 'static,
        F: FnOnce(Rapport) -> Result<M, ErreurLecteur>,
    {
        let etat = Arc::new(watch::Sender::new(EtatLecteur::default()));
        let suivi = etat.clone();
        let rapport: Rapport = Arc::new(move |evenement| {
            suivi.send_modify(|e| e.appliquer(evenement, Instant::now()));
        });
        let moteur = creer(rapport)?;
        Ok(Self {
            moteur: Arc::new(moteur),
            etat,
            pas: Mutex::new(PAS_DEPLACEMENT),
        })
    }

    pub fn factice() -> (Self, MoteurFactice) {
        let mut moteur = None;
        let lecteur = Self::nouveau(|rapport| {
            let m = MoteurFactice::nouveau(rapport);
            moteur = Some(m.clone());
            Ok(m)
        });
        match (lecteur, moteur) {
            (Ok(lecteur), Some(moteur)) => (lecteur, moteur),
            _ => unreachable!("le moteur factice ne peut pas échouer"),
        }
    }

    #[cfg(feature = "mpv")]
    pub fn mpv(options: mpv::OptionsMpv) -> Result<Self, ErreurLecteur> {
        Self::nouveau(|rapport| mpv::MoteurMpv::nouveau(options, rapport))
    }

    pub fn etat(&self) -> EtatLecteur {
        self.etat.borrow().clone()
    }

    pub fn observer(&self) -> watch::Receiver<EtatLecteur> {
        self.etat.subscribe()
    }

    pub fn images(&self) -> Arc<RecepteurVideo> {
        self.moteur.images()
    }

    pub fn position(&self) -> f64 {
        self.etat.borrow().position_a(Instant::now())
    }

    pub fn charger(&self, source: Source) {
        self.etat
            .send_modify(|e| e.appliquer(Evenement::Chargement, Instant::now()));
        self.moteur.charger(&source);
    }

    pub fn arreter(&self) {
        self.moteur.arreter();
    }

    pub fn lire(&self) {
        if self.etat.borrow().fin {
            self.moteur.aller(0.0);
        }
        self.moteur.mettre_en_pause(false);
    }

    pub fn pause(&self) {
        self.moteur.mettre_en_pause(true);
    }

    pub fn basculer(&self) {
        let (pause, fin) = {
            let e = self.etat.borrow();
            (e.en_pause, e.fin)
        };
        if pause || fin {
            self.lire();
        } else {
            self.pause();
        }
    }

    pub fn aller(&self, position: f64) {
        let duree = self.etat.borrow().duree;
        self.moteur.aller(position_bornee(position, duree));
    }

    pub fn decaler(&self, ecart: f64) {
        self.aller(self.position() + ecart);
    }

    pub fn definir_volume(&self, volume: u8) {
        let volume = volume.min(VOLUME_MAX);
        let muet = self.etat.borrow().muet && volume == 0;
        self.moteur.regler_volume(volume, muet);
    }

    pub fn ajuster_volume(&self, ecart: i16) {
        let (volume, muet) = {
            let e = self.etat.borrow();
            volume_ajuste(e.volume, e.muet, ecart)
        };
        self.moteur.regler_volume(volume, muet);
    }

    pub fn basculer_muet(&self) {
        let (volume, muet) = {
            let e = self.etat.borrow();
            son_bascule(e.volume, e.muet)
        };
        self.moteur.regler_volume(volume, muet);
    }

    pub fn definir_vitesse(&self, vitesse: f64) {
        self.moteur.regler_vitesse(borner_vitesse(vitesse));
    }

    pub fn vitesse_suivante(&self) {
        let vitesse = self.etat.borrow().vitesse;
        self.moteur.regler_vitesse(vitesse_suivante(vitesse));
    }

    pub fn choisir_audio(&self, piste: Option<i64>) {
        self.moteur.choisir_audio(piste);
    }

    pub fn choisir_sous_titres(&self, piste: Option<i64>) {
        self.moteur.choisir_sous_titres(piste);
    }

    pub fn choisir_sortie(&self, sortie: Option<&str>) {
        self.moteur.choisir_sortie(&peripherique_mpv(sortie));
    }

    pub fn definir_zone(&self, zone: (u32, u32)) {
        self.moteur.definir_zone(zone);
    }

    pub fn definir_visible(&self, visible: bool) {
        if self
            .etat
            .send_if_modified(|e| std::mem::replace(&mut e.visible, visible) != visible)
        {
            self.moteur.activer_video(visible);
        }
    }

    pub fn definir_plein_ecran(&self, plein: bool) {
        self.etat
            .send_if_modified(|e| std::mem::replace(&mut e.plein_ecran, plein) != plein);
    }

    pub fn basculer_plein_ecran(&self) {
        let plein = self.etat.borrow().plein_ecran;
        self.definir_plein_ecran(!plein);
    }

    pub fn definir_pas(&self, secondes: f64) {
        *self.pas.lock().unwrap_or_else(|e| e.into_inner()) = secondes.abs();
    }

    pub fn executer(&self, action: Raccourci) -> bool {
        let pas = *self.pas.lock().unwrap_or_else(|e| e.into_inner());
        match action {
            Raccourci::Basculer => self.basculer(),
            Raccourci::Reculer => self.decaler(-pas),
            Raccourci::Avancer => self.decaler(pas),
            Raccourci::PlusFort => self.ajuster_volume(i16::from(PAS_VOLUME)),
            Raccourci::MoinsFort => self.ajuster_volume(-i16::from(PAS_VOLUME)),
            Raccourci::Muet => self.basculer_muet(),
            Raccourci::PleinEcran => self.basculer_plein_ecran(),
            Raccourci::QuitterPleinEcran => {
                if !self.etat.borrow().plein_ecran {
                    return false;
                }
                self.definir_plein_ecran(false);
            }
            Raccourci::PasserIntro => return false,
        }
        true
    }

    pub fn touche(&self, texte: &str, ctrl: bool, meta: bool, alt: bool) -> bool {
        raccourci(texte, ctrl, meta, alt).is_some_and(|r| self.executer(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO: &str = "https://media.plbls.fr/film.m3u8";

    fn lecteur_pret() -> (Lecteur, MoteurFactice) {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media(VIDEO, MediaFactice::video(120.0));
        lecteur.charger(Source::adresse(VIDEO).avec_jeton("j"));
        moteur.vider_commandes();
        (lecteur, moteur)
    }

    #[test]
    fn chargement_lecture_pause_et_fin() {
        let (lecteur, moteur) = Lecteur::factice();
        let mut suivi = lecteur.observer();
        moteur.ajouter_media(VIDEO, MediaFactice::video(120.0));
        lecteur.charger(Source::adresse(VIDEO).a_partir_de(30.0));
        assert!(suivi.has_changed().unwrap());
        let e = suivi.borrow_and_update().clone();
        assert_eq!(e.phase, Phase::Pret);
        assert_eq!(
            (e.position, e.duree, e.en_pause),
            (30.0, Some(120.0), false)
        );
        assert_eq!(e.taille_video(), Some((1280, 720)));
        assert_eq!(e.audio, Some(1));
        assert_eq!(e.pistes_de(GenrePiste::Audio).count(), 2);
        assert!(e.a_de_la_video());
        lecteur.basculer();
        assert!(lecteur.etat().en_pause);
        moteur.avancer(5.0);
        assert!((lecteur.etat().position - 30.0).abs() < 1e-3);
        lecteur.basculer();
        moteur.avancer(200.0);
        let e = lecteur.etat();
        assert!(e.fin && !e.avance());
        assert_eq!(e.position, 120.0);
        moteur.vider_commandes();
        lecteur.basculer();
        assert_eq!(
            moteur.commandes(),
            [CommandeMoteur::Aller(0.0), CommandeMoteur::Pause(false)]
        );
        assert!(!lecteur.etat().fin);
    }

    #[test]
    fn un_echec_de_chargement_passe_en_erreur() {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_echec(VIDEO, "HTTP 401");
        lecteur.charger(Source::adresse(VIDEO));
        let e = lecteur.etat();
        assert_eq!(e.phase, Phase::Erreur);
        assert_eq!(e.erreur.as_deref(), Some("HTTP 401"));
        lecteur.charger(Source::adresse("https://ailleurs/v.mp4"));
        assert_eq!(
            lecteur.etat().erreur.as_deref(),
            Some("https://ailleurs/v.mp4 introuvable")
        );
    }

    #[test]
    fn raccourcis_executes_sur_le_moteur() {
        let (lecteur, moteur) = lecteur_pret();
        lecteur.pause();
        lecteur.aller(50.0);
        moteur.vider_commandes();
        assert!(lecteur.touche("\u{f703}", false, false, false));
        assert!(lecteur.touche("j", false, false, false));
        assert!(lecteur.touche("\u{f701}", false, false, false));
        assert!(lecteur.touche("m", false, false, false));
        assert!(lecteur.touche("m", false, false, false));
        assert!(lecteur.touche(" ", false, false, false));
        assert!(!lecteur.touche("s", false, false, false));
        assert!(!lecteur.touche("k", true, false, false));
        assert!(!lecteur.touche("\u{1b}", false, false, false));
        assert!(lecteur.touche("f", false, false, false));
        assert!(lecteur.etat().plein_ecran);
        assert!(lecteur.touche("\u{1b}", false, false, false));
        assert!(!lecteur.etat().plein_ecran);
        let commandes = moteur.commandes();
        assert!((aller(&commandes[0]) - 55.0).abs() < 1e-3);
        assert!((aller(&commandes[1]) - 50.0).abs() < 1e-3);
        assert_eq!(commandes[2], CommandeMoteur::Volume(95, false));
        assert_eq!(commandes[3], CommandeMoteur::Volume(95, true));
        assert_eq!(commandes[4], CommandeMoteur::Volume(95, false));
        assert_eq!(commandes[5], CommandeMoteur::Pause(false));
        assert_eq!(commandes.len(), 6);
        lecteur.pause();
        lecteur.definir_pas(PAS_DEPLACEMENT_FILM);
        moteur.vider_commandes();
        lecteur.executer(Raccourci::Reculer);
        let commandes = moteur.commandes();
        assert_eq!(commandes.len(), 1);
        assert!((aller(&commandes[0]) - 40.0).abs() < 1e-3);
    }

    fn aller(commande: &CommandeMoteur) -> f64 {
        match commande {
            CommandeMoteur::Aller(p) => *p,
            autre => panic!("Aller attendu : {autre:?}"),
        }
    }

    #[test]
    fn volume_vitesse_pistes_sortie_et_bornes() {
        let (lecteur, moteur) = lecteur_pret();
        lecteur.aller(500.0);
        lecteur.aller(-5.0);
        lecteur.definir_volume(180);
        lecteur.basculer_muet();
        lecteur.definir_volume(0);
        lecteur.definir_volume(30);
        lecteur.definir_vitesse(9.0);
        lecteur.vitesse_suivante();
        lecteur.choisir_audio(Some(2));
        lecteur.choisir_sous_titres(None);
        lecteur.choisir_sortie(Some("wasapi:{0.0.0.00000000}.{g}"));
        lecteur.choisir_sortie(None);
        assert_eq!(
            moteur.commandes(),
            [
                CommandeMoteur::Aller(120.0),
                CommandeMoteur::Aller(0.0),
                CommandeMoteur::Volume(100, false),
                CommandeMoteur::Volume(100, true),
                CommandeMoteur::Volume(0, true),
                CommandeMoteur::Volume(30, false),
                CommandeMoteur::Vitesse(VITESSE_MAX),
                CommandeMoteur::Vitesse(0.5),
                CommandeMoteur::Audio(Some(2)),
                CommandeMoteur::SousTitres(None),
                CommandeMoteur::Sortie("wasapi/{g}".into()),
                CommandeMoteur::Sortie("auto".into()),
            ]
        );
        let e = lecteur.etat();
        assert_eq!(
            (e.volume, e.muet, e.vitesse, e.audio),
            (30, false, 0.5, Some(2))
        );
    }

    #[test]
    fn masque_le_decodage_video_s_arrete_une_seule_fois() {
        let (lecteur, moteur) = lecteur_pret();
        let mut suivi = lecteur.observer();
        suivi.borrow_and_update();
        lecteur.definir_visible(true);
        assert!(!suivi.has_changed().unwrap());
        lecteur.definir_visible(false);
        lecteur.definir_visible(false);
        lecteur.definir_visible(true);
        lecteur.definir_zone((640, 360));
        assert_eq!(
            moteur.commandes(),
            [
                CommandeMoteur::Video(false),
                CommandeMoteur::Video(true),
                CommandeMoteur::Zone(640, 360)
            ]
        );
        assert!(suivi.has_changed().unwrap());
    }

    #[test]
    fn la_position_suit_les_evenements_du_moteur_meme_retardes() {
        let (lecteur, moteur) = lecteur_pret();
        moteur.suspendre_reponses(true);
        lecteur.aller(80.0);
        assert_eq!(lecteur.etat().position, 0.0);
        moteur.suspendre_reponses(false);
        moteur.emettre(Evenement::Position(80.0));
        moteur.emettre(Evenement::Tampon(true));
        assert!(lecteur.etat().en_attente());
        assert!((lecteur.position() - 80.0).abs() < 1e-3);
        moteur.emettre(Evenement::Tampon(false));
        assert!(lecteur.etat().avance());
        lecteur.pousser_test_image();
        assert_eq!(lecteur.images().images_recues(), 1);
    }

    impl Lecteur {
        fn pousser_test_image(&self) {
            self.images()
                .pousser(Arc::new(ImageLecteur::nouvelle(2, 2)));
        }
    }
}
