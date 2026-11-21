use std::collections::HashMap;

pub const SEUIL_CONFIRMATION: usize = 10;
pub const LONGUEUR_REQUETE_MAX: usize = 20;
pub const SUGGESTIONS_PAR_DEFAUT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenreMention {
    Personne,
    Role,
    Groupe,
}

pub trait ContexteMentions {
    fn genre(&self, nom_minuscule: &str) -> Option<GenreMention>;
    fn noms_composes(&self) -> Vec<String>;
    fn emoji_connu(&self, _nom_minuscule: &str) -> bool {
        false
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mentionnables {
    noms: HashMap<String, GenreMention>,
    emojis: Vec<String>,
}

impl Mentionnables {
    pub fn nouveau<P, R, G, E>(personnes: P, roles: R, groupes: G, emojis: E) -> Self
    where
        P: IntoIterator,
        P::Item: AsRef<str>,
        R: IntoIterator,
        R::Item: AsRef<str>,
        G: IntoIterator,
        G::Item: AsRef<str>,
        E: IntoIterator,
        E::Item: AsRef<str>,
    {
        let mut noms = HashMap::new();
        for (liste, genre) in [
            (
                groupes
                    .into_iter()
                    .map(|n| n.as_ref().to_lowercase())
                    .collect::<Vec<_>>(),
                GenreMention::Groupe,
            ),
            (
                roles
                    .into_iter()
                    .map(|n| n.as_ref().to_lowercase())
                    .collect(),
                GenreMention::Role,
            ),
            (
                personnes
                    .into_iter()
                    .map(|n| n.as_ref().to_lowercase())
                    .collect(),
                GenreMention::Personne,
            ),
        ] {
            for nom in liste {
                noms.insert(nom, genre);
            }
        }
        let mut emojis: Vec<String> = emojis.into_iter().map(|e| e.as_ref().to_string()).collect();
        emojis.sort();
        emojis.dedup();
        Mentionnables { noms, emojis }
    }

    pub fn personnes<P>(personnes: P) -> Self
    where
        P: IntoIterator,
        P::Item: AsRef<str>,
    {
        Self::nouveau(
            personnes,
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
        )
    }
}

impl ContexteMentions for Mentionnables {
    fn genre(&self, nom_minuscule: &str) -> Option<GenreMention> {
        self.noms.get(nom_minuscule).copied()
    }

    fn noms_composes(&self) -> Vec<String> {
        self.noms
            .keys()
            .filter(|n| !est_pseudo_simple(n))
            .cloned()
            .collect()
    }

    fn emoji_connu(&self, nom_minuscule: &str) -> bool {
        self.emojis
            .binary_search_by(|e| e.as_str().cmp(nom_minuscule))
            .is_ok()
    }
}

pub struct ContexteFermetures<F, C, E>
where
    F: Fn(&str) -> Option<GenreMention>,
    C: Fn() -> Vec<String>,
    E: Fn(&str) -> bool,
{
    pub genre: F,
    pub noms_composes: C,
    pub emoji_connu: E,
}

impl<F, C, E> ContexteMentions for ContexteFermetures<F, C, E>
where
    F: Fn(&str) -> Option<GenreMention>,
    C: Fn() -> Vec<String>,
    E: Fn(&str) -> bool,
{
    fn genre(&self, nom_minuscule: &str) -> Option<GenreMention> {
        (self.genre)(nom_minuscule)
    }

    fn noms_composes(&self) -> Vec<String> {
        (self.noms_composes)()
    }

    fn emoji_connu(&self, nom_minuscule: &str) -> bool {
        (self.emoji_connu)(nom_minuscule)
    }
}

pub(crate) fn car_de_pseudo(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

pub(crate) fn est_pseudo_simple(nom: &str) -> bool {
    let longueur = nom.chars().count();
    (3..=20).contains(&longueur)
        && nom
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionEnCours {
    pub debut: usize,
    pub requete: String,
}

fn car_de_salon(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '-')
}

fn jeton_en_cours(
    texte: &str,
    curseur: usize,
    prefixe: char,
    car: fn(char) -> bool,
) -> Option<MentionEnCours> {
    let avant = texte.get(..curseur)?;
    let requete_debut = avant
        .char_indices()
        .rev()
        .take_while(|(_, c)| car(*c))
        .last()
        .map_or(avant.len(), |(i, _)| i);
    let requete = &avant[requete_debut..];
    if requete.chars().count() > LONGUEUR_REQUETE_MAX {
        return None;
    }
    let tete = avant[..requete_debut].strip_suffix(prefixe)?;
    if tete.chars().next_back().is_some_and(car) {
        return None;
    }
    Some(MentionEnCours {
        debut: tete.len(),
        requete: requete.to_string(),
    })
}

pub fn mention_en_cours(texte: &str, curseur: usize) -> Option<MentionEnCours> {
    jeton_en_cours(texte, curseur, '@', car_de_pseudo)
}

pub fn salon_en_cours(texte: &str, curseur: usize) -> Option<MentionEnCours> {
    jeton_en_cours(texte, curseur, '#', car_de_salon)
}

pub fn remplacer(
    texte: &str,
    debut: usize,
    curseur: usize,
    insere: &str,
) -> Option<(String, usize)> {
    let avant = texte.get(..debut)?;
    let apres = texte.get(curseur..)?;
    Some((format!("{avant}{insere}{apres}"), debut + insere.len()))
}

pub fn inserer_mention(
    texte: &str,
    curseur: usize,
    debut: usize,
    pseudo: &str,
) -> Option<(String, usize)> {
    remplacer(texte, debut, curseur, &format!("@{pseudo} "))
}

pub fn suggerer(requete: &str, pseudos: &[String], moi: &str, max: usize) -> Vec<String> {
    let q = requete.to_lowercase();
    let liste: Vec<&String> = pseudos.iter().filter(|p| p.as_str() != moi).collect();
    let debuts = liste.iter().filter(|p| p.to_lowercase().starts_with(&q));
    let contenus = liste.iter().filter(|p| {
        let bas = p.to_lowercase();
        !bas.starts_with(&q) && bas.contains(&q)
    });
    debuts
        .chain(contenus)
        .take(max)
        .map(|p| (*p).clone())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupeCite {
    Tous,
    EnLigne,
}

impl GroupeCite {
    pub fn nom(self) -> &'static str {
        match self {
            GroupeCite::Tous => "everyone",
            GroupeCite::EnLigne => "here",
        }
    }
}

fn sans_code(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut reste = texte;
    while let Some(i) = reste.find('`') {
        sortie.push_str(&reste[..i]);
        let suite = &reste[i..];
        if let Some(apres) = suite.strip_prefix("```") {
            sortie.push(' ');
            reste = apres.find("```").map_or("", |f| &apres[f + 3..]);
            continue;
        }
        let corps = &suite[1..];
        match corps.find(['`', '\n']) {
            Some(f) if corps.as_bytes()[f] == b'`' => {
                sortie.push(' ');
                reste = &corps[f + 1..];
            }
            _ => {
                sortie.push('`');
                reste = corps;
            }
        }
    }
    sortie.push_str(reste);
    sortie
}

pub fn groupes_cites(texte: &str) -> Vec<GroupeCite> {
    let sans = sans_code(texte);
    let mut trouves = Vec::new();
    let mut precedent: Option<char> = None;
    for (i, c) in sans.char_indices() {
        if c == '@' && !precedent.is_some_and(car_de_pseudo) {
            let suite = &sans[i + 1..];
            for (mot, groupe) in [
                ("everyone", GroupeCite::Tous),
                ("here", GroupeCite::EnLigne),
            ] {
                let Some(tete) = suite.get(..mot.len()) else {
                    continue;
                };
                if !tete.eq_ignore_ascii_case(mot) {
                    continue;
                }
                let apres = suite[mot.len()..].chars().next();
                if apres.is_some_and(|a| a.is_alphanumeric() || a == '_') {
                    continue;
                }
                if !trouves.contains(&groupe) {
                    trouves.push(groupe);
                }
            }
        }
        precedent = Some(c);
    }
    trouves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en_cours(debut: usize, requete: &str) -> Option<MentionEnCours> {
        Some(MentionEnCours {
            debut,
            requete: requete.to_string(),
        })
    }

    #[test]
    fn detecte_une_mention_en_cours() {
        assert_eq!(mention_en_cours("salut @al", 9), en_cours(6, "al"));
        assert_eq!(mention_en_cours("@", 1), en_cours(0, ""));
        assert_eq!(mention_en_cours("a\n@bo", 5), en_cours(2, "bo"));
    }

    #[test]
    fn ignore_les_adresses_e_mail_et_les_mentions_deja_terminees() {
        assert_eq!(mention_en_cours("mail@alice", 10), None);
        assert_eq!(mention_en_cours("@alice ", 7), None);
        assert_eq!(mention_en_cours("rien", 4), None);
    }

    #[test]
    fn regarde_seulement_avant_le_curseur() {
        assert_eq!(mention_en_cours("@al suite", 3), en_cours(0, "al"));
    }

    #[test]
    fn mention_en_cours_borne_la_requete_et_le_curseur() {
        assert_eq!(
            mention_en_cours(&format!("@{}", "a".repeat(20)), 21),
            en_cours(0, &"a".repeat(20))
        );
        assert_eq!(mention_en_cours(&format!("@{}", "a".repeat(21)), 22), None);
        assert_eq!(mention_en_cours("é@al", 1), None);
        assert_eq!(mention_en_cours("@al", 99), None);
        assert_eq!(mention_en_cours("é @al", 6), en_cours(3, "al"));
    }

    #[test]
    fn detecte_un_salon_en_cours_avec_accents() {
        assert_eq!(salon_en_cours("va dans #gé", 12), en_cours(8, "gé"));
        assert_eq!(salon_en_cours("#", 1), en_cours(0, ""));
        assert_eq!(salon_en_cours("abc#x", 5), None);
        assert_eq!(salon_en_cours("# titre", 7), None);
        assert_eq!(salon_en_cours("@al", 3), None);
        assert_eq!(mention_en_cours("#al", 3), None);
    }

    #[test]
    fn remplace_la_saisie_et_place_le_curseur_apres_l_espace() {
        assert_eq!(
            inserer_mention("salut @al ok", 9, 6, "alice"),
            Some(("salut @alice  ok".to_string(), 13))
        );
        assert_eq!(inserer_mention("é", 1, 0, "x"), None);
    }

    fn tous() -> Vec<String> {
        ["Alice", "bob", "Malik", "moi"].map(String::from).to_vec()
    }

    #[test]
    fn classe_les_debuts_avant_les_contenus_et_exclut_soi_meme() {
        assert_eq!(
            suggerer("al", &tous(), "moi", SUGGESTIONS_PAR_DEFAUT),
            ["Alice", "Malik"]
        );
        assert_eq!(
            suggerer("", &tous(), "moi", SUGGESTIONS_PAR_DEFAUT),
            ["Alice", "bob", "Malik"]
        );
        assert!(suggerer("zzz", &tous(), "moi", SUGGESTIONS_PAR_DEFAUT).is_empty());
    }

    #[test]
    fn limite_le_nombre_de_propositions() {
        let pseudos: Vec<String> = (0..20).map(|i| format!("user{i}")).collect();
        assert_eq!(suggerer("", &pseudos, "moi", 3).len(), 3);
    }

    #[test]
    fn trouve_everyone_et_here_hors_code() {
        assert_eq!(
            groupes_cites("@here et @Everyone"),
            [GroupeCite::EnLigne, GroupeCite::Tous]
        );
        assert!(groupes_cites("`@everyone` et ```\n@here\n```").is_empty());
        assert!(groupes_cites("mail@here.fr @heres").is_empty());
    }

    #[test]
    fn groupes_cites_cas_limites() {
        assert!(groupes_cites("```\n@here").is_empty());
        assert_eq!(groupes_cites("` @here"), [GroupeCite::EnLigne]);
        assert_eq!(groupes_cites("`\n` @here @here"), [GroupeCite::EnLigne]);
        assert_eq!(groupes_cites("(@everyone)"), [GroupeCite::Tous]);
        assert!(groupes_cites("@hereé").is_empty());
        assert_eq!(GroupeCite::Tous.nom(), "everyone");
        assert_eq!(GroupeCite::EnLigne.nom(), "here");
    }

    #[test]
    fn le_contexte_simple_resout_les_genres_et_les_noms_composes() {
        let c = Mentionnables::nouveau(
            ["Alice"],
            ["Modérateur", "Chef de projet", "mods"],
            ["everyone"],
            ["smile"],
        );
        assert_eq!(c.genre("alice"), Some(GenreMention::Personne));
        assert_eq!(c.genre("modérateur"), Some(GenreMention::Role));
        assert_eq!(c.genre("everyone"), Some(GenreMention::Groupe));
        assert_eq!(c.genre("Alice"), None);
        let mut composes = c.noms_composes();
        composes.sort();
        assert_eq!(composes, ["chef de projet", "modérateur"]);
        assert!(c.emoji_connu("smile"));
        assert!(!c.emoji_connu("smil"));
    }

    #[test]
    fn le_contexte_par_fermetures_delegue() {
        let c = ContexteFermetures {
            genre: |n: &str| (n == "bob").then_some(GenreMention::Personne),
            noms_composes: Vec::new,
            emoji_connu: |n: &str| n == "ok",
        };
        assert_eq!(c.genre("bob"), Some(GenreMention::Personne));
        assert!(c.noms_composes().is_empty());
        assert!(c.emoji_connu("ok"));
    }
}
