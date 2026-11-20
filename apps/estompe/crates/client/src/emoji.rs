use crate::types::Reaction;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const DONNEES: &str = include_str!("../../../../../web/estompe/src/blocks/emojis/emoji-data.json");

pub const EMOJIS: [&str; 48] = [
    "👍", "👎", "❤️", "😂", "😮", "😢", "🙏", "🔥", "🎉", "👏", "😍", "🤔", "😅", "😎", "🥳", "😭",
    "😡", "🤣", "😊", "😉", "🙂", "🙃", "😴", "🤯", "👀", "💯", "✅", "❌", "⭐", "💀", "🤝", "💪",
    "🙌", "👋", "🤷", "🫡", "😬", "🥲", "😇", "🤗", "🍕", "☕", "🍺", "🎮", "🎵", "🚀", "💡", "📌",
];

pub const REACTIONS_RAPIDES: [&str; 6] = ["👍", "❤️", "😂", "😮", "😢", "🎉"];

pub const MAX_RECENTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupeEmoji {
    pub icone: &'static str,
    pub libelle: &'static str,
}

pub const GROUPES: [GroupeEmoji; 9] = [
    GroupeEmoji {
        icone: "😀",
        libelle: "Smileys et émotions",
    },
    GroupeEmoji {
        icone: "👋",
        libelle: "Personnes",
    },
    GroupeEmoji {
        icone: "🐻",
        libelle: "Animaux et nature",
    },
    GroupeEmoji {
        icone: "🍔",
        libelle: "Nourriture",
    },
    GroupeEmoji {
        icone: "⚽",
        libelle: "Activités",
    },
    GroupeEmoji {
        icone: "🚗",
        libelle: "Voyages et lieux",
    },
    GroupeEmoji {
        icone: "💡",
        libelle: "Objets",
    },
    GroupeEmoji {
        icone: "❤️",
        libelle: "Symboles",
    },
    GroupeEmoji {
        icone: "🏳️",
        libelle: "Drapeaux",
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LigneEmoji {
    pub caractere: String,
    pub code: String,
    pub alias: Vec<String>,
    pub mots: String,
    pub nom: String,
    pub groupe: u8,
}

static LIGNES: OnceLock<Vec<LigneEmoji>> = OnceLock::new();
static PAR_CODE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

pub fn lignes() -> &'static [LigneEmoji] {
    LIGNES.get_or_init(|| serde_json::from_str(DONNEES).unwrap_or_default())
}

fn table_par_code() -> &'static HashMap<&'static str, &'static str> {
    PAR_CODE.get_or_init(|| {
        let mut table = HashMap::new();
        for ligne in lignes() {
            for code in std::iter::once(&ligne.code).chain(&ligne.alias) {
                table
                    .entry(code.as_str())
                    .or_insert(ligne.caractere.as_str());
            }
        }
        table
    })
}

pub fn emoji_par_code(code: &str) -> Option<&'static str> {
    table_par_code().get(code.to_lowercase().as_str()).copied()
}

pub fn titre_de_reaction(reaction: &Reaction, moi: &str, max: usize) -> String {
    let noms: Vec<String> = reaction
        .users
        .iter()
        .flatten()
        .map(|u| {
            if u.id.as_deref() == Some(moi) {
                "Toi".to_string()
            } else {
                u.pseudo.clone().unwrap_or_default()
            }
        })
        .collect();
    let mut montres: Vec<String> = noms.iter().take(max).cloned().collect();
    let reste = noms.len() - montres.len();
    if reste > 0 {
        montres.push(format!("{reste} autre{}", if reste > 1 { "s" } else { "" }));
    }
    let liste = match montres.split_last() {
        Some((dernier, debut)) if !debut.is_empty() => {
            format!("{} et {dernier}", debut.join(", "))
        }
        Some((seul, _)) => seul.clone(),
        None => String::new(),
    };
    let verbe = if noms.len() > 1 { "ont" } else { "a" };
    format!(
        "{liste} {verbe} réagi avec {}",
        reaction.emoji.as_deref().unwrap_or_default()
    )
}

fn sans_accent(c: char) -> Option<char> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        '\u{300}'..='\u{36f}' => return None,
        autre => autre,
    })
}

pub fn replier(texte: &str) -> String {
    texte
        .to_lowercase()
        .chars()
        .filter_map(sans_accent)
        .collect()
}

