use crate::creation::erreur_nom_salon;
use crate::format::date_heure_complete;
use crate::roles::{domine, membre_exclu};
use crate::store::{id_de, Etat};
use crate::types::{GetServersIdAuditLogReponseElement, GetServersIdAuditLogReponseElementAction};

pub const MAX_RAISON: usize = 200;
pub const MAX_PSEUDO: usize = 32;
pub const MAX_EXCLUSION: i64 = 28 * 86_400;
pub const MAX_BAN: i64 = 10 * 365 * 86_400;
pub const PERSONNALISE: i64 = -1;
pub const DUREES_BAN: [(&str, i64); 5] = [
    ("Définitif", 0),
    ("1 heure", 3600),
    ("24 heures", 86_400),
    ("7 jours", 604_800),
    ("Personnalisé…", PERSONNALISE),
];
pub const DUREES_EXCLUSION: [(&str, i64); 7] = [
    ("60 secondes", 60),
    ("5 minutes", 300),
    ("10 minutes", 600),
    ("1 heure", 3600),
    ("1 jour", 86_400),
    ("1 semaine", 604_800),
    ("Personnalisé…", PERSONNALISE),
];
pub const PURGES: [(&str, i64); 4] = [
    ("Ne rien supprimer", 0),
    ("Dernière heure", 3600),
    ("Dernières 24 heures", 86_400),
    ("7 derniers jours", 604_800),
];
pub const FILTRES_JOURNAL: [(&str, &str); 7] = [
    ("", "Toutes les actions"),
    ("member", "Modération des membres"),
    ("role", "Rôles"),
    ("channel", "Salons"),
    ("voice", "Vocal"),
    ("message", "Messages"),
    ("server", "Serveur"),
];
pub const MAX_SUPPRESSION: i64 = 100;
pub const JOURNAL_VIDE: &str = "Aucune action enregistrée.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreModeration {
    Expulser,
    Bannir,
    Exclure,
    Avertir,
    Pseudo,
}

impl GenreModeration {
    pub fn id(self) -> &'static str {
        match self {
            Self::Expulser => "kick",
            Self::Bannir => "ban",
            Self::Exclure => "timeout",
            Self::Avertir => "warn",
            Self::Pseudo => "nickname",
        }
    }

    pub fn depuis_id(id: &str) -> Option<Self> {
        [
            Self::Expulser,
            Self::Bannir,
            Self::Exclure,
            Self::Avertir,
            Self::Pseudo,
        ]
        .into_iter()
        .find(|g| g.id() == id)
    }

    pub fn titre(self, pseudo: &str) -> String {
        let verbe = match self {
            Self::Expulser => "Expulser",
            Self::Bannir => "Bannir",
            Self::Exclure => "Exclure temporairement",
            Self::Avertir => "Avertir",
            Self::Pseudo => "Changer le pseudo de",
        };
        format!("{verbe} {pseudo}")
    }

    pub fn fait(self) -> &'static str {
        match self {
            Self::Expulser => "Membre expulsé",
            Self::Bannir => "Membre banni",
            Self::Exclure => "Membre exclu temporairement",
            Self::Avertir => "Avertissement envoyé",
            Self::Pseudo => "Pseudo modifié",
        }
    }

    pub fn explication(self, pseudo: &str) -> String {
        match self {
            Self::Bannir => format!("{pseudo} sera retiré du serveur et ne pourra plus le rejoindre, même avec une invitation, jusqu'à la fin du bannissement."),
            Self::Expulser => format!("{pseudo} sera retiré du serveur et du vocal ; il pourra revenir avec une nouvelle invitation."),
            Self::Exclure => format!("{pseudo} reste membre et peut lire, mais ne peut plus écrire, réagir, parler ni rejoindre un vocal. Il est déconnecté du vocal."),
            Self::Avertir => format!("{pseudo} recevra un avertissement avec la raison indiquée."),
            Self::Pseudo => "Le pseudo ne change que sur ce serveur.".to_string(),
        }
    }

    pub fn destructif(self) -> bool {
        matches!(self, Self::Expulser | Self::Bannir)
    }

    pub fn bouton(self, pseudo: &str) -> String {
        if self.destructif() {
            self.titre(pseudo)
        } else {
            "Confirmer".to_string()
        }
    }

    pub fn durees(self) -> &'static [(&'static str, i64)] {
        match self {
            Self::Bannir => &DUREES_BAN,
            Self::Exclure => &DUREES_EXCLUSION,
            _ => &[],
        }
    }

    pub fn duree_de_depart(self) -> i64 {
        if self == Self::Bannir {
            0
        } else {
            600
        }
    }

    pub fn heures_max(self) -> f64 {
        if self == Self::Bannir {
            87_600.0
        } else {
            672.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionModeration {
    Moderer(GenreModeration),
    LeverExclusion,
}

impl ActionModeration {
    pub fn id(self) -> &'static str {
        match self {
            Self::Moderer(g) => g.id(),
            Self::LeverExclusion => "untimeout",
        }
    }

    pub fn depuis_id(id: &str) -> Option<Self> {
        if id == "untimeout" {
            return Some(Self::LeverExclusion);
        }
        GenreModeration::depuis_id(id).map(Self::Moderer)
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::LeverExclusion => "Lever l'exclusion temporaire",
            Self::Moderer(GenreModeration::Exclure) => "Exclure temporairement…",
            Self::Moderer(GenreModeration::Avertir) => "Avertir…",
            Self::Moderer(GenreModeration::Pseudo) => "Changer le pseudo…",
            Self::Moderer(GenreModeration::Expulser) => "Expulser…",
            Self::Moderer(GenreModeration::Bannir) => "Bannir…",
        }
    }

    pub fn icone(self) -> &'static str {
        match self {
            Self::LeverExclusion | Self::Moderer(GenreModeration::Exclure) => "horloge",
            Self::Moderer(GenreModeration::Avertir) => "alerte",
            Self::Moderer(GenreModeration::Pseudo) => "crayon",
            Self::Moderer(GenreModeration::Expulser) => "sortie",
            Self::Moderer(GenreModeration::Bannir) => "interdit",
        }
    }

    pub fn danger(self) -> bool {
        matches!(self, Self::Moderer(g) if g.destructif())
    }
}

