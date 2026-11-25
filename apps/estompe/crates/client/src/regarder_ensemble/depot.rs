use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bytes::Bytes;

pub const RACINE: &str = "entracte-avance";
pub const RESTES_MS: i64 = 24 * 3600 * 1000;
const MARQUE: &str = ".vu";

pub trait Depot: Send {
    fn contient(&self, nom: &str) -> bool;
    fn lire(&mut self, nom: &str) -> Option<Bytes>;
    fn ecrire(&mut self, nom: &str, octets: Bytes);
    fn retirer(&mut self, nom: &str);
    fn noms(&self) -> Vec<String>;
    fn octets(&self) -> u64;
    fn fermer(&mut self, purger: bool);
}

pub type DepotPartage = Arc<Mutex<Box<dyn Depot>>>;

pub fn partager(depot: Box<dyn Depot>) -> DepotPartage {
    Arc::new(Mutex::new(depot))
}

#[derive(Debug, Default)]
pub struct Memoire {
    ordre: Vec<String>,
    segments: HashMap<String, Bytes>,
    taille: u64,
}

impl Depot for Memoire {
    fn contient(&self, nom: &str) -> bool {
        self.segments.contains_key(nom)
    }

    fn lire(&mut self, nom: &str) -> Option<Bytes> {
        self.segments.get(nom).cloned()
    }

    fn ecrire(&mut self, nom: &str, octets: Bytes) {
        let taille = octets.len() as u64;
        match self.segments.insert(nom.to_string(), octets) {
            Some(ancien) => self.taille -= ancien.len() as u64,
            None => self.ordre.push(nom.to_string()),
        }
        self.taille += taille;
    }

    fn retirer(&mut self, nom: &str) {
        if let Some(ancien) = self.segments.remove(nom) {
            self.taille -= ancien.len() as u64;
            self.ordre.retain(|n| n != nom);
        }
    }

    fn noms(&self) -> Vec<String> {
        self.ordre.clone()
    }

    fn octets(&self) -> u64 {
        self.taille
    }

    fn fermer(&mut self, _purger: bool) {
        self.ordre.clear();
        self.segments.clear();
        self.taille = 0;
    }
}

pub fn dossier_de(cle: &str) -> String {
    cle.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect()
}

fn marquer(dossier: &Path, maintenant_ms: i64) -> std::io::Result<()> {
    fs::write(dossier.join(MARQUE), maintenant_ms.to_string())
}

fn vu_le(dossier: &Path) -> i64 {
    fs::read_to_string(dossier.join(MARQUE))
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

pub fn purger_restes(racine: Option<&Path>, sauf: Option<&str>, maintenant_ms: i64, age_ms: i64) {
    let Some(racine) = racine else {
        return;
    };
    let Ok(entrees) = fs::read_dir(racine.join(RACINE)) else {
        return;
    };
    for entree in entrees.flatten() {
        let nom = entree.file_name().to_string_lossy().into_owned();
        if Some(nom.as_str()) == sauf || !entree.path().is_dir() {
            continue;
        }
        if maintenant_ms - vu_le(&entree.path()) > age_ms {
            let _ = fs::remove_dir_all(entree.path());
        }
    }
}

pub fn ouvrir_depot(
    cle: Option<&str>,
    racine: Option<&Path>,
    maintenant_ms: i64,
) -> Box<dyn Depot> {
    let (Some(cle), Some(racine)) = (cle.filter(|c| !c.is_empty()), racine) else {
        return Box::new(Memoire::default());
    };
    let dossier = racine.join(RACINE).join(dossier_de(cle));
    if fs::create_dir_all(&dossier).is_err() || marquer(&dossier, maintenant_ms).is_err() {
        return Box::new(Memoire::default());
    }
    let mut noms: Vec<(String, u64)> = fs::read_dir(&dossier)
        .map(|e| {
            e.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        e.metadata().map_or(0, |m| m.len()),
                    )
                })
                .filter(|(n, _)| n != MARQUE)
                .collect()
        })
        .unwrap_or_default();
    noms.sort();
    let taille = noms.iter().map(|(_, t)| t).sum();
    Box::new(DepotDisque {
        dossier,
        ordre: noms.iter().map(|(n, _)| n.clone()).collect(),
        tailles: noms.into_iter().collect(),
        taille,
        secours: Memoire::default(),
        ferme: false,
    })
}

