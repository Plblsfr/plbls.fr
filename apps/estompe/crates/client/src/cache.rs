use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::hash::Hash;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

pub struct Lru<K, V> {
    capacite: usize,
    entrees: HashMap<K, (V, u64)>,
    ordre: BTreeMap<u64, K>,
    horloge: u64,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub fn nouveau(capacite: usize) -> Self {
        Self {
            capacite: capacite.max(1),
            entrees: HashMap::new(),
            ordre: BTreeMap::new(),
            horloge: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entrees.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entrees.is_empty()
    }

    pub fn capacite(&self) -> usize {
        self.capacite
    }

    pub fn contient(&self, cle: &K) -> bool {
        self.entrees.contains_key(cle)
    }

    fn toucher(&mut self, cle: &K) {
        self.horloge += 1;
        let horloge = self.horloge;
        if let Some((_, rang)) = self.entrees.get_mut(cle) {
            self.ordre.remove(rang);
            *rang = horloge;
            self.ordre.insert(horloge, cle.clone());
        }
    }

    pub fn lire(&mut self, cle: &K) -> Option<&V> {
        self.toucher(cle);
        self.entrees.get(cle).map(|(valeur, _)| valeur)
    }

    pub fn inserer(&mut self, cle: K, valeur: V) -> Vec<K> {
        self.horloge += 1;
        if let Some((_, rang)) = self.entrees.remove(&cle) {
            self.ordre.remove(&rang);
        }
        self.ordre.insert(self.horloge, cle.clone());
        self.entrees.insert(cle, (valeur, self.horloge));
        let mut oubliees = Vec::new();
        while self.entrees.len() > self.capacite {
            let Some((_, ancienne)) = self.ordre.pop_first() else {
                break;
            };
            self.entrees.remove(&ancienne);
            oubliees.push(ancienne);
        }
        oubliees
    }

    pub fn retirer(&mut self, cle: &K) -> Option<V> {
        let (valeur, rang) = self.entrees.remove(cle)?;
        self.ordre.remove(&rang);
        Some(valeur)
    }

    pub fn lire_ou_inserer(&mut self, cle: K, fabriquer: impl FnOnce() -> V) -> &V {
        if !self.entrees.contains_key(&cle) {
            self.inserer(cle.clone(), fabriquer());
        }
        self.lire(&cle).expect("entrée présente")
    }
}

const EXTENSION: &str = "cache";
const EXTENSION_TEMPORAIRE: &str = "tmp";
const LONGUEUR_CLE_MAX: usize = 200;

pub fn cle_valide(cle: &str) -> bool {
    !cle.is_empty()
        && cle.len() <= LONGUEUR_CLE_MAX
        && !cle.starts_with('.')
        && cle
            .bytes()
            .all(|o| o.is_ascii_alphanumeric() || matches!(o, b'-' | b'_' | b'.'))
}

#[derive(Default)]
struct Index {
    fichiers: HashMap<String, (u64, u64)>,
    ordre: BTreeMap<u64, String>,
    total: u64,
    rang: u64,
    derniere_date: Option<SystemTime>,
}

impl Index {
    fn placer(&mut self, cle: &str, taille: u64) {
        self.oublier(cle);
        self.rang += 1;
        self.fichiers.insert(cle.to_string(), (taille, self.rang));
        self.ordre.insert(self.rang, cle.to_string());
        self.total += taille;
    }

    fn oublier(&mut self, cle: &str) -> bool {
        match self.fichiers.remove(cle) {
            Some((taille, rang)) => {
                self.ordre.remove(&rang);
                self.total -= taille;
                true
            }
            None => false,
        }
    }

