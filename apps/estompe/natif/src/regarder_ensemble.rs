use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_client::http::{Client, ErreurApi};
use estompe_client::journal::{Journal, Niveau};
use estompe_client::lecteur::{GenrePiste, Lecteur, Origine};
use estompe_client::regarder_ensemble::fournisseurs::avance_chargee;
use estompe_client::regarder_ensemble::salon::{self as vue_salon, ContexteVue, MessageSeance};
use estompe_client::regarder_ensemble::{
    touche_passer_intro, AppelSeance, Commande, FabriqueLecteur, Fournisseur, GenreSeance,
    LecteurFichier, LecteurFilm, LecteurSeance, OutilsSeance, PiloteSeance, PistesFilm, Relais,
    SeanceActive, SeanceEnCours, SignalSeance, VuePilote,
};
use estompe_client::signalements::{requete, Brouillon};
use estompe_client::store::{GenreSalon, Store};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::coquille::projection::{initiales, teinte};
use crate::menu::{self, Ancre, Entree};
use crate::messagerie::pont::Navigateur;
use crate::session::Memoire;
use crate::video::lecteur::GestionLecteur;
use crate::{
    ChoixSeance, DonneesCoquille, DonneesLecteur, DonneesSeance, ElementFileSeance,
    GenreMessageSeance, ParticipantSeance, Principale, SalonChoixSeance, SansAcces, VueSeance,
};

pub const ORIGINE: &str = "seance";
pub const AUTRE_MEDIA: &str = "Un autre média joue sur cet appareil : la séance est en pause ici.";
pub const LIEN_MANQUANT: &str = "Colle le lien d'une vidéo.";
const SOURCE_JOURNAL: &str = "seance";
const HAUTEUR_BOUTON: f32 = 32.0;

#[derive(Clone)]
pub struct OutilsSeanceApp {
    pub lecteur: Option<Arc<GestionLecteur>>,
    pub entracte: Option<Client>,
    pub racine: Option<PathBuf>,
    pub navigateur: Navigateur,
    pub journal: Option<Arc<Journal>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Lieu {
    salon: Option<String>,
    serveur: Option<String>,
    ici: bool,
}

#[derive(Debug, Clone)]
struct Choix {
    serveur: Option<String>,
    genre: GenreSeance,
    erreur: String,
}

#[derive(Default)]
struct Interne {
    outils: Option<OutilsSeanceApp>,
    active: Option<SeanceActive>,
    relais: Option<Arc<Relais>>,
    acces_films: bool,
    lieu: Lieu,
    charges: HashSet<String>,
    avis: String,
    cede: bool,
    choix: Option<Choix>,
    en_attente: Option<(String, String)>,
    demande_envoyee: bool,
    demande_en_cours: bool,
    participants: Vec<vue_salon::ParticipantSeance>,
    file: Vec<vue_salon::ElementFileVue>,
}

#[derive(Clone)]
pub struct PontSeance {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    memoire: Option<Memoire>,
    en_vol: Arc<AtomicBool>,
}

fn origine() -> Origine {
    Origine::Autre(ORIGINE.to_string())
}

fn ancre(x: f32, y: f32) -> Ancre {
    Ancre::Element {
        x,
        haut: y - HAUTEUR_BOUTON,
        bas: y,
    }
}

fn entrees(choix: Vec<vue_salon::Choix>) -> Vec<Entree> {
    choix
        .into_iter()
        .map(|c| Entree::nouvelle(&c.id, &c.libelle, "").cochee(c.coche))
        .collect()
}

fn participant_slint(p: &vue_salon::ParticipantSeance) -> ParticipantSeance {
    ParticipantSeance {
        id: p.id.as_str().into(),
        nom: p.nom.as_str().into(),
        initiales: initiales(&p.pseudo).into(),
        teinte: teinte(&p.id),
        pret: p.pret.libelle().into(),
        charge: p.pret == vue_salon::EtatPret::Chargement,
        ok: p.pret == vue_salon::EtatPret::Pret,
    }
}

fn element_slint(e: &vue_salon::ElementFileVue) -> ElementFileSeance {
    ElementFileSeance {
        id: e.id.as_str().into(),
        libelle: e.libelle.as_str().into(),
        monter: e.monter,
        descendre: e.descendre,
        retirer: e.retirer,
    }
}

struct Pistes {
    film: Option<PistesFilm>,
    audio: usize,
    sous_titres: usize,
}

fn vue_slint(
    v: &vue_salon::VueSeance,
    cinema: bool,
    avis: &str,
    pistes: &Pistes,
    avance: f64,
    compte: usize,
) -> VueSeance {
    let (genre_message, message) = match &v.message {
        MessageSeance::Aucun => (GenreMessageSeance::Aucun, String::new()),
        MessageSeance::Erreur(m) => (GenreMessageSeance::Erreur, m.clone()),
        MessageSeance::Lent => (
            GenreMessageSeance::Lent,
            vue_salon::NE_SE_CHARGE_PAS.to_string(),
        ),
        MessageSeance::Bloque => (GenreMessageSeance::Bloque, String::new()),
        MessageSeance::Attente(m) => (GenreMessageSeance::Attente, m.clone()),
    };
    let film = pistes.film.as_ref();
    VueSeance {
        active: true,
        sans_acces: false,
        cinema,
        titre: v.titre.as_str().into(),
        sous_titre: v.sous_titre.as_str().into(),
        ajoute_par: v.ajoute_par.as_str().into(),
        statut: v.statut.as_str().into(),
        suspendu: v.suspendu,
        avis: avis.into(),
        peut_piloter: v.peut_piloter,
        hote_seul: v.hote_seul,
        bascule_hote: v.bascule_hote,
        suivant: v.suivant.clone().unwrap_or_default().into(),
        arreter: v.arreter,
        direct: v.direct,
        lecture: v.lecture,
        message: message.into(),
        genre_message,
        passer_suivante: v.passer_suivante,
        grand_bouton: v.grand_bouton,
        chargement: v.chargement,
        passer_intro: v.passer_intro,
        barre: v.barre,
        position: v.position as f32,
        duree: v.duree as f32,
        temps: v.temps.as_str().into(),
        vitesse: vue_salon::libelle_vitesse_seance(v.vitesse).into(),
        navigateur: v.navigateur.clone().unwrap_or_default().into(),
        compte_file: v.compte_file.as_str().into(),
        en_lecture: v.en_lecture.as_str().into(),
        pistes_audio: film.map_or(pistes.audio > 1, |p| p.audio.len() > 1),
        pistes_sous_titres: film.map_or(pistes.sous_titres > 0, |p| !p.sous_titres.is_empty()),
        qualites: film.is_some_and(|p| p.qualites.len() > 1),
        prechargement: film.is_some(),
        avance: avance as f32,
        compte: compte.to_string().into(),
    }
}

impl PontSeance {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        memoire: Option<Memoire>,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(Interne::default())),
            store,
            fenetre,
            runtime,
            memoire,
            en_vol: Arc::new(AtomicBool::new(false)),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn maintenant(&self) -> i64 {
        self.store
            .maintenant_ms()
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
    }

