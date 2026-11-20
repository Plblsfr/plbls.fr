use std::collections::HashMap;

use super::{
    lien_ouvrable, Contenu, Etat, Resolveur, Segment, Styles, LONGUEUR_BALISE_MAX,
    LONGUEUR_URL_MAX, PROFONDEUR_EN_LIGNE_MAX,
};
use crate::mention::{car_de_pseudo, GenreMention};

const PARENTHESES_MAX: usize = 32;

enum Element {
    Texte(String),
    Delim(usize),
    Saut,
    Code(String),
    Mention {
        texte: String,
        genre: GenreMention,
    },
    Emoji(String),
    Lien {
        url: Option<String>,
        texte: String,
        apres_delim: Option<usize>,
    },
    Conteneur {
        segments: Vec<Segment>,
        lien: Option<String>,
        masque: Option<u32>,
    },
}

struct Delimiteur {
    element: usize,
    car: char,
    longueur: usize,
    restant: usize,
    ouvre: bool,
    ferme: bool,
    prec: Option<usize>,
    suiv: Option<usize>,
}

#[derive(Clone, Copy)]
enum Emphase {
    Gras,
    Italique,
    Barre,
}

pub(super) fn analyser(texte: &str, resolveur: &Resolveur, etat: &mut Etat) -> Vec<Segment> {
    let caracteres: Vec<char> = texte.chars().collect();
    tranche(&caracteres, 0, false, resolveur, etat)
}

fn ponctuation(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace()
}

fn ponctuation_echappable(c: char) -> bool {
    c.is_ascii_punctuation()
}

struct Analyse {
    elements: Vec<Element>,
    delims: Vec<Delimiteur>,
    tampon: String,
    jeton_depuis: Option<usize>,
    ouvrants_souligne: usize,
}

impl Analyse {
    fn vider(&mut self) {
        if !self.tampon.is_empty() {
            self.elements
                .push(Element::Texte(std::mem::take(&mut self.tampon)));
        }
        self.jeton_depuis = None;
    }

    fn pousser(&mut self, element: Element) {
        self.vider();
        self.elements.push(element);
    }

    fn car_brut(&mut self, c: char, i: usize) {
        if self.tampon.is_empty() {
            self.jeton_depuis = Some(i);
        }
        self.tampon.push(c);
    }

    fn car_converti(&mut self, c: char) {
        self.tampon.push(c);
        self.jeton_depuis = None;
    }

    fn texte_converti(&mut self, t: &str) {
        self.tampon.push_str(t);
        self.jeton_depuis = None;
    }
}

fn longueur_suite(s: &[char], i: usize, c: char) -> usize {
    s[i..].iter().take_while(|&&x| x == c).count()
}

fn suites_accents_graves(s: &[char]) -> HashMap<usize, Vec<usize>> {
    let mut suites: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == '`' {
            let l = longueur_suite(s, i, '`');
            suites.entry(l).or_default().push(i);
            i += l;
        } else {
            i += 1;
        }
    }
    suites
}

