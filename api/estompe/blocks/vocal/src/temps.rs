use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use estompe_core::sablier::Donnees;

pub const TRANCHE: Duration = Duration::from_secs(60);
pub const PRESENCE_MAX: Duration = Duration::from_secs(12 * 3600);

pub type Sorties = Vec<(String, Donnees)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Present {
    pub salon: String,
    pub serveur: String,
    pub cinema: bool,
    pub id: String,
    pub sub: String,
}

struct Personne {
    sub: String,
    arrivee: Instant,
    plusieurs: Option<Instant>,
    appel: u64,
    seance: bool,
    partage: Option<Instant>,
}

struct Salle {
    serveur: String,
    cinema: bool,
    personnes: BTreeMap<String, Personne>,
}

#[derive(Default)]
pub struct Compteur {
    salles: BTreeMap<String, Salle>,
}

fn ecoule(depuis: &mut Instant, t: Instant, seuil: Duration) -> u32 {
    let duree = t.saturating_duration_since(*depuis);
    if duree < seuil {
        return 0;
    }
    let secondes = duree.as_secs();
    *depuis += Duration::from_secs(secondes);
    u32::try_from(secondes).unwrap_or(u32::MAX)
}

impl Salle {
    fn solder(&mut self, salon: &str, t: Instant, seuil: Duration, sorties: &mut Sorties) {
        for p in self.personnes.values_mut() {
            let t = t.min(p.arrivee + PRESENCE_MAX);
            if let Some(depuis) = &mut p.plusieurs {
                let secondes = ecoule(depuis, t, seuil);
                if secondes > 0 {
                    p.appel += u64::from(secondes);
                    let donnees = if self.cinema {
                        Donnees::EstompeCinemaTemps { secondes }
                    } else {
                        Donnees::EstompeVocalTemps {
                            serveur: Some(self.serveur.clone()),
                            salon: salon.to_string(),
                            secondes,
                        }
                    };
                    sorties.push((p.sub.clone(), donnees));
                }
            }
            if let Some(depuis) = &mut p.partage {
                let secondes = ecoule(depuis, t, seuil);
                if secondes > 0 {
                    sorties.push((p.sub.clone(), Donnees::EstompePartageEcran { secondes }));
                }
            }
        }
    }

    fn regrouper(&mut self, t: Instant, sorties: &mut Sorties) {
        let plusieurs = self.personnes.len() >= 2;
        for p in self.personnes.values_mut() {
            if !plusieurs {
                p.plusieurs = None;
                continue;
            }
            p.plusieurs.get_or_insert(t);
            if self.cinema && !p.seance {
                p.seance = true;
                sorties.push((p.sub.clone(), Donnees::EstompeCinemaSeance {}));
            }
        }
    }
}

impl Compteur {
    fn salon_de(&self, id: &str) -> Option<String> {
        self.salles
            .iter()
            .find(|(_, s)| s.personnes.contains_key(id))
            .map(|(salon, _)| salon.clone())
    }

    pub fn arriver(&mut self, p: &Present, t: Instant) -> Sorties {
        let ancien = self.salon_de(&p.id);
        if ancien.as_deref() == Some(p.salon.as_str()) {
            return Vec::new();
        }
        let mut sorties = match ancien {
            Some(salon) => self.partir(&salon, &p.id, t),
            None => Vec::new(),
        };
        let salle = self.salles.entry(p.salon.clone()).or_insert_with(|| Salle {
            serveur: p.serveur.clone(),
            cinema: p.cinema,
            personnes: BTreeMap::new(),
        });
        salle.solder(&p.salon, t, Duration::ZERO, &mut sorties);
        salle.personnes.insert(
            p.id.clone(),
            Personne {
                sub: p.sub.clone(),
                arrivee: t,
                plusieurs: None,
                appel: 0,
                seance: false,
                partage: None,
            },
        );
        salle.regrouper(t, &mut sorties);
        sorties
    }

    pub fn partir(&mut self, salon: &str, id: &str, t: Instant) -> Sorties {
        let mut sorties = Vec::new();
        let Some(salle) = self
            .salles
            .get_mut(salon)
            .filter(|s| s.personnes.contains_key(id))
        else {
            return sorties;
        };
        salle.solder(salon, t, Duration::ZERO, &mut sorties);
        if let Some(p) = salle.personnes.remove(id)
            && !salle.cinema
            && p.appel > 0
        {
            sorties.push((
                p.sub,
                Donnees::EstompeVocalAppel {
                    serveur: Some(salle.serveur.clone()),
                    salon: salon.to_string(),
                    secondes: u32::try_from(p.appel).unwrap_or(u32::MAX),
                },
            ));
        }
        salle.regrouper(t, &mut sorties);
        if salle.personnes.is_empty() {
            self.salles.remove(salon);
        }
        sorties
    }

