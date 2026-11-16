use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    etat::AppState,
    recommandation::{
        Demande, Donnees, charger,
        score::{NEUTRE, proximite, sauts},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suggestions {
    Off,
    Light,
    Frequent,
}

impl Suggestions {
    pub fn lire(texte: &str) -> Option<Self> {
        match texte {
            "off" => Some(Self::Off),
            "light" => Some(Self::Light),
            "frequent" => Some(Self::Frequent),
            _ => None,
        }
    }

    pub fn nom(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Light => "light",
            Self::Frequent => "frequent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element {
    pub id: Uuid,
    pub piste: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub element: Option<Uuid>,
    pub piste: Uuid,
    pub suggeree: bool,
}

pub struct Contexte<'a> {
    pub sub: &'a str,
    pub graine: i64,
    pub premier: Option<Uuid>,
    pub elements: &'a [Element],
    pub suggestions: Suggestions,
}

pub fn graine() -> i64 {
    i64::try_from(Uuid::new_v4().as_u64_pair().0 >> 11).unwrap_or(0)
}

struct SplitMix(u64);

impl SplitMix {
    fn suivant(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unite(&mut self) -> f32 {
        (self.suivant() >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub const FENETRE: usize = 8;
pub const HORIZON: usize = 50;

impl Suggestions {
    pub fn rythme(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Light => Some(10),
            Self::Frequent => Some(4),
        }
    }
}

struct Tirage<'a> {
    d: &'a Donnees,
    base: Vec<Element>,
    artistes: Vec<Option<Uuid>>,
    pris: Vec<bool>,
    debut: usize,
    reste: usize,
    comptes: HashMap<Uuid, usize>,
    tailles: BTreeSet<(usize, Uuid)>,
    files: HashMap<Uuid, VecDeque<usize>>,
}

impl Tirage<'_> {
    fn prendre(&mut self, i: usize) -> Element {
        self.pris[i] = true;
        self.reste -= 1;
        if let Some(a) = self.artistes[i]
            && let Some(n) = self.comptes.get_mut(&a)
        {
            self.tailles.remove(&(*n, a));
            *n -= 1;
            if *n > 0 {
                self.tailles.insert((*n, a));
            }
        }
        while self.debut < self.pris.len() && self.pris[self.debut] {
            self.debut += 1;
        }
        self.base[i]
    }

    fn premier_de(&mut self, artiste: Uuid) -> Option<usize> {
        let f = self.files.get_mut(&artiste)?;
        while f.front().is_some_and(|i| self.pris[*i]) {
            f.pop_front();
        }
        f.front().copied()
    }

    fn cout(&self, rang: usize, i: usize, precedent: Option<Element>) -> f32 {
        let piste = self.base[i].piste;
        let meta = |p: Uuid| self.d.metas.get(&p);
        let transition = precedent
            .and_then(|p| self.d.voisins.distance(p.piste, piste))
            .map_or(NEUTRE, |d| 1.0 - proximite(d));
        let meme_album = precedent.is_some_and(|p| {
            meta(p.piste)
                .zip(meta(piste))
                .is_some_and(|(a, b)| a.album == b.album)
        });
        rang as f32 / FENETRE as f32
            + transition
            + f32::from(u8::from(meme_album))
            + 2.0 * sauts(self.d.profil.pistes.get(&piste), self.d.maintenant)
    }

    fn suivant(&mut self, precedent: Option<Element>) -> Option<Element> {
        if self.reste == 0 {
            return None;
        }
        let dernier = precedent
            .and_then(|p| self.d.metas.get(&p.piste))
            .map(|m| m.artiste);
        let force = self
            .tailles
            .last()
            .copied()
            .filter(|(n, a)| 2 * n > self.reste && Some(*a) != dernier);
        if let Some(i) = force.and_then(|(_, a)| self.premier_de(a)) {
            return Some(self.prendre(i));
        }
        let autre = |t: &Self, i: usize| t.artistes[i].is_none() || t.artistes[i] != dernier;
        let fenetre: Vec<usize> = (self.debut..self.pris.len())
            .filter(|i| !self.pris[*i])
            .take(FENETRE)
            .collect();
        let choix = fenetre
            .iter()
            .enumerate()
            .filter(|(_, i)| autre(self, **i))
            .map(|(rang, i)| (self.cout(rang, *i, precedent), *i))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|c| c.1);
        let ailleurs =
            dernier.is_some_and(|a| self.comptes.get(&a).copied().unwrap_or(0) < self.reste);
        let i = match choix {
            Some(i) => i,
            None if ailleurs => (self.debut..self.pris.len())
                .find(|i| !self.pris[*i] && autre(self, *i))
                .unwrap_or(fenetre[0]),
            None => fenetre[0],
        };
        Some(self.prendre(i))
    }
}

pub fn ordonner(c: &Contexte, d: &Donnees) -> Vec<Place> {
    let mut tete = None;
    let mut reste: Vec<Element> = Vec::with_capacity(c.elements.len());
    for e in c.elements {
        if tete.is_none() && Some(e.id) == c.premier {
            tete = Some(*e);
        } else {
            reste.push(*e);
        }
    }
    let mut g = SplitMix(c.graine as u64);
    let mut cles: Vec<(f32, usize)> = reste
        .iter()
        .enumerate()
        .map(|(i, e)| {
            (
                g.unite() + sauts(d.profil.pistes.get(&e.piste), d.maintenant),
                i,
            )
        })
        .collect();
    cles.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let base: Vec<Element> = cles.iter().map(|(_, i)| reste[*i]).collect();
    let artistes: Vec<Option<Uuid>> = base
        .iter()
        .map(|e| d.metas.get(&e.piste).map(|m| m.artiste))
        .collect();
    let mut comptes: HashMap<Uuid, usize> = HashMap::new();
    let mut files: HashMap<Uuid, VecDeque<usize>> = HashMap::new();
    for (i, a) in artistes.iter().enumerate() {
        if let Some(a) = a {
            *comptes.entry(*a).or_default() += 1;
            files.entry(*a).or_default().push_back(i);
        }
    }
    let mut t = Tirage {
        d,
        reste: base.len(),
        pris: vec![false; base.len()],
        debut: 0,
        tailles: comptes.iter().map(|(a, n)| (*n, *a)).collect(),
        comptes,
        files,
        artistes,
        base,
    };
    let mut sortie: Vec<Element> = Vec::with_capacity(c.elements.len());
    sortie.extend(tete);
    while let Some(e) = t.suivant(sortie.last().copied()) {
        sortie.push(e);
    }
    sortie
        .into_iter()
        .map(|e| Place {
            element: Some(e.id),
            piste: e.piste,
            suggeree: false,
        })
        .collect()
}

pub fn inserer(places: &mut Vec<Place>, index: usize, rythme: usize, d: &Donnees) -> usize {
    let mut prises: HashSet<Uuid> = places.iter().map(|p| p.piste).collect();
    let mut artistes: HashSet<Uuid> = places
        .iter()
        .skip(index)
        .filter(|p| p.suggeree)
        .filter_map(|p| d.metas.get(&p.piste))
        .map(|m| m.artiste)
        .collect();
    let (mut suite, mut i, mut ajoutees) = (0, 0, 0);
    while i < places.len() && i <= index + HORIZON {
        suite = if places[i].suggeree { 0 } else { suite + 1 };
        if suite == rythme {
            suite = 0;
            let libre = places.get(i + 1).is_none_or(|p| !p.suggeree);
            if i >= index
                && libre
                && let Some(p) = d.choisir(Some(places[i].piste), &prises, &artistes)
            {
                prises.insert(p);
                artistes.extend(d.metas.get(&p).map(|m| m.artiste));
                places.insert(
                    i + 1,
                    Place {
                        element: None,
                        piste: p,
                        suggeree: true,
                    },
                );
                ajoutees += 1;
                i += 1;
            }
        }
        i += 1;
    }
    ajoutees
}

pub fn a_prolonger(places: &[Place], index: usize, rythme: usize) -> bool {
    let mut suite = 0;
    for (i, p) in places.iter().enumerate().take(index + HORIZON / 2 + 1) {
        suite = if p.suggeree { 0 } else { suite + 1 };
        if suite == rythme {
            suite = 0;
            if i >= index && places.get(i + 1).is_none_or(|p| !p.suggeree) {
                return true;
            }
        }
    }
    false
}

async fn donnees(state: &AppState, c: &Contexte<'_>, places: &[Place]) -> Donnees {
    let source: Vec<Uuid> = places
        .iter()
        .filter(|p| !p.suggeree)
        .map(|p| p.piste)
        .collect();
    let exclues: Vec<Uuid> = places.iter().map(|p| p.piste).collect();
    let demande = Demande {
        sub: c.sub,
        source: &source,
        exclues: &exclues,
        graine: c.graine,
        candidats: c.suggestions != Suggestions::Off,
        maintenant: Utc::now(),
    };
    match charger(state, &demande).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("recommandation : {e}");
            Donnees::default()
        }
    }
}

