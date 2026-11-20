use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{SensAudio, SystemeAudio};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::audio::{Bascule, Capture, Lecture};
use crate::http::{Client, ErreurApi};

use super::erreurs::{CauseMicro, ErreurVocal};
use super::etat::{EtatVocal, Phase};
use super::mixage::Mixeur;
use super::salle::{
    est_partage, proprietaire, Connecteur, Connexion, EvenementSalle, JetonSalle,
    RaisonDeconnexion, Salle, SortieMicro, SourceAudio,
};

pub const DELAI_FERMETURE: Duration = Duration::from_secs(3);
pub const MESSAGE_CONNEXION_PERDUE: &str = "Connexion au vocal perdue.";

pub trait ApiVocale: Send + Sync {
    fn jeton(&self, salon: &str) -> BoxFuture<'_, Result<JetonSalle, ErreurApi>>;
}

impl ApiVocale for Client {
    fn jeton(&self, salon: &str) -> BoxFuture<'_, Result<JetonSalle, ErreurApi>> {
        let salon = salon.to_string();
        Box::pin(async move {
            let reponse = self.jeton_vocal(&salon).await?;
            match (reponse.url, reponse.token) {
                (Some(url), Some(jeton)) if !url.is_empty() && !jeton.is_empty() => {
                    Ok(JetonSalle {
                        url,
                        jeton,
                        salle: reponse.room.unwrap_or_default(),
                        max_participants: reponse
                            .max_participants
                            .and_then(|m| u32::try_from(m).ok()),
                    })
                }
                _ => Err(ErreurApi::ReponseInvalide(
                    "jeton vocal sans adresse ni jeton".to_string(),
                )),
            }
        })
    }
}

pub trait TraitementMicro: Send {
    fn traiter(&mut self, trame: &mut [i16]);
}

#[derive(Debug, Clone)]
pub struct Reglages {
    pub micro: Option<String>,
    pub sortie: Option<String>,
    pub surveillance: Duration,
    pub attentes: Vec<Duration>,
    pub fenetre_deplacement: Duration,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            micro: None,
            sortie: None,
            surveillance: Duration::from_secs(2),
            attentes: [1, 2, 4, 8, 10, 10, 10, 10, 10, 10, 10, 10]
                .into_iter()
                .map(Duration::from_secs)
                .collect(),
            fenetre_deplacement: Duration::from_secs(5),
        }
    }
}

pub struct Dependances {
    pub api: Arc<dyn ApiVocale>,
    pub connecteur: Arc<dyn Connecteur>,
    pub audio: Arc<dyn SystemeAudio>,
    pub reglages: Reglages,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvisVocal {
    Rejoint(String),
    Quitte(String),
    Arrivee(String),
    Depart(String),
    Deplace(String),
    Peripherique(SensAudio, Bascule),
}

enum Commande {
    Rejoindre(String),
    Deplacer(String),
    DeplaceParModeration {
        de: String,
        vers: String,
    },
    Quitter,
    BasculerMicro,
    BasculerCasque,
    Imposer {
        micro: bool,
        son: bool,
    },
    Porte(bool),
    ChoisirMicro(Option<String>),
    ChoisirSortie(Option<String>),
    Volume {
        identite: String,
        source: SourceAudio,
        gain: f32,
    },
    Traitement(Box<dyn TraitementMicro>),
    Fermer(oneshot::Sender<()>),
}

#[derive(Debug)]
enum EchecConnexion {
    Api(ErreurVocal),
    Salle,
}

impl EchecConnexion {
    fn reessayable(&self) -> bool {
        matches!(
            self,
            Self::Api(ErreurVocal::Connexion | ErreurVocal::Indisponible) | Self::Salle
        )
    }

