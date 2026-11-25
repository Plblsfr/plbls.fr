use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::http::ErreurApi;
use estompe_client::journal::{Journal, Niveau};
use estompe_client::musique::{
    self as m, ContexteMusique, MorceauTrouve, Repetition, VueMiniMusique, RECHERCHE_INDISPONIBLE,
    RECHERCHE_MIN,
};
use estompe_client::regarder_ensemble::salon as vue_salon;
use estompe_client::regarder_ensemble::{
    AppelSeance, Commande, FabriqueLecteur, Fournisseur, GenreSeance, LecteurSeance,
    LecteurYoutube, LienVue, OutilsSeance, PiloteSeance, PrefTampon, SeanceActive, SeanceEnCours,
    SignalSeance,
};
use estompe_client::store::Store;
use slint::{ComponentHandle, Image, Model, ModelRc, SharedPixelBuffer, VecModel};

use crate::messagerie::pont::Navigateur;
use crate::regarder_ensemble::{participant_slint, PontSeance};
use crate::session::Memoire;
use crate::video::{youtube, Pixels};
use crate::{
    DonneesMusique, ElementMusique, MiniMusique, ParticipantSeance, Principale, ResultatMusique,
    VoisinMusique, VueMusique,
};

const SOURCE_JOURNAL: &str = "musique";
const COTE_POCHETTE: u32 = 280;
const RESULTATS_MAX: usize = 6;
const RAFRAICHISSEMENT: Duration = Duration::from_millis(500);

type Pochette = (Image, Option<[u8; 3]>);

thread_local! {
    static POCHETTES: RefCell<HashMap<String, Pochette>> = RefCell::new(HashMap::new());
    static MINUTERIE: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

pub fn installer_pochette(chemin: &str, image: Image, couleur: Option<[u8; 3]>) {
    POCHETTES.with(|p| p.borrow_mut().insert(chemin.to_string(), (image, couleur)));
}

fn pochette(chemin: &str) -> Option<Pochette> {
    POCHETTES.with(|p| p.borrow().get(chemin).cloned())
}

fn image_de(chemin: &str) -> (Image, bool) {
    match pochette(chemin) {
        Some((image, _)) => (image, true),
        None => (Image::default(), false),
    }
}

pub fn decoder_pochette(octets: &[u8]) -> Option<(Pixels, Option<[u8; 3]>)> {
    let image = image::load_from_memory(octets).ok()?;
    let c = m::cadrage_pochette(image.width(), image.height());
    if c.cote == 0 {
        return None;
    }
    let carre = image.crop_imm(c.x, c.y, c.cote, c.cote);
    let couleur = m::couleur_moyenne(carre.thumbnail_exact(16, 16).to_rgba8().as_raw());
    let carre = if c.cote > COTE_POCHETTE {
        carre.thumbnail(COTE_POCHETTE, COTE_POCHETTE)
    } else {
        carre
    };
    let rgba = carre.into_rgba8();
    Some((
        SharedPixelBuffer::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height()),
        couleur,
    ))
}

fn poser<T: Clone + PartialEq + 'static>(
    actuel: ModelRc<T>,
    nouveau: Vec<T>,
    poser: impl FnOnce(ModelRc<T>),
) {
    let identique = actuel.row_count() == nouveau.len()
        && actuel.iter().zip(nouveau.iter()).all(|(a, b)| &a == b);
    if !identique {
        poser(ModelRc::new(VecModel::from(nouveau)));
    }
}

fn voisin_slint(v: Option<&m::Voisin>) -> VoisinMusique {
    let Some(v) = v else {
        return VoisinMusique::default();
    };
    let (pochette, a_pochette) = image_de(&v.pochette);
    VoisinMusique {
        present: true,
        titre: v.titre.as_str().into(),
        infobulle: v.infobulle.as_str().into(),
        pochette,
        a_pochette,
    }
}

fn mini_slint(v: &VueMiniMusique) -> MiniMusique {
    let (pochette, a_pochette) = image_de(&v.pochette);
    MiniMusique {
        salon: v.salon.as_str().into(),
        titre: v.titre.as_str().into(),
        sous_titre: v.sous_titre.as_str().into(),
        pochette,
        a_pochette,
        lecture: v.lecture,
        peut_piloter: v.peut_piloter,
        position: v.position as f32,
        duree: v.duree as f32,
        texte_position: v.texte_position.as_str().into(),
        texte_duree: v.texte_duree.as_str().into(),
        texte_restant: v.texte_restant.as_str().into(),
    }
}