struct DepotDisque {
    dossier: PathBuf,
    ordre: Vec<String>,
    tailles: HashMap<String, u64>,
    taille: u64,
    secours: Memoire,
    ferme: bool,
}

impl DepotDisque {
    fn oublier(&mut self, nom: &str) {
        if let Some(t) = self.tailles.remove(nom) {
            self.taille -= t;
            self.ordre.retain(|n| n != nom);
        }
    }
}

impl Depot for DepotDisque {
    fn contient(&self, nom: &str) -> bool {
        self.tailles.contains_key(nom) || self.secours.contient(nom)
    }

    fn lire(&mut self, nom: &str) -> Option<Bytes> {
        if self.secours.contient(nom) {
            return self.secours.lire(nom);
        }
        if !self.tailles.contains_key(nom) {
            return None;
        }
        match fs::read(self.dossier.join(nom)) {
            Ok(o) => Some(Bytes::from(o)),
            Err(_) => {
                self.oublier(nom);
                None
            }
        }
    }

    fn ecrire(&mut self, nom: &str, octets: Bytes) {
        if self.ferme || self.contient(nom) || nom == MARQUE || nom.contains(['/', '\\']) {
            return;
        }
        match fs::write(self.dossier.join(nom), &octets) {
            Ok(()) => {
                self.tailles.insert(nom.to_string(), octets.len() as u64);
                self.ordre.push(nom.to_string());
                self.taille += octets.len() as u64;
            }
            Err(_) => self.secours.ecrire(nom, octets),
        }
    }

    fn retirer(&mut self, nom: &str) {
        if self.secours.contient(nom) {
            self.secours.retirer(nom);
            return;
        }
        if self.tailles.contains_key(nom) {
            self.oublier(nom);
            let _ = fs::remove_file(self.dossier.join(nom));
        }
    }

    fn noms(&self) -> Vec<String> {
        let mut noms = self.ordre.clone();
        noms.extend(self.secours.noms());
        noms
    }

    fn octets(&self) -> u64 {
        self.taille + self.secours.octets()
    }

