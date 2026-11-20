mod chargement;
mod envois;
mod etat;
mod reducteur;

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use tokio::time::Instant;

pub use envois::{
    ajouter_envoi, compteur, confirmer_envoi, echec_envoi, est_local, maintenant_iso,
    prochain_a_expedier, reprendre_envoi, retirer_envoi, Brouillon, EnvoiLocal, EtatEnvoi,
    RaisonEchec, RythmeSaisie, LONGUEUR_MAX, PREFIXE_LOCAL, RYTHME_SAISIE, SEUIL_ALERTE,
    SEUIL_COMPTEUR,
};
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
use super::reducteur::{definir_bas_de_liste, marquer_non_lu, Application};
use super::Store;
use crate::http::ErreurApi;
use crate::temps_reel::evenements::{Lecture, ReactionModifiee};
use crate::temps_reel::Evenement;
use crate::types::{Message, Person};

impl Store {
    pub fn envois(&self, cle: &CleConversation) -> Vec<EnvoiLocal> {
        self.lire_messages(|m| m.envois_de(cle).into_iter().cloned().collect())
    }

    pub fn deposer(&self, cle: &CleConversation, brouillon: Brouillon) -> Option<u64> {
        if brouillon.vide() {
            return None;
        }
        let auteur: Person = self.lire(|e| Person {
            id: Some(e.moi().to_string()),
            pseudo: Some(e.pseudo().to_string()),
        });
        let cree_a = self
            .maintenant_ms()
            .map(crate::format::iso_de_ms)
            .unwrap_or_else(maintenant_iso);
        Some(self.modifier_messages(|m, _| ajouter_envoi(m, cle, brouillon, auteur, cree_a)))
    }

    pub async fn envoyer(&self, cle: &CleConversation, brouillon: Brouillon) -> Option<u64> {
        let id = self.deposer(cle, brouillon)?;
        self.expedier().await;
        Some(id)
    }

