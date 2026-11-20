use super::etat::{
    comparer_ids, id_message, LIMITE_PAGE, LIMITE_PAGE_LONGUE, MESSAGES_PAR_CONVERSATION,
    MESSAGES_SAUT_MAX, PAGES_RATTRAPAGE_MAX,
};
use super::reducteur::{installer, installer_lectures, mettre_en_evidence, Installation};
use crate::appels::Page;
use crate::http::ErreurApi;
use crate::store::etat::CleConversation;
use crate::store::reducteur::{Application, Changement};
use crate::store::Store;
use crate::types::{Message, MessagePage};

fn decouper(page: MessagePage) -> (Vec<Message>, bool) {
    (
        page.messages.unwrap_or_default(),
        page.has_more == Some(true),
    )
}

struct Fenetre {
    premier: String,
    dernier: String,
    fin_atteinte: bool,
}

fn page_avant(avant: &str, limite: u32) -> Page {
    Page {
        limite,
        avant: Some(avant.to_string()),
        apres: None,
    }
}

fn page_apres(apres: &str, limite: u32) -> Page {
    Page {
        limite,
        avant: None,
        apres: Some(apres.to_string()),
    }
}

impl Store {
    fn installer_messages(&self, cle: &CleConversation, installation: Installation) {
        self.modifier_messages(|m, contexte| ((), installer(m, contexte, cle, installation)));
    }

    fn liberer(&self, cle: &CleConversation) {
        self.modifier_messages(|m, _| {
            match m.conversations.get_mut(cle) {
                Some(c) if !c.chargee => {
                    m.oublier(cle);
                }
                Some(c) => c.en_chargement = false,
                None => {}
            }
            ((), Application::default())
        });
    }

    fn reserver_chargement<T>(
        &self,
        cle: &CleConversation,
        borne: impl FnOnce(&super::Conversation) -> Option<T>,
    ) -> Option<T> {
        self.modifier_messages(|m, _| {
            let borne = match m.conversations.get_mut(cle) {
                Some(c) if c.chargee && !c.en_chargement => borne(c),
                _ => None,
            };
            if let (Some(_), Some(c)) = (&borne, m.conversations.get_mut(cle)) {
                c.en_chargement = true;
            }
            (borne, Application::default())
        })
    }

    pub async fn charger_conversation(&self, cle: &CleConversation) -> Result<(), ErreurApi> {
        let a_charger = self.modifier_messages(|m, contexte| {
            let c = m.reserver(cle, contexte.courante.as_ref());
            let a_charger = !c.chargee && !c.en_chargement;
            if a_charger {
                c.en_chargement = true;
            }
            (a_charger, Application::default())
        });
        if !a_charger {
            return Ok(());
        }
        let page = match self.client().messages_de(cle, &Page::default()).await {
            Ok(page) => page,
            Err(erreur) => {
                self.liberer(cle);
                return Err(erreur);
            }
        };
        let (messages, plus_anciens) = decouper(page);
        self.installer_messages(
            cle,
            Installation::Dernieres {
                messages,
                plus_anciens,
                rechargee: false,
            },
        );
        if let Ok(lectures) = self.client().lectures(cle).await {
            self.modifier_messages(|m, _| ((), installer_lectures(m, cle, lectures)));
        }
        Ok(())
    }

    pub async fn charger_plus_anciens(&self, cle: &CleConversation) -> Result<bool, ErreurApi> {
        let Some(avant) = self.reserver_chargement(cle, |c| {
            c.premier_id()
                .filter(|_| !c.debut_atteint)
                .map(str::to_string)
        }) else {
            return Ok(false);
        };
        match self
            .client()
            .messages_de(cle, &page_avant(&avant, LIMITE_PAGE))
            .await
        {
            Ok(page) => {
                let (messages, plus_anciens) = decouper(page);
                self.installer_messages(
                    cle,
                    Installation::PlusAnciens {
                        avant,
                        messages,
                        plus_anciens,
                    },
                );
                Ok(true)
            }
            Err(erreur) => {
                self.liberer(cle);
                Err(erreur)
            }
        }
    }

