use crate::commun;

use std::collections::VecDeque;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use commun::bureau::FausseZone;
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::mise_a_jour::{
    self as maj, ErreurMiseAJour, Futur, MiseAJour, Progres, SourceMaj,
};
use estompe_client::preferences::Preferences;
use estompe_client::version::MiseAJourDisponible;
use estompe_natif::bureau::{Bureau, OptionsBureau};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::mise_a_jour::{self, OptionsService};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, ecrans, DonneesMiseAJour, Page, Principale};
use estompe_plateforme::{Erreur, Indisponible, Installation, Resultat};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::PointerEventButton;
use slint::ComponentHandle;
use tokio::sync::oneshot;

struct SourceFactice {
    disponible: Mutex<Result<Option<MiseAJourDisponible>, ErreurMiseAJour>>,
    echecs: Mutex<VecDeque<ErreurMiseAJour>>,
    retenue: Mutex<Option<oneshot::Receiver<()>>>,
    dossier: PathBuf,
    telechargements: AtomicUsize,
}

impl SourceFactice {
    fn nouvelle(dossier: &Path) -> Arc<Self> {
        Arc::new(Self {
            disponible: Mutex::new(Ok(None)),
            echecs: Mutex::new(VecDeque::new()),
            retenue: Mutex::new(None),
            dossier: dossier.to_path_buf(),
            telechargements: AtomicUsize::new(0),
        })
    }

    fn proposer(&self, version: &str, imposee: bool) {
        *self.disponible.lock().unwrap() = Ok(Some(MiseAJourDisponible {
            version: version.into(),
            notes: format!("Notes de la version {version}."),
            message: if imposee {
                "Faille corrigée.".into()
            } else {
                String::new()
            },
            imposee,
            ..MiseAJourDisponible::default()
        }));
    }

    fn retenir(&self) -> oneshot::Sender<()> {
        let (envoi, reception) = oneshot::channel();
        *self.retenue.lock().unwrap() = Some(reception);
        envoi
    }
}

impl SourceMaj for SourceFactice {
    fn verifier(&self) -> Futur<Result<Option<MiseAJourDisponible>, ErreurMiseAJour>> {
        let resultat = self.disponible.lock().unwrap().clone();
        Box::pin(async move { resultat })
    }

    fn telecharger(
        &self,
        mise_a_jour: MiseAJourDisponible,
        progres: Progres,
    ) -> Futur<Result<PathBuf, ErreurMiseAJour>> {
        self.telechargements.fetch_add(1, Ordering::SeqCst);
        let retenue = self.retenue.lock().unwrap().take();
        let echec = self.echecs.lock().unwrap().pop_front();
        let chemin = self.dossier.join(maj::nom_de_fichier(&mise_a_jour.version));
        Box::pin(async move {
            progres(40, 100);
            if let Some(retenue) = retenue {
                let _ = retenue.await;
            }
            if let Some(echec) = echec {
                return Err(echec);
            }
            std::fs::write(&chemin, b"installeur")
                .map_err(|e| ErreurMiseAJour::Disque(e.to_string()))?;
            progres(100, 100);
            Ok(chemin)
        })
    }

    fn controler(&self, _: &MiseAJourDisponible, installeur: &Path) -> Result<(), ErreurMiseAJour> {
        if installeur.exists() {
            Ok(())
        } else {
            Err(ErreurMiseAJour::Signature("installeur absent".into()))
        }
    }
}

#[derive(Default)]
struct FausseInstallation {
    lancements: Mutex<Vec<PathBuf>>,
}

impl FausseInstallation {
    fn nombre(&self) -> usize {
        self.lancements.lock().unwrap().len()
    }
}

impl Installation for FausseInstallation {
    fn disponible(&self) -> bool {
        true
    }
    fn cible(&self) -> Option<&'static str> {
        Some("windows-x86_64")
    }
    fn lancer(&self, installeur: &Path) -> Resultat<()> {
        if !installeur.exists() {
            return Err(Erreur::Echec("introuvable".into()));
        }
        self.lancements
            .lock()
            .unwrap()
            .push(installeur.to_path_buf());
        Ok(())
    }
}

fn element(f: &Principale, libelle: &str, role: Option<AccessibleRole>) -> Option<ElementHandle> {
    ElementHandle::find_by_accessible_label(f, libelle)
        .find(|e| role.is_none_or(|r| e.accessible_role() == Some(r)) && e.computed_opacity() > 0.0)
}

