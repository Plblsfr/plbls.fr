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
    appliquer_evenement, expirer_saisies, installer, installer_lectures, mettre_en_evidence,
    ChangementMessages, Contexte, Installation,
};

use super::etat::CleConversation;
use super::reducteur::{definir_bas_de_liste, Application};
use super::Store;
use crate::temps_reel::evenements::Lecture;
use crate::temps_reel::Evenement;

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
