mod capture;
pub mod dialogue;
mod encodeur;
mod gpu;
mod materiel;
mod son;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    CreateEventW, CreateWaitableTimerExW, GetCurrentThread, GetCurrentThreadId, OpenProcess, SetEvent, SetThreadPriority, SetWaitableTimer, WaitForMultipleObjects, WaitForSingleObject,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, PROCESS_SYNCHRONIZE, THREAD_MODE_BACKGROUND_BEGIN, TIMER_ALL_ACCESS,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetMessageW, PostThreadMessageW, EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, MSG, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
    WM_QUIT,
};

use self::capture::Capture;
use self::encodeur::{Encodeur, Envoi, Parametres};
use self::gpu::{Appareil, Convertisseur};
use self::materiel::Gpu;
use super::automate::{Automate, Etat, Jeu};
use super::cadence::{Cadence, Perte, Pertes};
use super::ecriture::{FileEcriture, Tache};
use super::galerie::Horodatage;
use super::reglages::{self, Materiel, AUDIO_JEU_KBPS, AUDIO_MICRO_KBPS};
use super::tampon::{Format, Tampon, UNITE_TEMPS};
use super::types::{Disponibilite, Erreur, EtatPublic, Reglages};
use crate::debuglog;

const SOURCE: &str = "replay";
const IMAGE_CHANGEE: &str = "image-changee";
const ATTENTE_ENCODEUR: Duration = Duration::from_secs(2);

pub enum Message {
    Configurer(Box<Reglages>, PathBuf),
    Sauvegarder,
    Partage(bool, bool),
    PremierPlan,
    JeuFerme(u32),
    Echec(u64, &'static str, String),
    Materiel(Sender<Result<Materiel, String>>),
    Disponibilite(Sender<Disponibilite>),
}

pub struct Moteur {
    tx: Sender<Message>,
    etat: Arc<Mutex<EtatPublic>>,
}

impl Moteur {
    pub fn demarrer(app: AppHandle) -> Self {
        let (tx, rx) = channel();
        let etat = Arc::new(Mutex::new(EtatPublic::simple("inactif", None)));
        let (canal, partage) = (tx.clone(), etat.clone());
        std::thread::spawn(move || Controleur::nouveau(app, canal, partage).tourner(rx));
        Self { tx, etat }
    }

    pub fn envoyer(&self, message: Message) -> Result<(), String> {
        self.tx.send(message).map_err(|_| "moteur de l'instant replay arrêté".to_string())
    }

    pub fn etat(&self) -> EtatPublic {
        self.etat.lock().map(|e| e.clone()).unwrap_or_else(|_| EtatPublic::simple("inactif", None))
    }

    pub fn materiel(&self) -> Result<Materiel, String> {
        let (tx, rx) = channel();
        self.envoyer(Message::Materiel(tx))?;
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn disponibilite(&self) -> Disponibilite {
        let (tx, rx) = channel();
        let reponse = self.envoyer(Message::Disponibilite(tx)).and_then(|_| rx.recv().map_err(|e| e.to_string()));
        reponse.unwrap_or_else(|e| Disponibilite { disponible: false, raison: Some(e), encodeur: None })
    }
}

static CANAL: Mutex<Option<Sender<Message>>> = Mutex::new(None);

unsafe extern "system" fn premier_plan(_: HWINEVENTHOOK, evenement: u32, fenetre: HWND, objet: i32, _: i32, _: u32, _: u32) {
    let suivi = match evenement {
        EVENT_OBJECT_LOCATIONCHANGE => objet == OBJID_WINDOW.0 && fenetre == GetForegroundWindow(),
        EVENT_SYSTEM_FOREGROUND | EVENT_SYSTEM_MINIMIZESTART | EVENT_SYSTEM_MINIMIZEEND => true,
        _ => false,
    };
    if !suivi {
        return;
    }
    if let Ok(canal) = CANAL.lock() {
        if let Some(tx) = canal.as_ref() {
            let _ = tx.send(Message::PremierPlan);
        }
    }
}

struct Ecoute {
    fil: u32,
}

impl Ecoute {
    fn demarrer(tx: Sender<Message>) -> Option<Self> {
        *CANAL.lock().ok()? = Some(tx);
        let (pret, attente) = channel();
        std::thread::spawn(move || unsafe {
            let options = WINEVENT_OUTOFCONTEXT;
            let crochets = [
                SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, None, Some(premier_plan), 0, 0, options),
                SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE, None, Some(premier_plan), 0, 0, options),
            ];
            let _ = pret.send(GetCurrentThreadId());
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {}
            for crochet in crochets {
                let _ = UnhookWinEvent(crochet);
            }
        });
        attente.recv().ok().map(|fil| Self { fil })
    }
}

