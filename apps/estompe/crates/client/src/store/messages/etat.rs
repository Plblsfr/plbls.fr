use std::cmp::Ordering;
use std::collections::BTreeMap;

use tokio::time::{Duration, Instant};

use super::envois::EnvoiLocal;
use crate::store::etat::{id_de, CleConversation, Etat, Presence};
use crate::temps_reel::evenements::Lecture;
use crate::types::Message;

pub const CONVERSATIONS_EN_CACHE: usize = 8;
pub const MESSAGES_PAR_CONVERSATION: usize = 600;
pub const LIMITE_PAGE: u32 = crate::appels::LIMITE_MESSAGES;
pub const LIMITE_PAGE_LONGUE: u32 = 100;
pub const PAGES_RATTRAPAGE_MAX: usize = 5;
pub const MESSAGES_SAUT_MAX: usize = MESSAGES_PAR_CONVERSATION / 2;
pub const DUREE_SAISIE: Duration = Duration::from_secs(5);
pub const ID_COMPTE_ESTOMPE: &str = "00000000-0000-0000-0000-000000000001";

pub fn est_compte_estompe(utilisateur: &str) -> bool {
    utilisateur == ID_COMPTE_ESTOMPE
}

pub fn comparer_ids(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

pub fn id_message(message: &Message) -> &str {
    id_de(&message.id)
}

pub fn auteur(message: &Message) -> &str {
    message
        .author
        .as_ref()
        .map(|p| id_de(&p.id))
        .unwrap_or_default()
}

pub fn conversation_du_message(message: &Message) -> Option<CleConversation> {
    CleConversation::de(message.channel_id.as_deref(), message.dm_id.as_deref())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub debut_atteint: bool,
    pub fin_atteinte: bool,
    pub chargee: bool,
    pub en_chargement: bool,
    pub rechargee: bool,
    pub ancre: Option<String>,
    pub lectures: BTreeMap<String, Lecture>,
    pub(crate) en_attente: Vec<Message>,
}

impl Conversation {
    pub fn position(&self, id: &str) -> Result<usize, usize> {
        self.messages
            .binary_search_by(|m| comparer_ids(id_message(m), id))
    }

    pub fn contient(&self, id: &str) -> bool {
        self.position(id).is_ok()
    }

    pub fn message(&self, id: &str) -> Option<&Message> {
        self.position(id).ok().map(|i| &self.messages[i])
    }

    pub fn premier_id(&self) -> Option<&str> {
        self.messages.first().map(id_message)
    }

    pub fn dernier_id(&self) -> Option<&str> {
        self.messages.last().map(id_message)
    }

    pub fn lecteurs(&self, moi: &str) -> BTreeMap<String, Vec<Lecture>> {
        let mut sortie: BTreeMap<String, Vec<Lecture>> = BTreeMap::new();
        for lecture in self.lectures.values().filter(|l| l.user_id != moi) {
            let i = match self.position(&lecture.last_id) {
                Ok(i) => Some(i),
                Err(0) => None,
                Err(i) => Some(i - 1),
            };
            if let Some(i) = i {
                sortie
                    .entry(id_message(&self.messages[i]).to_string())
                    .or_default()
                    .push(lecture.clone());
            }
        }
        for liste in sortie.values_mut() {
            liste.sort_by(|a, b| a.read_at.cmp(&b.read_at));
        }
        sortie
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaisieEnCours {
    pub utilisateur: String,
    pub pseudo: String,
    pub expire: Instant,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EtatMessages {
    pub conversations: BTreeMap<CleConversation, Conversation>,
    pub recentes: Vec<CleConversation>,
    pub saisies: BTreeMap<CleConversation, Vec<SaisieEnCours>>,
    pub remontees_privees: Vec<String>,
    pub epingles: Option<(CleConversation, Vec<Message>)>,
    pub envois: Vec<EnvoiLocal>,
    pub prochain_envoi: u64,
}

impl EtatMessages {
    pub fn conversation(&self, cle: &CleConversation) -> Option<&Conversation> {
        self.conversations.get(cle)
    }

    pub fn envois_de(&self, cle: &CleConversation) -> Vec<&EnvoiLocal> {
        self.envois
            .iter()
            .filter(|e| e.conversation == *cle)
            .collect()
    }

    pub fn envoi(&self, id: u64) -> Option<&EnvoiLocal> {
        self.envois.iter().find(|e| e.id == id)
    }

    pub fn epingles_de(&self, cle: &CleConversation) -> Option<&[Message]> {
        match &self.epingles {
            Some((c, liste)) if c == cle => Some(liste),
            _ => None,
        }
    }

    pub fn saisies_de(&self, cle: &CleConversation) -> Vec<String> {
        self.saisies
            .get(cle)
            .map(|l| l.iter().map(|s| s.pseudo.clone()).collect())
            .unwrap_or_default()
    }

    pub fn prochaine_expiration(&self) -> Option<Instant> {
        self.saisies.values().flatten().map(|s| s.expire).min()
    }

    pub fn lecteurs(&self, cle: &CleConversation, moi: &str) -> BTreeMap<String, Vec<Lecture>> {
        self.conversations
            .get(cle)
            .map(|c| c.lecteurs(moi))
            .unwrap_or_default()
    }

    pub(crate) fn toucher(&mut self, cle: &CleConversation) {
        self.recentes.retain(|c| c != cle);
        self.recentes.insert(0, cle.clone());
    }

    pub(crate) fn reserver(
        &mut self,
        cle: &CleConversation,
        courante: Option<&CleConversation>,
    ) -> &mut Conversation {
        self.toucher(cle);
        if !self.conversations.contains_key(cle) {
            while self.conversations.len() >= CONVERSATIONS_EN_CACHE {
                let Some(vieille) = self
                    .recentes
                    .iter()
                    .rev()
                    .find(|c| *c != cle && Some(*c) != courante)
                    .cloned()
                else {
                    break;
                };
                self.oublier(&vieille);
            }
        }
        self.conversations.entry(cle.clone()).or_default()
    }

    pub(crate) fn oublier(&mut self, cle: &CleConversation) -> bool {
        self.recentes.retain(|c| c != cle);
        let saisies = self.saisies.remove(cle).is_some();
        self.conversations.remove(cle).is_some() || saisies
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LignePrivee {
    pub id: String,
    pub utilisateur: String,
    pub pseudo: String,
    pub non_lus: i64,
    pub mentions: usize,
    pub presence: Presence,
    pub lecture_seule: bool,
}

pub fn lignes_privees(etat: &Etat, messages: &EtatMessages) -> Vec<LignePrivee> {
    let rang = |id: &str| {
        messages
            .remontees_privees
            .iter()
            .position(|r| r == id)
            .unwrap_or(usize::MAX)
    };
    let mut lignes: Vec<(usize, usize, LignePrivee)> = etat
        .messages_prives
        .iter()
        .enumerate()
        .map(|(i, dm)| {
            let id = id_de(&dm.id).to_string();
            let autre = dm.other.clone().unwrap_or_default();
            let utilisateur = id_de(&autre.id).to_string();
            let cle = CleConversation::Prive(id.clone());
            let ligne = LignePrivee {
                non_lus: etat.non_lus_de(&cle),
                mentions: etat.nombre_mentions(&cle),
                presence: etat.presence_de(&utilisateur),
                lecture_seule: est_compte_estompe(&utilisateur),
                pseudo: id_de(&autre.pseudo).to_string(),
                utilisateur,
                id,
            };
            (rang(&ligne.id), i, ligne)
        })
        .collect();
    lignes.sort_by_key(|(r, i, _)| (*r, *i));
    lignes.into_iter().map(|(_, _, l)| l).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::store::essais::{depuis_json, etat_de_depart};

    fn cle(texte: &str) -> CleConversation {
        CleConversation::analyser(texte).unwrap()
    }

    fn conversation(ids: &[&str]) -> Conversation {
        Conversation {
            messages: ids
                .iter()
                .map(|id| depuis_json(json!({"id": id, "channel_id": "c1"})))
                .collect(),
            chargee: true,
            ..Conversation::default()
        }
    }

    fn lire(c: &mut Conversation, utilisateur: &str, dernier: &str) {
        c.lectures.insert(
            utilisateur.to_string(),
            Lecture {
                channel_id: Some("c1".into()),
                user_id: utilisateur.into(),
                pseudo: utilisateur.into(),
                last_id: dernier.into(),
                read_at: "2026-10-13T10:05:00Z".into(),
                ..Lecture::default()
            },
        );
    }

    fn places(c: &Conversation) -> Vec<String> {
        c.lecteurs("me").into_keys().collect()
    }

    #[test]
    fn compare_les_ids_comme_des_entiers() {
        assert_eq!(comparer_ids("9", "10"), Ordering::Less);
        assert_eq!(comparer_ids("100", "99"), Ordering::Greater);
        assert_eq!(comparer_ids("42", "42"), Ordering::Equal);
    }

    #[test]
    fn place_chaque_personne_sous_son_dernier_message_vu_sans_soi_meme() {
        let mut c = conversation(&["9", "10", "11"]);
        lire(&mut c, "bob", "10");
        lire(&mut c, "me", "11");
        assert_eq!(places(&c), ["10"]);
    }

    #[test]
    fn deplace_l_avatar_vers_un_message_plus_recent() {
        let mut c = conversation(&["9", "10", "11"]);
        lire(&mut c, "bob", "9");
        assert_eq!(places(&c), ["9"]);
        lire(&mut c, "bob", "11");
        assert_eq!(places(&c), ["11"]);
    }

    #[test]
    fn retombe_sur_le_message_charge_precedent_si_le_dernier_vu_n_est_plus_affiche() {
        let mut c = conversation(&["9", "10", "11"]);
        lire(&mut c, "bob", "100");
        assert_eq!(places(&c), ["11"]);
        lire(&mut c, "bob", "8");
        assert!(places(&c).is_empty());
    }

    #[test]
    fn garde_au_plus_le_nombre_de_conversations_prevu_sans_oublier_la_courante() {
        let mut m = EtatMessages::default();
        let courante = cle("c:c0");
        m.reserver(&courante, Some(&courante)).chargee = true;
        for i in 1..=CONVERSATIONS_EN_CACHE * 3 {
            m.reserver(&cle(&format!("c:c{i}")), Some(&courante));
            assert!(m.conversations.len() <= CONVERSATIONS_EN_CACHE);
        }
        assert!(m.conversations.contains_key(&courante));
        let dernier = cle(&format!("c:c{}", CONVERSATIONS_EN_CACHE * 3));
        assert_eq!(m.recentes[0], dernier);
        assert_eq!(m.recentes.len(), CONVERSATIONS_EN_CACHE);
        assert!(!m.conversations.contains_key(&cle("c:c1")));
    }

    #[test]
    fn repere_le_compte_estompe_par_son_identifiant_pas_par_son_pseudo() {
        assert!(est_compte_estompe(ID_COMPTE_ESTOMPE));
        assert!(!est_compte_estompe("u1"));
        let mut etat = etat_de_depart();
        etat.messages_prives = vec![
            depuis_json(json!({"id": "d1", "other": {"id": "u1", "pseudo": "Estompe"}})),
            depuis_json(json!({"id": "d2", "other": {"id": ID_COMPTE_ESTOMPE, "pseudo": "x"}})),
        ];
        let lignes = lignes_privees(&etat, &EtatMessages::default());
        let seules: Vec<bool> = lignes.iter().map(|l| l.lecture_seule).collect();
        assert_eq!(seules, [false, true]);
    }

    #[test]
    fn liste_les_conversations_privees_par_activite_avec_non_lus_et_presence() {
        let mut etat = etat_de_depart();
        etat.messages_prives = vec![
            depuis_json(json!({"id": "d1", "other": {"id": "dave", "pseudo": "dave"}})),
            depuis_json(json!({"id": "d2", "other": {"id": "fred", "pseudo": "fred"}})),
            depuis_json(json!({"id": "d3", "other": {"id": "gil", "pseudo": "gil"}})),
        ];
        etat.en_ligne.insert("fred".into());
        etat.non_lus.insert(cle("d:d3"), 2);
        let messages = EtatMessages {
            remontees_privees: vec!["d3".into(), "d2".into(), "d9".into()],
            ..EtatMessages::default()
        };
        let lignes = lignes_privees(&etat, &messages);
        let resume: Vec<(&str, i64, Presence)> = lignes
            .iter()
            .map(|l| (l.id.as_str(), l.non_lus, l.presence))
            .collect();
        assert_eq!(
            resume,
            [
                ("d3", 2, Presence::HorsLigne),
                ("d2", 0, Presence::EnLigne),
                ("d1", 0, Presence::HorsLigne)
            ]
        );
        assert_eq!(lignes[0].pseudo, "gil");
    }
}
