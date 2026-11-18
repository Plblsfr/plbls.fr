use std::collections::HashSet;

pub const PAS: f64 = 1365.0 / 11025.0;
const FENETRE: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reglages {
    pub min: f64,
    pub max: f64,
    pub bits: u32,
    pub trou: f64,
    pub consensus: f64,
    pub tolerance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub debut: f64,
    pub fin: f64,
}

#[derive(Clone, Copy)]
struct Suite {
    debut: usize,
    fin: usize,
    decalage: isize,
}

impl Suite {
    fn longueur(&self) -> usize {
        self.fin - self.debut + 1
    }
}

fn points(secondes: f64) -> usize {
    let n = (secondes / PAS).round();
    if n.is_finite() && n > 0.0 {
        n as usize
    } else {
        0
    }
}

fn secondes(point: usize) -> f64 {
    point as f64 * PAS
}

fn variee(points: &[u32]) -> bool {
    let distincts: HashSet<u32> = points.iter().copied().collect();
    distincts.len() * 8 >= points.len()
}

struct Bornes {
    min: usize,
    max: usize,
    trou: usize,
    bits: u32,
}

fn meilleure_suite(a: &[u32], b: &[u32], decalage: isize, l: &Bornes) -> Option<Suite> {
    let premier = usize::try_from(-decalage).unwrap_or(0);
    let dernier = a
        .len()
        .min(usize::try_from(b.len() as isize - decalage).unwrap_or(0));
    if dernier <= premier || dernier - premier < l.min {
        return None;
    }
    let semblable = |i: usize| (a[i] ^ b[(i as isize + decalage) as usize]).count_ones() <= l.bits;
    let dense =
        |de: usize, a_: usize| (de..a_).filter(|i| semblable(*i)).count() * 5 >= (a_ - de) * 3;
    let rogner = |mut debut: usize, mut fin: usize| {
        while fin - debut >= FENETRE && !(semblable(debut) && dense(debut, debut + FENETRE)) {
            debut += 1;
        }
        while fin - debut >= FENETRE && !(semblable(fin) && dense(fin + 1 - FENETRE, fin + 1)) {
            fin -= 1;
        }
        (debut, fin)
    };
    let mut meilleure: Option<Suite> = None;
    let mut debut: Option<usize> = None;
    let mut fin = 0usize;
    let mut clore = |debut: usize, fin: usize| {
        if fin - debut + 1 < l.min {
            return;
        }
        let (debut, fin) = rogner(debut, fin);
        let s = Suite {
            debut,
            fin,
            decalage,
        };
        let n = s.longueur();
        if n >= l.min
            && n <= l.max
            && meilleure.is_none_or(|m| n > m.longueur())
            && dense(debut, fin + 1)
            && variee(&a[debut..=fin])
        {
            meilleure = Some(s);
        }
    };
    for i in premier..dernier {
        if semblable(i) {
            if debut.is_none() {
                debut = Some(i);
            }
            fin = i;
        } else if let Some(d) = debut
            && i - fin > l.trou
        {
            clore(d, fin);
            debut = None;
        }
    }
    if let Some(d) = debut {
        clore(d, fin);
    }
    meilleure
}

pub fn comparer(a: &[u32], b: &[u32], r: &Reglages) -> Option<(Segment, Segment)> {
    let l = Bornes {
        min: points(r.min).max(1),
        max: points(r.max),
        trou: points(r.trou),
        bits: r.bits,
    };
    if a.len() < l.min || b.len() < l.min {
        return None;
    }
    let mut meilleure: Option<Suite> = None;
    let de = -((a.len() - l.min) as isize);
    let a_ = (b.len() - l.min) as isize;
    for decalage in de..=a_ {
        if let Some(s) = meilleure_suite(a, b, decalage, &l)
            && meilleure.is_none_or(|m| s.longueur() > m.longueur())
        {
            meilleure = Some(s);
        }
    }
    meilleure.map(|s| {
        let dans_b = |p: usize| (p as isize + s.decalage) as usize;
        (
            Segment {
                debut: secondes(s.debut),
                fin: secondes(s.fin + 1),
            },
            Segment {
                debut: secondes(dans_b(s.debut)),
                fin: secondes(dans_b(s.fin) + 1),
            },
        )
    })
}

