pub mod reglages;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use estompe_client::jeux::{self, JeuDetecte, Publication, ReglagesJeux, SuiviActivite};
use estompe_client::store::Store;
use estompe_plateforme::{ApplicationPremierPlan, PremierPlan};
use tokio::sync::{watch, Notify};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatJeux {
    pub disponible: bool,
    pub premier_plan: Option<JeuDetecte>,
    pub activite: String,
    pub partager: bool,
    pub derniere_application: Option<ApplicationPremierPlan>,
}

pub type LectureReglages = Arc<dyn Fn() -> ReglagesJeux + Send + Sync>;

pub struct Sondeur {
    premier_plan: Arc<dyn PremierPlan>,
    suivi: SuiviActivite,
    exclus: Option<Vec<String>>,
    derniere: Option<ApplicationPremierPlan>,
    processus: u32,
}

impl Sondeur {
    pub fn nouveau(premier_plan: Arc<dyn PremierPlan>) -> Self {
        Self {
            premier_plan,
            suivi: SuiviActivite::default(),
            exclus: None,
            derniere: None,
            processus: std::process::id(),
        }
    }

    pub fn sonder(&mut self, reglages: &ReglagesJeux, maintenant: Instant) -> EtatJeux {
        if self.exclus.as_ref() != Some(&reglages.exclus) {
            self.suivi.oublier();
            self.exclus = Some(reglages.exclus.clone());
        }
        let application = self.premier_plan.application().ok().flatten();
        if let Some(a) = application
            .as_ref()
            .filter(|a| a.processus != self.processus && !jeux::hors_jeu(&a.executable))
        {
            self.derniere = Some(a.clone());
        }
        let premier_plan = self.premier_plan.clone();
        let detection = jeux::detecter(
            application,
            || premier_plan.fenetres().unwrap_or_default(),
            reglages,
        );
        EtatJeux {
            disponible: true,
            activite: self
                .suivi
                .observer(detection.en_cours.as_deref(), maintenant),
            premier_plan: detection.premier_plan,
            partager: reglages.partager,
            derniere_application: self.derniere.clone(),
        }
    }
}

pub struct ServiceJeux {
    etat: Arc<watch::Sender<EtatJeux>>,
    relance: Arc<Notify>,
    tache: Option<tokio::task::JoinHandle<()>>,
}

impl ServiceJeux {
    pub fn demarrer(
        runtime: &tokio::runtime::Handle,
        premier_plan: Arc<dyn PremierPlan>,
        reglages: LectureReglages,
        periode: Duration,
    ) -> Self {
        let disponible = premier_plan.disponible();
        let etat = Arc::new(
            watch::channel(EtatJeux {
                disponible,
                partager: reglages().partager,
                ..EtatJeux::default()
            })
            .0,
        );
        let relance = Arc::new(Notify::new());
        let tache = disponible.then(|| {
            let (etat, relance) = (etat.clone(), relance.clone());
            runtime.spawn(async move {
                let mut sondeur = Sondeur::nouveau(premier_plan);
                loop {
                    let nouvel = sondeur.sonder(&reglages(), Instant::now());
                    etat.send_if_modified(|e| {
                        let change = *e != nouvel;
                        *e = nouvel;
                        change
                    });
                    tokio::select! {
                        _ = tokio::time::sleep(periode) => {}
                        _ = relance.notified() => {}
                    }
                }
            })
        });
        Self {
            etat,
            relance,
            tache,
        }
    }

    pub fn indisponible() -> Self {
        Self {
            etat: Arc::new(watch::channel(EtatJeux::default()).0),
            relance: Arc::new(Notify::new()),
            tache: None,
        }
    }

    pub fn disponible(&self) -> bool {
        self.etat.borrow().disponible
    }

    pub fn abonner(&self) -> watch::Receiver<EtatJeux> {
        self.etat.subscribe()
    }

    pub fn actuel(&self) -> EtatJeux {
        self.etat.borrow().clone()
    }

    pub fn relancer(&self) {
        self.relance.notify_one();
    }
}

impl Drop for ServiceJeux {
    fn drop(&mut self) {
        if let Some(tache) = self.tache.take() {
            tache.abort();
        }
    }
}

#[derive(Clone, Default)]
pub struct Annonce {
    publication: Arc<Mutex<Publication>>,
}