    fn fermer(&mut self, purger: bool) {
        self.ferme = true;
        self.secours.fermer(true);
        if !purger {
            return;
        }
        self.tailles.clear();
        self.ordre.clear();
        self.taille = 0;
        let _ = fs::remove_dir_all(&self.dossier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn octets(n: usize) -> Bytes {
        Bytes::from(vec![0u8; n])
    }

    fn enfants(dossier: &Path) -> Vec<String> {
        let mut noms: Vec<String> = fs::read_dir(dossier)
            .map(|e| {
                e.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        noms.sort();
        noms
    }

    #[test]
    fn garde_en_memoire_sans_cle_ou_sans_stockage() {
        let racine = tempfile::tempdir().unwrap();
        for mut d in [
            ouvrir_depot(None, Some(racine.path()), 0),
            ouvrir_depot(Some("f-480p-0-g"), None, 0),
        ] {
            d.ecrire("seg0.ts", octets(5));
            assert_eq!((d.contient("seg0.ts"), d.octets()), (true, 5));
        }
        assert!(enfants(racine.path()).is_empty());
    }

    #[test]
    fn ecrit_sur_le_disque_et_retrouve_apres_un_redemarrage() {
        let racine = tempfile::tempdir().unwrap();
        let mut a = ouvrir_depot(Some("f-480p-0-g"), Some(racine.path()), 0);
        a.ecrire("seg00000.ts", octets(10));
        a.ecrire("seg00001.ts", octets(20));
        a.ecrire("seg00001.ts", octets(99));
        assert_eq!(
            (a.octets(), a.noms()),
            (
                30,
                vec!["seg00000.ts".to_string(), "seg00001.ts".to_string()]
            )
        );
        let dossier = racine.path().join(RACINE).join("f-480p-0-g");
        assert_eq!(enfants(&dossier), [".vu", "seg00000.ts", "seg00001.ts"]);
        a.fermer(false);
        let mut b = ouvrir_depot(Some("f-480p-0-g"), Some(racine.path()), 0);
        assert_eq!(
            (b.octets(), b.contient("seg00001.ts"), b.contient(".vu")),
            (30, true, false)
        );
        assert_eq!(b.lire("seg00001.ts").unwrap().len(), 20);
        assert_eq!(b.lire("seg00009.ts"), None);
        b.retirer("seg00000.ts");
        assert_eq!((b.octets(), b.contient("seg00000.ts")), (20, false));
        assert!(!dossier.join("seg00000.ts").exists());
    }

    #[test]
    fn separe_qualites_et_pistes_et_supprime_les_segments_d_un_titre_a_sa_fermeture() {
        let racine = tempfile::tempdir().unwrap();
        let mut a = ouvrir_depot(Some("f-480p-0-g"), Some(racine.path()), 0);
        a.ecrire("seg00000.ts", octets(10));
        let mut b = ouvrir_depot(Some("f-720p-0-g"), Some(racine.path()), 0);
        assert!(!b.contient("seg00000.ts"));
        b.ecrire("seg00000.ts", octets(40));
        a.fermer(true);
        assert_eq!(enfants(&racine.path().join(RACINE)), ["f-720p-0-g"]);
        assert_eq!(a.octets(), 0);
        a.ecrire("seg00001.ts", octets(10));
        assert!(!a.contient("seg00001.ts"));
        assert_eq!(dossier_de("../a b/é"), "___a_b__");
    }

    #[test]
    fn continue_en_memoire_quand_l_ecriture_est_refusee() {
        let racine = tempfile::tempdir().unwrap();
        let mut a = ouvrir_depot(Some("f-480p-0-g"), Some(racine.path()), 0);
        a.ecrire("seg00000.ts", octets(10));
        fs::remove_dir_all(racine.path().join(RACINE)).unwrap();
        a.ecrire("seg00001.ts", octets(20));
        assert_eq!(
            (a.contient("seg00001.ts"), a.octets(), a.noms()),
            (
                true,
                30,
                vec!["seg00000.ts".to_string(), "seg00001.ts".to_string()]
            )
        );
        assert_eq!(a.lire("seg00001.ts").unwrap().len(), 20);
        a.retirer("seg00001.ts");
        assert_eq!(a.octets(), 10);
        assert_eq!(a.lire("seg00000.ts"), None);
        assert_eq!(a.octets(), 0);
        let fichier = racine.path().join("pas-un-dossier");
        fs::write(&fichier, "x").unwrap();
        let mut b = ouvrir_depot(Some("f-480p-0-g"), Some(&fichier), 0);
        b.ecrire("seg00000.ts", octets(3));
        assert_eq!(b.octets(), 3);
    }

    #[test]
    fn purge_les_restes_d_une_seance_interrompue_pas_ceux_d_une_recente() {
        let racine = tempfile::tempdir().unwrap();
        let mut maintenant = 1_000_000;
        ouvrir_depot(Some("vieux"), Some(racine.path()), maintenant)
            .ecrire("seg00000.ts", octets(10));
        maintenant += RESTES_MS + 1;
        ouvrir_depot(Some("recent"), Some(racine.path()), maintenant)
            .ecrire("seg00000.ts", octets(10));
        fs::create_dir_all(racine.path().join(RACINE).join("sans-marque")).unwrap();
        purger_restes(Some(racine.path()), None, maintenant + 1000, RESTES_MS);
        assert_eq!(enfants(&racine.path().join(RACINE)), ["recent"]);
        purger_restes(
            Some(racine.path()),
            Some("recent"),
            maintenant + 2 * RESTES_MS,
            RESTES_MS,
        );
        assert_eq!(enfants(&racine.path().join(RACINE)), ["recent"]);
        purger_restes(None, None, 0, RESTES_MS);
        let vide = tempfile::tempdir().unwrap();
        purger_restes(Some(vide.path()), None, 0, RESTES_MS);
    }

    #[test]
    fn compte_les_octets_du_depot_en_memoire() {
        let mut m = Memoire::default();
        m.ecrire("a", octets(4));
        m.ecrire("a", octets(6));
        m.ecrire("b", octets(1));
        assert_eq!(
            (m.octets(), m.noms()),
            (7, vec!["a".to_string(), "b".to_string()])
        );
        m.retirer("a");
        assert_eq!(m.octets(), 1);
        m.fermer(true);
        assert_eq!((m.octets(), m.contient("b")), (0, false));
    }
}