    fn gestion(&self) -> Option<Arc<GestionLecteur>> {
        self.verrou()
            .outils
            .as_ref()
            .and_then(|o| o.lecteur.clone())
    }

    fn journal(&self, message: &str) {
        let journal = self
            .verrou()
            .outils
            .as_ref()
            .and_then(|o| o.journal.clone());
        if let Some(j) = journal {
            j.ecrire(Niveau::Info, SOURCE_JOURNAL, message);
        }
    }

    pub fn definir_outils(&self, outils: OutilsSeanceApp) {
        self.verrou().outils = Some(outils);
    }

    pub fn definir_acces_films(&self, f: &Principale, acces: bool) {
        self.verrou().acces_films = acces;
        self.actualiser(f);
    }

    fn salon_de(&self, salon: &str) -> (bool, String) {
        self.store.lire(|e| {
            e.salon(salon).map_or((false, String::new()), |(_, s)| {
                (
                    s.kind == Some(GenreSalon::Cinema),
                    s.name.clone().unwrap_or_default(),
                )
            })
        })
    }

    fn genre_de(&self, salon: &str) -> GenreSeance {
        if self.salon_de(salon).0 {
            GenreSeance::Cinema
        } else {
            GenreSeance::Regarder
        }
    }

    fn seance_de(&self, salon: &str) -> Option<SeanceEnCours> {
        let genre = self.genre_de(salon);
        self.store.lire(|e| e.seances.seance(genre, salon).cloned())
    }

    fn charger_serveur(&self, serveur: &str) {
        if !self.verrou().charges.insert(serveur.to_string()) {
            return;
        }
        let cinema = self.store.lire(|e| e.compte.cinema == Some(true));
        let genres = if cinema {
            vec![GenreSeance::Regarder, GenreSeance::Cinema]
        } else {
            vec![GenreSeance::Regarder]
        };
        let (store, serveur) = (self.store.clone(), serveur.to_string());
        self.runtime.spawn(async move {
            store.charger_seances(&serveur, &genres).await;
        });
    }