    fn date_suivante(&mut self) -> SystemTime {
        let maintenant = SystemTime::now();
        let date = match self.derniere_date {
            Some(derniere) if maintenant <= derniere => derniere + Duration::from_millis(1),
            _ => maintenant,
        };
        self.derniere_date = Some(date);
        date
    }
}

pub struct CacheDisque {
    dossier: PathBuf,
    borne: u64,
    index: Mutex<Index>,
}

impl CacheDisque {
    pub fn ouvrir(dossier: impl Into<PathBuf>, borne: u64) -> io::Result<Self> {
        let dossier = dossier.into();
        fs::create_dir_all(&dossier)?;
        let mut trouves = Vec::new();
        for entree in fs::read_dir(&dossier)? {
            let entree = entree?;
            let chemin = entree.path();
            let Ok(meta) = entree.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let extension = chemin.extension().and_then(|e| e.to_str());
            let cle = chemin
                .file_stem()
                .and_then(|n| n.to_str())
                .map(str::to_string);
            match (extension, cle) {
                (Some(EXTENSION), Some(cle)) if cle_valide(&cle) => {
                    let date = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    trouves.push((date, cle, meta.len()));
                }
                (Some(EXTENSION_TEMPORAIRE), _) => {
                    let _ = fs::remove_file(&chemin);
                }
                _ => {}
            }
        }
        trouves.sort();
        let mut index = Index::default();
        for (date, cle, taille) in trouves {
            index.derniere_date = Some(index.derniere_date.map_or(date, |d| d.max(date)));
            index.placer(&cle, taille);
        }
        let cache = Self {
            dossier,
            borne,
            index: Mutex::new(index),
        };
        cache.evincer(&mut cache.verrou());
        Ok(cache)
    }

    fn verrou(&self) -> MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn chemin(&self, cle: &str) -> PathBuf {
        self.dossier.join(format!("{cle}.{EXTENSION}"))
    }

    pub fn dossier(&self) -> &Path {
        &self.dossier
    }

    pub fn borne(&self) -> u64 {
        self.borne
    }

    pub fn taille_totale(&self) -> u64 {
        self.verrou().total
    }

    pub fn nombre(&self) -> usize {
        self.verrou().fichiers.len()
    }

    pub fn contient(&self, cle: &str) -> bool {
        self.verrou().fichiers.contains_key(cle)
    }

    pub fn lire(&self, cle: &str) -> Option<Vec<u8>> {
        if !cle_valide(cle) {
            return None;
        }
        let mut index = self.verrou();
        let taille = index.fichiers.get(cle)?.0;
        let chemin = self.chemin(cle);
        match fs::read(&chemin) {
            Ok(octets) => {
                let date = index.date_suivante();
                let _ = File::options()
                    .write(true)
                    .open(&chemin)
                    .and_then(|f| f.set_modified(date));
                index.placer(cle, taille.max(octets.len() as u64));
                Some(octets)
            }
            Err(_) => {
                index.oublier(cle);
                None
            }
        }
    }

