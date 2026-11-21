mod audio;
mod coffre;
mod dossiers;
mod erreur;
mod fichiers;
mod indisponible;
mod raccourcis;
mod traits;
mod url;

pub use audio::AudioSysteme;
pub use coffre::{CoffreMemoire, CoffreSysteme};
pub use dossiers::{
    dossier_telechargements, Dossiers, DOSSIER_TELECHARGEMENTS_ESSAI, IDENTIFIANT, VARIABLE_DOSSIER,
};
pub use erreur::{Erreur, Resultat};
pub use fichiers::{FichiersSysteme, PressePapiersSysteme};
pub use indisponible::Indisponible;
pub use raccourcis::{
    code_d_evdev, code_de_virtuelle, libelle_touche, touche, RaccourcisFactices, RaccourcisSysteme,
    Touche, TOUCHES,
};
pub use traits::{
    ActionRaccourci, ApplicationPremierPlan, CaptureEcran, CaptureSon, Coffre, DemarrageSession,
    ErreurFluxAudio, EtatTouche, Fichiers, FluxAudio, FormatAudio, FournisseurAudio, Image,
    Notification, Notifications, OuvertureUrl, Overlay, PeripheriqueAudio, PremierPlan,
    PressePapiers, RaccourcisGlobaux, RecepteurAudio, RecepteurSon, SensAudio, SignalErreurAudio,
    SourceCapture, SystemeAudio, TypeSource,
};
pub use url::NavigateurSysteme;

use std::sync::Arc;

pub struct Plateforme {
    pub coffre: Box<dyn Coffre>,
    pub notifications: Box<dyn Notifications>,
    pub raccourcis: Arc<dyn RaccourcisGlobaux>,
    pub url: Box<dyn OuvertureUrl>,
    pub fichiers: Box<dyn Fichiers + Send + Sync>,
    pub presse_papiers: Box<dyn PressePapiers + Send + Sync>,
    pub capture_ecran: Box<dyn CaptureEcran>,
    pub capture_son: Box<dyn CaptureSon>,
    pub audio: Arc<dyn SystemeAudio>,
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
            ..Self::indisponible()
        }
    }

    pub fn indisponible() -> Self {
        Self {
            coffre: Box::new(Indisponible),
            notifications: Box::new(Indisponible),
            raccourcis: Arc::new(Indisponible),
            url: Box::new(Indisponible),
            fichiers: Box::new(Indisponible),
            presse_papiers: Box::new(Indisponible),
            capture_ecran: Box::new(Indisponible),
            capture_son: Box::new(Indisponible),
            audio: Arc::new(Indisponible),
            overlay: Box::new(Indisponible),
            premier_plan: Box::new(Indisponible),
            demarrage: Box::new(Indisponible),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_plateforme_de_l_os_annonce_le_coffre_et_l_ouverture_d_url() {
        let p = Plateforme::de_l_os();
        assert!(p.url.disponible());
        assert!(p.fichiers.disponible() && p.presse_papiers.disponible());
        assert!(p.audio.disponible());
        for (nom, disponible) in [
            ("notifications", p.notifications.disponible()),
            ("capture_ecran", p.capture_ecran.disponible()),
            ("capture_son", p.capture_son.disponible()),
            ("overlay", p.overlay.disponible()),
            ("premier_plan", p.premier_plan.disponible()),
            ("demarrage", p.demarrage.disponible()),
        ] {
            assert!(!disponible, "{nom} n'est pas encore implémenté");
        }
    }

    #[test]
    fn une_fonction_non_implementee_repond_indisponible_sans_panique() {
        let p = Plateforme::de_l_os();
        let notification = Notification {
            titre: "Titre".into(),
            corps: "Corps".into(),
        };
        assert!(matches!(
            p.notifications.notifier(&notification),
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
