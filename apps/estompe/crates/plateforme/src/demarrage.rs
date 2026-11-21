use crate::{DemarrageSession, Resultat};
use std::path::Path;

pub const VALEUR_DEMARRAGE: &str = "EstompeNatif";
pub const OPTION_ARRIERE_PLAN: &str = "--arriere-plan";
pub const CLE_DEMARRAGE: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemarrageSysteme {
    pub nom_valeur: String,
    pub commande: String,
}

pub fn commande_de_demarrage(executable: &Path) -> String {
    format!("\"{}\" {OPTION_ARRIERE_PLAN}", executable.display())
}

impl DemarrageSysteme {
    pub fn de_l_executable() -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        Some(Self {
            nom_valeur: VALEUR_DEMARRAGE.to_string(),
            commande: commande_de_demarrage(&executable),
        })
    }

    #[cfg(windows)]
    pub fn commande_enregistree(&self) -> Resultat<Option<String>> {
        crate::registre_windows::lire(CLE_DEMARRAGE, &self.nom_valeur)
    }
}

#[cfg(windows)]
impl DemarrageSession for DemarrageSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn actif(&self) -> Resultat<bool> {
        Ok(self.commande_enregistree()?.is_some())
    }

    fn definir(&self, actif: bool) -> Resultat<()> {
        if actif {
            crate::registre_windows::ecrire(CLE_DEMARRAGE, &self.nom_valeur, &self.commande)
        } else {
            crate::registre_windows::effacer(CLE_DEMARRAGE, &self.nom_valeur)
        }
    }
}

#[cfg(not(windows))]
impl DemarrageSession for DemarrageSysteme {
    fn disponible(&self) -> bool {
        false
    }

    fn actif(&self) -> Resultat<bool> {
        crate::Indisponible.actif()
    }

    fn definir(&self, actif: bool) -> Resultat<()> {
        crate::Indisponible.definir(actif)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_commande_lance_l_executable_en_arriere_plan() {
        assert_eq!(
            commande_de_demarrage(Path::new("C:/Programmes/Estompe natif/estompe-natif.exe")),
            "\"C:/Programmes/Estompe natif/estompe-natif.exe\" --arriere-plan"
        );
    }

    #[cfg(windows)]
    #[test]
    fn la_valeur_de_demarrage_est_ecrite_puis_retiree() {
        let demarrage = DemarrageSysteme {
            nom_valeur: format!("EstompeNatifEssai{}", std::process::id()),
            commande: "\"C:\\essai.exe\" --arriere-plan".into(),
        };
        demarrage.definir(true).unwrap();
        assert!(demarrage.actif().unwrap());
        assert_eq!(
            demarrage.commande_enregistree().unwrap().as_deref(),
            Some(demarrage.commande.as_str())
        );
        demarrage.definir(false).unwrap();
        assert!(!demarrage.actif().unwrap());
        assert_eq!(demarrage.commande_enregistree().unwrap(), None);
        demarrage.definir(false).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn le_demarrage_est_indisponible_hors_de_windows() {
        let demarrage = DemarrageSysteme::de_l_executable().unwrap();
        assert!(!demarrage.disponible());
        assert!(matches!(
            demarrage.definir(true),
            Err(crate::Erreur::Indisponible(_))
        ));
    }
}