    pub async fn charger_plus_recents(&self, cle: &CleConversation) -> Result<bool, ErreurApi> {
        let Some(apres) = self.reserver_chargement(cle, |c| {
            c.dernier_id()
                .filter(|_| !c.fin_atteinte)
                .map(str::to_string)
        }) else {
            return Ok(false);
        };
        match self
            .client()
            .messages_de(cle, &page_apres(&apres, LIMITE_PAGE))
            .await
        {
            Ok(page) => {
                let (messages, plus_recents) = decouper(page);
                self.installer_messages(
                    cle,
                    Installation::PlusRecents {
                        apres,
                        messages,
                        plus_recents,
                    },
                );
                Ok(true)
            }
            Err(erreur) => {
                self.liberer(cle);
                Err(erreur)
            }
        }
    }

    pub async fn sauter_a(&self, cle: &CleConversation, message: &str) -> Result<bool, ErreurApi> {
        let deja = self.modifier_messages(|m, contexte| {
            let a = mettre_en_evidence(m, cle, message);
            if a.changements.is_empty() {
                let c = m.reserver(cle, contexte.courante.as_ref());
                if c.en_chargement {
                    return (None, a);
                }
                c.en_chargement = true;
                return (Some(false), a);
            }
            (Some(true), a)
        });
        match deja {
            Some(true) => Ok(true),
            None => Ok(false),
            Some(false) => {
                let resultat = self.charger_autour(cle, message, true).await;
                if !matches!(resultat, Ok(true)) {
                    self.liberer(cle);
                }
                resultat
            }
        }
    }

    async fn charger_autour(
        &self,
        cle: &CleConversation,
        message: &str,
        mise_en_evidence: bool,
    ) -> Result<bool, ErreurApi> {
        let Some(precedent) = message.parse::<i64>().ok().and_then(|n| n.checked_sub(1)) else {
            return Ok(false);
        };
        let mut apres = precedent.to_string();
        let mut recents: Vec<Message> = Vec::new();
        let plus_recents = loop {
            let page = self
                .client()
                .messages_de(cle, &page_apres(&apres, LIMITE_PAGE_LONGUE))
                .await?;
            let (messages, encore) = decouper(page);
            if recents.is_empty() && messages.first().map(id_message) != Some(message) {
                return Ok(false);
            }
            recents.extend(messages);
            match recents.last() {
                Some(dernier) if encore && recents.len() < MESSAGES_SAUT_MAX => {
                    apres = id_message(dernier).to_string();
                }
                _ => break encore,
            }
        };
        let (mut messages, plus_anciens) = decouper(
            self.client()
                .messages_de(cle, &page_avant(message, LIMITE_PAGE))
                .await?,
        );
        messages.extend(recents);
        self.installer_messages(
            cle,
            Installation::Autour {
                message: message.to_string(),
                messages,
                plus_anciens,
                plus_recents,
                mise_en_evidence,
            },
        );
        Ok(true)
    }

    pub async fn rattraper_messages(&self) {
        let cles: Vec<CleConversation> = self.modifier_messages(|m, _| {
            let mut a = Application::default();
            if !m.remontees_privees.is_empty() {
                m.remontees_privees.clear();
                a.changer(Changement::MessagesPrives);
            }
            let cles = m
                .conversations
                .iter()
                .filter(|(_, c)| c.chargee && !c.en_chargement)
                .map(|(cle, _)| cle.clone())
                .collect();
            (cles, a)
        });
        for cle in cles {
            if self.rattraper(&cle).await.is_ok() {
                if let Ok(lectures) = self.client().lectures(&cle).await {
                    self.modifier_messages(|m, _| ((), installer_lectures(m, &cle, lectures)));
                }
            }
        }
    }

    async fn rattraper(&self, cle: &CleConversation) -> Result<(), ErreurApi> {
        let Some(fenetre) = self.reserver_chargement(cle, |c| {
            Some(Fenetre {
                premier: c.premier_id().unwrap_or_default().to_string(),
                dernier: c.dernier_id().unwrap_or_default().to_string(),
                fin_atteinte: c.fin_atteinte,
            })
        }) else {
            return Ok(());
        };
        let mut resultat = self.relire_fenetre(cle, &fenetre).await;
        if resultat.is_ok() && fenetre.fin_atteinte {
            resultat = self.rattraper_depuis(cle, &fenetre.dernier).await;
        }
        self.liberer(cle);
        resultat
    }

