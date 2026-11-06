use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::MusicMeta;

pub const TAILLE_MAX: usize = 500;
pub const DUREE_VIE: Duration = Duration::from_secs(24 * 3600);

#[derive(Default)]
pub struct Cache {
    meta: Mutex<HashMap<String, (MusicMeta, Instant)>>,
    pochettes: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

impl Cache {
    pub fn meta(&self, id: &str, maintenant: Instant) -> Option<MusicMeta> {
        let meta = self.meta.lock().unwrap();
        meta.get(id)
            .filter(|(_, depuis)| maintenant.saturating_duration_since(*depuis) < DUREE_VIE)
            .map(|(m, _)| m.clone())
    }

    pub fn ranger_meta(&self, nouveaux: &HashMap<String, MusicMeta>, maintenant: Instant) {
        let mut meta = self.meta.lock().unwrap();
        if meta.len() + nouveaux.len() > TAILLE_MAX {
            meta.clear();
        }
        for (id, m) in nouveaux {
            meta.insert(id.clone(), (m.clone(), maintenant));
        }
    }

    pub fn pochette(&self, id: &str) -> Option<Arc<Vec<u8>>> {
        self.pochettes.lock().unwrap().get(id).cloned()
    }

    pub fn ranger_pochette(&self, id: &str, octets: Vec<u8>) -> Arc<Vec<u8>> {
        let mut pochettes = self.pochettes.lock().unwrap();
        if pochettes.len() >= TAILLE_MAX {
            pochettes.clear();
        }
        let octets = Arc::new(octets);
        pochettes.insert(id.to_string(), octets.clone());
        octets
    }
}

#[cfg(test)]
mod tests {
    use super::{Cache, DUREE_VIE, TAILLE_MAX};
    use crate::MusicMeta;
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    fn meta(titre: &str) -> MusicMeta {
        MusicMeta {
            title: titre.into(),
            ..MusicMeta::default()
        }
    }

    #[test]
    fn les_metadonnees_perimees_au_bout_de_24_heures() {
        let c = Cache::default();
        let t0 = Instant::now();
        c.ranger_meta(&HashMap::from([("a".into(), meta("A"))]), t0);
        assert_eq!(c.meta("a", t0).unwrap().title, "A");
        assert!(
            c.meta("a", t0 + DUREE_VIE - Duration::from_secs(1))
                .is_some()
        );
        assert!(c.meta("a", t0 + DUREE_VIE).is_none());
        assert!(c.meta("b", t0).is_none());
    }

    #[test]
    fn les_metadonnees_sont_videes_au_dela_du_plafond() {
        let c = Cache::default();
        let t0 = Instant::now();
        let lot = |de: usize, a: usize| {
            (de..a)
                .map(|i| (format!("id{i}"), meta("x")))
                .collect::<HashMap<_, _>>()
        };
        c.ranger_meta(&lot(0, TAILLE_MAX), t0);
        assert!(c.meta("id0", t0).is_some());
        c.ranger_meta(&lot(TAILLE_MAX, TAILLE_MAX + 1), t0);
        assert!(c.meta("id0", t0).is_none());
        assert!(c.meta(&format!("id{TAILLE_MAX}"), t0).is_some());
    }

    #[test]
    fn les_pochettes_sont_videes_quand_le_plafond_est_atteint() {
        let c = Cache::default();
        for i in 0..TAILLE_MAX {
            c.ranger_pochette(&format!("p{i}"), vec![1]);
        }
        assert_eq!(*c.pochette("p0").unwrap(), vec![1]);
        c.ranger_pochette("nouvelle", vec![2]);
        assert!(c.pochette("p0").is_none());
        assert_eq!(*c.pochette("nouvelle").unwrap(), vec![2]);
    }
}