pub fn actions_de_moderation(
    etat: &Etat,
    serveur: &str,
    cible: &str,
    maintenant_ms: i64,
) -> Vec<ActionModeration> {
    let detail = etat.details.get(serveur);
    if !domine(detail, etat.moi(), cible) {
        return Vec::new();
    }
    let peut = |p: &str| etat.peut(Some(serveur), p, None);
    let membre = etat.membre(serveur, cible);
    let mut actions = Vec::new();
    if peut("timeout_members") {
        actions.push(if membre_exclu(membre, maintenant_ms) {
            ActionModeration::LeverExclusion
        } else {
            ActionModeration::Moderer(GenreModeration::Exclure)
        });
        actions.push(ActionModeration::Moderer(GenreModeration::Avertir));
    }
    if peut("manage_nicknames") {
        actions.push(ActionModeration::Moderer(GenreModeration::Pseudo));
    }
    if peut("kick_members") {
        actions.push(ActionModeration::Moderer(GenreModeration::Expulser));
    }
    if peut("ban_members") {
        actions.push(ActionModeration::Moderer(GenreModeration::Bannir));
    }
    actions
}

pub fn pseudo_de_serveur(etat: &Etat, serveur: &str, membre: &str) -> String {
    etat.membre(serveur, membre)
        .and_then(|m| m.nickname.clone())
        .unwrap_or_default()
}

pub fn nom_de_la_cible(etat: &Etat, serveur: &str, membre: &str) -> String {
    etat.membre(serveur, membre)
        .and_then(|m| m.pseudo.clone())
        .unwrap_or_else(|| "?".to_string())
}

pub fn secondes(duree: i64, heures: f64) -> i64 {
    if duree == PERSONNALISE {
        (heures * 3600.0).round() as i64
    } else {
        duree
    }
}

pub fn heures_saisies(texte: &str) -> Option<f64> {
    texte
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|h| h.is_finite())
}

