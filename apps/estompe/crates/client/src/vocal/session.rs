use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{ErreurCamera, SensAudio, SystemeAudio, SystemeCamera};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::audio::{
    Apm, Bascule, Capture, ChaineMicro, Lecture, MesureMicro, NiveauMicro, ReglagesMicro,
};
use crate::http::{Client, ErreurApi};

use super::camera::{message_camera, CaptureCamera, QualiteCamera, SignalCamera, SortieImages};
use super::erreurs::{CauseMicro, ErreurVocal};
use super::etat::{EtatVocal, Phase};
use super::mixage::{Ecoute, Mixeur};
use super::salle::{
    est_partage, proprietaire, Connecteur, Connexion, EvenementSalle, JetonSalle,
    RaisonDeconnexion, Salle, SortieMicro, SortieVideo, SourceAudio,
};
use super::video::{CleVideo, DemandeVideo, ImageI420, RecepteurVideo, SourceVideo, Videos};

pub const DELAI_FERMETURE: Duration = Duration::from_secs(3);
pub const MESSAGE_CONNEXION_PERDUE: &str = "Connexion au vocal perdue.";
pub const MESSAGE_PUBLICATION_CAMERA: &str = "Impossible de publier la caméra dans le salon.";

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

#[derive(Debug, Clone)]
pub struct Reglages {
    pub micro: Option<String>,
    pub sortie: Option<String>,
    pub surveillance: Duration,
    pub attentes: Vec<Duration>,
    pub fenetre_deplacement: Duration,
    pub traitement: ReglagesMicro,
    pub ecoute: Ecoute,
    pub camera: Option<String>,
    pub qualite_camera: QualiteCamera,
    pub regarder_auto: bool,
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
            traitement: ReglagesMicro::default(),
            ecoute: Ecoute::default(),
            camera: None,
            qualite_camera: QualiteCamera::default(),
            regarder_auto: false,
        }
    }
}

pub struct Dependances {
    pub api: Arc<dyn ApiVocale>,
    pub connecteur: Arc<dyn Connecteur>,
    pub audio: Arc<dyn SystemeAudio>,
    pub cameras: Arc<dyn SystemeCamera>,
    pub apm: Option<Arc<Apm>>,
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
    RejoindreAvecCamera(String),
    Camera(bool),
    ChoisirCamera(Option<String>),
    QualiteCamera(QualiteCamera),
    CameraPerdue(ErreurCamera, u64),
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
        pourcent: u16,
    },
    Muet {
        identite: String,
        source: SourceAudio,
        muet: bool,
    },
    VolumeGeneral(u16),
    Traitement(ReglagesMicro),
    Affichage(BTreeMap<CleVideo, DemandeVideo>),
    Regarder(String, Option<bool>),
    RegarderAuto(bool),
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

struct RouteMicro {
    sortie: Mutex<Option<Arc<dyn SortieMicro>>>,
    emet: AtomicBool,
    chaine: Mutex<ChaineMicro>,
}

#[derive(Default)]
struct RouteCamera {
    sortie: Mutex<Option<Arc<dyn SortieVideo>>>,
    recepteur: Mutex<Option<Arc<RecepteurVideo>>>,
}

impl RouteCamera {
    fn envoyer(&self, image: Arc<ImageI420>) {
        if let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) {
            sortie.envoyer(&image);
        }
        if let Some(recepteur) = self.recepteur.lock().ok().and_then(|r| r.clone()) {
            recepteur.pousser(image);
        }
    }

    fn brancher(&self, sortie: Option<Arc<dyn SortieVideo>>) {
        if let Ok(mut s) = self.sortie.lock() {
            *s = sortie;
        }
    }

    fn afficher(&self, recepteur: Option<Arc<RecepteurVideo>>) {
        let ancien = match self.recepteur.lock() {
            Ok(mut r) => std::mem::replace(&mut *r, recepteur),
            Err(_) => None,
        };
        if let Some(ancien) = ancien {
            ancien.effacer();
        }
    }
}

impl RouteMicro {
    fn nouvelle(chaine: ChaineMicro) -> Self {
        Self {
            sortie: Mutex::new(None),
            emet: AtomicBool::new(false),
            chaine: Mutex::new(chaine),
        }
    }

