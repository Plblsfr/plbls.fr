mod machine;
mod manifeste;
mod politique;
mod service;
mod signature;
mod telechargement;

#[cfg(test)]
mod essais;

use std::fmt;
use std::path::{Path, PathBuf};

pub use machine::{
    Contexte, EffetMaj, EtatMaj, EvenementMaj, MachineMaj, Phase, ATTENTE_AVANT_BLOCAGE,
};
pub use manifeste::{Manifeste, Paquet};
pub use politique::{PolitiqueAdresses, Refus, HOTES_PRODUCTION, REDIRECTIONS_MAX};
pub use service::{
    canal, Bloqueur, EmetteurMaj, MiseAJour, OptionsMiseAJour, ReceptionMaj, Redemarreur,
    SourceContexte, SourceMaj, SourceReseau, PREMIERE_VERIFICATION, VERIFICATION_PERIODIQUE,
};
pub use signature::{verifier, verifier_fichier};
pub use telechargement::{EcrivainDisque, Telecharge, Telechargeur};

pub const ADRESSE_MANIFESTE: &str =
    "https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-beta/natif.json";
pub const VARIABLE_MANIFESTE: &str = "ESTOMPE_NATIF_MANIFESTE";
pub const CLE_PUBLIQUE: &str = include_str!("cle.pub");
pub const DOSSIER_MISES_A_JOUR: &str = "mises-a-jour";
pub const TAILLE_MAX: u64 = 200 * 1024 * 1024;
pub const TAILLE_MAX_MANIFESTE: u64 = 1024 * 1024;
const PREFIXE_FICHIER: &str = "Estompe-natif-beta-";
const SUFFIXE_FICHIER: &str = "-setup.exe";
pub const EXTENSION_PARTIELLE: &str = ".partiel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurMiseAJour {
    Adresse(String),
    Reseau(String),
    Serveur(u16),
    Manifeste(String),
    Taille,
    Incomplet { recus: u64, attendus: u64 },
    Signature(String),
    Disque(String),
    Installation(String),
    AucuneMiseAJour,
}

impl fmt::Display for ErreurMiseAJour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adresse(raison) => write!(f, "Adresse de mise à jour refusée : {raison}."),
            Self::Reseau(raison) => write!(
                f,
                "Le serveur des mises à jour est injoignable ({raison}). Réessaie plus tard."
            ),
            Self::Serveur(statut) => write!(
                f,
                "Le serveur des mises à jour a répondu par une erreur ({statut})."
            ),
            Self::Manifeste(raison) => {
                write!(f, "La description de la mise à jour est illisible ({raison}).")
            }
            Self::Taille => f.write_str("L'installeur annoncé est trop gros : téléchargement refusé."),
            Self::Incomplet { recus, attendus } => write!(
                f,
                "L'installeur reçu est incomplet ({recus} octets sur {attendus}) : il a été supprimé."
            ),
            Self::Signature(raison) => write!(
                f,
                "La signature de l'installeur est invalide ({raison}) : il a été supprimé, rien n'a été exécuté."
            ),
            Self::Disque(raison) => write!(f, "Écriture de la mise à jour impossible : {raison}."),
            Self::Installation(raison) => write!(
                f,
                "L'installeur n'a pas pu être lancé ({raison}). L'app actuelle continue de fonctionner."
            ),
            Self::AucuneMiseAJour => {
                f.write_str("Aucune mise à jour à installer pour l'instant.")
            }
        }
    }
}

impl std::error::Error for ErreurMiseAJour {}

pub fn adresse_manifeste(surcharge: Option<&str>, politique: &PolitiqueAdresses) -> String {
    surcharge
        .map(str::trim)
        .filter(|adresse| politique.accepter(adresse).is_ok())
        .unwrap_or(ADRESSE_MANIFESTE)
        .to_string()
}

pub fn nom_de_fichier(version: &str) -> String {
    format!("{PREFIXE_FICHIER}{version}{SUFFIXE_FICHIER}")
}

pub fn version_du_fichier(nom: &str) -> Option<&str> {
    let nom = nom.strip_suffix(EXTENSION_PARTIELLE).unwrap_or(nom);
    let version = nom
        .strip_prefix(PREFIXE_FICHIER)?
        .strip_suffix(SUFFIXE_FICHIER)?;
    (!version.is_empty()
        && version
            .split('.')
            .all(|morceau| !morceau.is_empty() && morceau.bytes().all(|o| o.is_ascii_digit())))
    .then_some(version)
}

