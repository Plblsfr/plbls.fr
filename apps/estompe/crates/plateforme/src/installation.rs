use crate::{Installation, Resultat};
use std::path::Path;

pub const CIBLE_WINDOWS: &str = "windows-x86_64";
pub const ARGUMENTS_INSTALLEUR: [&str; 2] = ["/S", "/RELANCER"];

#[derive(Debug, Default, Clone, Copy)]
pub struct InstallationSysteme;

#[cfg(windows)]
impl Installation for InstallationSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn cible(&self) -> Option<&'static str> {
        Some(CIBLE_WINDOWS)
    }

    fn lancer(&self, installeur: &Path) -> Resultat<()> {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        if !installeur.is_file() {
            return Err(crate::Erreur::Echec(format!(
                "installeur introuvable : {}",
                installeur.display()
            )));
        }
        Command::new(installeur)
            .args(ARGUMENTS_INSTALLEUR)
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| crate::Erreur::Echec(format!("lancement de l'installeur : {e}")))
    }
}

#[cfg(not(windows))]
impl Installation for InstallationSysteme {
    fn disponible(&self) -> bool {
        false
    }

    fn cible(&self) -> Option<&'static str> {
        crate::Indisponible.cible()
    }

    fn lancer(&self, installeur: &Path) -> Resultat<()> {
        crate::Indisponible.lancer(installeur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_installeur_est_lance_en_mode_silencieux_puis_relance_l_app() {
        assert_eq!(ARGUMENTS_INSTALLEUR, ["/S", "/RELANCER"]);
        assert_eq!(InstallationSysteme.disponible(), cfg!(windows));
        assert_eq!(
            InstallationSysteme.cible(),
            cfg!(windows).then_some(CIBLE_WINDOWS)
        );
    }

    #[test]
    fn un_installeur_absent_n_est_pas_lance() {
        let dossier = tempfile::tempdir().unwrap();
        assert!(InstallationSysteme
            .lancer(&dossier.path().join("Estompe-natif-beta-9.9.9-setup.exe"))
            .is_err());
    }
}
