use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::preferences::ecrire_atomiquement;

pub const NOM_FICHIER: &str = "brouillons.json";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Brouillons {
    textes: BTreeMap<String, String>,
    chemin: Option<PathBuf>,
    modifies: bool,
}

impl Brouillons {
    pub fn en_memoire() -> Self {
        Self::default()
    }

    pub fn charger(dossier: &Path) -> Self {
        let chemin = dossier.join(NOM_FICHIER);
        let textes: BTreeMap<String, String> = fs::read(&chemin)
            .ok()
            .and_then(|o| serde_json::from_slice(&o).ok())
            .unwrap_or_default();
        Self {
            textes: textes.into_iter().filter(|(_, t)| !t.is_empty()).collect(),
            chemin: Some(chemin),
            modifies: false,
        }
    }

    pub fn texte(&self, cle: &str) -> &str {
        self.textes.get(cle).map(String::as_str).unwrap_or_default()
    }

    pub fn definir(&mut self, cle: &str, texte: &str) -> bool {
        let change = if texte.is_empty() {
            self.textes.remove(cle).is_some()
        } else if self.texte(cle) != texte {
            self.textes.insert(cle.to_string(), texte.to_string());
            true
        } else {
            false
        };
        self.modifies |= change;
        change
    }

    pub fn effacer(&mut self, cle: &str) -> bool {
        self.definir(cle, "")
    }

    pub fn a_un_brouillon(&self, cle: &str) -> bool {
        self.textes.contains_key(cle)
    }

    pub fn cles(&self) -> impl Iterator<Item = &str> {
        self.textes.keys().map(String::as_str)
    }

    pub fn a_enregistrer(&self) -> bool {
        self.modifies && self.chemin.is_some()
    }

    pub fn enregistrer(&mut self) -> io::Result<()> {
        let Some(chemin) = &self.chemin else {
            return Ok(());
        };
        if !self.modifies {
            return Ok(());
        }
        if let Some(parent) = chemin.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut octets = serde_json::to_vec_pretty(&self.textes)?;
        octets.push(b'\n');
        ecrire_atomiquement(chemin, &octets)?;
        self.modifies = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garde_le_brouillon_de_chaque_conversation() {
        let mut b = Brouillons::en_memoire();
        assert!(b.definir("c:1", "bonjour"));
        assert!(!b.definir("c:1", "bonjour"));
        b.definir("c:2", "");
        assert_eq!(b.texte("c:1"), "bonjour");
        assert_eq!(b.texte("c:2"), "");
        assert_eq!(b.texte("c:3"), "");
    }

    #[test]
    fn oublie_le_brouillon_a_l_envoi() {
        let mut b = Brouillons::en_memoire();
        b.definir("c:1", "bonjour");
        assert!(b.effacer("c:1"));
        assert_eq!(b.texte("c:1"), "");
        assert!(!b.effacer("c:1"));
    }

    #[test]
    fn signale_les_conversations_ayant_un_brouillon() {
        let mut b = Brouillons::en_memoire();
        b.definir("c:1", "a");
        b.definir("c:2", "b");
        b.definir("c:2", "");
        assert_eq!(b.cles().collect::<Vec<_>>(), ["c:1"]);
        assert!(b.a_un_brouillon("c:1") && !b.a_un_brouillon("c:2"));
    }

    #[test]
    fn retrouve_les_brouillons_apres_un_redemarrage() {
        let dossier = tempfile::tempdir().unwrap();
        let mut b = Brouillons::charger(dossier.path());
        assert!(!b.a_enregistrer());
        b.definir("c:1", "à suivre…\nligne 2");
        b.definir("d:9", "salut");
        assert!(b.a_enregistrer());
        b.enregistrer().unwrap();
        assert!(!b.a_enregistrer());
        let relu = Brouillons::charger(dossier.path());
        assert_eq!(relu.texte("c:1"), "à suivre…\nligne 2");
        assert_eq!(relu.cles().collect::<Vec<_>>(), ["c:1", "d:9"]);
        let noms: Vec<String> = fs::read_dir(dossier.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(noms, [NOM_FICHIER]);
    }

    #[test]
    fn un_fichier_illisible_donne_des_brouillons_vides() {
        let dossier = tempfile::tempdir().unwrap();
        fs::write(dossier.path().join(NOM_FICHIER), b"{pas du json").unwrap();
        let b = Brouillons::charger(dossier.path());
        assert_eq!(b.cles().count(), 0);
    }
}