    pub fn suivre(&self, f: &Principale, salon: Option<&str>, serveur: Option<&str>, ici: bool) {
        let lieu = Lieu {
            salon: salon.map(str::to_string),
            serveur: serveur.map(str::to_string),
            ici,
        };
        self.verrou().lieu = lieu;
        self.actualiser(f);
    }

    fn terminer(&self) {
        let active = self.verrou().active.take();
        if let Some(a) = active {
            a.arreter();
            if let Some(g) = self.gestion() {
                g.liberer(&origine());
            }
        }
    }

    pub fn arreter(&self) {
        self.terminer();
        let mut i = self.verrou();
        i.relais = None;
        i.en_attente = None;
        i.choix = None;
    }

    pub fn reconnecte(&self, f: &Principale) {
        let serveur = {
            let mut i = self.verrou();
            i.charges.clear();
            if let Some(a) = &i.active {
                a.reconnecte();
            }
            i.lieu.serveur.clone()
        };
        if let Some(s) = serveur {
            self.charger_serveur(&s);
        }
        self.projeter(f);
    }

    fn lancer_en_attente(&self, salon: Option<&str>, ici: bool) {
        let pret = {
            let mut i = self.verrou();
            match &i.en_attente {
                Some((s, _)) if ici && Some(s.as_str()) == salon => i.en_attente.take(),
                _ => None,
            }
        };
        if let Some((salon, lien)) = pret {
            let client = self.store.client().clone();
            self.executer(async move {
                client
                    .lancer_seance(&salon, GenreSeance::Regarder, &lien, &[])
                    .await
            });
        }
    }

    pub fn actualiser(&self, f: &Principale) {
        let Lieu {
            salon,
            serveur,
            ici,
        } = self.verrou().lieu.clone();
        if let Some(s) = serveur.as_deref() {
            self.charger_serveur(s);
        }
        self.lancer_en_attente(salon.as_deref(), ici);
        let Some(salon) = salon else {
            self.terminer();
            self.projeter(f);
            return;
        };
        let genre = self.genre_de(&salon);
        let seance = self.seance_de(&salon);
        let sans_acces =
            genre == GenreSeance::Cinema && seance.is_some() && !self.verrou().acces_films;
        let voulue = ici && seance.is_some() && !sans_acces;
        let perimee = {
            let mut i = self.verrou();
            let garder = i
                .active
                .as_ref()
                .is_some_and(|a| voulue && a.salon() == salon && a.genre() == genre);
            if !voulue {
                i.cede = false;
            }
            if garder {
                None
            } else {
                i.active.take()
            }
        };
        if let Some(a) = perimee {
            a.arreter();
            if let Some(g) = self.gestion() {
                g.liberer(&origine());
            }
        }
        if voulue {
            let a_creer = {
                let i = self.verrou();
                i.active.is_none() && !i.cede
            };
            if a_creer {
                self.demarrer(&salon, genre);
            }
            let reprise = self.gestion().is_some_and(|g| !g.reservee_par(&origine()));
            if reprise {
                let active = self.verrou().active.take();
                if let Some(a) = active {
                    a.arreter();
                    self.verrou().cede = true;
                }
            }
            let i = self.verrou();
            if let Some(a) = &i.active {
                a.recevoir(seance.as_ref());
            }
            drop(i);
        }
        self.projeter(f);
    }

