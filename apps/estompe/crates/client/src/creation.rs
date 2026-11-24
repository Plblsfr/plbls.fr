use crate::appels::code_d_invitation;
use crate::http::ErreurApi;
use crate::store::{Etat, GenreSalon, Salon};
use crate::types::{PostServersIdChannelsRequete, PostServersIdChannelsRequeteKind};

pub const MAX_NOM_SALON: usize = 30;
pub const MAX_SUJET: usize = 200;
pub const MAX_NOM_SERVEUR: usize = 50;
pub const INVITATION_INVALIDE: &str = "Invitation invalide ou révoquée.";
pub const MODES_LENTS: [(&str, i64); 9] = [
    ("Désactivé", 0),
    ("5 secondes", 5),
    ("10 secondes", 10),
    ("30 secondes", 30),
    ("1 minute", 60),
    ("5 minutes", 300),
    ("15 minutes", 900),
    ("1 heure", 3600),
    ("6 heures", 21600),
];
pub const LIMITES: [i64; 9] = [0, 2, 3, 4, 5, 6, 7, 8, 9];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenrePropose {
    pub genre: GenreSalon,
    pub libelle: &'static str,
    pub indice: &'static str,
    pub icone: &'static str,
}

pub const GENRES: [GenrePropose; 5] = [
    GenrePropose {
        genre: GenreSalon::Text,
        libelle: "Texte",
        indice: "Discuter par messages",
        icone: "hash",
    },
    GenrePropose {
        genre: GenreSalon::Voice,
        libelle: "Vocal",
        indice: "Parler, partager son écran, regarder ensemble",
        icone: "volume",
    },
    GenrePropose {
        genre: GenreSalon::Videos,
        libelle: "Vidéos",
        indice: "Esquisses : partager des vidéos et organiser des séances",
        icone: "video",
    },
    GenrePropose {
        genre: GenreSalon::Music,
        libelle: "Musique",
        indice: "Esquisses : partager des morceaux, des playlists et écouter ensemble",
        icone: "headphones",
    },
    GenrePropose {
        genre: GenreSalon::Cinema,
        libelle: "Cinéma",
        indice: "Regarder un film ou une série d’Entracte ensemble, en vocal",
        icone: "film",
    },
];

pub fn cinema_actif(etat: &Etat) -> bool {
    etat.compte.cinema == Some(true)
}

pub fn genres_proposes(cinema: bool) -> Vec<GenrePropose> {
    GENRES
        .into_iter()
        .filter(|g| cinema || g.genre != GenreSalon::Cinema)
        .collect()
}

pub fn genre_de_depart(voulu: GenreSalon, cinema: bool) -> GenreSalon {
    match voulu {
        GenreSalon::Cinema if !cinema => GenreSalon::Voice,
        GenreSalon::Inconnu => GenreSalon::Text,
        autre => autre,
    }
}

pub fn est_vocal_ou_cinema(genre: GenreSalon) -> bool {
    matches!(genre, GenreSalon::Voice | GenreSalon::Cinema)
}

pub fn erreur_nom_salon(nom: &str, genre: GenreSalon, salons: &[Salon]) -> &'static str {
    let n = nom.trim();
    if n.is_empty() {
        return "Donne un nom au salon.";
    }
    if n.chars().count() > MAX_NOM_SALON {
        return "30 caractères maximum.";
    }
    if n.chars().any(char::is_control) {
        return "Ce nom contient un caractère non autorisé.";
    }
    let bas = n.to_lowercase();
    if salons.iter().any(|c| {
        c.kind == Some(genre) && c.name.as_deref().unwrap_or_default().to_lowercase() == bas
    }) {
        return "Un salon de ce type porte déjà ce nom.";
    }
    ""
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NouveauSalon {
    pub nom: String,
    pub genre: Option<GenreSalon>,
    pub sujet: String,
    pub mode_lent: i64,
    pub limite: i64,
    pub apres: Option<String>,
    pub prive: bool,
    pub roles: Vec<String>,
    pub membres: Vec<String>,
}

fn genre_requete(genre: GenreSalon) -> PostServersIdChannelsRequeteKind {
    match genre {
        GenreSalon::Voice => PostServersIdChannelsRequeteKind::Voice,
        GenreSalon::Videos => PostServersIdChannelsRequeteKind::Videos,
        GenreSalon::Music => PostServersIdChannelsRequeteKind::Music,
        GenreSalon::Cinema => PostServersIdChannelsRequeteKind::Cinema,
        GenreSalon::Text | GenreSalon::Inconnu => PostServersIdChannelsRequeteKind::Text,
    }
}

impl NouveauSalon {
    pub fn genre(&self) -> GenreSalon {
        self.genre.unwrap_or(GenreSalon::Text)
    }

    pub fn apres_valide(&self, salons: &[Salon]) -> Option<String> {
        let apres = self.apres.as_deref()?;
        salons
            .iter()
            .any(|c| c.id.as_deref() == Some(apres) && c.kind == Some(self.genre()))
            .then(|| apres.to_string())
    }

