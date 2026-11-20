mod blocs;
mod en_ligne;
#[cfg(test)]
mod tests;

use crate::mention::{est_pseudo_simple, ContexteMentions, GenreMention};

pub const BLOCS_MAX: usize = 2_000;
pub const SEGMENTS_MAX: usize = 20_000;
pub const PROFONDEUR_BLOCS_MAX: usize = 16;
pub const PROFONDEUR_EN_LIGNE_MAX: usize = 3;
pub const LONGUEUR_URL_MAX: usize = 2_048;
pub const LONGUEUR_BALISE_MAX: usize = 512;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendu {
    pub blocs: Vec<Bloc>,
    pub tronque: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Bloc {
    Paragraphe(Vec<Segment>),
    Titre {
        niveau: u8,
        segments: Vec<Segment>,
    },
    Citation(Vec<Bloc>),
    Liste {
        ordonnee: bool,
        elements: Vec<Vec<Bloc>>,
    },
    Code {
        langage: Option<String>,
        texte: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Styles {
    pub gras: bool,
    pub italique: bool,
    pub barre: bool,
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Contenu {
    Texte(String),
    SautDeLigne,
    Mention { texte: String, genre: GenreMention },
    Emoji(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub contenu: Contenu,
    pub styles: Styles,
    pub lien: Option<String>,
    pub masque: Option<u32>,
}

impl Segment {
    pub fn texte(texte: &str) -> Self {
        Segment {
            contenu: Contenu::Texte(texte.to_string()),
            styles: Styles::default(),
            lien: None,
            masque: None,
        }
    }

    pub fn texte_affiche(&self) -> String {
        match &self.contenu {
            Contenu::Texte(t) => t.clone(),
            Contenu::SautDeLigne => "\n".to_string(),
            Contenu::Mention { texte, .. } => format!("@{texte}"),
            Contenu::Emoji(nom) => format!(":{nom}:"),
        }
    }
}

pub(crate) struct Etat {
    blocs: usize,
    segments: usize,
    masques: u32,
    tronque: bool,
}

impl Etat {
    fn nouveau() -> Self {
        Etat {
            blocs: 0,
            segments: 0,
            masques: 0,
            tronque: false,
        }
    }

    fn reserver_bloc(&mut self) -> bool {
        if self.tronque || self.blocs >= BLOCS_MAX {
            self.tronque = true;
            return false;
        }
        self.blocs += 1;
        true
    }

    fn borner_segments(&mut self, segments: &mut Vec<Segment>) {
        let reste = SEGMENTS_MAX - self.segments;
        if segments.len() > reste {
            segments.truncate(reste);
            self.tronque = true;
        }
        self.segments += segments.len();
    }

    fn nouveau_masque(&mut self) -> u32 {
        self.masques += 1;
        self.masques
    }
}

pub(crate) struct Resolveur<'a> {
    contexte: &'a dyn ContexteMentions,
    composes: Vec<Vec<char>>,
}

impl<'a> Resolveur<'a> {
    fn nouveau(contexte: &'a dyn ContexteMentions) -> Self {
        let mut composes: Vec<Vec<char>> = contexte
            .noms_composes()
            .into_iter()
            .map(|n| n.to_lowercase())
            .filter(|n| !n.is_empty() && !est_pseudo_simple(n))
            .map(|n| n.chars().collect())
            .collect();
        composes.sort_by_key(|n| std::cmp::Reverse(n.len()));
        composes.dedup();
        Resolveur { contexte, composes }
    }
}

pub fn analyser(texte: &str, contexte: &dyn ContexteMentions) -> Rendu {
    let resolveur = Resolveur::nouveau(contexte);
    let mut etat = Etat::nouveau();
    let normalise = texte.replace("\r\n", "\n").replace('\r', "\n");
    let lignes: Vec<String> = normalise.split('\n').map(String::from).collect();
    let blocs = blocs::analyser(&lignes, 0, &resolveur, &mut etat);
    Rendu {
        blocs,
        tronque: etat.tronque,
    }
}

fn espace_d_attribut(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{20}' | '\u{a0}' | '\u{1680}' | '\u{180e}' | '\u{2000}'..='\u{2029}' | '\u{205f}' | '\u{3000}')
}

pub fn lien_ouvrable(url: &str) -> Option<String> {
    let compact: String = url.chars().filter(|c| !espace_d_attribut(*c)).collect();
    let nettoye: String = url
        .trim_matches(espace_d_attribut)
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let schema_permis = |texte: &str| {
        ["http:", "https:", "mailto:"].iter().any(|s| {
            texte
                .get(..s.len())
                .is_some_and(|tete| tete.eq_ignore_ascii_case(s))
        })
    };
    if !schema_permis(&compact) || !schema_permis(&nettoye) || nettoye.len() > LONGUEUR_URL_MAX {
        return None;
    }
    let mut sortie = String::with_capacity(nettoye.len());
    for c in nettoye.chars() {
        if c.is_ascii_alphanumeric() || ";,/?:@&=+$-_.!~*'()#%".contains(c) {
            sortie.push(c);
        } else {
            let mut tampon = [0u8; 4];
            for octet in c.encode_utf8(&mut tampon).bytes() {
                sortie.push_str(&format!("%{octet:02X}"));
            }
        }
    }
    Some(sortie)
}

fn texte_brut_dans(blocs: &[Bloc], sortie: &mut Vec<String>) {
    for bloc in blocs {
        match bloc {
            Bloc::Paragraphe(segments) | Bloc::Titre { segments, .. } => {
                sortie.push(segments.iter().map(Segment::texte_affiche).collect());
            }
            Bloc::Code { texte, .. } => sortie.push(texte.clone()),
            Bloc::Citation(interieur) => texte_brut_dans(interieur, sortie),
            Bloc::Liste { elements, .. } => {
                for element in elements {
                    texte_brut_dans(element, sortie);
                }
            }
        }
    }
}

pub fn texte_brut(blocs: &[Bloc]) -> String {
    let mut lignes = Vec::new();
    texte_brut_dans(blocs, &mut lignes);
    lignes.join("\n")
}

fn sans_code_pour_lien(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut reste = texte;
    while let Some(i) = reste.find("```") {
        let apres = &reste[i + 3..];
        match apres.find("```") {
            Some(f) => {
                sortie.push_str(&reste[..i]);
                reste = &apres[f + 3..];
            }
            None => break,
        }
    }
    sortie.push_str(reste);
    let mut final_ = String::with_capacity(sortie.len());
    let mut reste = sortie.as_str();
    while let Some(i) = reste.find('`') {
        final_.push_str(&reste[..i]);
        let corps = &reste[i + 1..];
        match corps.find(['`', '\n']) {
            Some(f) if corps.as_bytes()[f] == b'`' => reste = &corps[f + 1..],
            _ => {
                final_.push('`');
                reste = corps;
            }
        }
    }
    final_.push_str(reste);
    final_
}

pub fn premier_lien(texte: &str) -> Option<String> {
    let propre = sans_code_pour_lien(texte);
    let mut precedent_mot = false;
    for (i, c) in propre.char_indices() {
        let debut_possible = !precedent_mot && (c == 'h' || c == 'H');
        precedent_mot = c.is_ascii_alphanumeric() || c == '_';
        if !debut_possible {
            continue;
        }
        let suite = &propre[i..];
        let schema = ["https://", "http://"].into_iter().find(|s| {
            suite
                .get(..s.len())
                .is_some_and(|t| t.eq_ignore_ascii_case(s))
        });
        let Some(schema) = schema else {
            continue;
        };
        let corps = &suite[schema.len()..];
        let fin = corps
            .find(|c: char| c.is_whitespace() || "<>()]\"'".contains(c))
            .unwrap_or(corps.len());
        if fin == 0 {
            continue;
        }
        let lien = &propre[i..i + schema.len() + fin];
        let lien = lien.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        return Some(lien.to_string());
    }
    None
}