impl Drop for Ecoute {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.fil, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Ok(mut canal) = CANAL.lock() {
            *canal = None;
        }
    }
}

fn surveiller(pid: u32, tx: Sender<Message>) {
    std::thread::spawn(move || unsafe {
        if let Ok(processus) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(processus, INFINITE);
            let _ = CloseHandle(processus);
            let _ = tx.send(Message::JeuFerme(pid));
        }
    });
}

struct Session {
    numero: u64,
    arret: Arc<AtomicBool>,
    signal: Envoi<HANDLE>,
    fils: Vec<JoinHandle<()>>,
    encodeur_termine: Receiver<()>,
    format: Format,
    micro_par_defaut: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        unsafe {
            let _ = SetEvent(self.signal.0);
        }
        for fil in self.fils.drain(..) {
            let _ = fil.join();
        }
        let _ = self.encodeur_termine.recv_timeout(ATTENTE_ENCODEUR);
        unsafe {
            let _ = CloseHandle(self.signal.0);
        }
    }
}

struct Rythme {
    capture: Capture,
    appareil: Appareil,
    convertisseur: Convertisseur,
    encodeur: Encodeur,
    ips: u32,
    horloge: Instant,
    signal: HANDLE,
    tampon: Arc<Mutex<Tampon>>,
    echec: Box<dyn Fn(&'static str, String)>,
    alerte: Box<dyn Fn(Erreur)>,
}

impl Rythme {
    unsafe fn tourner(self) {
        let precis = CreateWaitableTimerExW(None, None, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0);
        let Ok(minuteur) = precis.or_else(|_| CreateWaitableTimerExW(None, None, 0, TIMER_ALL_ACCESS.0)) else {
            return (self.echec)("minuteur", "minuteur haute résolution indisponible".into());
        };
        let mut cadence = Cadence::nouvelle(self.ips);
        let fenetre = 10 * self.ips as u64;
        let (mut sans_image, mut releve, mut refus) = (0u64, (0u64, Pertes::default()), None);
        loop {
            let attente = cadence.echeance() - (self.horloge.elapsed().as_nanos() / 100) as i64;
            if attente > 0 {
                let relatif = -attente;
                if SetWaitableTimer(minuteur, &relatif, 0, None, None, false).is_err() {
                    (self.echec)("minuteur", "minuteur haute résolution indisponible".into());
                    break;
                }
            }
            let index = WaitForMultipleObjects(&[self.signal, minuteur], false, if attente > 0 { INFINITE } else { 0 });
            if index.0 == 0 {
                break;
            }
            for n in cadence.dues((self.horloge.elapsed().as_nanos() / 100) as i64) {
                let Some(image) = self.capture.derniere() else {
                    sans_image += 1;
                    continue;
                };
                sans_image = 0;
                let envoyee = self.convertisseur.convertir(&self.appareil, &image.texture).and_then(|e| {
                    e.SetSampleTime(cadence.horodatage(n))?;
                    e.SetSampleDuration(cadence.duree(n))?;
                    Ok(e)
                });
                match envoyee.map(|e| self.encodeur.envoyer(e)) {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => cadence.perdre(Perte::Encodeur),
                    Ok(Err(())) => {
                        let _ = CloseHandle(minuteur);
                        return;
                    }
                    Err(e) => {
                        cadence.perdre(Perte::Conversion);
                        refus = Some(materiel::texte(e));
                    }
                }
            }
            if sans_image > 5 * self.ips as u64 {
                (self.echec)("capture-refusee", "le jeu ne fournit aucune image à la capture (plein écran exclusif ou protection)".into());
                break;
            }
            let (emises, pertes) = cadence.bilan();
            if emises - releve.0 >= fenetre {
                let (images, recentes) = (emises - releve.0, pertes.depuis(&releve.1));
                let (octets, duree, debordements) = self.tampon.lock().map_or((0, 0, 0), |t| (t.octets(), t.duree(), t.debordements()));
                let refus: Option<String> = refus.take();
                let conversion = refus.as_ref().map_or(String::new(), |e| format!(" : {e}"));
                debuglog::log("info", SOURCE, &format!(
                    "images : {images} émises, {} perdues sur 10 s (retard {}, encodeur {}, conversion {}{conversion}) ; {emises} émises, {} perdues depuis le démarrage (retard {}, encodeur {}, conversion {}) ; tampon : {} Mo, {} s, {debordements} débordement(s)",
                    recentes.total(), recentes.retard, recentes.encodeur, recentes.conversion,
                    pertes.total(), pertes.retard, pertes.encodeur, pertes.conversion,
                    octets / (1024 * 1024), duree / UNITE_TEMPS
                ));
                if let Some(cause) = recentes.alerte(images) {
                    let (code, message) = cause.erreur();
                    let detail = refus.filter(|_| cause == Perte::Conversion).map_or(String::new(), |e| format!(" ({e})"));
                    (self.alerte)(Erreur::nouvelle(code, format!("{message}{detail}")));
                }
                releve = (emises, pertes);
            }
        }
        let (emises, pertes) = cadence.bilan();
        debuglog::log("info", SOURCE, &format!(
            "arrêt de la capture : {emises} images émises, {} perdues (retard {}, encodeur {}, conversion {})",
            pertes.total(), pertes.retard, pertes.encodeur, pertes.conversion
        ));
        let _ = CloseHandle(minuteur);
    }
}

struct Controleur {
    app: AppHandle,
    tx: Sender<Message>,
    etat: Arc<Mutex<EtatPublic>>,
    reglages: Reglages,
    dossier: PathBuf,
    automate: Automate,
    tampon: Arc<Mutex<Tampon>>,
    gpu: Option<Result<Gpu, String>>,
    session: Option<Session>,
    numero: u64,
    ecoute: Option<Ecoute>,
    surveille: Option<u32>,
    file: FileEcriture,
    dernier_jeu: String,
}

impl Controleur {
    fn nouveau(app: AppHandle, tx: Sender<Message>, etat: Arc<Mutex<EtatPublic>>) -> Self {
        let emetteur = app.clone();
        let file = FileEcriture::demarrer(
            || unsafe {
                let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
            },
            move |resultat| match resultat {
                Ok(ecrit) => {
                    if let Err(e) = crate::replay::creer_vignette(&emetteur, &ecrit.dossier, &ecrit.chemin) {
                        debuglog::log("warn", SOURCE, &format!("vignette non créée : {e}"));
                    }
                    debuglog::log("info", SOURCE, &format!("clip enregistré : {} ({} octets, {:.1} s)", ecrit.clip.nom, ecrit.clip.taille_octets, ecrit.clip.duree_s));
                    let _ = emetteur.emit("replay://clip", &ecrit.clip);
                    if ecrit.quota_atteint {
                        let _ = emetteur.emit("replay://erreur", Erreur::nouvelle("quota", "Quota du dossier des clips atteint : supprime d'anciens clips pour continuer à enregistrer."));
                    }
                }
                Err(e) => {
                    debuglog::log("error", SOURCE, &format!("clip non enregistré ({}) : {}", e.code, e.message));
                    let _ = emetteur.emit("replay://erreur", &e);
                }
            },
        );
        Self {
            app,
            tx,
            etat,
            reglages: Reglages::default(),
            dossier: PathBuf::new(),
            automate: Automate::default(),
            tampon: Arc::default(),
            gpu: None,
            session: None,
            numero: 0,
            ecoute: None,
            surveille: None,
            file,
            dernier_jeu: String::new(),
        }
    }

