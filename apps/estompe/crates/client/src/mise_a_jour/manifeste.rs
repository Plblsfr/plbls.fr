use std::collections::BTreeMap;

use serde::Deserialize;

use super::{nom_de_fichier, version_du_fichier, ErreurMiseAJour};
use crate::version::{disponibilite, MiseAJourDisponible, Publication};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Paquet {
    pub url: String,
    pub signature: String,
    #[serde(default)]
    pub taille: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifeste {
    #[serde(flatten)]
    pub publication: Publication,
    #[serde(default)]
    pub platforms: BTreeMap<String, Paquet>,
}

impl Manifeste {
    pub fn lire(texte: &str) -> Result<Self, ErreurMiseAJour> {
        let manifeste: Self =
            serde_json::from_str(texte).map_err(|e| ErreurMiseAJour::Manifeste(e.to_string()))?;
        let version = manifeste.publication.version.as_str();
        if version_du_fichier(&nom_de_fichier(version)) != Some(version) {
            return Err(ErreurMiseAJour::Manifeste(format!(
                "version « {version} » illisible"
            )));
        }
        Ok(manifeste)
    }

    pub fn paquet_pour(&self, cible: &str) -> Option<&Paquet> {
        self.platforms.get(cible)
    }

    pub fn disponibilite(&self, installee: &str, cible: &str) -> Option<MiseAJourDisponible> {
        self.paquet_pour(cible)?;
        disponibilite(installee, &self.publication)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON_MANIFESTE: &str = r#"{
  "version": "0.2.0",
  "notes": "Notifications du système.",
  "pub_date": "2026-11-20T12:00:00.000Z",
  "mandatory": false,
  "force": false,
  "minVersion": "",
  "message": "",
  "platforms": {
    "windows-x86_64": {
      "url": "https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-v0.2.0/Estompe-natif-beta-0.2.0-setup.exe",
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK",
      "taille": 31457280
    }
  }
}"#;

    #[test]
    fn lit_le_manifeste_publie_par_la_livraison() {
        let m = Manifeste::lire(JSON_MANIFESTE).unwrap();
        assert_eq!(m.publication.version, "0.2.0");
        assert_eq!(
            m.publication.notes.as_deref(),
            Some("Notifications du système.")
        );
        assert_eq!(m.publication.mandatory, Some(false));
        let paquet = m.paquet_pour("windows-x86_64").unwrap();
        assert_eq!(
            paquet.url,
            "https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-v0.2.0/Estompe-natif-beta-0.2.0-setup.exe"
        );
        assert_eq!(paquet.signature, "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK");
        assert_eq!(paquet.taille, Some(31_457_280));
        assert!(m.paquet_pour("linux-x86_64").is_none());
    }

    #[test]
    fn propose_seulement_une_version_superieure_avec_un_paquet_pour_la_cible() {
        let m = Manifeste::lire(JSON_MANIFESTE).unwrap();
        let dispo = m.disponibilite("0.1.0", "windows-x86_64").unwrap();
        assert_eq!(dispo.version, "0.2.0");
        assert!(!dispo.imposee);
        assert_eq!(m.disponibilite("0.2.0", "windows-x86_64"), None);
        assert_eq!(m.disponibilite("0.3.0", "windows-x86_64"), None);
        assert_eq!(m.disponibilite("0.1.0", "linux-x86_64"), None);
    }

    #[test]
    fn la_politique_de_forcage_du_manifeste_impose_la_mise_a_jour() {
        let texte = JSON_MANIFESTE
            .replace(r#""minVersion": """#, r#""minVersion": "0.2.0""#)
            .replace(r#""message": """#, r#""message": "Faille corrigée""#);
        let m = Manifeste::lire(&texte).unwrap();
        let dispo = m.disponibilite("0.1.5", "windows-x86_64").unwrap();
        assert!(dispo.imposee);
        assert_eq!(dispo.message, "Faille corrigée");
    }

    #[test]
    fn un_manifeste_illisible_est_une_erreur_lisible() {
        assert!(matches!(
            Manifeste::lire("<html>"),
            Err(ErreurMiseAJour::Manifeste(_))
        ));
    }

    #[test]
    fn refuse_une_version_qui_ne_donne_pas_un_nom_de_fichier_sur() {
        for version in ["9.0.0\\..\\..\\x", "9.0.0/../x", "", "1.2.3-beta", "1..2"] {
            let texte = JSON_MANIFESTE.replace(
                r#""version": "0.2.0""#,
                &format!(r#""version": {}"#, serde_json::to_string(version).unwrap()),
            );
            assert!(
                matches!(Manifeste::lire(&texte), Err(ErreurMiseAJour::Manifeste(_))),
                "{version}"
            );
        }
    }
}