    pub async fn expedier(&self) {
        let interne = &self.interne;
        loop {
            interne.relance.store(true, Ordering::SeqCst);
            let Ok(verrou) = interne.expedition.try_lock() else {
                return;
            };
            interne.relance.store(false, Ordering::SeqCst);
            while let Some(envoi) = self.modifier_messages(|m, _| prochain_a_expedier(m)) {
                let requete = envoi.brouillon.requete();
                match self
                    .client()
                    .envoyer_message(&envoi.conversation, &requete)
                    .await
                {
                    Ok(message) => self.modifier_messages(|m, contexte| {
                        ((), confirmer_envoi(m, contexte, envoi.id, &message))
                    }),
                    Err(erreur) => {
                        let reseau =
                            self.modifier_messages(|m, _| echec_envoi(m, envoi.id, &erreur));
                        if reseau {
                            drop(verrou);
                            return;
                        }
                    }
                }
            }
            drop(verrou);
            if !interne.relance.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    pub async fn reessayer(&self, id: u64) {
        self.modifier_messages(|m, _| ((), reprendre_envoi(m, id)));
        self.expedier().await;
    }

    pub fn abandonner(&self, id: u64) {
        self.modifier_messages(|m, _| ((), retirer_envoi(m, id)));
    }

    pub async fn modifier_message(
        &self,
        message: &str,
        contenu: &str,
        fichiers: Option<Vec<String>>,
    ) -> Result<(), ErreurApi> {
        let modifie = self
            .client()
            .modifier_message(message, contenu.trim(), fichiers)
            .await?;
        self.appliquer_aux_messages(&Evenement::MessageModifie(modifie));
        Ok(())
    }

    pub async fn supprimer_message(
        &self,
        cle: &CleConversation,
        message: &str,
    ) -> Result<(), ErreurApi> {
        self.client().supprimer_message(message).await?;
        self.modifier_messages(|m, _| {
            let mut a = Application::default();
            reducteur::supprimer(m, cle, &[message.to_string()], &mut a);
            ((), a)
        });
        Ok(())
    }

    pub async fn basculer_reaction(&self, message: &str, emoji: &str) -> Result<(), ErreurApi> {
        let (moi, pseudo) = self.lire(|e| (e.moi().to_string(), e.pseudo().to_string()));
        let avait = self.lire_messages(|m| {
            m.conversations
                .values()
                .find_map(|c| c.message(message))
                .is_some_and(|x| {
                    x.reactions
                        .iter()
                        .flatten()
                        .any(|r| r.emoji.as_deref() == Some(emoji) && r.me == Some(true))
                })
        });
        let evenement = |added| {
            Evenement::ReactionModifiee(ReactionModifiee {
                message_id: message.to_string(),
                emoji: emoji.to_string(),
                user_id: moi.clone(),
                pseudo: Some(pseudo.clone()),
                added,
            })
        };
        self.appliquer_aux_messages(&evenement(!avait));
        let resultat = if avait {
            self.client().retirer_reaction(message, emoji).await
        } else {
            self.client().reagir(message, emoji).await
        };
        if resultat.is_err() {
            self.appliquer_aux_messages(&evenement(avait));
        }
        resultat
    }

    pub async fn epingler(&self, message: &str, epingle: bool) -> Result<(), ErreurApi> {
        self.client().epingler(message, epingle).await
    }

    pub fn marquer_non_lu(&self, cle: &CleConversation, depuis: &str) -> i64 {
        let moi = self.lire(|e| e.moi().to_string());
        let nombre = self.lire_messages(|m| {
            m.conversation(cle)
                .and_then(|c| {
                    let debut = c.position(depuis).ok()?;
                    Some(
                        c.messages[debut..]
                            .iter()
                            .filter(|x| auteur(x) != moi && x.system != Some(true))
                            .count(),
                    )
                })
                .unwrap_or(0)
        }) as i64;
        self.appliquer_application(|etat| marquer_non_lu(etat, cle, nombre));
        nombre
    }

    pub async fn signaler_saisie(&self, cle: &CleConversation, texte: &str) -> bool {
        let signaler = self
            .interne
            .rythme
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .doit_signaler(cle, texte, Instant::now());
        if signaler {
            let _ = self.client().signaler_saisie(cle).await;
        }
        signaler
    }

    pub fn oublier_saisie(&self) {
        self.interne
            .rythme
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .oublier();
    }

    pub fn dernier_de_moi(&self, cle: &CleConversation) -> Option<Message> {
        let moi = self.lire(|e| e.moi().to_string());
        self.lire_messages(|m| {
            m.conversation(cle).and_then(|c| {
                c.messages
                    .iter()
                    .rev()
                    .find(|x| auteur(x) == moi && x.system != Some(true) && x.poll.is_none())
                    .cloned()
            })
        })
    }

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
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::http::Client;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use crate::store::essais::{depuis_json, etat_de_depart};
    use crate::store::Changement;

    fn store_sur(client: Client) -> (Store, CleConversation) {
        let store = Store::nouveau(client, &[]);
        store.installer(etat_de_depart());
        let cle = CleConversation::Salon("c1".into());
        store.installer_conversation(
            &cle,
            vec![depuis_json(json!({"id": "1", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "salut"}))],
            false,
        );
        (store, cle)
    }

    fn contenus(store: &Store, cle: &CleConversation) -> Vec<String> {
        store
            .conversation(cle)
            .unwrap()
            .messages
            .iter()
            .map(|m| m.content.clone().unwrap_or_default())
            .collect()
    }

    fn serveur_d_echo(serveur: &ServeurFactice) {
        let compteur = Arc::new(AtomicUsize::new(10));
        serveur.router_dynamique("POST", "/channels/c1/messages", move |r| {
            let id = compteur.fetch_add(1, Ordering::SeqCst);
            let corps = r.json();
            Reponse::json(
                201,
                json!({"id": id.to_string(), "channel_id": "c1", "author": {"id": "me", "pseudo": "moi"}, "content": corps["content"], "reply_to": null}),
            )
        });
    }

    #[tokio::test]
    async fn un_envoi_apparait_tout_de_suite_puis_devient_le_message_du_serveur() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        serveur_d_echo(&serveur);
        let (store, cle) = store_sur(serveur.client());
        let mut abonne = store.abonner();
        let id = store.deposer(&cle, Brouillon::texte("coucou")).unwrap();
        assert_eq!(store.envois(&cle)[0].etat, EtatEnvoi::EnAttente);
        assert_eq!(
            store.envois(&cle)[0].message().content.as_deref(),
            Some("coucou")
        );
        store.expedier().await;
        assert!(store.envois(&cle).is_empty());
        assert_eq!(contenus(&store, &cle), ["salut", "coucou"]);
        let recus: Vec<Changement> = std::iter::from_fn(|| abonne.try_recv().ok())
            .flatten()
            .collect();
        assert!(
            recus.contains(&Changement::Messages(ChangementMessages::Envois {
                conversation: cle.clone()
            }))
        );
        assert_eq!(store.lire_messages(|m| m.envoi(id).cloned()), None);
        let requete = serveur
            .requetes()
            .into_iter()
            .find(|r| r.methode == "POST")
            .unwrap();
        assert_eq!(requete.json(), json!({"content": "coucou", "file_ids": []}));
    }

    #[tokio::test]
    async fn trois_messages_saisis_hors_ligne_partent_dans_l_ordre_au_retour_du_reseau() {
        let lieu = {
            let libre = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            libre.local_addr().unwrap().to_string()
        };
        let client =
            Client::avec_reglages(&format!("http://{lieu}"), ServeurFactice::reglages()).unwrap();
        let (store, cle) = store_sur(client);
        for texte in ["un", "deux", "trois"] {
            store.envoyer(&cle, Brouillon::texte(texte)).await;
        }
        let etats: Vec<EtatEnvoi> = store.envois(&cle).into_iter().map(|e| e.etat).collect();
        assert_eq!(
            etats,
            [
                EtatEnvoi::EnAttente,
                EtatEnvoi::EnAttente,
                EtatEnvoi::EnAttente
            ]
        );
        let serveur = ServeurFactice::demarrer_sur(&lieu, [Reponse::vide(500)]).await;
        serveur_d_echo(&serveur);
        store.expedier().await;
        assert!(store.envois(&cle).is_empty());
        assert_eq!(contenus(&store, &cle), ["salut", "un", "deux", "trois"]);
        let envoyes: Vec<String> = serveur
            .requetes()
            .into_iter()
            .filter(|r| r.methode == "POST")
            .map(|r| r.json()["content"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(envoyes, ["un", "deux", "trois"]);
    }

    #[tokio::test]
    async fn un_envoi_refuse_reste_non_envoye_avec_la_raison() {
        for (statut, code, raison) in [
            (403, "forbidden", RaisonEchec::DroitManquant),
            (413, "too_large", RaisonEchec::TropLong),
            (429, "too_many_messages", RaisonEchec::Debit),
        ] {
            let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
            let accepte = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let bascule = accepte.clone();
            serveur.router_dynamique("POST", "/channels/c1/messages", move |r| {
                if bascule.load(Ordering::SeqCst) {
                    Reponse::json(201, json!({"id": "9", "channel_id": "c1", "author": {"id": "me", "pseudo": "moi"}, "content": r.json()["content"]}))
                } else {
                    Reponse::json(statut, json!({"error": code, "message": "non"}))
                }
            });
            let (store, cle) = store_sur(serveur.client());
            let id = store.envoyer(&cle, Brouillon::texte("a")).await.unwrap();
            let envoi = store.lire_messages(|m| m.envoi(id).cloned()).unwrap();
            assert_eq!(
                envoi.etat,
                EtatEnvoi::Echec(raison.clone()),
                "statut {statut}"
            );
            assert_eq!(contenus(&store, &cle), ["salut"]);
            accepte.store(true, Ordering::SeqCst);
            store.reessayer(id).await;
            assert!(store.envois(&cle).is_empty());
            assert_eq!(contenus(&store, &cle), ["salut", "a"]);
        }
    }

    #[tokio::test]
    async fn un_refus_ne_bloque_pas_les_envois_suivants_et_supprimer_l_oublie() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        let compteur = Arc::new(AtomicUsize::new(0));
        serveur.router_dynamique("POST", "/channels/c1/messages", move |r| {
            let n = compteur.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                return Reponse::json(429, json!({"error": "too_many_messages", "message": "non"}));
            }
            Reponse::json(201, json!({"id": (20 + n).to_string(), "channel_id": "c1", "author": {"id": "me", "pseudo": "moi"}, "content": r.json()["content"]}))
        });
        let (store, cle) = store_sur(serveur.client());
        let premier = store.deposer(&cle, Brouillon::texte("un")).unwrap();
        store.deposer(&cle, Brouillon::texte("deux"));
        store.expedier().await;
        assert_eq!(contenus(&store, &cle), ["salut", "deux"]);
        assert_eq!(store.envois(&cle).len(), 1);
        store.abandonner(premier);
        assert!(store.envois(&cle).is_empty());
    }

    #[tokio::test]
    async fn modifier_applique_la_reponse_et_garde_mes_reactions() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        serveur.router(
            "PATCH",
            "/messages/1",
            Reponse::json(200, json!({"id": "1", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "modifié", "edited_at": "x", "reactions": [{"emoji": "👍", "count": 2, "me": false}]})),
        );
        let (store, cle) = store_sur(serveur.client());
        store.modifier_messages(|m, _| {
            m.conversations.get_mut(&cle).unwrap().messages[0].reactions = Some(vec![depuis_json(
                json!({"emoji": "👍", "count": 2, "me": true}),
            )]);
            ((), Application::default())
        });
        store
            .modifier_message("1", " modifié ", None)
            .await
            .unwrap();
        let m = store.conversation(&cle).unwrap().messages[0].clone();
        assert_eq!(m.content.as_deref(), Some("modifié"));
        assert_eq!(m.reactions.unwrap()[0].me, Some(true));
        let requete = serveur
            .requetes()
            .into_iter()
            .find(|r| r.methode == "PATCH")
            .unwrap();
        assert_eq!(requete.json(), json!({"content": "modifié"}));
    }

