use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use std::collections::{BTreeMap, HashSet};

use estompe_client::audio::son_perso::{self, FILTRE_AUDIO, TITRE_DIALOGUE};
use estompe_client::audio::{
    tonalite, Apm, Bascule, ChoixSon, EssaiMicro, Lecture, ModeBruit, NiveauMicro, ReglagesMicro,
    ReglagesSons, SensAudio, SonVocal, SonsPersonnalises, SystemeAudio,
};
use estompe_client::route::Route;
use estompe_client::store::{est_vocal, Avis, Changement, GenreSalon, Store};
use estompe_client::video::ffmpeg_du_systeme;
use estompe_client::vocal::{message_camera, ApercuCamera, Epinglage, QualiteCamera, QualiteRecue};
use estompe_client::vocal::{
    AvisVocal, CleVideo, DemandeVideo, EtatPtt, EtatVocal, Phase, PousserPourParler, ReglagesPtt,
    SessionVocale, SourceAudio, SourceTouche, SourceVideo, TrameVideo, Videos,
};
use estompe_plateforme::{
    libelle_touche, ErreurCamera, InfoCamera, RaccourcisGlobaux, SystemeCamera,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::projection::{self, Barre, Menu, Participant, Vue};
use super::touches::code_de_texte;
use crate::coquille::avatars::{CleAvatar, Profils};
use crate::coquille::modeles::synchroniser;
use crate::coquille::pont::vec_de;
use crate::messagerie::pont::SystemeFichiers;
use crate::session::Memoire;
use crate::video::{self, FluxVoulu, GestionVideo};
use crate::{
    BarreVocal, ChoixCameraVocal, DonneesVideo, DonneesVocal, ListePeripheriques, MenuVocal,
    ParticipantVocal, PeripheriqueVocal, Principale, ReglagesVocal, SalonDestination, VueVocal,
};

pub const PERIODE_MESURE: Duration = Duration::from_millis(50);
const MARGE_SON: Duration = Duration::from_millis(150);
const PAR_DEFAUT: &str = "Par défaut";
const ZONE_PAR_DEFAUT: (u32, u32) = (640, 360);
pub const RANG_PERSONNALISE: i32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glisse {
    pub salon: String,
    pub utilisateur: String,
}

pub fn donnees_glisse(salon: &str, utilisateur: &str) -> slint::DataTransfer {
    let mut donnees = slint::DataTransfer::default();
    donnees.set_user_data(Rc::new(Glisse {
        salon: salon.to_string(),
        utilisateur: utilisateur.to_string(),
    }));
    donnees
}

pub fn glisse_de(donnees: &slint::DataTransfer) -> Option<Glisse> {
    donnees
        .user_data()?
        .downcast::<Glisse>()
        .ok()
        .map(|g| (*g).clone())
}

fn indice_du_son(son: SonVocal) -> usize {
    match son {
        SonVocal::Arrivee => 0,
        SonVocal::Depart => 1,
    }
}

fn son_de(arrivee: bool) -> SonVocal {
    if arrivee {
        SonVocal::Arrivee
    } else {
        SonVocal::Depart
    }
}

#[derive(Clone)]
pub struct OutilsVocaux {
    pub session: Arc<SessionVocale>,
    pub audio: Arc<dyn SystemeAudio>,
    pub raccourcis: Arc<dyn RaccourcisGlobaux>,
    pub cameras: Arc<dyn SystemeCamera>,
    pub apm: Option<Arc<Apm>>,
}

#[derive(Default)]
struct ChoixCamera {
    ouvert: bool,
    apercu: Option<ApercuCamera>,
    erreur: String,
    generation: u64,
    cameras: Vec<InfoCamera>,
}

pub fn cle_apercu() -> CleVideo {
    CleVideo::camera("#apercu")
}

const ZONE_APERCU: (u32, u32) = (960, 540);
const ZONE_BANDE: (f32, f32) = (192.0, 108.0);

fn en_pixels(zone: (f32, f32), echelle: f32, defaut: (u32, u32)) -> (u32, u32) {
    if zone.0 >= 1.0 && zone.1 >= 1.0 {
        (
            (zone.0 * echelle).round() as u32,
            (zone.1 * echelle).round() as u32,
        )
    } else {
        defaut
    }
}

fn pluriel_spectateurs(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => "1 spectateur".to_string(),
        n => format!("{n} spectateurs"),
    }
}

pub type HorlogeMs = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Default)]
struct Ombres {
    tuiles: Vec<Participant>,
    participants: Vec<Participant>,
    vue: Option<Vue>,
    barre: Option<Barre>,
    niveau: Option<(i32, bool, bool)>,
    je_parle: bool,
    camera: Option<OmbreCamera>,
}

type OmbreCamera = (bool, String, i32, String, QualiteCamera, Vec<InfoCamera>);

struct Interne {
    outils: Option<OutilsVocaux>,
    ptt: Option<PousserPourParler>,
    etat: EtatVocal,
    niveau: NiveauMicro,
    debut_ms: Option<i64>,
    route: Route,
    menu: Option<(String, String)>,
    essai: Option<EssaiMicro>,
    ptt_etat: EtatPtt,
    impose: Option<(String, bool, bool)>,
    avis: String,
    capture_erreur: String,
    menu_erreur: String,
    plein_cadre: bool,
    demandes_avatars: HashSet<CleAvatar>,
    echecs_avatars: HashSet<CleAvatar>,
    ombres: Ombres,
    fichiers: Option<SystemeFichiers>,
    son_erreur: String,
    sons_decodes: [Option<Arc<[f32]>>; 2],
    lecture_perso: Option<(u64, Lecture)>,
    generation_perso: u64,
    erreur_depot: Option<(String, String)>,
    video: Option<GestionVideo>,
    zone_tuile: (f32, f32),
    fenetre_visible: bool,
    affichage: BTreeMap<CleVideo, DemandeVideo>,
    camera: ChoixCamera,
    epinglage: Epinglage,
    plein_ecran: Option<String>,
    zone_vedette: (f32, f32),
    menu_partage: bool,
}

#[derive(Clone)]
pub struct PontVocal {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    memoire: Option<Memoire>,
    horloge: HorlogeMs,
    profils: Arc<Profils>,
    videos_hors_session: Arc<Videos>,
}

pub fn installer_modeles(f: &Principale) {
    let v = f.global::<DonneesVocal>();
    v.set_tuiles(ModelRc::new(VecModel::<ParticipantVocal>::default()));
    v.set_participants(ModelRc::new(VecModel::<ParticipantVocal>::default()));
    v.set_micros(ModelRc::new(VecModel::<PeripheriqueVocal>::default()));
    v.set_sorties(ModelRc::new(VecModel::<PeripheriqueVocal>::default()));
    v.set_cameras(ModelRc::new(VecModel::<PeripheriqueVocal>::default()));
    video::installer_modeles(f);
}

pub fn vider_modeles(f: &Principale) {
    let v = f.global::<DonneesVocal>();
    vec_de(&v.get_tuiles()).set_vec(Vec::new());
    vec_de(&v.get_participants()).set_vec(Vec::new());
    vec_de(&f.global::<DonneesVideo>().get_flux()).set_vec(Vec::new());
    v.set_vue(VueVocal::default());
    v.set_barre(BarreVocal::default());
    v.set_menu_ouvert(false);
    v.set_reglages_ouverts(false);
    v.set_liste(ListePeripheriques::Aucune);
    v.set_niveau(0.0);
    v.set_voix(false);
    v.set_parle_micro_coupe(false);
    v.set_plein_cadre(false);
    v.set_son_erreur("".into());
    v.set_glisse_depuis("".into());
    v.set_erreur_depot_salon("".into());
    v.set_erreur_depot("".into());
    v.set_camera(ChoixCameraVocal::default());
    vec_de(&v.get_cameras()).set_vec(Vec::new());
}

fn participant_slint(p: &Participant, profils: &Profils) -> ParticipantVocal {
    ParticipantVocal {
        salon: p.salon.as_str().into(),
        utilisateur: p.utilisateur.as_str().into(),
        nom: p.nom.as_str().into(),
        initiales: p.initiales.as_str().into(),
        teinte: p.teinte,
        moi: p.moi,
        parle: p.parle,
        muet: p.muet,
        sourd: p.sourd,
        micro_impose: p.micro_impose,
        son_impose: p.son_impose,
        barres: p.barres,
        qualite: p.qualite.into(),
        deplacable: p.deplacable,
        video: p.indice_video,
        libelle_video: if p.partage { "écran" } else { "caméra" }.into(),
        cle: p.cle.as_str().into(),
        partage: p.partage,
        regarde: p.regarde,
        vedette: p.vedette,
        rang: p.rang,
        spectateurs: pluriel_spectateurs(p.spectateurs.len()).into(),
        noms_spectateurs: p.spectateurs.join(", ").into(),
        son_partage: p.son_partage,
        portrait: if p.portrait {
            p.avatar
                .as_ref()
                .and_then(|c| profils.avatar(c))
                .map(slint::Image::from_rgba8)
                .unwrap_or_default()
        } else {
            slint::Image::default()
        },
    }
}