pub fn rechercher<'a>(donnees: &'a [LigneEmoji], requete: &str, max: usize) -> Vec<&'a LigneEmoji> {
    let sans_deux_points = requete.trim();
    let sans_deux_points = sans_deux_points
        .strip_prefix(':')
        .unwrap_or(sans_deux_points);
    let sans_deux_points = sans_deux_points
        .strip_suffix(':')
        .unwrap_or(sans_deux_points);
    let q = replier(sans_deux_points);
    if q.is_empty() {
        return Vec::new();
    }
    let mut notes: Vec<(u8, &LigneEmoji)> = Vec::new();
    for ligne in donnees {
        let codes = || std::iter::once(&ligne.code).chain(&ligne.alias);
        let note = if codes().any(|c| *c == q) {
            4
        } else if codes().any(|c| c.starts_with(&q)) {
            3
        } else if ligne.mots.split(' ').any(|m| m.starts_with(&q)) {
            2
        } else if codes().any(|c| c.contains(&q))
            || replier(&ligne.nom).contains(&q)
            || ligne.mots.contains(&q)
        {
            1
        } else {
            0
        };
        if note > 0 {
            notes.push((note, ligne));
        }
    }
    notes.sort_by_key(|(note, _)| std::cmp::Reverse(*note));
    notes.into_iter().take(max).map(|(_, l)| l).collect()
}

fn avant(texte: &str, position: usize) -> &str {
    let mut p = position.min(texte.len());
    while !texte.is_char_boundary(p) {
        p -= 1;
    }
    &texte[..p]
}

pub fn dans_code_ou_url(texte: &str, position: usize) -> bool {
    let avant = avant(texte, position);
    if avant.matches("```").count() % 2 == 1 {
        return true;
    }
    let ligne = avant.rsplit('\n').next().unwrap_or_default();
    if ligne.replace("```", "").matches('`').count() % 2 == 1 {
        return true;
    }
    avant
        .rsplit(|c: char| c.is_whitespace() || c == '\u{feff}')
        .next()
        .is_some_and(|dernier| dernier.contains("://"))
}

fn est_caractere_de_nom(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-')
}

fn queue_de_nom(texte: &str) -> &str {
    let debut = texte
        .char_indices()
        .rev()
        .take_while(|(_, c)| est_caractere_de_nom(*c))
        .last()
        .map_or(texte.len(), |(i, _)| i);
    &texte[debut..]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEnCours {
    pub debut: usize,
    pub requete: String,
}

pub fn code_en_cours(texte: &str, curseur: usize) -> Option<CodeEnCours> {
    let tete = avant(texte, curseur);
    let nom = queue_de_nom(tete);
    if nom.len() < 2 {
        return None;
    }
    let reste = tete[..tete.len() - nom.len()].strip_suffix(':')?;
    if reste
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == ':')
    {
        return None;
    }
    let debut = reste.len();
    if dans_code_ou_url(texte, debut) {
        return None;
    }
    Some(CodeEnCours {
        debut,
        requete: nom.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remplacement {
    pub texte: String,
    pub curseur: usize,
}

pub fn completer_code(
    texte: &str,
    curseur: usize,
    trouver: impl Fn(&str) -> Option<String>,
) -> Option<Remplacement> {
    let tete = avant(texte, curseur);
    let corps = tete.strip_suffix(':')?;
    let nom = queue_de_nom(corps);
    if nom.len() < 2 {
        return None;
    }
    let reste = corps[..corps.len() - nom.len()].strip_suffix(':')?;
    if reste
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return None;
    }
    let debut = reste.len();
    let emoji = if dans_code_ou_url(texte, debut) {
        None
    } else {
        trouver(nom)
    }?;
    Some(Remplacement {
        texte: format!("{}{}{}", &texte[..debut], emoji, &texte[tete.len()..]),
        curseur: debut + emoji.len(),
    })
}

pub fn charger_recents(json: &str) -> Vec<String> {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(valeurs)) => valeurs
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .take(MAX_RECENTS)
            .collect(),
        _ => Vec::new(),
    }
}