    async fn relire_fenetre(&self, cle: &CleConversation, f: &Fenetre) -> Result<(), ErreurApi> {
        let Some(precedent) = f.premier.parse::<i64>().ok().and_then(|n| n.checked_sub(1)) else {
            return Ok(());
        };
        let mut apres = precedent.to_string();
        let mut relus: Vec<Message> = Vec::new();
        let pages = MESSAGES_PAR_CONVERSATION / LIMITE_PAGE_LONGUE as usize + 1;
        for _ in 0..pages {
            let (messages, encore) = decouper(
                self.client()
                    .messages_de(cle, &page_apres(&apres, LIMITE_PAGE_LONGUE))
                    .await?,
            );
            relus.extend(messages);
            match relus.last() {
                Some(d) if encore && comparer_ids(id_message(d), &f.dernier).is_lt() => {
                    apres = id_message(d).to_string()
                }
                _ => {
                    apres.clear();
                    break;
                }
            }
        }
        let jusqu_a = if apres.is_empty() {
            f.dernier.clone()
        } else {
            apres
        };
        relus.retain(|m| comparer_ids(id_message(m), &jusqu_a).is_le());
        self.installer_messages(
            cle,
            Installation::Relecture {
                de: f.premier.clone(),
                jusqu_a,
                messages: relus,
            },
        );
        self.modifier_messages(|m, _| {
            if let Some(c) = m.conversations.get_mut(cle) {
                c.en_chargement = true;
            }
            ((), Application::default())
        });
        Ok(())
    }