    fn message(&self) -> String {
        match self {
            Self::Api(e) => e.message(),
            Self::Salle => ErreurVocal::Connexion.message(),
        }
    }
}

struct Tentative {
    generation: u64,
    resultat: Result<Connexion, EchecConnexion>,
}

#[derive(Default)]
struct RouteMicro {
    sortie: Mutex<Option<Arc<dyn SortieMicro>>>,
    emet: AtomicBool,
    traitement: Mutex<Option<Box<dyn TraitementMicro>>>,
}

impl RouteMicro {
    fn envoyer(&self, trame: &[i16]) {
        if !self.emet.load(Ordering::Relaxed) {
            return;
        }
        let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) else {
            return;
        };
        let mut traitement = self.traitement.lock().ok();
        match traitement.as_mut().and_then(|t| t.as_mut()) {
            Some(traitement) => {
                let mut copie = trame.to_vec();
                traitement.traiter(&mut copie);
                sortie.envoyer(&copie);
            }
            None => sortie.envoyer(trame),
        }
    }

    fn brancher(&self, sortie: Option<Arc<dyn SortieMicro>>) {
        if let Ok(mut s) = self.sortie.lock() {
            *s = sortie;
        }
    }

    fn appliquer(&self, emet: bool) {
        self.emet.store(emet, Ordering::Relaxed);
        if let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) {
            sortie.couper(!emet);
        }
    }
}

pub struct SessionVocale {
    commandes: mpsc::UnboundedSender<Commande>,
    etat: watch::Receiver<EtatVocal>,
    avis: broadcast::Sender<AvisVocal>,
    runtime: Handle,
}

impl SessionVocale {
    pub fn demarrer(runtime: &Handle, dependances: Dependances) -> Self {
        let (commandes, reception) = mpsc::unbounded_channel();
        let mut etat_initial = EtatVocal::default();
        etat_initial.micro = dependances.reglages.micro.clone();
        etat_initial.sortie = dependances.reglages.sortie.clone();
        let (diffusion, etat) = watch::channel(etat_initial.clone());
        let (avis, _) = broadcast::channel(64);
        let acteur = Acteur::nouveau(dependances, etat_initial, diffusion, avis.clone());
        runtime.spawn(acteur.tourner(reception));
        Self {
            commandes,
            etat,
            avis,
            runtime: runtime.clone(),
        }
    }

    fn envoyer(&self, commande: Commande) {
        let _ = self.commandes.send(commande);
    }

    pub fn etat(&self) -> watch::Receiver<EtatVocal> {
        self.etat.clone()
    }

    pub fn instantane(&self) -> EtatVocal {
        self.etat.borrow().clone()
    }

    pub fn abonner_avis(&self) -> broadcast::Receiver<AvisVocal> {
        self.avis.subscribe()
    }

    pub fn rejoindre(&self, salon: &str) {
        self.envoyer(Commande::Rejoindre(salon.to_string()));
    }

    pub fn deplacer(&self, salon: &str) {
        self.envoyer(Commande::Deplacer(salon.to_string()));
    }

    pub fn deplace_par_moderation(&self, de: &str, vers: &str) {
        self.envoyer(Commande::DeplaceParModeration {
            de: de.to_string(),
            vers: vers.to_string(),
        });
    }

    pub fn quitter(&self) {
        self.envoyer(Commande::Quitter);
    }

    pub fn basculer_micro(&self) {
        self.envoyer(Commande::BasculerMicro);
    }

    pub fn basculer_casque(&self) {
        self.envoyer(Commande::BasculerCasque);
    }

    pub fn imposer(&self, micro_coupe: bool, son_coupe: bool) {
        self.envoyer(Commande::Imposer {
            micro: micro_coupe,
            son: son_coupe,
        });
    }

    pub fn definir_porte(&self, ouverte: bool) {
        self.envoyer(Commande::Porte(ouverte));
    }

    pub fn choisir_micro(&self, id: Option<String>) {
        self.envoyer(Commande::ChoisirMicro(id));
    }

    pub fn choisir_sortie(&self, id: Option<String>) {
        self.envoyer(Commande::ChoisirSortie(id));
    }

    pub fn definir_volume(&self, identite: &str, source: SourceAudio, gain: f32) {
        self.envoyer(Commande::Volume {
            identite: identite.to_string(),
            source,
            gain,
        });
    }

    pub fn definir_traitement(&self, traitement: Box<dyn TraitementMicro>) {
        self.envoyer(Commande::Traitement(traitement));
    }

    pub async fn fermer(&self) {
        let (fin, attente) = oneshot::channel();
        if self.commandes.send(Commande::Fermer(fin)).is_ok() {
            let _ = tokio::time::timeout(DELAI_FERMETURE, attente).await;
        }
    }

