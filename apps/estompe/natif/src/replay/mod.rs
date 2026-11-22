mod session;

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use estompe_client::jeux::JeuDetecte;
use estompe_client::replay::automate::{Automate, Etat, Jeu};
use estompe_client::replay::ecriture::{FileEcriture, Tache};
use estompe_client::replay::galerie::{self, Horodatage};
use estompe_client::replay::garde::Garde;
use estompe_client::replay::reglages::{self, Materiel, Options};
use estompe_client::replay::tampon::{Tampon, UNITE_TEMPS};
use estompe_client::replay::types::{
    Disponibilite, ErreurReplay, EtatReplay, EvenementReplay, ReglagesReplay, DUREE_MAX_S,
};
use estompe_plateforme::{
    CaptureEcran, CaptureSon, EncodageMateriel, EtatTouche, MaterielEncodage, Micro, PremierPlan,
    RaccourcisGlobaux,
};
use tokio::sync::{broadcast, watch};

use crate::jeux::EtatJeux;

pub use session::IMAGE_CHANGEE;

pub const HORS_WINDOWS: &str = "instant replay disponible sous Windows uniquement";
pub const ENCODEUR_MUET: &str = "encodeur-muet";
pub const PERIODE_VERIFICATION: Duration = Duration::from_secs(5);
const SOURCE_PAR_DEFAUT: (u32, u32) = (1920, 1080);
const CAPACITE_EVENEMENTS: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct ReglagesMoteur {
    pub replay: ReglagesReplay,
    pub debruiter: bool,
    pub gain_micro: f32,
}

impl Default for ReglagesMoteur {
    fn default() -> Self {
        Self {
            replay: ReglagesReplay::default(),
            debruiter: true,
            gain_micro: 1.0,
        }
    }
}

pub type TouchesPrises = Arc<dyn Fn() -> Vec<String> + Send + Sync>;
pub type Horloge = Arc<dyn Fn() -> Horodatage + Send + Sync>;

#[derive(Clone)]
pub struct Dependances {
    pub encodage: Arc<dyn EncodageMateriel>,
    pub ecrans: Arc<dyn CaptureEcran>,
    pub sons: Arc<dyn CaptureSon>,
    pub micro: Option<Arc<dyn Micro + Send + Sync>>,
    pub premier_plan: Arc<dyn PremierPlan>,
    pub raccourcis: Arc<dyn RaccourcisGlobaux>,
    pub touches_prises: TouchesPrises,
    pub videos: Option<PathBuf>,
    pub journaux: Option<PathBuf>,
    pub version: String,
    pub horloge: Option<Horloge>,
}

pub fn heure_locale() -> Horodatage {
    use chrono::{Datelike, Timelike};
    let t = chrono::Local::now();
    Horodatage {
        annee: i64::from(t.year()),
        mois: t.month(),
        jour: t.day(),
        heure: t.hour(),
        minute: t.minute(),
        seconde: t.second(),
    }
}

pub(crate) enum Message {
    Configurer(Box<ReglagesMoteur>),
    Jeux(Option<JeuDetecte>),
    Sauvegarder(Option<u32>),
    Partage(bool, bool),
    JeuFerme(u32),
    Echec(u64, &'static str, String),
    Alerte(ErreurReplay),
    Options(Box<ReglagesReplay>, Sender<Result<Options, String>>),
    Disponibilite(Sender<Disponibilite>),
    Arreter,
}

pub struct ServiceReplay {
    tx: Sender<Message>,
    etat: Arc<watch::Sender<EtatReplay>>,
    evenements: broadcast::Sender<EvenementReplay>,
    reglages: Mutex<ReglagesMoteur>,
    videos: Option<PathBuf>,
    fil: Mutex<Option<JoinHandle<()>>>,
}

impl ServiceReplay {
    pub fn demarrer(dependances: Dependances, reglages: ReglagesMoteur) -> Arc<Self> {
        let reglages = ReglagesMoteur {
            replay: reglages.replay.borne(),
            ..reglages
        };
        let (tx, rx) = mpsc::channel();
        let etat = Arc::new(watch::channel(EtatReplay::default()).0);
        let (evenements, _) = broadcast::channel(CAPACITE_EVENEMENTS);
        let videos = dependances.videos.clone();
        let controleur =
            Controleur::nouveau(dependances, tx.clone(), etat.clone(), evenements.clone());
        let fil = std::thread::Builder::new()
            .name("replay".into())
            .spawn(move || controleur.tourner(rx))
            .ok();
        let _ = tx.send(Message::Configurer(Box::new(reglages.clone())));
        Arc::new(Self {
            tx,
            etat,
            evenements,
            reglages: Mutex::new(reglages),
            videos,
            fil: Mutex::new(fil),
        })
    }

