use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const IDENTIFIANT: &str = "fr.plbls.estompe";
const MARQUEUR: &str = "version-webview";
const DOSSIERS_PERIMES: &[&str] = &["Service Worker", "Cache", "Code Cache", "GPUCache"];

pub fn dossier_donnees() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(IDENTIFIANT))
}

pub fn purger_si_nouvelle_version(dossier: &Path, version: &str) -> bool {
    let marqueur = dossier.join(MARQUEUR);
    if fs::read_to_string(&marqueur).is_ok_and(|v| v.trim() == version) {
        return false;
    }
    let profil = dossier.join("EBWebView").join("Default");
    let tout_retire = DOSSIERS_PERIMES
        .iter()
        .all(|d| match fs::remove_dir_all(profil.join(d)) {
            Ok(()) => true,
            Err(e) => e.kind() == ErrorKind::NotFound,
        });
    if tout_retire && fs::create_dir_all(dossier).is_ok() {
        let _ = fs::write(&marqueur, version);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier_temporaire(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("estompe-webview-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn profil_avec_donnees(dossier: &Path) -> PathBuf {
        let profil = dossier.join("EBWebView").join("Default");
        for d in [
            "Service Worker/CacheStorage",
            "Cache",
            "Code Cache",
            "Local Storage",
            "IndexedDB",
        ] {
            fs::create_dir_all(profil.join(d)).unwrap();
            fs::write(profil.join(d).join("donnee"), "x").unwrap();
        }
        profil
    }

    #[test]
    fn purge_les_caches_du_front_a_chaque_nouvelle_version_et_garde_le_stockage() {
        let dossier = dossier_temporaire("version");
        let profil = profil_avec_donnees(&dossier);
        assert!(purger_si_nouvelle_version(&dossier, "0.87.2"));
        for d in ["Service Worker", "Cache", "Code Cache"] {
            assert!(!profil.join(d).exists(), "{d} devrait être supprimé");
        }
        assert!(profil.join("Local Storage/donnee").exists());
        assert!(profil.join("IndexedDB/donnee").exists());
        assert_eq!(
            fs::read_to_string(dossier.join(MARQUEUR)).unwrap(),
            "0.87.2"
        );

        fs::create_dir_all(profil.join("Service Worker")).unwrap();
        assert!(!purger_si_nouvelle_version(&dossier, "0.87.2"));
        assert!(profil.join("Service Worker").exists());

        assert!(purger_si_nouvelle_version(&dossier, "0.88.0"));
        assert!(!profil.join("Service Worker").exists());
        fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn premier_lancement_sans_donnees() {
        let dossier = dossier_temporaire("vide");
        assert!(purger_si_nouvelle_version(&dossier, "0.87.2"));
        assert_eq!(
            fs::read_to_string(dossier.join(MARQUEUR)).unwrap(),
            "0.87.2"
        );
        fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn identifiant_identique_a_la_configuration() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], IDENTIFIANT);
    }
}
