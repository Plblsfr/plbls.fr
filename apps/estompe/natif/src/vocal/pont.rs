use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::audio::{
    tonalite, Apm, Bascule, EssaiMicro, ModeBruit, NiveauMicro, ReglagesMicro, SensAudio, SonVocal,
    SystemeAudio,
};
use estompe_client::route::Route;
use estompe_client::store::{Avis, Changement, GenreSalon, Store};
use estompe_client::vocal::{
    AvisVocal, EtatPtt, EtatVocal, Phase, PousserPourParler, ReglagesPtt, SessionVocale,
    SourceAudio, SourceTouche,
};
use estompe_plateforme::{libelle_touche, RaccourcisGlobaux};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::projection::{self, Barre, Menu, Participant, Vue};
use super::touches::code_de_texte;
use crate::coquille::modeles::synchroniser;
use crate::coquille::pont::vec_de;
use crate::session::Memoire;
use crate::{
    BarreVocal, DonneesVocal, ListePeripheriques, MenuVocal, ParticipantVocal, PeripheriqueVocal,
    Principale, ReglagesVocal, VueVocal,
};

pub const PERIODE_MESURE: Duration = Duration::from_millis(50);
const MARGE_SON: Duration = Duration::from_millis(150);
const PAR_DEFAUT: &str = "Par défaut";

#[derive(Clone)]
pub struct OutilsVocaux {
    pub session: Arc<SessionVocale>,
    pub audio: Arc<dyn SystemeAudio>,
    pub raccourcis: Arc<dyn RaccourcisGlobaux>,
    pub apm: Option<Arc<Apm>>,
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
}

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
    ombres: Ombres,
}

#[derive(Clone)]
pub struct PontVocal {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    memoire: Option<Memoire>,
    horloge: HorlogeMs,
}

pub fn installer_modeles(f: &Principale) {
    let v = f.global::<DonneesVocal>();
    v.set_tuiles(ModelRc::new(VecModel::<ParticipantVocal>::default()));
    v.set_participants(ModelRc::new(VecModel::<ParticipantVocal>::default()));
    v.set_micros(ModelRc::new(VecModel::<PeripheriqueVocal>::default()));
    v.set_sorties(ModelRc::new(VecModel::<PeripheriqueVocal>::default()));
}

pub fn vider_modeles(f: &Principale) {
    let v = f.global::<DonneesVocal>();
    vec_de(&v.get_tuiles()).set_vec(Vec::new());
    vec_de(&v.get_participants()).set_vec(Vec::new());
    v.set_vue(VueVocal::default());
    v.set_barre(BarreVocal::default());
    v.set_menu_ouvert(false);
    v.set_reglages_ouverts(false);
    v.set_liste(ListePeripheriques::Aucune);
    v.set_niveau(0.0);
    v.set_voix(false);
    v.set_parle_micro_coupe(false);
}