    pub fn fermer_bloquant(&self, delai: Duration) -> bool {
        let (fin, attente) = oneshot::channel();
        if self.commandes.send(Commande::Fermer(fin)).is_err() {
            return true;
        }
        let (signal, reception) = std::sync::mpsc::channel();
        self.runtime.spawn(async move {
            let _ = attente.await;
            let _ = signal.send(());
        });
        reception.recv_timeout(delai).is_ok()
    }
}

struct Acteur {
    api: Arc<dyn ApiVocale>,
    connecteur: Arc<dyn Connecteur>,
    audio: Arc<dyn SystemeAudio>,
    reglages: Reglages,
    etat: EtatVocal,
    diffusion: watch::Sender<EtatVocal>,
    avis: broadcast::Sender<AvisVocal>,
    generation: u64,
    salle: Option<Arc<dyn Salle>>,
    ancienne: Option<Arc<dyn Salle>>,
    evenements: Option<mpsc::UnboundedReceiver<EvenementSalle>>,
    route: Arc<RouteMicro>,
    capture: Option<Capture>,
    lecture: Option<Lecture>,
    mixeur: Arc<Mutex<Mixeur>>,
    pistes: BTreeMap<(String, SourceAudio), JoinHandle<()>>,
    tentatives: (
        mpsc::UnboundedSender<Tentative>,
        mpsc::UnboundedReceiver<Tentative>,
    ),
    essais: usize,
    reconnexion: Option<Instant>,
    dernier_quitte: Option<(String, Instant)>,
}

async fn attendre(moment: Option<Instant>) {
    match moment {
        Some(moment) => tokio::time::sleep_until(moment).await,
        None => std::future::pending().await,
    }
}

async fn suivant(
    evenements: &mut Option<mpsc::UnboundedReceiver<EvenementSalle>>,
) -> Option<EvenementSalle> {
    match evenements {
        Some(e) => e.recv().await,
        None => std::future::pending().await,
    }
}

impl Acteur {
    fn nouveau(
        dependances: Dependances,
        etat: EtatVocal,
        diffusion: watch::Sender<EtatVocal>,
        avis: broadcast::Sender<AvisVocal>,
    ) -> Self {
        Self {
            api: dependances.api,
            connecteur: dependances.connecteur,
            audio: dependances.audio,
            reglages: dependances.reglages,
            etat,
            diffusion,
            avis,
            generation: 0,
            salle: None,
            ancienne: None,
            evenements: None,
            route: Arc::new(RouteMicro::default()),
            capture: None,
            lecture: None,
            mixeur: Arc::new(Mutex::new(Mixeur::default())),
            pistes: BTreeMap::new(),
            tentatives: mpsc::unbounded_channel(),
            essais: 0,
            reconnexion: None,
            dernier_quitte: None,
        }
    }

