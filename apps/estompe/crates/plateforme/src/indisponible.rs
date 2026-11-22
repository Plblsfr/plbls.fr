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
    fn accepte_les_actions(&self) -> bool {
        false
    }
    fn accepte_la_reponse(&self) -> bool {
        false
    }
    fn ecouter(&self, _: RecepteurActions) {}
    fn notifier(&self, _: &Notification) -> Resultat<()> {
        refus("notifications")
    }
    fn retirer(&self, _: &str) -> Resultat<()> {
        refus("notifications")
    }
}

impl Pastille for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn attacher(&self, _: raw_window_handle::RawWindowHandle) -> Resultat<()> {
        refus("pastille de la barre des tâches")
    }
    fn definir(&self, _: Option<&Image>, _: &str) -> Resultat<()> {
        refus("pastille de la barre des tâches")
    }
}

impl ZoneNotification for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn presente(&self) -> bool {
        false
    }
    fn installer(&self, _: &Image, _: &str, _: RecepteurZone) -> Resultat<()> {
        refus("zone de notification")
    }
    fn definir_icone(&self, _: &Image) -> Resultat<()> {
        refus("zone de notification")
    }
    fn retirer(&self) {}
}

impl Installation for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn cible(&self) -> Option<&'static str> {
        None
    }
    fn lancer(&self, _: &std::path::Path) -> Resultat<()> {
        refus("installation d'une mise à jour")
    }
}

impl RaccourcisGlobaux for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn enregistrer(&self, _: &str, _: ActionRaccourci) -> Resultat<()> {
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
    fn choisir_fichiers(&self, _: Option<&std::path::Path>) -> Resultat<Vec<std::path::PathBuf>> {
        refus("choix de fichiers")
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
    fn lire_pieces(&self) -> Resultat<Option<ContenuPressePapiers>> {
        refus("lecture du presse-papiers")
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
    fn miniature(&self, _: &str, _: u32) -> Resultat<Image> {
        refus("capture d'écran")
    }
    fn demarrer(
        &self,
        _: &str,
        _: OptionsCapture,
        _: RecepteurCapture,
    ) -> Resultat<crate::capture::FluxCapture> {
        refus("capture d'écran")
    }
}

impl CaptureSon for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn demarrer(&self, _: CibleSon, _: RecepteurSonCapture) -> Resultat<FluxSon> {
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

impl MesureProcessus for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn compteurs(&self) -> Resultat<CompteursProcessus> {
        refus("compteurs du processus")
    }
    fn machine(&self) -> InfosMachine {
        InfosMachine::default()
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

impl Micro for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn ecouter(&self, _: RecepteurSon) -> Resultat<FluxSon> {
        refus("micro")
    }
}

impl SortieSon for Indisponible {
    fn disponible(&self) -> bool {
        false
    }
    fn jouer(&self, _: ProducteurSon) -> Resultat<FluxSon> {
        refus("sortie du son")
    }
}
