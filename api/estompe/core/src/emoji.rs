use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use regex::Regex;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

fn codes() -> &'static HashMap<String, String> {
    static CODES: OnceLock<HashMap<String, String>> = OnceLock::new();
    CODES.get_or_init(|| {
        serde_json::from_str(include_str!("emoji/codes.json")).expect("codes.json invalide")
    })
}

fn motif() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            "(?s)```.*?```|`[^`\n]*`|[^\t\n\x0C\r ]*://[^\t\n\x0C\r ]*|:([A-Za-z0-9_+-]{2,}):",
        )
        .expect("motif invalide")
    })
}

pub fn convertir(texte: &str, ignorer: &HashSet<String>) -> String {
    if !texte.contains(':') {
        return texte.to_string();
    }
    let octets = texte.as_bytes();
    let mut sortie = String::with_capacity(texte.len());
    let mut dernier = 0;
    for c in motif().captures_iter(texte) {
        let Some(nom) = c.get(1) else { continue };
        let tout = c.get(0).expect("groupe 0");
        let nom = nom.as_str().to_ascii_lowercase();
        let Some(smiley) = codes().get(&nom) else {
            continue;
        };
        if ignorer.contains(&nom) || (tout.start() > 0 && est_mot(octets[tout.start() - 1])) {
            continue;
        }
        sortie.push_str(&texte[dernier..tout.start()]);
        sortie.push_str(smiley);
        dernier = tout.end();
    }
    sortie.push_str(&texte[dernier..]);
    sortie
}

fn est_mot(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

pub fn valide(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 || s.chars().count() > 12 {
        return false;
    }
    let touche = s.contains('\u{20E3}');
    s.chars().all(|r| match r {
        '\u{200D}' | '\u{FE0F}' | '\u{20E3}' => true,
        '#' | '*' | '0'..='9' if touche => true,
        '\u{1F1E6}'..='\u{1F1FF}' | '\u{E0020}'..='\u{E007F}' => true,
        r => matches!(
            r.general_category(),
            GeneralCategory::MathSymbol
                | GeneralCategory::CurrencySymbol
                | GeneralCategory::ModifierSymbol
                | GeneralCategory::OtherSymbol
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_des_codes() {
        let perso: HashSet<String> = ["thumbsup".to_string()].into();
        for (entree, attendu) in [
            ("Ajouter :joy: en smiley", "Ajouter 😂 en smiley"),
            (":joy::JOY:", "😂😂"),
            ("code inconnu :zzzzzz:", "code inconnu :zzzzzz:"),
            ("une `:joy:` en ligne :joy:", "une `:joy:` en ligne 😂"),
            ("```\n:joy:\n``` puis :joy:", "```\n:joy:\n``` puis 😂"),
            (
                "http://exemple.fr:joy:/x :joy:",
                "http://exemple.fr:joy:/x 😂",
            ),
            ("à 12:30:45 :joy:", "à 12:30:45 😂"),
            (":thumbsup: perso", ":thumbsup: perso"),
            ("pas de code", "pas de code"),
        ] {
            assert_eq!(convertir(entree, &perso), attendu, "{entree}");
        }
    }

    #[test]
    fn emojis_valides() {
        for bon in [
            "👍",
            "❤️",
            "👨‍👩‍👧",
            "🇫🇷",
            "1️⃣",
            "#️⃣",
            "*️⃣",
            "😂",
            "🏴󠁧󠁢󠁥󠁮󠁧󠁿",
            "+",
            "$",
        ] {
            assert!(valide(bon), "{bon}");
        }
        for mauvais in [
            "",
            "a",
            "1",
            "#",
            "*",
            "ab😂",
            "😂😂😂😂😂😂😂😂😂😂😂😂😂",
            &"x".repeat(65),
        ] {
            assert!(!valide(mauvais), "{mauvais}");
        }
    }
}