    fn gpu(&mut self) -> &Result<Gpu, String> {
        if self.gpu.is_none() {
            let gpu = materiel::gpu();
            match &gpu {
                Ok(g) => {
                    let nom = |e: &Option<reglages::Encodeur>| e.as_ref().map_or("aucun".to_string(), |e| format!("{} ({} macroblocs/s)", e.nom, e.mb_par_seconde));
                    debuglog::log("info", SOURCE, &format!("carte graphique {} ; H.264 : {} ; HEVC : {}", g.nom, nom(&g.h264), nom(&g.hevc)));
                }
                Err(e) => debuglog::log("warn", SOURCE, e),
            }
            self.automate.materiel(gpu.as_ref().err().cloned());
            self.gpu = Some(gpu);
        }
        self.gpu.get_or_insert_with(|| Err("carte graphique non lue".into()))
    }

    fn tourner(mut self, rx: Receiver<Message>) {
        for message in rx {
            self.traiter(message);
            self.accorder();
            self.publier();
        }
    }

    fn vider(&mut self) {
        self.session = None;
        if let Ok(mut t) = self.tampon.lock() {
            t.vider();
        }
    }

    fn alerter(&self, erreur: Erreur) {
        debuglog::log("warn", SOURCE, &format!("{} : {}", erreur.code, erreur.message));
        let _ = self.app.emit("replay://erreur", &erreur);
    }

