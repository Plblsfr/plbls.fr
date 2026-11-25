use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_client::message_vocal::{
    decoder, decoder_avec_ffmpeg, horloge, Audio, ErreurVocal, Lecture, LIBELLES_VITESSE, VITESSES,
};
use estompe_client::store::messages::Brouillon;
use estompe_client::video::ffmpeg_du_systeme;
use estompe_plateforme::{Erreur, FluxSon};
use slint::{ComponentHandle, ModelRc, VecModel};

use super::super::saisie::Mode;
use super::super::vocal::{
    ecouter, message_du_micro, Courant, Etape, MicroPartage, NoterEcoutes, Signal, SortiePartagee,
    APERCU, ECHEC_D_ENVOI,
};
use super::{barres, verrou_de, PontMessagerie};
use crate::{DonneesMessageVocal, EtapeVocal, Principale};

pub const RAFRAICHISSEMENT_LECTURE: Duration = Duration::from_millis(100);
pub const SORTIE_INDISPONIBLE: &str =
    "Aucune sortie audio disponible pour écouter le message vocal.";

thread_local! {
    static LECTURE: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

type Lue = Arc<Mutex<Lecture>>;

fn verrou<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PontMessagerie {
    pub fn definir_son(&self, micro: MicroPartage, sortie: SortiePartagee) {
        *verrou(&self.vocal.micro) = micro;
        *verrou(&self.vocal.sortie) = sortie;
    }

    pub fn definir_ecoutes(&self, ecoutes: Vec<String>, noter: NoterEcoutes) {
        self.vocal.definir_ecoutes(ecoutes, noter);
    }

    pub fn vocal_possible(&self) -> bool {
        self.pieces_possibles() && verrou_de(&self.vocal.micro).disponible()
    }

    pub fn etape_vocal(&self) -> Etape {
        self.vocal.enregistreur().etape
    }

    pub fn demarrer_vocal(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            return;
        };
        if !self.vocal_possible() || self.etape_vocal() != Etape::Aucune {
            return;
        }
        if self.selecteur.ouvert() {
            self.fermer_selecteur(f);
        }
        if self.gifs.ouvert() {
            self.fermer_gifs(f);
        }
        self.fermer_propositions(f);
        self.saisie.signaler_erreur(f, "");
        self.arreter_lecture(f);
        self.arreter_media();
        let (generation, suivi) = {
            let mut e = self.vocal.enregistreur();
            (e.commencer(cle), e.suivi.clone())
        };
        self.afficher_vocal(f);
        let micro = verrou_de(&self.vocal.micro);
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-micro".into())
            .spawn(move || {
                let p = pont.clone();
                let resultat = ecouter(micro.as_ref(), suivi, move |signal| {
                    let q = p.clone();
                    let _ = p
                        .fenetre
                        .upgrade_in_event_loop(move |f| q.recevoir_signal(&f, generation, signal));
                });
                let p = pont.clone();
                let _ = pont
                    .fenetre
                    .upgrade_in_event_loop(move |f| p.micro_ouvert(&f, generation, resultat));
            });
    }

    fn micro_ouvert(&self, f: &Principale, generation: u64, resultat: Result<FluxSon, Erreur>) {
        let refus = {
            let mut e = self.vocal.enregistreur();
            if e.generation != generation || e.etape != Etape::Ouverture {
                return;
            }
            match resultat {
                Ok(flux) => {
                    e.flux = Some(flux);
                    e.etape = Etape::Enregistrement;
                    None
                }
                Err(erreur) => {
                    e.abandonner();
                    Some(message_du_micro(&erreur))
                }
            }
        };
        self.afficher_vocal(f);
        if let Some(message) = refus {
            self.saisie.signaler_erreur(f, message);
            self.saisie.demander_focus(f);
        }
    }

    fn recevoir_signal(&self, f: &Principale, generation: u64, signal: Signal) {
        let mut envoyer = false;
        let mut echec = None;
        {
            let mut e = self.vocal.enregistreur();
            if e.generation != generation {
                return;
            }
            match signal {
                Signal::Niveau => {}
                Signal::Limite => {
                    if e.etape == Etape::Enregistrement {
                        e.flux = None;
                        e.etape = Etape::Finalisation { envoyer: false };
                    }
                }
                Signal::Fin(Ok(prise)) => {
                    envoyer = e.etape == (Etape::Finalisation { envoyer: true });
                    e.prise = Some(Arc::new(prise));
                    e.etape = Etape::Ecoute { echec: false };
                }
                Signal::Fin(Err(raison)) => {
                    e.abandonner();
                    echec = Some(raison);
                }
            }
        }
        self.afficher_vocal(f);
        if let Some(raison) = echec {
            self.saisie.signaler_erreur(f, &raison);
        }
        if envoyer {
            self.envoyer_vocal(f);
        }
    }

    pub fn arreter_vocal(&self, f: &Principale) {
        {
            let mut e = self.vocal.enregistreur();
            if e.etape != Etape::Enregistrement {
                return;
            }
            e.flux = None;
            e.etape = Etape::Finalisation { envoyer: false };
        }
        self.afficher_vocal(f);
    }

    pub fn annuler_vocal(&self, f: &Principale) {
        let actif = {
            let mut e = self.vocal.enregistreur();
            let actif = e.etape != Etape::Aucune;
            if actif {
                e.abandonner();
            }
            actif
        };
        if !actif {
            return;
        }
        if self.vocal.lecteur().piece() == Some(APERCU) {
            self.arreter_lecture(f);
        }
        self.afficher_vocal(f);
        self.saisie.demander_focus(f);
    }

    pub fn envoyer_vocal(&self, f: &Principale) {
        let (generation, cle, prise) = {
            let mut e = self.vocal.enregistreur();
            match e.etape {
                Etape::Enregistrement => {
                    e.flux = None;
                    e.etape = Etape::Finalisation { envoyer: true };
                    drop(e);
                    self.afficher_vocal(f);
                    return;
                }
                Etape::Ecoute { .. } => {}
                _ => return,
            }
            let (Some(cle), Some(prise)) = (e.cle.clone(), e.prise.clone()) else {
                return;
            };
            e.etape = Etape::Envoi;
            (e.generation, cle, prise)
        };
        if self.vocal.lecteur().piece() == Some(APERCU) {
            self.arreter_lecture(f);
        }
        self.afficher_vocal(f);
        let reponse_a = match self.saisie.mode() {
            Mode::Reponse { id, .. } => Some(id),
            _ => None,
        };
        let pont = self.clone();
        self.runtime.spawn(async move {
            let client = pont.store.client().clone();
            let resultat = client
                .envoyer_vocal(prise.fichier.clone(), prise.duree_ms, &prise.forme)
                .await;
            let p = pont.clone();
            let piece = resultat.as_ref().ok().cloned();
            let reponse = reponse_a.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.vocal_envoye(
                    &f,
                    generation,
                    reponse,
                    resultat.map(|_| ()).map_err(|e| e.message().to_string()),
                )
            });
            if let Some(piece) = piece {
                let fichier = piece.id.clone();
                let brouillon = Brouillon {
                    pieces: vec![piece],
                    reponse_a,
                    ..Brouillon::default()
                };
                let envoi = pont.store.envoyer(&cle, brouillon).await;
                let envoye =
                    envoi.is_some_and(|id| pont.store.lire_messages(|m| m.envoi(id).is_none()));
                if let (true, true, Some(fichier)) = (envoye, pont.transcription_auto(), fichier) {
                    pont.transcrire_apres_envoi(&fichier);
                }
            }
        });
    }

    fn vocal_envoye(
        &self,
        f: &Principale,
        generation: u64,
        reponse_a: Option<String>,
        resultat: Result<(), String>,
    ) {
        {
            let mut e = self.vocal.enregistreur();
            if e.generation != generation {
                return;
            }
            match &resultat {
                Ok(()) => e.abandonner(),
                Err(_) => e.etape = Etape::Ecoute { echec: true },
            }
        }
        self.afficher_vocal(f);
        match resultat {
            Ok(()) => {
                let cite = matches!(self.saisie.mode(), Mode::Reponse { id, .. } if Some(&id) == reponse_a.as_ref());
                if cite {
                    self.saisie.annuler(f);
                }
                {
                    let mut i = self.verrou();
                    i.nouveau_depuis = None;
                    i.defilement.aller_en_bas();
                }
                self.saisie.demander_focus(f);
            }
            Err(raison) => self
                .saisie
                .signaler_erreur(f, &format!("{ECHEC_D_ENVOI} : {raison}")),
        }
    }

    fn afficher_vocal(&self, f: &Principale) {
        let v = f.global::<DonneesMessageVocal>();
        let e = self.vocal.enregistreur();
        v.set_etape(match e.etape {
            Etape::Aucune => EtapeVocal::Aucune,
            Etape::Ouverture => EtapeVocal::Ouverture,
            Etape::Enregistrement => EtapeVocal::Enregistrement,
            Etape::Finalisation { .. } => EtapeVocal::Finalisation,
            Etape::Ecoute { .. } => EtapeVocal::Ecoute,
            Etape::Envoi => EtapeVocal::Envoi,
        });
        v.set_duree(horloge(e.duree_ms()).into());
        v.set_niveaux(ModelRc::new(VecModel::from(e.niveaux_affiches())));
        v.set_forme(ModelRc::new(VecModel::from(
            e.prise
                .as_ref()
                .map(|p| barres(&p.forme))
                .unwrap_or_default(),
        )));
        v.set_echec(e.etape == Etape::Ecoute { echec: true });
    }

    pub fn basculer_lecture(&self, f: &Principale, piece: &str) {
        self.arreter_media();
        let mut l = self.vocal.lecteur();
        if let Some(c) = l.courant.as_mut().filter(|c| c.piece == piece) {
            if c.flux.is_some() || (c.jouer && c.lecture.is_none()) {
                c.flux = None;
                c.jouer = false;
                drop(l);
                self.afficher_lecture(f);
                return;
            }
            c.jouer = true;
            let lecture = c.lecture.clone();
            drop(l);
            match lecture {
                Some(lecture) => {
                    {
                        let mut lu = verrou(&lecture);
                        if lu.finie() {
                            lu.placer(0.0);
                        }
                    }
                    self.jouer(piece, lecture);
                }
                None => self.charger_audio(piece),
            }
            self.afficher_lecture(f);
            return;
        }
        l.courant = Some(Courant::nouveau(piece, true, None));
        drop(l);
        self.charger_audio(piece);
        self.afficher_lecture(f);
        if piece != APERCU && self.vocal.marquer(piece) {
            self.rafraichir(f);
        }
    }

    pub fn placer_lecture(&self, f: &Principale, piece: &str, fraction: f32) {
        self.arreter_media();
        let charger = {
            let mut l = self.vocal.lecteur();
            match l.courant.as_mut().filter(|c| c.piece == piece) {
                Some(c) => {
                    match &c.lecture {
                        Some(lecture) => verrou(lecture).placer(fraction),
                        None => c.position_demandee = Some(fraction),
                    }
                    false
                }
                None => {
                    l.courant = Some(Courant::nouveau(piece, false, Some(fraction)));
                    true
                }
            }
        };
        if charger {
            self.charger_audio(piece);
        }
        self.afficher_lecture(f);
    }

    pub fn changer_vitesse(&self, f: &Principale) {
        {
            let mut l = self.vocal.lecteur();
            l.vitesse = (l.vitesse + 1) % VITESSES.len();
            let vitesse = VITESSES[l.vitesse];
            if let Some(lecture) = l.courant.as_ref().and_then(|c| c.lecture.as_ref()) {
                verrou(lecture).definir_vitesse(vitesse);
            }
        }
        self.afficher_lecture(f);
    }

    pub fn arreter_lecture(&self, f: &Principale) {
        self.vocal.lecteur().arreter();
        self.afficher_lecture(f);
    }

    fn charger_audio(&self, piece: &str) {
        let en_cache = self
            .vocal
            .lecteur()
            .audios
            .lire(&piece.to_string())
            .cloned();
        if let Some(audio) = en_cache {
            if let Some(f) = self.fenetre.upgrade() {
                self.installer_audio(&f, piece, Ok(audio));
            }
            return;
        }
        let apercu = (piece == APERCU)
            .then(|| self.vocal.enregistreur().prise.clone())
            .flatten();
        let pont = self.clone();
        let piece = piece.to_string();
        self.runtime.spawn(async move {
            let octets = match apercu {
                Some(prise) => Ok(prise.fichier.clone()),
                None => pont
                    .store
                    .client()
                    .contenu_de_fichier(&piece)
                    .await
                    .map(|o| o.to_vec())
                    .map_err(|e| e.message().to_string()),
            };
            let audio = match octets {
                Ok(octets) => tokio::task::spawn_blocking(move || {
                    match decoder(&octets) {
                        Err(ErreurVocal::FormatInconnu) => {
                            decoder_avec_ffmpeg(&ffmpeg_du_systeme(), &octets)
                        }
                        autre => autre,
                    }
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r),
                Err(e) => Err(e),
            };
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.installer_audio(&f, &piece, audio));
        });
    }

    fn installer_audio(&self, f: &Principale, piece: &str, audio: Result<Arc<Audio>, String>) {
        let mut l = self.vocal.lecteur();
        let audio = match audio {
            Ok(audio) => audio,
            Err(_) => {
                if l.piece() == Some(piece) {
                    l.arreter();
                }
                drop(l);
                if piece != APERCU {
                    self.marquer_illisible(piece);
                }
                self.afficher_lecture(f);
                self.rafraichir(f);
                return;
            }
        };
        if piece != APERCU {
            l.audios.inserer(piece.to_string(), audio.clone());
        }
        let vitesse = VITESSES[l.vitesse];
        let a_jouer = match l.courant.as_mut().filter(|c| c.piece == piece) {
            Some(c) if c.lecture.is_none() => {
                let mut lecture = Lecture::nouvelle(audio, vitesse);
                if let Some(position) = c.position_demandee.take() {
                    lecture.placer(position);
                }
                let lecture = Arc::new(Mutex::new(lecture));
                c.lecture = Some(lecture.clone());
                c.jouer.then_some(lecture)
            }
            _ => None,
        };
        drop(l);
        if let Some(lecture) = a_jouer {
            self.jouer(piece, lecture);
        }
        self.afficher_lecture(f);
    }

    pub fn marquer_illisible(&self, piece: &str) {
        self.noter_piece(piece, |e| e.illisible = true);
    }

    fn jouer(&self, piece: &str, lecture: Lue) {
        let sortie = verrou_de(&self.vocal.sortie);
        let pont = self.clone();
        let piece = piece.to_string();
        let _ = std::thread::Builder::new()
            .name("estompe-lecture".into())
            .spawn(move || {
                let source = lecture.clone();
                let resultat = sortie.jouer(Box::new(move |tampon, taux| {
                    verrou(&source).remplir(tampon, taux);
                }));
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.sortie_ouverte(&f, &piece, &lecture, resultat)
                });
            });
    }

    fn sortie_ouverte(
        &self,
        f: &Principale,
        piece: &str,
        lecture: &Lue,
        resultat: Result<FluxSon, Erreur>,
    ) {
        let mut l = self.vocal.lecteur();
        let Some(c) = l.courant.as_mut().filter(|c| {
            c.piece == piece
                && c.jouer
                && c.flux.is_none()
                && c.lecture.as_ref().is_some_and(|x| Arc::ptr_eq(x, lecture))
        }) else {
            return;
        };
        let echec = match resultat {
            Ok(flux) => {
                c.flux = Some(flux);
                false
            }
            Err(_) => {
                c.jouer = false;
                true
            }
        };
        drop(l);
        if echec {
            self.saisie.signaler_erreur(f, SORTIE_INDISPONIBLE);
        } else {
            self.armer_lecture();
        }
        self.afficher_lecture(f);
    }

    fn armer_lecture(&self) {
        let pont = self.clone();
        LECTURE.with(|m| {
            let mut m = m.borrow_mut();
            let minuterie = m.get_or_insert_with(slint::Timer::default);
            if minuterie.running() {
                return;
            }
            minuterie.start(
                slint::TimerMode::Repeated,
                RAFRAICHISSEMENT_LECTURE,
                move || {
                    if let Some(f) = pont.fenetre.upgrade() {
                        pont.suivre_lecture(&f);
                    }
                },
            );
        });
    }

    fn suivre_lecture(&self, f: &Principale) {
        let actif = {
            let mut l = self.vocal.lecteur();
            if let Some(c) = l.courant.as_mut() {
                let finie = c.lecture.as_ref().is_some_and(|x| verrou(x).finie());
                if finie && c.flux.is_some() {
                    c.flux = None;
                    c.jouer = false;
                    if let Some(lecture) = &c.lecture {
                        verrou(lecture).placer(0.0);
                    }
                }
            }
            l.en_lecture()
        };
        self.afficher_lecture(f);
        if !actif {
            LECTURE.with(|m| {
                if let Some(t) = m.borrow().as_ref() {
                    t.stop();
                }
            });
        }
    }

    fn afficher_lecture(&self, f: &Principale) {
        let v = f.global::<DonneesMessageVocal>();
        let l = self.vocal.lecteur();
        let (position, duree) = l.position().map(|(p, d, _)| (p, d)).unwrap_or((0, 0));
        v.set_piece(l.piece().unwrap_or_default().into());
        v.set_en_lecture(l.en_lecture());
        v.set_chargement(l.chargement());
        v.set_avance(if duree > 0 {
            position as f32 / duree as f32
        } else {
            0.0
        });
        v.set_temps(horloge(position).into());
        v.set_vitesse(LIBELLES_VITESSE[l.vitesse].into());
    }
}
