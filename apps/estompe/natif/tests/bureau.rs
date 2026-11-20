mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use commun::bureau::{EtatZone, FausseZone};
use commun::coquille::{evenement, message, FausseFabrique};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::preferences::Preferences;
use estompe_natif::bureau::{Bureau, EtapeFermeture, OptionsBureau, DELAI_ARRET};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Evenement, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, ecrans, DonneesCoquille, Page, Principale};
use estompe_plateforme::instance::{MessageInstance, RecepteurInstance};
use estompe_plateforme::{EvenementZone, Indisponible};
use slint::platform::WindowEvent;
use slint::{ComponentHandle, Model};

fn visible(faible: &slint::Weak<Principale>) -> bool {
    sur_l_interface(faible, |f| f.window().is_visible())
}

fn demander_la_fermeture(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        f.window().dispatch_event(WindowEvent::CloseRequested);
    });
}

fn flux_ferme(fabrique: &FausseFabrique) -> bool {
    fabrique
        .signaux
        .lock()
        .unwrap()
        .as_ref()
        .is_none_or(|s| s.is_closed())
}

fn non_lus(f: &Principale, serveur: &str) -> i32 {
    f.global::<DonneesCoquille>()
        .get_serveurs()
        .iter()
        .find(|l| l.id == serveur)
        .map(|l| l.non_lus)
        .unwrap_or(-1)
}

fn scenario_zone_presente(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    zone: Arc<EtatZone>,
    recepteur: RecepteurInstance,
    recus: Arc<Mutex<Vec<Evenement>>>,
) {
    attendre_l_interface(
        &faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.global::<DonneesCoquille>().get_serveurs().row_count() == 4
        },
        "coquille ouverte",
    );
    assert!(zone.installee(), "icône posée dans la zone");
    assert_eq!(zone.icones.lock().unwrap()[0].largeur, 64);

    sur_l_interface(&faible, |f| f.hide().unwrap());
    assert!(!visible(&faible));
    recepteur(MessageInstance {
        arguments: vec!["--invitation".into(), "ABCDE-23456".into()],
        jeton_activation: None,
    });
    attendre(|| visible(&faible), "le second lancement ramène la fenêtre");
    attendre(
        || {
            recus
                .lock()
                .unwrap()
                .contains(&Evenement::Rejoindre("ABCDE-23456".into()))
        },
        "invitation transmise à l'entrée",
    );

    demander_la_fermeture(&faible);
    attendre(|| !visible(&faible), "fermer masque la fenêtre");
    assert!(
        !flux_ferme(&fabrique),
        "la connexion temps réel reste ouverte"
    );
    assert_eq!(sur_l_interface(&faible, |f| f.get_page()), Page::Coquille);
    fabrique.envoyer(evenement("message.created", message("90", "t1", "carol")));
    attendre_l_interface(
        &faible,
        |f| non_lus(f, "s2") == 1,
        "un message reçu fenêtre masquée arrive encore",
    );

    zone.declencher(EvenementZone::Clic);
    attendre(|| visible(&faible), "un clic sur l'icône ramène la fenêtre");
    demander_la_fermeture(&faible);
    attendre(|| !visible(&faible), "masquée à nouveau");
    zone.declencher(EvenementZone::Ouvrir);
    attendre(|| visible(&faible), "« Ouvrir » ramène la fenêtre");

    zone.declencher(EvenementZone::Quitter);
}

fn scenario_zone_absente(faible: slint::Weak<Principale>) {
    attendre(|| visible(&faible), "fenêtre affichée");
    demander_la_fermeture(&faible);
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

#[test]
fn c3_fermer_garde_la_connexion_et_quitter_la_ferme() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None);
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let auth = FausseAuth::avec_session(true);
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(
            auth,
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let fabrique = FausseFabrique::nouvelle(commun::coquille::etat(), vec![]);
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let coquille = Coquille::brancher(
        &fenetre,
        &entree,
        fabrique.clone(),
        memoire.clone(),
        Arc::new(move || midi),
    );
    let (fausse_zone, zone) = FausseZone::nouvelle(true);
    let recus = Arc::new(Mutex::new(Vec::new()));
    let journal = recus.clone();
    let bureau = Bureau::installer(
        &fenetre,
        OptionsBureau {
            zone: fausse_zone,
            demarrage: Box::new(Indisponible),
            memoire: memoire.clone(),
            entree: Arc::new(move |e| journal.lock().unwrap().push(e)),
            journal: None,
        },
    );
    let arret = coquille.arret();
    bureau
        .fermeture()
        .avant_de_quitter(EtapeFermeture::Coquille, move || {
            assert!(arret.arreter(DELAI_ARRET), "coquille arrêtée à temps");
        });
    assert!(!fenetre.get_demarrage_disponible());
    let recepteur = bureau.recepteur_instance();
    fenetre.show().unwrap();
    let (faible, f, z) = (fenetre.as_weak(), fabrique.clone(), zone.clone());
    piloter(move || scenario_zone_presente(faible, f, z, recepteur, recus));
    assert!(bureau.fermeture().quittee(), "« Quitter » ferme vraiment");
    assert!(zone.retiree.load(Ordering::SeqCst), "icône retirée");
    attendre(
        || flux_ferme(&fabrique),
        "« Quitter » ferme la connexion temps réel",
    );
    assert!(!fenetre.window().is_visible());
    drop(coquille);
    drop(entree);

    let seconde = creer_fenetre().unwrap();
    let (zone_absente, etat_absente) = FausseZone::nouvelle(false);
    let bureau_sans_zone = Bureau::installer(
        &seconde,
        OptionsBureau {
            zone: zone_absente,
            demarrage: Box::new(Indisponible),
            memoire,
            entree: Arc::new(|_| {}),
            journal: None,
        },
    );
    seconde.show().unwrap();
    let faible = seconde.as_weak();
    piloter(move || scenario_zone_absente(faible));
    assert!(
        bureau_sans_zone.fermeture().quittee(),
        "sans zone de notification, fermer quitte"
    );
    assert!(etat_absente.retiree.load(Ordering::SeqCst));
}