pub fn pousser_recent(recents: &[String], caractere: &str) -> Vec<String> {
    std::iter::once(caractere.to_string())
        .chain(recents.iter().filter(|r| *r != caractere).cloned())
        .take(MAX_RECENTS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Person;

    fn trouver(nom: &str) -> Option<String> {
        if nom == "perso" {
            None
        } else {
            emoji_par_code(nom).map(str::to_string)
        }
    }

    fn saisir(texte: &str) -> Option<Remplacement> {
        completer_code(texte, texte.len(), trouver)
    }

    #[test]
    fn remplace_un_code_complet_des_le_second_deux_points_et_place_le_curseur_apres() {
        let attendu = "Ajouter la conversion de 😂";
        assert_eq!(
            saisir("Ajouter la conversion de :joy:"),
            Some(Remplacement {
                texte: attendu.to_string(),
                curseur: attendu.len()
            })
        );
        assert_eq!(saisir(":thumbsup:").map(|r| r.texte).as_deref(), Some("👍"));
    }

    #[test]
    fn laisse_un_code_inconnu_ou_reserve_tel_quel() {
        assert_eq!(saisir("salut :zzzzzzz:"), None);
        assert_eq!(saisir("salut :perso:"), None);
    }

    #[test]
    fn ne_convertit_ni_dans_un_bloc_de_code_ni_dans_du_code_en_ligne_ni_dans_une_url_ni_colle_a_un_mot(
    ) {
        assert_eq!(saisir("```\nx :joy:"), None);
        assert_eq!(saisir("du `code :joy:"), None);
        assert_eq!(saisir("http://exemple.fr:joy:"), None);
        assert_eq!(saisir("à 12:30:45"), None);
        assert_eq!(saisir("a:joy:"), None);
        assert_eq!(
            saisir("```\ncode\n``` puis :joy:")
                .map(|r| r.texte)
                .as_deref(),
            Some("```\ncode\n``` puis 😂")
        );
    }

    #[test]
    fn la_conversion_garde_le_texte_apres_le_curseur_et_compte_en_octets() {
        let texte = "é :joy: fin";
        let curseur = "é :joy:".len();
        let r = completer_code(texte, curseur, trouver).unwrap();
        assert_eq!(r.texte, "é 😂 fin");
        assert_eq!(r.curseur, "é 😂".len());
        assert_eq!(completer_code("x :joy", 6, trouver), None);
        assert_eq!(completer_code(":a:", 3, trouver), None);
        assert_eq!(completer_code("", 0, trouver), None);
    }

    #[test]
    fn autocompletion_propose_a_partir_de_deux_points_suivi_de_deux_caracteres() {
        assert_eq!(
            code_en_cours("bonjour :jo", 11),
            Some(CodeEnCours {
                debut: 8,
                requete: "jo".to_string()
            })
        );
        assert_eq!(code_en_cours("bonjour :j", 10), None);
        assert_eq!(code_en_cours("http://exemple.fr:jo", 20), None);
        assert_eq!(code_en_cours("```\n:jo", 7), None);
        let codes: Vec<&str> = rechercher(lignes(), "jo", 200)
            .into_iter()
            .take(5)
            .map(|r| r.code.as_str())
            .collect();
        assert!(codes.contains(&"joy"));
    }

    #[test]
    fn autocompletion_cas_limites() {
        assert_eq!(
            code_en_cours(":jo", 3),
            Some(CodeEnCours {
                debut: 0,
                requete: "jo".to_string()
            })
        );
        assert_eq!(code_en_cours("a:jo", 4), None);
        assert_eq!(code_en_cours("::jo", 4), None);
        assert_eq!(code_en_cours("12:30", 5), None);
        assert_eq!(
            code_en_cours("é :jo", "é :jo".len()).map(|c| c.debut),
            Some(3)
        );
        assert_eq!(
            code_en_cours("x :jo bar", 5).map(|c| c.requete).as_deref(),
            Some("jo")
        );
        assert_eq!(code_en_cours("", 0), None);
        assert_eq!(
            code_en_cours(":thumbs_up-2+x", 14)
                .map(|c| c.requete)
                .as_deref(),
            Some("thumbs_up-2+x")
        );
    }

    #[test]
    fn recherche_par_code_nom_francais_et_mot_cle() {
        let premier = |q: &str| {
            rechercher(lignes(), q, 200)
                .first()
                .map(|r| r.caractere.as_str())
        };
        assert_eq!(premier(":joy:"), Some("😂"));
        assert_eq!(premier("joy"), Some("😂"));
        assert_eq!(premier("pouce vers le haut"), Some("👍"));
        let contient = |q: &str, e: &str| {
            rechercher(lignes(), q, 200)
                .iter()
                .any(|r| r.caractere == e)
        };
        assert!(contient("thumbs", "👍"));
        assert!(contient("fromage", "🧀"));
        assert!(rechercher(lignes(), "zzzzzzzzz", 200).is_empty());
    }

    #[test]
    fn la_recherche_ignore_casse_accents_et_espaces() {
        assert_eq!(
            rechercher(lignes(), "  VISAGE riant aux larmes ", 5)
                .first()
                .map(|r| r.caractere.as_str()),
            Some("😂")
        );
        assert!(!rechercher(lignes(), "émoticône", 5).is_empty());
        assert!(rechercher(lignes(), "", 5).is_empty());
        assert!(rechercher(lignes(), "  ", 5).is_empty());
        assert!(rechercher(lignes(), "::", 5).is_empty());
        assert_eq!(rechercher(lignes(), "visage", 3).len(), 3);
    }

    #[test]
    fn les_codes_exacts_passent_avant_les_prefixes() {
        let r = rechercher(lignes(), "smile", 200);
        assert_eq!(r[0].code, "smile");
        let notes: Vec<bool> = r.iter().map(|l| l.code == "smile").collect();
        assert!(notes[0] && !notes[1..].iter().any(|b| *b));
    }

    #[test]
    fn classe_les_neuf_categories_et_ne_laisse_aucun_smiley_hors_categorie() {
        let groupes: std::collections::BTreeSet<usize> =
            lignes().iter().map(|r| usize::from(r.groupe)).collect();
        let attendus: std::collections::BTreeSet<usize> = (0..GROUPES.len()).collect();
        assert_eq!(groupes, attendus);
        assert!(lignes().len() > 1800);
    }

    #[test]
    fn les_codes_en_double_gardent_le_premier_smiley() {
        assert_eq!(emoji_par_code("joy"), Some("😂"));
        assert_eq!(emoji_par_code("JOY"), Some("😂"));
        assert_eq!(emoji_par_code("lmao"), Some("😂"));
        assert_eq!(emoji_par_code("dog"), Some("🐶"));
        assert_eq!(emoji_par_code("point_up"), Some("👆"));
        assert_eq!(emoji_par_code("inconnu_zzz"), None);
    }

    #[test]
    fn place_le_dernier_utilise_en_tete_sans_doublon() {
        let r = pousser_recent(&[], "😂");
        let r = pousser_recent(&r, "👍");
        let r = pousser_recent(&r, "😂");
        assert_eq!(r, vec!["😂".to_string(), "👍".to_string()]);
    }

    #[test]
    fn les_recents_sont_bornes_et_le_stockage_illisible_est_ignore() {
        let mut r = Vec::new();
        for i in 0..40 {
            r = pousser_recent(&r, &format!("e{i}"));
        }
        assert_eq!(r.len(), MAX_RECENTS);
        assert_eq!(r[0], "e39");
        assert_eq!(charger_recents("[\"a\", 3, null, \"b\"]"), vec!["a", "b"]);
        assert_eq!(charger_recents("pas du json"), Vec::<String>::new());
        assert_eq!(charger_recents("{\"a\":1}"), Vec::<String>::new());
        assert_eq!(charger_recents(""), Vec::<String>::new());
        let long =
            serde_json::to_string(&(0..50).map(|i| i.to_string()).collect::<Vec<_>>()).unwrap();
        assert_eq!(charger_recents(&long).len(), MAX_RECENTS);
    }

    fn personne(id: &str) -> Person {
        Person {
            id: Some(id.to_string()),
            pseudo: Some(id.to_string()),
        }
    }

    fn reaction(emoji: &str, ids: &[&str]) -> Reaction {
        Reaction {
            emoji: Some(emoji.to_string()),
            users: Some(ids.iter().map(|i| personne(i)).collect()),
            ..Reaction::default()
        }
    }

    #[test]
    fn titre_de_reaction_liste_les_personnes_et_regroupe_au_dela_du_maximum() {
        assert_eq!(
            titre_de_reaction(&reaction("😂", &["bob"]), "me", 3),
            "bob a réagi avec 😂"
        );
        assert_eq!(
            titre_de_reaction(&reaction("😂", &["bob", "me"]), "me", 3),
            "bob et Toi ont réagi avec 😂"
        );
        assert_eq!(
            titre_de_reaction(&reaction("👍", &["a", "b", "c", "d", "e"]), "me", 3),
            "a, b, c et 2 autres ont réagi avec 👍"
        );
    }

    #[test]
    fn titre_de_reaction_cas_limites() {
        assert_eq!(
            titre_de_reaction(&reaction("👍", &["a", "b", "c", "d"]), "me", 3),
            "a, b, c et 1 autre ont réagi avec 👍"
        );
        assert_eq!(
            titre_de_reaction(&reaction("👍", &["me"]), "me", 3),
            "Toi a réagi avec 👍"
        );
        assert_eq!(
            titre_de_reaction(&Reaction::default(), "me", 3),
            " a réagi avec "
        );
    }

    #[test]
    fn les_listes_de_reference_sont_celles_du_front() {
        assert_eq!(EMOJIS.len(), 48);
        assert_eq!(EMOJIS[0], "👍");
        assert_eq!(EMOJIS[47], "📌");
        assert_eq!(REACTIONS_RAPIDES, ["👍", "❤️", "😂", "😮", "😢", "🎉"]);
        assert_eq!(GROUPES[8].libelle, "Drapeaux");
    }

    #[test]
    fn replier_retire_les_accents_courants() {
        assert_eq!(replier("Éléphant ÀÇÙÑ œuf"), "elephant acun œuf");
        assert_eq!(replier("e\u{301}te\u{301}"), "ete");
    }
}