    async fn tourner(mut self, mut commandes: mpsc::UnboundedReceiver<Commande>) {
        let mut surveillance = tokio::time::interval(self.reglages.surveillance);
        surveillance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                commande = commandes.recv() => match commande {
                    Some(Commande::Fermer(fin)) => {
                        self.quitter().await;
                        let _ = fin.send(());
                        break;
                    }
                    Some(commande) => self.commande(commande).await,
                    None => {
                        self.quitter().await;
                        break;
                    }
                },
                Some(tentative) = self.tentatives.1.recv() => self.tentative(tentative).await,
                evenement = suivant(&mut self.evenements) => match evenement {
                    Some(evenement) => self.evenement(evenement).await,
                    None => self.evenement(EvenementSalle::Deconnecte(RaisonDeconnexion::Inconnue)).await,
                },
                _ = attendre(self.reconnexion) => {
                    self.reconnexion = None;
                    self.connecter();
                },
                _ = surveillance.tick() => self.surveiller(),
            }
        }
    }

    fn publier(&self) {
        self.diffusion.send_replace(self.etat.clone());
    }

    fn signaler(&self, avis: AvisVocal) {
        let _ = self.avis.send(avis);
    }

    fn appliquer_micro(&self) {
        self.route.appliquer(self.etat.micro_emet());
        if let Ok(mut mixeur) = self.mixeur.lock() {
            mixeur.definir_muet(!self.etat.son_recu());
        }
    }

    async fn publier_attributs(&mut self) {
        if let Some(salle) = &self.salle {
            let _ = salle.definir_attributs(self.etat.attributs()).await;
        }
    }

    async fn commande(&mut self, commande: Commande) {
        match commande {
            Commande::Rejoindre(salon) => self.rejoindre(salon, false).await,
            Commande::Deplacer(salon) => self.rejoindre(salon, true).await,
            Commande::DeplaceParModeration { de, vers } => {
                let recent = self.dernier_quitte.as_ref().is_some_and(|(salon, quand)| {
                    salon == &de && quand.elapsed() < self.reglages.fenetre_deplacement
                });
                if self.etat.dans(&de) || (!self.etat.actif() && recent) {
                    self.signaler(AvisVocal::Deplace(vers.clone()));
                    let echange = self.etat.phase == Phase::Connecte;
                    self.rejoindre(vers, echange).await;
                }
            }
            Commande::Quitter => self.quitter().await,
            Commande::BasculerMicro => {
                if self.etat.basculer_micro() {
                    self.appliquer_micro();
                    self.publier();
                    self.publier_attributs().await;
                }
            }
            Commande::BasculerCasque => {
                if self.etat.basculer_casque() {
                    self.appliquer_micro();
                    self.publier();
                    self.publier_attributs().await;
                }
            }
            Commande::Imposer { micro, son } => {
                let avant = self.etat.micro_coupe;
                self.etat.imposer(micro, son);
                self.appliquer_micro();
                self.publier();
                if avant != self.etat.micro_coupe {
                    self.publier_attributs().await;
                }
            }
            Commande::Porte(ouverte) => {
                self.etat.porte_ouverte = ouverte;
                self.appliquer_micro();
                self.publier();
            }
            Commande::ChoisirMicro(id) => {
                self.etat.micro = id.clone();
                self.reglages.micro = id.clone();
                if let Some(bascule) = self.capture.as_mut().and_then(|c| c.choisir(id).ok()) {
                    self.signaler(AvisVocal::Peripherique(SensAudio::Entree, bascule));
                }
                self.publier();
            }
            Commande::ChoisirSortie(id) => {
                self.etat.sortie = id.clone();
                self.reglages.sortie = id.clone();
                if let Some(bascule) = self.lecture.as_mut().and_then(|l| l.choisir(id).ok()) {
                    self.signaler(AvisVocal::Peripherique(SensAudio::Sortie, bascule));
                }
                self.publier();
            }
            Commande::Volume {
                identite,
                source,
                gain,
            } => {
                if let Ok(mut mixeur) = self.mixeur.lock() {
                    mixeur.definir_gain(&identite, source, gain);
                }
            }
            Commande::Traitement(traitement) => {
                if let Ok(mut t) = self.route.traitement.lock() {
                    *t = Some(traitement);
                }
            }
            Commande::Fermer(_) => {}
        }
    }

    async fn rejoindre(&mut self, salon: String, echange: bool) {
        if self.etat.dans(&salon) {
            return;
        }
        if echange && self.salle.is_some() {
            self.ancienne = self.salle.take();
            self.evenements = None;
            self.route.brancher(None);
            self.arreter_pistes();
            self.etat.imposer(false, false);
        } else {
            self.quitter().await;
        }
        self.etat.rejoindre(&salon);
        self.essais = 0;
        self.publier();
        self.connecter();
    }

    fn connecter(&mut self) {
        let Some(salon) = self.etat.salon.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let api = self.api.clone();
        let connecteur = self.connecteur.clone();
        let retour = self.tentatives.0.clone();
        tokio::spawn(async move {
            let resultat = match api.jeton(&salon).await {
                Ok(jeton) => connecteur
                    .connecter(jeton)
                    .await
                    .map_err(|_| EchecConnexion::Salle),
                Err(e) => Err(EchecConnexion::Api(ErreurVocal::depuis_api(&e))),
            };
            let _ = retour.send(Tentative {
                generation,
                resultat,
            });
        });
    }

    async fn tentative(&mut self, tentative: Tentative) {
        if tentative.generation != self.generation {
            if let Ok(connexion) = tentative.resultat {
                connexion.salle.fermer().await;
            }
            return;
        }
        match tentative.resultat {
            Ok(connexion) => self.connecte(connexion).await,
            Err(echec) if self.essais > 0 && echec.reessayable() => self.programmer_reconnexion(),
            Err(echec) => {
                let message = echec.message();
                self.quitter().await;
                self.etat.erreur = Some(message);
                self.publier();
            }
        }
    }

    async fn connecte(&mut self, connexion: Connexion) {
        if let Some(ancienne) = self.ancienne.take() {
            ancienne.fermer().await;
        }
        let reprise = self.essais > 0;
        self.essais = 0;
        let salle = connexion.salle;
        self.etat.connecte(salle.identite(), connexion.participants);
        self.evenements = Some(connexion.evenements);
        self.salle = Some(salle.clone());
        if self.lecture.is_none() {
            self.lecture = Lecture::ouvrir(
                self.audio.clone(),
                self.reglages.sortie.clone(),
                self.mixeur.clone(),
            )
            .ok();
        }
        if self.capture.is_none() {
            let route = self.route.clone();
            match Capture::ouvrir(
                self.audio.clone(),
                self.reglages.micro.clone(),
                Box::new(move |trame| route.envoyer(trame)),
            ) {
                Ok(capture) => {
                    self.capture = Some(capture);
                    self.etat.micro_indisponible = None;
                }
                Err(e) => {
                    let cause = CauseMicro::depuis_plateforme(&e);
                    self.etat.micro_indisponible = Some(cause);
                    self.etat.micro_coupe = true;
                    self.etat.erreur = Some(cause.erreur());
                }
            }
        }
        if self.capture.is_some() {
            match salle.publier_micro().await {
                Ok(sortie) => self.route.brancher(Some(sortie)),
                Err(_) if self.etat.micro_impose => {}
                Err(_) => {
                    self.capture = None;
                    self.etat.micro_indisponible = Some(CauseMicro::Inutilisable);
                    self.etat.micro_coupe = true;
                    self.etat.erreur = Some(CauseMicro::Inutilisable.erreur());
                }
            }
        }
        self.appliquer_micro();
        self.publier();
        self.publier_attributs().await;
        if !reprise {
            if let Some(salon) = self.etat.salon.clone() {
                self.signaler(AvisVocal::Rejoint(salon));
            }
        }
    }

    fn programmer_reconnexion(&mut self) {
        match self.reglages.attentes.get(self.essais).copied() {
            Some(attente) => {
                self.essais += 1;
                self.etat.phase = Phase::Reconnexion;
                self.reconnexion = Some(Instant::now() + attente);
                self.publier();
            }
            None => {
                let salon = self.etat.salon.clone();
                self.liberer();
                self.dernier_quitte = salon.map(|s| (s, Instant::now()));
                self.etat.reinitialiser();
                self.etat.erreur = Some(MESSAGE_CONNEXION_PERDUE.to_string());
                self.publier();
            }
        }
    }

    async fn evenement(&mut self, evenement: EvenementSalle) {
        match evenement {
            EvenementSalle::Arrivee(participant) => {
                if est_partage(&participant.identite) {
                    return;
                }
                let identite = participant.identite.clone();
                self.etat.participants.insert(identite.clone(), participant);
                self.signaler(AvisVocal::Arrivee(identite));
            }
            EvenementSalle::Depart(identite) => {
                if self.etat.participants.remove(&identite).is_some() {
                    self.etat.parlent.remove(&identite);
                    self.signaler(AvisVocal::Depart(identite));
                }
            }
            EvenementSalle::Attributs {
                identite,
                attributs,
            } => {
                if let Some(p) = self.etat.participants.get_mut(&identite) {
                    p.attributs = attributs;
                }
            }
            EvenementSalle::Qualite { identite, qualite } => {
                if let Some(p) = self.etat.participants.get_mut(&identite) {
                    p.qualite = qualite;
                }
            }
            EvenementSalle::Parole(identites) => {
                self.etat.parlent = identites.into_iter().filter(|i| !est_partage(i)).collect();
            }
            EvenementSalle::Permissions {
                publier_micro,
                recevoir,
            } => {
                let avant = self.etat.micro_coupe;
                self.etat.imposer(!publier_micro, !recevoir);
                self.appliquer_micro();
                if avant != self.etat.micro_coupe {
                    self.publier();
                    self.publier_attributs().await;
                }
            }
            EvenementSalle::Reconnexion => self.etat.phase = Phase::Reconnexion,
            EvenementSalle::Reconnecte => {
                self.etat.phase = Phase::Connecte;
                self.publier();
                self.publier_attributs().await;
            }
            EvenementSalle::Deconnecte(raison) if raison.definitive() => {
                let salon = self.etat.salon.clone();
                self.liberer();
                self.dernier_quitte = salon.clone().map(|s| (s, Instant::now()));
                self.etat.reinitialiser();
                if let Some(salon) = salon {
                    self.signaler(AvisVocal::Quitte(salon));
                }
            }
            EvenementSalle::Deconnecte(_) => {
                if let Some(salle) = self.salle.take() {
                    salle.fermer().await;
                }
                self.evenements = None;
                self.route.brancher(None);
                self.arreter_pistes();
                self.programmer_reconnexion();
            }
            EvenementSalle::PisteAudio {
                identite,
                source,
                mut trames,
            } => {
                let proprietaire = proprietaire(&identite).to_string();
                let source = if est_partage(&identite) {
                    SourceAudio::Partage
                } else {
                    source
                };
                if let Ok(mut mixeur) = self.mixeur.lock() {
                    mixeur.ajouter(&proprietaire, source);
                }
                let mixeur = self.mixeur.clone();
                let cle = proprietaire.clone();
                let tache = tokio::spawn(async move {
                    while let Some(trame) = trames.next().await {
                        if let Ok(mut mixeur) = mixeur.lock() {
                            mixeur.pousser(&cle, source, &trame);
                        }
                    }
                });
                if let Some(precedente) = self.pistes.insert((proprietaire, source), tache) {
                    precedente.abort();
                }
            }
            EvenementSalle::PisteRetiree { identite, source } => {
                let proprietaire = proprietaire(&identite).to_string();
                let source = if est_partage(&identite) {
                    SourceAudio::Partage
                } else {
                    source
                };
                if let Some(tache) = self.pistes.remove(&(proprietaire.clone(), source)) {
                    tache.abort();
                }
                if let Ok(mut mixeur) = self.mixeur.lock() {
                    mixeur.retirer(&proprietaire, source);
                }
            }
        }
        self.publier();
    }

    fn surveiller(&mut self) {
        if let Some(bascule) = self.capture.as_mut().and_then(Capture::surveiller) {
            self.signaler(AvisVocal::Peripherique(SensAudio::Entree, bascule));
        }
        if let Some(bascule) = self.lecture.as_mut().and_then(Lecture::surveiller) {
            self.signaler(AvisVocal::Peripherique(SensAudio::Sortie, bascule));
        }
    }

    fn arreter_pistes(&mut self) {
        for (_, tache) in std::mem::take(&mut self.pistes) {
            tache.abort();
        }
        if let Ok(mut mixeur) = self.mixeur.lock() {
            mixeur.vider();
        }
    }

    fn liberer(&mut self) {
        self.generation += 1;
        self.reconnexion = None;
        self.essais = 0;
        self.salle = None;
        self.ancienne = None;
        self.evenements = None;
        self.route.brancher(None);
        self.route.appliquer(false);
        self.arreter_pistes();
        self.capture = None;
        self.lecture = None;
    }

    async fn quitter(&mut self) {
        let salles: Vec<_> = self
            .salle
            .take()
            .into_iter()
            .chain(self.ancienne.take())
            .collect();
        let salon = self.etat.actif().then(|| self.etat.salon.clone()).flatten();
        self.liberer();
        for salle in salles {
            salle.fermer().await;
        }
        if let Some(salon) = salon {
            self.dernier_quitte = Some((salon.clone(), Instant::now()));
            self.signaler(AvisVocal::Quitte(salon));
        }
        self.etat.reinitialiser();
        self.publier();
    }
}