fn mediane(mut valeurs: Vec<f64>) -> f64 {
    valeurs.sort_by(f64::total_cmp);
    let n = valeurs.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        valeurs[n / 2]
    } else {
        (valeurs[n / 2 - 1] + valeurs[n / 2]) / 2.0
    }
}

fn accord(candidats: &[Segment], paires: usize, r: &Reglages) -> Option<Segment> {
    if paires == 0 {
        return None;
    }
    let requis = ((r.consensus * paires as f64).ceil() as usize).max(paires.min(2));
    let proches = |c: &Segment| -> Vec<Segment> {
        candidats
            .iter()
            .filter(|x| {
                (x.debut - c.debut).abs() <= r.tolerance && (x.fin - c.fin).abs() <= r.tolerance
            })
            .copied()
            .collect()
    };
    let groupe = candidats
        .iter()
        .map(proches)
        .max_by_key(Vec::len)
        .filter(|g| g.len() >= requis)?;
    Some(Segment {
        debut: mediane(groupe.iter().map(|s| s.debut).collect()),
        fin: mediane(groupe.iter().map(|s| s.fin).collect()),
    })
}

pub fn analyser(empreintes: &[Vec<u32>], r: &Reglages) -> Vec<Option<Segment>> {
    let n = empreintes.len();
    let mut candidats: Vec<Vec<Segment>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in i + 1..n {
            if let Some((a, b)) = comparer(&empreintes[i], &empreintes[j], r) {
                candidats[i].push(a);
                candidats[j].push(b);
            }
        }
    }
    candidats
        .iter()
        .map(|c| accord(c, n.saturating_sub(1), r))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Reglages = Reglages {
        min: 15.0,
        max: 120.0,
        bits: 8,
        trou: 3.0,
        consensus: 0.6,
        tolerance: 5.0,
    };

    struct Hasard(u64);

    impl Hasard {
        fn suivant(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 32) as u32
        }

        fn suite(&mut self, secondes: f64) -> Vec<u32> {
            (0..points(secondes)).map(|_| self.suivant()).collect()
        }
    }

    fn episode(h: &mut Hasard, avant: f64, intro: &[u32], apres: f64) -> Vec<u32> {
        let mut e = h.suite(avant);
        e.extend_from_slice(intro);
        e.extend(h.suite(apres));
        e
    }

    fn proche(s: Option<Segment>, debut: f64, fin: f64) {
        let s = s.unwrap_or_else(|| panic!("aucun segment, attendu {debut} à {fin}"));
        assert!(
            (s.debut - debut).abs() < 1.0 && (s.fin - fin).abs() < 1.0,
            "{s:?}, attendu {debut} à {fin}"
        );
    }

    #[test]
    fn intro_retrouvee_malgre_le_decalage() {
        let mut h = Hasard(1);
        let intro = h.suite(40.0);
        let episodes = vec![
            episode(&mut h, 5.0, &intro, 80.0),
            episode(&mut h, 62.0, &intro, 30.0),
            episode(&mut h, 0.0, &intro, 90.0),
            episode(&mut h, 110.0, &intro, 20.0),
        ];
        let r = analyser(&episodes, &R);
        proche(r[0], 5.0, 45.0);
        proche(r[1], 62.0, 102.0);
        proche(r[2], 0.0, 40.0);
        proche(r[3], 110.0, 150.0);
    }

    #[test]
    fn intro_retrouvee_malgre_le_bruit() {
        let mut h = Hasard(2);
        let intro = h.suite(30.0);
        let bruitee = |h: &mut Hasard| -> Vec<u32> {
            intro
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i % 23 == 11 {
                        h.suivant()
                    } else {
                        let x = h.suivant();
                        p ^ (1 << (x % 32)) ^ (1 << ((x >> 8) % 32)) ^ (1 << ((x >> 16) % 32))
                    }
                })
                .collect()
        };
        let (b1, b2, b3) = (bruitee(&mut h), bruitee(&mut h), bruitee(&mut h));
        let episodes = vec![
            episode(&mut h, 20.0, &b1, 60.0),
            episode(&mut h, 45.0, &b2, 40.0),
            episode(&mut h, 8.0, &b3, 70.0),
        ];
        let r = analyser(&episodes, &R);
        proche(r[0], 20.0, 50.0);
        proche(r[1], 45.0, 75.0);
        proche(r[2], 8.0, 38.0);
    }

    #[test]
    fn sans_consensus_rien_n_est_retenu() {
        let mut h = Hasard(3);
        let commun = h.suite(30.0);
        let episodes = vec![
            episode(&mut h, 10.0, &commun, 60.0),
            episode(&mut h, 30.0, &commun, 60.0),
            h.suite(100.0),
            h.suite(100.0),
            h.suite(100.0),
        ];
        assert!(comparer(&episodes[0], &episodes[1], &R).is_some());
        assert_eq!(analyser(&episodes, &R), vec![None; 5]);
        let sans_rapport = vec![h.suite(100.0), h.suite(100.0), h.suite(100.0)];
        assert_eq!(analyser(&sans_rapport, &R), vec![None; 3]);
    }

    #[test]
    fn segment_isole_rejete_quand_les_autres_s_accordent() {
        let mut h = Hasard(4);
        let intro = h.suite(30.0);
        let autre = h.suite(35.0);
        let mut e0 = episode(&mut h, 10.0, &intro, 20.0);
        e0.extend_from_slice(&autre);
        let mut e1 = episode(&mut h, 40.0, &intro, 20.0);
        e1.extend_from_slice(&autre);
        let (isole, _) = comparer(&e0, &e1, &R).expect("segment commun aux deux premiers");
        assert!(isole.debut > 55.0, "{isole:?}");
        let e2 = episode(&mut h, 3.0, &intro, 60.0);
        let e3 = episode(&mut h, 25.0, &intro, 60.0);
        let r = analyser(&[e0, e1, e2, e3], &R);
        proche(r[0], 10.0, 40.0);
        proche(r[1], 40.0, 70.0);
        proche(r[2], 3.0, 33.0);
        proche(r[3], 25.0, 55.0);
    }

    #[test]
    fn deux_episodes_suffisent() {
        let mut h = Hasard(5);
        let intro = h.suite(20.0);
        let episodes = vec![
            episode(&mut h, 12.0, &intro, 50.0),
            episode(&mut h, 33.0, &intro, 50.0),
        ];
        let r = analyser(&episodes, &R);
        proche(r[0], 12.0, 32.0);
        proche(r[1], 33.0, 53.0);
        assert_eq!(analyser(&episodes[..1], &R), vec![None]);
        assert!(analyser(&[], &R).is_empty());
    }

    #[test]
    fn segments_trop_courts_trop_longs_ou_uniformes_rejetes() {
        let mut h = Hasard(6);
        let court = h.suite(10.0);
        let a = episode(&mut h, 5.0, &court, 60.0);
        let b = episode(&mut h, 20.0, &court, 60.0);
        assert_eq!(comparer(&a, &b, &R), None);
        let long = h.suite(150.0);
        let a = episode(&mut h, 5.0, &long, 20.0);
        let b = episode(&mut h, 20.0, &long, 20.0);
        assert_eq!(comparer(&a, &b, &R), None);
        let silence = vec![0u32; points(40.0)];
        let a = episode(&mut h, 5.0, &silence, 30.0);
        let b = episode(&mut h, 20.0, &silence, 30.0);
        assert_eq!(comparer(&a, &b, &R), None);
        assert_eq!(comparer(&[], &[], &R), None);
    }

    #[test]
    fn trou_plus_long_que_la_tolerance_coupe_la_suite() {
        let mut h = Hasard(7);
        let debut = h.suite(20.0);
        let fin = h.suite(20.0);
        let mut a = h.suite(10.0);
        a.extend_from_slice(&debut);
        a.extend(h.suite(8.0));
        a.extend_from_slice(&fin);
        let mut b = h.suite(30.0);
        b.extend_from_slice(&debut);
        b.extend(h.suite(8.0));
        b.extend_from_slice(&fin);
        let (sa, _) = comparer(&a, &b, &R).expect("un des deux morceaux");
        assert!((sa.fin - sa.debut - 20.0).abs() < 1.0, "{sa:?}");
    }
}