    fn demarrer(&self, salon: &str, genre: GenreSeance) {
        let Some(outils) = self.verrou().outils.clone() else {
            return;
        };
        let entracte = outils.entracte.clone();
        let relais = {
            let existant = self.verrou().relais.clone();
            existant.or_else(|| {
                let r = entracte
                    .as_ref()
                    .and_then(|e| Relais::demarrer_sur(&self.runtime, e.clone()).ok())
                    .map(Arc::new);
                self.verrou().relais = r.clone();
                r
            })
        };
        let lecteur: Option<Arc<Lecteur>> = outils.lecteur.as_ref().map(|g| g.lecteur().clone());
        if let Some(g) = &outils.lecteur {
            g.reserver(origine());
        }
        let moi = self.store.lire(|e| e.moi().to_string());
        let client = self.store.client().clone();
        let api = client.adresse().to_string();
        let memoire = self.memoire.clone();
        let adresse_relais = relais.as_ref().map(|r| r.adresse().to_string());
        let base_entracte = entracte.as_ref().map(|e| e.adresse().to_string());
        let client_entracte = entracte.clone();
        let l = lecteur.clone();
        let fabrique: FabriqueLecteur = Box::new(move |f| {
            let lecteur = l.clone()?;
            match f {
                Fournisseur::Fichier => {
                    let mut lf = LecteurFichier::nouveau(lecteur);
                    if let Some(j) = client.jeton() {
                        lf = lf.avec_jeton_pour(api.clone(), j);
                    }
                    Some(Box::new(lf) as Box<dyn LecteurSeance>)
                }
                Fournisseur::Film => {
                    let base = base_entracte.clone()?;
                    let prefs = memoire
                        .as_ref()
                        .map(|m| m.lire(|p| p.film.clone()))
                        .unwrap_or_default();
                    let mut lf = LecteurFilm::nouveau(lecteur, base, prefs);
                    if let Some(j) = client_entracte.as_ref().and_then(Client::jeton) {
                        lf = lf.avec_jeton(j);
                    }
                    if let Some(r) = &adresse_relais {
                        lf = lf.avec_relais(r.clone());
                    }
                    Some(Box::new(lf) as Box<dyn LecteurSeance>)
                }
                _ => None,
            }
        });
        let tampon = self
            .memoire
            .as_ref()
            .map(|m| m.lire(|p| p.film.tampon))
            .unwrap_or_default();
        let (pont, horloge, retenue, position) = (
            self.clone(),
            self.store.clone(),
            self.memoire.clone(),
            lecteur.clone(),
        );
        let active = SeanceActive::demarrer(
            &self.runtime,
            salon,
            genre,
            PiloteSeance::nouveau(genre, moi, fabrique),
            OutilsSeance {
                client: self.store.client().clone(),
                entracte,
                relais,
                racine_depot: outils.racine.clone(),
                horloge: Arc::new(move || {
                    horloge
                        .maintenant_ms()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
                }),
                preferences: Arc::new(move |p| {
                    if let Some(m) = &retenue {
                        m.modifier(|x| x.film = p);
                    }
                }),
                signal: Arc::new(move |s| pont.signal(genre, s)),
                suivi: lecteur.as_ref().map(|l| l.observer()),
                position: Arc::new(move || position.as_ref().map_or(0.0, |l| l.position())),
                tampon,
            },
        );
        self.verrou().active = Some(active);
    }

    fn signal(&self, genre: GenreSeance, s: SignalSeance) {
        match s {
            SignalSeance::Change => self.planifier(),
            SignalSeance::Refus(m) | SignalSeance::Info(m) => {
                self.verrou().avis = m;
                self.planifier();
            }
            SignalSeance::Journal(m) => self.journal(&m),
            SignalSeance::Etat(vue) => self.store.appliquer_seance(genre, &vue, true),
        }
    }

    fn planifier(&self) {
        if self.en_vol.swap(true, Ordering::AcqRel) {
            return;
        }
        let moi = self.clone();
        let resultat = self.fenetre.upgrade_in_event_loop(move |f| {
            moi.en_vol.store(false, Ordering::Release);
            moi.projeter(&f);
        });
        if resultat.is_err() {
            self.en_vol.store(false, Ordering::Release);
        }
    }

    fn executer(&self, futur: impl Future<Output = Result<(), ErreurApi>> + Send + 'static) {
        let moi = self.clone();
        self.runtime.spawn(async move {
            if let Err(e) = futur.await {
                moi.verrou().avis = e.message().to_string();
                moi.planifier();
            }
        });
    }

    fn salon_courant(&self) -> Option<(String, GenreSeance)> {
        let salon = self.verrou().lieu.salon.clone()?;
        let genre = self.genre_de(&salon);
        Some((salon, genre))
    }

    fn avec_active<R>(&self, action: impl FnOnce(&mut PiloteSeance, i64) -> R) -> Option<R> {
        let i = self.verrou();
        let r = i.active.as_ref().map(|a| a.avec(action));
        drop(i);
        r
    }

    fn commande(&self, f: &Principale, fabrique: impl FnOnce(&PiloteSeance, i64) -> Commande) {
        let resultat = self.avec_active(|p, t| {
            let c = fabrique(p, t);
            p.commande(c, t)
        });
        self.verrou().avis = match resultat {
            Some(Err(m)) => m.to_string(),
            _ => String::new(),
        };
        self.projeter(f);
    }

    pub fn basculer(&self, f: &Principale) {
        self.commande(f, |p, t| {
            let position = p.vue(t).position;
            if p.seance().is_some_and(|s| s.etat.playing) {
                Commande::pause(position)
            } else {
                Commande::lire(position)
            }
        });
    }

