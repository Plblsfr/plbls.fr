mod audio;
pub mod capture;
mod coffre;
mod demarrage;
mod dossiers;
mod erreur;
mod fichiers;
mod indisponible;
mod installation;
pub mod instance;
#[cfg(target_os = "linux")]
mod notifications_linux;
#[cfg(windows)]
mod pastille_windows;
mod processus;
mod raccourcis;
#[cfg(windows)]
mod registre_windows;
pub mod toast;
#[cfg(windows)]
mod toast_windows;
mod traits;
mod url;
mod zone;

pub use audio::{AudioSysteme, ChoixPeripherique, SonPeripheriques};
pub use capture::{CaptureSysteme, FluxCapture, IdSource};
pub use coffre::{CoffreMemoire, CoffreSysteme};
pub use demarrage::{
    commande_de_demarrage, DemarrageSysteme, CLE_DEMARRAGE, OPTION_ARRIERE_PLAN, VALEUR_DEMARRAGE,
};
pub use dossiers::{
    dossier_telechargements, Dossiers, DOSSIER_TELECHARGEMENTS_ESSAI, IDENTIFIANT, VARIABLE_DOSSIER,
};
pub use erreur::{Erreur, Resultat};
pub use fichiers::{FichiersSysteme, PressePapiersSysteme};
pub use indisponible::Indisponible;
pub use installation::{InstallationSysteme, ARGUMENTS_INSTALLEUR, CIBLE_WINDOWS};
pub use processus::{commande_d_arriere_plan, MesureSysteme};
pub use raccourcis::{
    code_d_evdev, code_de_virtuelle, libelle_touche, touche, RaccourcisFactices, RaccourcisSysteme,
    Touche, TOUCHES,
};
pub use raw_window_handle::RawWindowHandle;
pub use traits::{
    ActionNotification, ActionRaccourci, ApplicationPremierPlan, CaptureEcran, CaptureSon, Coffre,
    CompteursProcessus, ContenuPressePapiers, DemarrageSession, ErreurFluxAudio, EtatTouche,
    EvenementCapture, EvenementZone, Fichiers, FiltreFichiers, FluxAudio, FluxSon, FormatAudio,
    FournisseurAudio, Image, InfosMachine, Installation, MesureProcessus, Micro, Notification,
    Notifications, OptionsCapture, OuvertureUrl, Overlay, Pastille, PeripheriqueAudio, PremierPlan,
    PressePapiers, ProducteurSon, RaccourcisGlobaux, RecepteurActions, RecepteurAudio,
    RecepteurCapture, RecepteurSon, RecepteurZone, SensAudio, SignalErreurAudio, SortieSon,
    SourceCapture, SystemeAudio, TypeSource, ZoneNotification,
};
pub use url::NavigateurSysteme;
pub use zone::{zone_presente, ZoneSysteme, LIBELLE_OUVRIR, LIBELLE_QUITTER};

pub const NOM_EXECUTABLE: &str = "estompe-natif.exe";
pub const NOM_AFFICHE: &str = "Estompe natif (bêta)";
pub const ICONE_INSTALLEE: &str = "estompe.ico";

use std::sync::Arc;

pub struct Plateforme {
    pub coffre: Box<dyn Coffre>,
    pub notifications: Arc<dyn Notifications>,
    pub zone: Box<dyn ZoneNotification>,
    pub pastille: Box<dyn Pastille>,
    pub installation: Arc<dyn Installation>,
    pub raccourcis: Arc<dyn RaccourcisGlobaux>,
    pub url: Box<dyn OuvertureUrl>,
    pub fichiers: Box<dyn Fichiers + Send + Sync>,
    pub presse_papiers: Box<dyn PressePapiers + Send + Sync>,
    pub capture_ecran: Arc<dyn CaptureEcran>,
    pub capture_son: Box<dyn CaptureSon>,
    pub audio: Arc<dyn SystemeAudio>,
    pub mesure: Arc<dyn MesureProcessus>,
    pub overlay: Box<dyn Overlay>,
    pub premier_plan: Box<dyn PremierPlan>,
    pub demarrage: Box<dyn DemarrageSession>,
}

