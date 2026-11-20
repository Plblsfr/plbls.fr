use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

pub struct Lru<K, V> {
    capacite: usize,
    entrees: HashMap<K, (V, u64)>,
    ordre: BTreeMap<u64, K>,
    horloge: u64,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub fn nouveau(capacite: usize) -> Self {
        Self {
            capacite,
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

    pub fn vider(&mut self) {
        self.entrees.clear();
        self.ordre.clear();
    }

    pub fn retirer_le_plus_ancien(&mut self) -> Option<(K, V)> {
        let (_, cle) = self.ordre.pop_first()?;
        let (valeur, _) = self.entrees.remove(&cle)?;
        Some((cle, valeur))
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Lru<K, V> {
    pub fn lire_ou_inserer(&mut self, cle: K, fabriquer: impl FnOnce() -> V) -> V {
        if let Some(valeur) = self.lire(&cle) {
            return valeur.clone();
        }
        let valeur = fabriquer();
        self.inserer(cle, valeur.clone());
        valeur
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn oublie_la_moins_recemment_utilisee_au_dela_de_la_limite() {
        let mut c: Lru<&str, i32> = Lru::nouveau(2);
        c.inserer("a", 1);
        c.inserer("b", 2);
        assert_eq!(c.lire(&"a"), Some(&1));
        assert_eq!(c.inserer("c", 3), ["b"]);
        let taille = c.len();
        let a = c.lire(&"a").copied();
        let b = c.lire(&"b").copied();
        let c3 = c.lire(&"c").copied();
        assert_eq!((taille, a, b, c3), (2, Some(1), None, Some(3)));
    }

    #[test]
    fn ne_calcule_une_valeur_absente_qu_une_fois() {
        let mut c: Lru<String, i32> = Lru::nouveau(1);
        let appels = Cell::new(0);
        let creer = || {
            appels.set(appels.get() + 1);
            7
        };
        let deux = [
            c.lire_ou_inserer("x".into(), creer),
            c.lire_ou_inserer("x".into(), creer),
        ];
        assert_eq!(deux, [7, 7]);
        assert_eq!(appels.get(), 1);
        c.inserer("y".into(), 1);
        assert_eq!(c.lire_ou_inserer("x".into(), creer), 7);
        assert_eq!(appels.get(), 2);
    }

    #[test]
    fn reecrire_une_cle_la_rend_la_plus_recente_sans_doubler_l_entree() {
        let mut c: Lru<&str, i32> = Lru::nouveau(2);
        c.inserer("a", 1);
        c.inserer("b", 2);
        c.inserer("a", 3);
        assert_eq!(c.len(), 2);
        c.inserer("c", 4);
        assert!(c.contient(&"a"));
        assert!(!c.contient(&"b"));
        assert_eq!(c.lire(&"a"), Some(&3));
        assert_eq!(c.retirer(&"a"), Some(3));
        assert!(!c.contient(&"a") && c.contient(&"c"));
    }

    #[test]
    fn une_limite_nulle_ne_garde_rien() {
        let mut c: Lru<&str, i32> = Lru::nouveau(0);
        c.inserer("a", 1);
        assert!(c.is_empty());
        assert_eq!(c.lire_ou_inserer("a", || 5), 5);
        assert_eq!(c.len(), 0);
        assert_eq!(c.capacite(), 0);
    }

    #[test]
    fn retirer_le_plus_ancien_suit_l_ordre_des_lectures() {
        let mut c: Lru<&str, i32> = Lru::nouveau(3);
        c.inserer("a", 1);
        c.inserer("b", 2);
        c.inserer("c", 3);
        c.lire(&"a");
        assert_eq!(c.retirer_le_plus_ancien(), Some(("b", 2)));
        assert_eq!(c.retirer_le_plus_ancien(), Some(("c", 3)));
        assert_eq!(c.retirer_le_plus_ancien(), Some(("a", 1)));
        assert_eq!(c.retirer_le_plus_ancien(), None);
        assert!(c.is_empty());
    }

    #[test]
    fn vider_oublie_tout() {
        let mut c: Lru<&str, i32> = Lru::nouveau(3);
        c.inserer("a", 1);
        c.inserer("b", 2);
        c.vider();
        assert!(c.is_empty());
        c.inserer("c", 3);
        assert_eq!(c.lire(&"c"), Some(&3));
    }
}
