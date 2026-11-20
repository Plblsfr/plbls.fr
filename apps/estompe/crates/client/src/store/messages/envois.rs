use std::time::{SystemTime, UNIX_EPOCH};

use tokio::time::{Duration, Instant};

use super::etat::{auteur, conversation_du_message, id_message, EtatMessages};
use super::reducteur::{message_cree, ChangementMessages, Contexte};
use crate::appels::{NouveauMessage, NouveauSondage};
use crate::format::iso_de_ms;
use crate::http::ErreurApi;
use crate::store::etat::CleConversation;
use crate::store::reducteur::{Application, Changement};
use crate::types::{Attachment, Message, MessageReplyTo, Person};

pub const LONGUEUR_MAX: usize = 4000;
pub const SEUIL_COMPTEUR: usize = 3500;
pub const SEUIL_ALERTE: usize = 3900;
pub const RYTHME_SAISIE: Duration = Duration::from_secs(3);
pub const PREFIXE_LOCAL: &str = "local:";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Brouillon {
    pub contenu: String,
    pub reponse_a: Option<String>,
    pub pieces: Vec<Attachment>,
    pub sondage: Option<NouveauSondage>,
}

impl Brouillon {
    pub fn texte(contenu: &str) -> Self {
        Self {
            contenu: contenu.to_string(),
            ..Self::default()
        }
    }

    pub fn requete(&self) -> NouveauMessage {
        NouveauMessage {
            content: self.contenu.trim().to_string(),
            file_ids: self.pieces.iter().filter_map(|p| p.id.clone()).collect(),
            reply_to: self.reponse_a.clone(),
            poll: self.sondage.clone(),
        }
    }