fn vue_slint(v: &Vue) -> VueVocal {
    VueVocal {
        salon: v.salon.as_str().into(),
        nom: v.nom.as_str().into(),
        cinema: v.cinema,
        ici: v.ici,
        connexion: v.connexion,
        reconnexion: v.reconnexion,
        peut_rejoindre: v.peut_rejoindre,
        erreur: v.erreur.as_str().into(),
        compte: v.compte.as_str().into(),
        resume: v.resume.as_str().into(),
        seul: v.seul,
        colonnes: v.colonnes,
        rangees: v.rangees,
        vedette: v.vedette,
        bande: v.bande,
        plein_ecran: v.plein_ecran.as_str().into(),
    }
}

fn barre_slint(b: &Barre) -> BarreVocal {
    BarreVocal {
        visible: b.visible,
        connexion: b.connexion,
        reconnexion: b.reconnexion,
        titre: b.titre.as_str().into(),
        salon: b.salon.as_str().into(),
        serveur: b.serveur.as_str().into(),
        lieu: b.lieu.as_str().into(),
        duree: b.duree.as_str().into(),
        barres: b.barres,
        qualite: b.qualite.into(),
        micro_coupe: b.micro_coupe,
        casque_coupe: b.casque_coupe,
        micro_impose: b.micro_impose,
        son_impose: b.son_impose,
        peut_parler: b.peut_parler,
        message: b.message.as_str().into(),
        camera_active: b.camera_active,
        camera_demarrage: b.camera_demarrage,
    }
}

