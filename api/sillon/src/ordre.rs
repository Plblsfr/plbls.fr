use uuid::Uuid;

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

    fn sous(&mut self, n: usize) -> usize {
        let n = n as u64;
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let x = self.suivant();
            if x < zone {
                return usize::try_from(x % n).unwrap_or(0);
            }
        }
    }
}

pub fn melanger(c: &Contexte) -> Vec<Place> {
    let mut tete = Vec::new();
    let mut reste: Vec<Element> = Vec::with_capacity(c.elements.len());
    for e in c.elements {
        if tete.is_empty() && Some(e.id) == c.premier {
            tete.push(*e);
        } else {
            reste.push(*e);
        }
    }
    let mut g = SplitMix(c.graine as u64);
    for i in (1..reste.len()).rev() {
        reste.swap(i, g.sous(i + 1));
    }
    tete.into_iter()
        .chain(reste)
        .map(|e| Place {
            element: Some(e.id),
            piste: e.piste,
            suggeree: false,
        })
        .collect()
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
    use std::collections::HashMap;

    use super::*;

    fn elements(n: usize) -> Vec<Element> {
        (0..n)
            .map(|_| Element {
                id: Uuid::new_v4(),
                piste: Uuid::new_v4(),
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

    #[test]
    fn melange_deterministe_complet_et_premier_en_tete() {
        let e = elements(50);
        let a = melanger(&ctx(&e, 42, Some(e[10].id)));
        assert_eq!(a, melanger(&ctx(&e, 42, Some(e[10].id))));
        assert_ne!(a, melanger(&ctx(&e, 43, Some(e[10].id))));
        assert_eq!(a[0].element, Some(e[10].id));
        let mut ids: Vec<Uuid> = a.iter().filter_map(|p| p.element).collect();
        let mut attendus: Vec<Uuid> = e.iter().map(|x| x.id).collect();
        ids.sort();
        attendus.sort();
        assert_eq!(ids, attendus);
        assert!(a.iter().all(|p| !p.suggeree));
        assert!(melanger(&ctx(&[], 1, None)).is_empty());
    }

    #[test]
    fn melange_uniforme() {
        let e = elements(3);
        let mut vus: HashMap<Vec<Option<Uuid>>, usize> = HashMap::new();
        for g in 0..6000 {
            let o = melanger(&ctx(&e, g, None))
                .into_iter()
                .map(|p| p.element)
                .collect();
            *vus.entry(o).or_default() += 1;
        }
        assert_eq!(vus.len(), 6);
        assert!(vus.values().all(|&n| (800..1200).contains(&n)), "{vus:?}");
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