    pub fn aller(&self, f: &Principale, position: f32) {
        self.commande(f, |_, _| Commande::aller(f64::from(position)));
    }

    pub fn suivant(&self) {
        let Some((salon, genre)) = self.salon_courant() else {
            return;
        };
        let Some(rev) = self.seance_de(&salon).map(|s| s.etat.rev) else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(async move {
            let appel = AppelSeance::Action {
                numero: None,
                action: "ended".into(),
                position: None,
                vitesse: None,
                rev: Some(rev),
                video: None,
            };
            client
                .executer_appel_seance(&salon, genre, &appel)
                .await
                .map(|_| ())
        });
    }

    pub fn arreter_seance(&self) {
        let Some((salon, genre)) = self.salon_courant() else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(async move { client.arreter_seance(&salon, genre).await });
    }

    pub fn recharger(&self, f: &Principale) {
        let cede = std::mem::take(&mut self.verrou().cede);
        if cede {
            self.actualiser(f);
            return;
        }
        self.avec_active(|p, t| p.recharger(t));
        self.projeter(f);
    }

    pub fn reprendre(&self) {
        self.avec_active(|p, _| p.reprendre());
    }

    pub fn hote_seul(&self, actif: bool) {
        let Some((salon, genre)) = self.salon_courant() else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(async move { client.options_seance(&salon, genre, actif).await });
    }

    pub fn passer_intro(&self, f: &Principale) -> bool {
        let fait = self.avec_active(|p, t| p.passer_intro(t)).unwrap_or(false);
        self.projeter(f);
        fait
    }

    pub fn touche(&self, f: &Principale, texte: &str, ctrl: bool, meta: bool, alt: bool) -> bool {
        if touche_passer_intro(texte, ctrl, meta, alt) {
            return self.passer_intro(f);
        }
        if !(ctrl || meta || alt) && (texte == " " || texte.eq_ignore_ascii_case("k")) {
            self.basculer(f);
            return true;
        }
        false
    }

    pub fn retirer(&self, id: &str) {
        let Some((salon, genre)) = self.salon_courant() else {
            return;
        };
        let (client, id) = (self.store.client().clone(), id.to_string());
        self.executer(async move { client.retirer_de_la_file(&salon, genre, &id).await });
    }

    pub fn deplacer(&self, rang: i32, monter: bool) {
        let Some((salon, genre)) = self.salon_courant() else {
            return;
        };
        let ids: Vec<String> = self
            .seance_de(&salon)
            .map(|s| s.etat.queue.iter().map(|q| q.id.clone()).collect())
            .unwrap_or_default();
        let Some(ids) = vue_salon::file_deplacee(&ids, rang.max(0) as usize, monter) else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(async move { client.reordonner_file(&salon, genre, &ids).await });
    }

    pub fn ajouter(&self, lien: &str) {
        let lien = lien.trim().to_string();
        let Some((salon, genre)) = self.salon_courant() else {
            return;
        };
        if lien.is_empty() {
            return;
        }
        let client = self.store.client().clone();
        self.executer(async move { client.ajouter_a_la_file(&salon, genre, &lien).await });
    }

    pub fn ouvrir_navigateur(&self) {
        let url = self.avec_active(|p, t| p.vue(t).navigateur).flatten();
        let navigateur = self.verrou().outils.as_ref().map(|o| o.navigateur.clone());
        if let (Some(url), Some(n)) = (url, navigateur) {
            let _ = n.ouvrir(&url);
        }
    }

    pub fn ouvrir_choix(&self, f: &Principale) {
        let serveur = self.verrou().lieu.serveur.clone();
        self.verrou().choix = Some(Choix {
            serveur,
            genre: GenreSeance::Regarder,
            erreur: String::new(),
        });
        self.projeter_choix(f);
    }

    pub fn fermer_choix(&self, f: &Principale) {
        self.verrou().choix = None;
        self.projeter_choix(f);
    }

    pub fn lancer(&self, f: &Principale, salon: &str, lien: &str) {
        let lien = lien.trim().to_string();
        if lien.is_empty() {
            if let Some(c) = self.verrou().choix.as_mut() {
                c.erreur = LIEN_MANQUANT.to_string();
            }
            self.projeter_choix(f);
            return;
        }
        let ici = {
            let mut i = self.verrou();
            i.choix = None;
            i.lieu.ici && i.lieu.salon.as_deref() == Some(salon)
        };
        if ici {
            let (client, salon) = (self.store.client().clone(), salon.to_string());
            self.executer(async move {
                client
                    .lancer_seance(&salon, GenreSeance::Regarder, &lien, &[])
                    .await
            });
        } else {
            self.verrou().en_attente = Some((salon.to_string(), lien));
            f.global::<DonneesCoquille>()
                .invoke_ouvrir_salon(salon.into());
        }
        self.projeter_choix(f);
    }

