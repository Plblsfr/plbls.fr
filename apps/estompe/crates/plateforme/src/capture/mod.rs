mod cadence;
mod factice;
mod flux;
mod pixels;
mod son;
#[cfg(windows)]
mod son_windows;
mod source;
#[cfg(windows)]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

#[cfg(windows)]
use windows as os;
#[cfg(all(unix, not(target_os = "macos")))]
use x11 as os;

pub use cadence::{Cadence, IPS_MAX};
pub use factice::{image_unie, EcransFactices};
pub use flux::FluxCapture;
pub use pixels::{bgra_vers_rgba, incruster_curseur, melanger_curseur, reduire};
pub use son::{
    cible_du_son, flottants, pcm16, SonSysteme, SonsFactices, CANAUX_SON, FREQUENCE_SON,
};
pub use source::IdSource;

use crate::traits::{
    CaptureEcran, Image, OptionsCapture, RecepteurCapture, SourceCapture, TypeSource,
};
use crate::{Erreur, Resultat};

pub const HAUTEUR_MINIATURE: u32 = 180;
pub const LARGEUR_MIN_FENETRE: u32 = 120;
pub const HAUTEUR_MIN_FENETRE: u32 = 80;

pub(crate) fn fenetre_retenue(titre: &str, largeur: u32, hauteur: u32, reduite: bool) -> bool {
    !titre.trim().is_empty()
        && largeur >= LARGEUR_MIN_FENETRE
        && hauteur >= HAUTEUR_MIN_FENETRE
        && !reduite
}

pub fn fenetre_d_estompe<'a>(
    sources: &'a [SourceCapture],
    titre: &str,
) -> Option<&'a SourceCapture> {
    sources
        .iter()
        .filter(|s| s.estompe && s.genre == TypeSource::Fenetre)
        .max_by_key(|s| (s.nom == titre, u64::from(s.largeur) * u64::from(s.hauteur)))
}

pub fn capturer_fenetre_d_estompe(capture: &dyn CaptureEcran, titre: &str) -> Resultat<Image> {
    let sources = capture.sources()?;
    let fenetre = fenetre_d_estompe(&sources, titre)
        .ok_or_else(|| Erreur::Echec("fenêtre d'Estompe introuvable".into()))?;
    capture.capturer(&fenetre.id)
}

pub(crate) fn source_inconnue(source: &str) -> Erreur {
    Erreur::Echec(format!("source de capture inconnue : {source}"))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureSysteme;

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
impl CaptureEcran for CaptureSysteme {
    fn disponible(&self) -> bool {
        os::disponible()
    }
    fn sources(&self) -> Resultat<Vec<SourceCapture>> {
        os::sources()
    }
    fn capturer(&self, source: &str) -> Resultat<Image> {
        let id = IdSource::lire(source).ok_or_else(|| source_inconnue(source))?;
        os::capturer(id)
    }
    fn demarrer(
        &self,
        source: &str,
        options: OptionsCapture,
        recepteur: RecepteurCapture,
    ) -> Resultat<FluxCapture> {
        let id = IdSource::lire(source).ok_or_else(|| source_inconnue(source))?;
        os::demarrer(id, options, recepteur)
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
impl CaptureEcran for CaptureSysteme {
    fn disponible(&self) -> bool {
        false
    }
    fn sources(&self) -> Resultat<Vec<SourceCapture>> {
        Err(Erreur::Indisponible("capture d'écran"))
    }
    fn capturer(&self, _: &str) -> Resultat<Image> {
        Err(Erreur::Indisponible("capture d'écran"))
    }
    fn demarrer(&self, _: &str, _: OptionsCapture, _: RecepteurCapture) -> Resultat<FluxCapture> {
        Err(Erreur::Indisponible("capture d'écran"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seules_les_fenetres_utiles_sont_proposees() {
        assert!(fenetre_retenue("Navigateur", 800, 600, false));
        assert!(!fenetre_retenue("  ", 800, 600, false));
        assert!(!fenetre_retenue("Info-bulle", 119, 600, false));
        assert!(!fenetre_retenue("Barre", 800, 79, false));
        assert!(!fenetre_retenue("Navigateur", 800, 600, true));
    }

    fn source(
        id: &str,
        nom: &str,
        genre: TypeSource,
        taille: (u32, u32),
        estompe: bool,
    ) -> SourceCapture {
        SourceCapture {
            id: id.into(),
            nom: nom.into(),
            application: String::new(),
            genre,
            largeur: taille.0,
            hauteur: taille.1,
            principale: genre == TypeSource::Ecran,
            processus: estompe.then_some(std::process::id()),
            estompe,
        }
    }

    #[test]
    fn la_capture_d_un_signalement_ne_contient_que_la_fenetre_d_estompe() {
        let ecrans = EcransFactices::nouveaux();
        assert!(matches!(
            capturer_fenetre_d_estompe(&ecrans, "Estompe"),
            Err(Erreur::Echec(_))
        ));
        ecrans.brancher(source(
            "ecran:0",
            "Écran 1",
            TypeSource::Ecran,
            (2560, 1440),
            false,
        ));
        ecrans.brancher(source(
            "fenetre:1",
            "Navigateur",
            TypeSource::Fenetre,
            (1920, 1080),
            false,
        ));
        ecrans.brancher(source(
            "fenetre:2",
            "Estompe — overlay",
            TypeSource::Fenetre,
            (2560, 1440),
            true,
        ));
        ecrans.brancher(source(
            "fenetre:3",
            "Estompe",
            TypeSource::Fenetre,
            (1180, 900),
            true,
        ));
        let sources = ecrans.sources().unwrap();
        assert_eq!(
            fenetre_d_estompe(&sources, "Estompe").unwrap().id,
            "fenetre:3"
        );
        assert_eq!(
            fenetre_d_estompe(&sources, "Autre titre").unwrap().id,
            "fenetre:2"
        );
        let image = capturer_fenetre_d_estompe(&ecrans, "Estompe").unwrap();
        assert_eq!((image.largeur, image.hauteur), (1180, 900));
        assert_eq!(image, ecrans.capturer("fenetre:3").unwrap());
        ecrans.retirer("fenetre:2");
        ecrans.retirer("fenetre:3");
        assert!(capturer_fenetre_d_estompe(&ecrans, "Estompe").is_err());
    }

    #[test]
    fn une_source_mal_formee_est_refusee_sans_panique() {
        let capture = CaptureSysteme;
        for source in ["", "m:0", "fenetre:abc"] {
            assert!(matches!(capture.capturer(source), Err(Erreur::Echec(_))));
            assert!(matches!(
                capture.demarrer(source, OptionsCapture::default(), Box::new(|_| true)),
                Err(Erreur::Echec(_))
            ));
        }
    }
}
