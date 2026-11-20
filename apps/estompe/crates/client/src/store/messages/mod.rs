mod chargement;
mod etat;
mod reducteur;

use std::collections::BTreeMap;

use tokio::time::Instant;

pub use etat::{
    auteur, comparer_ids, conversation_du_message, est_compte_estompe, id_message, lignes_privees,
    Conversation, EtatMessages, LignePrivee, SaisieEnCours, CONVERSATIONS_EN_CACHE, DUREE_SAISIE,
    ID_COMPTE_ESTOMPE, LIMITE_PAGE, LIMITE_PAGE_LONGUE, MESSAGES_PAR_CONVERSATION,
    MESSAGES_SAUT_MAX, PAGES_RATTRAPAGE_MAX,
};
pub use reducteur::{
    appliquer_evenement, expirer_saisies, installer, installer_epingles, installer_lectures,
    mettre_en_evidence, ChangementMessages, Contexte, Installation,
};

use super::etat::CleConversation;
use super::reducteur::{definir_bas_de_liste, Application};
use super::Store;
use crate::http::ErreurApi;
use crate::temps_reel::evenements::Lecture;
use crate::temps_reel::Evenement;
use crate::types::Message;

impl Store {
    pub fn lire_messages<R>(&self, lecture: impl FnOnce(&EtatMessages) -> R) -> R {
        lecture(
            &self
                .interne
                .messages
                .read()
                .unwrap_or_else(|e| e.into_inner()),
        )
    }

    pub fn conversation(&self, cle: &CleConversation) -> Option<Conversation> {
        self.lire_messages(|m| m.conversation(cle).cloned())
    }

    pub fn installer_conversation(
        &self,
        cle: &CleConversation,
        messages: Vec<Message>,
        plus_anciens: bool,
    ) {
        self.modifier_messages(|m, contexte| {
            m.reserver(cle, contexte.courante.as_ref());
            let installation = Installation::Dernieres {
                messages,
                plus_anciens,
                rechargee: false,
            };
            ((), installer(m, contexte, cle, installation))
        });
    }

    pub fn epingles(&self, cle: &CleConversation) -> Option<Vec<Message>> {
        self.lire_messages(|m| m.epingles_de(cle).map(<[Message]>::to_vec))
    }

    pub async fn charger_epingles(&self, cle: &CleConversation) -> Result<(), ErreurApi> {
        let messages = self.client().epingles(cle).await?;
        self.installer_epingles(cle, messages);
        Ok(())
    }

    pub fn installer_epingles(&self, cle: &CleConversation, messages: Vec<Message>) {
        self.modifier_messages(|m, _| ((), installer_epingles(m, cle, messages)));
    }

    pub fn oublier_epingles(&self) {
        self.modifier_messages(|m, _| {
            m.epingles = None;
            ((), Application::default())
        });
    }

    pub fn saisies(&self, cle: &CleConversation) -> Vec<String> {
        self.lire_messages(|m| m.saisies_de(cle))
    }

    pub fn lecteurs(&self, cle: &CleConversation) -> BTreeMap<String, Vec<Lecture>> {
        let moi = self.lire(|e| e.moi().to_string());
        self.lire_messages(|m| m.lecteurs(cle, &moi))
    }

    pub fn lignes_privees(&self) -> Vec<LignePrivee> {
        self.lire(|etat| self.lire_messages(|m| lignes_privees(etat, m)))
    }

    pub(super) fn modifier_messages<R>(
        &self,
        modification: impl FnOnce(&mut EtatMessages, &Contexte) -> (R, Application),
    ) -> R {
        let contexte = self.lire(Contexte::de);
        let (resultat, application) = {
            let mut messages = self
                .interne
                .messages
                .write()
                .unwrap_or_else(|e| e.into_inner());
            modification(&mut messages, &contexte)
        };
        self.publier(application.changements);
        resultat
    }

    pub(super) fn appliquer_aux_messages(&self, evenement: &Evenement) {
        let maintenant = Instant::now();
        self.modifier_messages(|m, contexte| {
            ((), appliquer_evenement(m, contexte, evenement, maintenant))
        });
    }

    pub fn expirer_saisies(&self) {
        let maintenant = Instant::now();
        self.modifier_messages(|m, _| ((), expirer_saisies(m, maintenant)));
    }

    pub fn prochaine_expiration(&self) -> Option<Instant> {
        self.lire_messages(EtatMessages::prochaine_expiration)
    }

    pub async fn signaler_lecture(&self, en_bas: bool, ancre: Option<String>) {
        self.modifier_messages(|m, contexte| {
            if let Some(c) = contexte
                .courante
                .as_ref()
                .and_then(|cle| m.conversations.get_mut(cle))
            {
                c.ancre = ancre;
            }
            ((), Application::default())
        });
        let effets = self.appliquer_application(|etat| definir_bas_de_liste(etat, en_bas));
        self.executer(effets).await;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::http::Client;
    use crate::store::essais::depuis_json;
    use crate::store::Changement;

    #[test]
    fn installer_une_conversation_la_rend_lisible_et_l_annonce() {
        let store = Store::nouveau(Client::nouveau("http://127.0.0.1:9").unwrap(), &[]);
        let mut abonne = store.abonner();
        let cle = CleConversation::Salon("c1".into());
        let messages: Vec<Message> = ["2", "1"]
            .iter()
            .map(|id| depuis_json(json!({"id": id, "channel_id": "c1"})))
            .collect();
        store.installer_conversation(&cle, messages, true);
        let c = store.conversation(&cle).unwrap();
        assert!(c.chargee && c.fin_atteinte && !c.debut_atteint);
        assert_eq!(c.premier_id(), Some("1"));
        assert_eq!(
            abonne.try_recv().unwrap(),
            [Changement::Messages(ChangementMessages::Rechargee {
                conversation: cle
            })]
        );
    }
}
