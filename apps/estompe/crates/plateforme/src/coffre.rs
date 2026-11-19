use crate::{Coffre, Erreur, Resultat};
use std::collections::HashMap;
use std::sync::Mutex;

const SONDE: &str = "sonde-disponibilite";

#[derive(Debug, Clone)]
pub struct CoffreSysteme {
    service: String,
}

impl CoffreSysteme {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn entree(&self, cle: &str) -> Resultat<keyring::Entry> {
        keyring::Entry::new(&self.service, cle).map_err(echec)
    }
}

fn echec(e: keyring::Error) -> Erreur {
    match e {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            Erreur::Indisponible("coffre de secrets")
        }
        autre => Erreur::Echec(autre.to_string()),
    }
}

impl Coffre for CoffreSysteme {
    fn disponible(&self) -> bool {
        self.lire(SONDE).is_ok()
    }

    fn lire(&self, cle: &str) -> Resultat<Option<String>> {
        match self.entree(cle)?.get_password() {
            Ok(valeur) => Ok(Some(valeur)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(echec(e)),
        }
    }

    fn ecrire(&self, cle: &str, valeur: &str) -> Resultat<()> {
        self.entree(cle)?.set_password(valeur).map_err(echec)
    }

    fn effacer(&self, cle: &str) -> Resultat<()> {
        match self.entree(cle)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(echec(e)),
        }
    }
}

#[derive(Debug, Default)]
pub struct CoffreMemoire(Mutex<HashMap<String, String>>);

impl CoffreMemoire {
    fn secrets(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Coffre for CoffreMemoire {
    fn disponible(&self) -> bool {
        true
    }

    fn lire(&self, cle: &str) -> Resultat<Option<String>> {
        Ok(self.secrets().get(cle).cloned())
    }

    fn ecrire(&self, cle: &str, valeur: &str) -> Resultat<()> {
        self.secrets().insert(cle.to_owned(), valeur.to_owned());
        Ok(())
    }

    fn effacer(&self, cle: &str) -> Resultat<()> {
        self.secrets().remove(cle);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrat(coffre: &dyn Coffre, cle: &str) {
        coffre.effacer(cle).unwrap();
        assert_eq!(coffre.lire(cle).unwrap(), None, "clé absente");
        coffre.ecrire(cle, "premier secret").unwrap();
        assert_eq!(coffre.lire(cle).unwrap().as_deref(), Some("premier secret"));
        coffre.ecrire(cle, "second secret é").unwrap();
        assert_eq!(
            coffre.lire(cle).unwrap().as_deref(),
            Some("second secret é")
        );
        coffre.effacer(cle).unwrap();
        assert_eq!(coffre.lire(cle).unwrap(), None, "effacée");
        coffre.effacer(cle).unwrap();
    }

    #[test]
    fn coffre_en_memoire_ecrire_lire_effacer_cle_absente() {
        contrat(&CoffreMemoire::default(), "jeton");
    }

    #[test]
    fn coffre_du_systeme_ecrire_lire_effacer_cle_absente() {
        let coffre = CoffreSysteme::new("fr.plbls.estompe.natif.tests");
        if !coffre.disponible() {
            assert!(
                std::env::var_os("ESTOMPE_COFFRE_EXIGE").is_none(),
                "ESTOMPE_COFFRE_EXIGE est défini mais le coffre du système ne répond pas"
            );
            eprintln!("ignoré : aucun coffre de secrets sur cette machine");
            assert!(matches!(
                coffre.ecrire("jeton", "x"),
                Err(Erreur::Indisponible(_))
            ));
            return;
        }
        contrat(&coffre, &format!("jeton-{}", std::process::id()));
    }
}