impl Annonce {
    pub fn appliquer(
        &self,
        store: &Store,
        runtime: &tokio::runtime::Handle,
        connecte: bool,
        etat: &EtatJeux,
    ) {
        if !etat.disponible {
            return;
        }
        store.definir_activite_locale(&etat.activite);
        let Some(nom) = self
            .publication
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .a_envoyer(connecte, &etat.activite, etat.partager)
        else {
            return;
        };
        let store = store.clone();
        let publication = self.publication.clone();
        runtime.spawn(async move {
            if store.publier_activite(&nom).await.is_err() {
                publication
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .echec(&nom);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_plateforme::{application_de_chemin, Indisponible, PremierPlanFactice};

    fn jeu(chemin: &str) -> ApplicationPremierPlan {
        ApplicationPremierPlan {
            processus: 4242,
            plein_ecran: true,
            ..application_de_chemin(chemin)
        }
    }

    #[test]
    fn le_sondeur_retient_le_jeu_et_la_derniere_application() {
        let factice = PremierPlanFactice::nouveau();
        let mut sondeur = Sondeur::nouveau(Arc::new(factice.clone()));
        let t0 = Instant::now();
        let reglages = ReglagesJeux::default();
        factice.mettre_au_premier_plan(Some(jeu(r"C:\Games\cs2.exe")));
        let etat = sondeur.sonder(&reglages, t0);
        assert_eq!(etat.activite, "Counter-Strike 2");
        assert_eq!(
            etat.premier_plan.as_ref().map(|j| j.nom.as_str()),
            Some("Counter-Strike 2")
        );
        assert_eq!(factice.enumerations(), 0);
        factice.mettre_au_premier_plan(Some(ApplicationPremierPlan {
            processus: std::process::id(),
            ..jeu(r"C:\Estompe\estompe-natif.exe")
        }));
        let etat = sondeur.sonder(&reglages, t0 + Duration::from_secs(5));
        assert_eq!(etat.premier_plan, None);
        assert_eq!(etat.activite, "Counter-Strike 2");
        assert_eq!(
            etat.derniere_application.map(|a| a.executable),
            Some("cs2.exe".to_string())
        );
        assert_eq!(factice.enumerations(), 1);
        let etat = sondeur.sonder(&reglages, t0 + Duration::from_secs(70));
        assert_eq!(etat.activite, "");
    }

    #[test]
    fn exclure_le_jeu_en_cours_l_efface_tout_de_suite() {
        let factice = PremierPlanFactice::nouveau();
        let mut sondeur = Sondeur::nouveau(Arc::new(factice.clone()));
        let t0 = Instant::now();
        let mut reglages = ReglagesJeux::default();
        factice.mettre_au_premier_plan(Some(jeu(r"C:\Games\cs2.exe")));
        assert_eq!(sondeur.sonder(&reglages, t0).activite, "Counter-Strike 2");
        reglages.exclure("CS2.exe");
        let etat = sondeur.sonder(&reglages, t0 + Duration::from_secs(1));
        assert_eq!((etat.activite.as_str(), etat.premier_plan), ("", None));
    }

    #[test]
    fn le_service_publie_l_etat_et_repond_a_la_relance() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let factice = PremierPlanFactice::nouveau();
        let reglages = Arc::new(Mutex::new(ReglagesJeux::default()));
        let lecture = reglages.clone();
        let service = ServiceJeux::demarrer(
            runtime.handle(),
            Arc::new(factice.clone()),
            Arc::new(move || lecture.lock().unwrap().clone()),
            Duration::from_secs(3600),
        );
        assert!(service.disponible());
        let mut etat = service.abonner();
        factice.mettre_au_premier_plan(Some(jeu(r"D:\Jeux\perso.exe")));
        reglages.lock().unwrap().ajouter(r"D:\Jeux\perso.exe");
        runtime.block_on(async {
            etat.changed().await.unwrap();
            assert_eq!(etat.borrow_and_update().activite, "perso");
            reglages.lock().unwrap().partager = true;
            service.relancer();
            etat.changed().await.unwrap();
            let e = etat.borrow_and_update().clone();
            assert_eq!((e.activite.as_str(), e.partager), ("perso", true));
        });
        assert_eq!(service.actuel().activite, "perso");
    }

    #[test]
    fn sans_detection_le_service_ne_sonde_pas() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let service = ServiceJeux::demarrer(
            runtime.handle(),
            Arc::new(Indisponible),
            Arc::new(ReglagesJeux::default),
            Duration::from_millis(1),
        );
        assert!(!service.disponible());
        assert!(service.tache.is_none());
        assert!(!ServiceJeux::indisponible().disponible());
    }
}