    fn envoyer(&self, message: Message) {
        let _ = self.tx.send(message);
    }

    fn reglages_courants(&self) -> MutexGuard<'_, ReglagesMoteur> {
        self.reglages.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn reglages(&self) -> ReglagesMoteur {
        self.reglages_courants().clone()
    }

    pub fn configurer(&self, reglages: ReglagesMoteur) {
        let reglages = ReglagesMoteur {
            replay: reglages.replay.borne(),
            ..reglages
        };
        *self.reglages_courants() = reglages.clone();
        self.envoyer(Message::Configurer(Box::new(reglages)));
    }

    pub fn sauvegarder(&self) {
        self.envoyer(Message::Sauvegarder(None));
    }

    pub fn sauvegarder_les_dernieres(&self, secondes: u32) {
        self.envoyer(Message::Sauvegarder(Some(secondes)));
    }

    pub fn partage(&self, actif: bool, suspendre: bool) {
        self.envoyer(Message::Partage(actif, suspendre));
    }

    pub fn definir_jeu(&self, jeu: Option<JeuDetecte>) {
        self.envoyer(Message::Jeux(jeu));
    }

    pub fn dossier(&self) -> Result<PathBuf, String> {
        galerie::dossier_effectif(
            self.reglages_courants().replay.dossier.as_deref(),
            self.videos.clone(),
        )
    }

    pub fn etat(&self) -> watch::Receiver<EtatReplay> {
        self.etat.subscribe()
    }

    pub fn actuel(&self) -> EtatReplay {
        self.etat.borrow().clone()
    }

    pub fn evenements(&self) -> broadcast::Receiver<EvenementReplay> {
        self.evenements.subscribe()
    }

    pub fn disponibilite(&self) -> Disponibilite {
        let (tx, rx) = mpsc::channel();
        self.envoyer(Message::Disponibilite(tx));
        rx.recv().unwrap_or_else(|_| Disponibilite {
            disponible: false,
            raison: Some("moteur de l'instant replay arrêté".into()),
            encodeur: None,
        })
    }

    pub fn options(&self, reglages: &ReglagesReplay) -> Result<Options, String> {
        let (tx, rx) = mpsc::channel();
        self.envoyer(Message::Options(Box::new(reglages.clone().borne()), tx));
        rx.recv()
            .map_err(|_| "moteur de l'instant replay arrêté".to_string())?
    }

    pub fn suivre_les_jeux(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        mut jeux: watch::Receiver<EtatJeux>,
    ) {
        self.definir_jeu(jeux.borrow_and_update().premier_plan.clone());
        let faible = Arc::downgrade(self);
        runtime.spawn(async move {
            while jeux.changed().await.is_ok() {
                let jeu = jeux.borrow_and_update().premier_plan.clone();
                let Some(service) = faible.upgrade() else {
                    break;
                };
                service.definir_jeu(jeu);
            }
        });
    }
}

impl Drop for ServiceReplay {
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Arreter);
        if let Some(fil) = self.fil.lock().ok().and_then(|mut f| f.take()) {
            if fil.thread().id() != std::thread::current().id() {
                let _ = fil.join();
            }
        }
    }
}