    pub fn vide(&self) -> bool {
        self.contenu.trim().is_empty() && self.pieces.is_empty() && self.sondage.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaisonEchec {
    DroitManquant,
    TropLong,
    Debit,
    SalonSupprime,
    ReponseIntrouvable,
    FichiersInvalides,
    Autre(String),
}

impl RaisonEchec {
    pub fn de(erreur: &ErreurApi) -> Self {
        match (erreur.statut(), erreur.code()) {
            (403, _) => Self::DroitManquant,
            (413, _) => Self::TropLong,
            (429, _) => Self::Debit,
            (404, _) => Self::SalonSupprime,
            (400, "invalid_reply") => Self::ReponseIntrouvable,
            (400, "invalid_files") => Self::FichiersInvalides,
            (400, "invalid_message") => Self::TropLong,
            _ => Self::Autre(erreur.message().to_string()),
        }
    }

    pub fn libelle(&self) -> String {
        match self {
            Self::DroitManquant => "Tu n'as pas le droit d'écrire ici.".to_string(),
            Self::TropLong => format!("Message trop long ({LONGUEUR_MAX} caractères au plus)."),
            Self::Debit => {
                "Trop de messages d'un coup : réessaie dans quelques secondes.".to_string()
            }
            Self::SalonSupprime => "Cette conversation n'existe plus.".to_string(),
            Self::ReponseIntrouvable => "Le message auquel tu réponds n'existe plus.".to_string(),
            Self::FichiersInvalides => "Une pièce jointe n'est plus disponible.".to_string(),
            Self::Autre(detail) if detail.is_empty() => "Envoi impossible.".to_string(),
            Self::Autre(detail) => format!("Envoi impossible : {detail}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EtatEnvoi {
    EnAttente,
    EnCours,
    Echec(RaisonEchec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnvoiLocal {
    pub id: u64,
    pub conversation: CleConversation,
    pub brouillon: Brouillon,
    pub etat: EtatEnvoi,
    pub cree_a: String,
    pub auteur: Person,
    pub citation: Option<MessageReplyTo>,
}

impl EnvoiLocal {
    pub fn id_local(&self) -> String {
        format!("{PREFIXE_LOCAL}{}", self.id)
    }

    pub fn message(&self) -> Message {
        let (channel_id, dm_id) = match &self.conversation {
            CleConversation::Salon(id) => (Some(id.clone()), None),
            CleConversation::Prive(id) => (None, Some(id.clone())),
        };
        Message {
            id: Some(self.id_local()),
            channel_id,
            dm_id,
            author: Some(self.auteur.clone()),
            content: Some(self.brouillon.contenu.trim().to_string()),
            created_at: Some(self.cree_a.clone()),
            attachments: Some(self.brouillon.pieces.clone()),
            reactions: Some(Vec::new()),
            mentions: Some(Vec::new()),
            reply_to: self.citation.clone(),
            ..Message::default()
        }
    }

    fn correspond(&self, m: &Message) -> bool {
        let reponse = m.reply_to.as_ref().and_then(|r| r.id.as_deref());
        self.etat == EtatEnvoi::EnCours
            && m.content.as_deref().unwrap_or_default() == self.brouillon.contenu.trim()
            && reponse == self.brouillon.reponse_a.as_deref()
    }
}

pub fn est_local(id: &str) -> bool {
    id.starts_with(PREFIXE_LOCAL)
}

pub fn maintenant_iso() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();
    iso_de_ms(ms)
}

fn envois_changes(a: &mut Application, cle: &CleConversation) {
    a.changer(Changement::Messages(ChangementMessages::Envois {
        conversation: cle.clone(),
    }));
}

pub fn ajouter_envoi(
    etat: &mut EtatMessages,
    cle: &CleConversation,
    brouillon: Brouillon,
    auteur: Person,
    cree_a: String,
) -> (u64, Application) {
    let mut a = Application::default();
    etat.prochain_envoi += 1;
    let id = etat.prochain_envoi;
    let citation = brouillon.reponse_a.as_ref().map(|r| {
        let cite = etat.conversation(cle).and_then(|c| c.message(r));
        MessageReplyTo {
            id: Some(r.clone()),
            author: cite.and_then(|m| m.author.clone()),
            content: cite.and_then(|m| m.content.clone()),
        }
    });
    etat.envois.push(EnvoiLocal {
        id,
        conversation: cle.clone(),
        brouillon,
        etat: EtatEnvoi::EnAttente,
        cree_a,
        auteur,
        citation,
    });
    envois_changes(&mut a, cle);
    (id, a)
}

pub fn prochain_a_expedier(etat: &mut EtatMessages) -> (Option<EnvoiLocal>, Application) {
    let mut a = Application::default();
    let Some(envoi) = etat
        .envois
        .iter_mut()
        .find(|e| e.etat == EtatEnvoi::EnAttente)
    else {
        return (None, a);
    };
    envoi.etat = EtatEnvoi::EnCours;
    let copie = envoi.clone();
    envois_changes(&mut a, &copie.conversation);
    (Some(copie), a)
}

pub fn confirmer_envoi(
    etat: &mut EtatMessages,
    contexte: &Contexte,
    id: u64,
    message: &Message,
) -> Application {
    let mut a = Application::default();
    if let Some(i) = etat.envois.iter().position(|e| e.id == id) {
        let envoi = etat.envois.remove(i);
        envois_changes(&mut a, &envoi.conversation);
    }
    message_cree(etat, contexte, message, &mut a);
    a
}

pub fn echec_envoi(etat: &mut EtatMessages, id: u64, erreur: &ErreurApi) -> (bool, Application) {
    let mut a = Application::default();
    let reseau = matches!(erreur, ErreurApi::Reseau(_) | ErreurApi::DelaiDepasse);
    let Some(envoi) = etat.envois.iter_mut().find(|e| e.id == id) else {
        return (reseau, a);
    };
    envoi.etat = if reseau {
        EtatEnvoi::EnAttente
    } else {
        EtatEnvoi::Echec(RaisonEchec::de(erreur))
    };
    let cle = envoi.conversation.clone();
    envois_changes(&mut a, &cle);
    (reseau, a)
}

pub fn reprendre_envoi(etat: &mut EtatMessages, id: u64) -> Application {
    let mut a = Application::default();
    if let Some(envoi) = etat
        .envois
        .iter_mut()
        .find(|e| e.id == id && matches!(e.etat, EtatEnvoi::Echec(_)))
    {
        envoi.etat = EtatEnvoi::EnAttente;
        let cle = envoi.conversation.clone();
        envois_changes(&mut a, &cle);
    }
    a
}

pub fn retirer_envoi(etat: &mut EtatMessages, id: u64) -> Application {
    let mut a = Application::default();
    if let Some(i) = etat
        .envois
        .iter()
        .position(|e| e.id == id && e.etat != EtatEnvoi::EnCours)
    {
        let envoi = etat.envois.remove(i);
        envois_changes(&mut a, &envoi.conversation);
    }
    a
}

pub(super) fn rapprocher_du_temps_reel(
    etat: &mut EtatMessages,
    contexte: &Contexte,
    m: &Message,
    a: &mut Application,
) {
    if auteur(m) != contexte.moi || id_message(m).is_empty() {
        return;
    }
    let Some(cle) = conversation_du_message(m) else {
        return;
    };
    if let Some(i) = etat
        .envois
        .iter()
        .position(|e| e.conversation == cle && e.correspond(m))
    {
        let id = etat.envois[i].id;
        etat.envois.retain(|e| e.id != id);
        envois_changes(a, &cle);
    }
}

#[derive(Debug, Default)]
pub struct RythmeSaisie {
    dernier: Option<(CleConversation, Instant)>,
}

impl RythmeSaisie {
    pub fn doit_signaler(
        &mut self,
        cle: &CleConversation,
        texte: &str,
        maintenant: Instant,
    ) -> bool {
        if texte.trim().is_empty() {
            return false;
        }
        if let Some((c, t)) = &self.dernier {
            if c == cle && maintenant.duration_since(*t) < RYTHME_SAISIE {
                return false;
            }
        }
        self.dernier = Some((cle.clone(), maintenant));
        true
    }

    pub fn oublier(&mut self) {
        self.dernier = None;
    }
}

pub fn compteur(texte: &str) -> Option<(usize, bool)> {
    let n = texte.chars().count();
    (n > SEUIL_COMPTEUR).then_some((n, n >= SEUIL_ALERTE))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::reducteur::{appliquer_evenement, installer, Installation};
    use super::*;
    use crate::http::Refus;
    use crate::store::essais::{depuis_json, evenement};

    fn conversation_chargee() -> (EtatMessages, Contexte, CleConversation) {
        let mut etat = EtatMessages::default();
        let cle = CleConversation::Salon("c1".into());
        let contexte = Contexte {
            moi: "me".into(),
            courante: Some(cle.clone()),
            en_bas: true,
        };
        etat.reserver(&cle, Some(&cle));
        installer(
            &mut etat,
            &contexte,
            &cle,
            Installation::Dernieres {
                messages: vec![depuis_json(
                    json!({"id": "1", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "salut"}),
                )],
                plus_anciens: false,
                rechargee: false,
            },
        );
        (etat, contexte, cle)
    }

    fn moi() -> Person {
        depuis_json(json!({"id": "me", "pseudo": "moi"}))
    }

    fn reponse(id: &str, contenu: &str) -> Message {
        depuis_json(
            json!({"id": id, "channel_id": "c1", "author": {"id": "me", "pseudo": "moi"}, "content": contenu}),
        )
    }

    fn ids(etat: &EtatMessages, cle: &CleConversation) -> Vec<String> {
        etat.conversation(cle)
            .unwrap()
            .messages
            .iter()
            .map(|m| id_message(m).to_string())
            .collect()
    }

    #[test]
    fn la_reponse_http_puis_le_temps_reel_donnent_un_seul_message() {
        let (mut etat, contexte, cle) = conversation_chargee();
        let (id, _) = ajouter_envoi(
            &mut etat,
            &cle,
            Brouillon::texte("coucou"),
            moi(),
            "t".into(),
        );
        assert_eq!(etat.envois_de(&cle).len(), 1);
        let (envoi, _) = prochain_a_expedier(&mut etat);
        assert_eq!(envoi.unwrap().etat, EtatEnvoi::EnCours);
        let m = reponse("2", "coucou");
        confirmer_envoi(&mut etat, &contexte, id, &m);
        appliquer_evenement(
            &mut etat,
            &contexte,
            &evenement("message.created", json!(m)),
            Instant::now(),
        );
        assert_eq!(ids(&etat, &cle), ["1", "2"]);
        assert!(etat.envois_de(&cle).is_empty());
    }

    #[test]
    fn le_temps_reel_puis_la_reponse_http_donnent_un_seul_message() {
        let (mut etat, contexte, cle) = conversation_chargee();
        let (id, _) = ajouter_envoi(
            &mut etat,
            &cle,
            Brouillon::texte(" coucou "),
            moi(),
            "t".into(),
        );
        prochain_a_expedier(&mut etat);
        let m = reponse("2", "coucou");
        let a = appliquer_evenement(
            &mut etat,
            &contexte,
            &evenement("message.created", json!(m)),
            Instant::now(),
        );
        assert!(
            etat.envois_de(&cle).is_empty(),
            "l'envoi local est remplacé dès l'évènement"
        );
        assert!(a
            .changements
            .contains(&Changement::Messages(ChangementMessages::Envois {
                conversation: cle.clone()
            })));
        confirmer_envoi(&mut etat, &contexte, id, &m);
        assert_eq!(ids(&etat, &cle), ["1", "2"]);
    }

    #[test]
    fn un_message_d_un_autre_ou_d_un_autre_texte_ne_remplace_pas_l_envoi() {
        let (mut etat, contexte, cle) = conversation_chargee();
        ajouter_envoi(
            &mut etat,
            &cle,
            Brouillon::texte("coucou"),
            moi(),
            "t".into(),
        );
        prochain_a_expedier(&mut etat);
        let autre: Message = depuis_json(
            json!({"id": "2", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "coucou"}),
        );
        appliquer_evenement(
            &mut etat,
            &contexte,
            &evenement("message.created", json!(autre)),
            Instant::now(),
        );
        appliquer_evenement(
            &mut etat,
            &contexte,
            &evenement("message.created", json!(reponse("3", "autre"))),
            Instant::now(),
        );
        assert_eq!(etat.envois_de(&cle).len(), 1);
    }

    #[test]
    fn un_echec_reseau_remet_l_envoi_en_attente_et_un_refus_le_marque_non_envoye() {
        let (mut etat, _, cle) = conversation_chargee();
        let (id, _) = ajouter_envoi(&mut etat, &cle, Brouillon::texte("a"), moi(), "t".into());
        prochain_a_expedier(&mut etat);
        let (reseau, _) = echec_envoi(&mut etat, id, &ErreurApi::Reseau("coupé".into()));
        assert!(reseau);
        assert_eq!(etat.envoi(id).unwrap().etat, EtatEnvoi::EnAttente);
        prochain_a_expedier(&mut etat);
        let (reseau, _) = echec_envoi(&mut etat, id, &refus(403, "forbidden"));
        assert!(!reseau);
        assert_eq!(
            etat.envoi(id).unwrap().etat,
            EtatEnvoi::Echec(RaisonEchec::DroitManquant)
        );
        reprendre_envoi(&mut etat, id);
        assert_eq!(etat.envoi(id).unwrap().etat, EtatEnvoi::EnAttente);
        retirer_envoi(&mut etat, id);
        assert!(etat.envoi(id).is_none());
    }

    #[test]
    fn l_envoi_local_cite_le_message_auquel_il_repond() {
        let (mut etat, _, cle) = conversation_chargee();
        let brouillon = Brouillon {
            contenu: "oui".into(),
            reponse_a: Some("1".into()),
            ..Brouillon::default()
        };
        let (id, _) = ajouter_envoi(
            &mut etat,
            &cle,
            brouillon,
            moi(),
            "2026-11-20T10:00:00.000Z".into(),
        );
        let m = etat.envoi(id).unwrap().message();
        assert_eq!(m.id.as_deref(), Some("local:1"));
        assert!(est_local("local:1"));
        assert_eq!(m.reply_to.unwrap().content.as_deref(), Some("salut"));
        assert_eq!(m.created_at.as_deref(), Some("2026-11-20T10:00:00.000Z"));
    }

    fn refus(statut: u16, code: &str) -> ErreurApi {
        let r = Refus {
            statut,
            code: code.into(),
            message: "non".into(),
            donnees: Default::default(),
        };
        match statut {
            403 => ErreurApi::Interdit(r),
            404 => ErreurApi::Introuvable(r),
            _ => ErreurApi::Autre(r),
        }
    }

    #[test]
    fn les_refus_du_serveur_sont_traduits() {
        assert_eq!(
            RaisonEchec::de(&refus(403, "forbidden")),
            RaisonEchec::DroitManquant
        );
        assert_eq!(
            RaisonEchec::de(&refus(413, "too_large")),
            RaisonEchec::TropLong
        );
        assert_eq!(
            RaisonEchec::de(&refus(429, "too_many_messages")),
            RaisonEchec::Debit
        );
        assert_eq!(
            RaisonEchec::de(&refus(404, "not_found")),
            RaisonEchec::SalonSupprime
        );
        assert_eq!(
            RaisonEchec::de(&refus(400, "invalid_reply")),
            RaisonEchec::ReponseIntrouvable
        );
        assert!(RaisonEchec::Debit.libelle().contains("réessaie"));
        assert!(RaisonEchec::TropLong.libelle().contains("4000"));
    }

    #[test]
    fn le_signal_de_saisie_suit_le_rythme_du_front() {
        let mut rythme = RythmeSaisie::default();
        let c1 = CleConversation::Salon("c1".into());
        let c2 = CleConversation::Salon("c2".into());
        let t = Instant::now();
        assert!(!rythme.doit_signaler(&c1, "   ", t));
        assert!(rythme.doit_signaler(&c1, "a", t));
        assert!(!rythme.doit_signaler(&c1, "ab", t + Duration::from_millis(2900)));
        assert!(rythme.doit_signaler(&c1, "abc", t + Duration::from_millis(3000)));
        assert!(rythme.doit_signaler(&c2, "a", t + Duration::from_millis(3100)));
    }

    #[test]
    fn le_compteur_apparait_a_l_approche_de_la_limite() {
        assert_eq!(compteur(&"a".repeat(3500)), None);
        assert_eq!(compteur(&"a".repeat(3501)), Some((3501, false)));
        assert_eq!(compteur(&"é".repeat(3900)), Some((3900, true)));
    }

    #[test]
    fn un_brouillon_vide_ne_part_pas() {
        assert!(Brouillon::texte("  \n ").vide());
        assert!(!Brouillon::texte("salut").vide());
        let b = Brouillon {
            pieces: vec![Attachment {
                id: Some("f1".into()),
                ..Attachment::default()
            }],
            ..Brouillon::default()
        };
        assert!(!b.vide());
        assert_eq!(b.requete().file_ids, ["f1"]);
        assert_eq!(
            EnvoiLocal {
                id: 1,
                conversation: CleConversation::Salon("c1".into()),
                brouillon: b,
                etat: EtatEnvoi::EnAttente,
                cree_a: String::new(),
                auteur: Person::default(),
                citation: None,
            }
            .message()
            .attachments
            .map(|a| a.len()),
            Some(1),
            "le message local montre ses pièces jointes"
        );
        assert_eq!(Brouillon::texte(" salut \n").requete().content, "salut");
    }
}