impl Plateforme {
    pub fn de_l_os() -> Self {
        Self {
            coffre: Box::new(CoffreSysteme::new(IDENTIFIANT)),
            url: Box::new(NavigateurSysteme),
            fichiers: Box::new(FichiersSysteme),
            presse_papiers: Box::new(PressePapiersSysteme),
            audio: Arc::new(AudioSysteme),
            raccourcis: Arc::new(RaccourcisSysteme),
            mesure: Arc::new(MesureSysteme),
            zone: Box::new(ZoneSysteme::default()),
            demarrage: demarrage_de_l_os(),
            notifications: notifications_de_l_os(),
            pastille: pastille_de_l_os(),
            installation: installation_de_l_os(),
            capture_ecran: capture_de_l_os(),
            ..Self::indisponible()
        }
    }

    pub fn indisponible() -> Self {
        Self {
            coffre: Box::new(Indisponible),
            notifications: Arc::new(Indisponible),
            zone: Box::new(Indisponible),
            pastille: Box::new(Indisponible),
            installation: Arc::new(Indisponible),
            raccourcis: Arc::new(Indisponible),
            url: Box::new(Indisponible),
            fichiers: Box::new(Indisponible),
            presse_papiers: Box::new(Indisponible),
            capture_ecran: Arc::new(Indisponible),
            capture_son: Box::new(Indisponible),
            audio: Arc::new(Indisponible),
            mesure: Arc::new(Indisponible),
            overlay: Box::new(Indisponible),
            premier_plan: Box::new(Indisponible),
            demarrage: Box::new(Indisponible),
        }
    }
}

#[cfg(target_os = "linux")]
fn notifications_de_l_os() -> std::sync::Arc<dyn Notifications> {
    let notifications = notifications_linux::NotificationsLinux::nouvelles();
    if notifications.disponible() {
        std::sync::Arc::new(notifications)
    } else {
        std::sync::Arc::new(Indisponible)
    }
}