pub fn valide(genre: GenreModeration, raison: &str, pseudo: &str, secondes: i64) -> bool {
    match genre {
        GenreModeration::Pseudo => pseudo.trim().chars().count() <= MAX_PSEUDO,
        GenreModeration::Avertir => !raison.trim().is_empty(),
        GenreModeration::Exclure => (1..=MAX_EXCLUSION).contains(&secondes),
        GenreModeration::Bannir => (0..=MAX_BAN).contains(&secondes),
        GenreModeration::Expulser => true,
    }
}

pub fn borner(texte: &str, max: usize) -> String {
    texte.chars().take(max).collect()
}

pub fn texte_d_exclusion(
    etat: &Etat,
    salon: &str,
    maintenant_ms: i64,
    decalage_minutes: i32,
) -> Option<String> {
    let serveur = etat.serveur_du_salon(salon)?;
    let membre = etat.membre(&serveur, etat.moi());
    if !membre_exclu(membre, maintenant_ms) {
        return None;
    }
    let fin = membre?.timeout_until.as_deref()?;
    Some(format!(
        "Tu es exclu temporairement de ce serveur jusqu'au {}.",
        date_heure_complete(fin, decalage_minutes)?
    ))
}

pub fn peut_moderer_salon(etat: &Etat, serveur: &str, salon: &str) -> (bool, bool) {
    (
        etat.peut(Some(serveur), "manage_channels", None),
        etat.peut(Some(serveur), "manage_messages", Some(salon)),
    )
}

pub fn erreur_renommage(etat: &Etat, serveur: &str, salon: &str, nom: &str) -> &'static str {
    let salons = etat.salons(serveur);
    let Some(genre) = salons
        .iter()
        .find(|c| id_de(&c.id) == salon)
        .and_then(|c| c.kind)
    else {
        return "";
    };
    let autres: Vec<_> = salons
        .iter()
        .filter(|c| id_de(&c.id) != salon)
        .cloned()
        .collect();
    erreur_nom_salon(nom, genre, &autres)
}

pub fn nombre_a_supprimer(texte: &str) -> Option<i64> {
    texte
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|n| (1..=MAX_SUPPRESSION).contains(n))
}

pub fn question_de_suppression(nombre: i64, salon: &str, d_un_membre: bool) -> String {
    let membre = if d_un_membre { " de ce membre" } else { "" };
    format!("Supprimer les {nombre} derniers messages{membre} de « {salon} » ? Cette action est définitive.")
}

pub fn messages_supprimes(nombre: usize) -> String {
    format!("{nombre} message(s) supprimé(s).")
}

pub fn libelle_action(action: Option<GetServersIdAuditLogReponseElementAction>) -> &'static str {
    use GetServersIdAuditLogReponseElementAction as A;
    match action {
        Some(A::RoleCreate) => "a créé le rôle",
        Some(A::RoleUpdate) => "a modifié le rôle",
        Some(A::RoleDelete) => "a supprimé le rôle",
        Some(A::RoleReorder) => "a réordonné les rôles",
        Some(A::RoleAssign) => "a attribué le rôle",
        Some(A::RoleUnassign) => "a retiré le rôle",
        Some(A::ChannelOverride) => "a modifié les permissions du salon",
        Some(A::ChannelModeration) => "a réglé le mode lent / verrouillage de",
        Some(A::MemberKick) => "a expulsé",
        Some(A::MemberBan) => "a banni",
        Some(A::MemberUnban) => "a levé le ban de",
        Some(A::MemberTimeout) => "a exclu temporairement",
        Some(A::MemberUntimeout) => "a levé l'exclusion de",
        Some(A::MemberWarn) => "a averti",
        Some(A::MemberNickname) => "a changé le pseudo de",
        Some(A::VoiceMove) => "a déplacé en vocal",
        Some(A::VoiceDisconnect) => "a déconnecté du vocal",
        Some(A::MessageDelete) => "a supprimé un message de",
        Some(A::MessageBulkDelete) => "a supprimé en masse",
        Some(A::ServerTransfer) => "a transféré le serveur à",
        Some(A::Inconnu) | None => "a agi sur",
    }
}

fn prefixe(action: GetServersIdAuditLogReponseElementAction) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ton {
    Neutre,
    Attention,
    Danger,
}

