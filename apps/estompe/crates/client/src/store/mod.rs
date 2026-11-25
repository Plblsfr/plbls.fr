mod chargement;
mod etat;
pub mod messages;
mod reconciliation;
mod reducteur;

#[cfg(test)]
mod essais;

use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub use chargement::{
    charger, charger_serveur, etat_depuis, Chargement, DonneesServeur, ErreurStore,
};
pub use etat::{
    est_vocal, reglages_par_defaut, salon_depuis, texte_du_genre, trier_salons, CleConversation,
    Etat, GenreSalon, ParticipantVocal, Presence, Salon,
};
pub use reconciliation::reconcilier;
pub use reducteur::{
    appliquer, appliquer_avec_effets, decision_lecture, definir_bas_de_liste, definir_focus,
    lire_conversation, marquer_non_lu, ouvrir_conversation, tout_lire, Application, Avis,
    Changement, Effet,
};

use crate::derniere_vue::{adresse_a_rouvrir, conversation_de_la_derniere_vue, DerniereVue};
use crate::http::{Client, ErreurApi};
use crate::ordre_serveurs::{deplacer_serveur, Sens};
use crate::regarder_ensemble::GenreSeance;
use crate::temps_reel::evenements::{Seance, StatutPresence};
use crate::temps_reel::{Evenement, FluxTempsReel, Signal};
use crate::types::NotifySettings;
pub(crate) use etat::id_de;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinFlux {
    Termine,
    SessionExpiree,
    AccesRetire,
}

struct Interne {
    client: Client,
    etat: RwLock<Etat>,
    messages: RwLock<messages::EtatMessages>,
    abonnes: Mutex<Vec<mpsc::UnboundedSender<Vec<Changement>>>>,
    expedition: tokio::sync::Mutex<()>,
    relance: std::sync::atomic::AtomicBool,
    rythme: Mutex<messages::RythmeSaisie>,
    horloge: RwLock<Option<Horloge>>,
}

pub type Horloge = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Clone)]
pub struct Store {
    interne: Arc<Interne>,
}

impl Store {
    pub fn nouveau(client: Client, ordre_serveurs: &[String]) -> Self {
        Self {
            interne: Arc::new(Interne {
                client,
                etat: RwLock::new(Etat::avec_ordre_serveurs(ordre_serveurs)),
                messages: RwLock::new(messages::EtatMessages::default()),
                abonnes: Mutex::new(Vec::new()),
                expedition: tokio::sync::Mutex::new(()),
                relance: std::sync::atomic::AtomicBool::new(false),
                rythme: Mutex::new(messages::RythmeSaisie::default()),
                horloge: RwLock::new(None),
            }),
        }
    }

    pub fn definir_horloge(&self, horloge: Horloge) {
        *self
            .interne
            .horloge
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(horloge);
    }