struct Controleur {
    d: Dependances,
    tx: Sender<Message>,
    etat: Arc<watch::Sender<EtatReplay>>,
    evenements: broadcast::Sender<EvenementReplay>,
    reglages: ReglagesMoteur,
    automate: Automate,
    tampon: Arc<Mutex<Tampon>>,
    materiel: Option<Result<MaterielEncodage, String>>,
    session: Option<session::Session>,
    numero: u64,
    file: FileEcriture,
    dernier_jeu: String,
    garde: Arc<Garde>,
    touche: Option<String>,
    source: Option<(u32, u32)>,
    horloge: Horloge,
}

impl Controleur {
    fn nouveau(
        d: Dependances,
        tx: Sender<Message>,
        etat: Arc<watch::Sender<EtatReplay>>,
        evenements: broadcast::Sender<EvenementReplay>,
    ) -> Self {
        let emetteur = evenements.clone();
        let file = FileEcriture::demarrer(
            || {},
            move |resultat| match resultat {
                Ok(ecrit) => {
                    let quota = ecrit.quota_atteint;
                    let _ = emetteur.send(EvenementReplay::Clip(ecrit.clip));
                    if quota {
                        let _ = emetteur.send(EvenementReplay::Erreur(ErreurReplay::nouvelle(
                            "quota",
                            "Quota du dossier des clips atteint : supprime d'anciens clips pour continuer à enregistrer.",
                        )));
                    }
                }
                Err(e) => {
                    let _ = emetteur.send(EvenementReplay::Erreur(e));
                }
            },
        );
        let garde = Arc::new(Garde::charger(d.journaux.clone(), &d.version));
        let horloge = d.horloge.clone().unwrap_or_else(|| Arc::new(heure_locale));
        Self {
            d,
            tx,
            etat,
            evenements,
            reglages: ReglagesMoteur::default(),
            automate: Automate::default(),
            tampon: Arc::default(),
            materiel: None,
            session: None,
            numero: 0,
            file,
            dernier_jeu: String::new(),
            garde,
            touche: None,
            source: None,
            horloge,
        }
    }

    fn tourner(mut self, rx: mpsc::Receiver<Message>) {
        loop {
            match rx.recv_timeout(PERIODE_VERIFICATION) {
                Ok(Message::Arreter) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(message) => self.traiter(message),
                Err(RecvTimeoutError::Timeout) => self.verifier_le_jeu(),
            }
            self.accorder();
            if self.session.is_none() {
                self.garde.relacher();
            }
            self.accorder_la_touche();
            self.publier();
        }
        self.session = None;
        self.garde.relacher();
        if let Some(touche) = self.touche.take() {
            let _ = self.d.raccourcis.retirer(&touche);
        }
    }

    fn alerter(&self, erreur: ErreurReplay) {
        let _ = self.evenements.send(EvenementReplay::Erreur(erreur));
    }

    fn vider(&mut self) {
        self.session = None;
        if let Ok(mut t) = self.tampon.lock() {
            t.vider();
        }
    }

    fn materiel(&mut self) -> Result<MaterielEncodage, String> {
        if self.materiel.is_none() {
            let lu = if self.d.encodage.disponible() {
                self.d
                    .encodage
                    .materiel(None)
                    .map_err(|e| e.to_string())
                    .and_then(|m| {
                        if m.h264.is_some() || m.hevc.is_some() {
                            Ok(m)
                        } else {
                            Err("aucun encodeur vidéo matériel : l'instant replay est indisponible sur cette machine".into())
                        }
                    })
            } else {
                Err(HORS_WINDOWS.into())
            };
            let lu = match lu {
                Ok(_) if !self.garde.permis() => Err(
                    "l'encodeur matériel a arrêté l'app pendant son essai : instant replay désactivé jusqu'à la prochaine version".into(),
                ),
                autre => autre,
            };
            self.automate.materiel(lu.as_ref().err().cloned());
            self.materiel = Some(lu);
        }
        self.materiel
            .clone()
            .unwrap_or_else(|| Err("matériel non lu".into()))
    }