fn present(faible: &slint::Weak<Principale>, libelle: &'static str) -> bool {
    sur_l_interface(faible, move |f| element(f, libelle, None).is_some())
}

fn cliquer(faible: &slint::Weak<Principale>, libelle: &'static str) {
    sur_l_interface(faible, move |f| {
        let bouton = element(f, libelle, Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("bouton « {libelle} » introuvable"));
        assert_eq!(bouton.accessible_enabled(), Some(true), "{libelle}");
        bouton.mock_single_click(PointerEventButton::Left);
    });
}

fn piloter(scenario: impl FnOnce() + Send + 'static) {
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(scenario));
        if resultat.is_err() {
            let _ = slint::quit_event_loop();
        }
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    if let Err(panique) = pilote.join().unwrap() {
        resume_unwind(panique);
    }
}

struct Banc {
    fenetre: Principale,
    entree: Entree,
    coquille: Coquille,
    bureau: Bureau,
    service: MiseAJour,
    source: Arc<SourceFactice>,
    installation: Arc<FausseInstallation>,
    navigateur: Arc<FauxNavigateur>,
    _dossier: tempfile::TempDir,
}

fn banc() -> Banc {
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None);
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let navigateur = Arc::new(FauxNavigateur::default());
    let canal = entree::canal();
    let canal_maj = maj::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let mut services = commun::services(
        FausseAuth::avec_session(true),
        FausseApi::compte_actif(),
        navigateur.clone(),
    );
    services.installer = Some(mise_a_jour::installateur(canal_maj.0.clone()));
    let entree = Entree::demarrer(
        Machine::nouvelle(None).avec_installation(true),
        services,
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let coquille = Coquille::brancher(
        &fenetre,
        &entree,
        commun::coquille::FausseFabrique::nouvelle(commun::coquille::etat(), Vec::new()),
        memoire.clone(),
        estompe_natif::coquille::horloge_locale(),
    );
    let (zone, _) = FausseZone::nouvelle(true);
    let bureau = Bureau::installer(
        &fenetre,
        OptionsBureau {
            zone,
            pastille: Box::new(Indisponible),
            demarrage: Box::new(Indisponible),
            memoire,
            entree: Arc::new(|_| {}),
            journal: None,
            suivi_du_focus: None,
        },
    );
    let source = SourceFactice::nouvelle(dossier.path());
    let installation = Arc::new(FausseInstallation::default());
    let service = mise_a_jour::demarrer_service(
        &entree.runtime(),
        OptionsService {
            canal: canal_maj,
            source: source.clone(),
            installation: installation.clone(),
            fenetre: fenetre.as_weak(),
            entree: entree.emetteur(),
            redemarrer: mise_a_jour::redemarrage(),
            journal: None,
        },
    );
    mise_a_jour::brancher(&fenetre, &service, &entree.runtime());
    fenetre.show().unwrap();
    Banc {
        fenetre,
        entree,
        coquille,
        bureau,
        service,
        source,
        installation,
        navigateur,
        _dossier: dossier,
    }
}

const BANDEAU_02: &str = "Nouvelle version disponible : Estompe natif 0.2.0";
const BANDEAU_03: &str = "Nouvelle version disponible : Estompe natif 0.3.0";

fn scenario_bandeau(
    faible: slint::Weak<Principale>,
    service: MiseAJour,
    source: Arc<SourceFactice>,
    installation: Arc<FausseInstallation>,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille,
        "coquille affichée",
    );
    assert!(
        sur_l_interface(&faible, |f| f
            .global::<DonneesMiseAJour>()
            .get_automatique()),
        "mises à jour automatiques quand l'installation est disponible"
    );
    assert!(!present(&faible, BANDEAU_02));

    source.proposer("0.2.0", false);
    service.envoyer(maj::EvenementMaj::Verifier { manuelle: false });
    attendre(
        || present(&faible, BANDEAU_02),
        "bandeau de la version 0.2.0",
    );
    assert!(
        present(&faible, "Notes de la version 0.2.0."),
        "notes affichées"
    );

    cliquer(&faible, "Plus tard");
    attendre(
        || !present(&faible, BANDEAU_02),
        "« Plus tard » masque le bandeau",
    );
    service.envoyer(maj::EvenementMaj::Verifier { manuelle: false });
    attendre(
        || service.etat().verifications == 2,
        "seconde vérification de la même version",
    );
    assert!(
        !present(&faible, BANDEAU_02),
        "la version écartée reste masquée"
    );

    source.proposer("0.3.0", false);
    service.envoyer(maj::EvenementMaj::Verifier { manuelle: false });
    attendre(
        || present(&faible, BANDEAU_03),
        "une nouvelle version réaffiche le bandeau",
    );

    let feu = source.retenir();
    cliquer(&faible, "Mettre à jour");
    attendre_l_interface(
        &faible,
        |f| {
            let d = f.global::<DonneesMiseAJour>();
            d.get_en_cours() && (d.get_progression() - 0.4).abs() < 1e-6
        },
        "progression à 40 %",
    );
    assert!(
        present(&faible, "40 %"),
        "progression sur le bouton du bandeau"
    );
    assert_eq!(
        installation.nombre(),
        0,
        "rien n'est lancé avant la fin du téléchargement"
    );
    feu.send(()).unwrap();
    attendre(|| installation.nombre() == 1, "installeur lancé une fois");
}