    pub fn partager(&mut self, id: &str, actif: bool, t: Instant) -> Sorties {
        let Some(p) = self
            .salles
            .values_mut()
            .find_map(|s| s.personnes.get_mut(id))
        else {
            return Vec::new();
        };
        if actif {
            p.partage.get_or_insert(t);
            return Vec::new();
        }
        let Some(mut depuis) = p.partage.take() else {
            return Vec::new();
        };
        match ecoule(&mut depuis, t.min(p.arrivee + PRESENCE_MAX), Duration::ZERO) {
            0 => Vec::new(),
            secondes => vec![(p.sub.clone(), Donnees::EstompePartageEcran { secondes })],
        }
    }

    pub fn battre(&mut self, t: Instant) -> Sorties {
        let mut sorties = Vec::new();
        for (salon, salle) in &mut self.salles {
            salle.solder(salon, t, TRANCHE, &mut sorties);
        }
        sorties
    }

    pub fn observer(&mut self, presents: &[Present], t: Instant) -> Sorties {
        let absents: Vec<(String, String)> = self
            .salles
            .iter()
            .flat_map(|(salon, s)| s.personnes.keys().map(move |id| (salon, id)))
            .filter(|(salon, id)| !presents.iter().any(|p| p.salon == **salon && p.id == **id))
            .map(|(salon, id)| (salon.clone(), id.clone()))
            .collect();
        let mut sorties = Vec::new();
        for (salon, id) in absents {
            sorties.extend(self.partir(&salon, &id, t));
        }
        for p in presents {
            sorties.extend(self.arriver(p, t));
        }
        sorties
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn present(salon: &str, nom: &str) -> Present {
        Present {
            salon: salon.to_string(),
            serveur: "srv".to_string(),
            cinema: salon == "cine",
            id: nom.to_string(),
            sub: format!("sub-{nom}"),
        }
    }

    fn vocal(nom: &str, secondes: u32) -> (String, Donnees) {
        (
            format!("sub-{nom}"),
            Donnees::EstompeVocalTemps {
                serveur: Some("srv".into()),
                salon: "voc".into(),
                secondes,
            },
        )
    }

    fn appel(nom: &str, secondes: u32) -> (String, Donnees) {
        (
            format!("sub-{nom}"),
            Donnees::EstompeVocalAppel {
                serveur: Some("srv".into()),
                salon: "voc".into(),
                secondes,
            },
        )
    }

    fn cinema(nom: &str, secondes: u32) -> (String, Donnees) {
        (
            format!("sub-{nom}"),
            Donnees::EstompeCinemaTemps { secondes },
        )
    }

    fn seance(nom: &str) -> (String, Donnees) {
        (format!("sub-{nom}"), Donnees::EstompeCinemaSeance {})
    }

    fn partage(nom: &str, secondes: u32) -> (String, Donnees) {
        (
            format!("sub-{nom}"),
            Donnees::EstompePartageEcran { secondes },
        )
    }

    #[test]
    fn seul_rien_ne_compte_a_plusieurs_le_temps_court_pour_tous() {
        let t0 = Instant::now();
        let t = |s: u64| t0 + Duration::from_secs(s);
        let mut c = Compteur::default();
        assert!(c.arriver(&present("voc", "alice"), t(0)).is_empty());
        assert!(c.battre(t(3600)).is_empty());
        assert!(c.arriver(&present("voc", "bob"), t(3600)).is_empty());
        assert!(c.arriver(&present("voc", "bob"), t(3601)).is_empty());
        assert!(c.battre(t(3659)).is_empty());
        assert_eq!(c.battre(t(3660)), [vocal("alice", 60), vocal("bob", 60)]);
        assert!(c.battre(t(3661)).is_empty());
        assert_eq!(
            c.arriver(&present("voc", "carol"), t(3690)),
            [vocal("alice", 30), vocal("bob", 30)]
        );
        assert_eq!(
            c.partir("voc", "carol", t(3700)),
            [
                vocal("alice", 10),
                vocal("bob", 10),
                vocal("carol", 10),
                appel("carol", 10)
            ]
        );
        assert_eq!(
            c.partir("voc", "bob", t(3720)),
            [vocal("alice", 20), vocal("bob", 20), appel("bob", 120)]
        );
        assert!(c.battre(t(9000)).is_empty());
        assert!(c.partir("voc", "bob", t(9000)).is_empty());
        assert!(c.arriver(&present("voc", "bob"), t(9000)).is_empty());
        assert_eq!(
            c.partir("voc", "alice", t(9005)),
            [vocal("alice", 5), vocal("bob", 5), appel("alice", 125)]
        );
        assert_eq!(c.partir("voc", "bob", t(9900)), [appel("bob", 5)]);
        assert!(c.salles.is_empty());
    }

    #[test]
    fn fractions_de_seconde_gardees_et_presence_plafonnee() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut c = Compteur::default();
        c.arriver(&present("voc", "alice"), ms(0));
        c.arriver(&present("voc", "bob"), ms(0));
        assert_eq!(
            c.arriver(&present("voc", "carol"), ms(1500)),
            [vocal("alice", 1), vocal("bob", 1)]
        );
        assert_eq!(
            c.partir("voc", "carol", ms(2000)),
            [vocal("alice", 1), vocal("bob", 1)]
        );
        let fin = t0 + PRESENCE_MAX + Duration::from_secs(3600);
        let total = u32::try_from(PRESENCE_MAX.as_secs()).unwrap();
        assert_eq!(
            c.battre(fin),
            [vocal("alice", total - 2), vocal("bob", total - 2)]
        );
        assert!(c.battre(fin + TRANCHE).is_empty());
        assert_eq!(
            c.partir("voc", "bob", fin + TRANCHE),
            [appel("bob", total)]
        );
    }