    fn observer(&mut self) {
        let cible = crate::jeux::cible(&self.reglages.jeux_ajoutes, &self.reglages.jeux_exclus);
        let jeu = cible.map(|c| Jeu { fenetre: c.fenetre, pid: c.pid, nom: c.nom });
        if let Some(j) = &jeu {
            if self.surveille != Some(j.pid) {
                self.surveille = Some(j.pid);
                surveiller(j.pid, self.tx.clone());
            }
        }
        if self.automate.premier_plan(jeu) {
            self.vider();
        }
    }

    fn traiter(&mut self, message: Message) {
        match message {
            Message::Configurer(reglages, dossier) => {
                let change = !reglages.meme_capture(&self.reglages);
                if self.automate.configurer(reglages.actif, change) {
                    self.vider();
                }
                (self.reglages, self.dossier) = (*reglages, dossier);
                if self.reglages.actif && self.ecoute.is_none() {
                    self.ecoute = Ecoute::demarrer(self.tx.clone());
                } else if !self.reglages.actif {
                    self.ecoute = None;
                }
                if self.reglages.actif {
                    self.gpu();
                    self.observer();
                }
            }
            Message::PremierPlan => self.observer(),
            Message::JeuFerme(pid) => {
                if self.surveille == Some(pid) {
                    self.surveille = None;
                }
                if self.automate.jeu_ferme(pid) {
                    debuglog::log("info", SOURCE, "jeu fermé : tampon vidé");
                    self.vider();
                }
            }
            Message::Partage(actif, suspendre) => self.automate.partage(actif, suspendre),
            Message::Echec(numero, code, raison) => {
                if self.session.as_ref().is_some_and(|s| s.numero == numero) {
                    self.session = None;
                    if code == IMAGE_CHANGEE {
                        self.alerter(Erreur::nouvelle(code, "L'image du jeu a changé de taille : le tampon de l'instant replay redémarre."));
                    } else {
                        self.echouer(code, raison);
                    }
                }
            }
            Message::Sauvegarder => self.sauvegarder(),
            Message::Materiel(reponse) => {
                let fenetre = self.automate.jeu().map(|j| j.fenetre);
                let _ = reponse.send(self.gpu().as_ref().map(|g| materiel::lire(g, fenetre, None)).map_err(|e| e.clone()));
            }
            Message::Disponibilite(reponse) => {
                let d = match self.gpu() {
                    Ok(g) => Disponibilite { disponible: true, raison: None, encodeur: g.h264.as_ref().or(g.hevc.as_ref()).map(|e| e.nom.clone()) },
                    Err(e) => Disponibilite { disponible: false, raison: Some(e.clone()), encodeur: None },
                };
                let _ = reponse.send(d);
            }
        }
    }

