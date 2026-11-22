use std::sync::{Arc, Mutex, MutexGuard};

use crate::traits::{ApplicationPremierPlan, PremierPlan};
use crate::Resultat;

#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, Default)]
pub struct PremierPlanSysteme;

#[cfg(windows)]
impl PremierPlan for PremierPlanSysteme {
    fn disponible(&self) -> bool {
        true
    }
    fn application(&self) -> Resultat<Option<ApplicationPremierPlan>> {
        Ok(windows::application())
    }
    fn fenetres(&self) -> Resultat<Vec<ApplicationPremierPlan>> {
        Ok(windows::fenetres())
    }
}

#[cfg(not(windows))]
impl PremierPlan for PremierPlanSysteme {
    fn disponible(&self) -> bool {
        false
    }
    fn application(&self) -> Resultat<Option<ApplicationPremierPlan>> {
        Err(crate::Erreur::Indisponible("application au premier plan"))
    }
    fn fenetres(&self) -> Resultat<Vec<ApplicationPremierPlan>> {
        Err(crate::Erreur::Indisponible("application au premier plan"))
    }
}

pub fn nom_de_fichier(chemin: &str) -> &str {
    chemin.rsplit(['\\', '/']).next().unwrap_or(chemin)
}

#[derive(Debug, Default)]
struct EtatFactice {
    premier_plan: Option<ApplicationPremierPlan>,
    fenetres: Vec<ApplicationPremierPlan>,
    sondages: usize,
    enumerations: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PremierPlanFactice {
    etat: Arc<Mutex<EtatFactice>>,
}

impl PremierPlanFactice {
    pub fn nouveau() -> Self {
        Self::default()
    }

    fn etat(&self) -> MutexGuard<'_, EtatFactice> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn mettre_au_premier_plan(&self, application: Option<ApplicationPremierPlan>) {
        self.etat().premier_plan = application;
    }

    pub fn ouvrir(&self, fenetres: Vec<ApplicationPremierPlan>) {
        self.etat().fenetres = fenetres;
    }

    pub fn sondages(&self) -> usize {
        self.etat().sondages
    }

    pub fn enumerations(&self) -> usize {
        self.etat().enumerations
    }
}

impl PremierPlan for PremierPlanFactice {
    fn disponible(&self) -> bool {
        true
    }
    fn application(&self) -> Resultat<Option<ApplicationPremierPlan>> {
        let mut etat = self.etat();
        etat.sondages += 1;
        Ok(etat.premier_plan.clone())
    }
    fn fenetres(&self) -> Resultat<Vec<ApplicationPremierPlan>> {
        let mut etat = self.etat();
        etat.enumerations += 1;
        Ok(etat.fenetres.clone())
    }
}

pub fn application_de_chemin(chemin: &str) -> ApplicationPremierPlan {
    ApplicationPremierPlan {
        executable: nom_de_fichier(chemin).to_string(),
        chemin: chemin.to_string(),
        ..ApplicationPremierPlan::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Erreur;

    #[test]
    fn le_nom_du_fichier_se_lit_avec_les_deux_separateurs() {
        assert_eq!(nom_de_fichier(r"C:\Jeux\Hades\Hades.exe"), "Hades.exe");
        assert_eq!(nom_de_fichier("D:/Jeux/a b/jeu.exe"), "jeu.exe");
        assert_eq!(nom_de_fichier("jeu.exe"), "jeu.exe");
    }

    #[test]
    fn la_detection_n_existe_que_sous_windows() {
        let systeme = PremierPlanSysteme;
        assert_eq!(systeme.disponible(), cfg!(windows));
        if !cfg!(windows) {
            assert!(matches!(
                systeme.application(),
                Err(Erreur::Indisponible(_))
            ));
            assert!(matches!(systeme.fenetres(), Err(Erreur::Indisponible(_))));
        }
    }

    #[test]
    fn la_fausse_detection_compte_ses_sondages() {
        let factice = PremierPlanFactice::nouveau();
        assert_eq!(factice.application().unwrap(), None);
        let jeu = application_de_chemin(r"C:\Jeux\cs2.exe");
        factice.mettre_au_premier_plan(Some(jeu.clone()));
        factice.ouvrir(vec![jeu.clone()]);
        assert_eq!(factice.application().unwrap(), Some(jeu.clone()));
        assert_eq!(factice.fenetres().unwrap(), vec![jeu]);
        assert_eq!((factice.sondages(), factice.enumerations()), (2, 1));
    }
}