    #[test]
    fn changement_de_salon_et_observation() {
        let t0 = Instant::now();
        let t = |s: u64| t0 + Duration::from_secs(s);
        let mut c = Compteur::default();
        let tous = [present("voc", "alice"), present("voc", "bob")];
        assert!(c.observer(&tous, t(0)).is_empty());
        assert!(c.observer(&tous, t(10)).is_empty());
        let ailleurs = Present {
            salon: "autre".into(),
            ..present("voc", "bob")
        };
        assert_eq!(
            c.observer(&[present("voc", "alice"), ailleurs], t(40)),
            [vocal("alice", 40), vocal("bob", 40), appel("bob", 40)]
        );
        assert_eq!(c.observer(&[], t(100)), [appel("alice", 40)]);
        assert!(c.salles.is_empty());
    }

    #[test]
    fn partage_d_ecran_meme_seul() {
        let t0 = Instant::now();
        let t = |s: u64| t0 + Duration::from_secs(s);
        let mut c = Compteur::default();
        assert!(c.partager("alice", true, t(0)).is_empty());
        c.arriver(&present("voc", "alice"), t(0));
        assert!(c.partager("alice", false, t(5)).is_empty());
        assert!(c.partager("alice", true, t(10)).is_empty());
        assert!(c.partager("alice", true, t(20)).is_empty());
        assert_eq!(c.battre(t(70)), [partage("alice", 60)]);
        assert_eq!(c.partager("alice", false, t(75)), [partage("alice", 5)]);
        assert!(c.partager("alice", false, t(80)).is_empty());
        c.partager("alice", true, t(100));
        assert_eq!(c.partir("voc", "alice", t(130)), [partage("alice", 30)]);
    }

    #[test]
    fn salon_cinema_une_seance_et_un_temps_par_personne_et_par_tranche() {
        let t0 = Instant::now();
        let t = |s: u64| t0 + Duration::from_secs(s);
        let mut c = Compteur::default();
        assert!(c.arriver(&present("cine", "alice"), t(0)).is_empty());
        assert!(c.battre(t(600)).is_empty());
        let mut emis = c.arriver(&present("cine", "bob"), t(600));
        assert_eq!(emis, [seance("alice"), seance("bob")]);
        emis.extend(c.battre(t(660)));
        assert_eq!(emis[2..], [cinema("alice", 60), cinema("bob", 60)]);
        assert!(c.battre(t(660)).is_empty());
        assert!(c.battre(t(700)).is_empty());
        emis.extend(c.battre(t(720)));
        emis.extend(c.arriver(&present("cine", "carol"), t(730)));
        assert_eq!(
            emis[6..],
            [cinema("alice", 10), cinema("bob", 10), seance("carol")]
        );
        emis.extend(c.partir("cine", "carol", t(740)));
        emis.extend(c.partir("cine", "bob", t(750)));
        assert!(c.battre(t(5000)).is_empty());
        emis.extend(c.arriver(&present("cine", "bob"), t(5000)));
        assert_eq!(emis.last(), Some(&seance("bob")));
        emis.extend(c.partir("cine", "alice", t(5030)));
        let de = |nom: &str, d: &Donnees| {
            emis.iter()
                .filter(|(sub, x)| sub == &format!("sub-{nom}") && x == d)
                .count()
        };
        assert_eq!(de("alice", &Donnees::EstompeCinemaSeance {}), 1);
        assert_eq!(de("bob", &Donnees::EstompeCinemaSeance {}), 2);
        assert_eq!(de("carol", &Donnees::EstompeCinemaSeance {}), 1);
        let secondes = |nom: &str| -> u32 {
            emis.iter()
                .filter(|(sub, _)| sub == &format!("sub-{nom}"))
                .map(|(_, d)| match d {
                    Donnees::EstompeCinemaTemps { secondes } => *secondes,
                    Donnees::EstompeCinemaSeance {} => 0,
                    autre => panic!("évènement inattendu : {autre:?}"),
                })
                .sum()
        };
        assert_eq!(
            (secondes("alice"), secondes("bob"), secondes("carol")),
            (180, 180, 10)
        );
        for (sub, d) in &emis {
            let json = estompe_core::sablier::Evenement::nouveau(sub.clone(), d.clone()).json();
            for autre in ["sub-alice", "sub-bob", "sub-carol", "srv", "cine"] {
                assert!(autre == sub || !json.contains(autre), "{json}");
            }
        }
    }
}