    pub fn demander_acces(&self, f: &Principale) {
        let Some(salon) = self.verrou().lieu.salon.clone() else {
            return;
        };
        let (pseudo, nom) = (
            self.store.lire(|e| e.pseudo().to_string()),
            self.salon_de(&salon).1,
        );
        let (title, description) = vue_salon::demande_d_acces(&pseudo, &nom);
        let corps = requete(
            &Brouillon {
                title,
                description,
                category: "autre".into(),
            },
            &[],
            None,
            Vec::new(),
        );
        self.verrou().demande_en_cours = true;
        self.projeter(f);
        let (client, moi) = (self.store.client().clone(), self.clone());
        self.runtime.spawn(async move {
            let resultat = client.envoyer_signalement(&corps).await;
            {
                let mut i = moi.verrou();
                i.demande_en_cours = false;
                match resultat {
                    Ok(()) => i.demande_envoyee = true,
                    Err(e) => i.avis = e.message().to_string(),
                }
            }
            moi.planifier();
        });
    }

    pub fn menu_vitesse(&self, f: &Principale, x: f32, y: f32) {
        let Some((salon, _)) = self.salon_courant() else {
            return;
        };
        let vitesse = self.seance_de(&salon).map_or(1.0, |s| s.etat.rate);
        let moi = self.clone();
        menu::ouvrir(
            f,
            "Vitesse",
            ancre(x, y),
            entrees(vue_salon::choix_vitesses(vitesse)),
            move |f, id| {
                if let Some(v) = vue_salon::vitesse_choisie(id) {
                    moi.commande(f, |_, _| Commande::vitesse(v));
                }
                false
            },
        );
    }

    fn pistes_film(&self) -> Option<PistesFilm> {
        self.avec_active(|p, _| p.pistes()).flatten()
    }

    pub fn menu_audio(&self, f: &Principale, x: f32, y: f32) {
        let Some(pistes) = self.pistes_film() else {
            f.global::<DonneesLecteur>().invoke_menu_audio(x, y);
            return;
        };
        let moi = self.clone();
        menu::ouvrir(
            f,
            "Piste audio",
            ancre(x, y),
            entrees(vue_salon::choix_audio(&pistes)),
            move |f, id| {
                if let Ok(n) = id.parse::<i64>() {
                    moi.avec_active(|p, _| p.choisir_audio(n));
                    moi.projeter(f);
                }
                false
            },
        );
    }

    pub fn menu_sous_titres(&self, f: &Principale, x: f32, y: f32) {
        let Some(pistes) = self.pistes_film() else {
            f.global::<DonneesLecteur>().invoke_menu_sous_titres(x, y);
            return;
        };
        let moi = self.clone();
        menu::ouvrir(
            f,
            "Sous-titres",
            ancre(x, y),
            entrees(vue_salon::choix_sous_titres(&pistes)),
            move |f, id| {
                let piste = id.parse::<i64>().ok();
                moi.avec_active(|p, _| p.choisir_sous_titres(piste));
                moi.projeter(f);
                false
            },
        );
    }

    pub fn menu_qualite(&self, f: &Principale, x: f32, y: f32) {
        let Some(pistes) = self.pistes_film() else {
            return;
        };
        let moi = self.clone();
        let choix = vue_salon::choix_qualites(&pistes);
        menu::ouvrir(f, "Qualité", ancre(x, y), entrees(choix), move |f, id| {
            if let Some(q) = vue_salon::qualite_choisie(&pistes, id) {
                moi.avec_active(|p, _| p.choisir_qualite(q));
                moi.projeter(f);
            }
            false
        });
    }

    pub fn menu_prechargement(&self, f: &Principale, x: f32, y: f32) {
        let actuel = self
            .memoire
            .as_ref()
            .map(|m| m.lire(|p| p.film.tampon))
            .unwrap_or_default();
        let moi = self.clone();
        menu::ouvrir(
            f,
            "Préchargement sur cet appareil",
            ancre(x, y),
            entrees(vue_salon::choix_prechargement(actuel)),
            move |_, id| {
                if let Some(t) = vue_salon::prechargement_choisi(id) {
                    moi.avec_active(|p, _| p.regler_tampon(t));
                }
                false
            },
        );
    }