fn scenario_erreur_et_ecran_force(
    faible: slint::Weak<Principale>,
    service: MiseAJour,
    source: Arc<SourceFactice>,
    installation: Arc<FausseInstallation>,
    navigateur: Arc<FauxNavigateur>,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille,
        "coquille affichée",
    );
    sur_l_interface(&faible, |f| {
        f.set_a_propos_ouvert(true);
        slint::platform::update_timers_and_animations();
    });
    *source.disponible.lock().unwrap() = Err(ErreurMiseAJour::Reseau("délai dépassé".into()));
    cliquer(&faible, "Rechercher une mise à jour");
    let message = ErreurMiseAJour::Reseau("délai dépassé".into()).to_string();
    let attendu = message.clone();
    attendre_l_interface(
        &faible,
        move |f| element(f, &attendu, None).is_some(),
        "erreur lisible dans le panneau",
    );
    sur_l_interface(&faible, |f| {
        f.set_a_propos_ouvert(false);
        slint::platform::update_timers_and_animations();
    });

    source.proposer("0.2.0", true);
    service.envoyer(maj::EvenementMaj::Verifier { manuelle: false });
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::MiseAJour,
        "mise à jour imposée : écran bloquant",
    );
    assert_eq!(
        sur_l_interface(&faible, |f| f.get_mise_a_jour_message()),
        "Faille corrigée."
    );

    source
        .echecs
        .lock()
        .unwrap()
        .push_back(ErreurMiseAJour::Reseau("connexion coupée".into()));
    cliquer(&faible, "Installer la mise à jour");
    attendre(
        || present(&faible, "Ouvrir la page de téléchargement"),
        "échec : le lien de téléchargement reste proposé",
    );
    assert!(
        navigateur.ouvertes.lock().unwrap().is_empty(),
        "aucun navigateur ouvert pour installer"
    );
    assert_eq!(installation.nombre(), 0);
    cliquer(&faible, "Ouvrir la page de téléchargement");
    attendre(
        || navigateur.ouvertes.lock().unwrap().len() == 1,
        "page de téléchargement ouverte sur demande",
    );
    cliquer(&faible, "Réessayer");
    attendre(
        || installation.nombre() == 1,
        "installeur lancé depuis l'écran forcé",
    );
    assert_eq!(source.telechargements.load(Ordering::SeqCst), 2);
}

#[test]
fn c2_bandeau_installation_redemarrage_erreur_et_ecran_force() {
    i_slint_backend_testing::init_integration_test_with_mock_time();

    let premier = banc();
    let faible = premier.fenetre.as_weak();
    let (service, source, installation) = (
        premier.service.clone(),
        premier.source.clone(),
        premier.installation.clone(),
    );
    piloter(move || scenario_bandeau(faible, service, source, installation));
    assert!(
        premier.bureau.fermeture().quittee(),
        "après le lancement de l'installeur, l'app quitte pour redémarrer"
    );
    assert_eq!(premier.installation.nombre(), 1);
    assert!(premier.navigateur.ouvertes.lock().unwrap().is_empty());
    drop(premier.coquille);
    drop(premier.entree);

    let second = banc();
    let faible = second.fenetre.as_weak();
    let (service, source, installation, navigateur) = (
        second.service.clone(),
        second.source.clone(),
        second.installation.clone(),
        second.navigateur.clone(),
    );
    piloter(move || {
        scenario_erreur_et_ecran_force(faible, service, source, installation, navigateur)
    });
    assert!(second.bureau.fermeture().quittee());
    assert_eq!(second.installation.nombre(), 1);
    drop(second.coquille);
    drop(second.entree);
}