fn menu_slint(m: &Menu) -> MenuVocal {
    MenuVocal {
        utilisateur: m.utilisateur.as_str().into(),
        nom: m.nom.as_str().into(),
        muet: m.muet,
        volume: m.volume,
        peut_couper_micro: m.peut_couper_micro,
        peut_couper_son: m.peut_couper_son,
        micro_impose: m.micro_impose,
        son_impose: m.son_impose,
        destinations: ModelRc::new(VecModel::from(
            m.destinations
                .iter()
                .map(|(id, nom)| SalonDestination {
                    id: id.as_str().into(),
                    nom: nom.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )),
        peut_deconnecter: m.peut_deconnecter,
        erreur: m.erreur.as_str().into(),
        partage: m.partage,
        son_partage: m.son_partage,
        spectateurs: ModelRc::new(VecModel::from(
            m.spectateurs
                .iter()
                .map(|n| slint::SharedString::from(n.as_str()))
                .collect::<Vec<_>>(),
        )),
        qualites: ModelRc::new(VecModel::from(
            QualiteRecue::TOUTES
                .iter()
                .map(|q| slint::SharedString::from(q.libelle()))
                .collect::<Vec<_>>(),
        )),
        qualite: QualiteRecue::TOUTES
            .iter()
            .position(|q| *q == m.qualite_recue)
            .unwrap_or(0) as i32,
    }
}

pub fn rang_du_son(choix: ChoixSon) -> i32 {
    if choix == ChoixSon::Personnalise {
        return RANG_PERSONNALISE;
    }
    ChoixSon::PROPOSES
        .iter()
        .position(|c| *c == choix)
        .unwrap_or_default() as i32
}

pub fn son_du_rang(rang: i32) -> ChoixSon {
    if rang == RANG_PERSONNALISE {
        return ChoixSon::Personnalise;
    }
    usize::try_from(rang)
        .ok()
        .and_then(|i| ChoixSon::PROPOSES.get(i).copied())
        .unwrap_or(ChoixSon::Aucun)
}

pub fn traitement_effectif(reglages: ReglagesMicro, ptt: bool) -> ReglagesMicro {
    ReglagesMicro {
        detection_voix: reglages.detection_voix && !ptt,
        ..reglages
    }
}

fn bruit_de(niveau: i32) -> ModeBruit {
    match niveau {
        0 => ModeBruit::Aucun,
        1 => ModeBruit::Standard,
        _ => ModeBruit::Renforce,
    }
}

fn niveau_de(bruit: ModeBruit) -> i32 {
    match bruit {
        ModeBruit::Aucun => 0,
        ModeBruit::Standard => 1,
        ModeBruit::Renforce => 2,
    }
}

pub fn texte_bascule(sens: SensAudio, bascule: &Bascule) -> Option<String> {
    let (appareil, defaut) = match sens {
        SensAudio::Entree => ("Micro", "au micro"),
        SensAudio::Sortie => ("Sortie audio", "à la sortie"),
    };
    match bascule {
        Bascule::Repli(_) => Some(format!(
            "{appareil} débranché : retour {defaut} par défaut."
        )),
        Bascule::Retour(_) => Some(format!("{appareil} retrouvé.")),
        Bascule::Perdu(_) => Some(format!("{appareil} introuvable.")),
        Bascule::Choix(_) => None,
    }
}

fn marquer_portraits(liste: &mut [Participant], profils: &Profils, manquants: &mut Vec<CleAvatar>) {
    for p in liste.iter_mut() {
        if let Some(cle) = &p.avatar {
            p.portrait = profils.avatar(cle).is_some();
            if !p.portrait && !manquants.contains(cle) {
                manquants.push(cle.clone());
            }
        }
    }
}

fn salon_vocal_de_la_route(store: &Store, route: &Route) -> Option<String> {
    let Route::Salon {
        salon: Some(salon), ..
    } = route
    else {
        return None;
    };
    store.lire(|etat| {
        etat.salon(salon)
            .filter(|(_, s)| matches!(s.kind, Some(GenreSalon::Voice | GenreSalon::Cinema)))
            .map(|_| salon.clone())
    })
}

impl PontVocal {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        memoire: Option<Memoire>,
        horloge: HorlogeMs,
        profils: Arc<Profils>,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(Interne {
                outils: None,
                ptt: None,
                etat: EtatVocal::default(),
                niveau: NiveauMicro {
                    db: estompe_client::audio::micro::SILENCE_DB,
                    voix: false,
                },
                debut_ms: None,
                route: Route::Accueil,
                menu: None,
                essai: None,
                ptt_etat: EtatPtt::default(),
                impose: None,
                avis: String::new(),
                capture_erreur: String::new(),
                menu_erreur: String::new(),
                plein_cadre: false,
                demandes_avatars: HashSet::new(),
                echecs_avatars: HashSet::new(),
                ombres: Ombres::default(),
                fichiers: None,
                son_erreur: String::new(),
                sons_decodes: [None, None],
                lecture_perso: None,
                generation_perso: 0,
                erreur_depot: None,
                video: None,
                zone_tuile: (0.0, 0.0),
                fenetre_visible: true,
                affichage: BTreeMap::new(),
                camera: ChoixCamera::default(),
                epinglage: Epinglage::default(),
                plein_ecran: None,
                zone_vedette: (0.0, 0.0),
                menu_partage: false,
            })),
            store,
            fenetre,
            runtime,
            memoire,
            horloge,
            profils,
            videos_hors_session: Videos::nouvelles(),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn preferences<R>(
        &self,
        lecture: impl FnOnce(&estompe_client::preferences::Preferences) -> R,
    ) -> R {
        match &self.memoire {
            Some(m) => m.lire(lecture),
            None => lecture(&estompe_client::preferences::Preferences::default()),
        }
    }

    fn retenir(&self, modification: impl FnOnce(&mut estompe_client::preferences::Preferences)) {
        if let Some(m) = &self.memoire {
            m.modifier(modification);
        }
    }

    pub fn definir_fichiers(&self, fichiers: SystemeFichiers) {
        self.verrou().fichiers = Some(fichiers);
    }

    fn sons_perso(&self) -> Option<SonsPersonnalises> {
        self.memoire
            .as_ref()
            .map(|m| SonsPersonnalises::dans(m.dossier()))
    }

    fn echantillons_perso(&self, son: SonVocal) -> Option<Arc<[f32]>> {
        if let Some(echantillons) = self.verrou().sons_decodes[indice_du_son(son)].clone() {
            return Some(echantillons);
        }
        let octets = self.sons_perso()?.lire(son)?;
        let echantillons: Arc<[f32]> = son_perso::decoder(&octets, Some(&ffmpeg_du_systeme()))
            .ok()?
            .into();
        self.verrou().sons_decodes[indice_du_son(son)] = Some(echantillons.clone());
        Some(echantillons)
    }

    pub fn brancher(&self, outils: OutilsVocaux) {
        let reglages_ptt = self.preferences(|p| p.ptt.clone());
        let session = outils.session.clone();
        let ptt = PousserPourParler::nouveau(
            &self.runtime,
            outils.raccourcis.clone(),
            reglages_ptt.clone(),
            Arc::new(move |ouverte| session.definir_porte(ouverte)),
        );
        let traitement = self.preferences(|p| p.traitement_micro);
        outils
            .session
            .definir_traitement(traitement_effectif(traitement, reglages_ptt.actif));
        let etat = outils.session.instantane();
        let fenetre = self.fenetre.clone();
        let moi = self.clone();
        ptt.suivre(Arc::new(move |etat| {
            let moi = moi.clone();
            let _ = fenetre.upgrade_in_event_loop(move |f| moi.ptt_change(&f, etat));
        }));
        let mut i = self.verrou();
        i.etat = etat;
        i.ptt = Some(ptt);
        i.outils = Some(outils);
    }

    pub fn arreter(&self) {
        let (outils, ptt, essai, video) = {
            let mut i = self.verrou();
            i.affichage.clear();
            i.camera.ouvert = false;
            let apercu = i.camera.apercu.take();
            (
                i.outils.clone(),
                i.ptt.take(),
                i.essai.take(),
                (i.video.take(), apercu),
            )
        };
        drop(video);
        drop(essai);
        if let Some(ptt) = ptt {
            ptt.en_vocal(false);
        }
        if let Some(outils) = outils {
            if outils.session.instantane().actif() {
                outils.session.quitter();
            }
        }
    }

    pub fn outils(&self) -> Option<OutilsVocaux> {
        self.verrou().outils.clone()
    }

    pub fn session(&self) -> Option<Arc<SessionVocale>> {
        self.verrou().outils.as_ref().map(|o| o.session.clone())
    }

    pub fn etat(&self) -> EtatVocal {
        self.verrou().etat.clone()
    }

    pub fn mesure_active(&self) -> bool {
        let i = self.verrou();
        i.etat.actif() || i.essai.is_some()
    }

    pub fn definir_route(&self, route: &Route) {
        self.verrou().route = route.clone();
    }

    pub fn fenetre_active(&self, active: bool) {
        let ptt = self.verrou().ptt.clone();
        if let Some(ptt) = ptt {
            ptt.fenetre_active(active);
        }
    }

    pub fn definir_etat(&self, f: &Principale, etat: EtatVocal) {
        let (ptt, entree, sortie) = {
            let mut i = self.verrou();
            let avant = i.etat.actif();
            let maintenant = etat.actif();
            if maintenant && etat.phase != Phase::Connexion && i.debut_ms.is_none() {
                let moi = self.store.lire(|e| e.moi().to_string());
                let depuis = etat.salon.as_deref().and_then(|salon| {
                    self.store.lire(|e| {
                        e.serveur_du_salon(salon)
                            .and_then(|s| e.arrivee_vocale(&s, &moi))
                    })
                });
                i.debut_ms = Some(depuis.unwrap_or_else(|| (self.horloge)()));
            }
            if !maintenant {
                i.debut_ms = None;
                i.impose = None;
                if i.menu.is_some() {
                    i.menu = None;
                    f.global::<DonneesVocal>().set_menu_ouvert(false);
                }
            }
            if maintenant && !avant {
                i.avis.clear();
            }
            i.etat = etat;
            (i.ptt.clone(), maintenant && !avant, avant && !maintenant)
        };
        if let Some(ptt) = ptt {
            if entree {
                ptt.en_vocal(true);
            }
            if sortie {
                ptt.en_vocal(false);
            }
        }
        if entree {
            self.arreter_essai(f);
        }
        self.projeter(f);
    }

    pub fn avis(&self, avis: &AvisVocal) {
        let son = match avis {
            AvisVocal::Rejoint(_) | AvisVocal::Arrivee(_) => Some(SonVocal::Arrivee),
            AvisVocal::Quitte(_) | AvisVocal::Depart(_) => Some(SonVocal::Depart),
            _ => None,
        };
        if let AvisVocal::Peripherique(sens, bascule) = avis {
            if let Some(texte) = texte_bascule(*sens, bascule) {
                self.verrou().avis = texte;
                let moi = self.clone();
                let _ = self
                    .fenetre
                    .upgrade_in_event_loop(move |f| moi.projeter(&f));
            }
        }
        if let Some(son) = son {
            self.jouer_son(son);
        }
    }

    pub fn jouer_son(&self, son: SonVocal) {
        if !self.preferences(|p| p.sons_vocal) {
            return;
        }
        let sons = self.preferences(|p| p.sons);
        self.jouer_choix(son, sons.choix(son), sons.volume);
    }

    fn jouer_choix(&self, son: SonVocal, choix: ChoixSon, volume: u16) {
        let Some(outils) = self.outils() else {
            return;
        };
        let sortie = self.preferences(|p| p.sortie_audio.clone());
        if choix == ChoixSon::Personnalise {
            self.jouer_perso(outils.audio, sortie, son, volume);
            return;
        }
        if let Ok(Some((lecture, duree))) =
            tonalite::jouer(outils.audio.clone(), sortie, choix, volume)
        {
            self.runtime.spawn(async move {
                tokio::time::sleep(duree + MARGE_SON).await;
                drop(lecture);
            });
        }
    }

    fn jouer_perso(
        &self,
        audio: Arc<dyn SystemeAudio>,
        sortie: Option<String>,
        son: SonVocal,
        volume: u16,
    ) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let p = pont.clone();
            let lancee = tokio::task::spawn_blocking(move || {
                let echantillons = p.echantillons_perso(son)?;
                son_perso::jouer(audio, sortie, echantillons, volume)
                    .ok()
                    .flatten()
            })
            .await
            .ok()
            .flatten();
            let Some((lecture, duree)) = lancee else {
                return;
            };
            let (generation, ancienne) = {
                let mut i = pont.verrou();
                i.generation_perso += 1;
                let generation = i.generation_perso;
                (generation, i.lecture_perso.replace((generation, lecture)))
            };
            drop(ancienne);
            tokio::time::sleep(duree + MARGE_SON).await;
            let finie = {
                let mut i = pont.verrou();
                match &i.lecture_perso {
                    Some((g, _)) if *g == generation => i.lecture_perso.take(),
                    _ => None,
                }
            };
            drop(finie);
        });
    }

    pub fn choisir_fichier_son(&self, arrivee: bool) {
        let son = son_de(arrivee);
        let Some(fichiers) = self.verrou().fichiers.clone() else {
            return;
        };
        let Some(sons) = self.sons_perso() else {
            return;
        };
        if !fichiers.disponible() {
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-son".into())
            .spawn(move || {
                let Ok(Some(chemin)) =
                    fichiers.choisir_fichier(TITRE_DIALOGUE, &FILTRE_AUDIO, None)
                else {
                    return;
                };
                pont.retenir_fichier_son(&sons, son, &chemin);
            });
    }

    pub fn retenir_fichier_son(
        &self,
        sons: &SonsPersonnalises,
        son: SonVocal,
        chemin: &std::path::Path,
    ) {
        let erreur = match son_perso::lire_fichier(chemin, Some(&ffmpeg_du_systeme())) {
            Err(e) => Some(e.message().to_string()),
            Ok((octets, echantillons)) => match sons.enregistrer(son, &octets) {
                Err(e) => Some(format!("Enregistrement du son impossible : {e}")),
                Ok(()) => {
                    self.verrou().sons_decodes[indice_du_son(son)] = Some(echantillons.into());
                    self.retenir(|p| match son {
                        SonVocal::Arrivee => p.sons.arrivee = ChoixSon::Personnalise,
                        SonVocal::Depart => p.sons.depart = ChoixSon::Personnalise,
                    });
                    None
                }
            },
        };
        let retenu = erreur.is_none();
        self.verrou().son_erreur = erreur.unwrap_or_default();
        let p = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| p.projeter_reglages(&f));
        if retenu {
            self.ecouter_son(son == SonVocal::Arrivee);
        }
    }

    pub fn peut_deposer(&self, glisse: &Glisse, cible: &str) -> bool {
        if glisse.salon == cible {
            return false;
        }
        let etat = self.etat();
        self.store.lire(|e| {
            let moi = e.moi().to_string();
            let Some(serveur) = e.serveur_du_salon(&glisse.salon) else {
                return false;
            };
            e.serveur_du_salon(cible).as_deref() == Some(serveur.as_str())
                && e.salon(cible).is_some_and(|(_, s)| est_vocal(s.kind))
                && projection::participants(e, &etat, &glisse.salon, &moi, false)
                    .iter()
                    .any(|p| p.utilisateur == glisse.utilisateur && p.deplacable)
        })
    }

    pub fn deposer(&self, f: &Principale, glisse: Glisse, cible: &str) {
        let v = f.global::<DonneesVocal>();
        v.set_glisse_depuis("".into());
        if !self.peut_deposer(&glisse, cible) {
            return;
        }
        self.verrou().erreur_depot = None;
        self.projeter_depot(f);
        let client = self.store.client().clone();
        let pont = self.clone();
        let cible = cible.to_string();
        self.runtime.spawn(async move {
            let resultat = client
                .deplacer_participant_vocal(&glisse.salon, &glisse.utilisateur, &cible)
                .await;
            if let Err(erreur) = resultat {
                pont.verrou().erreur_depot =
                    Some((cible, format!("Action impossible : {}", erreur.message())));
                let p = pont.clone();
                let _ = pont
                    .fenetre
                    .upgrade_in_event_loop(move |f| p.projeter_depot(&f));
            }
        });
    }

    pub fn projeter_depot(&self, f: &Principale) {
        let erreur = self.verrou().erreur_depot.clone().unwrap_or_default();
        let v = f.global::<DonneesVocal>();
        v.set_erreur_depot_salon(erreur.0.into());
        v.set_erreur_depot(erreur.1.into());
    }

    pub fn changements(&self, f: &Principale, changements: &[Changement]) {
        let mut projeter = false;
        for changement in changements {
            match changement {
                Changement::DeplacementVocal { de, vers, par, nom } => {
                    let session = self.session();
                    let concerne = {
                        let i = self.verrou();
                        i.etat.salon.as_deref() == Some(de.as_str()) || !i.etat.actif()
                    };
                    if let (Some(session), true) = (session, concerne) {
                        session.deplace_par_moderation(de, vers);
                        self.verrou().avis = format!("{par} t'a déplacé dans « {nom} »");
                        projeter = true;
                    }
                }
                Changement::Avis(Avis::DeconnexionVocale) => {
                    self.verrou().avis = Avis::DeconnexionVocale.texte();
                    projeter = true;
                }
                _ => {}
            }
        }
        if projeter {
            self.projeter(f);
        }
    }

    fn appliquer_coupures(&self) {
        let (session, salon, deja) = {
            let i = self.verrou();
            let Some(outils) = &i.outils else {
                return;
            };
            if !i.etat.actif() {
                return;
            }
            let Some(salon) = i.etat.salon.clone() else {
                return;
            };
            (outils.session.clone(), salon, i.impose.clone())
        };
        let voulu = self.store.lire(|e| {
            let moi = e.moi();
            e.participants(&salon)
                .iter()
                .find(|p| p.utilisateur == moi)
                .map(|p| (p.micro_coupe, p.son_coupe))
        });
        let Some((micro, son)) = voulu else {
            return;
        };
        let cle = (salon, micro, son);
        if deja.as_ref() == Some(&cle) {
            return;
        }
        let initial = deja.is_none() && !micro && !son;
        self.verrou().impose = Some(cle);
        if !initial {
            session.imposer(micro, son);
        }
    }

    pub fn projeter(&self, f: &Principale) {
        self.appliquer_coupures();
        let v = f.global::<DonneesVocal>();
        let route = self.verrou().route.clone();
        let salon_vue = salon_vocal_de_la_route(&self.store, &route);
        let serveur = crate::coquille::projection::serveur_courant(&route).map(str::to_string);
        let maintenant = (self.horloge)();
        let mut guard = self.verrou();
        let i = &mut *guard;
        let je_parle = i.etat.micro_emet() && i.niveau.voix;
        i.ombres.je_parle = je_parle;
        let menu = i.menu.clone();
        let profils = &*self.profils;
        let mut manquants: Vec<CleAvatar> = Vec::new();
        let ici = salon_vue.as_deref().is_some_and(|s| i.etat.dans(s));
        if i.plein_cadre && !ici {
            i.plein_cadre = false;
            v.set_plein_cadre(false);
            f.window().set_fullscreen(false);
        }
        let qualite_recue = self.preferences(|p| p.qualite_partage);
        self.store.lire(|etat| {
            let moi = etat.moi().to_string();
            let scene = salon_vue
                .as_deref()
                .map(|s| projection::scene(etat, &i.etat, s, &moi, je_parle, &mut i.epinglage));
            let vedette = scene.as_ref().is_some_and(|s| s.vedette.is_some());
            let mut tuiles = scene.map(|s| s.tuiles).unwrap_or_default();
            if let Some(cle) = &i.plein_ecran {
                if !tuiles.iter().any(|p| &p.cle == cle && p.video.is_some()) {
                    i.plein_ecran = None;
                }
            }
            let personnes = tuiles.iter().filter(|p| !p.partage).count();
            let vue = salon_vue.as_deref().map(|s| {
                let mut vue =
                    projection::vue_de_scene(etat, &i.etat, s, personnes, tuiles.len(), vedette);
                vue.plein_ecran = i.plein_ecran.clone().unwrap_or_default();
                vue
            });
            marquer_portraits(&mut tuiles, profils, &mut manquants);
            self.projeter_videos(f, i, &mut tuiles, ici, qualite_recue);
            synchroniser(
                vec_de(&v.get_tuiles()),
                &mut i.ombres.tuiles,
                tuiles,
                |p| p.cle.clone(),
                |p| participant_slint(p, profils),
            );
            if i.ombres.vue != vue {
                v.set_vue(vue.as_ref().map(vue_slint).unwrap_or_default());
                i.ombres.vue = vue;
            }
            let mut participants = serveur
                .as_deref()
                .map(|s| projection::participants_du_serveur(etat, &i.etat, s, &moi, je_parle))
                .unwrap_or_default();
            marquer_portraits(&mut participants, profils, &mut manquants);
            synchroniser(
                vec_de(&v.get_participants()),
                &mut i.ombres.participants,
                participants,
                |p| (p.salon.clone(), p.utilisateur.clone()),
                |p| participant_slint(p, profils),
            );
            let mut barre = projection::barre(etat, &i.etat, &moi, maintenant, i.debut_ms);
            if barre.message.is_empty() {
                barre.message = i.avis.clone();
            }
            if i.ombres.barre.as_ref() != Some(&barre) {
                v.set_barre(barre_slint(&barre));
                i.ombres.barre = Some(barre);
            }
            if let Some((utilisateur, salon)) = &menu {
                let calcule = if i.menu_partage {
                    projection::menu_partage(etat, &i.etat, utilisateur, &moi, qualite_recue)
                } else {
                    projection::menu(etat, &i.etat, salon, utilisateur, &moi)
                };
                match calcule {
                    Some(mut m) => {
                        m.erreur = i.menu_erreur.clone();
                        v.set_menu(menu_slint(&m));
                    }
                    None => {
                        i.menu = None;
                        v.set_menu_ouvert(false);
                    }
                }
            }
        });
        let demandes: Vec<CleAvatar> = manquants
            .into_iter()
            .filter(|c| !i.echecs_avatars.contains(c) && i.demandes_avatars.insert(c.clone()))
            .collect();
        self.projeter_niveau(f, i);
        self.projeter_camera(f, i);
        drop(guard);
        for cle in demandes {
            self.telecharger_avatar(cle);
        }
    }

    fn projeter_videos(
        &self,
        f: &Principale,
        i: &mut Interne,
        tuiles: &mut [Participant],
        ici: bool,
        qualite_recue: QualiteRecue,
    ) {
        let session = i.outils.as_ref().map(|o| o.session.clone());
        let echelle = f.window().scale_factor();
        let grille = en_pixels(i.zone_tuile, echelle, ZONE_PAR_DEFAUT);
        let grande = en_pixels(i.zone_vedette, echelle, grille);
        let bande = en_pixels(ZONE_BANDE, echelle, ZONE_PAR_DEFAUT);
        let vedette = tuiles.iter().any(|p| p.vedette);
        let plein = i.plein_ecran.clone();
        let zone_de = |p: &Participant| match &plein {
            Some(cle) if *cle == p.cle => Some(grande),
            Some(_) => None,
            None if p.vedette => Some(grande),
            None if vedette => Some(bande),
            None => Some(grille),
        };
        let mut voulus: Vec<FluxVoulu> = match &session {
            _ if ici && i.fenetre_visible => {
                let videos = session
                    .as_ref()
                    .map_or_else(|| self.videos_hors_session.clone(), |s| s.videos());
                tuiles
                    .iter()
                    .filter_map(|p| Some((p.video.clone()?, p.moi && !p.partage, zone_de(p)?)))
                    .map(|(cle, miroir, zone)| FluxVoulu {
                        recepteur: videos.recepteur(&cle),
                        cle,
                        zone,
                        miroir,
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        let moi = i.etat.identite.clone().unwrap_or_default();
        let distants: Vec<FluxVoulu> = voulus
            .iter()
            .filter(|v| v.cle.identite != moi)
            .cloned()
            .collect();
        if i.camera.ouvert && i.fenetre_visible {
            voulus.push(FluxVoulu {
                cle: cle_apercu(),
                recepteur: self.videos_hors_session.recepteur(&cle_apercu()),
                zone: ZONE_APERCU,
                miroir: true,
            });
        }
        let gestion = i.video.get_or_insert_with(|| {
            GestionVideo::nouvelle(video::livraison_slint(self.fenetre.clone()))
        });
        gestion.synchroniser(f, &voulus);
        for p in tuiles.iter_mut() {
            p.indice_video = p.video.as_ref().map_or(-1, |c| video::indice(f, c));
        }
        let demandes: BTreeMap<CleVideo, DemandeVideo> = distants
            .iter()
            .map(|v| {
                let qualite = match v.cle.source {
                    SourceVideo::Partage => qualite_recue,
                    SourceVideo::Camera => QualiteRecue::Auto,
                };
                (v.cle.clone(), qualite.demande(v.zone))
            })
            .collect();
        if demandes != i.affichage {
            if let Some(session) = session {
                session.definir_affichage(demandes.clone());
            }
            i.affichage = demandes;
        }
    }

    fn projeter_camera(&self, f: &Principale, i: &mut Interne) {
        let choisie = i.etat.camera.clone().unwrap_or_default();
        let qualite = i.etat.qualite_camera;
        let ombre = (
            i.camera.ouvert,
            choisie.clone(),
            video::indice(f, &cle_apercu()),
            i.camera.erreur.clone(),
            qualite,
            i.camera.cameras.clone(),
        );
        if i.ombres.camera.as_ref() == Some(&ombre) {
            return;
        }
        i.ombres.camera = Some(ombre);
        let v = f.global::<DonneesVocal>();
        v.set_camera(ChoixCameraVocal {
            ouvert: i.camera.ouvert,
            choisie: choisie.as_str().into(),
            apercu: video::indice(f, &cle_apercu()),
            erreur: i.camera.erreur.as_str().into(),
            qualites: ModelRc::new(VecModel::from(
                QualiteCamera::TOUTES
                    .iter()
                    .map(|q| slint::SharedString::from(q.libelle()))
                    .collect::<Vec<_>>(),
            )),
            qualite: QualiteCamera::TOUTES
                .iter()
                .position(|q| *q == qualite)
                .unwrap_or(1) as i32,
            debit: qualite.debit_affiche().into(),
        });
        vec_de(&v.get_cameras()).set_vec(
            i.camera
                .cameras
                .iter()
                .map(|c| PeripheriqueVocal {
                    id: c.id.as_str().into(),
                    nom: c.nom.as_str().into(),
                    choisi: c.id == choisie,
                })
                .collect::<Vec<_>>(),
        );
    }

    fn rafraichir_camera(&self, f: &Principale) {
        self.projeter(f);
    }

    pub fn basculer_camera(&self, f: &Principale) {
        let etat = self.etat();
        if !etat.actif() || etat.camera_demarrage {
            return;
        }
        if etat.camera_active {
            if let Some(session) = self.session() {
                session.activer_camera(false);
            }
        } else {
            self.ouvrir_camera(f);
        }
    }

    pub fn ouvrir_camera(&self, f: &Principale) {
        self.verrou().camera.ouvert = true;
        self.demarrer_apercu(f);
    }

    fn demarrer_apercu(&self, f: &Principale) {
        let (outils, generation, id, qualite, ouvert) = {
            let mut i = self.verrou();
            i.camera.apercu = None;
            i.camera.generation += 1;
            i.camera.erreur.clear();
            (
                i.outils.clone(),
                i.camera.generation,
                i.etat.camera.clone(),
                i.etat.qualite_camera,
                i.camera.ouvert,
            )
        };
        self.videos_hors_session.recepteur(&cle_apercu()).effacer();
        self.rafraichir_camera(f);
        let Some(outils) = outils.filter(|_| ouvert) else {
            return;
        };
        let pont = self.clone();
        let recepteur = self.videos_hors_session.recepteur(&cle_apercu());
        let fenetre = self.fenetre.clone();
        self.runtime.spawn_blocking(move || {
            let cameras = outils.cameras.cameras().unwrap_or_default();
            let alerte = {
                let pont = pont.clone();
                let fenetre = fenetre.clone();
                Arc::new(move |erreur: ErreurCamera| {
                    let pont = pont.clone();
                    let _ = fenetre.upgrade_in_event_loop(move |f| {
                        {
                            let mut i = pont.verrou();
                            if i.camera.generation != generation {
                                return;
                            }
                            i.camera.erreur = message_camera(&erreur);
                        }
                        pont.rafraichir_camera(&f);
                    });
                })
            };
            let resultat = ApercuCamera::ouvrir(
                &*outils.cameras,
                id.as_deref(),
                qualite,
                recepteur,
                Some(alerte),
            );
            let _ = fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = pont.verrou();
                    if i.camera.generation != generation || !i.camera.ouvert {
                        return;
                    }
                    i.camera.cameras = cameras;
                    match resultat {
                        Ok(apercu) => i.camera.apercu = Some(apercu),
                        Err(e) => i.camera.erreur = message_camera(&e),
                    }
                }
                pont.rafraichir_camera(&f);
            });
        });
    }

    pub fn fermer_camera(&self, f: &Principale) {
        let apercu = {
            let mut i = self.verrou();
            i.camera.ouvert = false;
            i.camera.generation += 1;
            i.camera.apercu.take()
        };
        drop(apercu);
        self.videos_hors_session.recepteur(&cle_apercu()).effacer();
        self.rafraichir_camera(f);
    }

    pub fn choisir_camera(&self, f: &Principale, id: &str) {
        let id = (!id.is_empty()).then(|| id.to_string());
        self.retenir(|p| p.camera = id.clone());
        self.verrou().etat.camera = id.clone();
        if let Some(session) = self.session() {
            session.choisir_camera(id);
        }
        self.demarrer_apercu(f);
    }

    pub fn choisir_qualite_camera(&self, f: &Principale, rang: i32) {
        let Some(qualite) = QualiteCamera::TOUTES.get(rang.max(0) as usize).copied() else {
            return;
        };
        self.retenir(|p| p.qualite_camera = qualite);
        self.verrou().etat.qualite_camera = qualite;
        if let Some(session) = self.session() {
            session.definir_qualite_camera(qualite);
        }
        self.demarrer_apercu(f);
    }

    pub fn activer_camera(&self, f: &Principale) {
        self.fermer_camera(f);
        if let Some(session) = self.session() {
            session.activer_camera(true);
        }
    }

    pub fn camera_ouverte(&self) -> bool {
        self.verrou().camera.ouvert
    }

    pub fn definir_zone_tuile(&self, f: &Principale, largeur: f32, hauteur: f32) {
        {
            let mut i = self.verrou();
            if i.zone_tuile == (largeur, hauteur) {
                return;
            }
            i.zone_tuile = (largeur, hauteur);
        }
        self.projeter(f);
    }

    pub fn fenetre_visible(&self, f: &Principale, visible: bool) {
        {
            let mut i = self.verrou();
            if i.fenetre_visible == visible {
                return;
            }
            i.fenetre_visible = visible;
        }
        self.projeter(f);
    }

    pub fn images_video_converties(&self) -> u64 {
        self.verrou()
            .video
            .as_ref()
            .map_or(0, GestionVideo::images_converties)
    }

    pub fn flux_video_actifs(&self) -> usize {
        self.verrou().video.as_ref().map_or(0, GestionVideo::actifs)
    }

    pub fn poser_image_video(&self, f: &Principale, cle: &CleVideo, image: &dyn TrameVideo) {
        let zone = {
            let i = self.verrou();
            let echelle = f.window().scale_factor();
            if i.zone_tuile.0 >= 1.0 {
                (
                    (i.zone_tuile.0 * echelle).round() as u32,
                    (i.zone_tuile.1 * echelle).round() as u32,
                )
            } else {
                ZONE_PAR_DEFAUT
            }
        };
        let (zone, miroir) = if *cle == cle_apercu() {
            (ZONE_APERCU, true)
        } else {
            (
                zone,
                self.etat().identite.as_deref() == Some(cle.identite.as_str()),
            )
        };
        video::poser_pixels(f, &cle.texte(), video::convertir(image, zone, miroir));
    }

    pub fn presenter_camera(
        &self,
        f: &Principale,
        cameras: Vec<InfoCamera>,
        erreur: Option<&ErreurCamera>,
    ) {
        {
            let mut i = self.verrou();
            i.camera.cameras = cameras;
            i.camera.erreur = erreur.map(message_camera).unwrap_or_default();
        }
        self.projeter(f);
    }

    fn telecharger_avatar(&self, cle: CleAvatar) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let recu = pont.profils.charger_avatar(cle.clone()).await.is_some();
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    i.demandes_avatars.remove(&cle);
                    if !recu {
                        i.echecs_avatars.insert(cle);
                    }
                }
                p.projeter(&f);
            });
        });
    }

    pub fn basculer_plein_cadre(&self, f: &Principale, plein: bool) {
        let (route, etat) = {
            let i = self.verrou();
            (i.route.clone(), i.etat.clone())
        };
        let ici = salon_vocal_de_la_route(&self.store, &route).is_some_and(|s| etat.dans(&s));
        let plein = plein && ici;
        {
            let mut i = self.verrou();
            i.plein_cadre = plein;
            if !plein {
                i.plein_ecran = None;
            }
        }
        f.global::<DonneesVocal>().set_plein_cadre(plein);
        f.window().set_fullscreen(plein);
    }

    pub fn plein_cadre(&self) -> bool {
        self.verrou().plein_cadre
    }

    fn projeter_niveau(&self, f: &Principale, i: &mut Interne) {
        let db = i.niveau.db;
        let niveau = projection::jauge(db);
        let voix = (i.essai.is_some() || i.etat.actif()) && db > i.etat.traitement.seuil as f32;
        let parle_coupe = projection::parle_micro_coupe(&i.etat, db);
        let cle = ((niveau * 1000.0) as i32, voix, parle_coupe);
        if i.ombres.niveau != Some(cle) {
            let v = f.global::<DonneesVocal>();
            v.set_niveau(niveau);
            v.set_voix(voix);
            v.set_parle_micro_coupe(parle_coupe);
            i.ombres.niveau = Some(cle);
        }
    }

    pub fn mesurer(&self, f: &Principale) {
        let (niveau, reprojeter) = {
            let i = self.verrou();
            let niveau = match (&i.essai, &i.outils) {
                (Some(essai), _) => essai.niveau(),
                (None, Some(outils)) if i.etat.actif() => outils.session.niveau_micro(),
                _ => NiveauMicro {
                    db: estompe_client::audio::micro::SILENCE_DB,
                    voix: false,
                },
            };
            let je_parle = i.etat.micro_emet() && niveau.voix;
            let duree = i.debut_ms.map(|d| ((self.horloge)() - d) / 1000);
            let ancienne = i.debut_ms.map(|_| {
                i.ombres
                    .barre
                    .as_ref()
                    .map(|b| b.duree.clone())
                    .unwrap_or_default()
            });
            let change_duree = duree.is_some()
                && ancienne.as_deref()
                    != Some(&estompe_client::format::formater_duree(
                        duree.unwrap_or(0) as f64
                    ));
            (niveau, je_parle != i.ombres.je_parle || change_duree)
        };
        self.verrou().niveau = niveau;
        if reprojeter {
            self.projeter(f);
        } else {
            let mut guard = self.verrou();
            self.projeter_niveau(f, &mut guard);
        }
    }

    pub fn ptt_change(&self, f: &Principale, etat: EtatPtt) {
        self.verrou().ptt_etat = etat;
        self.projeter_reglages(f);
    }

    pub fn projeter_reglages(&self, f: &Principale) {
        let v = f.global::<DonneesVocal>();
        let (ptt_etat, essai, capture_erreur, global_disponible, son_erreur) = {
            let i = self.verrou();
            (
                i.ptt_etat.clone(),
                i.essai.is_some(),
                i.capture_erreur.clone(),
                i.ptt.as_ref().is_some_and(|p| p.global_disponible()),
                i.son_erreur.clone(),
            )
        };
        let perso = self.sons_perso();
        let perso = perso.as_ref();
        let (traitement, ecoute, sons, ptt, choix_sons) = self.preferences(|p| {
            (
                p.traitement_micro,
                p.ecoute.clone(),
                p.sons_vocal,
                p.ptt.clone(),
                p.sons,
            )
        });
        let message = match (&ptt_etat.erreur, ptt.actif, ptt_etat.global) {
            (Some(erreur), true, _) => erreur.clone(),
            (None, true, true) => "La touche marche aussi quand une autre fenêtre (un jeu) a le focus, et elle continue de lui parvenir.".to_string(),
            (None, true, false) if !global_disponible => estompe_client::vocal::ptt::MESSAGE_INDISPONIBLE.to_string(),
            _ => String::new(),
        };
        v.set_reglages(ReglagesVocal {
            bruit: niveau_de(traitement.bruit),
            gain: i32::from(traitement.gain),
            detection: traitement.detection_voix,
            seuil: traitement.seuil,
            general: i32::from(ecoute.general),
            sons,
            ptt: ptt.actif,
            touche: ptt.libelle().into(),
            delai: i32::from(ptt.delai_ms),
            ptt_message: message.into(),
            ptt_global: ptt_etat.global,
            son_arrivee: rang_du_son(choix_sons.arrivee),
            son_depart: rang_du_son(choix_sons.depart),
            volume_sons: i32::from(choix_sons.volume),
            perso_arrivee: perso.is_some_and(|s| s.existe(SonVocal::Arrivee)),
            perso_depart: perso.is_some_and(|s| s.existe(SonVocal::Depart)),
        });
        v.set_son_erreur(son_erreur.into());
        v.set_ptt(ptt.actif);
        v.set_touche_ptt(ptt.libelle().into());
        v.set_ptt_parle(ptt_etat.parle);
        v.set_essai(essai);
        v.set_capture_erreur(capture_erreur.into());
    }

    fn peripheriques(&self, sens: SensAudio) -> Vec<PeripheriqueVocal> {
        let Some(outils) = self.outils() else {
            return Vec::new();
        };
        let choisi = self.preferences(|p| match sens {
            SensAudio::Entree => p.micro.clone(),
            SensAudio::Sortie => p.sortie_audio.clone(),
        });
        let liste = outils.audio.peripheriques(sens).unwrap_or_default();
        let present = choisi
            .as_deref()
            .is_some_and(|c| liste.iter().any(|p| p.id == c));
        std::iter::once(PeripheriqueVocal {
            id: "".into(),
            nom: PAR_DEFAUT.into(),
            choisi: !present,
        })
        .chain(liste.into_iter().map(|p| PeripheriqueVocal {
            choisi: present && choisi.as_deref() == Some(p.id.as_str()),
            id: p.id.into(),
            nom: p.nom.into(),
        }))
        .collect()
    }

    pub fn rafraichir_peripheriques(&self, f: &Principale) {
        let v = f.global::<DonneesVocal>();
        vec_de(&v.get_micros()).set_vec(self.peripheriques(SensAudio::Entree));
        vec_de(&v.get_sorties()).set_vec(self.peripheriques(SensAudio::Sortie));
    }

    pub fn rejoindre(&self, salon: &str) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let peut = self.store.lire(|e| {
            e.salon(salon).is_some()
                && e.peut(e.serveur_du_salon(salon).as_deref(), "connect", Some(salon))
        });
        if !peut {
            return false;
        }
        let etat = self.etat();
        if etat.dans(salon) {
            return false;
        }
        if etat.actif() {
            session.deplacer(salon);
        } else {
            session.rejoindre(salon);
        }
        true
    }

    pub fn rejoindre_avec_camera(&self, salon: &str) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let peut = self.store.lire(|e| {
            e.salon(salon).is_some()
                && e.peut(e.serveur_du_salon(salon).as_deref(), "connect", Some(salon))
        });
        if !peut || self.etat().dans(salon) {
            return false;
        }
        session.rejoindre_avec_camera(salon);
        true
    }

    pub fn quitter(&self) {
        if let Some(session) = self.session() {
            session.quitter();
        }
    }

    pub fn basculer_micro(&self) {
        if let Some(session) = self.session() {
            session.basculer_micro();
        }
    }

    pub fn basculer_casque(&self) {
        if let Some(session) = self.session() {
            session.basculer_casque();
        }
    }

    pub fn ouvrir_liste(&self, f: &Principale, liste: ListePeripheriques) {
        self.rafraichir_peripheriques(f);
        f.global::<DonneesVocal>().set_liste(liste);
    }

    pub fn choisir(&self, f: &Principale, sens: SensAudio, id: &str) {
        let choisi = (!id.is_empty()).then(|| id.to_string());
        if let Some(session) = self.session() {
            match sens {
                SensAudio::Entree => session.choisir_micro(choisi.clone()),
                SensAudio::Sortie => session.choisir_sortie(choisi.clone()),
            }
        }
        self.retenir(|p| match sens {
            SensAudio::Entree => p.micro = choisi,
            SensAudio::Sortie => p.sortie_audio = choisi,
        });
        if self.verrou().essai.is_some() {
            self.arreter_essai(f);
            self.demarrer_essai(f);
        }
        self.rafraichir_peripheriques(f);
    }

    pub fn choisir_depuis_la_liste(&self, f: &Principale, id: &str) {
        let v = f.global::<DonneesVocal>();
        let sens = match v.get_liste() {
            ListePeripheriques::Micros => SensAudio::Entree,
            ListePeripheriques::Sorties => SensAudio::Sortie,
            ListePeripheriques::Aucune => return,
        };
        self.choisir(f, sens, id);
        v.set_liste(ListePeripheriques::Aucune);
    }

    pub fn ouvrir_menu(&self, f: &Principale, utilisateur: &str, salon: &str) {
        let etat = self.etat();
        let menu = self.store.lire(|e| {
            let moi = e.moi().to_string();
            projection::menu(e, &etat, salon, utilisateur, &moi)
        });
        let Some(menu) = menu else {
            return;
        };
        {
            let mut i = self.verrou();
            i.menu = Some((utilisateur.to_string(), salon.to_string()));
            i.menu_partage = false;
            i.menu_erreur.clear();
        }
        let v = f.global::<DonneesVocal>();
        v.set_menu(menu_slint(&menu));
        v.set_menu_ouvert(true);
    }

    pub fn ouvrir_menu_partage(&self, f: &Principale, utilisateur: &str) {
        let etat = self.etat();
        let qualite = self.preferences(|p| p.qualite_partage);
        let menu = self.store.lire(|e| {
            let moi = e.moi().to_string();
            projection::menu_partage(e, &etat, utilisateur, &moi, qualite)
        });
        let Some(menu) = menu else {
            return;
        };
        {
            let mut i = self.verrou();
            i.menu = Some((
                utilisateur.to_string(),
                etat.salon.clone().unwrap_or_default(),
            ));
            i.menu_partage = true;
            i.menu_erreur.clear();
        }
        let v = f.global::<DonneesVocal>();
        v.set_menu(menu_slint(&menu));
        v.set_menu_ouvert(true);
    }

    fn source_du_menu(&self) -> SourceAudio {
        if self.verrou().menu_partage {
            SourceAudio::Partage
        } else {
            SourceAudio::Micro
        }
    }

    pub fn regarder(&self, f: &Principale, utilisateur: &str, regarde: bool) {
        if let Some(session) = self.session() {
            session.regarder(utilisateur, Some(regarde));
        }
        {
            let mut i = self.verrou();
            i.etat.regardes.insert(utilisateur.to_string(), regarde);
            if !regarde {
                let cle = CleVideo::partage(utilisateur).texte();
                if i.plein_ecran.as_deref() == Some(cle.as_str()) {
                    i.plein_ecran = None;
                }
                if i.menu_partage && i.menu.as_ref().is_some_and(|(u, _)| u == utilisateur) {
                    i.menu = None;
                    i.menu_partage = false;
                    f.global::<DonneesVocal>().set_menu_ouvert(false);
                }
            }
        }
        self.projeter(f);
    }

    pub fn epingler(&self, f: &Principale, cle: &str) {
        {
            let etat = self.etat();
            let route = self.verrou().route.clone();
            let Some(salon) = salon_vocal_de_la_route(&self.store, &route) else {
                return;
            };
            let mut i = self.verrou();
            let mut epinglage = i.epinglage.clone();
            self.store.lire(|e| {
                let moi = e.moi().to_string();
                let scene = projection::scene(e, &etat, &salon, &moi, false, &mut epinglage);
                epinglage.basculer(&scene.elements, cle);
            });
            i.epinglage = epinglage;
        }
        self.projeter(f);
    }

    pub fn plein_ecran(&self, f: &Principale, cle: &str) {
        if cle.is_empty() {
            self.verrou().plein_ecran = None;
            self.basculer_plein_cadre(f, false);
            return;
        }
        self.verrou().plein_ecran = Some(cle.to_string());
        self.basculer_plein_cadre(f, true);
        self.projeter(f);
    }

    pub fn choisir_qualite_recue(&self, f: &Principale, rang: i32) {
        let Some(qualite) = QualiteRecue::TOUTES.get(rang.max(0) as usize).copied() else {
            return;
        };
        self.retenir(|p| p.qualite_partage = qualite);
        let mut menu = f.global::<DonneesVocal>().get_menu();
        menu.qualite = rang;
        f.global::<DonneesVocal>().set_menu(menu);
        self.projeter(f);
    }

    pub fn definir_zone_vedette(&self, f: &Principale, largeur: f32, hauteur: f32) {
        {
            let mut i = self.verrou();
            if i.zone_vedette == (largeur, hauteur) {
                return;
            }
            i.zone_vedette = (largeur, hauteur);
        }
        self.projeter(f);
    }

    fn menu_courant(&self) -> Option<(String, String)> {
        self.verrou().menu.clone()
    }

    pub fn muet_pour_moi(&self, f: &Principale, muet: bool) {
        let Some((utilisateur, _)) = self.menu_courant() else {
            return;
        };
        let source = self.source_du_menu();
        if let Some(session) = self.session() {
            session.definir_muet(&utilisateur, source, muet);
        }
        self.retenir(|p| p.ecoute.definir_muet(&utilisateur, source, muet));
        let mut menu = f.global::<DonneesVocal>().get_menu();
        menu.muet = muet;
        f.global::<DonneesVocal>().set_menu(menu);
    }

    pub fn volume(&self, pourcent: i32, fini: bool) {
        let Some((utilisateur, _)) = self.menu_courant() else {
            return;
        };
        let pourcent = u16::try_from(pourcent.max(0)).unwrap_or(u16::MAX);
        let source = self.source_du_menu();
        if let Some(session) = self.session() {
            session.definir_volume(&utilisateur, source, pourcent);
        }
        if fini {
            self.retenir(|p| p.ecoute.definir_volume(&utilisateur, source, pourcent));
        }
    }

    pub fn moderer(&self, micro: bool, son: bool) {
        let Some((utilisateur, salon)) = self.menu_courant() else {
            return;
        };
        let client = self.store.client().clone();
        self.runtime.spawn(async move {
            let _ = client.moderer_vocal(&salon, &utilisateur, micro, son).await;
        });
    }

    pub fn deplacer_participant(&self, vers: &str) {
        let Some((utilisateur, salon)) = self.menu_courant() else {
            return;
        };
        let client = self.store.client().clone();
        let vers = vers.to_string();
        self.action_de_moderation(async move {
            client
                .deplacer_participant_vocal(&salon, &utilisateur, &vers)
                .await
        });
    }

    pub fn deconnecter_participant(&self) {
        let Some((utilisateur, salon)) = self.menu_courant() else {
            return;
        };
        let client = self.store.client().clone();
        self.action_de_moderation(async move {
            client
                .deconnecter_participant_vocal(&salon, &utilisateur)
                .await
        });
    }

    fn action_de_moderation(
        &self,
        appel: impl std::future::Future<Output = Result<(), estompe_client::http::ErreurApi>>
            + Send
            + 'static,
    ) {
        let menu = self.menu_courant();
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = appel.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.menu_courant() != menu {
                    return;
                }
                match resultat {
                    Ok(()) => p.fermer_menu(&f),
                    Err(erreur) => {
                        p.verrou().menu_erreur =
                            format!("Action impossible : {}", erreur.message());
                        p.projeter(&f);
                    }
                }
            });
        });
    }

    pub fn fermer_menu(&self, f: &Principale) {
        {
            let mut i = self.verrou();
            i.menu = None;
            i.menu_partage = false;
            i.menu_erreur.clear();
        }
        f.global::<DonneesVocal>().set_menu_ouvert(false);
    }

    pub fn choisir_son(&self, f: &Principale, arrivee: bool, rang: i32) {
        let choix = son_du_rang(rang);
        if choix == ChoixSon::Personnalise
            && !self.sons_perso().is_some_and(|s| s.existe(son_de(arrivee)))
        {
            return;
        }
        self.retenir(|p| {
            if arrivee {
                p.sons.arrivee = choix;
            } else {
                p.sons.depart = choix;
            }
        });
        self.projeter_reglages(f);
        self.ecouter_son(arrivee);
    }

    pub fn volume_sons(&self, f: &Principale, pourcent: i32, fini: bool) {
        if !fini {
            return;
        }
        let volume = u16::try_from(pourcent.max(0)).unwrap_or(u16::MAX);
        self.retenir(|p| {
            p.sons = ReglagesSons { volume, ..p.sons }.borne();
        });
        self.projeter_reglages(f);
        self.ecouter_son(true);
    }

    pub fn ecouter_son(&self, arrivee: bool) {
        let sons = self.preferences(|p| p.sons);
        let son = son_de(arrivee);
        self.jouer_choix(son, sons.choix(son), sons.volume);
    }

    pub fn ouvrir_reglages(&self, f: &Principale) {
        self.verrou().son_erreur.clear();
        self.rafraichir_peripheriques(f);
        self.projeter_reglages(f);
        let v = f.global::<DonneesVocal>();
        v.set_reglages_decalage(0.0);
        v.set_reglages_ouverts(true);
    }

    pub fn fermer_reglages(&self, f: &Principale) {
        self.arreter_essai(f);
        {
            let mut i = self.verrou();
            i.capture_erreur.clear();
            i.son_erreur.clear();
        }
        let v = f.global::<DonneesVocal>();
        v.set_reglages_ouverts(false);
        v.set_capture_touche(false);
        self.projeter_reglages(f);
    }

    pub fn regler(&self, f: &Principale, r: ReglagesVocal, fini: bool) {
        let traitement = ReglagesMicro {
            bruit: bruit_de(r.bruit),
            gain: u16::try_from(r.gain.max(0)).unwrap_or(u16::MAX),
            detection_voix: r.detection,
            seuil: r.seuil,
        }
        .borne();
        let general = u16::try_from(r.general.max(0)).unwrap_or(u16::MAX);
        let ptt_avant = self.preferences(|p| p.ptt.clone());
        let ptt = ReglagesPtt {
            actif: r.ptt,
            delai_ms: u16::try_from(r.delai.max(0)).unwrap_or(u16::MAX),
            ..ptt_avant.clone()
        }
        .borne();
        let (session, ptt_ctrl, essai_actif) = {
            let i = self.verrou();
            (
                i.outils.as_ref().map(|o| o.session.clone()),
                i.ptt.clone(),
                i.essai.is_some(),
            )
        };
        if let Some(session) = &session {
            session.definir_traitement(traitement_effectif(traitement, ptt.actif));
            session.definir_volume_general(general);
        }
        if essai_actif {
            if let Some(essai) = &self.verrou().essai {
                essai.regler(traitement_effectif(traitement, ptt.actif));
            }
        }
        if ptt != ptt_avant {
            if let Some(ptt_ctrl) = &ptt_ctrl {
                ptt_ctrl.configurer(ptt.clone());
            }
        }
        let sons = r.sons;
        if fini || ptt != ptt_avant {
            self.retenir(|p| {
                p.traitement_micro = traitement;
                p.ecoute.definir_general(general);
                p.sons_vocal = sons;
                p.ptt = ptt;
            });
            self.projeter_reglages(f);
        }
    }

    pub fn demarrer_essai(&self, f: &Principale) {
        let Some(outils) = self.outils() else {
            return;
        };
        if self.etat().actif() {
            return;
        }
        let (micro, sortie, traitement, ptt) = self.preferences(|p| {
            (
                p.micro.clone(),
                p.sortie_audio.clone(),
                p.traitement_micro,
                p.ptt.actif,
            )
        });
        match EssaiMicro::ouvrir(
            outils.audio.clone(),
            micro,
            sortie,
            traitement_effectif(traitement, ptt),
            outils.apm.clone(),
        ) {
            Ok(essai) => {
                let mut i = self.verrou();
                i.etat.traitement = traitement;
                i.essai = Some(essai);
                i.capture_erreur.clear();
            }
            Err(e) => {
                self.verrou().capture_erreur =
                    estompe_client::vocal::CauseMicro::depuis_plateforme(&e).erreur();
            }
        }
        self.projeter_reglages(f);
    }

    pub fn arreter_essai(&self, f: &Principale) {
        let essai = self.verrou().essai.take();
        if essai.is_some() {
            drop(essai);
            let mut guard = self.verrou();
            guard.niveau = NiveauMicro {
                db: estompe_client::audio::micro::SILENCE_DB,
                voix: false,
            };
            self.projeter_niveau(f, &mut guard);
            drop(guard);
            self.projeter_reglages(f);
        }
    }

    pub fn essai(&self, f: &Principale, actif: bool) {
        if actif {
            self.demarrer_essai(f);
        } else {
            self.arreter_essai(f);
        }
    }

    pub fn touche(&self, texte: &str, appui: bool) -> bool {
        let Some(ptt) = self.verrou().ptt.clone() else {
            return false;
        };
        let Some(code) = code_de_texte(texte) else {
            return false;
        };
        if !ptt.est_la_touche(&code) {
            return false;
        }
        ptt.touche(SourceTouche::Fenetre, appui);
        true
    }

    pub fn touche_choisie(&self, f: &Principale, texte: &str) -> bool {
        let Some(code) = code_de_texte(texte) else {
            self.verrou().capture_erreur = "Cette touche ne peut pas servir à parler : choisis une lettre, un chiffre, une touche de fonction ou une touche de modification.".to_string();
            self.projeter_reglages(f);
            return false;
        };
        let ptt = ReglagesPtt {
            touche: code,
            ..self.preferences(|p| p.ptt.clone())
        }
        .borne();
        let ctrl = self.verrou().ptt.clone();
        if let Some(ctrl) = ctrl {
            ctrl.configurer(ptt.clone());
        }
        self.retenir(|p| p.ptt = ptt);
        self.verrou().capture_erreur.clear();
        self.projeter_reglages(f);
        true
    }

    pub fn libelle_ptt(&self) -> String {
        libelle_touche(&self.preferences(|p| p.ptt.touche.clone()))
    }
}