    fn pistes_du_lecteur(&self, film: Option<PistesFilm>) -> Pistes {
        let etat = self.gestion().map(|g| g.lecteur().etat());
        Pistes {
            film,
            audio: etat
                .as_ref()
                .map_or(0, |e| e.pistes_de(GenrePiste::Audio).count()),
            sous_titres: etat
                .as_ref()
                .map_or(0, |e| e.pistes_de(GenrePiste::SousTitres).count()),
        }
    }

    pub fn projeter(&self, f: &Principale) {
        let g = f.global::<DonneesSeance>();
        let Lieu { salon, ici, .. } = self.verrou().lieu.clone();
        let seance = salon.as_deref().and_then(|s| self.seance_de(s));
        let (Some(salon), Some(seance), true) = (salon, seance, ici) else {
            if g.get_vue() != VueSeance::default() {
                g.set_vue(VueSeance::default());
            }
            self.projeter_choix(f);
            return;
        };
        let (cinema, nom) = self.salon_de(&salon);
        let genre = if cinema {
            GenreSeance::Cinema
        } else {
            GenreSeance::Regarder
        };
        let (moi, pseudo, mut personnes, signalements) = self.store.lire(|e| {
            (
                e.moi().to_string(),
                e.pseudo().to_string(),
                vue_salon::personnes_du_salon(e, &salon),
                e.bloc_actif("signalements"),
            )
        });
        if !personnes.iter().any(|(u, _)| *u == moi) {
            personnes.insert(0, (moi.clone(), pseudo));
        }
        let acces = self.verrou().acces_films;
        if cinema && !acces {
            let autres: Vec<&str> = personnes
                .iter()
                .filter(|(u, _)| *u != moi)
                .map(|(_, p)| p.as_str())
                .collect();
            let i = self.verrou();
            g.set_sans_acces(SansAcces {
                salon: nom.as_str().into(),
                titre: seance
                    .etat
                    .meta
                    .as_ref()
                    .map(|m| m.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Film ou série".to_string())
                    .into(),
                texte: vue_salon::texte_sans_acces(&autres).into(),
                demande_possible: signalements,
                demande_envoyee: i.demande_envoyee,
                occupe: i.demande_en_cours,
            });
            drop(i);
            g.set_vue(VueSeance {
                sans_acces: true,
                cinema: true,
                ..VueSeance::default()
            });
            self.projeter_choix(f);
            return;
        }
        let (vp, intro, lecteur_present, film) = {
            let i = self.verrou();
            let r = match &i.active {
                Some(a) => a.lire(|p, t| {
                    (
                        p.vue(t),
                        p.intro_passable(t),
                        p.lecteur().is_some(),
                        p.pistes(),
                    )
                }),
                None => (
                    VuePilote {
                        erreur: if i.cede {
                            AUTRE_MEDIA.to_string()
                        } else {
                            String::new()
                        },
                        peut_piloter: !seance.etat.host_only || seance.etat.host == moi,
                        ..VuePilote::default()
                    },
                    false,
                    false,
                    None,
                ),
            };
            drop(i);
            r
        };
        let avance = vp.position + avance_chargee(vp.position, &vp.plages);
        let v = vue_salon::vue_seance(
            &seance,
            &vp,
            &ContexteVue {
                genre,
                moi: &moi,
                personnes: &personnes,
                lecteur: lecteur_present,
                intro_passable: intro,
            },
        );
        let pistes = self.pistes_du_lecteur(film);
        let mut i = self.verrou();
        let vue = vue_slint(&v, cinema, &i.avis, &pistes, avance, personnes.len());
        if g.get_vue() != vue {
            g.set_vue(vue);
        }
        if i.participants != v.participants {
            g.set_participants(ModelRc::new(VecModel::from(
                v.participants
                    .iter()
                    .map(participant_slint)
                    .collect::<Vec<_>>(),
            )));
            i.participants = v.participants.clone();
        }
        if i.file != v.file {
            g.set_file(ModelRc::new(VecModel::from(
                v.file.iter().map(element_slint).collect::<Vec<_>>(),
            )));
            i.file = v.file.clone();
        }
        drop(i);
        self.projeter_choix(f);
    }

    fn projeter_choix(&self, f: &Principale) {
        let g = f.global::<DonneesSeance>();
        let (choix, lieu) = {
            let i = self.verrou();
            (i.choix.clone(), i.lieu.clone())
        };
        let Some(c) = choix else {
            if g.get_choix().ouvert {
                g.set_choix(ChoixSeance::default());
            }
            return;
        };
        let connecte = if lieu.ici { lieu.salon.clone() } else { None };
        let salons = self.store.lire(|e| {
            vue_salon::salons_possibles(e, c.serveur.as_deref(), c.genre, connecte.as_deref())
        });
        g.set_salons(ModelRc::new(VecModel::from(
            salons
                .iter()
                .map(|s| SalonChoixSeance {
                    id: s.id.as_str().into(),
                    nom: s.nom.as_str().into(),
                    nom_serveur: s.nom_serveur.clone().unwrap_or_default().into(),
                    en_cours: s.en_cours.clone().unwrap_or_default().into(),
                    ici: s.ici,
                    personnes: ModelRc::new(VecModel::from(
                        s.personnes
                            .iter()
                            .map(|(id, pseudo)| ParticipantSeance {
                                id: id.as_str().into(),
                                nom: pseudo.as_str().into(),
                                initiales: initiales(pseudo).into(),
                                teinte: teinte(id),
                                ..ParticipantSeance::default()
                            })
                            .collect::<Vec<_>>(),
                    )),
                    plus: s.plus().unwrap_or_default().into(),
                })
                .collect::<Vec<_>>(),
        )));
        g.set_choix(ChoixSeance {
            ouvert: true,
            titre: vue_salon::titre_choix(c.genre).into(),
            vide: vue_salon::aucun_salon(c.genre, c.serveur.is_some()).into(),
            occupe: false,
            erreur: c.erreur.as_str().into(),
            lien: true,
            cinema: c.genre == GenreSeance::Cinema,
        });
    }

    pub fn brancher(&self, f: &Principale) {
        let g = f.global::<DonneesSeance>();
        let pont = self.clone();
        let faible = f.as_weak();
        macro_rules! rappel {
            ($on:ident, |$p:ident, $f:ident $(, $a:ident)*| $corps:expr) => {{
                let $p = pont.clone();
                let faible = faible.clone();
                g.$on(move |$($a),*| {
                    if let Some($f) = faible.upgrade() {
                        $corps;
                    }
                });
            }};
        }
        macro_rules! seul {
            ($on:ident, |$p:ident $(, $a:ident)*| $corps:expr) => {{
                let $p = pont.clone();
                g.$on(move |$($a),*| {
                    $corps;
                });
            }};
        }
        rappel!(on_basculer, |p, f| p.basculer(&f));
        rappel!(on_aller, |p, f, v| p.aller(&f, v));
        rappel!(on_menu_vitesse, |p, f, x, y| p.menu_vitesse(&f, x, y));
        rappel!(on_menu_audio, |p, f, x, y| p.menu_audio(&f, x, y));
        rappel!(on_menu_sous_titres, |p, f, x, y| p
            .menu_sous_titres(&f, x, y));
        rappel!(on_menu_qualite, |p, f, x, y| p.menu_qualite(&f, x, y));
        rappel!(on_menu_prechargement, |p, f, x, y| p
            .menu_prechargement(&f, x, y));
        rappel!(on_recharger, |p, f| p.recharger(&f));
        rappel!(on_passer_intro, |p, f| p.passer_intro(&f));
        rappel!(on_ouvrir_choix, |p, f| p.ouvrir_choix(&f));
        rappel!(on_fermer_choix, |p, f| p.fermer_choix(&f));
        rappel!(on_lancer, |p, f, salon, lien| p.lancer(&f, &salon, &lien));
        rappel!(on_demander_acces, |p, f| p.demander_acces(&f));
        seul!(on_suivant, |p| p.suivant());
        seul!(on_arreter, |p| p.arreter_seance());
        seul!(on_reprendre, |p| p.reprendre());
        seul!(on_hote_seul, |p, actif| p.hote_seul(actif));
        seul!(on_retirer, |p, id| p.retirer(&id));
        seul!(on_monter, |p, rang| p.deplacer(rang, true));
        seul!(on_descendre, |p, rang| p.deplacer(rang, false));
        seul!(on_ajouter, |p, lien| p.ajouter(&lien));
        seul!(on_ouvrir_navigateur, |p| p.ouvrir_navigateur());
        let (moi, fenetre) = (self.clone(), f.as_weak());
        g.on_touche(move |texte, ctrl, meta, alt| {
            fenetre
                .upgrade()
                .is_some_and(|f| moi.touche(&f, &texte, ctrl, meta, alt))
        });
    }
}
