use crate::appels::{resoudre_emoji, EmojiPersonnalise};
use crate::emoji::{
    code_en_cours, completer_code, emoji_par_code, lignes, rechercher, Remplacement,
};
use crate::mention::{
    mention_en_cours, remplacer, salon_en_cours, suggerer, MentionEnCours, SUGGESTIONS_PAR_DEFAUT,
};
use crate::store::{CleConversation, Etat, GenreSalon};

pub const SUGGESTIONS_EMOJI: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declencheur {
    Mention,
    Salon,
    Emoji,
}

impl Declencheur {
    pub fn titre(self) -> &'static str {
        match self {
            Declencheur::Mention => "Mentionner",
            Declencheur::Salon => "Salons",
            Declencheur::Emoji => "Smileys",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenreProposition {
    Personne,
    Groupe,
    Role,
    Salon,
    Emoji,
    EmojiServeur,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposition {
    pub genre: GenreProposition,
    pub libelle: String,
    pub detail: String,
    pub emoji: String,
    pub insertion: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Autocompletion {
    pub declencheur: Declencheur,
    pub debut: usize,
    pub propositions: Vec<Proposition>,
}

impl Autocompletion {
    pub fn appliquer(&self, texte: &str, curseur: usize, index: usize) -> Option<(String, usize)> {
        let p = self.propositions.get(index)?;
        remplacer(texte, self.debut, curseur, &p.insertion)
    }
}

fn detail_de_groupe(etat: &Etat, cle: &CleConversation, nom: &str) -> String {
    let libelle = if nom == "here" {
        "membres en ligne"
    } else {
        "tout le salon"
    };
    match etat.touches_par_groupe(Some(cle), nom) {
        Some(n) if nom == "here" => format!("{libelle} — {n} en ligne"),
        Some(n) => format!("{libelle} — {n} personnes"),
        None => libelle.to_string(),
    }
}

fn mentions(etat: &Etat, cle: &CleConversation, en_cours: &MentionEnCours) -> Vec<Proposition> {
    let mut noms = etat.pseudos_de(Some(cle));
    let personnes = noms.len();
    let groupes = etat.suggestions_de_groupe(Some(cle));
    noms.extend(groupes.iter().cloned());
    suggerer(
        &en_cours.requete,
        &noms,
        etat.pseudo(),
        SUGGESTIONS_PAR_DEFAUT,
    )
    .into_iter()
    .map(|nom| {
        let groupe = !noms[..personnes].contains(&nom);
        let (genre, detail) = match nom.as_str() {
            "everyone" | "here" if groupe => {
                (GenreProposition::Groupe, detail_de_groupe(etat, cle, &nom))
            }
            _ if groupe => (GenreProposition::Role, "rôle".to_string()),
            _ => (GenreProposition::Personne, String::new()),
        };
        Proposition {
            genre,
            libelle: format!("@{nom}"),
            detail,
            emoji: String::new(),
            insertion: format!("@{nom} "),
        }
    })
    .collect()
}

fn salons(etat: &Etat, cle: &CleConversation, en_cours: &MentionEnCours) -> Vec<Proposition> {
    let Some(serveur) = etat.serveur_de(cle) else {
        return Vec::new();
    };
    let noms: Vec<String> = etat
        .salons(&serveur)
        .iter()
        .filter(|s| s.kind == Some(GenreSalon::Text))
        .filter_map(|s| s.name.clone())
        .collect();
    suggerer(&en_cours.requete, &noms, "", SUGGESTIONS_PAR_DEFAUT)
        .into_iter()
        .map(|nom| Proposition {
            genre: GenreProposition::Salon,
            libelle: format!("#{nom}"),
            detail: String::new(),
            emoji: String::new(),
            insertion: format!("#{nom} "),
        })
        .collect()
}

fn smileys(emojis: &[EmojiPersonnalise], requete: &str) -> Vec<Proposition> {
    let q = requete.to_lowercase();
    let mut vus = Vec::new();
    let serveur = emojis
        .iter()
        .filter(|e| e.name.starts_with(&q))
        .filter(|e| {
            let nouveau = !vus.contains(&e.name);
            vus.push(e.name.clone());
            nouveau
        })
        .map(|e| Proposition {
            genre: GenreProposition::EmojiServeur,
            libelle: format!(":{}:", e.name),
            detail: String::new(),
            emoji: e.id.clone(),
            insertion: format!(":{}:", e.name),
        });
    let standard = rechercher(lignes(), requete, SUGGESTIONS_EMOJI)
        .into_iter()
        .map(|l| Proposition {
            genre: GenreProposition::Emoji,
            libelle: format!(":{}:", l.code),
            detail: String::new(),
            emoji: l.caractere.clone(),
            insertion: l.caractere.clone(),
        });
    serveur.chain(standard).take(SUGGESTIONS_EMOJI).collect()
}

pub fn autocompletion(
    etat: &Etat,
    cle: &CleConversation,
    emojis: &[EmojiPersonnalise],
    texte: &str,
    curseur: usize,
) -> Option<Autocompletion> {
    let (declencheur, debut, propositions) = if let Some(m) = mention_en_cours(texte, curseur) {
        (Declencheur::Mention, m.debut, mentions(etat, cle, &m))
    } else if let Some(s) = salon_en_cours(texte, curseur) {
        (Declencheur::Salon, s.debut, salons(etat, cle, &s))
    } else {
        let c = code_en_cours(texte, curseur)?;
        (Declencheur::Emoji, c.debut, smileys(emojis, &c.requete))
    };
    (!propositions.is_empty()).then_some(Autocompletion {
        declencheur,
        debut,
        propositions,
    })
}

pub fn convertir_code(
    emojis: &[EmojiPersonnalise],
    texte: &str,
    curseur: usize,
) -> Option<Remplacement> {
    completer_code(texte, curseur, |nom| {
        if resoudre_emoji(emojis, nom, None).is_some() {
            None
        } else {
            emoji_par_code(nom).map(str::to_string)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::time::{Duration, Instant};

    fn etat(membres: Vec<Value>, permissions: &[&str]) -> Etat {
        let mut etat = Etat {
            compte: serde_json::from_value(
                json!({"activated": true, "allowed": true, "id": "me", "pseudo": "moi"}),
            )
            .unwrap(),
            ..Etat::default()
        };
        etat.details.insert(
            "s1".into(),
            serde_json::from_value(json!({
                "server": {"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0},
                "channels": [
                    {"id": "c1", "name": "général", "kind": "text", "position": 1, "permissions": permissions},
                    {"id": "c2", "name": "jeux", "kind": "text", "position": 2},
                    {"id": "c3", "name": "génération", "kind": "text", "position": 3},
                    {"id": "v1", "name": "gêne", "kind": "voice", "position": 1}
                ],
                "members": membres,
                "roles": [
                    {"id": "r1", "name": "Modérateur", "position": 1, "is_default": false},
                    {"id": "r0", "name": "@tout le monde", "position": 0, "is_default": true}
                ]
            }))
            .unwrap(),
        );
        etat
    }

    fn membre(pseudo: &str, en_ligne: bool) -> Value {
        let id = if pseudo == "moi" { "me" } else { pseudo };
        json!({"id": id, "pseudo": pseudo, "online": en_ligne})
    }

    fn quatre() -> Etat {
        etat(
            vec![
                membre("moi", true),
                membre("Alice", true),
                membre("bob", false),
                membre("Malik", false),
            ],
            &["view_channel", "send_messages", "mention_everyone"],
        )
    }

    fn c1() -> CleConversation {
        CleConversation::Salon("c1".into())
    }

    fn libelles(a: &Option<Autocompletion>) -> Vec<String> {
        a.iter()
            .flat_map(|a| a.propositions.iter().map(|p| p.libelle.clone()))
            .collect()
    }

    fn emoji_perso(id: &str, nom: &str) -> EmojiPersonnalise {
        EmojiPersonnalise {
            id: id.into(),
            server_id: "s1".into(),
            name: nom.into(),
            created_by: "me".into(),
        }
    }

    #[test]
    fn arobase_propose_personnes_groupes_et_roles_dans_l_ordre_du_front() {
        let e = quatre();
        let a = autocompletion(&e, &c1(), &[], "salut @", 7);
        assert_eq!(
            libelles(&a),
            [
                "@Alice",
                "@bob",
                "@Malik",
                "@everyone",
                "@here",
                "@Modérateur"
            ]
        );
        let a = a.unwrap();
        assert_eq!(a.declencheur.titre(), "Mentionner");
        assert_eq!(a.propositions[0].genre, GenreProposition::Personne);
        assert_eq!(a.propositions[3].genre, GenreProposition::Groupe);
        assert_eq!(a.propositions[3].detail, "tout le salon — 3 personnes");
        assert_eq!(a.propositions[4].detail, "membres en ligne — 1 en ligne");
        assert_eq!(a.propositions[5].genre, GenreProposition::Role);
        assert_eq!(
            libelles(&autocompletion(&e, &c1(), &[], "@al", 3)),
            ["@Alice", "@Malik"]
        );
        assert_eq!(
            a.appliquer("salut @ ok", 7, 0),
            Some(("salut @Alice  ok".to_string(), 13))
        );
    }

    #[test]
    fn sans_le_droit_les_groupes_ne_sont_pas_proposes() {
        let e = etat(
            vec![membre("moi", true), membre("bob", true)],
            &["view_channel", "send_messages"],
        );
        assert_eq!(libelles(&autocompletion(&e, &c1(), &[], "@", 1)), ["@bob"]);
        assert_eq!(autocompletion(&e, &c1(), &[], "@zzz", 4), None);
        assert_eq!(autocompletion(&e, &c1(), &[], "mail@b", 6), None);
    }

    #[test]
    fn diese_propose_les_salons_texte_du_serveur() {
        let e = quatre();
        let a = autocompletion(&e, &c1(), &[], "va dans #gé", 12);
        assert_eq!(libelles(&a), ["#général", "#génération"]);
        let a = a.unwrap();
        assert_eq!(a.declencheur, Declencheur::Salon);
        assert_eq!(
            a.appliquer("va dans #gé", 12, 1),
            Some(("va dans #génération ".to_string(), 22))
        );
        assert_eq!(
            autocompletion(&e, &CleConversation::Prive("d1".into()), &[], "#", 1),
            None
        );
    }

    #[test]
    fn deux_points_propose_les_smileys_et_ceux_du_serveur() {
        let e = quatre();
        let a = autocompletion(&e, &c1(), &[], "bonjour :jo", 11).unwrap();
        assert_eq!(a.declencheur, Declencheur::Emoji);
        assert!(a.propositions.len() <= SUGGESTIONS_EMOJI);
        let joy = a
            .propositions
            .iter()
            .position(|p| p.libelle == ":joy:")
            .unwrap();
        assert_eq!(a.propositions[joy].insertion, "😂");
        assert_eq!(
            a.appliquer("bonjour :jo", 11, joy),
            Some(("bonjour 😂".to_string(), 12))
        );
        assert_eq!(autocompletion(&e, &c1(), &[], "bonjour :j", 10), None);
        let perso = [
            emoji_perso("e1", "joie_club"),
            emoji_perso("e2", "joie_club"),
        ];
        let a = autocompletion(&e, &c1(), &perso, ":jo", 3).unwrap();
        assert_eq!(a.propositions[0].genre, GenreProposition::EmojiServeur);
        assert_eq!(a.propositions[0].insertion, ":joie_club:");
        assert_eq!(a.propositions[1].genre, GenreProposition::Emoji);
    }

    #[test]
    fn la_conversion_laisse_les_emoji_de_serveur_en_code() {
        assert_eq!(
            convertir_code(&[], "super :joy:", 11).map(|r| r.texte),
            Some("super 😂".to_string())
        );
        assert_eq!(
            convertir_code(&[emoji_perso("e1", "joy")], "super :joy:", 11),
            None
        );
        assert_eq!(convertir_code(&[], "`code :joy:", 11), None);
    }

    #[test]
    fn cinq_mille_membres_en_moins_de_50_ms() {
        let mut membres: Vec<Value> = (0..5000)
            .map(|i| membre(&format!("membre{i:04}"), i % 3 == 0))
            .collect();
        membres.push(membre("moi", true));
        let e = etat(
            membres,
            &["view_channel", "send_messages", "mention_everyone"],
        );
        let mut pire = Duration::ZERO;
        for requete in ["@", "@m", "@membre49", "@4999", "@zzz"] {
            let debut = Instant::now();
            let a = autocompletion(&e, &c1(), &[], requete, requete.len());
            pire = pire.max(debut.elapsed());
            if requete == "@4999" {
                assert_eq!(libelles(&a), ["@membre4999"]);
            }
        }
        println!("315 C2 : autocomplétion sur 5 000 membres, pire cas {pire:?}");
        assert!(pire < Duration::from_millis(50), "{pire:?}");
    }
}
