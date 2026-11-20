use std::collections::HashSet;

use estompe_client::format::{date_heure_complete, heure_de, lignes_de_messages, Ligne};
use estompe_client::mention::Mentionnables;
use estompe_client::roles::{couleur_du_pseudo, Membre};
use estompe_client::store::messages::{auteur, est_compte_estompe, id_message};
use estompe_client::store::{CleConversation, Etat};
use estompe_client::types::Message;

use crate::coquille::avatars::CleAvatar;
use crate::coquille::membres::{analyser_couleur, cle_avatar};
use crate::coquille::projection::{initiales, teinte};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreLigne {
    Jour,
    Nouveau,
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesReponse {
    pub id: String,
    pub auteur: String,
    pub texte: String,
    pub supprimee: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesReaction {
    pub emoji: String,
    pub nombre: i64,
    pub moi: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DonneesAnnexes {
    pub pieces_jointes: usize,
    pub sondage: bool,
    pub reactions: Vec<DonneesReaction>,
    pub lien: Option<String>,
}

impl DonneesAnnexes {
    pub fn est_vide(&self) -> bool {
        self.pieces_jointes == 0 && !self.sondage && self.reactions.is_empty()
    }

    pub fn resume(&self) -> String {
        let mut parties = Vec::new();
        match self.pieces_jointes {
            0 => {}
            1 => parties.push("1 pièce jointe".to_string()),
            n => parties.push(format!("{n} pièces jointes")),
        }
        if self.sondage {
            parties.push("Sondage".to_string());
        }
        if !self.reactions.is_empty() {
            parties.push(
                self.reactions
                    .iter()
                    .map(|r| format!("{} {}", r.emoji, r.nombre))
                    .collect::<Vec<_>>()
                    .join("  "),
            );
        }
        parties.join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesMessage {
    pub id: String,
    pub utilisateur: String,
    pub auteur: String,
    pub initiales: String,
    pub teinte: i32,
    pub couleur: Option<String>,
    pub avatar: Option<CleAvatar>,
    pub bot: bool,
    pub heure: String,
    pub date_complete: String,
    pub suite: bool,
    pub systeme: bool,
    pub epingle: bool,
    pub modifie: bool,
    pub date_modification: String,
    pub mention: bool,
    pub mis_en_evidence: bool,
    pub reponse: Option<DonneesReponse>,
    pub contenu: String,
    pub annexes: DonneesAnnexes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesLigne {
    pub cle: String,
    pub genre: GenreLigne,
    pub libelle: String,
    pub message: Option<DonneesMessage>,
}

pub struct ContexteLignes<'a> {
    pub etat: &'a Etat,
    pub serveur: Option<&'a str>,
    pub nouveau_depuis: Option<&'a str>,
    pub mis_en_evidence: Option<&'a str>,
    pub supprimes: &'a HashSet<String>,
    pub decalage_minutes: i32,
}

fn texte(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

fn membre<'a>(etat: &'a Etat, serveur: Option<&str>, utilisateur: &str) -> Option<&'a Membre> {
    etat.details
        .get(serveur?)?
        .members
        .as_deref()?
        .iter()
        .find(|m| texte(&m.id) == utilisateur)
}

fn cle_avatar_du_message(
    etat: &Etat,
    serveur: Option<&str>,
    utilisateur: &str,
) -> Option<CleAvatar> {
    match serveur {
        Some(serveur) => cle_avatar(etat, serveur, utilisateur),
        None => {
            let version = etat.version_avatar(utilisateur);
            (version > 0).then(|| CleAvatar {
                utilisateur: utilisateur.to_string(),
                version,
                serveur: None,
            })
        }
    }
}

fn une_ligne(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn annexes(message: &Message) -> DonneesAnnexes {
    DonneesAnnexes {
        pieces_jointes: message.attachments.as_ref().map_or(0, Vec::len),
        sondage: message.poll.is_some(),
        reactions: message
            .reactions
            .iter()
            .flatten()
            .filter(|r| r.count.unwrap_or(0) > 0)
            .map(|r| DonneesReaction {
                emoji: texte(&r.emoji).to_string(),
                nombre: r.count.unwrap_or(0),
                moi: r.me == Some(true),
            })
            .collect(),
        lien: estompe_client::markdown::premier_lien(texte(&message.content)),
    }
}

pub fn donnees_message(message: &Message, suite: bool, c: &ContexteLignes) -> DonneesMessage {
    let id = id_message(message).to_string();
    let utilisateur = auteur(message).to_string();
    let pseudo = message
        .author
        .as_ref()
        .map(|p| texte(&p.pseudo).to_string())
        .unwrap_or_default();
    let detail = c.serveur.and_then(|s| c.etat.details.get(s));
    let couleur = couleur_du_pseudo(detail, membre(c.etat, c.serveur, &utilisateur))
        .filter(|c| analyser_couleur(c).is_some())
        .map(str::to_string);
    let systeme = message.system == Some(true);
    let moi = c.etat.moi();
    let cree = texte(&message.created_at);
    DonneesMessage {
        initiales: initiales(&pseudo),
        teinte: teinte(if utilisateur.is_empty() {
            &pseudo
        } else {
            &utilisateur
        }),
        couleur,
        avatar: cle_avatar_du_message(c.etat, c.serveur, &utilisateur),
        bot: est_compte_estompe(&utilisateur),
        heure: heure_de(cree, c.decalage_minutes).unwrap_or_default(),
        date_complete: date_heure_complete(cree, c.decalage_minutes).unwrap_or_default(),
        suite: suite && !systeme,
        systeme,
        epingle: message.pinned_at.is_some(),
        modifie: message.edited_at.is_some(),
        date_modification: message
            .edited_at
            .as_deref()
            .and_then(|d| date_heure_complete(d, c.decalage_minutes))
            .unwrap_or_default(),
        mention: utilisateur != moi
            && !systeme
            && !moi.is_empty()
            && message.mentions.iter().flatten().any(|m| m == moi),
        mis_en_evidence: c.mis_en_evidence == Some(id.as_str()),
        reponse: message.reply_to.as_ref().map(|r| {
            let cible = texte(&r.id).to_string();
            DonneesReponse {
                supprimee: c.supprimes.contains(&cible),
                auteur: r
                    .author
                    .as_ref()
                    .map(|p| texte(&p.pseudo).to_string())
                    .unwrap_or_default(),
                texte: une_ligne(texte(&r.content)),
                id: cible,
            }
        }),
        contenu: texte(&message.content).to_string(),
        annexes: annexes(message),
        auteur: pseudo,
        utilisateur,
        id,
    }
}

pub fn lignes_de_conversation(messages: &[Message], c: &ContexteLignes) -> Vec<DonneesLigne> {
    lignes_de_messages(messages, c.nouveau_depuis, c.decalage_minutes)
        .into_iter()
        .map(|ligne| match ligne {
            Ligne::Jour { cle, libelle } => DonneesLigne {
                cle,
                genre: GenreLigne::Jour,
                libelle: majuscule(&libelle),
                message: None,
            },
            Ligne::Nouveau { cle } => DonneesLigne {
                cle,
                genre: GenreLigne::Nouveau,
                libelle: "Nouveaux messages".to_string(),
                message: None,
            },
            Ligne::Message { cle, index, suite } => DonneesLigne {
                cle,
                genre: GenreLigne::Message,
                libelle: String::new(),
                message: Some(donnees_message(&messages[index], suite, c)),
            },
        })
        .collect()
}

fn majuscule(texte: &str) -> String {
    let mut caracteres = texte.chars();
    match caracteres.next() {
        Some(premier) => premier.to_uppercase().chain(caracteres).collect(),
        None => String::new(),
    }
}

pub fn premier_non_lu(messages: &[Message], non_lus: i64, moi: &str) -> Option<String> {
    if non_lus <= 0 {
        return None;
    }
    let mut restants = non_lus;
    let mut premier = None;
    for message in messages.iter().rev() {
        if auteur(message) == moi {
            continue;
        }
        premier = Some(id_message(message).to_string());
        restants -= 1;
        if restants == 0 {
            break;
        }
    }
    premier
}

pub fn mentionnables(etat: &Etat, cle: &CleConversation) -> Mentionnables {
    let mut personnes = etat.pseudos_de(Some(cle));
    let mut roles = Vec::new();
    let mut groupes = Vec::new();
    if let Some((detail, _)) = cle.salon().and_then(|s| etat.salon(s)) {
        for m in detail.members.as_deref().unwrap_or_default() {
            if let Some(surnom) = m.nickname.as_deref().filter(|s| !s.is_empty()) {
                personnes.push(surnom.to_string());
            }
        }
        roles = detail
            .roles
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|r| r.is_default != Some(true))
            .map(|r| texte(&r.name).to_string())
            .collect();
        groupes = vec!["everyone".to_string(), "here".to_string()];
    }
    Mentionnables::nouveau(personnes, roles, groupes, Vec::<String>::new())
}

pub fn utilisateur_mentionne(etat: &Etat, cle: &CleConversation, nom: &str) -> Option<String> {
    let nom = nom.to_lowercase();
    match cle {
        CleConversation::Salon(salon) => etat.salon(salon).and_then(|(detail, _)| {
            detail
                .members
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|m| {
                    texte(&m.pseudo).to_lowercase() == nom
                        || m.nickname.as_deref().map(str::to_lowercase).as_deref()
                            == Some(nom.as_str())
                })
                .map(|m| texte(&m.id).to_string())
        }),
        CleConversation::Prive(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;
    use serde_json::json;

    fn message(id: &str, auteur: &str, heure: &str, contenu: &str) -> Message {
        serde_json::from_value(json!({
            "id": id, "channel_id": "c1",
            "author": {"id": auteur, "pseudo": auteur},
            "content": contenu,
            "created_at": format!("2026-11-19T{heure}:00Z"),
        }))
        .unwrap()
    }

    fn contexte<'a>(etat: &'a Etat, supprimes: &'a HashSet<String>) -> ContexteLignes<'a> {
        ContexteLignes {
            etat,
            serveur: Some("s1"),
            nouveau_depuis: None,
            mis_en_evidence: None,
            supprimes,
            decalage_minutes: 60,
        }
    }

    #[test]
    fn regroupe_les_messages_consecutifs_et_separe_les_jours() {
        let e = etat();
        let supprimes = HashSet::new();
        let mut hier = message("1", "carol", "09:00", "hier");
        hier.created_at = Some("2026-11-18T09:00:00Z".into());
        let messages = vec![
            hier,
            message("2", "carol", "09:00", "a"),
            message("3", "carol", "09:02", "b"),
            message("4", "dave", "09:03", "c"),
            message("5", "dave", "09:30", "d"),
        ];
        let lignes = lignes_de_conversation(&messages, &contexte(&e, &supprimes));
        let resume: Vec<(GenreLigne, bool)> = lignes
            .iter()
            .map(|l| (l.genre, l.message.as_ref().is_some_and(|m| m.suite)))
            .collect();
        assert_eq!(
            resume,
            [
                (GenreLigne::Jour, false),
                (GenreLigne::Message, false),
                (GenreLigne::Jour, false),
                (GenreLigne::Message, false),
                (GenreLigne::Message, true),
                (GenreLigne::Message, false),
                (GenreLigne::Message, false),
            ]
        );
        assert_eq!(lignes[0].libelle, "Mercredi 18 novembre 2026");
        assert_eq!(lignes[3].message.as_ref().unwrap().heure, "10:00");
    }

    #[test]
    fn la_ligne_nouveau_coupe_le_groupe_et_le_premier_non_lu_ignore_mes_messages() {
        let e = etat();
        let supprimes = HashSet::new();
        let messages = vec![
            message("1", "carol", "09:00", "a"),
            message("2", "carol", "09:01", "b"),
            message("3", "me", "09:02", "moi"),
            message("4", "carol", "09:03", "c"),
        ];
        assert_eq!(premier_non_lu(&messages, 2, "me").as_deref(), Some("2"));
        assert_eq!(premier_non_lu(&messages, 0, "me"), None);
        assert_eq!(premier_non_lu(&messages, 99, "me").as_deref(), Some("1"));
        let mut c = contexte(&e, &supprimes);
        c.nouveau_depuis = Some("2");
        let lignes = lignes_de_conversation(&messages, &c);
        let cles: Vec<&str> = lignes.iter().map(|l| l.cle.as_str()).collect();
        assert_eq!(cles, ["day-1", "1", "new-2", "2", "3", "4"]);
        assert!(!lignes[3].message.as_ref().unwrap().suite);
    }

    #[test]
    fn chaque_variante_du_message_est_projetee() {
        let mut e = etat();
        e.details.get_mut("s1").unwrap().roles = Some(
            serde_json::from_value(json!([{"id": "r1", "name": "Art", "color": "#b0413e", "position": 1}]))
                .unwrap(),
        );
        e.details.get_mut("s1").unwrap().members = Some(
            serde_json::from_value(json!([{"id": "carol", "pseudo": "carol", "roles": ["r1"]}]))
                .unwrap(),
        );
        let supprimes = HashSet::from(["70".to_string()]);
        let mut m: Message = serde_json::from_value(json!({
            "id": "80", "channel_id": "c1",
            "author": {"id": "carol", "pseudo": "carol"},
            "content": "@bob regarde",
            "created_at": "2026-11-19T09:00:00Z",
            "edited_at": "2026-11-19T09:05:00Z",
            "pinned_at": "2026-11-19T09:06:00Z",
            "mentions": ["me"],
            "reply_to": {"id": "70", "author": {"id": "dave", "pseudo": "dave"}, "content": "texte\nsur deux lignes"},
            "attachments": [{"id": "a"}, {"id": "b"}],
            "reactions": [{"emoji": "👍", "count": 3, "me": true}, {"emoji": "🎉", "count": 0}]
        }))
        .unwrap();
        let c = contexte(&e, &supprimes);
        let d = donnees_message(&m, false, &c);
        assert_eq!(d.couleur.as_deref(), Some("#b0413e"));
        assert!(d.modifie && d.epingle && d.mention && !d.bot && !d.systeme);
        assert_eq!(d.date_modification, "19/11/2026 10:05:00");
        let reponse = d.reponse.unwrap();
        assert_eq!(
            (reponse.auteur.as_str(), reponse.texte.as_str(), reponse.supprimee),
            ("dave", "texte sur deux lignes", true)
        );
        assert_eq!(d.annexes.resume(), "2 pièces jointes · 👍 3");
        m.system = Some(true);
        m.author = serde_json::from_value(json!({"id": estompe_client::store::messages::ID_COMPTE_ESTOMPE, "pseudo": "Estompe"})).unwrap();
        let systeme = donnees_message(&m, true, &c);
        assert!(systeme.systeme && systeme.bot && !systeme.suite && !systeme.mention);
        let mut evidence = contexte(&e, &supprimes);
        evidence.mis_en_evidence = Some("80");
        assert!(donnees_message(&m, false, &evidence).mis_en_evidence);
    }

    #[test]
    fn mentionnables_et_resolution_d_une_mention() {
        let mut e = etat();
        e.details.get_mut("s1").unwrap().members = Some(
            serde_json::from_value(json!([{"id": "carol", "pseudo": "carol", "nickname": "Caro"}]))
                .unwrap(),
        );
        let cle = CleConversation::Salon("c1".into());
        let contexte = mentionnables(&e, &cle);
        let rendu = estompe_client::markdown::analyser("@Caro @everyone", &contexte);
        assert_eq!(rendu.blocs.len(), 1);
        assert_eq!(utilisateur_mentionne(&e, &cle, "caro").as_deref(), Some("carol"));
        assert_eq!(utilisateur_mentionne(&e, &cle, "CAROL").as_deref(), Some("carol"));
        assert_eq!(utilisateur_mentionne(&e, &cle, "personne"), None);
    }
}