    async fn rattraper_depuis(&self, cle: &CleConversation, depart: &str) -> Result<(), ErreurApi> {
        if !depart.is_empty() {
            let mut apres = depart.to_string();
            let mut manques: Vec<Message> = Vec::new();
            for _ in 0..PAGES_RATTRAPAGE_MAX {
                let (messages, encore) = decouper(
                    self.client()
                        .messages_de(cle, &page_apres(&apres, LIMITE_PAGE_LONGUE))
                        .await?,
                );
                manques.extend(messages);
                match manques.last() {
                    Some(dernier) if encore => apres = id_message(dernier).to_string(),
                    _ => {
                        self.installer_messages(
                            cle,
                            Installation::Rattrapage {
                                apres: depart.to_string(),
                                messages: manques,
                            },
                        );
                        return Ok(());
                    }
                }
            }
        }
        let ancre = self.modifier_messages(|m, contexte| {
            let ancre = m
                .conversation(cle)
                .and_then(|c| c.ancre.clone())
                .filter(|_| contexte.lit_l_historique(cle));
            (ancre, Application::default())
        });
        if let Some(ancre) = ancre {
            if self.charger_autour(cle, &ancre, false).await? {
                return Ok(());
            }
        }
        let (messages, plus_anciens) =
            decouper(self.client().messages_de(cle, &Page::default()).await?);
        self.installer_messages(
            cle,
            Installation::Dernieres {
                messages,
                plus_anciens,
                rechargee: !depart.is_empty(),
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::{json, Value};
    use tokio::sync::mpsc;

    use super::*;
    use crate::serveur_factice::{Reponse, Requete, ServeurFactice};
    use crate::store::essais::{evenement, message, monde_a, programmer};
    use crate::store::messages::{
        ChangementMessages, CONVERSATIONS_EN_CACHE, MESSAGES_PAR_CONVERSATION,
    };
    use crate::temps_reel::{FluxTempsReel, Signal};

    const GARDE_FOU: Duration = Duration::from_secs(20);

    type Historique = Arc<Mutex<Vec<u64>>>;

    fn c1() -> CleConversation {
        CleConversation::Salon("c1".into())
    }

    fn json_message(salon: &str, id: u64) -> Value {
        let mut m = message(&id.to_string(), salon, "bob");
        m["created_at"] = json!(format!(
            "2026-10-13T{:02}:{:02}:{:02}Z",
            10 + (id / 3600) % 10,
            (id / 60) % 60,
            id % 60
        ));
        m
    }

    fn page(salon: &str, requete: &Requete, ids: &[u64]) -> Reponse {
        page_retouchee(salon, requete, ids, |_| {})
    }

    fn page_retouchee(
        salon: &str,
        requete: &Requete,
        ids: &[u64],
        retouche: impl Fn(&mut Value),
    ) -> Reponse {
        let limite: usize = requete
            .parametre("limit")
            .and_then(|l| l.parse().ok())
            .unwrap_or(50);
        let entier = |nom: &str| requete.parametre(nom).map(|v| v.parse::<u64>().unwrap());
        let (choisis, plus): (Vec<u64>, bool) = match (entier("after"), entier("before")) {
            (Some(apres), _) => {
                let suite: Vec<u64> = ids.iter().copied().filter(|i| *i > apres).collect();
                (
                    suite.iter().take(limite).copied().collect(),
                    suite.len() > limite,
                )
            }
            (None, avant) => {
                let borne = avant.unwrap_or(u64::MAX);
                let anciens: Vec<u64> = ids.iter().copied().filter(|i| *i < borne).collect();
                let debut = anciens.len().saturating_sub(limite);
                (anciens[debut..].to_vec(), debut > 0)
            }
        };
        let messages: Vec<Value> = choisis
            .iter()
            .map(|i| {
                let mut m = json_message(salon, *i);
                retouche(&mut m);
                m
            })
            .collect();
        Reponse::json(200, json!({"messages": messages, "has_more": plus}))
    }

    fn servir_historique(serveur: &ServeurFactice, salon: &str, historique: Historique) {
        let nom = salon.to_string();
        serveur.router_dynamique(
            "GET",
            &format!("/channels/{salon}/messages"),
            move |requete| page(&nom, requete, &historique.lock().unwrap()),
        );
        serveur.router(
            "GET",
            &format!("/channels/{salon}/reads"),
            Reponse::json(
                200,
                json!([{"user_id": "bob", "pseudo": "bob", "last_id": "1", "read_at": "x"}]),
            ),
        );
    }

    async fn monde(historique: &[u64]) -> (ServeurFactice, Historique, Store) {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        programmer(&serveur, monde_a());
        let partage: Historique = Arc::new(Mutex::new(historique.to_vec()));
        servir_historique(&serveur, "c1", partage.clone());
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        (serveur, partage, store)
    }

    fn ids(store: &Store, cle: &CleConversation) -> Vec<u64> {
        store
            .conversation(cle)
            .map(|c| {
                c.messages
                    .iter()
                    .map(|m| id_message(m).parse().unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn dernier(store: &Store) -> u64 {
        ids(store, &c1()).last().copied().unwrap_or(0)
    }

    async fn attendre(
        store: &Store,
        abonne: &mut mpsc::UnboundedReceiver<Vec<Changement>>,
        recus: &mut Vec<Changement>,
        condition: impl Fn(&Store) -> bool,
    ) {
        tokio::time::timeout(GARDE_FOU, async {
            while !condition(store) {
                recus.extend(abonne.recv().await.expect("store fermé"));
            }
        })
        .await
        .expect("condition jamais remplie");
    }

    fn lectures_envoyees(serveur: &ServeurFactice) -> usize {
        serveur
            .lignes()
            .iter()
            .filter(|(chemin, methode)| chemin == "/channels/c1/read" && methode == "POST")
            .count()
    }

    fn messages(changements: &[Changement]) -> Vec<ChangementMessages> {
        changements
            .iter()
            .filter_map(|c| match c {
                Changement::Messages(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn ouvrir_charge_la_derniere_page_et_les_accuses_de_lecture() {
        let (serveur, _, store) = monde(&(1000..1120).collect::<Vec<_>>()).await;
        let mut abonne = store.abonner();
        store.ouvrir(Some(c1())).await;
        assert_eq!(ids(&store, &c1()), (1070..1120).collect::<Vec<_>>());
        let c = store.conversation(&c1()).unwrap();
        assert!(c.chargee && c.fin_atteinte && !c.debut_atteint);
        assert_eq!(c.lectures["bob"].last_id, "1");
        let mut recus = Vec::new();
        while let Ok(lot) = abonne.try_recv() {
            recus.extend(lot);
        }
        assert!(messages(&recus).contains(&ChangementMessages::Rechargee { conversation: c1() }));
        assert!(store.charger_plus_anciens(&c1()).await.unwrap());
        assert_eq!(ids(&store, &c1()).len(), 100);
        assert!(!store.charger_plus_recents(&c1()).await.unwrap());
        let avant = serveur.lignes().len();
        store.ouvrir(Some(c1())).await;
        assert!(serveur
            .lignes()
            .iter()
            .skip(avant)
            .all(|(chemin, _)| !chemin.contains("/messages")));
    }

    #[tokio::test]
    async fn apres_une_coupure_de_cinq_minutes_l_historique_est_complet_ordonne_et_sans_doublon() {
        let (serveur, historique, store) = monde(&(1000..1120).collect::<Vec<_>>()).await;
        store.ouvrir(Some(c1())).await;
        let mut abonne = store.abonner();
        let (signaux, flux) = FluxTempsReel::factice();
        let boucle = store.brancher(flux);
        let envoyer = |type_evenement: &str, data: Value| {
            signaux
                .send(Signal::Evenement(Box::new(evenement(type_evenement, data))))
                .unwrap()
        };
        for id in 1120..1125 {
            historique.lock().unwrap().push(id);
            envoyer("message.created", json_message("c1", id));
        }
        envoyer(
            "typing",
            json!({"channel_id": "c1", "user_id": "carol", "pseudo": "carol"}),
        );
        let mut recus = Vec::new();
        attendre(&store, &mut abonne, &mut recus, |s| {
            dernier(s) == 1124 && s.saisies(&c1()) == ["carol"]
        })
        .await;

        tokio::time::pause();
        historique.lock().unwrap().extend(1125..1325);
        tokio::time::advance(Duration::from_secs(5 * 60)).await;
        while !store.saisies(&c1()).is_empty() {
            recus.extend(abonne.recv().await.expect("store fermé"));
        }
        tokio::time::resume();
        assert_eq!(dernier(&store), 1124);

        recus.clear();
        signaux.send(Signal::Reconnecte).unwrap();
        envoyer("message.created", json_message("c1", 1324));
        envoyer("message.created", json_message("c1", 1325));
        attendre(&store, &mut abonne, &mut recus, |s| dernier(s) == 1325).await;

        assert_eq!(ids(&store, &c1()), (1070..1326).collect::<Vec<_>>());
        let changes = messages(&recus);
        assert!(changes.contains(&ChangementMessages::AjoutesEnFin {
            conversation: c1(),
            nombre: 200
        }));
        assert!(!changes
            .iter()
            .any(|c| matches!(c, ChangementMessages::Rechargee { .. })));
        let rattrapage: Vec<String> = serveur
            .lignes()
            .into_iter()
            .map(|(chemin, _)| chemin)
            .filter(|chemin| chemin.contains("after="))
            .collect();
        assert_eq!(
            rattrapage,
            [
                "/channels/c1/messages?limit=100&after=1069",
                "/channels/c1/messages?limit=100&after=1124",
                "/channels/c1/messages?limit=100&after=1224"
            ]
        );
        drop(signaux);
        assert_eq!(boucle.await.unwrap(), crate::store::FinFlux::Termine);
    }

    #[tokio::test]
    async fn apres_une_coupure_les_modifications_suppressions_et_reactions_manquees_sont_relues() {
        let (serveur, historique, store) = monde(&(1000..1120).collect::<Vec<_>>()).await;
        store.ouvrir(Some(c1())).await;
        assert!(store.charger_plus_anciens(&c1()).await.unwrap());
        assert_eq!(ids(&store, &c1()), (1020..1120).collect::<Vec<_>>());
        let mut abonne = store.abonner();
        let (signaux, flux) = FluxTempsReel::factice();
        let boucle = store.brancher(flux);

        historique.lock().unwrap().retain(|i| *i != 1050);
        historique.lock().unwrap().push(1120);
        let partage = historique.clone();
        serveur.router_dynamique("GET", "/channels/c1/messages", move |requete| {
            page_retouchee("c1", requete, &partage.lock().unwrap(), |m| {
                match m["id"].as_str() {
                    Some("1030") => {
                        m["content"] = json!("corrigé pendant la coupure");
                        m["edited_at"] = json!("2026-10-13T11:00:00Z");
                    }
                    Some("1100") => {
                        m["reactions"] = json!([{"emoji": "👍", "count": 2, "me": true}]);
                        m["pinned_at"] = json!("2026-10-13T11:05:00Z");
                    }
                    _ => {}
                }
            })
        });
        serveur.router(
            "GET",
            "/channels/c1/reads",
            Reponse::json(
                200,
                json!([{"user_id": "bob", "pseudo": "bob", "last_id": "1119", "read_at": "y"}]),
            ),
        );

        let mut recus = Vec::new();
        signaux.send(Signal::Reconnecte).unwrap();
        attendre(&store, &mut abonne, &mut recus, |s| {
            s.conversation(&c1())
                .is_some_and(|c| c.lectures["bob"].last_id == "1119")
        })
        .await;

        let attendus: Vec<u64> = (1020..1121).filter(|i| *i != 1050).collect();
        assert_eq!(ids(&store, &c1()), attendus);
        let c = store.conversation(&c1()).unwrap();
        let corrige = c.message("1030").unwrap();
        assert_eq!(
            corrige.content.as_deref(),
            Some("corrigé pendant la coupure")
        );
        assert!(corrige.edited_at.is_some());
        let reagi = c.message("1100").unwrap();
        assert_eq!(reagi.reactions.as_ref().unwrap()[0].count, Some(2));
        assert!(reagi.pinned_at.is_some());
        let changes = messages(&recus);
        for attendu in [
            ChangementMessages::Modifie {
                conversation: c1(),
                message: "1030".into(),
            },
            ChangementMessages::Modifie {
                conversation: c1(),
                message: "1100".into(),
            },
            ChangementMessages::Supprime {
                conversation: c1(),
                message: "1050".into(),
            },
            ChangementMessages::AjoutesEnFin {
                conversation: c1(),
                nombre: 1,
            },
            ChangementMessages::Lectures { conversation: c1() },
        ] {
            assert!(
                changes.contains(&attendu),
                "{attendu:?} absent de {changes:?}"
            );
        }
        assert!(!changes
            .iter()
            .any(|c| matches!(c, ChangementMessages::Rechargee { .. })));
        assert!(!store.conversation(&c1()).unwrap().en_chargement);
        drop(signaux);
        assert_eq!(boucle.await.unwrap(), crate::store::FinFlux::Termine);
    }

    #[tokio::test]
    async fn la_relecture_d_un_historique_lu_au_milieu_ne_touche_pas_au_present() {
        let (serveur, historique, store) = monde(&(1000..1600).collect::<Vec<_>>()).await;
        assert!(store.sauter_a(&c1(), "1100").await.unwrap());
        let avant = ids(&store, &c1());
        assert!(!store.conversation(&c1()).unwrap().fin_atteinte);
        historique.lock().unwrap().retain(|i| *i != 1110);
        historique.lock().unwrap().extend(1600..1650);
        let lignes = serveur.lignes().len();
        store.rattraper_messages().await;
        let attendus: Vec<u64> = avant.iter().copied().filter(|i| *i != 1110).collect();
        assert_eq!(ids(&store, &c1()), attendus);
        let c = store.conversation(&c1()).unwrap();
        assert!(!c.fin_atteinte && !c.en_chargement);
        assert!(serveur
            .lignes()
            .iter()
            .skip(lignes)
            .filter(|(chemin, _)| chemin.contains("/messages"))
            .all(|(chemin, _)| chemin.contains("after=")));
    }

    #[tokio::test]
    async fn un_trou_trop_grand_recharge_la_conversation_autour_de_sa_position() {
        let (_serveur, historique, store) = monde(&(1000..1120).collect::<Vec<_>>()).await;
        store.ouvrir(Some(c1())).await;
        let manques = (PAGES_RATTRAPAGE_MAX * LIMITE_PAGE_LONGUE as usize) as u64 + 50;
        historique.lock().unwrap().extend(1120..1120 + manques);
        let mut abonne = store.abonner();
        store.rattraper_messages().await;
        let fin = 1120 + manques;
        assert_eq!(ids(&store, &c1()), (fin - 50..fin).collect::<Vec<_>>());
        let c = store.conversation(&c1()).unwrap();
        assert!(c.rechargee && c.fin_atteinte);
        let mut recus = Vec::new();
        while let Ok(lot) = abonne.try_recv() {
            recus.extend(lot);
        }
        assert!(messages(&recus).contains(&ChangementMessages::Rechargee { conversation: c1() }));

        store
            .signaler_lecture(false, Some((fin - 20).to_string()))
            .await;
        historique.lock().unwrap().extend(fin..fin + manques);
        store.rattraper_messages().await;
        let c = store.conversation(&c1()).unwrap();
        assert!(c.rechargee && !c.fin_atteinte && !c.debut_atteint);
        assert!(c.contient(&(fin - 20).to_string()));
        let charges = ids(&store, &c1());
        assert!(charges.windows(2).all(|w| w[1] == w[0] + 1));
        assert!(charges.len() <= MESSAGES_PAR_CONVERSATION);
        assert!(store.charger_plus_recents(&c1()).await.unwrap());
        assert_eq!(
            ids(&store, &c1()).last().copied(),
            charges.last().map(|d| d + LIMITE_PAGE as u64)
        );
    }

    #[tokio::test]
    async fn sauter_a_un_message_cite_non_charge_le_charge_puis_le_met_en_evidence() {
        let (serveur, _, store) = monde(&(900..1120).collect::<Vec<_>>()).await;
        store.ouvrir(Some(c1())).await;
        let mut abonne = store.abonner();
        let avant = serveur.lignes().len();
        assert!(store.sauter_a(&c1(), "1010").await.unwrap());
        let suite: Vec<String> = serveur
            .lignes()
            .into_iter()
            .skip(avant)
            .map(|(chemin, _)| chemin)
            .collect();
        assert_eq!(
            suite,
            [
                "/channels/c1/messages?limit=100&after=1009",
                "/channels/c1/messages?limit=100&after=1109",
                "/channels/c1/messages?limit=50&before=1010"
            ]
        );
        assert_eq!(ids(&store, &c1()), (960..1120).collect::<Vec<_>>());
        let c = store.conversation(&c1()).unwrap();
        assert!(c.fin_atteinte && !c.debut_atteint);
        let recus: Vec<Changement> = abonne.try_recv().unwrap();
        assert_eq!(
            messages(&recus),
            [
                ChangementMessages::Rechargee { conversation: c1() },
                ChangementMessages::MiseEnEvidence {
                    conversation: c1(),
                    message: "1010".into()
                }
            ]
        );
        let avant = serveur.lignes().len();
        assert!(store.sauter_a(&c1(), "1100").await.unwrap());
        assert_eq!(serveur.lignes().len(), avant);
        assert_eq!(
            messages(&abonne.try_recv().unwrap()),
            [ChangementMessages::MiseEnEvidence {
                conversation: c1(),
                message: "1100".into()
            }]
        );
    }

    #[tokio::test]
    async fn sauter_a_un_message_disparu_laisse_l_historique_intact() {
        let (_serveur, _, store) = monde(&[7, 99]).await;
        store.ouvrir(Some(c1())).await;
        assert!(!store.sauter_a(&c1(), "5").await.unwrap());
        assert_eq!(ids(&store, &c1()), [7, 99]);
        assert!(!store.conversation(&c1()).unwrap().en_chargement);
    }

    #[tokio::test]
    async fn un_message_recu_fenetre_inactive_reste_non_lu_puis_passe_a_lu_au_retour_en_bas() {
        let (serveur, _, store) = monde(&[1, 2]).await;
        store.ouvrir(Some(c1())).await;
        store.definir_focus(false).await;
        let lues = lectures_envoyees(&serveur);
        store.appliquer(&evenement("message.created", json_message("c1", 3)));
        assert_eq!(store.lire(|e| e.non_lus_de(&c1())), 1);
        assert_eq!(ids(&store, &c1()), [1, 2, 3]);
        store.signaler_lecture(false, Some("2".into())).await;
        store.definir_focus(true).await;
        assert_eq!(store.lire(|e| e.non_lus_de(&c1())), 1);
        assert_eq!(lectures_envoyees(&serveur), lues);
        store.signaler_lecture(true, None).await;
        assert_eq!(store.lire(|e| e.non_lus_de(&c1())), 0);
        assert_eq!(lectures_envoyees(&serveur), lues + 1);
    }

    #[tokio::test]
    async fn une_lecture_faite_sur_un_autre_appareil_efface_la_pastille() {
        let (_serveur, _, store) = monde(&[1]).await;
        let mut abonne = store.abonner();
        let c3 = CleConversation::Salon("c3".into());
        store.appliquer(&evenement("message.created", json_message("c3", 4)));
        assert_eq!(store.lire(|e| e.non_lus_de(&c3)), 1);
        store.appliquer(&evenement(
            "read.updated",
            json!({"channel_id": "c3", "user_id": "me", "pseudo": "moi", "last_id": "4", "read_at": "x"}),
        ));
        assert_eq!(store.lire(|e| e.non_lus_de(&c3)), 0);
        let mut recus = Vec::new();
        while let Ok(lot) = abonne.try_recv() {
            recus.extend(lot);
        }
        assert_eq!(
            recus.last(),
            Some(&Changement::NonLus {
                conversation: c3,
                serveur: Some("s1".into())
            })
        );
    }

    #[tokio::test]
    async fn les_bornes_du_cache_tiennent_apres_de_nombreuses_pages_et_conversations() {
        let (serveur, _, store) = monde(&(10_000..15_000).collect::<Vec<_>>()).await;
        store.ouvrir(Some(c1())).await;
        for _ in 0..30 {
            store.charger_plus_anciens(&c1()).await.unwrap();
            assert!(ids(&store, &c1()).len() <= MESSAGES_PAR_CONVERSATION);
        }
        let charges = ids(&store, &c1());
        assert_eq!(charges.len(), MESSAGES_PAR_CONVERSATION);
        assert!(charges.windows(2).all(|w| w[1] == w[0] + 1));
        assert!(!store.conversation(&c1()).unwrap().fin_atteinte);
        for i in 0..CONVERSATIONS_EN_CACHE * 3 {
            let salon = format!("k{i}");
            servir_historique(&serveur, &salon, Arc::new(Mutex::new((1..=80).collect())));
            store
                .charger_conversation(&CleConversation::Salon(salon))
                .await
                .unwrap();
            let (nombre, total) = store.lire_messages(|m| {
                (
                    m.conversations.len(),
                    m.conversations
                        .values()
                        .map(|c| c.messages.len())
                        .sum::<usize>(),
                )
            });
            assert!(nombre <= CONVERSATIONS_EN_CACHE);
            assert!(total <= CONVERSATIONS_EN_CACHE * MESSAGES_PAR_CONVERSATION);
        }
        assert_eq!(ids(&store, &c1()), charges);
    }

    #[tokio::test]
    async fn une_conversation_privee_remonte_a_chaque_message_et_se_recale_a_la_reconnexion() {
        let (_serveur, _, store) = monde(&[]).await;
        store.appliquer(&evenement(
            "message.created",
            json!({"id": "9", "dm_id": "d1", "author": {"id": "dave", "pseudo": "dave"}}),
        ));
        let lignes = store.lignes_privees();
        assert_eq!((lignes[0].id.as_str(), lignes[0].non_lus), ("d1", 1));
        assert_eq!(store.lire_messages(|m| m.remontees_privees.clone()), ["d1"]);
        store.rattraper_messages().await;
        assert!(store.lire_messages(|m| m.remontees_privees.is_empty()));
    }
}