    fn disponibilite(&mut self) -> Disponibilite {
        match self.materiel() {
            Ok(m) => Disponibilite {
                disponible: true,
                raison: None,
                encodeur: m.h264.as_ref().or(m.hevc.as_ref()).map(|e| e.nom.clone()),
            },
            Err(raison) => Disponibilite {
                disponible: false,
                raison: Some(raison),
                encodeur: None,
            },
        }
    }

    fn verifier_le_jeu(&mut self) {
        let Some(jeu) = self.automate.jeu().cloned() else {
            return;
        };
        if self.automate.etat() == Etat::Arme {
            return;
        }
        let Ok(fenetres) = self.d.premier_plan.fenetres() else {
            return;
        };
        if !fenetres.iter().any(|a| a.processus == jeu.pid) && self.automate.jeu_ferme(jeu.pid) {
            self.vider();
        }
    }

    fn traiter(&mut self, message: Message) {
        match message {
            Message::Configurer(reglages) => {
                let change = !reglages.replay.meme_capture(&self.reglages.replay)
                    || reglages.debruiter != self.reglages.debruiter
                    || reglages.gain_micro != self.reglages.gain_micro;
                if self.automate.configurer(reglages.replay.actif, change) {
                    self.vider();
                }
                self.reglages = *reglages;
                if self.reglages.replay.actif {
                    let _ = self.materiel();
                }
            }
            Message::Jeux(detecte) => {
                let jeu = detecte.map(|j| Jeu {
                    fenetre: j.application.fenetre,
                    pid: j.application.processus,
                    nom: j.nom,
                });
                let absent = jeu.is_none();
                if self.automate.premier_plan(jeu) {
                    self.vider();
                }
                if absent {
                    self.verifier_le_jeu();
                }
            }
            Message::JeuFerme(pid) => {
                if self.automate.jeu_ferme(pid) {
                    self.vider();
                }
            }
            Message::Partage(actif, suspendre) => self.automate.partage(actif, suspendre),
            Message::Echec(numero, code, raison) => {
                if self.session.as_ref().is_some_and(|s| s.numero == numero) {
                    self.session = None;
                    match code {
                        IMAGE_CHANGEE => self.alerter(ErreurReplay::nouvelle(
                            code,
                            "L'image du jeu a changé de taille : le tampon de l'instant replay redémarre.",
                        )),
                        ENCODEUR_MUET => self.echouer(
                            code,
                            "l'encodeur matériel n'encode aucune image".into(),
                        ),
                        _ => self.echouer(code, raison),
                    }
                }
            }
            Message::Alerte(erreur) => self.alerter(erreur),
            Message::Sauvegarder(secondes) => self.sauvegarder(secondes),
            Message::Options(voulus, reponse) => {
                let source = self.source.unwrap_or(SOURCE_PAR_DEFAUT);
                let options = self
                    .materiel()
                    .map(|m| reglages::options(&voulus, &Materiel::de(&m, source)));
                let _ = reponse.send(options);
            }
            Message::Disponibilite(reponse) => {
                let d = self.disponibilite();
                let _ = reponse.send(d);
            }
            Message::Arreter => {}
        }
    }

    fn echouer(&mut self, code: &'static str, raison: String) {
        if self.automate.echec(raison.clone()) {
            self.alerter(ErreurReplay::nouvelle(
                code,
                format!("Instant replay arrêté : {raison}."),
            ));
        }
    }