pub async fn melanger(state: &AppState, c: &Contexte<'_>) -> Vec<Place> {
    let brutes: Vec<Place> = c
        .elements
        .iter()
        .map(|e| Place {
            element: Some(e.id),
            piste: e.piste,
            suggeree: false,
        })
        .collect();
    let d = donnees(state, c, &brutes).await;
    let mut places = ordonner(c, &d);
    if let Some(rythme) = c.suggestions.rythme() {
        inserer(&mut places, 0, rythme, &d);
    }
    places
}

pub async fn prolonger(
    state: &AppState,
    c: &Contexte<'_>,
    places: &mut Vec<Place>,
    index: usize,
) -> usize {
    let Some(rythme) = c.suggestions.rythme() else {
        return 0;
    };
    let d = donnees(state, c, places).await;
    inserer(places, index, rythme, &d)
}

pub fn deplacer<T>(liste: &mut Vec<T>, depuis: usize, nombre: usize, vers: usize) -> bool {
    let n = liste.len();
    if nombre == 0
        || depuis.checked_add(nombre).is_none_or(|f| f > n)
        || vers.checked_add(nombre).is_none_or(|f| f > n)
    {
        return false;
    }
    let plage: Vec<T> = liste.drain(depuis..depuis + nombre).collect();
    let fin = liste.split_off(vers);
    liste.extend(plage);
    liste.extend(fin);
    true
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Instant};

    use chrono::{DateTime, Duration};

    use super::*;
    use crate::{
        analyse::{DIMENSION, Voisins},
        recommandation::{
            Ambiance, Meta, proches,
            score::{Habitude, Poids},
        },
    };

    fn id(i: usize) -> Uuid {
        Uuid::from_u128(i as u128 + 1)
    }

    fn elements(n: usize) -> Vec<Element> {
        (0..n)
            .map(|i| Element {
                id: Uuid::from_u128((1 << 64) + i as u128),
                piste: id(i),
            })
            .collect()
    }

    fn ctx<'a>(elements: &'a [Element], graine: i64, premier: Option<Uuid>) -> Contexte<'a> {
        Contexte {
            sub: "s",
            graine,
            premier,
            elements,
            suggestions: Suggestions::Off,
        }
    }

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn donnees(artistes: &[usize]) -> Donnees {
        Donnees {
            metas: artistes
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    (
                        id(i),
                        Meta {
                            artiste: id(1_000_000 + a),
                            album: id(2_000_000 + i),
                            present: true,
                            ..Meta::default()
                        },
                    )
                })
                .collect(),
            maintenant: t0(),
            ..Donnees::default()
        }
    }

    fn pistes(places: &[Place]) -> Vec<Uuid> {
        places.iter().map(|p| p.piste).collect()
    }

    fn repetitions(d: &Donnees, places: &[Place]) -> usize {
        places
            .windows(2)
            .filter(|p| d.metas[&p[0].piste].artiste == d.metas[&p[1].piste].artiste)
            .count()
    }

    #[test]
    fn meme_graine_meme_ordre_et_chaque_piste_une_fois() {
        let e = elements(50);
        let artistes: Vec<usize> = (0..50).map(|i| i % 7).collect();
        for d in [Donnees::default(), donnees(&artistes)] {
            let a = ordonner(&ctx(&e, 42, Some(e[10].id)), &d);
            assert_eq!(a, ordonner(&ctx(&e, 42, Some(e[10].id)), &d));
            assert_ne!(a, ordonner(&ctx(&e, 43, Some(e[10].id)), &d));
            assert_eq!(a[0].element, Some(e[10].id));
            let mut ids: Vec<Uuid> = a.iter().filter_map(|p| p.element).collect();
            let mut attendus: Vec<Uuid> = e.iter().map(|x| x.id).collect();
            ids.sort();
            attendus.sort();
            assert_eq!(ids, attendus);
            assert!(a.iter().all(|p| !p.suggeree));
        }
        assert!(ordonner(&ctx(&[], 1, None), &Donnees::default()).is_empty());
    }

    #[test]
    fn melange_uniforme() {
        let e = elements(3);
        let mut vus: HashMap<Vec<Option<Uuid>>, usize> = HashMap::new();
        for g in 0..6000 {
            let o = ordonner(&ctx(&e, g, None), &Donnees::default())
                .into_iter()
                .map(|p| p.element)
                .collect();
            *vus.entry(o).or_default() += 1;
        }
        assert_eq!(vus.len(), 6);
        assert!(vus.values().all(|&n| (800..1200).contains(&n)), "{vus:?}");
    }

    #[test]
    fn jamais_deux_fois_le_meme_artiste_a_la_suite_quand_c_est_possible() {
        let varies: Vec<usize> = (0..50).map(|i| i % 5).collect();
        let moitie: Vec<usize> = (0..19).map(|i| if i < 10 { 0 } else { i }).collect();
        let dominant: Vec<usize> = (0..20)
            .map(|i| if i < 10 { 0 } else { i % 3 + 1 })
            .collect();
        for artistes in [&varies, &moitie, &dominant] {
            let d = donnees(artistes);
            let e = elements(artistes.len());
            for g in 0..300 {
                let o = ordonner(&ctx(&e, g, None), &d);
                assert_eq!(repetitions(&d, &o), 0, "graine {g} : {artistes:?}");
                assert_eq!(o.len(), e.len());
            }
        }
        let d = donnees(&dominant);
        let e = elements(dominant.len());
        for g in 0..300 {
            let o = ordonner(&ctx(&e, g, Some(e[0].id)), &d);
            assert_eq!(o[0].element, Some(e[0].id));
            assert_eq!(
                repetitions(&d, &o),
                0,
                "piste de départ imposée, graine {g}"
            );
        }
        let impossible: Vec<usize> = (0..18).map(|i| usize::from(i >= 15)).collect();
        let d = donnees(&impossible);
        let e = elements(impossible.len());
        for g in 0..50 {
            let o = ordonner(&ctx(&e, g, None), &d);
            let mut p = pistes(&o);
            p.sort();
            assert_eq!(p, (0..18).map(id).collect::<Vec<_>>());
            assert_eq!(repetitions(&d, &o), 11, "au mieux : trois coupures");
        }
    }

    #[test]
    fn piste_souvent_sautee_apres_une_piste_ecoutee_en_entier() {
        let artistes: Vec<usize> = (0..30).collect();
        let mut d = donnees(&artistes);
        let hier = t0() - Duration::days(1);
        d.profil.pistes.insert(
            id(3),
            Habitude {
                sauts: 3,
                derniere: Some(hier),
                sauts_recents: vec![hier; 3],
                ..Habitude::default()
            },
        );
        d.profil.pistes.insert(
            id(4),
            Habitude {
                completes: 3,
                derniere: Some(hier),
                ..Habitude::default()
            },
        );
        let e = elements(30);
        for g in 0..300 {
            let o = pistes(&ordonner(&ctx(&e, g, None), &d));
            let rang = |p: Uuid| o.iter().position(|x| *x == p).unwrap();
            assert!(rang(id(3)) > rang(id(4)), "graine {g}");
            assert_eq!(rang(id(3)), 29, "graine {g}");
        }
        d.profil.pistes.remove(&id(3));
        let derniere = (0..300)
            .filter(|g| pistes(&ordonner(&ctx(&e, *g, None), &d))[29] == id(3))
            .count();
        assert!(derniere < 40, "sans saut, la piste n'est plus repoussée");
    }

    fn avec_candidats(source: usize, candidats: usize) -> (Donnees, Vec<Place>) {
        let artistes: Vec<usize> = (0..source + candidats).collect();
        let mut d = donnees(&artistes);
        d.candidats = (source..source + candidats).map(id).collect();
        let places = ordonner(&ctx(&elements(source), 7, None), &d);
        (d, places)
    }

    fn suggerees(places: &[Place]) -> Vec<usize> {
        places
            .iter()
            .enumerate()
            .filter(|(_, p)| p.suggeree)
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn suggestions_hors_source_marquees_au_rythme_choisi() {
        assert_eq!(Suggestions::Off.rythme(), None);
        let (d, base) = avec_candidats(30, 20);
        let source: HashSet<Uuid> = pistes(&base).into_iter().collect();
        for (mode, attendues) in [
            (Suggestions::Light, vec![10, 21, 32]),
            (Suggestions::Frequent, vec![4, 9, 14, 19, 24, 29, 34]),
        ] {
            let mut places = base.clone();
            let n = inserer(&mut places, 0, mode.rythme().unwrap(), &d);
            assert_eq!(suggerees(&places), attendues, "{mode:?}");
            assert_eq!(n, attendues.len());
            let s: Vec<&Place> = places.iter().filter(|p| p.suggeree).collect();
            assert!(
                s.iter()
                    .all(|p| p.element.is_none() && !source.contains(&p.piste))
            );
            let uniques: HashSet<Uuid> = s.iter().map(|p| p.piste).collect();
            assert_eq!(uniques.len(), s.len());
            let reste: Vec<Place> = places.iter().filter(|p| !p.suggeree).copied().collect();
            assert_eq!(reste, base, "l'ordre de la source est conservé");
            assert_eq!(inserer(&mut places, 0, mode.rythme().unwrap(), &d), 0);
            assert!(!a_prolonger(&places, 0, mode.rythme().unwrap()));
        }
        let (d, mut places) = avec_candidats(200, 60);
        assert!(a_prolonger(&places, 0, 4));
        inserer(&mut places, 0, 4, &d);
        let premiere = suggerees(&places);
        assert_eq!(premiere.len(), 10);
        assert!(*premiere.last().unwrap() <= HORIZON + 1);
        assert!(!a_prolonger(&places, 20, 4), "fenêtre déjà planifiée");
        assert!(a_prolonger(&places, 40, 4));
        inserer(&mut places, 40, 4, &d);
        let seconde = suggerees(&places);
        assert_eq!(seconde[..10], premiere[..]);
        assert!(seconde.len() > 15 && seconde.windows(2).all(|p| p[1] - p[0] == 5));
        let (mut vide, mut places) = avec_candidats(30, 20);
        vide.candidats.clear();
        assert_eq!(inserer(&mut places, 0, 4, &vide), 0);
        assert_eq!(places.len(), 30);
    }

    #[test]
    fn suggestion_sautee_ou_refusee_ecartee_et_like_qui_renforce_les_voisines() {
        let (mut d, _) = avec_candidats(10, 3);
        let (a, b, c) = (id(10), id(11), id(12));
        let libre = HashSet::new();
        let premier = d.choisir(None, &libre, &libre).unwrap();
        d.profil.sautees.insert(premier);
        let second = d.choisir(None, &libre, &libre).unwrap();
        assert_ne!(second, premier);
        d.profil.refusees.insert(second);
        let troisieme = d.choisir(None, &libre, &libre).unwrap();
        assert!(![premier, second].contains(&troisieme));
        assert!([a, b, c].contains(&troisieme));
        d.profil.sautees.insert(troisieme);
        assert_eq!(d.choisir(None, &libre, &libre), None);
        assert_eq!(d.ecartee(premier), Some("skipped_recently"));
        assert_eq!(d.ecartee(second), Some("dismissed"));

        let vecteur = |x: f32| vec![x, -x, x / 2.0];
        let (aimee, voisine, lointaine) = (id(100), id(101), id(102));
        let mut d = Donnees {
            voisins: Arc::new(Voisins::construire(vec![
                (id(0), vecteur(0.0)),
                (aimee, vecteur(1.0)),
                (voisine, vecteur(1.1)),
                (lointaine, vecteur(-1.1)),
            ])),
            candidats: vec![voisine, lointaine],
            maintenant: t0(),
            ..Donnees::default()
        };
        d.ambiance.barycentre = d.voisins.barycentre(&[voisine, lointaine]);
        let avant = (d.noter(voisine, None).total, d.noter(lointaine, None).total);
        assert!((avant.0 - avant.1).abs() < 1e-4, "{avant:?}");
        d.profil.aimees.insert(aimee);
        d.aimees = vec![aimee];
        d.preparer();
        let apres = (d.noter(voisine, None).total, d.noter(lointaine, None).total);
        assert!(apres.0 > avant.0 + 0.1, "{avant:?} {apres:?}");
        assert_eq!(apres.1, avant.1);
        assert_eq!(d.choisir(None, &libre, &libre), Some(voisine));
    }

    #[test]
    fn fenetre_de_file_rapide_sur_cinquante_mille_pistes() {
        let n = 50_000;
        let mut graine = 0x2545_f491_4f6c_dd1du64;
        let mut hasard = move || {
            graine ^= graine << 13;
            graine ^= graine >> 7;
            graine ^= graine << 17;
            (graine >> 40) as f32 / (1u64 << 24) as f32
        };
        let bruts: Vec<(Uuid, Vec<f32>)> = (0..n)
            .map(|i| {
                let groupe = (i % 10) as f32;
                (id(i), (0..DIMENSION).map(|_| groupe + hasard()).collect())
            })
            .collect();
        let voisins = Arc::new(Voisins::construire(bruts));
        let metas: HashMap<Uuid, Meta> = (0..n)
            .map(|i| {
                (
                    id(i),
                    Meta {
                        artiste: id(1_000_000 + i % 2000),
                        album: id(2_000_000 + i % 5000),
                        annee: Some(1970 + (i % 50) as i32),
                        genres: vec![(i % 30) as i64, (i % 7) as i64],
                        artiste_mbid: (i % 2000).to_string(),
                        present: true,
                    },
                )
            })
            .collect();
        let hier = t0() - Duration::days(1);
        let mut d = Donnees {
            voisins,
            metas,
            aimees: (0..300).map(|i| id(i * 150)).collect(),
            poids: Poids::default(),
            maintenant: t0(),
            ..Donnees::default()
        };
        for i in 0..5000 {
            d.profil.pistes.insert(
                id(i * 10),
                Habitude {
                    completes: (i % 4) as u32,
                    sauts: (i % 3) as u32,
                    derniere: Some(hier),
                    sauts_recents: vec![hier; i % 3],
                },
            );
        }
        d.profil.aimees = d.aimees.iter().copied().collect();
        let e: Vec<Element> = (0..1000)
            .map(|i| Element {
                id: Uuid::from_u128((1 << 64) + i as u128),
                piste: id(i * 50),
            })
            .collect();
        let source: Vec<Uuid> = e.iter().map(|x| x.piste).collect();
        let debut = Instant::now();
        d.ambiance = Ambiance::de(&source, &d.metas, &d.voisins);
        d.candidats = proches(
            &d.voisins,
            &d.ambiance,
            &source.iter().copied().collect::<HashSet<_>>(),
        );
        d.preparer();
        let mut places = ordonner(&ctx(&e, 5, None), &d);
        let ajoutees = inserer(&mut places, 0, 4, &d);
        let duree = debut.elapsed();
        assert_eq!(d.candidats.len(), crate::recommandation::CANDIDATS);
        assert_eq!((places.len(), ajoutees), (1010, 10));
        let seuil = if cfg!(debug_assertions) { 1000 } else { 200 };
        eprintln!("fenêtre de file sur {n} pistes : {duree:?} (seuil {seuil} ms)");
        assert!(duree.as_millis() < seuil, "{duree:?}");
    }

    #[test]
    fn deplacement_de_plage() {
        let mut l: Vec<u32> = (0..6).collect();
        assert!(deplacer(&mut l, 1, 2, 3));
        assert_eq!(l, [0, 3, 4, 1, 2, 5]);
        assert!(deplacer(&mut l, 3, 2, 0));
        assert_eq!(l, [1, 2, 0, 3, 4, 5]);
        assert!(!deplacer(&mut l, 5, 2, 0));
        assert!(!deplacer(&mut l, 0, 2, 5));
        assert!(!deplacer(&mut l, 0, 0, 1));
        assert!(!deplacer(&mut l, usize::MAX, 2, 0));
        assert_eq!(l, [1, 2, 0, 3, 4, 5]);
    }
}