pub fn icone_et_ton(
    action: Option<GetServersIdAuditLogReponseElementAction>,
) -> (&'static str, Ton) {
    use GetServersIdAuditLogReponseElementAction as A;
    let texte = action.map(prefixe).unwrap_or_default();
    let icone = match action {
        Some(A::MemberBan | A::MemberKick) => "interdit",
        Some(A::MemberTimeout | A::MemberUntimeout) => "horloge",
        Some(A::MemberWarn) => "alerte",
        _ if texte.starts_with("member") => "perso",
        _ if texte.starts_with("role") => "couronne",
        _ if texte.starts_with("channel") => "hash",
        _ if texte.starts_with("voice") => "vol",
        _ if texte.starts_with("message") => "corbeille",
        _ => "reglages",
    };
    let ton = match action {
        Some(A::MemberBan | A::MemberKick) => Ton::Danger,
        _ if texte.starts_with("message") => Ton::Danger,
        Some(A::MemberTimeout | A::MemberWarn) => Ton::Attention,
        _ => Ton::Neutre,
    };
    (icone, ton)
}

pub fn phrase_du_journal(entree: &GetServersIdAuditLogReponseElement) -> String {
    let mut phrase = libelle_action(entree.action).to_string();
    if let Some(cible) = entree.target.as_deref().filter(|c| !c.is_empty()) {
        phrase.push_str(&format!(" « {cible} »"));
    }
    if let Some(raison) = entree.reason.as_deref().filter(|r| !r.is_empty()) {
        phrase.push_str(&format!(" — {raison}"));
    }
    phrase
}