    fn sauvegarder(&mut self, secondes: Option<u32>) {
        let duree = secondes
            .unwrap_or(self.reglages.replay.duree_s)
            .clamp(1, DUREE_MAX_S);
        let instantane = self
            .tampon
            .lock()
            .ok()
            .and_then(|t| t.instantane(i64::from(duree) * UNITE_TEMPS));
        let Some(instantane) = instantane.filter(|i| !i.video.is_empty()) else {
            return self.alerter(ErreurReplay::nouvelle(
                "tampon-vide",
                "Rien à enregistrer : le tampon de l'instant replay est vide.",
            ));
        };
        let dossier = match galerie::dossier_effectif(
            self.reglages.replay.dossier.as_deref(),
            self.d.videos.clone(),
        ) {
            Ok(d) => d,
            Err(e) => {
                return self.alerter(ErreurReplay::nouvelle(
                    "dossier-inaccessible",
                    format!("Clip non enregistré : {e}."),
                ))
            }
        };
        let jeu = self
            .automate
            .jeu()
            .map_or_else(|| self.dernier_jeu.clone(), |j| j.nom.clone());
        let tache = Tache {
            instantane,
            jeu,
            dossier,
            quota: self.reglages.replay.quota_octets,
            date: (self.horloge)(),
        };
        if let Err(e) = self.file.demander(tache) {
            self.alerter(e);
        }
    }

    fn accorder(&mut self) {
        if self.automate.etat() != Etat::Arme {
            self.session = None;
        } else if self.session.is_none() {
            if let Err((code, raison)) = self.ouvrir() {
                self.echouer(code, raison);
            }
        }
    }

    fn ouvrir(&mut self) -> Result<(), (&'static str, String)> {
        let jeu = self
            .automate
            .jeu()
            .cloned()
            .ok_or(("capture-refusee", "aucun jeu au premier plan".to_string()))?;
        let materiel = self.materiel().map_err(|e| ("sans-encodeur", e))?;
        self.numero += 1;
        let ouverte = session::Session::ouvrir(session::Ouverture {
            numero: self.numero,
            jeu: &jeu,
            reglages: &self.reglages,
            materiel: &materiel,
            d: &self.d,
            tampon: self.tampon.clone(),
            tx: self.tx.clone(),
            garde: self.garde.clone(),
        })?;
        self.source = Some(ouverte.source);
        self.dernier_jeu = jeu.nom;
        self.session = Some(ouverte);
        Ok(())
    }

    fn accorder_la_touche(&mut self) {
        let disponible = self.materiel.as_ref().is_some_and(|m| m.is_ok());
        let touche = &self.reglages.replay.touche;
        let voulue = (self.reglages.replay.actif
            && disponible
            && self.d.raccourcis.disponible()
            && !(self.d.touches_prises)().contains(touche))
        .then(|| touche.clone());
        if voulue == self.touche {
            return;
        }
        if let Some(ancienne) = self.touche.take() {
            let _ = self.d.raccourcis.retirer(&ancienne);
        }
        if let Some(touche) = voulue {
            let tx = Mutex::new(self.tx.clone());
            let action = Box::new(move |etat| {
                if etat == EtatTouche::Appui {
                    if let Ok(tx) = tx.lock() {
                        let _ = tx.send(Message::Sauvegarder(None));
                    }
                }
            });
            if self.d.raccourcis.enregistrer(&touche, action).is_ok() {
                self.touche = Some(touche);
            }
        }
    }

    fn publier(&mut self) {
        let public = EtatReplay {
            etat: self.automate.etat(),
            jeu: self.automate.jeu().map(|j| j.nom.clone()),
            format: self.session.as_ref().map(|s| s.format),
            encodeur: self.session.as_ref().map(|s| s.encodeur.clone()),
            raccourci: self.touche.clone(),
        };
        self.etat.send_if_modified(|e| {
            let change = *e != public;
            *e = public;
            change
        });
    }
}

#[cfg(test)]
mod tests;
