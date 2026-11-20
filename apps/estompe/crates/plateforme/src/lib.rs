mod coffre;
mod dossiers;
mod erreur;
mod indisponible;
mod traits;
mod url;

pub use coffre::{CoffreMemoire, CoffreSysteme};
pub use dossiers::{Dossiers, IDENTIFIANT, VARIABLE_DOSSIER};
pub use erreur::{Erreur, Resultat};
pub use indisponible::Indisponible;
pub use traits::{
    ApplicationPremierPlan, CaptureEcran, CaptureSon, Coffre, DemarrageSession, Image,
    Notification, Notifications, OuvertureUrl, Overlay, PremierPlan, RaccourcisGlobaux,
    RecepteurSon, SourceCapture, TypeSource,
};
pub use url::NavigateurSysteme;

pub struct Plateforme {
    pub coffre: Box<dyn Coffre>,
    pub notifications: Box<dyn Notifications>,
    pub raccourcis: Box<dyn RaccourcisGlobaux>,
    pub url: Box<dyn OuvertureUrl>,
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
            ..Self::indisponible()
        }
    }

    pub fn indisponible() -> Self {
        Self {
            coffre: Box::new(Indisponible),
            notifications: Box::new(Indisponible),
            raccourcis: Box::new(Indisponible),
            url: Box::new(Indisponible),
            capture_ecran: Box::new(Indisponible),
            capture_son: Box::new(Indisponible),
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
        for (nom, disponible) in [
            ("notifications", p.notifications.disponible()),
            ("raccourcis", p.raccourcis.disponible()),
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
            p.demarrage.definir(true),
            Err(Erreur::Indisponible(_))
        ));
    }

    #[test]
    fn la_plateforme_indisponible_refuse_aussi_le_coffre_et_les_url() {
        let p = Plateforme::indisponible();
        assert!(!p.coffre.disponible());
        assert!(!p.url.disponible());
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