    pub fn maintenant_ms(&self) -> Option<i64> {
        self.interne
            .horloge
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|h| h())
    }

    pub fn client(&self) -> &Client {
        &self.interne.client
    }

    pub fn abonner(&self) -> mpsc::UnboundedReceiver<Vec<Changement>> {
        let (entree, sortie) = mpsc::unbounded_channel();
        self.interne
            .abonnes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entree);
        sortie
    }

    pub fn lire<R>(&self, lecture: impl FnOnce(&Etat) -> R) -> R {
        lecture(&self.interne.etat.read().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn instantane(&self) -> Etat {
        self.lire(Etat::clone)
    }

    fn modifier<R>(&self, modification: impl FnOnce(&mut Etat) -> (R, Vec<Changement>)) -> R {
        let (resultat, changements) = {
            let mut etat = self.interne.etat.write().unwrap_or_else(|e| e.into_inner());
            modification(&mut etat)
        };
        self.publier(changements);
        resultat
    }

    fn appliquer_application(&self, calcul: impl FnOnce(&mut Etat) -> Application) -> Vec<Effet> {
        self.modifier(|etat| {
            let Application {
                changements,
                effets,
            } = calcul(etat);
            (effets, changements)
        })
    }

    fn remplacer(&self, calcul: impl FnOnce(&Etat) -> Etat) {
        self.modifier(|etat| {
            let nouveau = calcul(etat);
            let changements = reconcilier(etat, &nouveau);
            *etat = nouveau;
            ((), changements)
        });
    }

    fn publier(&self, changements: Vec<Changement>) {
        if changements.is_empty() {
            return;
        }
        let mut abonnes = self
            .interne
            .abonnes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        abonnes.retain(|abonne| abonne.send(changements.clone()).is_ok());
    }

    pub fn installer(&self, nouveau: Etat) {
        self.remplacer(|_| nouveau);
    }

    pub async fn charger(&self) -> Result<(), ErreurStore> {
        let chargement = charger(self.client()).await?;
        self.remplacer(|etat| etat_depuis(chargement, etat));
        Ok(())
    }

    pub fn appliquer_seance(&self, genre: GenreSeance, vue: &Seance, frais: bool) {
        let maintenant = self
            .maintenant_ms()
            .unwrap_or_else(|| crate::mesures::maintenant_ms() as i64);
        self.modifier(|etat| {
            let change = etat.seances.appliquer(genre, vue, frais, maintenant);
            let changements = if change {
                vec![Changement::Seance {
                    genre,
                    salon: vue.channel_id.clone(),
                }]
            } else {
                Vec::new()
            };
            ((), changements)
        });
    }

    pub async fn charger_seances(&self, serveur: &str, genres: &[GenreSeance]) {
        for &genre in genres {
            let Ok(vues) = self.client().seances_du_serveur(serveur, genre).await else {
                continue;
            };
            let absents: Vec<String> = self.lire(|etat| {
                etat.salons(serveur)
                    .iter()
                    .map(|c| id_de(&c.id).to_string())
                    .filter(|s| {
                        etat.seances.seance(genre, s).is_some()
                            && !vues.iter().any(|v| &v.channel_id == s && v.state.is_some())
                    })
                    .collect()
            });
            for vue in &vues {
                self.appliquer_seance(genre, vue, true);
            }
            for salon in absents {
                let vide = Seance {
                    server_id: serveur.to_string(),
                    channel_id: salon,
                    ..Seance::default()
                };
                self.appliquer_seance(genre, &vide, true);
            }
        }
    }

    pub fn appliquer(&self, evenement: &Evenement) -> Vec<Effet> {
        match evenement {
            Evenement::SeanceRegarder(v) => self.appliquer_seance(GenreSeance::Regarder, v, false),
            Evenement::SeanceCinema(v) => self.appliquer_seance(GenreSeance::Cinema, v, false),
            Evenement::SeanceMusique(v) => self.appliquer_seance(GenreSeance::Musique, v, false),
            _ => {}
        }
        let effets = self.appliquer_application(|etat| appliquer_avec_effets(etat, evenement));
        self.appliquer_aux_messages(evenement);
        effets
    }

    pub async fn executer(&self, effets: Vec<Effet>) {
        for effet in effets {
            let _ = match effet {
                Effet::RechargerServeurs => self.rafraichir_serveurs().await,
                Effet::RechargerServeur(id) => self.recharger_serveur(&id).await,
                Effet::RechargerMessagesPrives => self.rafraichir_messages_prives().await,
                Effet::MarquerLu(cle) => self.client().marquer_lu(&cle).await,
                Effet::LireNotifications(ids) => self.client().lire_notifications(&ids).await,
            };
        }
    }

    pub async fn recharger_serveur(&self, id: &str) -> Result<(), ErreurApi> {
        let detail = self.client().serveur(id).await?;
        self.modifier(|etat| {
            let Some(ancien) = etat.details.get(id).cloned() else {
                return ((), Vec::new());
            };
            let mut a = Application::default();
            let gardes: Vec<&str> = detail
                .channels
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|c| id_de(&c.id))
                .collect();
            for salon in ancien.channels.as_deref().unwrap_or_default() {
                if !gardes.contains(&id_de(&salon.id)) {
                    reducteur::oublier_salon(etat, id_de(&salon.id), Some(id), &mut a);
                }
            }
            let mut changements = Vec::new();
            reconciliation::differences_detail(id, Some(&ancien), Some(&detail), &mut changements);
            let proprietaire = detail.server.as_ref().and_then(|s| s.owner_id.clone());
            if let Some(serveur) = etat.serveurs.iter_mut().find(|s| id_de(&s.id) == id) {
                if serveur.owner_id != proprietaire {
                    serveur.owner_id = proprietaire;
                    a.changer(Changement::Serveur {
                        serveur: id.to_string(),
                    });
                }
            }
            etat.details.insert(id.to_string(), detail);
            for changement in changements {
                a.changer(changement);
            }
            ((), a.changements)
        });
        Ok(())
    }

    pub async fn rafraichir_serveurs(&self) -> Result<(), ErreurApi> {
        let serveurs = self.client().serveurs().await?;
        let manquants: Vec<String> = self.lire(|etat| {
            serveurs
                .iter()
                .map(|s| id_de(&s.id).to_string())
                .filter(|id| !id.is_empty() && !etat.details.contains_key(id))
                .collect()
        });
        let charges = chargement::charger_les_serveurs(self.client(), &manquants).await;
        self.remplacer(|etat| {
            let mut nouveau = etat.clone();
            nouveau.serveurs = serveurs;
            for (id, donnees) in charges {
                if let Ok(donnees) = donnees {
                    chargement::installer_serveur(&mut nouveau, etat, &id, donnees);
                }
            }
            nouveau
        });
        Ok(())
    }

    pub async fn creer_serveur(
        &self,
        nom: &str,
        icone: Option<(&str, Vec<u8>)>,
    ) -> Result<(String, bool), ErreurApi> {
        let serveur = self.client().creer_serveur(nom.trim()).await?;
        let id = id_de(&serveur.id).to_string();
        let icone_posee = match icone {
            Some((type_mime, octets)) => self
                .client()
                .definir_icone_de_serveur(&id, type_mime, octets)
                .await
                .is_ok(),
            None => true,
        };
        let _ = self.rafraichir_serveurs().await;
        Ok((id, icone_posee))
    }

    pub async fn rejoindre_serveur(&self, code: &str) -> Result<crate::types::Server, ErreurApi> {
        let serveur = self.client().rejoindre_serveur(code).await?;
        let _ = self.rafraichir_serveurs().await;
        Ok(serveur)
    }

    pub async fn quitter_serveur(&self, serveur: &str) -> Result<(), ErreurApi> {
        self.client().quitter_serveur(serveur).await?;
        self.appliquer_application(|etat| {
            let mut a = Application::default();
            reducteur::oublier_serveur(etat, serveur, &mut a);
            a
        });
        Ok(())
    }

    pub async fn renommer_serveur(&self, serveur: &str, nom: &str) -> Result<(), ErreurApi> {
        let nom = nom.trim();
        self.client().renommer_serveur(serveur, nom).await?;
        self.appliquer(&Evenement::ServeurModifie(
            crate::temps_reel::evenements::ServeurModifie {
                id: serveur.to_string(),
                name: nom.to_string(),
            },
        ));
        Ok(())
    }

    pub async fn supprimer_serveur(&self, serveur: &str) -> Result<(), ErreurApi> {
        self.client().supprimer_serveur(serveur).await?;
        self.appliquer_application(|etat| {
            let mut a = Application::default();
            reducteur::oublier_serveur(etat, serveur, &mut a);
            a
        });
        Ok(())
    }

    pub async fn creer_salon(
        &self,
        serveur: &str,
        salon: &crate::creation::NouveauSalon,
    ) -> Result<String, ErreurApi> {
        let requete = self.lire(|etat| salon.requete(etat.salons(serveur)));
        let apres = requete.after.clone();
        let cree = self.client().creer_salon(serveur, &requete).await?;
        let id = id_de(&cree.id).to_string();
        self.appliquer(&Evenement::SalonCree(
            crate::temps_reel::evenements::SalonCree {
                server_id: serveur.to_string(),
                channel: cree,
                after: apres,
            },
        ));
        Ok(id)
    }

    pub async fn rafraichir_messages_prives(&self) -> Result<(), ErreurApi> {
        let messages_prives = self.client().conversations_privees().await?;
        self.remplacer(|etat| {
            let mut nouveau = etat.clone();
            for dm in &messages_prives {
                if dm.online == Some(true) {
                    if let Some(autre) = dm.other.as_ref().and_then(|p| p.id.clone()) {
                        nouveau.en_ligne.insert(autre);
                    }
                }
            }
            nouveau.messages_prives = messages_prives;
            nouveau
        });
        Ok(())
    }

    pub async fn ouvrir(&self, conversation: Option<CleConversation>) {
        let effets =
            self.appliquer_application(|etat| ouvrir_conversation(etat, conversation.clone()));
        self.executer(effets).await;
        if let Some(cle) = conversation {
            let _ = self.charger_conversation(&cle).await;
        }
    }

    pub async fn lire_conversation(&self, conversation: &CleConversation) {
        let effets = self.appliquer_application(|etat| lire_conversation(etat, conversation));
        self.executer(effets).await;
    }

    pub async fn tout_lire(&self) {
        let effets = self.appliquer_application(tout_lire);
        self.executer(effets).await;
    }

    pub async fn ouvrir_message_prive(&self, pseudo: &str) -> Result<String, ErreurApi> {
        let dm = self.client().ouvrir_conversation_privee(pseudo).await?;
        let id = id_de(&dm.id).to_string();
        self.modifier(|etat| {
            let mut changements = Vec::new();
            let autre = dm.other.as_ref().map(|p| id_de(&p.id).to_string());
            if let Some(autre) = autre.filter(|_| dm.online == Some(true)) {
                if etat.en_ligne.insert(autre.clone()) {
                    changements.push(Changement::Presence { utilisateur: autre });
                }
            }
            if !etat.messages_prives.iter().any(|d| id_de(&d.id) == id) {
                etat.messages_prives.insert(0, dm);
                changements.push(Changement::MessagesPrives);
            }
            ((), changements)
        });
        Ok(id)
    }

    pub async fn definir_focus(&self, focus: bool) {
        let effets = self.appliquer_application(|etat| definir_focus(etat, focus));
        self.executer(effets).await;
    }

    pub fn definir_activite_locale(&self, nom: &str) {
        let nom = nom.to_string();
        self.modifier(|etat| {
            if etat.activite_locale == nom {
                return ((), Vec::new());
            }
            etat.activite_locale = nom;
            let moi = etat.moi().to_string();
            ((), vec![Changement::Presence { utilisateur: moi }])
        });
    }

    pub async fn publier_activite(&self, nom: &str) -> Result<(), ErreurApi> {
        self.client().definir_activite(nom).await
    }

    fn poser_statut(&self, statut: Option<StatutPresence>) -> Option<StatutPresence> {
        self.modifier(|etat| {
            let moi = etat.moi().to_string();
            let avant = match statut.filter(|s| *s != StatutPresence::Online) {
                Some(s) => etat.statuts.insert(moi.clone(), s),
                None => etat.statuts.remove(&moi),
            };
            let changements = if avant != etat.statuts.get(&moi).copied() {
                vec![Changement::Presence { utilisateur: moi }]
            } else {
                Vec::new()
            };
            (avant, changements)
        })
    }

    pub async fn definir_statut(&self, statut: StatutPresence) -> Result<(), ErreurApi> {
        let avant = self.poser_statut(Some(statut));
        let resultat = self.client().definir_statut(statut).await;
        if resultat.is_err() {
            self.poser_statut(avant);
        }
        resultat
    }

    fn poser_reglages(&self, reglages: NotifySettings) -> NotifySettings {
        self.modifier(|etat| {
            let avant = std::mem::replace(&mut etat.reglages_notification, reglages);
            let changements = if avant != etat.reglages_notification {
                vec![Changement::ReglagesNotification]
            } else {
                Vec::new()
            };
            (avant, changements)
        })
    }

    pub fn appliquer_reglages_notification(
        &self,
        modifier: impl FnOnce(&NotifySettings) -> Option<NotifySettings>,
    ) -> Option<(NotifySettings, NotifySettings)> {
        self.modifier(|etat| {
            let Some(nouveaux) = modifier(&etat.reglages_notification) else {
                return (None, Vec::new());
            };
            let avant = std::mem::replace(&mut etat.reglages_notification, nouveaux.clone());
            let changements = if avant != nouveaux {
                vec![Changement::ReglagesNotification]
            } else {
                Vec::new()
            };
            (Some((avant, nouveaux)), changements)
        })
    }

    pub async fn envoyer_reglages_notification(
        &self,
        reglages: NotifySettings,
        avant: NotifySettings,
    ) -> Result<(), ErreurApi> {
        let resultat = self
            .client()
            .enregistrer_reglages_notification(&reglages)
            .await;
        if resultat.is_err() {
            self.poser_reglages(avant);
        }
        resultat
    }

    pub async fn enregistrer_reglages_notification(
        &self,
        reglages: NotifySettings,
    ) -> Result<(), ErreurApi> {
        let avant = self.poser_reglages(reglages.clone());
        self.envoyer_reglages_notification(reglages, avant).await
    }

    pub fn ordre_serveurs(&self) -> Vec<String> {
        self.lire(Etat::ids_serveurs_ordonnes)
    }

    pub fn definir_ordre_serveurs(&self, ordre: &[String]) {
        self.modifier(|etat| {
            let avant = etat.ids_serveurs_ordonnes();
            etat.ordre_serveurs = ordre.to_vec();
            let changements = if avant != etat.ids_serveurs_ordonnes() {
                vec![Changement::OrdreServeurs]
            } else {
                Vec::new()
            };
            ((), changements)
        });
    }

    pub fn deplacer_serveur(&self, de: &str, vers: &str) -> Vec<String> {
        let ordre = deplacer_serveur(&self.ordre_serveurs(), de, vers);
        self.definir_ordre_serveurs(&ordre);
        ordre
    }

    pub fn serveur_voisin(&self, courant: Option<&str>, sens: Sens) -> Option<String> {
        self.lire(|etat| etat.serveur_voisin(courant, sens))
    }

    fn ids_connus(etat: &Etat) -> (Vec<String>, Vec<String>) {
        (
            etat.serveurs
                .iter()
                .map(|s| id_de(&s.id).to_string())
                .collect(),
            etat.messages_prives
                .iter()
                .map(|d| id_de(&d.id).to_string())
                .collect(),
        )
    }

    pub fn adresse_a_rouvrir(&self, vue: Option<&DerniereVue>) -> Option<String> {
        self.lire(|etat| {
            let (serveurs, prives) = Self::ids_connus(etat);
            adresse_a_rouvrir(vue, etat.moi(), &serveurs, &prives).map(str::to_string)
        })
    }

    pub fn conversation_a_rouvrir(&self, vue: Option<&DerniereVue>) -> Option<CleConversation> {
        self.lire(|etat| {
            let (serveurs, prives) = Self::ids_connus(etat);
            conversation_de_la_derniere_vue(vue, etat.moi(), &serveurs, &prives)
                .and_then(|cle| CleConversation::analyser(&cle))
        })
    }

    pub fn memoriser_vue(&self, adresse: &str) -> DerniereVue {
        self.lire(|etat| DerniereVue::enregistrer(etat.moi(), adresse))
    }

    pub async fn suivre(&self, mut flux: FluxTempsReel) -> FinFlux {
        loop {
            let premier = match self.prochaine_expiration() {
                Some(echeance) => tokio::select! {
                    signal = flux.suivant() => signal,
                    _ = tokio::time::sleep_until(echeance) => {
                        self.expirer_saisies();
                        continue;
                    }
                },
                None => flux.suivant().await,
            };
            let Some(premier) = premier else {
                return FinFlux::Termine;
            };
            let mut lot = vec![premier];
            while let Some(signal) = flux.essayer() {
                lot.push(signal);
            }
            let mut effets: Vec<Effet> = Vec::new();
            let mut fin = None;
            for signal in lot {
                match signal {
                    Signal::Evenement(evenement) => {
                        for effet in self.appliquer(&evenement) {
                            if !effets.contains(&effet) {
                                effets.push(effet);
                            }
                        }
                    }
                    Signal::Reconnecte => {
                        let _ = self.charger().await;
                        self.rattraper_messages().await;
                        self.expedier().await;
                    }
                    Signal::SessionExpiree => {
                        fin = Some(FinFlux::SessionExpiree);
                        break;
                    }
                    Signal::AccesRetire => {
                        fin = Some(FinFlux::AccesRetire);
                        break;
                    }
                    Signal::Statut(_) | Signal::Reseau(_) => {}
                }
            }
            self.executer(effets).await;
            if let Some(fin) = fin {
                return fin;
            }
        }
    }

    pub fn brancher(&self, flux: FluxTempsReel) -> JoinHandle<FinFlux> {
        let store = self.clone();
        tokio::spawn(async move { store.suivre(flux).await })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::essais::{evenement, message, monde_a, monde_b, programmer};
    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};

    const GARDE_FOU: Duration = Duration::from_secs(20);

    async fn monde(routes: Vec<(&'static str, serde_json::Value)>) -> ServeurFactice {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        programmer(&serveur, routes);
        serveur
    }

    async fn attendre(
        store: &Store,
        abonne: &mut mpsc::UnboundedReceiver<Vec<Changement>>,
        recus: &mut Vec<Changement>,
        condition: impl Fn(&Etat) -> bool,
    ) {
        tokio::time::timeout(GARDE_FOU, async {
            while !store.lire(&condition) {
                recus.extend(abonne.recv().await.expect("store fermé"));
            }
        })
        .await
        .expect("condition jamais remplie");
    }

    fn cle(texte: &str) -> CleConversation {
        CleConversation::analyser(texte).unwrap()
    }

    #[test]
    fn une_capacite_suit_allow_et_deny_comme_le_front() {
        let mut etat = Etat::default();
        assert!(
            etat.capacite("gifs.use"),
            "sans capacités annoncées, tout est permis"
        );
        etat.compte.capabilities = Some(
            serde_json::from_value(json!({
                "estompe:gifs.use": {"allow": ["*"], "deny": []},
                "estompe:videos.use": {"allow": ["*"], "deny": ["*"]},
                "estompe:vocal.screen": {"allow": ["srv-2"], "deny": []}
            }))
            .unwrap(),
        );
        assert!(etat.capacite("gifs.use"));
        assert!(!etat.capacite("videos.use"));
        assert!(!etat.capacite("vocal.screen"));
        assert!(!etat.capacite("admin.users"));
    }

    #[tokio::test]
    async fn les_seances_suivent_le_temps_reel_et_le_rechargement_du_serveur() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        let store = Store::nouveau(serveur.client(), &[]);
        let mut etat = essais::etat_de_depart();
        etat.details.get_mut("s1").unwrap().channels = Some(
            serde_json::from_value(json!([
                {"id": "v1", "name": "Vocal", "kind": "voice", "position": 1},
                {"id": "k1", "name": "Cinéma", "kind": "cinema", "position": 2}
            ]))
            .unwrap(),
        );
        store.installer(etat);
        store.definir_horloge(Arc::new(|| 1_000));
        let mut abonne = store.abonner();
        let seance = |rev: i64| {
            json!({"server_id": "s1", "channel_id": "k1", "now": "1970-01-01T00:00:03Z",
                "state": {"rev": rev, "provider": "film", "video": "f1", "playing": true, "pos": 10.0, "rate": 1.0, "at": "1970-01-01T00:00:02Z", "host": "bob"}})
        };
        store.appliquer(&evenement("cinema.state", seance(4)));
        let s = store.lire(|e| e.seances.seance(GenreSeance::Cinema, "k1").cloned());
        assert_eq!(
            s.as_ref().map(|s| (s.etat.rev, s.decalage)),
            Some((4, 2_000))
        );
        assert!(store.lire(|e| e.seances.seance(GenreSeance::Regarder, "k1").is_none()));
        assert_eq!(
            abonne.try_recv().unwrap(),
            [Changement::Seance {
                genre: GenreSeance::Cinema,
                salon: "k1".into()
            }]
        );
        store.appliquer(&evenement("cinema.state", seance(3)));
        assert!(abonne.try_recv().is_err());
        assert_eq!(
            store.lire(|e| e
                .seances
                .seance(GenreSeance::Cinema, "k1")
                .map(|s| s.etat.rev)),
            Some(4)
        );
        serveur.router("GET", "/servers/s1/watch", Reponse::json(200, json!({"v1": {"state": {"rev": 1, "provider": "file", "video": "https://x.test/v.mp4"}, "now": "1970-01-01T00:00:01Z"}})));
        serveur.router("GET", "/servers/s1/cinema", Reponse::json(200, json!({})));
        store
            .charger_seances("s1", &[GenreSeance::Regarder, GenreSeance::Cinema])
            .await;
        assert!(store.lire(|e| e.seances.seance(GenreSeance::Regarder, "v1").is_some()));
        assert!(store.lire(|e| e.seances.seance(GenreSeance::Cinema, "k1").is_none()));
        let mut recus = Vec::new();
        while let Ok(lot) = abonne.try_recv() {
            recus.extend(lot);
        }
        assert!(recus.contains(&Changement::Seance {
            genre: GenreSeance::Cinema,
            salon: "k1".into()
        }));
        store.appliquer(&evenement(
            "channel.deleted",
            json!({"server_id": "s1", "id": "v1"}),
        ));
        assert!(store.lire(|e| e.seances.seance(GenreSeance::Regarder, "v1").is_none()));
    }

    #[tokio::test]
    async fn charge_le_compte_les_serveurs_et_leurs_details() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        let mut abonne = store.abonner();
        store.charger().await.unwrap();
        let etat = store.instantane();
        assert_eq!((etat.moi(), etat.pseudo()), ("me", "moi"));
        assert!(etat.bloc_actif("vocal") && !etat.bloc_actif("films"));
        assert_eq!(etat.salons("s1").len(), 3);
        assert_eq!(
            etat.membre("s1", "carol").unwrap().nickname.as_deref(),
            Some("Caro")
        );
        assert_eq!(etat.non_lus_de(&cle("c:c1")), 3);
        assert_eq!(etat.nombre_mentions(&cle("c:c1")), 1);
        assert_eq!(etat.presence_de("bob"), Presence::HorsLigne);
        assert_eq!(etat.presence_de("dave"), Presence::EnLigne);
        assert_eq!(etat.participants("v1")[0].utilisateur, "carol");
        assert_eq!(etat.version_avatar_du_serveur("bob", Some("s1")), 2);
        assert_eq!(etat.version_avatar("me"), 1);
        let recus = abonne.try_recv().unwrap();
        assert!(recus.contains(&Changement::ServeurAjoute {
            serveur: "s1".into()
        }));
        assert!(recus.contains(&Changement::Compte));
        assert!(!recus
            .iter()
            .any(|c| matches!(c, Changement::SalonAjoute { .. })));
    }

    #[tokio::test(start_paused = true)]
    async fn l_indicateur_de_saisie_expire_au_bout_de_sa_duree_en_temps_simule() {
        let store = Store::nouveau(Client::nouveau("http://127.0.0.1:9").unwrap(), &[]);
        store.installer(Etat {
            compte: serde_json::from_value(json!({"id": "me", "pseudo": "moi"})).unwrap(),
            ..Etat::default()
        });
        let mut abonne = store.abonner();
        let (signaux, flux) = FluxTempsReel::factice();
        let suivi = store.brancher(flux);
        let c1 = cle("c:c1");
        let saisie = crate::temps_reel::decoder(
            &json!({"type": "typing", "data": {"channel_id": "c1", "user_id": "bob", "pseudo": "bob"}})
                .to_string(),
        );
        let crate::temps_reel::Trame::Evenement(e) = saisie else {
            panic!("trame de saisie");
        };
        let depart = tokio::time::Instant::now();
        signaux.send(Signal::Evenement(e)).unwrap();
        let attendu = vec![Changement::Messages(
            crate::store::messages::ChangementMessages::Saisie {
                conversation: c1.clone(),
            },
        )];
        assert_eq!(abonne.recv().await.unwrap(), attendu);
        assert_eq!(store.saisies(&c1), ["bob"]);
        tokio::time::advance(messages::DUREE_SAISIE / 2).await;
        tokio::task::yield_now().await;
        assert_eq!(store.saisies(&c1), ["bob"], "pas encore expirée");
        assert_eq!(abonne.recv().await.unwrap(), attendu);
        assert!(store.saisies(&c1).is_empty());
        assert_eq!(depart.elapsed(), messages::DUREE_SAISIE);
        drop(signaux);
        assert_eq!(suivi.await.unwrap(), FinFlux::Termine);
    }

    #[tokio::test]
    async fn installer_un_etat_ne_publie_que_ses_differences() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        let mut abonne = store.abonner();
        let mut etat = store.instantane();
        etat.non_lus.insert(cle("c:c3"), 4);
        store.installer(etat.clone());
        assert_eq!(store.instantane(), etat);
        assert_eq!(
            abonne.try_recv().unwrap(),
            [Changement::NonLus {
                conversation: cle("c:c3"),
                serveur: Some("s1".into())
            }]
        );
        store.installer(etat);
        assert!(abonne.try_recv().is_err());
    }

    #[tokio::test]
    async fn refuse_un_compte_non_active() {
        let serveur = monde(vec![("/me", json!({"activated": false, "allowed": true}))]).await;
        let store = Store::nouveau(serveur.client(), &[]);
        assert_eq!(store.charger().await, Err(ErreurStore::CompteInactif));
        assert_eq!(store.instantane(), Etat::default());
    }

    #[tokio::test]
    async fn apres_une_reconnexion_le_store_est_identique_a_un_chargement_a_neuf() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        let mut abonne = store.abonner();
        let (signaux, flux) = FluxTempsReel::factice();
        let boucle = store.brancher(flux);
        let envoyer = |type_evenement: &str, data: serde_json::Value| {
            signaux
                .send(Signal::Evenement(Box::new(evenement(type_evenement, data))))
                .unwrap()
        };
        envoyer(
            "presence.snapshot",
            json!({"online": ["carol", "dave"], "activities": {"carol": "Tetris"}}),
        );
        envoyer("message.created", message("10", "c3", "carol"));
        let mut recus = Vec::new();
        attendre(&store, &mut abonne, &mut recus, |e| {
            e.non_lus_de(&cle("c:c3")) == 1
        })
        .await;

        programmer(&serveur, monde_b());
        signaux.send(Signal::Reconnecte).unwrap();
        let instantane =
            json!({"online": ["carol", "eve", "fred"], "activities": {"eve": "Factorio"}});
        envoyer("presence.snapshot", instantane.clone());
        recus.clear();
        attendre(&store, &mut abonne, &mut recus, |e| {
            e.serveur("s2").is_some() && e.activite_de("eve") == "Factorio"
        })
        .await;

        let neuf = Store::nouveau(serveur.client(), &[]);
        neuf.charger().await.unwrap();
        neuf.appliquer(&evenement("presence.snapshot", instantane));
        assert_eq!(store.instantane(), neuf.instantane());

        for attendu in [
            Changement::ServeurAjoute {
                serveur: "s2".into(),
            },
            Changement::Serveur {
                serveur: "s1".into(),
            },
            Changement::Salon {
                serveur: "s1".into(),
                salon: "c1".into(),
            },
            Changement::SalonAjoute {
                serveur: "s1".into(),
                salon: "c2".into(),
            },
            Changement::MembreRetire {
                serveur: "s1".into(),
                utilisateur: "bob".into(),
            },
            Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "carol".into(),
            },
            Changement::NonLus {
                conversation: cle("c:c3"),
                serveur: Some("s1".into()),
            },
            Changement::Vocal { salon: "v1".into() },
            Changement::MessagesPrives,
        ] {
            assert!(recus.contains(&attendu), "{attendu:?} absent de {recus:?}");
        }
        for absent in [
            Changement::Salon {
                serveur: "s1".into(),
                salon: "c3".into(),
            },
            Changement::SalonAjoute {
                serveur: "s2".into(),
                salon: "t1".into(),
            },
            Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "me".into(),
            },
        ] {
            assert!(!recus.contains(&absent), "{absent:?} émis : {recus:?}");
        }
        drop(signaux);
        assert_eq!(boucle.await.unwrap(), FinFlux::Termine);
    }

    #[tokio::test]
    async fn un_evenement_sur_un_salon_ne_notifie_que_ce_salon() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        let mut abonne = store.abonner();
        let effets = store.appliquer(&evenement(
            "channel.updated",
            json!({"server_id": "s1", "id": "c3", "name": "jeux-video"}),
        ));
        assert!(effets.is_empty());
        assert_eq!(
            abonne.try_recv().unwrap(),
            [Changement::Salon {
                serveur: "s1".into(),
                salon: "c3".into()
            }]
        );
        assert!(abonne.try_recv().is_err());
    }

    #[tokio::test]
    async fn retire_un_salon_devenu_invisible_au_rechargement_du_serveur() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        store.appliquer(&evenement("message.created", message("11", "c3", "carol")));
        let mut abonne = store.abonner();
        serveur.router(
            "GET",
            "/servers/s1",
            Reponse::json(
                200,
                json!({
                    "server": {"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0},
                    "channels": [{"id": "c1", "name": "général", "kind": "text", "position": 1, "permissions": ["view_channel"]}],
                    "members": [{"id": "me", "pseudo": "moi", "online": true}],
                    "roles": [],
                    "permissions": ["view_channel"]
                }),
            ),
        );
        let (signaux, flux) = FluxTempsReel::factice();
        let boucle = store.brancher(flux);
        signaux
            .send(Signal::Evenement(Box::new(evenement(
                "roles.updated",
                json!({"server_id": "s1"}),
            ))))
            .unwrap();
        let mut recus = Vec::new();
        attendre(&store, &mut abonne, &mut recus, |e| {
            e.salons("s1").len() == 1
        })
        .await;
        let etat = store.instantane();
        assert_eq!(etat.salons("s1")[0].id.as_deref(), Some("c1"));
        assert_eq!(
            etat.salons("s1")[0].permissions,
            Some(vec!["view_channel".to_string()])
        );
        assert_eq!(
            etat.details["s1"].permissions,
            Some(vec!["view_channel".to_string()])
        );
        assert_eq!(etat.non_lus.get(&cle("c:c3")), None);
        assert!(etat.participants("v1").is_empty());
        assert!(recus.contains(&Changement::Roles {
            serveur: "s1".into()
        }));
        assert!(recus.contains(&Changement::SalonRetire {
            serveur: "s1".into(),
            salon: "c3".into()
        }));
        signaux.send(Signal::AccesRetire).unwrap();
        assert_eq!(boucle.await.unwrap(), FinFlux::AccesRetire);
    }

    #[tokio::test]
    async fn rend_la_main_quand_la_session_expire() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        let (signaux, flux) = FluxTempsReel::factice();
        signaux.send(Signal::SessionExpiree).unwrap();
        assert_eq!(store.suivre(flux).await, FinFlux::SessionExpiree);
    }

    #[tokio::test]
    async fn enregistre_les_reglages_et_les_retablit_en_cas_d_erreur() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let store = Store::nouveau(serveur.client(), &[]);
        let mut reglages = store.lire(|e| e.reglages_notification.clone());
        reglages.sound = Some(false);
        store
            .enregistrer_reglages_notification(reglages.clone())
            .await
            .unwrap();
        assert_eq!(store.lire(|e| e.reglages_notification.sound), Some(false));
        let envoye = serveur.requetes()[0].json();
        assert_eq!(
            (envoye["sound"].clone(), envoye["preview"].clone()),
            (json!(false), json!(true))
        );
        serveur.programmer([Reponse::json(500, json!({"error": "x", "message": "x"}))]);
        let mut sans_apercu = reglages.clone();
        sans_apercu.preview = Some(false);
        assert!(store
            .enregistrer_reglages_notification(sans_apercu)
            .await
            .is_err());
        assert_eq!(store.lire(|e| e.reglages_notification.preview), Some(true));
        let premier = store.appliquer_reglages_notification(|r| {
            Some(NotifySettings {
                sound: Some(true),
                ..r.clone()
            })
        });
        let second = store.appliquer_reglages_notification(|r| {
            Some(NotifySettings {
                preview: Some(false),
                ..r.clone()
            })
        });
        assert_eq!(
            (premier.unwrap().1.sound, second.clone().unwrap().1.sound),
            (Some(true), Some(true)),
            "deux réglages changés coup sur coup se cumulent avant l'envoi"
        );
        assert!(store.appliquer_reglages_notification(|_| None).is_none());
        serveur.programmer([Reponse::json(500, json!({"error": "x", "message": "x"}))]);
        let (avant, apres) = second.unwrap();
        assert!(store
            .envoyer_reglages_notification(apres, avant)
            .await
            .is_err());
        assert_eq!(store.lire(|e| e.reglages_notification.preview), Some(true));
    }

    #[tokio::test]
    async fn change_son_statut_et_le_retablit_en_cas_d_erreur() {
        let serveur = monde(monde_a()).await;
        serveur.router("PUT", "/me/status", Reponse::vide(204));
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        store.definir_statut(StatutPresence::Dnd).await.unwrap();
        assert_eq!(store.lire(|e| e.presence_de("me")), Presence::NePasDeranger);
        serveur.router(
            "PUT",
            "/me/status",
            Reponse::json(500, json!({"error": "x", "message": "x"})),
        );
        assert!(store.definir_statut(StatutPresence::Idle).await.is_err());
        assert_eq!(store.lire(|e| e.presence_de("me")), Presence::NePasDeranger);
        store.definir_activite_locale("Tetris");
        assert_eq!(store.lire(|e| e.activite_de("me").to_string()), "Tetris");
    }

    fn relayer(
        serveur: &ServeurFactice,
        methode: &str,
        chemin: &str,
        vers: &mpsc::UnboundedSender<Signal>,
        trame: impl Fn(&crate::serveur_factice::Requete) -> (&'static str, serde_json::Value)
            + Send
            + Sync
            + 'static,
    ) {
        let vers = vers.clone();
        serveur.router_dynamique(methode, chemin, move |requete| {
            let (type_evenement, data) = trame(requete);
            let _ = vers.send(Signal::Evenement(Box::new(evenement(type_evenement, data))));
            Reponse::vide(204)
        });
    }

    #[tokio::test]
    async fn tout_marquer_lu_vide_les_pastilles_ici_et_sur_un_autre_appareil() {
        let serveur = monde(monde_a()).await;
        programmer(
            &serveur,
            vec![(
                "/unread",
                json!({"channels": ["c1", "c3"], "dms": ["d1"], "counts": {"c1": 3, "c3": 1, "d1": 2}}),
            )],
        );
        serveur.router("POST", "/notifications/read", Reponse::vide(204));
        let (vers_autre, flux_autre) = FluxTempsReel::factice();
        for (chemin, salon, prive) in [
            ("/channels/c1/read", Some("c1"), None),
            ("/channels/c3/read", Some("c3"), None),
            ("/dms/d1/read", None, Some("d1")),
        ] {
            relayer(&serveur, "POST", chemin, &vers_autre, move |_| {
                (
                    "read.updated",
                    json!({"channel_id": salon, "dm_id": prive, "user_id": "me", "pseudo": "moi", "last_id": "9", "read_at": ""}),
                )
            });
        }
        let ici = Store::nouveau(serveur.client(), &[]);
        let ailleurs = Store::nouveau(serveur.client(), &[]);
        ici.charger().await.unwrap();
        ailleurs.charger().await.unwrap();
        let pastilles = |e: &Etat| (e.non_lus.values().sum::<i64>(), e.total_mentions());
        assert_eq!(ailleurs.lire(pastilles), (6, 1));
        let mut abonne = ailleurs.abonner();
        let suivi = ailleurs.brancher(flux_autre);
        let avant = serveur.requetes().len();
        ici.tout_lire().await;
        assert_eq!(ici.lire(pastilles), (0, 0));
        let mut lignes: Vec<_> = serveur
            .requetes()
            .into_iter()
            .skip(avant)
            .map(|r| (r.chemin.clone(), r.methode.clone()))
            .collect();
        lignes.sort();
        assert_eq!(
            lignes,
            [
                ("/channels/c1/read".to_string(), "POST".to_string()),
                ("/channels/c3/read".to_string(), "POST".to_string()),
                ("/dms/d1/read".to_string(), "POST".to_string()),
                ("/notifications/read".to_string(), "POST".to_string()),
            ]
        );
        attendre(&ailleurs, &mut abonne, &mut Vec::new(), |e| {
            pastilles(e) == (0, 0)
        })
        .await;
        suivi.abort();
    }

    #[tokio::test]
    async fn un_changement_de_statut_est_vu_par_un_autre_client_en_temps_reel() {
        let serveur = monde(monde_a()).await;
        let (vers_autre, flux_autre) = FluxTempsReel::factice();
        relayer(&serveur, "PUT", "/me/status", &vers_autre, |requete| {
            (
                "status.updated",
                json!({"user_id": "me", "status": requete.json()["status"]}),
            )
        });
        let moi = Store::nouveau(serveur.client(), &[]);
        let autre = Store::nouveau(serveur.client(), &[]);
        moi.charger().await.unwrap();
        autre.charger().await.unwrap();
        let mut abonne = autre.abonner();
        let suivi = autre.brancher(flux_autre);
        for (statut, presence) in [
            (StatutPresence::Dnd, Presence::NePasDeranger),
            (StatutPresence::Idle, Presence::Absent),
            (StatutPresence::Online, Presence::EnLigne),
        ] {
            moi.definir_statut(statut).await.unwrap();
            assert_eq!(moi.lire(|e| e.presence_de("me")), presence);
            let mut recus = Vec::new();
            attendre(&autre, &mut abonne, &mut recus, |e| {
                e.presence_de("me") == presence
            })
            .await;
            while let Ok(lot) = abonne.try_recv() {
                recus.extend(lot);
            }
            assert!(recus.contains(&Changement::Presence {
                utilisateur: "me".into()
            }));
        }
        suivi.abort();
    }

    #[tokio::test]
    async fn ouvrir_un_message_prive_avec_une_personne_deja_en_conversation_retrouve_l_existante() {
        let serveur = monde(monde_a()).await;
        serveur.router(
            "POST",
            "/dms",
            Reponse::json(
                200,
                json!({"id": "d1", "other": {"id": "dave", "pseudo": "dave"}, "online": true}),
            ),
        );
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        let mut abonne = store.abonner();
        assert_eq!(store.ouvrir_message_prive("dave").await.unwrap(), "d1");
        assert_eq!(store.lire(|e| e.messages_prives.len()), 1);
        assert!(abonne.try_recv().is_err());
        assert_eq!(
            serveur.requetes().last().unwrap().json(),
            json!({"pseudo": "dave"})
        );
        serveur.router(
            "POST",
            "/dms",
            Reponse::json(
                200,
                json!({"id": "d2", "other": {"id": "carol", "pseudo": "carol"}, "online": true}),
            ),
        );
        assert_eq!(store.ouvrir_message_prive("carol").await.unwrap(), "d2");
        let ids: Vec<String> = store.lire(|e| {
            e.messages_prives
                .iter()
                .map(|d| id_de(&d.id).to_string())
                .collect()
        });
        assert_eq!(ids, ["d2", "d1"]);
        assert!(abonne
            .try_recv()
            .unwrap()
            .contains(&Changement::MessagesPrives));
        serveur.router(
            "POST",
            "/dms",
            Reponse::json(
                404,
                json!({"error": "user_not_found", "message": "introuvable"}),
            ),
        );
        let erreur = store.ouvrir_message_prive("zoe").await.unwrap_err();
        assert_eq!(
            crate::nouveau_prive::message_d_erreur(&erreur),
            "Aucun utilisateur avec ce pseudo."
        );
        assert_eq!(store.lire(|e| e.messages_prives.len()), 2);
    }

    #[tokio::test]
    async fn creer_rejoindre_et_quitter_mettent_le_store_a_jour() {
        let serveur = monde(monde_a()).await;
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        serveur.router(
            "POST",
            "/servers/s1/channels",
            Reponse::json(
                201,
                json!({"id": "c9", "name": "nouveau", "kind": "text", "position": 9}),
            ),
        );
        let salon = crate::creation::NouveauSalon {
            nom: "nouveau".into(),
            apres: Some("c1".into()),
            ..Default::default()
        };
        let mut abonne = store.abonner();
        assert_eq!(store.creer_salon("s1", &salon).await.unwrap(), "c9");
        let ordre: Vec<String> = store.lire(|e| {
            e.salons("s1")
                .iter()
                .filter(|c| c.kind == Some(GenreSalon::Text))
                .map(|c| id_de(&c.id).to_string())
                .collect()
        });
        assert_eq!(ordre, ["c1", "c9", "c3"], "placé après le salon choisi");
        assert!(abonne
            .try_recv()
            .unwrap()
            .contains(&Changement::SalonAjoute {
                serveur: "s1".into(),
                salon: "c9".into()
            }));
        assert_eq!(
            serveur.requetes().last().unwrap().json(),
            json!({"name": "nouveau", "kind": "text", "after": "c1"})
        );

        serveur.router(
            "POST",
            "/server-invites/ABCDE-23456/join",
            Reponse::json(200, json!({"id": "s2", "name": "Club"})),
        );
        programmer(
            &serveur,
            vec![
                (
                    "/servers",
                    json!([
                        {"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0},
                        {"id": "s2", "name": "Club", "owner_id": "eve", "icon_version": 0}
                    ]),
                ),
                (
                    "/servers/s2",
                    json!({"server": {"id": "s2", "name": "Club", "owner_id": "eve"}, "channels": [{"id": "k1", "name": "accueil", "kind": "text", "position": 1}], "members": [], "roles": []}),
                ),
            ],
        );
        let rejoint = store.rejoindre_serveur("ABCDE-23456").await.unwrap();
        assert_eq!(rejoint.id.as_deref(), Some("s2"));
        assert!(store.lire(|e| e.serveur("s2").is_some() && e.salons("s2").len() == 1));

        serveur.router("POST", "/servers/s2/leave", Reponse::vide(204));
        store.quitter_serveur("s2").await.unwrap();
        assert!(store.lire(|e| e.serveur("s2").is_none() && !e.details.contains_key("s2")));

        serveur.router(
            "POST",
            "/servers",
            Reponse::json(201, json!({"id": "s3", "name": "Neuf", "owner_id": "me"})),
        );
        serveur.router("PUT", "/servers/s3/icon", Reponse::vide(500));
        let (id, icone) = store
            .creer_serveur("  Neuf ", Some(("image/png", vec![1, 2])))
            .await
            .unwrap();
        assert_eq!((id.as_str(), icone), ("s3", false));
        assert!(serveur
            .requetes()
            .iter()
            .any(|r| r.ligne() == ("/servers", "POST") && r.json() == json!({"name": "Neuf"})));
    }

    #[tokio::test]
    async fn ouvrir_une_conversation_la_marque_lue_cote_serveur() {
        let serveur = monde(monde_a()).await;
        serveur.router("POST", "/channels/c1/read", Reponse::vide(204));
        serveur.router("POST", "/notifications/read", Reponse::vide(204));
        let store = Store::nouveau(serveur.client(), &[]);
        store.charger().await.unwrap();
        let avant = serveur.requetes().len();
        store.ouvrir(Some(cle("c:c1"))).await;
        let etat = store.instantane();
        assert_eq!(
            (etat.non_lus_de(&cle("c:c1")), etat.total_mentions()),
            (0, 0)
        );
        let requetes: Vec<_> = serveur.requetes().into_iter().skip(avant).collect();
        let lignes: Vec<_> = requetes.iter().map(|r| r.ligne()).collect();
        assert_eq!(
            lignes,
            [
                ("/notifications/read", "POST"),
                ("/channels/c1/read", "POST"),
                ("/channels/c1/messages?limit=50", "GET")
            ]
        );
        assert_eq!(requetes[0].json(), json!({"message_ids": ["5"]}));
    }

    #[tokio::test]
    async fn ordonne_les_serveurs_et_retient_la_derniere_vue() {
        let serveur = monde(monde_b()).await;
        programmer(&serveur, vec![monde_a().remove(0)]);
        let store = Store::nouveau(serveur.client(), &["s2".to_string()]);
        store.charger().await.unwrap();
        assert_eq!(store.ordre_serveurs(), ["s2", "s1"]);
        let mut abonne = store.abonner();
        assert_eq!(store.deplacer_serveur("s2", "s1"), ["s1", "s2"]);
        assert_eq!(store.ordre_serveurs(), ["s1", "s2"]);
        assert_eq!(abonne.try_recv().unwrap(), [Changement::OrdreServeurs]);
        assert_eq!(
            store.serveur_voisin(Some("s2"), Sens::Suivant).as_deref(),
            Some("")
        );
        assert_eq!(
            store.serveur_voisin(None, Sens::Suivant).as_deref(),
            Some("s1")
        );
        let vue = store.memoriser_vue("#/s/s2/t1");
        assert_eq!(vue, DerniereVue::enregistrer("me", "#/s/s2/t1"));
        assert_eq!(
            store.adresse_a_rouvrir(Some(&vue)).as_deref(),
            Some("#/s/s2/t1")
        );
        assert_eq!(store.conversation_a_rouvrir(Some(&vue)), Some(cle("c:t1")));
        let ailleurs = DerniereVue::enregistrer("autre", "#/dm/d2");
        assert_eq!(store.adresse_a_rouvrir(Some(&ailleurs)), None);
        assert_eq!(
            store.conversation_a_rouvrir(Some(&store.memoriser_vue("#/dm/d2"))),
            Some(cle("d:d2"))
        );
    }
}