pub fn chemin_partiel(destination: &Path) -> PathBuf {
    let mut nom = destination.as_os_str().to_owned();
    nom.push(EXTENSION_PARTIELLE);
    PathBuf::from(nom)
}

pub fn nettoyer(dossier: &Path, version_courante: &str) -> Vec<PathBuf> {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return Vec::new();
    };
    entrees
        .filter_map(Result::ok)
        .filter(|entree| {
            entree
                .file_name()
                .to_str()
                .and_then(version_du_fichier)
                .is_some_and(|version| !crate::version::est_plus_recente(version, version_courante))
        })
        .map(|entree| entree.path())
        .filter(|chemin| std::fs::remove_file(chemin).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_surcharge_de_l_adresse_doit_passer_la_politique() {
        let production = PolitiqueAdresses::production();
        assert_eq!(adresse_manifeste(None, &production), ADRESSE_MANIFESTE);
        assert_eq!(
            adresse_manifeste(Some("http://github.com/x/natif.json"), &production),
            ADRESSE_MANIFESTE
        );
        assert_eq!(
            adresse_manifeste(Some("https://evil.example/natif.json"), &production),
            ADRESSE_MANIFESTE
        );
        assert_eq!(
            adresse_manifeste(
                Some(" https://github.com/Plblsfr/plbls.fr/releases/download/essai/natif.json "),
                &production
            ),
            "https://github.com/Plblsfr/plbls.fr/releases/download/essai/natif.json"
        );
    }

    #[test]
    fn la_cle_publique_versionnee_se_decode() {
        assert!(minisign_verify::PublicKey::decode(CLE_PUBLIQUE).is_ok());
    }

    #[test]
    fn lit_la_version_dans_le_nom_du_fichier() {
        assert_eq!(
            nom_de_fichier("0.2.0"),
            "Estompe-natif-beta-0.2.0-setup.exe"
        );
        assert_eq!(
            version_du_fichier("Estompe-natif-beta-0.2.0-setup.exe"),
            Some("0.2.0")
        );
        assert_eq!(
            version_du_fichier("Estompe-natif-beta-1.10.3-setup.exe.partiel"),
            Some("1.10.3")
        );
        assert_eq!(version_du_fichier("Estompe-natif-beta--setup.exe"), None);
        assert_eq!(
            version_du_fichier("Estompe-natif-beta-../x-setup.exe"),
            None
        );
        assert_eq!(version_du_fichier("autre.exe"), None);
    }

    #[test]
    fn au_demarrage_les_installeurs_deja_installes_sont_supprimes() {
        let dossier = tempfile::tempdir().unwrap();
        for nom in [
            "Estompe-natif-beta-0.1.0-setup.exe",
            "Estompe-natif-beta-0.2.0-setup.exe.partiel",
            "Estompe-natif-beta-0.3.0-setup.exe",
            "Estompe-natif-beta-0.3.0-setup.exe.partiel",
            "notes.txt",
        ] {
            std::fs::write(dossier.path().join(nom), b"x").unwrap();
        }
        let mut supprimes: Vec<String> = nettoyer(dossier.path(), "0.2.0")
            .iter()
            .map(|c| c.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        supprimes.sort();
        assert_eq!(
            supprimes,
            [
                "Estompe-natif-beta-0.1.0-setup.exe",
                "Estompe-natif-beta-0.2.0-setup.exe.partiel"
            ]
        );
        let mut restants: Vec<String> = std::fs::read_dir(dossier.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        restants.sort();
        assert_eq!(
            restants,
            [
                "Estompe-natif-beta-0.3.0-setup.exe",
                "Estompe-natif-beta-0.3.0-setup.exe.partiel",
                "notes.txt"
            ]
        );
        assert!(nettoyer(&dossier.path().join("absent"), "0.2.0").is_empty());
    }

    #[test]
    fn les_erreurs_sont_lisibles() {
        assert!(ErreurMiseAJour::Signature("clé inattendue".into())
            .to_string()
            .contains("rien n'a été exécuté"));
        assert!(ErreurMiseAJour::Installation("refus".into())
            .to_string()
            .contains("continue de fonctionner"));
        assert!(ErreurMiseAJour::Incomplet {
            recus: 10,
            attendus: 20
        }
        .to_string()
        .contains("10 octets sur 20"));
    }
}