pub type SourcePontVocal = Arc<dyn Fn() -> Option<PontVocal> + Send + Sync>;
pub type OuvertureSalon = Arc<dyn Fn(&Principale, &str)>;

pub fn brancher_rappels(f: &Principale, source: SourcePontVocal, ouvrir: OuvertureSalon) {
    {
        let source = source.clone();
        let faible = f.as_weak();
        f.global::<DonneesVideo>()
            .on_zone_tuile(move |largeur, hauteur| {
                if let (Some(p), Some(f)) = (source(), faible.upgrade()) {
                    p.definir_zone_tuile(&f, largeur, hauteur);
                }
            });
    }
    {
        let source = source.clone();
        let faible = f.as_weak();
        f.global::<DonneesVideo>()
            .on_zone_vedette(move |largeur, hauteur| {
                if let (Some(p), Some(f)) = (source(), faible.upgrade()) {
                    p.definir_zone_vedette(&f, largeur, hauteur);
                }
            });
    }
    let v = f.global::<DonneesVocal>();
    macro_rules! rappel {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            v.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    {
        let source = source.clone();
        let faible = f.as_weak();
        let ouvrir = ouvrir.clone();
        v.on_rejoindre(move |salon| {
            if let (Some(p), Some(f)) = (source(), faible.upgrade()) {
                ouvrir(&f, &salon);
                p.rejoindre(&salon);
            }
        });
    }
    {
        let faible = f.as_weak();
        let ouvrir = ouvrir.clone();
        v.on_ouvrir_vocal(move |salon| {
            if let Some(f) = faible.upgrade() {
                ouvrir(&f, &salon);
            }
        });
    }
    rappel!(on_quitter, |p, _f| p.quitter());
    rappel!(on_basculer_camera, |p, f| p.basculer_camera(&f));
    rappel!(on_fermer_camera, |p, f| p.fermer_camera(&f));
    rappel!(on_choisir_camera, |p, f, id| p.choisir_camera(&f, &id));
    rappel!(on_choisir_qualite_camera, |p, f, rang| p
        .choisir_qualite_camera(&f, rang));
    rappel!(on_activer_camera, |p, f| p.activer_camera(&f));
    {
        let source = source.clone();
        let faible = f.as_weak();
        let ouvrir = ouvrir.clone();
        v.on_rejoindre_avec_camera(move |salon| {
            if let (Some(p), Some(f)) = (source(), faible.upgrade()) {
                ouvrir(&f, &salon);
                p.rejoindre_avec_camera(&salon);
            }
        });
    }
    rappel!(on_basculer_micro, |p, _f| p.basculer_micro());
    rappel!(on_basculer_casque, |p, _f| p.basculer_casque());
    rappel!(on_ouvrir_liste, |p, f, liste| p.ouvrir_liste(&f, liste));
    rappel!(on_choisir_peripherique, |p, f, id| p
        .choisir_depuis_la_liste(&f, &id));
    rappel!(on_choisir_micro_depuis_reglages, |p, f, id| p.choisir(
        &f,
        SensAudio::Entree,
        &id
    ));
    rappel!(on_choisir_sortie_depuis_reglages, |p, f, id| p.choisir(
        &f,
        SensAudio::Sortie,
        &id
    ));
    rappel!(on_ouvrir_menu, |p, f, utilisateur, salon| p.ouvrir_menu(
        &f,
        &utilisateur,
        &salon
    ));
    rappel!(on_ouvrir_menu_partage, |p, f, utilisateur| p
        .ouvrir_menu_partage(&f, &utilisateur));
    rappel!(on_regarder, |p, f, utilisateur, regarde| p.regarder(
        &f,
        &utilisateur,
        regarde
    ));
    rappel!(on_epingler, |p, f, cle| p.epingler(&f, &cle));
    rappel!(on_plein_ecran, |p, f, cle| p.plein_ecran(&f, &cle));
    rappel!(on_choisir_qualite_recue, |p, f, rang| p
        .choisir_qualite_recue(&f, rang));
    rappel!(on_muet_pour_moi, |p, f, muet| p.muet_pour_moi(&f, muet));
    rappel!(on_volume, |p, _f, valeur, fini| p.volume(valeur, fini));
    rappel!(on_moderer, |p, _f, micro, son| p.moderer(micro, son));
    rappel!(on_deplacer, |p, _f, vers| p.deplacer_participant(&vers));
    rappel!(on_deconnecter, |p, _f| p.deconnecter_participant());
    rappel!(on_basculer_plein_cadre, |p, f, plein| p
        .basculer_plein_cadre(&f, plein));
    rappel!(on_choisir_son, |p, f, arrivee, rang| p
        .choisir_son(&f, arrivee, rang));
    rappel!(on_volume_sons, |p, f, valeur, fini| p
        .volume_sons(&f, valeur, fini));
    rappel!(on_ecouter_son, |p, _f, arrivee| p.ecouter_son(arrivee));
    rappel!(on_choisir_fichier_son, |p, _f, arrivee| p
        .choisir_fichier_son(arrivee));
    v.on_donnees_glisse(|salon, utilisateur| donnees_glisse(&salon, &utilisateur));
    {
        let source = source.clone();
        v.on_peut_deposer(
            move |donnees, cible| match (source(), glisse_de(&donnees)) {
                (Some(p), Some(glisse)) => p.peut_deposer(&glisse, &cible),
                _ => false,
            },
        );
    }
    rappel!(
        on_deposer,
        |p, f, donnees, cible| if let Some(glisse) = glisse_de(&donnees) {
            p.deposer(&f, glisse, &cible)
        }
    );
    rappel!(on_ouvrir_reglages, |p, f| p.ouvrir_reglages(&f));
    rappel!(on_fermer_reglages, |p, f| p.fermer_reglages(&f));
    rappel!(on_regler, |p, f, reglages, fini| p
        .regler(&f, reglages, fini));
    rappel!(on_essai_micro, |p, f, actif| p.essai(&f, actif));
    {
        let source = source.clone();
        v.on_touche(move |texte, appui| source().is_some_and(|p| p.touche(&texte, appui)));
    }
    {
        let faible = f.as_weak();
        v.on_touche_choisie(move |texte| match (source(), faible.upgrade()) {
            (Some(p), Some(f)) => p.touche_choisie(&f, &texte),
            _ => false,
        });
    }
}

pub fn nombre_de_tuiles(f: &Principale) -> usize {
    f.global::<DonneesVocal>().get_tuiles().row_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_detection_de_voix_cede_a_l_appuyer_pour_parler() {
        let r = ReglagesMicro {
            detection_voix: true,
            ..ReglagesMicro::default()
        };
        assert!(traitement_effectif(r, false).detection_voix);
        assert!(!traitement_effectif(r, true).detection_voix);
        assert_eq!(
            bruit_de(niveau_de(ModeBruit::Standard)),
            ModeBruit::Standard
        );
    }

    #[test]
    fn le_rang_d_un_son_suit_la_liste_proposee() {
        for choix in ChoixSon::PROPOSES {
            assert_eq!(son_du_rang(rang_du_son(choix)), choix);
        }
        assert_eq!(rang_du_son(ChoixSon::Aucun), 5);
        assert_eq!(
            son_du_rang(rang_du_son(ChoixSon::Personnalise)),
            ChoixSon::Personnalise
        );
        assert_eq!(son_du_rang(42), ChoixSon::Aucun);
        assert_eq!(son_du_rang(-1), ChoixSon::Aucun);
    }

    #[test]
    fn le_glisser_porte_le_salon_et_le_participant() {
        let donnees = donnees_glisse("v1", "carol");
        assert_eq!(
            glisse_de(&donnees),
            Some(Glisse {
                salon: "v1".into(),
                utilisateur: "carol".into(),
            })
        );
        assert!(glisse_de(&slint::DataTransfer::default()).is_none());
    }

    #[test]
    fn une_bascule_de_peripherique_devient_un_avis() {
        assert_eq!(
            texte_bascule(SensAudio::Entree, &Bascule::Repli("x".into())).as_deref(),
            Some("Micro débranché : retour au micro par défaut.")
        );
        assert!(texte_bascule(SensAudio::Sortie, &Bascule::Choix("x".into())).is_none());
    }
}