pub fn date_du_journal(
    entree: &GetServersIdAuditLogReponseElement,
    decalage_minutes: i32,
) -> String {
    entree
        .at
        .as_deref()
        .and_then(|d| date_heure_complete(d, decalage_minutes))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn etat(permissions: &[&str], moi: &str) -> Etat {
        let mut etat = Etat {
            compte: serde_json::from_value(json!({"id": moi, "pseudo": moi})).unwrap(),
            ..Etat::default()
        };
        etat.details.insert(
            "s1".into(),
            serde_json::from_value(json!({
                "server": {"id": "s1", "name": "Atelier", "owner_id": "o"},
                "roles": [
                    {"id": "rh", "name": "Haut", "position": 3},
                    {"id": "rb", "name": "Bas", "position": 1},
                    {"id": "r0", "name": "everyone", "position": 0, "is_default": true}
                ],
                "members": [
                    {"id": "o", "pseudo": "olga", "roles": []},
                    {"id": "a", "pseudo": "alice", "roles": ["rh"]},
                    {"id": "b", "pseudo": "bob", "roles": ["rb"], "nickname": "Bobby"},
                    {"id": "c", "pseudo": "carl", "roles": ["rb"], "timeout_until": "2099-01-01T00:00:00Z"},
                    {"id": "d", "pseudo": "dora", "roles": ["rh"]}
                ],
                "channels": [{"id": "c1", "name": "général", "kind": "text", "permissions": ["manage_messages"]}],
                "permissions": permissions
            }))
            .unwrap(),
        );
        etat
    }

    const TOUT: &[&str] = &[
        "timeout_members",
        "manage_nicknames",
        "kick_members",
        "ban_members",
    ];

    fn ids(actions: &[ActionModeration]) -> Vec<&str> {
        actions.iter().map(|a| a.id()).collect()
    }

    #[test]
    fn on_ne_modere_que_strictement_sous_son_rang() {
        let e = etat(TOUT, "a");
        assert_eq!(
            ids(&actions_de_moderation(&e, "s1", "b", 0)),
            ["timeout", "warn", "nickname", "kick", "ban"]
        );
        assert!(actions_de_moderation(&e, "s1", "d", 0).is_empty());
        assert!(actions_de_moderation(&e, "s1", "o", 0).is_empty());
        assert!(actions_de_moderation(&e, "s1", "a", 0).is_empty());
        let bas = etat(TOUT, "b");
        assert!(actions_de_moderation(&bas, "s1", "a", 0).is_empty());
        assert!(actions_de_moderation(&bas, "s1", "c", 0).is_empty());
        let createur = etat(TOUT, "o");
        assert_eq!(actions_de_moderation(&createur, "s1", "a", 0).len(), 5);
        assert!(actions_de_moderation(&e, "s2", "b", 0).is_empty());
    }

    #[test]
    fn chaque_action_demande_sa_permission() {
        let e = etat(&["kick_members"], "a");
        assert_eq!(ids(&actions_de_moderation(&e, "s1", "b", 0)), ["kick"]);
        let e = etat(&["manage_nicknames", "ban_members"], "a");
        assert_eq!(
            ids(&actions_de_moderation(&e, "s1", "b", 0)),
            ["nickname", "ban"]
        );
        assert!(actions_de_moderation(&etat(&[], "a"), "s1", "b", 0).is_empty());
    }

    #[test]
    fn un_membre_exclu_propose_de_lever_l_exclusion() {
        let e = etat(&["timeout_members"], "a");
        let actions = actions_de_moderation(&e, "s1", "c", 0);
        assert_eq!(ids(&actions), ["untimeout", "warn"]);
        assert_eq!(actions[0].libelle(), "Lever l'exclusion temporaire");
        let apres = date_heure_ms("2099-01-02T00:00:00Z");
        assert_eq!(
            ids(&actions_de_moderation(&e, "s1", "c", apres)),
            ["timeout", "warn"]
        );
    }

    fn date_heure_ms(iso: &str) -> i64 {
        crate::roles::date_en_millisecondes(iso).unwrap()
    }

    #[test]
    fn les_libelles_suivent_le_front() {
        let g = GenreModeration::Exclure;
        assert_eq!(g.titre("bob"), "Exclure temporairement bob");
        assert_eq!(
            GenreModeration::Pseudo.titre("bob"),
            "Changer le pseudo de bob"
        );
        assert_eq!(GenreModeration::Bannir.bouton("bob"), "Bannir bob");
        assert_eq!(GenreModeration::Avertir.bouton("bob"), "Confirmer");
        assert!(ActionModeration::Moderer(GenreModeration::Expulser).danger());
        assert!(!ActionModeration::LeverExclusion.danger());
        for a in ["kick", "ban", "timeout", "warn", "nickname", "untimeout"] {
            assert_eq!(ActionModeration::depuis_id(a).unwrap().id(), a);
        }
        assert_eq!(ActionModeration::depuis_id("x"), None);
        let e = etat(TOUT, "a");
        assert_eq!(nom_de_la_cible(&e, "s1", "b"), "bob");
        assert_eq!(pseudo_de_serveur(&e, "s1", "b"), "Bobby");
        assert_eq!(nom_de_la_cible(&e, "s1", "z"), "?");
    }

    #[test]
    fn la_validation_reprend_celle_du_front() {
        use GenreModeration::*;
        assert!(valide(Expulser, "", "", 0));
        assert!(!valide(Avertir, "  ", "", 0));
        assert!(valide(Avertir, "spam", "", 0));
        assert!(valide(Pseudo, "", "", 0));
        assert!(valide(Pseudo, "", &"é".repeat(32), 0));
        assert!(!valide(Pseudo, "", &"é".repeat(33), 0));
        assert!(!valide(Exclure, "", "", 0));
        assert!(valide(Exclure, "", "", 60));
        assert!(valide(Exclure, "", "", MAX_EXCLUSION));
        assert!(!valide(Exclure, "", "", MAX_EXCLUSION + 1));
        assert!(valide(Bannir, "", "", 0));
        assert!(!valide(Bannir, "", "", -5));
        assert_eq!(secondes(600, 9.0), 600);
        assert_eq!(secondes(PERSONNALISE, 1.5), 5400);
        assert_eq!(secondes(PERSONNALISE, 0.0003), 1);
        assert_eq!(heures_saisies(" 2,5 "), Some(2.5));
        assert_eq!(heures_saisies("abc"), None);
        assert_eq!(borner("abcdef", 3), "abc");
        assert_eq!(Bannir.duree_de_depart(), 0);
        assert_eq!(Exclure.duree_de_depart(), 600);
        assert_eq!(Exclure.durees().len(), 7);
        assert!(Expulser.durees().is_empty());
    }

    #[test]
    fn un_membre_exclu_lit_jusqu_a_quand() {
        let e = etat(&[], "c");
        assert_eq!(
            texte_d_exclusion(&e, "c1", 0, 0).as_deref(),
            Some("Tu es exclu temporairement de ce serveur jusqu'au 01/01/2099 00:00:00.")
        );
        let apres = date_heure_ms("2099-01-02T00:00:00Z");
        assert_eq!(texte_d_exclusion(&e, "c1", apres, 0), None);
        assert_eq!(texte_d_exclusion(&etat(&[], "b"), "c1", 0, 0), None);
        assert_eq!(texte_d_exclusion(&e, "inconnu", 0, 0), None);
    }

    #[test]
    fn renommer_un_salon_ignore_son_propre_nom() {
        let mut e = etat(&[], "a");
        let detail = e.details.get_mut("s1").unwrap();
        detail.channels.as_mut().unwrap().push(
            serde_json::from_value(json!({"id": "c2", "name": "annonces", "kind": "text"}))
                .unwrap(),
        );
        assert_eq!(erreur_renommage(&e, "s1", "c1", "général"), "");
        assert_eq!(erreur_renommage(&e, "s1", "c1", "Général"), "");
        assert_eq!(
            erreur_renommage(&e, "s1", "c1", "Annonces"),
            "Un salon de ce type porte déjà ce nom."
        );
        assert_eq!(
            erreur_renommage(&e, "s1", "c1", " "),
            "Donne un nom au salon."
        );
        assert_eq!(
            erreur_renommage(&e, "s1", "c1", &"a".repeat(31)),
            "30 caractères maximum."
        );
    }

    #[test]
    fn la_suppression_en_masse_reste_entre_1_et_100() {
        assert_eq!(nombre_a_supprimer(" 10 "), Some(10));
        assert_eq!(nombre_a_supprimer("100"), Some(100));
        assert_eq!(nombre_a_supprimer("0"), None);
        assert_eq!(nombre_a_supprimer("101"), None);
        assert_eq!(nombre_a_supprimer("dix"), None);
        assert_eq!(
            question_de_suppression(5, "général", true),
            "Supprimer les 5 derniers messages de ce membre de « général » ? Cette action est définitive."
        );
        assert_eq!(messages_supprimes(2), "2 message(s) supprimé(s).");
    }

    #[test]
    fn moderer_un_salon_suit_ses_permissions() {
        let e = etat(&[], "a");
        assert_eq!(peut_moderer_salon(&e, "s1", "c1"), (false, true));
        let e = etat(&["manage_channels"], "a");
        assert_eq!(peut_moderer_salon(&e, "s1", "c1"), (true, true));
    }

    #[test]
    fn le_journal_se_lit_comme_dans_le_front() {
        let entree: GetServersIdAuditLogReponseElement = serde_json::from_value(json!({
            "at": "2026-03-04T10:20:00Z", "actor": "alice", "action": "member.ban", "target": "bob", "reason": "spam"
        }))
        .unwrap();
        assert_eq!(phrase_du_journal(&entree), "a banni « bob » — spam");
        assert_eq!(icone_et_ton(entree.action), ("interdit", Ton::Danger));
        assert!(!date_du_journal(&entree, 0).is_empty());
        let role: GetServersIdAuditLogReponseElement =
            serde_json::from_value(json!({"action": "role.reorder", "target": ""})).unwrap();
        assert_eq!(phrase_du_journal(&role), "a réordonné les rôles");
        assert_eq!(icone_et_ton(role.action), ("couronne", Ton::Neutre));
        use GetServersIdAuditLogReponseElementAction as A;
        assert_eq!(
            icone_et_ton(Some(A::MemberWarn)),
            ("alerte", Ton::Attention)
        );
        assert_eq!(
            icone_et_ton(Some(A::MemberNickname)),
            ("perso", Ton::Neutre)
        );
        assert_eq!(
            icone_et_ton(Some(A::MessageDelete)),
            ("corbeille", Ton::Danger)
        );
        assert_eq!(icone_et_ton(Some(A::VoiceMove)), ("vol", Ton::Neutre));
        assert_eq!(
            icone_et_ton(Some(A::ServerTransfer)),
            ("reglages", Ton::Neutre)
        );
        let inconnue: GetServersIdAuditLogReponseElement =
            serde_json::from_value(json!({"action": "autre.chose"})).unwrap();
        assert_eq!(phrase_du_journal(&inconnue), "a agi sur");
    }
}
