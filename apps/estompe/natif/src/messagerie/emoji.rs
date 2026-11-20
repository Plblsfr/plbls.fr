use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use estompe_client::lru::Lru;

const PAQUET: &[u8] = include_bytes!("../../emoji/twemoji.bin");
const MAGIQUE: &[u8; 4] = b"EMJ1";
pub const CAPACITE_BLOCS: usize = 4;
pub const CAPACITE_IMAGES: usize = 512;
pub const EMOJI_GRANDS_MAX: usize = 27;

struct Entree {
    bloc: u16,
    debut: u32,
    longueur: u32,
}

struct Bloc {
    debut: usize,
    compresse: usize,
    longueur: usize,
}

pub struct Paquet {
    entrees: HashMap<String, Entree>,
    premiers: HashSet<char>,
    longueur_max: usize,
    blocs: Vec<Bloc>,
    donnees: &'static [u8],
    decompresses: Mutex<Lru<u16, Arc<Vec<u8>>>>,
}

struct Lecteur<'a> {
    octets: &'a [u8],
    position: usize,
}

impl<'a> Lecteur<'a> {
    fn prendre(&mut self, n: usize) -> Option<&'a [u8]> {
        let fin = self.position.checked_add(n)?;
        let tranche = self.octets.get(self.position..fin)?;
        self.position = fin;
        Some(tranche)
    }

    fn u8(&mut self) -> Option<u8> {
        self.prendre(1).map(|o| o[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.prendre(2).map(|o| u16::from_le_bytes([o[0], o[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.prendre(4)
            .map(|o| u32::from_le_bytes([o[0], o[1], o[2], o[3]]))
    }
}

pub fn lire_paquet(octets: &'static [u8]) -> Option<Paquet> {
    let mut l = Lecteur {
        octets,
        position: 0,
    };
    if l.prendre(4)? != MAGIQUE {
        return None;
    }
    let nombre = l.u32()? as usize;
    let nombre_blocs = l.u32()? as usize;
    let mut entrees = HashMap::with_capacity(nombre);
    let mut premiers = HashSet::new();
    let mut longueur_max = 0;
    for _ in 0..nombre {
        let taille = usize::from(l.u8()?);
        let cle = std::str::from_utf8(l.prendre(taille)?).ok()?.to_string();
        let entree = Entree {
            bloc: l.u16()?,
            debut: l.u32()?,
            longueur: l.u32()?,
        };
        premiers.insert(cle.chars().next()?);
        longueur_max = longueur_max.max(cle.chars().count());
        entrees.insert(cle, entree);
    }
    let mut blocs = Vec::with_capacity(nombre_blocs);
    for _ in 0..nombre_blocs {
        blocs.push(Bloc {
            debut: l.u32()? as usize,
            compresse: l.u32()? as usize,
            longueur: l.u32()? as usize,
        });
    }
    let donnees = octets.get(l.position..)?;
    Some(Paquet {
        entrees,
        premiers,
        longueur_max,
        blocs,
        donnees,
        decompresses: Mutex::new(Lru::nouveau(CAPACITE_BLOCS)),
    })
}

pub fn paquet() -> &'static Paquet {
    static PAQUET_LU: OnceLock<Paquet> = OnceLock::new();
    PAQUET_LU.get_or_init(|| {
        lire_paquet(PAQUET).unwrap_or_else(|| Paquet {
            entrees: HashMap::new(),
            premiers: HashSet::new(),
            longueur_max: 0,
            blocs: Vec::new(),
            donnees: &[],
            decompresses: Mutex::new(Lru::nouveau(0)),
        })
    })
}

impl Paquet {
    pub fn nombre(&self) -> usize {
        self.entrees.len()
    }

    pub fn contient(&self, sequence: &str) -> bool {
        self.entrees.contains_key(sequence)
    }

    fn bloc(&self, numero: u16) -> Option<Arc<Vec<u8>>> {
        let mut cache = self.decompresses.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(bloc) = cache.lire(&numero) {
            return Some(bloc.clone());
        }
        let b = self.blocs.get(usize::from(numero))?;
        let compresse = self.donnees.get(b.debut..b.debut.checked_add(b.compresse)?)?;
        let brut = miniz_oxide::inflate::decompress_to_vec_with_limit(compresse, b.longueur).ok()?;
        let brut = Arc::new(brut);
        cache.inserer(numero, brut.clone());
        Some(brut)
    }

    pub fn svg(&self, sequence: &str) -> Option<Vec<u8>> {
        let e = self.entrees.get(sequence)?;
        let bloc = self.bloc(e.bloc)?;
        let debut = e.debut as usize;
        bloc.get(debut..debut.checked_add(e.longueur as usize)?)
            .map(<[u8]>::to_vec)
    }

    pub fn blocs_en_memoire(&self) -> usize {
        self.decompresses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    fn correspondance(&self, reste: &str) -> Option<usize> {
        let premier = reste.chars().next()?;
        if !self.premiers.contains(&premier) {
            return None;
        }
        let bornes: Vec<usize> = reste
            .char_indices()
            .map(|(i, _)| i)
            .skip(1)
            .chain(std::iter::once(reste.len()))
            .take(self.longueur_max)
            .collect();
        bornes
            .into_iter()
            .rev()
            .find(|&fin| self.entrees.contains_key(&reste[..fin]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Morceau<'a> {
    Texte(&'a str),
    Emoji(&'a str),
}

pub fn decouper(texte: &str) -> Vec<Morceau<'_>> {
    let p = paquet();
    let mut morceaux = Vec::new();
    let mut debut_texte = 0;
    let mut i = 0;
    while i < texte.len() {
        if let Some(longueur) = p.correspondance(&texte[i..]) {
            if debut_texte < i {
                morceaux.push(Morceau::Texte(&texte[debut_texte..i]));
            }
            morceaux.push(Morceau::Emoji(&texte[i..i + longueur]));
            i += longueur;
            debut_texte = i;
        } else {
            i += texte[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if debut_texte < texte.len() {
        morceaux.push(Morceau::Texte(&texte[debut_texte..]));
    }
    morceaux
}

pub fn contient_un_emoji(texte: &str) -> bool {
    decouper(texte)
        .iter()
        .any(|m| matches!(m, Morceau::Emoji(_)))
}

thread_local! {
    static IMAGES: RefCell<Lru<String, slint::Image>> = RefCell::new(Lru::nouveau(CAPACITE_IMAGES));
}

pub fn image(sequence: &str) -> Option<slint::Image> {
    IMAGES.with(|images| {
        let mut images = images.borrow_mut();
        if let Some(image) = images.lire(&sequence.to_string()) {
            return Some(image.clone());
        }
        let image = slint::Image::load_from_svg_data(&paquet().svg(sequence)?).ok()?;
        images.inserer(sequence.to_string(), image.clone());
        Some(image)
    })
}

pub fn images_en_memoire() -> usize {
    IMAGES.with(|images| images.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emoji(morceaux: &[Morceau]) -> Vec<String> {
        morceaux
            .iter()
            .filter_map(|m| match m {
                Morceau::Emoji(e) => Some(e.to_string()),
                Morceau::Texte(_) => None,
            })
            .collect()
    }

    #[test]
    fn le_paquet_embarque_couvre_les_sequences_composees() {
        let p = paquet();
        assert!(p.nombre() > 3700, "{}", p.nombre());
        for sequence in [
            "😀",
            "👍",
            "👍🏽",
            "🇫🇷",
            "🏳️‍🌈",
            "👩‍👩‍👧‍👦",
            "🧑🏻‍🤝‍🧑🏿",
            "❤️",
            "#️⃣",
            "👁️‍🗨️",
            "🫡",
        ] {
            assert!(p.contient(sequence), "{sequence}");
            let svg = p.svg(sequence).unwrap();
            assert!(svg.starts_with(b"<svg"), "{sequence}");
        }
        assert!(p.blocs_en_memoire() <= CAPACITE_BLOCS);
    }

    #[test]
    fn decoupe_le_texte_en_gardant_les_sequences_entieres() {
        let texte = "Salut 👋🏾 ! Drapeau 🇫🇷🇧🇪, famille 👨‍👩‍👧 et cœur ❤️.";
        let morceaux = decouper(texte);
        assert_eq!(emoji(&morceaux), ["👋🏾", "🇫🇷", "🇧🇪", "👨‍👩‍👧", "❤️"]);
        let recompose: String = morceaux
            .iter()
            .map(|m| match m {
                Morceau::Texte(t) | Morceau::Emoji(t) => *t,
            })
            .collect();
        assert_eq!(recompose, texte);
    }

    #[test]
    fn un_caractere_de_presentation_texte_reste_du_texte() {
        for texte in ["© 2026", "®", "5 € — 3 # 4", "❤ simple", "™", "↔"] {
            assert!(!contient_un_emoji(texte), "{texte}");
        }
        assert!(contient_un_emoji("❤️"));
        assert!(contient_un_emoji("☺️"));
        assert_eq!(decouper(""), Vec::<Morceau>::new());
        assert_eq!(decouper("abc"), [Morceau::Texte("abc")]);
    }

    #[test]
    fn un_paquet_corrompu_est_refuse_sans_panique() {
        assert!(lire_paquet(b"").is_none());
        assert!(lire_paquet(b"XXXX\0\0\0\0\0\0\0\0").is_none());
        assert!(lire_paquet(b"EMJ1\x05\0\0\0\0\0\0\0").is_none());
        let vide = lire_paquet(b"EMJ1\0\0\0\0\0\0\0\0").unwrap();
        assert_eq!(vide.nombre(), 0);
        assert!(vide.svg("😀").is_none());
    }
}