    #[tokio::test]
    async fn supprimer_retire_le_message_a_la_confirmation_et_le_garde_en_cas_d_erreur() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        serveur.router(
            "DELETE",
            "/messages/1",
            Reponse::json(403, json!({"error": "forbidden", "message": "non"})),
        );
        let (store, cle) = store_sur(serveur.client());
        assert!(store.supprimer_message(&cle, "1").await.is_err());
        assert_eq!(contenus(&store, &cle), ["salut"]);
        serveur.router("DELETE", "/messages/1", Reponse::vide(204));
        store.supprimer_message(&cle, "1").await.unwrap();
        assert!(contenus(&store, &cle).is_empty());
    }

    fn reactions_de(store: &Store, cle: &CleConversation) -> Vec<(String, i64, bool)> {
        store.conversation(cle).unwrap().messages[0]
            .reactions
            .iter()
            .flatten()
            .map(|r| {
                (
                    r.emoji.clone().unwrap_or_default(),
                    r.count.unwrap_or(0),
                    r.me == Some(true),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn basculer_une_reaction_l_ajoute_puis_la_retire_et_revient_en_arriere_en_cas_d_erreur() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        let chemin = "/messages/1/reactions/%F0%9F%91%8D";
        serveur.router("PUT", chemin, Reponse::vide(204));
        serveur.router("DELETE", chemin, Reponse::vide(204));
        let (store, cle) = store_sur(serveur.client());
        store.basculer_reaction("1", "👍").await.unwrap();
        assert_eq!(reactions_de(&store, &cle), [("👍".to_string(), 1, true)]);
        store.appliquer_aux_messages(&Evenement::ReactionModifiee(ReactionModifiee {
            message_id: "1".into(),
            emoji: "👍".into(),
            user_id: "me".into(),
            pseudo: Some("moi".into()),
            added: true,
        }));
        assert_eq!(reactions_de(&store, &cle), [("👍".to_string(), 1, true)]);
        store.basculer_reaction("1", "👍").await.unwrap();
        assert!(reactions_de(&store, &cle).is_empty());
        serveur.router(
            "PUT",
            chemin,
            Reponse::json(403, json!({"error": "forbidden", "message": "non"})),
        );
        assert!(store.basculer_reaction("1", "👍").await.is_err());
        assert!(reactions_de(&store, &cle).is_empty());
        let methodes: Vec<String> = serveur
            .requetes()
            .into_iter()
            .filter(|r| r.chemin == chemin)
            .map(|r| r.methode)
            .collect();
        assert_eq!(methodes, ["PUT", "DELETE", "PUT"]);
    }

    #[tokio::test]
    async fn marquer_comme_non_lu_compte_les_messages_des_autres_depuis_celui_choisi() {
        let (store, cle) = store_sur(Client::nouveau("http://127.0.0.1:9").unwrap());
        store.installer_conversation(
            &cle,
            vec![
                depuis_json(json!({"id": "1", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "a"})),
                depuis_json(json!({"id": "2", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "b"})),
                depuis_json(json!({"id": "3", "channel_id": "c1", "author": {"id": "me", "pseudo": "moi"}, "content": "c"})),
                depuis_json(json!({"id": "4", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "d"})),
            ],
            false,
        );
        store.ouvrir(Some(cle.clone())).await;
        let mut abonne = store.abonner();
        assert_eq!(store.marquer_non_lu(&cle, "2"), 2);
        assert_eq!(store.lire(|e| e.non_lus_de(&cle)), 2);
        assert!(!store.lire(|e| e.en_bas));
        assert!(abonne.try_recv().unwrap().contains(&Changement::NonLus {
            conversation: cle.clone(),
            serveur: store.lire(|e| e.serveur_de(&cle)),
        }));
        assert_eq!(store.marquer_non_lu(&cle, "absent"), 0);
        assert_eq!(store.lire(|e| e.non_lus_de(&cle)), 2);
        store.signaler_lecture(true, None).await;
        assert_eq!(store.lire(|e| e.non_lus_de(&cle)), 0);
    }

    #[tokio::test]
    async fn le_signal_de_saisie_part_au_plus_toutes_les_trois_secondes() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        serveur.router("POST", "/channels/c1/typing", Reponse::vide(204));
        let (store, cle) = store_sur(serveur.client());
        assert!(store.signaler_saisie(&cle, "a").await);
        assert!(!store.signaler_saisie(&cle, "ab").await);
        assert!(!store.signaler_saisie(&cle, "").await);
        let envoyes = serveur
            .requetes()
            .iter()
            .filter(|r| r.chemin == "/channels/c1/typing")
            .count();
        assert_eq!(envoyes, 1);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    #[tokio::test]
    async fn le_dernier_message_de_moi_est_celui_que_fleche_haut_modifie() {
        let (store, cle) = store_sur(Client::nouveau("http://127.0.0.1:9").unwrap());
        assert!(store.dernier_de_moi(&cle).is_none());
        store.installer_conversation(
            &cle,
            vec![
                depuis_json(json!({"id": "1", "channel_id": "c1", "author": {"id": "me", "pseudo": "moi"}, "content": "a"})),
                depuis_json(json!({"id": "2", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "b"})),
            ],
            false,
        );
        assert_eq!(
            store.dernier_de_moi(&cle).unwrap().content.as_deref(),
            Some("a")
        );
    }

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