    pub fn requete(&self, salons: &[Salon]) -> PostServersIdChannelsRequete {
        let genre = self.genre();
        let sujet = self.sujet.trim();
        PostServersIdChannelsRequete {
            name: self.nom.trim().to_string(),
            kind: genre_requete(genre),
            topic: (!sujet.is_empty()).then(|| sujet.chars().take(MAX_SUJET).collect()),
            slowmode: (genre == GenreSalon::Text && self.mode_lent > 0).then_some(self.mode_lent),
            user_limit: (est_vocal_ou_cinema(genre) && self.limite > 0).then_some(self.limite),
            after: self.apres_valide(salons),
            private: self.prive.then_some(true),
            roles: self.prive.then(|| self.roles.clone()),
            members: self.prive.then(|| self.membres.clone()),
        }
    }
}

pub fn basculer(liste: &mut Vec<String>, id: &str) {
    match liste.iter().position(|x| x == id) {
        Some(i) => {
            liste.remove(i);
        }
        None => liste.push(id.to_string()),
    }
}

pub fn libelle_limite(limite: i64) -> String {
    if limite == 0 {
        "Aucune (10 au plus)".to_string()
    } else {
        format!("{limite} participants")
    }
}

pub fn nom_de_serveur(saisie: &str) -> String {
    saisie.chars().take(MAX_NOM_SERVEUR).collect()
}

pub fn code_saisi(saisie: &str) -> Option<String> {
    let code = code_d_invitation(saisie);
    (!code.is_empty()).then_some(code)
}

pub fn erreur_de_rejoindre(erreur: &ErreurApi) -> String {
    match erreur {
        ErreurApi::Introuvable(_) => INVITATION_INVALIDE.to_string(),
        autre => message_d_erreur(autre),
    }
}

pub fn message_d_erreur(erreur: &ErreurApi) -> String {
    match erreur {
        ErreurApi::Reseau(_) | ErreurApi::DelaiDepasse => {
            "Connexion au serveur impossible.".to_string()
        }
        ErreurApi::Serveur(_) | ErreurApi::ReponseInvalide(_) | ErreurApi::RequeteInvalide(_) => {
            "Une erreur est survenue".to_string()
        }
        autre if autre.message().is_empty() => "Une erreur est survenue".to_string(),
        autre => autre.message().to_string(),
    }
}

pub fn peut_creer_serveur(etat: &Etat) -> bool {
    etat.capacite("servers.create")
}

pub fn peut_rejoindre_serveur(etat: &Etat) -> bool {
    etat.capacite("servers.join")
}

pub fn peut_creer_salon(etat: &Etat, serveur: &str) -> bool {
    etat.details.contains_key(serveur) && etat.peut(Some(serveur), "manage_channels", None)
}