fn participant_slint(p: &Participant) -> ParticipantVocal {
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
    }
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
                ombres: Ombres::default(),
            })),
            store,
            fenetre,
            runtime,
            memoire,
            horloge,
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
        let (outils, ptt, essai) = {
            let mut i = self.verrou();
            (i.outils.clone(), i.ptt.take(), i.essai.take())
        };
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
        let Some(outils) = self.outils() else {
            return;
        };
        let sortie = self.preferences(|p| p.sortie_audio.clone());
        if let Ok((lecture, duree)) = tonalite::jouer(outils.audio.clone(), sortie, son) {
            self.runtime.spawn(async move {
                tokio::time::sleep(duree + MARGE_SON).await;
                drop(lecture);
            });
        }
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
        self.store.lire(|etat| {
            let moi = etat.moi().to_string();
            let tuiles = salon_vue
                .as_deref()
                .map(|s| projection::participants(etat, &i.etat, s, &moi, je_parle))
                .unwrap_or_default();
            let vue = salon_vue
                .as_deref()
                .map(|s| projection::vue(etat, &i.etat, s, tuiles.len()));
            synchroniser(
                vec_de(&v.get_tuiles()),
                &mut i.ombres.tuiles,
                tuiles,
                |p| p.utilisateur.clone(),
                participant_slint,
            );
            if i.ombres.vue != vue {
                v.set_vue(vue.as_ref().map(vue_slint).unwrap_or_default());
                i.ombres.vue = vue;
            }
            let participants = serveur
                .as_deref()
                .map(|s| projection::participants_du_serveur(etat, &i.etat, s, &moi, je_parle))
                .unwrap_or_default();
            synchroniser(
                vec_de(&v.get_participants()),
                &mut i.ombres.participants,
                participants,
                |p| (p.salon.clone(), p.utilisateur.clone()),
                participant_slint,
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
                match projection::menu(etat, &i.etat, salon, utilisateur, &moi) {
                    Some(m) => v.set_menu(menu_slint(&m)),
                    None => {
                        i.menu = None;
                        v.set_menu_ouvert(false);
                    }
                }
            }
        });
        self.projeter_niveau(f, i);
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
        let (ptt_etat, essai, capture_erreur, global_disponible) = {
            let i = self.verrou();
            (
                i.ptt_etat.clone(),
                i.essai.is_some(),
                i.capture_erreur.clone(),
                i.ptt.as_ref().is_some_and(|p| p.global_disponible()),
            )
        };
        let (traitement, ecoute, sons, ptt) = self.preferences(|p| {
            (
                p.traitement_micro,
                p.ecoute.clone(),
                p.sons_vocal,
                p.ptt.clone(),
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
        });
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
        self.verrou().menu = Some((utilisateur.to_string(), salon.to_string()));
        let v = f.global::<DonneesVocal>();
        v.set_menu(menu_slint(&menu));
        v.set_menu_ouvert(true);
    }

    fn menu_courant(&self) -> Option<(String, String)> {
        self.verrou().menu.clone()
    }

    pub fn muet_pour_moi(&self, f: &Principale, muet: bool) {
        let Some((utilisateur, _)) = self.menu_courant() else {
            return;
        };
        if let Some(session) = self.session() {
            session.definir_muet(&utilisateur, SourceAudio::Micro, muet);
        }
        self.retenir(|p| {
            p.ecoute
                .definir_muet(&utilisateur, SourceAudio::Micro, muet)
        });
        let mut menu = f.global::<DonneesVocal>().get_menu();
        menu.muet = muet;
        f.global::<DonneesVocal>().set_menu(menu);
    }

    pub fn volume(&self, pourcent: i32, fini: bool) {
        let Some((utilisateur, _)) = self.menu_courant() else {
            return;
        };
        let pourcent = u16::try_from(pourcent.max(0)).unwrap_or(u16::MAX);
        if let Some(session) = self.session() {
            session.definir_volume(&utilisateur, SourceAudio::Micro, pourcent);
        }
        if fini {
            self.retenir(|p| {
                p.ecoute
                    .definir_volume(&utilisateur, SourceAudio::Micro, pourcent)
            });
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

    pub fn ouvrir_reglages(&self, f: &Principale) {
        self.rafraichir_peripheriques(f);
        self.projeter_reglages(f);
        f.global::<DonneesVocal>().set_reglages_ouverts(true);
    }

    pub fn fermer_reglages(&self, f: &Principale) {
        self.arreter_essai(f);
        self.verrou().capture_erreur.clear();
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
        v.on_ouvrir_vocal(move |salon| {
            if let Some(f) = faible.upgrade() {
                ouvrir(&f, &salon);
            }
        });
    }
    rappel!(on_quitter, |p, _f| p.quitter());
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
    rappel!(on_muet_pour_moi, |p, f, muet| p.muet_pour_moi(&f, muet));
    rappel!(on_volume, |p, _f, valeur, fini| p.volume(valeur, fini));
    rappel!(on_moderer, |p, _f, micro, son| p.moderer(micro, son));
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
    fn une_bascule_de_peripherique_devient_un_avis() {
        assert_eq!(
            texte_bascule(SensAudio::Entree, &Bascule::Repli("x".into())).as_deref(),
            Some("Micro débranché : retour au micro par défaut.")
        );
        assert!(texte_bascule(SensAudio::Sortie, &Bascule::Choix("x".into())).is_none());
    }
}
