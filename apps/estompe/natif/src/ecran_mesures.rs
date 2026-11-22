use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::mesures::{duree_scenario, maintenant_ms, ExportMesures};
use estompe_plateforme::MesureProcessus;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::mesures::{ecrire_export, SessionMesures, ECRAN_SCENARIOS};
use crate::{DonneesMesures, LigneScenario, Principale};

pub struct EcranMesures {
    source: Arc<dyn MesureProcessus>,
    dossier: PathBuf,
    etat: Mutex<Etat>,
}

#[derive(Default)]
struct Etat {
    session: Option<SessionMesures>,
    dernier: Option<ExportMesures>,
    message: String,
    erreur: String,
}

impl EcranMesures {
    pub fn nouveau(source: Arc<dyn MesureProcessus>, dossier: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            source,
            dossier,
            etat: Mutex::default(),
        })
    }

    fn etat(&self) -> MutexGuard<'_, Etat> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn demarrer(&self) {
        let mut e = self.etat();
        if e.session.is_some() {
            return;
        }
        e.message.clear();
        e.erreur.clear();
        match SessionMesures::demarrer(self.source.clone()) {
            Ok(s) => e.session = Some(s),
            Err(m) => e.erreur = m,
        }
    }

    pub fn arreter(&self) {
        let mut e = self.etat();
        if let Some(session) = e.session.take() {
            e.dernier = Some(session.arreter(None));
            e.message = "Session terminée : exporte les mesures pour les partager.".into();
        }
    }

    pub fn lancer(&self, id: &str) {
        let mut e = self.etat();
        let resultat = match e.session.as_mut() {
            Some(s) => s.lancer(id, maintenant_ms()),
            None => return,
        };
        e.erreur = resultat.err().unwrap_or_default();
    }

    pub fn terminer_scenario(&self) {
        if let Some(s) = self.etat().session.as_mut() {
            s.terminer_scenario(maintenant_ms());
        }
    }

    pub fn exporter(&self) {
        let mut e = self.etat();
        let Some(export) = e.dernier.as_ref() else {
            return;
        };
        match ecrire_export(export, &self.dossier) {
            Ok(f) => {
                e.message = format!("Mesures exportées : {}", f.display());
                e.erreur.clear();
            }
            Err(m) => e.erreur = format!("Export impossible : {m}"),
        }
    }

    pub fn tic(&self) {
        let mut e = self.etat();
        if let Some(s) = e.session.as_mut() {
            if s.reste_ms(maintenant_ms()) == Some(0) {
                s.terminer_scenario(maintenant_ms());
            }
        }
    }

    pub fn afficher(&self, f: &Principale) {
        let e = self.etat();
        let g = f.global::<DonneesMesures>();
        let session = e.session.as_ref();
        g.set_disponible(self.source.disponible());
        g.set_en_session(session.is_some());
        g.set_peut_exporter(e.dernier.is_some());
        g.set_message(SharedString::from(e.message.as_str()));
        g.set_erreur(SharedString::from(e.erreur.as_str()));
        let faits = session.map(|s| s.faits()).unwrap_or_default();
        g.set_scenarios(ModelRc::new(VecModel::from(
            ECRAN_SCENARIOS
                .iter()
                .map(|(id, titre, aide)| LigneScenario {
                    id: (*id).into(),
                    titre: (*titre).into(),
                    aide: (*aide).into(),
                    fait: faits.contains(id),
                })
                .collect::<Vec<_>>(),
        )));
        let en_cours = session.and_then(|s| s.scenario().map(|(id, _)| id.to_string()));
        let ligne = en_cours
            .as_deref()
            .and_then(|id| ECRAN_SCENARIOS.iter().find(|(i, _, _)| *i == id));
        g.set_scenario_en_cours(ligne.map_or("", |l| l.1).into());
        g.set_aide_en_cours(ligne.map_or("", |l| l.2).into());
        let reste = session
            .and_then(|s| s.reste_ms(maintenant_ms()))
            .unwrap_or(0);
        g.set_reste(reste.div_ceil(1000) as i32);
        debug_assert!(en_cours
            .as_deref()
            .is_none_or(|id| duree_scenario(id).is_some()));
    }
}

pub fn brancher(f: &Principale, ecran: Arc<EcranMesures>) -> Timer {
    let g = f.global::<DonneesMesures>();
    macro_rules! rappel {
        ($methode:ident, |$e:ident $(, $arg:ident)*| $corps:expr) => {{
            let $e = ecran.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                $corps;
                if let Some(f) = faible.upgrade() {
                    $e.afficher(&f);
                }
            });
        }};
    }
    rappel!(on_demarrer, |e| e.demarrer());
    rappel!(on_arreter, |e| e.arreter());
    rappel!(on_lancer, |e, id| e.lancer(id.as_str()));
    rappel!(on_terminer_scenario, |e| e.terminer_scenario());
    rappel!(on_exporter, |e| e.exporter());
    {
        let e = ecran.clone();
        let faible = f.as_weak();
        g.on_ouvrir(move || {
            if let Some(f) = faible.upgrade() {
                e.afficher(&f);
                f.global::<DonneesMesures>().set_ouvert(true);
            }
        });
    }
    g.on_fermer({
        let faible = f.as_weak();
        move || {
            if let Some(f) = faible.upgrade() {
                f.global::<DonneesMesures>().set_ouvert(false);
            }
        }
    });
    let minuterie = Timer::default();
    let faible = f.as_weak();
    minuterie.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(f) = faible.upgrade() {
            if f.global::<DonneesMesures>().get_ouvert() {
                ecran.tic();
                ecran.afficher(&f);
            } else {
                ecran.tic();
            }
        }
    });
    minuterie
}