fn element_slint(e: &m::ElementMusique) -> ElementMusique {
    let (pochette, a_pochette) = image_de(&e.pochette);
    ElementMusique {
        id: e.id.as_str().into(),
        titre: e.titre.as_str().into(),
        details: e.details.as_str().into(),
        duree: e.duree.as_str().into(),
        pochette,
        a_pochette,
        monter: e.monter,
        descendre: e.descendre,
        retirer: e.retirer,
    }
}

fn resultat_slint(r: &MorceauTrouve) -> ResultatMusique {
    let (pochette, a_pochette) = image_de(m::chemin_pochette(&r.cover).unwrap_or_default());
    ResultatMusique {
        id: r.id.as_str().into(),
        titre: r.title.as_str().into(),
        artiste: r.artist.as_str().into(),
        duree: m::horloge_morceau(r.duration as f64).into(),
        pochette,
        a_pochette,
    }
}

fn repetition_slint(r: Repetition) -> i32 {
    match r {
        Repetition::Aucune => 0,
        Repetition::Morceau => 1,
        Repetition::File => 2,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Lieu {
    vue: Option<String>,
    serveur: Option<String>,
    connecte: Option<String>,
}

#[derive(Default)]
struct Recherche {
    generation: u64,
    resultats: Vec<MorceauTrouve>,
    erreur: String,
    en_cours: bool,
}

#[derive(Default)]
struct Interne {
    navigateur: Option<Navigateur>,
    journal: Option<Arc<Journal>>,
    active: Option<SeanceActive>,
    lieu: Lieu,
    charges: HashSet<String>,
    avis: String,
    recherche: Recherche,
    demandees: HashSet<String>,
    volume_youtube: Option<(LienVue, u8)>,
}

#[derive(Clone)]
pub struct PontMusique {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    memoire: Option<Memoire>,
    seance: PontSeance,
    en_vol: Arc<AtomicBool>,
    lien_youtube: Arc<Mutex<Option<LienVue>>>,
}

impl PontMusique {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        memoire: Option<Memoire>,
        seance: PontSeance,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(Interne::default())),
            store,
            fenetre,
            runtime,
            memoire,
            seance,
            en_vol: Arc::new(AtomicBool::new(false)),
            lien_youtube: Arc::new(Mutex::new(None)),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lien_youtube(&self) -> MutexGuard<'_, Option<LienVue>> {
        self.lien_youtube.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn definir_outils(&self, navigateur: Navigateur, journal: Option<Arc<Journal>>) {
        let mut i = self.verrou();
        i.navigateur = Some(navigateur);
        i.journal = journal;
    }

    fn journal(&self, message: &str) {
        let journal = self.verrou().journal.clone();
        if let Some(j) = journal {
            j.ecrire(Niveau::Info, SOURCE_JOURNAL, message);
        }
    }

    fn volume(&self) -> u8 {
        self.memoire
            .as_ref()
            .map_or(m::VOLUME_PAR_DEFAUT, |mem| mem.lire(|p| p.volume_musique))
    }

    fn maintenant(&self) -> i64 {
        self.store
            .maintenant_ms()
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
    }

    fn disponible(&self) -> bool {
        self.store.lire(|e| e.bloc_actif("musique"))
    }

    fn seance_de(&self, salon: &str) -> Option<SeanceEnCours> {
        self.store
            .lire(|e| e.seances.seance(GenreSeance::Musique, salon).cloned())
    }

    fn salon_connecte(&self) -> Option<String> {
        self.verrou().lieu.connecte.clone()
    }

    fn charger_serveur(&self, serveur: &str) {
        if !self.verrou().charges.insert(serveur.to_string()) {
            return;
        }
        let (store, serveur) = (self.store.clone(), serveur.to_string());
        self.runtime.spawn(async move {
            store
                .charger_seances(&serveur, &[GenreSeance::Musique])
                .await;
        });
    }

    pub fn suivre(
        &self,
        f: &Principale,
        vue: Option<&str>,
        serveur: Option<&str>,
        connecte: Option<&str>,
    ) {
        self.verrou().lieu = Lieu {
            vue: vue.map(str::to_string),
            serveur: serveur.map(str::to_string),
            connecte: connecte.map(str::to_string),
        };
        self.actualiser(f);
    }

    pub fn reconnecte(&self, f: &Principale) {
        {
            let mut i = self.verrou();
            i.charges.clear();
            if let Some(a) = &i.active {
                a.reconnecte();
            }
        }
        self.actualiser(f);
    }

    pub fn arreter(&self) {
        let active = self.verrou().active.take();
        if let Some(a) = active {
            a.arreter();
        }
    }

    pub fn actualiser(&self, f: &Principale) {
        let Lieu {
            serveur, connecte, ..
        } = self.verrou().lieu.clone();
        let disponible = self.disponible();
        if disponible {
            let du_salon = connecte
                .as_deref()
                .and_then(|c| self.store.lire(|e| e.serveur_du_salon(c)));
            for s in serveur.iter().chain(du_salon.iter()) {
                self.charger_serveur(s);
            }
        }
        let voulue = connecte
            .filter(|_| disponible)
            .and_then(|c| self.seance_de(&c).map(|s| (c, s)));
        let perimee = {
            let mut i = self.verrou();
            let garder = i
                .active
                .as_ref()
                .is_some_and(|a| voulue.as_ref().is_some_and(|(salon, _)| a.salon() == salon));
            if garder {
                None
            } else {
                i.active.take()
            }
        };
        if let Some(a) = perimee {
            a.arreter();
        }
        if let Some((salon, seance)) = voulue {
            if self.verrou().active.is_none() {
                self.demarrer(&salon);
            }
            let i = self.verrou();
            if let Some(a) = &i.active {
                a.recevoir(Some(&seance));
            }
        }
        self.projeter(f);
    }

    fn demarrer(&self, salon: &str) {
        let moi = self.store.lire(|e| e.moi().to_string());
        let vue_youtube = youtube::disponible();
        *self.lien_youtube() = None;
        let lien_youtube = self.lien_youtube.clone();
        let fabrique: FabriqueLecteur = Box::new(move |f| {
            if f != Fournisseur::YouTube || !vue_youtube {
                return None;
            }
            let lien = LienVue::nouveau();
            *lien_youtube.lock().unwrap_or_else(|e| e.into_inner()) = Some(lien.clone());
            Some(Box::new(LecteurYoutube::nouveau(lien)) as Box<dyn LecteurSeance>)
        });
        let (pont, horloge) = (self.clone(), self.store.clone());
        let active = SeanceActive::demarrer(
            &self.runtime,
            salon,
            GenreSeance::Musique,
            PiloteSeance::nouveau(GenreSeance::Musique, moi, fabrique),
            OutilsSeance {
                client: self.store.client().clone(),
                entracte: None,
                relais: None,
                racine_depot: None,
                horloge: Arc::new(move || {
                    horloge
                        .maintenant_ms()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
                }),
                preferences: Arc::new(|_| {}),
                signal: Arc::new(move |s| pont.signal(s)),
                suivi: None,
                position: Arc::new(|| 0.0),
                tampon: PrefTampon::default(),
            },
        );
        let mut i = self.verrou();
        i.active = Some(active);
        i.avis.clear();
        i.volume_youtube = None;
    }

    fn signal(&self, s: SignalSeance) {
        match s {
            SignalSeance::Change => self.planifier(),
            SignalSeance::Refus(m) | SignalSeance::Info(m) => {
                self.verrou().avis = m;
                self.planifier();
            }
            SignalSeance::Journal(m) => self.journal(&m),
            SignalSeance::Etat(vue) => {
                self.store
                    .appliquer_seance(GenreSeance::Musique, &vue, true)
            }
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

    fn avec_active<R>(&self, action: impl FnOnce(&mut PiloteSeance, i64) -> R) -> Option<R> {
        let i = self.verrou();
        let r = i.active.as_ref().map(|a| a.avec(action));
        drop(i);
        r
    }

    fn lire_active<R>(&self, lecture: impl FnOnce(&PiloteSeance, i64) -> R) -> Option<R> {
        let i = self.verrou();
        let r = i.active.as_ref().map(|a| a.lire(lecture));
        drop(i);
        r
    }

    fn pilote_de(&self, salon: &str) -> bool {
        let i = self.verrou();
        i.active
            .as_ref()
            .is_some_and(|a| a.salon() == salon && a.lire(|p, _| p.lecteur().is_some()))
    }

    fn appel(&self, salon: String, action: &str, position: Option<f64>, rev: Option<i64>) {
        let client = self.store.client().clone();
        let appel = AppelSeance::Action {
            numero: None,
            action: action.to_string(),
            position,
            vitesse: None,
            rev,
            video: None,
        };
        self.executer(async move {
            client
                .executer_appel_seance(&salon, GenreSeance::Musique, &appel)
                .await
                .map(|_| ())
        });
    }

    fn commande(
        &self,
        f: &Principale,
        salon: &str,
        commande: impl FnOnce(&SeanceEnCours) -> Commande,
    ) {
        let Some(s) = self.seance_de(salon) else {
            return;
        };
        if !m::peut_piloter(&s, &self.store.lire(|e| e.moi().to_string())) {
            return;
        }
        let c = commande(&s);
        if self.pilote_de(salon) {
            let resultat = self.avec_active(|p, t| p.commande(c, t));
            self.verrou().avis = match resultat {
                Some(Err(e)) => e.to_string(),
                _ => String::new(),
            };
        } else {
            self.appel(salon.to_string(), c.action.nom(), c.position, None);
        }
        self.projeter(f);
    }

    pub fn basculer_salon(&self, f: &Principale, salon: &str) {
        let maintenant = self.maintenant();
        self.commande(f, salon, |s| {
            let (position, _) = m::position_musique(s, 0.0, maintenant);
            if s.etat.playing {
                Commande::pause(position)
            } else {
                Commande::lire(position)
            }
        });
    }

    pub fn aller_salon(&self, f: &Principale, salon: &str, position: f32) {
        self.commande(f, salon, |_| Commande::aller(f64::from(position.max(0.0))));
    }

    fn sur_le_salon(&self, action: impl FnOnce(&Self, String)) {
        if let Some(salon) = self.salon_connecte() {
            action(self, salon);
        }
    }

    pub fn precedent(&self) {
        self.sur_le_salon(|p, s| p.appel(s, "previous", None, None));
    }

    pub fn suivant(&self) {
        self.sur_le_salon(|p, s| {
            let rev = p.seance_de(&s).map(|x| x.etat.rev);
            p.appel(s, "ended", None, rev);
        });
    }

    pub fn melanger(&self) {
        self.sur_le_salon(|p, s| p.appel(s, "shuffle", None, None));
    }

    pub fn repetition(&self) {
        self.sur_le_salon(|p, s| {
            let suivante = p
                .seance_de(&s)
                .map(|x| Repetition::lire(&x.etat.repeat).suivante())
                .unwrap_or_default();
            let client = p.store.client().clone();
            p.executer(async move {
                client
                    .repetition_seance(&s, GenreSeance::Musique, suivante)
                    .await
            });
        });
    }

    pub fn hote_seul(&self, actif: bool) {
        self.sur_le_salon(|p, s| {
            let client = p.store.client().clone();
            p.executer(async move { client.options_seance(&s, GenreSeance::Musique, actif).await });
        });
    }

    pub fn arreter_seance(&self) {
        self.sur_le_salon(|p, s| {
            let client = p.store.client().clone();
            p.executer(async move { client.arreter_seance(&s, GenreSeance::Musique).await });
        });
    }

    pub fn regler_volume(&self, f: &Principale, volume: i32) {
        let volume = volume.clamp(0, 100) as u8;
        if let Some(mem) = &self.memoire {
            mem.modifier(|p| p.volume_musique = volume);
        }
        self.projeter(f);
    }

    pub fn ajouter(&self, f: &Principale, texte: &str) -> bool {
        let texte = texte.trim().to_string();
        if m::est_lien(&texte) {
            self.mettre_en_file(f, texte);
            return true;
        }
        if texte.chars().count() < RECHERCHE_MIN {
            return false;
        }
        let generation = {
            let mut i = self.verrou();
            i.recherche.generation += 1;
            i.recherche.en_cours = true;
            i.recherche.erreur.clear();
            i.recherche.generation
        };
        self.projeter(f);
        let (client, moi) = (self.store.client().clone(), self.clone());
        self.runtime.spawn(async move {
            let resultat = client.rechercher_musique(&texte).await;
            {
                let mut i = moi.verrou();
                if i.recherche.generation != generation {
                    return;
                }
                i.recherche.en_cours = false;
                match resultat {
                    Ok(r) => i.recherche.resultats = r.into_iter().take(RESULTATS_MAX).collect(),
                    Err(e) => {
                        i.recherche.resultats.clear();
                        i.recherche.erreur = match e.message() {
                            "" => RECHERCHE_INDISPONIBLE.to_string(),
                            texte => texte.to_string(),
                        };
                    }
                }
            }
            moi.planifier();
        });
        false
    }

    fn mettre_en_file(&self, f: &Principale, lien: String) {
        {
            let mut i = self.verrou();
            let generation = i.recherche.generation + 1;
            i.recherche = Recherche {
                generation,
                ..Recherche::default()
            };
        }
        self.sur_le_salon(|p, s| {
            let client = p.store.client().clone();
            p.executer(async move {
                client
                    .ajouter_a_la_file(&s, GenreSeance::Musique, &lien)
                    .await
            });
        });
        self.projeter(f);
    }

    pub fn choisir(&self, f: &Principale, id: &str) {
        self.mettre_en_file(f, m::lien_du_morceau(id));
    }

    pub fn retirer(&self, id: &str) {
        let id = id.to_string();
        self.sur_le_salon(|p, s| {
            let client = p.store.client().clone();
            p.executer(async move {
                client
                    .retirer_de_la_file(&s, GenreSeance::Musique, &id)
                    .await
            });
        });
    }

    pub fn deplacer(&self, rang: i32, monter: bool) {
        self.sur_le_salon(|p, s| {
            let ids: Vec<String> = p
                .seance_de(&s)
                .map(|x| x.etat.queue.iter().map(|q| q.id.clone()).collect())
                .unwrap_or_default();
            let Some(ids) = vue_salon::file_deplacee(&ids, rang.max(0) as usize, monter) else {
                return;
            };
            let client = p.store.client().clone();
            p.executer(async move { client.reordonner_file(&s, GenreSeance::Musique, &ids).await });
        });
    }

    pub fn ouvrir_navigateur(&self) {
        let url = self.lire_active(|p, t| p.vue(t).navigateur).flatten();
        let navigateur = self.verrou().navigateur.clone();
        if let (Some(url), Some(n)) = (url, navigateur) {
            let _ = n.ouvrir(&url);
        }
    }

    pub fn recharger(&self, f: &Principale) {
        self.verrou().avis.clear();
        self.avec_active(|p, t| p.recharger(t));
        self.projeter(f);
    }

    pub fn ecouter_ensemble(&self, f: &Principale) {
        let serveur = self.verrou().lieu.serveur.clone();
        self.seance
            .ouvrir_choix_avec(f, serveur, GenreSeance::Musique, Vec::new());
    }

    fn demander_pochettes(&self, chemins: Vec<String>) {
        let manquantes: Vec<String> = {
            let mut i = self.verrou();
            chemins
                .into_iter()
                .filter(|c| !c.is_empty() && pochette(c).is_none())
                .filter(|c| i.demandees.insert(c.clone()))
                .collect()
        };
        for chemin in manquantes {
            let moi = self.clone();
            self.runtime.spawn(async move {
                let Ok(octets) = moi.store.client().octets(&chemin).await else {
                    return;
                };
                let Some((pixels, couleur)) = decoder_pochette(&octets) else {
                    return;
                };
                let p = moi.clone();
                let _ = moi.fenetre.upgrade_in_event_loop(move |_| {
                    installer_pochette(&chemin, Image::from_rgba8(pixels), couleur);
                    p.planifier();
                });
            });
        }
    }

    fn pomper_youtube(&self, f: &Principale) {
        let lien = if self.verrou().active.is_some() {
            self.lien_youtube().clone()
        } else {
            None
        };
        let moi = self.clone();
        youtube::pomper_musique(
            f,
            lien.as_ref(),
            Rc::new(move || {
                moi.avec_active(|p, t| p.lecteur_change(t));
            }),
        );
        let Some(lien) = lien.filter(|l| !l.ferme()) else {
            self.verrou().volume_youtube = None;
            return;
        };
        let volume = self.volume();
        let changer = {
            let mut i = self.verrou();
            let meme = i
                .volume_youtube
                .as_ref()
                .is_some_and(|(l, v)| l.meme(&lien) && *v == volume);
            i.volume_youtube = Some((lien, volume));
            !meme
        };
        if changer {
            self.avec_active(|p, _| p.regler_volume(volume));
        }
    }

    fn rafraichir(&self, f: &Principale, actif: bool) {
        let moi = self.clone();
        let faible = f.as_weak();
        MINUTERIE.with(|t| {
            let mut t = t.borrow_mut();
            let minuterie = t.get_or_insert_with(slint::Timer::default);
            if actif && !minuterie.running() {
                minuterie.start(slint::TimerMode::Repeated, RAFRAICHISSEMENT, move || {
                    if let Some(f) = faible.upgrade() {
                        moi.projeter(&f);
                    }
                });
            } else if !actif && minuterie.running() {
                minuterie.stop();
            }
        });
    }

    pub fn projeter(&self, f: &Principale) {
        self.pomper_youtube(f);
        let g = f.global::<DonneesMusique>();
        let Lieu {
            vue,
            serveur,
            connecte,
        } = self.verrou().lieu.clone();
        let disponible = self.disponible();
        if g.get_disponible() != disponible {
            g.set_disponible(disponible);
        }
        let maintenant = self.maintenant();
        let (minis, ici) = if disponible {
            self.store.lire(|e| {
                m::minis_du_serveur(e, serveur.as_deref(), connecte.as_deref(), maintenant)
            })
        } else {
            (Vec::new(), None)
        };
        let ecoute = connecte
            .as_deref()
            .filter(|c| disponible && vue.as_deref() == Some(*c))
            .and_then(|c| self.seance_de(c).map(|s| (c.to_string(), s)));
        let (moi, personnes) = match &ecoute {
            Some((salon, _)) => self.store.lire(|e| {
                let moi = e.moi().to_string();
                let mut personnes = vue_salon::personnes_du_salon(e, salon);
                if !personnes.iter().any(|(u, _)| *u == moi) {
                    personnes.insert(0, (moi.clone(), e.pseudo().to_string()));
                }
                (moi, personnes)
            }),
            None => (String::new(), Vec::new()),
        };
        let (duree_lecteur, suspendu, erreur, navigateur) = self
            .lire_active(|p, t| {
                let v = p.vue(t);
                (v.duree, v.suspendu, v.erreur, v.navigateur.is_some())
            })
            .unwrap_or_default();
        let vm = ecoute.as_ref().and_then(|(_, s)| {
            m::vue_musique(
                s,
                &ContexteMusique {
                    moi: &moi,
                    personnes: &personnes,
                    duree_lecteur,
                    suspendu,
                    maintenant_ms: maintenant,
                },
            )
        });
        let resultats = self.verrou().recherche.resultats.clone();
        let mut chemins: Vec<String> = minis
            .iter()
            .chain(ici.iter())
            .map(|x| x.pochette.clone())
            .collect();
        chemins.extend(
            resultats
                .iter()
                .filter_map(|r| m::chemin_pochette(&r.cover).map(str::to_string)),
        );
        if let Some(v) = &vm {
            chemins.push(v.pochette.clone());
            chemins.extend(v.file.iter().map(|e| e.pochette.clone()));
            chemins.extend(
                [&v.precedent, &v.prochain]
                    .into_iter()
                    .flatten()
                    .map(|x| x.pochette.clone()),
            );
        }
        self.demander_pochettes(chemins);
        poser(g.get_minis(), minis.iter().map(mini_slint).collect(), |x| {
            g.set_minis(x)
        });
        let ici_slint = ici.as_ref().map(mini_slint).unwrap_or_default();
        if g.get_ici() != ici_slint {
            g.set_ici(ici_slint);
        }
        if g.get_a_ici() != ici.is_some() {
            g.set_a_ici(ici.is_some());
        }
        poser(
            g.get_resultats(),
            resultats.iter().map(resultat_slint).collect(),
            |x| g.set_resultats(x),
        );
        let affiche = vm.is_some();
        let vue_slint = match &vm {
            Some(v) => {
                let (pochette_image, a_pochette) = image_de(&v.pochette);
                let couleur = pochette(&v.pochette).and_then(|(_, c)| c);
                let i = self.verrou();
                let avis = if erreur.is_empty() {
                    i.avis.clone()
                } else {
                    erreur.clone()
                };
                VueMusique {
                    active: true,
                    titre: v.titre.as_str().into(),
                    details: v.details.as_str().into(),
                    pochette: pochette_image,
                    a_pochette,
                    couleur: couleur
                        .map(|[r, g, b]| slint::Color::from_rgb_u8(r, g, b))
                        .unwrap_or_default(),
                    a_couleur: couleur.is_some(),
                    peut_piloter: v.peut_piloter,
                    hote: v.hote,
                    hote_seul: v.hote_seul,
                    lecture: v.lecture,
                    position: v.position as f32,
                    duree: v.duree as f32,
                    texte_position: v.texte_position.as_str().into(),
                    texte_duree: v.texte_duree.as_str().into(),
                    texte_restant: format!(
                        "-{}",
                        estompe_client::videos::horloge((v.duree - v.position).max(0.0))
                    )
                    .into(),
                    repetition: repetition_slint(v.repetition),
                    libelle_repetition: v.repetition.description().into(),
                    melanger: v.melanger,
                    suivant: v.suivant,
                    precedent: voisin_slint(v.precedent.as_ref()),
                    prochain: voisin_slint(v.prochain.as_ref()),
                    statut: v.statut.as_str().into(),
                    avis: avis.into(),
                    navigateur,
                    volume: i32::from(self.volume()),
                    compte_file: v.file.len().min(i32::MAX as usize) as i32,
                    recherche_erreur: i.recherche.erreur.as_str().into(),
                    recherche_en_cours: i.recherche.en_cours,
                }
            }
            None => VueMusique::default(),
        };
        if g.get_vue() != vue_slint {
            g.set_vue(vue_slint);
        }
        let (auditeurs, file): (Vec<ParticipantSeance>, Vec<ElementMusique>) = match &vm {
            Some(v) => (
                v.auditeurs.iter().map(participant_slint).collect(),
                v.file.iter().map(element_slint).collect(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        poser(g.get_auditeurs(), auditeurs, |x| g.set_auditeurs(x));
        poser(g.get_file(), file, |x| g.set_file(x));
        self.rafraichir(f, affiche || ici.is_some() || !minis.is_empty());
    }

    pub fn brancher(&self, f: &Principale) {
        let g = f.global::<DonneesMusique>();
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
        rappel!(on_basculer, |p, f| {
            if let Some(s) = p.salon_connecte() {
                p.basculer_salon(&f, &s)
            }
        });
        rappel!(on_aller, |p, f, v| {
            if let Some(s) = p.salon_connecte() {
                p.aller_salon(&f, &s, v)
            }
        });
        rappel!(on_mini_basculer, |p, f, salon| p.basculer_salon(&f, &salon));
        rappel!(on_mini_aller, |p, f, salon, v| p.aller_salon(&f, &salon, v));
        rappel!(on_volume, |p, f, v| p.regler_volume(&f, v));
        rappel!(on_choisir, |p, f, id| p.choisir(&f, &id));
        rappel!(on_recharger, |p, f| p.recharger(&f));
        rappel!(on_ecouter_ensemble, |p, f| p.ecouter_ensemble(&f));
        seul!(on_precedent, |p| p.precedent());
        seul!(on_suivant, |p| p.suivant());
        seul!(on_repetition, |p| p.repetition());
        seul!(on_melanger, |p| p.melanger());
        seul!(on_hote_seul, |p, actif| p.hote_seul(actif));
        seul!(on_arreter, |p| p.arreter_seance());
        seul!(on_retirer, |p, id| p.retirer(&id));
        seul!(on_monter, |p, rang| p.deplacer(rang, true));
        seul!(on_descendre, |p, rang| p.deplacer(rang, false));
        seul!(on_ouvrir_navigateur, |p| p.ouvrir_navigateur());
        let (moi, fenetre) = (self.clone(), f.as_weak());
        g.on_ajouter(move |texte| fenetre.upgrade().is_some_and(|f| moi.ajouter(&f, &texte)));
    }
}
