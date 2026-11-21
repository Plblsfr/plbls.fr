mod cadence;
mod flux;
mod pixels;
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
pub use flux::FluxCapture;
pub use pixels::{bgra_vers_rgba, incruster_curseur, melanger_curseur, reduire};
pub use source::IdSource;

use crate::traits::{CaptureEcran, Image, OptionsCapture, RecepteurCapture, SourceCapture};
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