    fn envoyer(&self, trame: &[i16]) {
        let Ok(mut chaine) = self.chaine.lock() else {
            return;
        };
        let traitee = chaine.traiter(trame);
        if !self.emet.load(Ordering::Relaxed) {
            return;
        }
        if let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) {
            sortie.envoyer(traitee);
        }
    }

    fn regler(&self, reglages: ReglagesMicro) {
        if let Ok(mut chaine) = self.chaine.lock() {
            chaine.appliquer(reglages);
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
    mesure: Arc<MesureMicro>,
    videos: Arc<Videos>,
    runtime: Handle,
}

impl SessionVocale {
    pub fn demarrer(runtime: &Handle, dependances: Dependances) -> Self {
        let (commandes, reception) = mpsc::unbounded_channel();
        let mut etat_initial = EtatVocal::default();
        etat_initial.camera = dependances.reglages.camera.clone();
        etat_initial.qualite_camera = dependances.reglages.qualite_camera;
        etat_initial.micro = dependances.reglages.micro.clone();
        etat_initial.sortie = dependances.reglages.sortie.clone();
        etat_initial.traitement = dependances.reglages.traitement.borne();
        etat_initial.ecoute = dependances.reglages.ecoute.clone().bornee();
        etat_initial.regarder_auto = dependances.reglages.regarder_auto;
        let (diffusion, etat) = watch::channel(etat_initial.clone());
        let (avis, _) = broadcast::channel(64);
        let mesure = Arc::new(MesureMicro::default());
        let videos = Videos::nouvelles();
        let acteur = Acteur::nouveau(
            dependances,
            etat_initial,
            diffusion,
            avis.clone(),
            mesure.clone(),
            videos.clone(),
            commandes.clone(),
        );
        runtime.spawn(acteur.tourner(reception));
        Self {
            commandes,
            etat,
            avis,
            mesure,
            videos,
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

    pub fn rejoindre_avec_camera(&self, salon: &str) {
        self.envoyer(Commande::RejoindreAvecCamera(salon.to_string()));
    }

    pub fn activer_camera(&self, active: bool) {
        self.envoyer(Commande::Camera(active));
    }

    pub fn choisir_camera(&self, id: Option<String>) {
        self.envoyer(Commande::ChoisirCamera(id));
    }

    pub fn definir_qualite_camera(&self, qualite: QualiteCamera) {
        self.envoyer(Commande::QualiteCamera(qualite));
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

    pub fn definir_volume(&self, identite: &str, source: SourceAudio, pourcent: u16) {
        self.envoyer(Commande::Volume {
            identite: proprietaire(identite).to_string(),
            source,
            pourcent,
        });
    }

    pub fn definir_muet(&self, identite: &str, source: SourceAudio, muet: bool) {
        self.envoyer(Commande::Muet {
            identite: proprietaire(identite).to_string(),
            source,
            muet,
        });
    }

    pub fn definir_volume_general(&self, pourcent: u16) {
        self.envoyer(Commande::VolumeGeneral(pourcent));
    }

    pub fn definir_traitement(&self, reglages: ReglagesMicro) {
        self.envoyer(Commande::Traitement(reglages));
    }

    pub fn videos(&self) -> Arc<Videos> {
        self.videos.clone()
    }

    pub fn definir_affichage(&self, demandes: BTreeMap<CleVideo, DemandeVideo>) {
        self.envoyer(Commande::Affichage(demandes));
    }

    pub fn regarder(&self, identite: &str, regarde: Option<bool>) {
        self.envoyer(Commande::Regarder(identite.to_string(), regarde));
    }

    pub fn definir_regarder_auto(&self, auto: bool) {
        self.envoyer(Commande::RegarderAuto(auto));
    }

    pub fn niveau_micro(&self) -> NiveauMicro {
        self.mesure.lire()
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
    mesure: Arc<MesureMicro>,
    capture: Option<Capture>,
    lecture: Option<Lecture>,
    mixeur: Arc<Mutex<Mixeur>>,
    pistes: BTreeMap<(String, SourceAudio), JoinHandle<()>>,
    videos: Arc<Videos>,
    publications_video: BTreeMap<CleVideo, String>,
    pistes_video: BTreeMap<CleVideo, JoinHandle<()>>,
    affichage: BTreeMap<CleVideo, DemandeVideo>,
    abonnements_video: BTreeMap<CleVideo, DemandeVideo>,
    sons_partage: BTreeMap<String, String>,
    sons_abonnes: BTreeSet<String>,
    cameras: Arc<dyn SystemeCamera>,
    capture_camera: Option<CaptureCamera>,
    route_camera: Arc<RouteCamera>,
    camera_voulue: bool,
    generation_camera: u64,
    retour: mpsc::UnboundedSender<Commande>,
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
        mesure: Arc<MesureMicro>,
        videos: Arc<Videos>,
        retour: mpsc::UnboundedSender<Commande>,
    ) -> Self {
        let chaine =
            ChaineMicro::nouvelle(etat.traitement, dependances.apm.clone(), mesure.clone());
        let mixeur = Mixeur::nouveau(etat.ecoute.clone(), dependances.apm);
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
            route: Arc::new(RouteMicro::nouvelle(chaine)),
            mesure,
            capture: None,
            lecture: None,
            mixeur: Arc::new(Mutex::new(mixeur)),
            pistes: BTreeMap::new(),
            videos,
            publications_video: BTreeMap::new(),
            pistes_video: BTreeMap::new(),
            affichage: BTreeMap::new(),
            abonnements_video: BTreeMap::new(),
            sons_partage: BTreeMap::new(),
            sons_abonnes: BTreeSet::new(),
            cameras: dependances.cameras,
            capture_camera: None,
            route_camera: Arc::new(RouteCamera::default()),
            camera_voulue: false,
            generation_camera: 0,
            retour,
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
            mixeur.definir_sourd(!self.etat.son_recu());
        }
    }

    fn appliquer_ecoute(&self) {
        if let Ok(mut mixeur) = self.mixeur.lock() {
            mixeur.definir_ecoute(self.etat.ecoute.clone());
        }
        self.publier();
    }

    async fn publier_attributs(&mut self) {
        if let Some(salle) = &self.salle {
            let _ = salle.definir_attributs(self.etat.attributs()).await;
        }
    }

    async fn commande(&mut self, commande: Commande) {
        match commande {
            Commande::Rejoindre(salon) => self.rejoindre(salon, false).await,
            Commande::RejoindreAvecCamera(salon) => {
                self.rejoindre(salon, false).await;
                self.camera_voulue = true;
                self.etat.camera_erreur = None;
                if self.etat.phase == Phase::Connecte {
                    self.demarrer_camera().await;
                }
            }
            Commande::Camera(true) => {
                self.camera_voulue = true;
                self.etat.camera_erreur = None;
                if self.salle.is_some() && self.capture_camera.is_none() {
                    self.demarrer_camera().await;
                }
            }
            Commande::Camera(false) => {
                self.camera_voulue = false;
                self.etat.camera_erreur = None;
                self.arreter_camera().await;
            }
            Commande::ChoisirCamera(id) => {
                self.etat.camera = id.clone();
                self.reglages.camera = id;
                if self.capture_camera.take().is_some() {
                    match self.ouvrir_capture().await {
                        Ok(capture) => self.capture_camera = Some(capture),
                        Err(message) => {
                            self.arreter_camera().await;
                            self.camera_voulue = false;
                            self.etat.camera_erreur = (!message.is_empty()).then_some(message);
                        }
                    }
                }
                self.publier();
            }
            Commande::QualiteCamera(qualite) => {
                self.etat.qualite_camera = qualite;
                self.reglages.qualite_camera = qualite;
                if self.capture_camera.is_some() {
                    self.arreter_camera().await;
                    self.demarrer_camera().await;
                }
                self.publier();
            }
            Commande::CameraPerdue(erreur, generation) => {
                if generation == self.generation_camera && self.capture_camera.is_some() {
                    self.camera_voulue = false;
                    self.arreter_camera().await;
                    self.etat.camera_erreur = Some(message_camera(&erreur));
                    self.publier();
                }
            }
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
                pourcent,
            } => {
                self.etat.ecoute.definir_volume(&identite, source, pourcent);
                self.appliquer_ecoute();
            }
            Commande::Muet {
                identite,
                source,
                muet,
            } => {
                self.etat.ecoute.definir_muet(&identite, source, muet);
                self.appliquer_ecoute();
            }
            Commande::VolumeGeneral(pourcent) => {
                self.etat.ecoute.definir_general(pourcent);
                self.appliquer_ecoute();
            }
            Commande::Traitement(reglages) => {
                self.etat.traitement = reglages.borne();
                self.route.regler(self.etat.traitement);
                self.publier();
            }
            Commande::Affichage(demandes) => {
                self.affichage = demandes;
                self.reabonner().await;
            }
            Commande::Regarder(identite, regarde) => {
                match regarde {
                    Some(r) => self.etat.regardes.insert(identite, r),
                    None => self.etat.regardes.remove(&identite),
                };
                self.reabonner().await;
                self.publier();
            }
            Commande::RegarderAuto(auto) => {
                self.etat.regarder_auto = auto;
                self.reglages.regarder_auto = auto;
                self.reabonner().await;
                self.publier();
            }
            Commande::Fermer(_) => {}
        }
    }

    fn synchroniser_abonnements(&mut self) -> bool {
        let Some(salle) = self.salle.clone() else {
            return false;
        };
        let voulus: BTreeMap<CleVideo, DemandeVideo> = self
            .publications_video
            .keys()
            .filter(|cle| self.etat.recoit(cle))
            .filter_map(|cle| self.affichage.get(cle).map(|d| (cle.clone(), *d)))
            .collect();
        for (cle, brute) in &self.publications_video {
            let demande = voulus.get(cle).copied();
            if self.abonnements_video.get(cle).copied() != demande {
                salle.regler_video(brute, cle.source, demande);
            }
        }
        self.abonnements_video = voulus;
        let sons: BTreeSet<String> = self
            .sons_partage
            .keys()
            .filter(|p| self.etat.ecoute_partage(p))
            .cloned()
            .collect();
        for (proprietaire, brute) in &self.sons_partage {
            let voulu = sons.contains(proprietaire);
            if self.sons_abonnes.contains(proprietaire) != voulu {
                salle.regler_audio(brute, SourceAudio::Partage, voulu);
            }
        }
        self.sons_abonnes = sons;
        let vus: BTreeSet<String> = self
            .abonnements_video
            .keys()
            .filter_map(|cle| self.etat.pistes.get(cle).cloned())
            .collect();
        if vus == self.etat.vus {
            return false;
        }
        self.etat.vus = vus;
        true
    }

    async fn reabonner(&mut self) {
        if self.synchroniser_abonnements() {
            self.publier_attributs().await;
        }
    }

    async fn ouvrir_capture(&mut self) -> Result<CaptureCamera, String> {
        self.generation_camera += 1;
        let generation = self.generation_camera;
        let route = self.route_camera.clone();
        let sortie: SortieImages = Arc::new(move |image| route.envoyer(image));
        let retour = self.retour.clone();
        let perte: SignalCamera = Arc::new(move |erreur| {
            let _ = retour.send(Commande::CameraPerdue(erreur, generation));
        });
        let cameras = self.cameras.clone();
        let id = self.etat.camera.clone();
        let qualite = self.etat.qualite_camera;
        let ouverture = tokio::task::spawn_blocking(move || {
            CaptureCamera::ouvrir(&*cameras, id.as_deref(), qualite, sortie, perte)
        })
        .await;
        match ouverture {
            Ok(Ok(capture)) if generation == self.generation_camera => Ok(capture),
            Ok(Ok(_)) => Err(String::new()),
            Ok(Err(erreur)) => Err(message_camera(&erreur)),
            Err(_) => Err(message_camera(&ErreurCamera::Autre(String::new()))),
        }
    }

    async fn demarrer_camera(&mut self) {
        let Some(salle) = self.salle.clone() else {
            return;
        };
        if self.capture_camera.is_some() {
            return;
        }
        self.etat.camera_demarrage = true;
        self.etat.camera_erreur = None;
        self.publier();
        let local = CleVideo::camera(&salle.identite());
        self.route_camera
            .afficher(Some(self.videos.recepteur(&local)));
        let resultat = match self.ouvrir_capture().await {
            Ok(capture) => {
                match salle
                    .publier_video(SourceVideo::Camera, self.etat.qualite_camera.format())
                    .await
                {
                    Ok(sortie) => {
                        self.route_camera.brancher(Some(sortie));
                        self.capture_camera = Some(capture);
                        Ok(())
                    }
                    Err(_) => Err(MESSAGE_PUBLICATION_CAMERA.to_string()),
                }
            }
            Err(message) => Err(message),
        };
        self.etat.camera_demarrage = false;
        match resultat {
            Ok(()) => self.etat.camera_active = true,
            Err(message) => {
                self.couper_camera();
                self.camera_voulue = false;
                if !message.is_empty() {
                    self.etat.camera_erreur = Some(message);
                }
            }
        }
        self.publier();
    }

    fn couper_camera(&mut self) {
        self.generation_camera += 1;
        self.capture_camera = None;
        self.route_camera.brancher(None);
        self.route_camera.afficher(None);
        self.etat.camera_active = false;
        self.etat.camera_demarrage = false;
    }

    async fn arreter_camera(&mut self) {
        let publiee = self.etat.camera_active;
        self.couper_camera();
        if let (true, Some(salle)) = (publiee, self.salle.clone()) {
            salle.retirer_video(SourceVideo::Camera).await;
        }
        self.publier();
    }

    fn cle_video(identite: &str, source: SourceVideo) -> CleVideo {
        CleVideo {
            identite: proprietaire(identite).to_string(),
            source: if est_partage(identite) {
                SourceVideo::Partage
            } else {
                source
            },
        }
    }

    fn retirer_video(&mut self, cle: &CleVideo) {
        self.publications_video.remove(cle);
        self.abonnements_video.remove(cle);
        self.etat.pistes.remove(cle);
        self.etat.videos.remove(cle);
        if let Some(tache) = self.pistes_video.remove(cle) {
            tache.abort();
        }
        self.videos.effacer(cle);
    }

    async fn rejoindre(&mut self, salon: String, echange: bool) {
        if self.etat.dans(&salon) {
            return;
        }
        if echange && self.salle.is_some() {
            self.couper_camera();
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
        if self.camera_voulue {
            self.demarrer_camera().await;
        }
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
                let parties: Vec<CleVideo> = self
                    .publications_video
                    .iter()
                    .filter(|(_, brute)| **brute == identite)
                    .map(|(cle, _)| cle.clone())
                    .collect();
                for cle in parties {
                    self.retirer_video(&cle);
                }
                self.sons_partage.retain(|_, brute| *brute != identite);
                let sons: BTreeSet<String> = self.sons_partage.keys().cloned().collect();
                self.sons_abonnes.retain(|p| sons.contains(p));
                self.etat.sons_partage = sons;
                self.reabonner().await;
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
                self.couper_camera();
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
            EvenementSalle::AudioPubliee { identite, source } => {
                if est_partage(&identite) || source == SourceAudio::Partage {
                    let cle = proprietaire(&identite).to_string();
                    self.etat.sons_partage.insert(cle.clone());
                    if self.sons_partage.insert(cle.clone(), identite).is_some() {
                        self.sons_abonnes.remove(&cle);
                    }
                    self.reabonner().await;
                }
            }
            EvenementSalle::AudioRetiree { identite, source } => {
                if est_partage(&identite) || source == SourceAudio::Partage {
                    let cle = proprietaire(&identite).to_string();
                    self.etat.sons_partage.remove(&cle);
                    self.sons_partage.remove(&cle);
                    self.sons_abonnes.remove(&cle);
                }
            }
            EvenementSalle::VideoPubliee {
                identite,
                source,
                piste,
            } => {
                let cle = Self::cle_video(&identite, source);
                self.abonnements_video.remove(&cle);
                self.etat.videos.insert(cle.clone());
                self.etat.pistes.insert(cle.clone(), piste);
                self.publications_video.insert(cle, identite);
                self.reabonner().await;
            }
            EvenementSalle::VideoRetiree { identite, source } => {
                let cle = Self::cle_video(&identite, source);
                self.retirer_video(&cle);
                if cle.source == SourceVideo::Partage {
                    self.etat.regardes.remove(&cle.identite);
                }
                self.reabonner().await;
            }
            EvenementSalle::PisteVideo {
                identite,
                source,
                mut images,
            } => {
                let cle = Self::cle_video(&identite, source);
                let recepteur = self.videos.recepteur(&cle);
                let tache = tokio::spawn(async move {
                    while let Some(image) = images.next().await {
                        recepteur.pousser(image);
                    }
                });
                if let Some(precedente) = self.pistes_video.insert(cle, tache) {
                    precedente.abort();
                }
            }
            EvenementSalle::PisteVideoRetiree { identite, source } => {
                let cle = Self::cle_video(&identite, source);
                if let Some(tache) = self.pistes_video.remove(&cle) {
                    tache.abort();
                }
                self.videos.effacer(&cle);
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
        for (_, tache) in std::mem::take(&mut self.pistes_video) {
            tache.abort();
        }
        for cle in std::mem::take(&mut self.publications_video).into_keys() {
            self.etat.videos.remove(&cle);
            self.videos.effacer(&cle);
        }
        self.abonnements_video.clear();
        self.sons_partage.clear();
        self.sons_abonnes.clear();
        self.etat.pistes.clear();
        self.etat.sons_partage.clear();
        self.etat.vus.clear();
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
        self.mesure.reinitialiser();
        self.couper_camera();
        self.camera_voulue = false;
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
