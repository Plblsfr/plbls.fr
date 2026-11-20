use crate::traits::*;
use crate::{Erreur, Resultat};

#[derive(Debug, Clone, Copy, Default)]
pub struct Indisponible;

fn refus<T>(fonction: &'static str) -> Resultat<T> {
    Err(Erreur::Indisponible(fonction))
}

impl Coffre for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn lire(&self, _: &str) -> Resultat<Option<String>> {
        refus("coffre de secrets")
    }
    fn ecrire(&self, _: &str, _: &str) -> Resultat<()> {
        refus("coffre de secrets")
    }
    fn effacer(&self, _: &str) -> Resultat<()> {
        refus("coffre de secrets")
    }
}

impl Notifications for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn notifier(&self, _: &Notification) -> Resultat<()> {
        refus("notifications")
    }
}

impl RaccourcisGlobaux for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn enregistrer(&self, _: &str, _: Box<dyn Fn() + Send>) -> Resultat<()> {
        refus("raccourcis globaux")
    }
    fn retirer(&self, _: &str) -> Resultat<()> {
        refus("raccourcis globaux")
    }
}

impl OuvertureUrl for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn ouvrir(&self, _: &str) -> Resultat<()> {
        refus("ouverture d'une URL")
    }
}

impl Fichiers for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn choisir_dossier(&self, _: Option<&std::path::Path>) -> Resultat<Option<std::path::PathBuf>> {
        refus("choix d'un dossier")
    }
    fn ouvrir(&self, _: &std::path::Path) -> Resultat<()> {
        refus("ouverture d'un fichier")
    }
}

impl PressePapiers for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn copier_image(&self, _: &Image) -> Resultat<()> {
        refus("copie d'une image")
    }
}

impl CaptureEcran for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn sources(&self) -> Resultat<Vec<SourceCapture>> {
        refus("capture d'écran")
    }
    fn capturer(&self, _: &str) -> Resultat<Image> {
        refus("capture d'écran")
    }
}

impl CaptureSon for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn demarrer(&self, _: Option<&str>, _: RecepteurSon) -> Resultat<()> {
        refus("capture du son")
    }
    fn arreter(&self) -> Resultat<()> {
        refus("capture du son")
    }
}

impl Overlay for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn afficher(&self, _: &Image, _: i32, _: i32) -> Resultat<()> {
        refus("overlay")
    }
    fn masquer(&self) -> Resultat<()> {
        refus("overlay")
    }
}

impl PremierPlan for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn application(&self) -> Resultat<Option<ApplicationPremierPlan>> {
        refus("application au premier plan")
    }
}

impl DemarrageSession for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn actif(&self) -> Resultat<bool> {
        refus("démarrage avec la session")
    }
    fn definir(&self, _: bool) -> Resultat<()> {
        refus("démarrage avec la session")
    }
}

impl SystemeAudio for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn peripheriques(&self, _: SensAudio) -> Resultat<Vec<PeripheriqueAudio>> {
        refus("périphériques audio")
    }
    fn ouvrir_entree(
        &self,
        _: Option<&str>,
        _: RecepteurAudio,
        _: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        refus("périphériques audio")
    }
    fn ouvrir_sortie(
        &self,
        _: Option<&str>,
        _: FournisseurAudio,
        _: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        refus("périphériques audio")
    }
}