pub fn peut_quitter(etat: &Etat, serveur: &str) -> bool {
    etat.serveur(serveur)
        .is_some_and(|s| s.owner_id.as_deref() != Some(etat.moi()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn salons() -> Vec<Salon> {
        serde_json::from_value(json!([
            {"id": "c1", "name": "Général", "kind": "text", "position": 1},
            {"id": "c2", "name": "annonces", "kind": "text", "position": 2},
            {"id": "v1", "name": "Salon vocal", "kind": "voice", "position": 1}
        ]))
        .unwrap()
    }

    #[test]
    fn le_nom_d_un_salon_suit_les_regles_du_front() {
        let s = salons();
        assert_eq!(
            erreur_nom_salon("   ", GenreSalon::Text, &s),
            "Donne un nom au salon."
        );
        assert_eq!(
            erreur_nom_salon(&"é".repeat(31), GenreSalon::Text, &s),
            "30 caractères maximum."
        );
        assert_eq!(erreur_nom_salon(&"é".repeat(30), GenreSalon::Text, &s), "");
        assert_eq!(
            erreur_nom_salon("a\u{7}b", GenreSalon::Text, &s),
            "Ce nom contient un caractère non autorisé."
        );
        assert_eq!(
            erreur_nom_salon(" général ", GenreSalon::Text, &s),
            "Un salon de ce type porte déjà ce nom.",
            "sans casse et sans les espaces autour"
        );
        assert_eq!(
            erreur_nom_salon("Général", GenreSalon::Voice, &s),
            "",
            "le même nom est permis pour un autre type"
        );
        assert_eq!(erreur_nom_salon("discussions", GenreSalon::Text, &s), "");
    }

    #[test]
    fn la_requete_ne_porte_que_les_reglages_du_type() {
        let s = salons();
        let mut n = NouveauSalon {
            nom: "  jeux  ".into(),
            genre: Some(GenreSalon::Text),
            sujet: "  ".into(),
            mode_lent: 30,
            limite: 4,
            apres: Some("c1".into()),
            ..NouveauSalon::default()
        };
        let r = n.requete(&s);
        assert_eq!(r.name, "jeux");
        assert_eq!(r.kind, PostServersIdChannelsRequeteKind::Text);
        assert_eq!((r.topic, r.slowmode, r.user_limit), (None, Some(30), None));
        assert_eq!(r.after.as_deref(), Some("c1"));
        assert_eq!((r.private, r.roles, r.members), (None, None, None));
        n.genre = Some(GenreSalon::Voice);
        n.sujet = "On parle".into();
        n.prive = true;
        n.roles = vec!["r1".into()];
        let r = n.requete(&s);
        assert_eq!(
            (r.topic.as_deref(), r.slowmode, r.user_limit),
            (Some("On parle"), None, Some(4))
        );
        assert_eq!(r.after, None, "c1 n'est pas un salon vocal");
        assert_eq!(r.private, Some(true));
        assert_eq!(r.roles, Some(vec!["r1".to_string()]));
        assert_eq!(r.members, Some(Vec::new()));
        n.genre = Some(GenreSalon::Cinema);
        assert_eq!(n.requete(&s).kind, PostServersIdChannelsRequeteKind::Cinema);
        assert_eq!(n.requete(&s).user_limit, Some(4));
        let valeur = serde_json::to_value(
            NouveauSalon {
                nom: "x".into(),
                ..NouveauSalon::default()
            }
            .requete(&s),
        )
        .unwrap();
        assert_eq!(valeur, json!({"name": "x", "kind": "text"}));
    }

    #[test]
    fn le_cinema_n_est_propose_qu_avec_entracte() {
        let sans: Vec<GenreSalon> = genres_proposes(false).iter().map(|g| g.genre).collect();
        assert_eq!(
            sans,
            [
                GenreSalon::Text,
                GenreSalon::Voice,
                GenreSalon::Videos,
                GenreSalon::Music
            ]
        );
        assert_eq!(genres_proposes(true).len(), 5);
        assert_eq!(
            genre_de_depart(GenreSalon::Cinema, false),
            GenreSalon::Voice
        );
        assert_eq!(
            genre_de_depart(GenreSalon::Cinema, true),
            GenreSalon::Cinema
        );
    }

    #[test]
    fn choix_noms_et_codes() {
        let mut liste = vec!["a".to_string()];
        basculer(&mut liste, "b");
        basculer(&mut liste, "a");
        assert_eq!(liste, ["b"]);
        assert_eq!(libelle_limite(0), "Aucune (10 au plus)");
        assert_eq!(libelle_limite(5), "5 participants");
        assert_eq!(nom_de_serveur(&"x".repeat(60)).len(), MAX_NOM_SERVEUR);
        assert_eq!(
            code_saisi("https://chat.plbls.fr/#/join/abcde-23456").as_deref(),
            Some("ABCDE-23456")
        );
        assert_eq!(code_saisi("  "), None);
    }

    #[test]
    fn droits_de_creation_et_de_depart() {
        let mut etat = Etat {
            compte: serde_json::from_value(json!({"id": "u1", "pseudo": "bob", "capabilities": {
                "estompe:servers.create": {"allow": ["*"]},
                "estompe:servers.join": {"allow": ["*"], "deny": ["*"]}
            }}))
            .unwrap(),
            ..Etat::default()
        };
        etat.serveurs = serde_json::from_value(json!([
            {"id": "s1", "name": "À moi", "owner_id": "u1"},
            {"id": "s2", "name": "Autre", "owner_id": "eve"}
        ]))
        .unwrap();
        etat.details.insert(
            "s1".into(),
            serde_json::from_value(json!({"permissions": ["manage_channels"]})).unwrap(),
        );
        etat.details.insert(
            "s2".into(),
            serde_json::from_value(json!({"permissions": ["send_messages"]})).unwrap(),
        );
        assert!(peut_creer_serveur(&etat));
        assert!(!peut_rejoindre_serveur(&etat));
        assert!(peut_creer_salon(&etat, "s1"));
        assert!(!peut_creer_salon(&etat, "s2"));
        assert!(!peut_creer_salon(&etat, "s9"));
        assert!(!peut_quitter(&etat, "s1"), "le créateur ne peut pas partir");
        assert!(peut_quitter(&etat, "s2"));
    }

    #[test]
    fn les_erreurs_sont_lisibles() {
        let refus = |statut: u16| crate::http::Refus {
            statut,
            code: "x".into(),
            message: "Nom invalide".into(),
            donnees: Default::default(),
        };
        assert_eq!(
            erreur_de_rejoindre(&ErreurApi::Introuvable(refus(404))),
            INVITATION_INVALIDE
        );
        assert_eq!(
            erreur_de_rejoindre(&ErreurApi::Autre(refus(400))),
            "Nom invalide"
        );
        assert_eq!(
            message_d_erreur(&ErreurApi::Reseau("x".into())),
            "Connexion au serveur impossible."
        );
        assert_eq!(
            message_d_erreur(&ErreurApi::Serveur(refus(500))),
            "Une erreur est survenue"
        );
    }
}