fn crochets_apparies(s: &[char]) -> HashMap<usize, usize> {
    let mut pile = Vec::new();
    let mut paires = HashMap::new();
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            '\\' => i += 1,
            '[' => pile.push(i),
            ']' => {
                if let Some(o) = pile.pop() {
                    paires.insert(o, i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    paires
}

fn fermetures_masque(s: &[char]) -> Vec<usize> {
    (1..s.len().saturating_sub(1))
        .filter(|&j| s[j] == '|' && s[j + 1] == '|' && !s[j - 1].is_whitespace())
        .collect()
}

fn commence_par(s: &[char], i: usize, motif: &str) -> bool {
    let longueur = motif.chars().count();
    s.len() >= i + longueur
        && s[i..i + longueur]
            .iter()
            .zip(motif.chars())
            .all(|(a, m)| a.eq_ignore_ascii_case(&m))
}

fn destination(s: &[char], debut: usize) -> Option<(String, usize)> {
    let n = s.len();
    let mut j = debut;
    while j < n && (s[j] == ' ' || s[j] == '\t' || s[j] == '\n') {
        j += 1;
    }
    let mut dest = String::new();
    if j < n && s[j] == '<' {
        j += 1;
        let ouverture = j;
        while j < n && s[j] != '>' {
            if s[j] == '<' || s[j] == '\n' || j - ouverture > LONGUEUR_URL_MAX {
                return None;
            }
            if s[j] == '\\' && j + 1 < n && ponctuation_echappable(s[j + 1]) {
                j += 1;
            }
            dest.push(s[j]);
            j += 1;
        }
        if j >= n {
            return None;
        }
        j += 1;
    } else {
        let ouverture = j;
        let mut profondeur = 0usize;
        while j < n && j - ouverture <= LONGUEUR_URL_MAX {
            let c = s[j];
            if c == '\\' && j + 1 < n && ponctuation_echappable(s[j + 1]) {
                dest.push(s[j + 1]);
                j += 2;
                continue;
            }
            if c.is_whitespace() || c.is_control() {
                break;
            }
            if c == '(' {
                profondeur += 1;
                if profondeur > PARENTHESES_MAX {
                    return None;
                }
            } else if c == ')' {
                if profondeur == 0 {
                    break;
                }
                profondeur -= 1;
            }
            dest.push(c);
            j += 1;
        }
        if profondeur != 0 || j - ouverture > LONGUEUR_URL_MAX {
            return None;
        }
    }
    let avant_titre = j;
    while j < n && s[j].is_whitespace() {
        j += 1;
    }
    if j < n && j > avant_titre && matches!(s[j], '"' | '\'' | '(') {
        let fin = if s[j] == '(' { ')' } else { s[j] };
        let ouverture = j;
        j += 1;
        while j < n && s[j] != fin {
            if j - ouverture > LONGUEUR_URL_MAX {
                return None;
            }
            if s[j] == '\\' {
                j += 1;
            }
            j += 1;
        }
        if j >= n {
            return None;
        }
        j += 1;
        while j < n && s[j].is_whitespace() {
            j += 1;
        }
    }
    if j < n && s[j] == ')' {
        Some((dest, j + 1))
    } else {
        None
    }
}

fn autolien(s: &[char], i: usize) -> Option<(String, String, usize)> {
    let n = s.len();
    let limite = (i + 1 + LONGUEUR_URL_MAX).min(n);
    let mut j = i + 1;
    if j < n && s[j].is_ascii_alphabetic() {
        let mut k = j + 1;
        while k < limite && (s[k].is_ascii_alphanumeric() || matches!(s[k], '+' | '.' | '-')) {
            k += 1;
        }
        let longueur_schema = k - j;
        if (2..=32).contains(&longueur_schema) && k < limite && s[k] == ':' {
            k += 1;
            while k < limite
                && !s[k].is_whitespace()
                && !s[k].is_control()
                && s[k] != '<'
                && s[k] != '>'
            {
                k += 1;
            }
            if k < limite && s[k] == '>' {
                let interieur: String = s[i + 1..k].iter().collect();
                return Some((interieur.clone(), interieur, k + 1));
            }
        }
    }
    let debut_local = j;
    while j < limite && (s[j].is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(s[j])) {
        j += 1;
    }
    if j == debut_local || j >= limite || s[j] != '@' {
        return None;
    }
    j += 1;
    let debut_domaine = j;
    while j < limite && (s[j].is_ascii_alphanumeric() || s[j] == '-' || s[j] == '.') {
        j += 1;
    }
    if j == debut_domaine || j >= limite || s[j] != '>' {
        return None;
    }
    let interieur: String = s[i + 1..j].iter().collect();
    Some((interieur.clone(), format!("mailto:{interieur}"), j + 1))
}

fn balise(s: &[char], i: usize) -> Option<usize> {
    let n = s.len();
    let limite = (i + LONGUEUR_BALISE_MAX).min(n);
    if commence_par(s, i, "<!--") {
        let mut j = i + 4;
        while j + 2 < limite {
            if s[j] == '-' && s[j + 1] == '-' && s[j + 2] == '>' {
                return Some(j + 3);
            }
            j += 1;
        }
        return None;
    }
    let mut j = i + 1;
    if j < n && s[j] == '/' {
        j += 1;
    }
    if j >= n || !s[j].is_ascii_alphabetic() {
        return None;
    }
    while j < limite {
        match s[j] {
            '>' => return Some(j + 1),
            '<' => return None,
            _ => j += 1,
        }
    }
    None
}

fn entite_en_fin(t: &[char]) -> bool {
    t.len() >= 3
        && t[t.len() - 1] == ';'
        && t[1..t.len() - 1].iter().all(|c| c.is_ascii_alphanumeric())
}

fn recul(t: &[char]) -> usize {
    let ponct = |c: char| "?!.,:;*_'\"~)".contains(c);
    let n = t.len();
    let mut i = 0;
    while i < n {
        let c = t[i];
        if !"?!.,:;*_'\"~()&".contains(c) {
            i += 1;
        } else if c == '(' {
            match t[i + 1..].iter().position(|&x| x == ')') {
                Some(p) => i += p + 2,
                None => break,
            }
        } else if c == '&' {
            if entite_en_fin(&t[i..]) {
                break;
            }
            i += 1;
        } else {
            let mut k = i;
            while k < n && ponct(t[k]) {
                k += 1;
            }
            if k == n {
                break;
            }
            i = k;
        }
    }
    i
}

fn url_nue(s: &[char], i: usize) -> Option<(String, String, usize)> {
    let prefixe = ["https://", "http://", "ftp://", "www."]
        .into_iter()
        .find(|p| commence_par(s, i, p))?;
    let apres = i + prefixe.len();
    if apres >= s.len() || !(s[apres].is_ascii_alphanumeric() || s[apres] == '-') {
        return None;
    }
    let mut fin = apres;
    while fin < s.len() && !s[fin].is_whitespace() && s[fin] != '<' {
        fin += 1;
    }
    let mut longueur = fin - i;
    for _ in 0..8 {
        let l = recul(&s[i..i + longueur]);
        if l == longueur {
            break;
        }
        longueur = l;
    }
    if longueur <= prefixe.len() {
        return None;
    }
    let texte: String = s[i..i + longueur].iter().collect();
    let href = if prefixe == "www." {
        format!("http://{texte}")
    } else {
        texte.clone()
    };
    Some((texte, href, i + longueur))
}

fn domaine_de_courriel(s: &[char], debut: usize) -> Option<usize> {
    let n = s.len();
    let etiquette = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let mut j = debut;
    while j < n && etiquette(s[j]) {
        j += 1;
    }
    if j == debut {
        return None;
    }
    let mut fin = None;
    while j < n && s[j] == '.' {
        let mut k = j + 1;
        while k < n && etiquette(s[k]) {
            k += 1;
        }
        if k == j + 1 || !s[k - 1].is_ascii_alphanumeric() {
            break;
        }
        fin = Some(k);
        j = k;
    }
    fin
}

fn entite(s: &[char], i: usize) -> Option<(String, usize)> {
    let n = s.len();
    let mut j = i + 1;
    if j < n && s[j] == '#' {
        j += 1;
        let hexa = j < n && (s[j] == 'x' || s[j] == 'X');
        if hexa {
            j += 1;
        }
        let debut = j;
        while j < n
            && j - debut < 8
            && (if hexa {
                s[j].is_ascii_hexdigit()
            } else {
                s[j].is_ascii_digit()
            })
        {
            j += 1;
        }
        if j == debut || j >= n || s[j] != ';' || j - debut > (if hexa { 6 } else { 7 }) {
            return None;
        }
        let chiffres: String = s[debut..j].iter().collect();
        let valeur = u32::from_str_radix(&chiffres, if hexa { 16 } else { 10 }).ok()?;
        let c = if valeur == 0 {
            '\u{fffd}'
        } else {
            char::from_u32(valeur).unwrap_or('\u{fffd}')
        };
        return Some((c.to_string(), j + 1));
    }
    let debut = j;
    while j < n && j - debut < 32 && s[j].is_ascii_alphanumeric() {
        j += 1;
    }
    if j == debut || j >= n || s[j] != ';' {
        return None;
    }
    let nom: String = s[debut..j].iter().collect();
    let valeur = match nom.as_str() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "hellip" => "…",
        "mdash" => "—",
        "ndash" => "–",
        "laquo" => "«",
        "raquo" => "»",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "euro" => "€",
        "deg" => "°",
        "times" => "×",
        "divide" => "÷",
        "middot" => "·",
        "bull" => "•",
        "eacute" => "é",
        "egrave" => "è",
        "ecirc" => "ê",
        "euml" => "ë",
        "agrave" => "à",
        "acirc" => "â",
        "ccedil" => "ç",
        "icirc" => "î",
        "iuml" => "ï",
        "ocirc" => "ô",
        "ugrave" => "ù",
        "ucirc" => "û",
        "Eacute" => "É",
        "Egrave" => "È",
        "Agrave" => "À",
        "Ccedil" => "Ç",
        _ => return None,
    };
    Some((valeur.to_string(), j + 1))
}

fn mention(
    s: &[char],
    i: usize,
    resolveur: &Resolveur,
    souligne_ouvert: bool,
) -> Option<(Element, usize)> {
    let n = s.len();
    'noms: for nom in &resolveur.composes {
        let mut k = i + 1;
        let mut idx = 0;
        while idx < nom.len() {
            if k >= n {
                continue 'noms;
            }
            for bas in s[k].to_lowercase() {
                if idx < nom.len() && nom[idx] == bas {
                    idx += 1;
                } else {
                    continue 'noms;
                }
            }
            k += 1;
        }
        if k < n && (s[k].is_alphanumeric() || s[k] == '_') {
            continue;
        }
        let nom_texte: String = nom.iter().collect();
        let genre = resolveur
            .contexte
            .genre(&nom_texte)
            .unwrap_or(GenreMention::Role);
        let texte: String = s[i + 1..k].iter().collect();
        return Some((Element::Mention { texte, genre }, k));
    }
    let mut k = i + 1;
    while k < n && k - i - 1 < 20 && car_de_pseudo(s[k]) {
        k += 1;
    }
    if k - i - 1 < 3 {
        return None;
    }
    while k > i + 1 && matches!(s[k - 1], '.' | '-') {
        k -= 1;
    }
    if k - i - 1 < 3 {
        return None;
    }
    let texte: String = s[i + 1..k].iter().collect();
    if let Some(genre) = resolveur.contexte.genre(&texte.to_lowercase()) {
        return Some((Element::Mention { texte, genre }, k));
    }
    let raccourci = texte.trim_end_matches('_');
    if !souligne_ouvert || raccourci.len() == texte.len() || raccourci.len() < 3 {
        return None;
    }
    let genre = resolveur.contexte.genre(&raccourci.to_lowercase())?;
    let fin = i + 1 + raccourci.len();
    Some((
        Element::Mention {
            texte: raccourci.to_string(),
            genre,
        },
        fin,
    ))
}

fn emoji(s: &[char], i: usize, resolveur: &Resolveur) -> Option<(Element, usize)> {
    let mut k = i + 1;
    while k < s.len() && k - i - 1 <= 32 && (s[k].is_ascii_alphanumeric() || s[k] == '_') {
        k += 1;
    }
    let longueur = k - i - 1;
    if !(2..=32).contains(&longueur) || k >= s.len() || s[k] != ':' {
        return None;
    }
    let nom: String = s[i + 1..k].iter().collect::<String>().to_ascii_lowercase();
    if !resolveur.contexte.emoji_connu(&nom) {
        return None;
    }
    Some((Element::Emoji(nom), k + 1))
}

fn flanquements(s: &[char], i: usize, longueur: usize) -> (bool, bool) {
    let avant = if i == 0 { ' ' } else { s[i - 1] };
    let apres = s.get(i + longueur).copied().unwrap_or(' ');
    let gauche = !apres.is_whitespace()
        && (!ponctuation(apres) || avant.is_whitespace() || ponctuation(avant));
    let droite = !avant.is_whitespace()
        && (!ponctuation(avant) || apres.is_whitespace() || ponctuation(apres));
    match s[i] {
        '_' => (
            gauche && (!droite || ponctuation(avant)),
            droite && (!gauche || ponctuation(apres)),
        ),
        _ => (gauche, droite),
    }
}

fn tranche(
    s: &[char],
    profondeur: usize,
    dans_lien: bool,
    resolveur: &Resolveur,
    etat: &mut Etat,
) -> Vec<Segment> {
    let n = s.len();
    let accents = suites_accents_graves(s);
    let crochets = crochets_apparies(s);
    let fermetures = fermetures_masque(s);
    let conteneurs_permis = profondeur < PROFONDEUR_EN_LIGNE_MAX;
    let mut a = Analyse {
        elements: Vec::new(),
        delims: Vec::new(),
        tampon: String::new(),
        jeton_depuis: None,
        ouvrants_souligne: 0,
    };
    let mut i = 0;
    while i < n {
        let c = s[i];
        match c {
            '|' if conteneurs_permis
                && i + 2 < n
                && s[i + 1] == '|'
                && !s[i + 2].is_whitespace() =>
            {
                let depart = fermetures.partition_point(|&j| j < i + 3);
                if let Some(&j) = fermetures.get(depart) {
                    let segments =
                        tranche(&s[i + 2..j], profondeur + 1, dans_lien, resolveur, etat);
                    let masque = Some(etat.nouveau_masque());
                    a.pousser(Element::Conteneur {
                        segments,
                        lien: None,
                        masque,
                    });
                    i = j + 2;
                    continue;
                }
                a.car_brut(c, i);
                i += 1;
            }
            '\\' => {
                if i + 1 < n && ponctuation_echappable(s[i + 1]) {
                    a.pousser(Element::Texte(s[i + 1].to_string()));
                    i += 2;
                } else if i + 1 < n && s[i + 1] == '\n' {
                    a.pousser(Element::Saut);
                    i += 2;
                    while i < n && (s[i] == ' ' || s[i] == '\t') {
                        i += 1;
                    }
                } else {
                    a.car_brut(c, i);
                    i += 1;
                }
            }
            '`' => {
                let l = longueur_suite(s, i, '`');
                let suivante = accents.get(&l).and_then(|positions| {
                    let k = positions.partition_point(|&p| p <= i);
                    positions.get(k).copied()
                });
                match suivante {
                    Some(p) => {
                        let mut contenu: String = s[i + l..p]
                            .iter()
                            .map(|&x| if x == '\n' { ' ' } else { x })
                            .collect();
                        if contenu.len() >= 2
                            && contenu.starts_with(' ')
                            && contenu.ends_with(' ')
                            && contenu.trim() != ""
                        {
                            contenu = contenu[1..contenu.len() - 1].to_string();
                        }
                        a.pousser(Element::Code(contenu));
                        i = p + l;
                    }
                    None => {
                        for _ in 0..l {
                            a.car_converti('`');
                        }
                        i += l;
                    }
                }
            }
            '!' if conteneurs_permis && !dans_lien && i + 1 < n && s[i + 1] == '[' => {
                let image = crochets.get(&(i + 1)).and_then(|&m| {
                    if m + 1 < n && s[m + 1] == '(' {
                        destination(s, m + 2).map(|(dest, fin)| (m, dest, fin))
                    } else {
                        None
                    }
                });
                match image {
                    Some((m, dest, fin)) => {
                        let alt: String = s[i + 2..m].iter().collect();
                        let texte = if alt.is_empty() { dest.clone() } else { alt };
                        a.pousser(Element::Lien {
                            url: lien_ouvrable(&dest),
                            texte,
                            apres_delim: None,
                        });
                        i = fin;
                    }
                    None => {
                        a.car_brut(c, i);
                        i += 1;
                    }
                }
            }
            '[' if conteneurs_permis && !dans_lien => {
                let lien = crochets.get(&i).and_then(|&m| {
                    if m + 1 < n && s[m + 1] == '(' {
                        destination(s, m + 2).map(|(dest, fin)| (m, dest, fin))
                    } else {
                        None
                    }
                });
                match lien {
                    Some((m, dest, fin)) => {
                        let segments = tranche(&s[i + 1..m], profondeur + 1, true, resolveur, etat);
                        a.pousser(Element::Conteneur {
                            segments,
                            lien: lien_ouvrable(&dest),
                            masque: None,
                        });
                        i = fin;
                    }
                    None => {
                        a.car_brut(c, i);
                        i += 1;
                    }
                }
            }
            '<' => {
                if !dans_lien {
                    if let Some((texte, href, fin)) = autolien(s, i) {
                        a.pousser(Element::Lien {
                            url: lien_ouvrable(&href),
                            texte,
                            apres_delim: None,
                        });
                        i = fin;
                        continue;
                    }
                }
                match balise(s, i) {
                    Some(fin) => {
                        let brut: String = s[i..fin].iter().collect();
                        a.pousser(Element::Texte(brut));
                        i = fin;
                    }
                    None => {
                        a.car_brut(c, i);
                        i += 1;
                    }
                }
            }
            'h' | 'H' | 'f' | 'F' | 'w' | 'W'
                if !dans_lien
                    && (!matches!(c, 'w' | 'W')
                        || a.tampon.is_empty()
                        || s[i - 1].is_whitespace()
                        || matches!(s[i - 1], '*' | '_' | '~' | '(')) =>
            {
                match url_nue(s, i) {
                    Some((texte, href, fin)) => {
                        a.pousser(Element::Lien {
                            url: lien_ouvrable(&href),
                            texte,
                            apres_delim: None,
                        });
                        i = fin;
                    }
                    None => {
                        a.car_brut(c, i);
                        i += 1;
                    }
                }
            }
            '@' => {
                if !dans_lien {
                    if let Some(depuis) = a.jeton_depuis {
                        let local = depuis < i
                            && s[depuis..i].iter().all(|c| {
                                c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-')
                            });
                        if let Some(fin) = domaine_de_courriel(s, i + 1).filter(|_| local) {
                            a.tampon.clear();
                            let apres_delim = match a.elements.last() {
                                Some(Element::Delim(k)) => Some(*k),
                                _ => None,
                            };
                            let texte: String = s[depuis..fin].iter().collect();
                            a.pousser(Element::Lien {
                                url: lien_ouvrable(&format!("mailto:{texte}")),
                                texte,
                                apres_delim,
                            });
                            i = fin;
                            continue;
                        }
                    }
                }
                match mention(s, i, resolveur, a.ouvrants_souligne > 0) {
                    Some((element, fin)) => {
                        a.pousser(element);
                        i = fin;
                    }
                    None => {
                        a.car_brut(c, i);
                        i += 1;
                    }
                }
            }
            ':' => match emoji(s, i, resolveur) {
                Some((element, fin)) => {
                    a.pousser(element);
                    i = fin;
                }
                None => {
                    a.car_brut(c, i);
                    i += 1;
                }
            },
            '*' | '_' | '~' => {
                let l = longueur_suite(s, i, c);
                if c == '~' && l > 2 {
                    for _ in 0..l {
                        a.car_converti(c);
                    }
                    i += l;
                    continue;
                }
                let (ouvre, ferme) = flanquements(s, i, l);
                a.vider();
                let element = a.elements.len();
                let indice = a.delims.len();
                a.elements.push(Element::Delim(indice));
                if c == '_' && ouvre {
                    a.ouvrants_souligne += 1;
                }
                a.delims.push(Delimiteur {
                    element,
                    car: c,
                    longueur: l,
                    restant: l,
                    ouvre,
                    ferme,
                    prec: None,
                    suiv: None,
                });
                i += l;
            }
            '\n' => {
                let garde = a.tampon.trim_end_matches([' ', '\t']).len();
                a.tampon.truncate(garde);
                a.pousser(Element::Saut);
                i += 1;
                while i < n && (s[i] == ' ' || s[i] == '\t') {
                    i += 1;
                }
            }
            '&' => match entite(s, i) {
                Some((texte, fin)) => {
                    a.texte_converti(&texte);
                    i = fin;
                }
                None => {
                    a.car_brut(c, i);
                    i += 1;
                }
            },
            _ => {
                a.car_brut(c, i);
                i += 1;
            }
        }
    }
    a.vider();
    let paires = traiter_emphases(&mut a.delims);
    aplatir(a.elements, &a.delims, &paires)
}

fn compatibles(o: &Delimiteur, c: &Delimiteur) -> bool {
    if o.car == '~' {
        return o.longueur == c.longueur;
    }
    !((o.ferme || c.ouvre)
        && (o.longueur + c.longueur).is_multiple_of(3)
        && !(o.longueur.is_multiple_of(3) && c.longueur.is_multiple_of(3)))
}

fn detacher(d: &mut [Delimiteur], k: usize) {
    let (p, s) = (d[k].prec, d[k].suiv);
    if let Some(p) = p {
        d[p].suiv = s;
    }
    if let Some(s) = s {
        d[s].prec = p;
    }
}

fn traiter_emphases(d: &mut [Delimiteur]) -> Vec<(usize, usize, Emphase)> {
    let n = d.len();
    for (k, delim) in d.iter_mut().enumerate() {
        delim.prec = k.checked_sub(1);
        delim.suiv = if k + 1 < n { Some(k + 1) } else { None };
    }
    let mut bas: HashMap<(char, bool, usize), isize> = HashMap::new();
    let mut paires = Vec::new();
    let mut courant = if n > 0 { Some(0) } else { None };
    while let Some(c) = courant {
        if !d[c].ferme {
            courant = d[c].suiv;
            continue;
        }
        let car = d[c].car;
        let cle = (
            car,
            d[c].ouvre,
            if car == '~' {
                d[c].longueur
            } else {
                d[c].longueur % 3
            },
        );
        let borne = bas.get(&cle).copied().unwrap_or(-1);
        let mut o = d[c].prec;
        let mut trouve = None;
        while let Some(oi) = o {
            if (oi as isize) <= borne {
                break;
            }
            if d[oi].car == car && d[oi].ouvre && compatibles(&d[oi], &d[c]) {
                trouve = Some(oi);
                break;
            }
            o = d[oi].prec;
        }
        match trouve {
            Some(oi) => {
                let nombre = if car == '~' {
                    d[c].restant
                } else if d[oi].restant >= 2 && d[c].restant >= 2 {
                    2
                } else {
                    1
                };
                let emphase = match (car, nombre) {
                    ('~', _) => Emphase::Barre,
                    (_, 2) => Emphase::Gras,
                    _ => Emphase::Italique,
                };
                paires.push((d[oi].element, d[c].element, emphase));
                d[oi].restant -= nombre;
                d[c].restant -= nombre;
                let mut k = d[oi].suiv;
                while let Some(x) = k {
                    if x == c {
                        break;
                    }
                    k = d[x].suiv;
                    detacher(d, x);
                }
                if d[oi].restant == 0 {
                    detacher(d, oi);
                }
                if d[c].restant == 0 {
                    courant = d[c].suiv;
                    detacher(d, c);
                } else {
                    courant = Some(c);
                }
            }
            None => {
                bas.insert(cle, d[c].prec.map_or(-1, |p| p as isize));
                courant = d[c].suiv;
                if !d[c].ouvre {
                    detacher(d, c);
                }
            }
        }
    }
    paires
}

fn pousser_segment(segments: &mut Vec<Segment>, segment: Segment) {
    if let (Some(dernier), Contenu::Texte(ajout)) = (segments.last_mut(), &segment.contenu) {
        if dernier.styles == segment.styles
            && dernier.lien == segment.lien
            && dernier.masque == segment.masque
        {
            if let Contenu::Texte(t) = &mut dernier.contenu {
                t.push_str(ajout);
                return;
            }
        }
    }
    segments.push(segment);
}

fn aplatir(
    elements: Vec<Element>,
    delims: &[Delimiteur],
    paires: &[(usize, usize, Emphase)],
) -> Vec<Segment> {
    let taille = elements.len() + 1;
    let mut ecarts = [vec![0i32; taille], vec![0i32; taille], vec![0i32; taille]];
    for &(o, c, emphase) in paires {
        let e = emphase as usize;
        ecarts[e][o + 1] += 1;
        ecarts[e][c] -= 1;
    }
    let mut cumuls = [0i32; 3];
    let mut segments = Vec::with_capacity(elements.len());
    for (indice, element) in elements.into_iter().enumerate() {
        for e in 0..3 {
            cumuls[e] += ecarts[e][indice];
        }
        let styles = Styles {
            gras: cumuls[Emphase::Gras as usize] > 0,
            italique: cumuls[Emphase::Italique as usize] > 0,
            barre: cumuls[Emphase::Barre as usize] > 0,
            code: false,
        };
        let simple = |contenu: Contenu, styles: Styles| Segment {
            contenu,
            styles,
            lien: None,
            masque: None,
        };
        match element {
            Element::Texte(t) => pousser_segment(&mut segments, simple(Contenu::Texte(t), styles)),
            Element::Delim(k) => {
                if delims[k].restant > 0 {
                    let t: String = std::iter::repeat_n(delims[k].car, delims[k].restant).collect();
                    pousser_segment(&mut segments, simple(Contenu::Texte(t), styles));
                }
            }
            Element::Saut => segments.push(simple(Contenu::SautDeLigne, styles)),
            Element::Code(t) => pousser_segment(
                &mut segments,
                simple(
                    Contenu::Texte(t),
                    Styles {
                        code: true,
                        ..styles
                    },
                ),
            ),
            Element::Mention { texte, genre } => {
                segments.push(simple(Contenu::Mention { texte, genre }, styles))
            }
            Element::Emoji(nom) => segments.push(simple(Contenu::Emoji(nom), styles)),
            Element::Lien {
                url,
                texte,
                apres_delim,
            } => pousser_segment(
                &mut segments,
                Segment {
                    contenu: Contenu::Texte(texte),
                    styles,
                    lien: url.filter(|_| {
                        apres_delim.is_none_or(|k| delims[k].restant < delims[k].longueur)
                    }),
                    masque: None,
                },
            ),
            Element::Conteneur {
                segments: enfants,
                lien,
                masque,
            } => {
                for mut enfant in enfants {
                    enfant.styles.gras |= styles.gras;
                    enfant.styles.italique |= styles.italique;
                    enfant.styles.barre |= styles.barre;
                    if enfant.lien.is_none() {
                        enfant.lien.clone_from(&lien);
                    }
                    if enfant.masque.is_none() {
                        enfant.masque = masque;
                    }
                    pousser_segment(&mut segments, enfant);
                }
            }
        }
    }
    segments
}