#[cfg(windows)]
fn notifications_de_l_os() -> std::sync::Arc<dyn Notifications> {
    std::sync::Arc::new(toast_windows::ToastsWindows::nouveaux())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn notifications_de_l_os() -> std::sync::Arc<dyn Notifications> {
    std::sync::Arc::new(Indisponible)
}

#[cfg(windows)]
fn pastille_de_l_os() -> Box<dyn Pastille> {
    Box::new(pastille_windows::PastilleWindows::default())
}

#[cfg(not(windows))]
fn pastille_de_l_os() -> Box<dyn Pastille> {
    Box::new(Indisponible)
}

fn installation_de_l_os() -> std::sync::Arc<dyn Installation> {
    if InstallationSysteme.disponible() {
        std::sync::Arc::new(InstallationSysteme)
    } else {
        std::sync::Arc::new(Indisponible)
    }
}

fn capture_de_l_os() -> std::sync::Arc<dyn CaptureEcran> {
    if CaptureSysteme.disponible() {
        std::sync::Arc::new(CaptureSysteme)
    } else {
        std::sync::Arc::new(Indisponible)
    }
}

fn demarrage_de_l_os() -> Box<dyn DemarrageSession> {
    match DemarrageSysteme::de_l_executable() {
        Some(demarrage) if demarrage.disponible() => Box::new(demarrage),
        _ => Box::new(Indisponible),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_plateforme_de_l_os_annonce_ce_qu_elle_sait_faire() {
        let p = Plateforme::de_l_os();
        assert!(p.url.disponible());
        assert!(p.fichiers.disponible() && p.presse_papiers.disponible());
        assert!(p.audio.disponible());
        assert!(p.mesure.disponible());
        assert!(p.zone.disponible());
        assert_eq!(p.capture_ecran.disponible(), CaptureSysteme.disponible());
        assert_eq!(p.demarrage.disponible(), cfg!(windows));
        assert_eq!(p.pastille.disponible(), cfg!(windows));
        assert_eq!(p.installation.disponible(), cfg!(windows));
        assert_eq!(
            p.installation.cible(),
            cfg!(windows).then_some(CIBLE_WINDOWS)
        );
        for (nom, disponible) in [
            ("capture_son", p.capture_son.disponible()),
            ("overlay", p.overlay.disponible()),
            ("premier_plan", p.premier_plan.disponible()),
        ] {
            assert!(!disponible, "{nom} n'est pas encore implémenté");
        }
    }

    #[test]
    fn une_fonction_non_implementee_repond_indisponible_sans_panique() {
        let p = Plateforme::indisponible();
        let notification = Notification {
            titre: "Titre".into(),
            corps: "Corps".into(),
            ..Notification::default()
        };
        assert!(matches!(
            p.notifications.notifier(&notification),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_ecran.sources(),
            Err(Erreur::Indisponible(_))
        ));
        assert!(!p.capture_ecran.disponible());
        assert!(matches!(
            p.capture_ecran.capturer("ecran:0"),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_ecran.miniature("fenetre:1", 180),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_ecran
                .demarrer("ecran:0", OptionsCapture::default(), Box::new(|_| true))
                .map(|_| ()),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_son.demarrer(None, Box::new(|_, _| {})),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_son.arreter(),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.overlay.afficher(&Image::default(), 0, 0),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(p.overlay.masquer(), Err(Erreur::Indisponible(_))));
        assert!(matches!(
            p.premier_plan.application(),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(p.demarrage.actif(), Err(Erreur::Indisponible(_))));
        assert!(matches!(
            p.notifications.retirer("t"),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.zone.installer(&Image::default(), "", Box::new(|_| {})),
            Err(Erreur::Indisponible(_))
        ));
        assert!(!p.zone.presente());
        assert!(matches!(
            p.pastille.definir(None, ""),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.installation.lancer(std::path::Path::new("x")),
            Err(Erreur::Indisponible(_))
        ));
        assert_eq!(p.installation.cible(), None);
        assert!(matches!(
            p.demarrage.definir(true),
            Err(Erreur::Indisponible(_))
        ));
    }

    #[test]
    fn la_plateforme_indisponible_refuse_aussi_le_coffre_et_les_url() {
        let p = Plateforme::indisponible();
        assert!(!p.coffre.disponible());
        assert!(!p.url.disponible());
        assert!(!p.fichiers.disponible() && !p.presse_papiers.disponible());
        assert!(!p.audio.disponible());
        assert!(!p.mesure.disponible());
        assert!(matches!(p.mesure.compteurs(), Err(Erreur::Indisponible(_))));
        assert!(!p.raccourcis.disponible());
        assert!(matches!(
            p.raccourcis.enregistrer("KeyV", Box::new(|_| {})),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.raccourcis.retirer("KeyV"),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.audio.peripheriques(SensAudio::Entree),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.audio
                .ouvrir_sortie(None, Box::new(|_| {}), Box::new(|_| {}))
                .map(|_| ()),
            Err(Erreur::Indisponible(_))
        ));
        let son = SonPeripheriques::par_defaut(p.audio.clone());
        assert!(!Micro::disponible(&son) && !SortieSon::disponible(&son));
        assert!(matches!(
            son.ecouter(Box::new(|_, _| {})),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            son.jouer(Box::new(|_, _| {})),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.fichiers.choisir_dossier(None),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.fichiers.ouvrir(std::path::Path::new("x")),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.presse_papiers.copier_image(&Image::default()),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.fichiers.choisir_fichiers(None),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.presse_papiers.lire_pieces(),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(p.coffre.lire("cle"), Err(Erreur::Indisponible(_))));
        assert!(matches!(
            p.coffre.ecrire("cle", "valeur"),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.coffre.effacer("cle"),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.url.ouvrir("https://plbls.fr"),
            Err(Erreur::Indisponible(_))
        ));
    }
}
