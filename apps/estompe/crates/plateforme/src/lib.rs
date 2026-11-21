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
#[cfg(windows)]
mod registre_windows;
pub mod toast;
#[cfg(windows)]
mod toast_windows;
mod traits;
mod url;
mod zone;

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
pub use raw_window_handle::RawWindowHandle;
pub use traits::{
    ActionNotification, ApplicationPremierPlan, CaptureEcran, CaptureSon, Coffre, DemarrageSession,
    EvenementZone, Fichiers, Image, Installation, Notification, Notifications, OuvertureUrl,
    Overlay, Pastille, PremierPlan, PressePapiers, RaccourcisGlobaux, RecepteurActions,
    RecepteurSon, RecepteurZone, SourceCapture, TypeSource, ZoneNotification,
};
pub use url::NavigateurSysteme;
pub use zone::{zone_presente, ZoneSysteme, LIBELLE_OUVRIR, LIBELLE_QUITTER};

pub const NOM_EXECUTABLE: &str = "estompe-natif.exe";
pub const NOM_AFFICHE: &str = "Estompe natif (bêta)";
pub const ICONE_INSTALLEE: &str = "estompe.ico";

pub struct Plateforme {
    pub coffre: Box<dyn Coffre>,
    pub notifications: std::sync::Arc<dyn Notifications>,
    pub zone: Box<dyn ZoneNotification>,
    pub pastille: Box<dyn Pastille>,
    pub installation: std::sync::Arc<dyn Installation>,
    pub raccourcis: Box<dyn RaccourcisGlobaux>,
    pub url: Box<dyn OuvertureUrl>,
    pub fichiers: Box<dyn Fichiers + Send + Sync>,
    pub presse_papiers: Box<dyn PressePapiers + Send + Sync>,
    pub capture_ecran: Box<dyn CaptureEcran>,
    pub capture_son: Box<dyn CaptureSon>,
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
            zone: Box::new(ZoneSysteme::default()),
            demarrage: demarrage_de_l_os(),
            notifications: notifications_de_l_os(),
            pastille: pastille_de_l_os(),
            installation: installation_de_l_os(),
            ..Self::indisponible()
        }
    }

    pub fn indisponible() -> Self {
        Self {
            coffre: Box::new(Indisponible),
            notifications: std::sync::Arc::new(Indisponible),
            zone: Box::new(Indisponible),
            pastille: Box::new(Indisponible),
            installation: std::sync::Arc::new(Indisponible),
            raccourcis: Box::new(Indisponible),
            url: Box::new(Indisponible),
            fichiers: Box::new(Indisponible),
            presse_papiers: Box::new(Indisponible),
            capture_ecran: Box::new(Indisponible),
            capture_son: Box::new(Indisponible),
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
        assert!(p.zone.disponible());
        assert_eq!(p.demarrage.disponible(), cfg!(windows));
        assert_eq!(p.pastille.disponible(), cfg!(windows));
        assert_eq!(p.installation.disponible(), cfg!(windows));
        assert_eq!(
            p.installation.cible(),
            cfg!(windows).then_some(CIBLE_WINDOWS)
        );
        for (nom, disponible) in [
            ("raccourcis", p.raccourcis.disponible()),
            ("capture_ecran", p.capture_ecran.disponible()),
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
            p.raccourcis.enregistrer("Ctrl+Maj+M", Box::new(|| {})),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.raccourcis.retirer("Ctrl+Maj+M"),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_ecran.sources(),
            Err(Erreur::Indisponible(_))
        ));
        assert!(matches!(
            p.capture_ecran.capturer("ecran:0"),
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
