use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

pub struct Lru<K, V> {
    elements: HashMap<K, V>,
    ordre: VecDeque<K>,
    max: usize,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub fn new(max: usize) -> Self {
        Self {
            elements: HashMap::new(),
            ordre: VecDeque::new(),
            max,
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn contient(&self, cle: &K) -> bool {
        self.elements.contains_key(cle)
    }

    pub fn get(&mut self, cle: &K) -> Option<&V> {
        if !self.elements.contains_key(cle) {
            return None;
        }
        self.retirer_de_l_ordre(cle);
        self.ordre.push_back(cle.clone());
        self.elements.get(cle)
    }

    pub fn set(&mut self, cle: K, valeur: V) {
        self.retirer_de_l_ordre(&cle);
        self.ordre.push_back(cle.clone());
        self.elements.insert(cle, valeur);
        while self.elements.len() > self.max {
            match self.ordre.pop_front() {
                Some(ancienne) => {
                    self.elements.remove(&ancienne);
                }
                None => break,
            }
        }
    }

    fn retirer_de_l_ordre(&mut self, cle: &K) {
        if let Some(position) = self.ordre.iter().position(|k| k == cle) {
            self.ordre.remove(position);
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Lru<K, V> {
    pub fn get_or_set(&mut self, cle: K, creer: impl FnOnce() -> V) -> V {
        if let Some(valeur) = self.get(&cle) {
            return valeur.clone();
        }
        let valeur = creer();
        self.set(cle, valeur.clone());
        valeur
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn oublie_la_moins_recemment_utilisee_au_dela_de_la_limite() {
        let mut c: Lru<String, i32> = Lru::new(2);
        c.set("a".into(), 1);
        c.set("b".into(), 2);
        assert_eq!(c.get(&"a".to_string()), Some(&1));
        c.set("c".into(), 3);
        let taille = c.len();
        let a = c.get(&"a".to_string()).copied();
        let b = c.get(&"b".to_string()).copied();
        let c3 = c.get(&"c".to_string()).copied();
        assert_eq!((taille, a, b, c3), (2, Some(1), None, Some(3)));
    }

    #[test]
    fn ne_calcule_une_valeur_absente_qu_une_fois() {
        let mut c: Lru<String, i32> = Lru::new(1);
        let appels = Cell::new(0);
        let creer = || {
            appels.set(appels.get() + 1);
            7
        };
        let deux = [
            c.get_or_set("x".into(), creer),
            c.get_or_set("x".into(), creer),
        ];
        assert_eq!(deux, [7, 7]);
        assert_eq!(appels.get(), 1);
        c.set("y".into(), 1);
        assert_eq!(c.get_or_set("x".into(), creer), 7);
        assert_eq!(appels.get(), 2);
    }

    #[test]
    fn reecrire_une_cle_la_rend_la_plus_recente() {
        let mut c: Lru<&str, i32> = Lru::new(2);
        c.set("a", 1);
        c.set("b", 2);
        c.set("a", 3);
        c.set("c", 4);
        assert!(c.contient(&"a"));
        assert!(!c.contient(&"b"));
        assert_eq!(c.get(&"a"), Some(&3));
    }

    #[test]
    fn une_limite_nulle_ne_garde_rien() {
        let mut c: Lru<&str, i32> = Lru::new(0);
        c.set("a", 1);
        assert!(c.is_empty());
        assert_eq!(c.get_or_set("a", || 5), 5);
        assert_eq!(c.len(), 0);
    }
}