    fn echouer(&mut self, code: &'static str, raison: String) {
        if self.automate.echec(raison.clone()) {
            self.alerter(Erreur::nouvelle(code, format!("Instant replay arrêté : {raison}.")));
        } else {
            debuglog::log("warn", SOURCE, &format!("pause pendant le partage d'écran : {raison}"));
        }
    }

    fn sauvegarder(&mut self) {
        let instantane = self.tampon.lock().ok().and_then(|t| t.instantane(self.reglages.duree_s as i64 * UNITE_TEMPS));
        let Some(instantane) = instantane.filter(|i| !i.video.is_empty()) else {
            return self.alerter(Erreur::nouvelle("tampon-vide", "Rien à enregistrer : le tampon de l'instant replay est vide."));
        };
        let jeu = self.automate.jeu().map_or_else(|| self.dernier_jeu.clone(), |j| j.nom.clone());
        let tache = Tache { instantane, jeu, dossier: self.dossier.clone(), quota: self.reglages.quota_octets, date: Horodatage::maintenant() };
        if let Err(e) = self.file.demander(tache) {
            self.alerter(e);
        }
    }

    fn accorder(&mut self) {
        let arme = self.automate.etat() == Etat::Arme;
        if !arme {
            self.session = None;
        } else if self.session.is_none() {
            if let Err((code, raison)) = self.ouvrir() {
                self.echouer(code, raison);
            }
        }
    }

    fn ouvrir(&mut self) -> Result<(), (&'static str, String)> {
        let jeu = self.automate.jeu().cloned().ok_or(("capture-refusee", "aucun jeu au premier plan".to_string()))?;
        let gpu = self.gpu.as_ref().and_then(|g| g.as_ref().ok()).ok_or(("sans-encodeur", "aucun encodeur matériel".to_string()))?;
        let r = self.reglages.clone();
        let element = capture::element(jeu.fenetre).map_err(|e| ("capture-refusee", e))?;
        let taille = element.Size().map_err(|e| ("capture-refusee", materiel::texte(e)))?;
        let source = (taille.Width.max(2) as u32, taille.Height.max(2) as u32);
        let m = materiel::lire(gpu, Some(jeu.fenetre), Some(source));
        let (dimensions, ips, codec) = reglages::effectifs(&r, &m).map_err(|e| ("sans-encodeur", e))?;
        let debit = reglages::debit_kbps(dimensions, ips, r.qualite);
        let total = debit + r.audio_kbps();
        let duree = r.duree_s.min(reglages::duree_max_s(total, m.ram_libre)).max(reglages::DUREE_MIN_S);
        let format = Format { codec, largeur: dimensions.0, hauteur: dimensions.1, ips, micro: r.micro };
        let base = {
            let mut t = self.tampon.lock().map_err(|e| ("tampon", e.to_string()))?;
            if t.preparer(format, duree as i64 * UNITE_TEMPS, reglages::memoire_octets(total, duree)) {
                debuglog::log("info", SOURCE, "format changé : tampon redémarré");
            }
            t.reprise()
        };
        self.numero += 1;
        let numero = self.numero;
        let arret = Arc::new(AtomicBool::new(false));
        let signaler = |tx: &Sender<Message>| {
            let tx = tx.clone();
            move |code: &'static str, raison: String| {
                let _ = tx.send(Message::Echec(numero, code, raison));
            }
        };
        let appareil = Appareil::creer(&gpu.adaptateur).map_err(|e| ("gpu", e))?;
        let blanc = materiel::blanc_hdr(jeu.fenetre);
        let change = signaler(&self.tx);
        let capture = Capture::demarrer(&appareil, &element, r.curseur, blanc.is_some(), move || change(IMAGE_CHANGEE, String::new()));
        let capture = capture.map_err(|e| ("capture-refusee", e))?;
        let convertisseur = Convertisseur::creer(&appareil, capture.taille, dimensions, ips, blanc).map_err(|e| ("gpu", e))?;
        let parametres = Parametres { codec, largeur: dimensions.0, hauteur: dimensions.1, ips, debit_kbps: debit, base };
        let arret_encodeur = signaler(&self.tx);
        let encodeur = encodeur::demarrer(&appareil, &gpu.adaptateur, parametres, self.tampon.clone(), arret.clone(), move |e| arret_encodeur("gpu-sature", e));
        let (encodeur, encodeur_termine) = encodeur.map_err(|e| ("gpu-sature", e))?;
        let nom_encodeur = encodeur.nom.clone();
        let signal = unsafe { CreateEventW(None, true, false, None) }.map_err(|e| ("minuteur", materiel::texte(e)))?;
        let horloge = Instant::now();
        let contexte = || son::Contexte { horloge, base, tampon: self.tampon.clone(), arret: arret.clone() };
        let mut fils = Vec::new();