    pub fn ecrire(&self, cle: &str, octets: &[u8]) -> io::Result<()> {
        if !cle_valide(cle) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "clé invalide"));
        }
        let taille = octets.len() as u64;
        if taille > self.borne {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "fichier plus grand que la borne du cache",
            ));
        }
        let mut index = self.verrou();
        let temporaire = self.dossier.join(format!("{cle}.{EXTENSION_TEMPORAIRE}"));
        let date = index.date_suivante();
        let resultat = (|| {
            let mut fichier = File::create(&temporaire)?;
            fichier.write_all(octets)?;
            fichier.set_modified(date)?;
            fichier.sync_all()?;
            drop(fichier);
            fs::rename(&temporaire, self.chemin(cle))
        })();
        if let Err(erreur) = resultat {
            let _ = fs::remove_file(&temporaire);
            return Err(erreur);
        }
        index.placer(cle, taille);
        self.evincer(&mut index);
        Ok(())
    }

    pub fn retirer(&self, cle: &str) {
        if !cle_valide(cle) {
            return;
        }
        let mut index = self.verrou();
        index.oublier(cle);
        let _ = fs::remove_file(self.chemin(cle));
    }

    fn evincer(&self, index: &mut Index) {
        while index.total > self.borne {
            let Some((_, cle)) = index.ordre.first_key_value().map(|(r, c)| (*r, c.clone())) else {
                break;
            };
            index.oublier(&cle);
            let _ = fs::remove_file(self.chemin(&cle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_oublie_la_moins_recemment_utilisee_au_dela_de_la_limite() {
        let mut c = Lru::nouveau(2);
        c.inserer("a", 1);
        c.inserer("b", 2);
        assert_eq!(c.lire(&"a"), Some(&1));
        assert_eq!(c.inserer("c", 3), ["b"]);
        assert_eq!(c.len(), 2);
        assert_eq!(c.lire(&"a"), Some(&1));
        assert_eq!(c.lire(&"b"), None);
        assert_eq!(c.lire(&"c"), Some(&3));
    }

    #[test]
    fn lru_ne_calcule_une_valeur_absente_qu_une_fois() {
        let mut c = Lru::nouveau(1);
        let appels = std::cell::Cell::new(0);
        let fabriquer = || {
            appels.set(appels.get() + 1);
            7
        };
        assert_eq!(*c.lire_ou_inserer("x", fabriquer), 7);
        assert_eq!(*c.lire_ou_inserer("x", fabriquer), 7);
        assert_eq!(appels.get(), 1);
        c.inserer("y", 1);
        assert_eq!(*c.lire_ou_inserer("x", fabriquer), 7);
        assert_eq!(appels.get(), 2);
    }

    #[test]
    fn lru_reecrire_une_cle_ne_double_pas_l_entree() {
        let mut c = Lru::nouveau(2);
        c.inserer("a", 1);
        c.inserer("a", 2);
        c.inserer("b", 3);
        assert_eq!((c.len(), c.lire(&"a")), (2, Some(&2)));
        assert_eq!(c.retirer(&"a"), Some(2));
        assert!(!c.contient(&"a") && c.contient(&"b"));
    }

    fn fichiers(dossier: &Path) -> Vec<String> {
        let mut noms: Vec<String> = fs::read_dir(dossier)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        noms.sort();
        noms
    }

    #[test]
    fn le_cache_disque_reste_sous_sa_borne_et_evince_le_moins_recemment_lu() {
        let dossier = tempfile::tempdir().unwrap();
        let cache = CacheDisque::ouvrir(dossier.path(), 30).unwrap();
        cache.ecrire("a", &[1; 10]).unwrap();
        cache.ecrire("b", &[2; 10]).unwrap();
        cache.ecrire("c", &[3; 10]).unwrap();
        assert_eq!(cache.lire("a"), Some(vec![1; 10]));
        cache.ecrire("d", &[4; 10]).unwrap();
        assert_eq!(cache.taille_totale(), 30);
        assert_eq!(fichiers(dossier.path()), ["a.cache", "c.cache", "d.cache"]);
        assert_eq!(cache.lire("b"), None);
        cache.ecrire("gros", &[5; 25]).unwrap();
        assert_eq!(fichiers(dossier.path()), ["gros.cache"]);
        assert!(cache.ecrire("enorme", &[6; 31]).is_err());
        assert!(cache.taille_totale() <= cache.borne());
    }

    #[test]
    fn le_cache_disque_se_relit_apres_redemarrage_avec_l_ordre_des_lectures() {
        let dossier = tempfile::tempdir().unwrap();
        {
            let cache = CacheDisque::ouvrir(dossier.path(), 30).unwrap();
            cache.ecrire("a", &[1; 10]).unwrap();
            cache.ecrire("b", &[2; 10]).unwrap();
            cache.ecrire("c", &[3; 10]).unwrap();
            assert!(cache.lire("a").is_some());
        }
        let cache = CacheDisque::ouvrir(dossier.path(), 30).unwrap();
        assert_eq!((cache.nombre(), cache.taille_totale()), (3, 30));
        assert_eq!(cache.lire("c"), Some(vec![3; 10]));
        cache.ecrire("d", &[4; 10]).unwrap();
        assert!(!cache.contient("b"), "b est le moins récemment lu");
        assert!(cache.contient("a") && cache.contient("c") && cache.contient("d"));
        let reduit = CacheDisque::ouvrir(dossier.path(), 15).unwrap();
        assert_eq!(reduit.nombre(), 1);
        assert!(reduit.contient("d"));
    }

    #[test]
    fn le_cache_disque_ignore_les_fichiers_etrangers_et_les_cles_invalides() {
        let dossier = tempfile::tempdir().unwrap();
        fs::write(dossier.path().join("a.tmp"), b"interrompu").unwrap();
        fs::write(dossier.path().join("autre.txt"), b"rien").unwrap();
        fs::create_dir(dossier.path().join("x.cache")).unwrap();
        let cache = CacheDisque::ouvrir(dossier.path(), 100).unwrap();
        assert_eq!(cache.nombre(), 0);
        assert!(!dossier.path().join("a.tmp").exists());
        assert!(dossier.path().join("autre.txt").exists());
        for invalide in ["", "../x", "a/b", ".cache", "é"] {
            assert!(cache.ecrire(invalide, b"1").is_err(), "{invalide}");
            assert_eq!(cache.lire(invalide), None);
        }
        cache.ecrire("a", b"123").unwrap();
        fs::remove_file(dossier.path().join("a.cache")).unwrap();
        assert_eq!(cache.lire("a"), None);
        assert_eq!(cache.taille_totale(), 0);
        cache.ecrire("b", b"45").unwrap();
        cache.retirer("b");
        assert_eq!((cache.nombre(), cache.lire("b")), (0, None));
    }
}