        let (pret, attente) = channel();
        let (c, pid) = (contexte(), jeu.pid);
        fils.push(std::thread::spawn(move || son::jeu(pid, AUDIO_JEU_KBPS, c, pret)));
        if let Err(e) = attente.recv().map_err(|e| e.to_string()).and_then(|r| r) {
            self.alerter(Erreur::nouvelle("son-indisponible", format!("Son du jeu non enregistré (Windows 10 2004 ou plus requis) : {e}.")));
        }
        let mut micro_par_defaut = false;
        if r.micro {
            let (pret, attente) = channel();
            let (c, libelle, reduction, gain) = (contexte(), r.micro_libelle.clone(), r.micro_reduction.clone(), r.micro_gain);
            fils.push(std::thread::spawn(move || son::micro(libelle, reduction, gain, AUDIO_MICRO_KBPS, c, pret)));
            match attente.recv().map_err(|e| e.to_string()).and_then(|r| r) {
                Ok(defaut) => micro_par_defaut = defaut,
                Err(e) => self.alerter(Erreur::nouvelle("micro-indisponible", format!("Micro non enregistré : {e}."))),
            }
        }

        let (echec, alerte, tampon) = (signaler(&self.tx), self.app.clone(), self.tampon.clone());
        let rythme = Envoi((capture, appareil, convertisseur, encodeur, signal));
        fils.push(std::thread::spawn(move || {
            let _ = materiel::initialiser();
            let (capture, appareil, convertisseur, encodeur, signal) = rythme.prendre();
            let alerte: Box<dyn Fn(Erreur)> = Box::new(move |e| {
                let _ = alerte.emit("replay://erreur", &e);
            });
            unsafe { Rythme { capture, appareil, convertisseur, encodeur, ips, horloge, signal, tampon, echec: Box::new(echec), alerte }.tourner() };
        }));
        let hdr = blanc.map_or("SDR".to_string(), |b| format!("HDR converti en SDR (blanc {b:.0} nits)"));
        debuglog::log("info", SOURCE, &format!(
            "tampon armé pour {} : encodeur {nom_encodeur}, {codec:?}, {}x{} à {ips} i/s (source {}x{}, écran {} Hz), {debit} kbit/s, {duree} s, micro {}, {hdr}",
            jeu.nom, dimensions.0, dimensions.1, source.0, source.1, m.ecran_hz, r.micro
        ));
        self.dernier_jeu = jeu.nom;
        self.session = Some(Session { numero, arret, signal: Envoi(signal), fils, encodeur_termine, format, micro_par_defaut });
        Ok(())
    }

    fn publier(&mut self) {
        let etat = self.automate.etat();
        let format = self.session.as_ref().map(|s| s.format);
        let public = EtatPublic {
            etat: etat.nom(),
            jeu: self.automate.jeu().map(|j| j.nom.clone()),
            raison: etat.raison(),
            codec: format.map(|f| f.codec),
            largeur: format.map_or(0, |f| f.largeur),
            hauteur: format.map_or(0, |f| f.hauteur),
            ips: format.map_or(0, |f| f.ips),
            micro_par_defaut: self.session.as_ref().is_some_and(|s| s.micro_par_defaut),
        };
        let Ok(mut courant) = self.etat.lock() else { return };
        if *courant != public {
            debuglog::log("info", SOURCE, &format!("état : {}{}", public.etat, public.raison.as_ref().map_or(String::new(), |r| format!(" ({r})"))));
            *courant = public.clone();
            drop(courant);
            let _ = self.app.emit("replay://etat", &public);
        }
    }
}
