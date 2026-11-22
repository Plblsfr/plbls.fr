mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use commun::coquille::{FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::preferences::Preferences;
use estompe_client::temps_reel::EtatReseau;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::jeux::{self, ServiceJeux};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesJeux, DonneesMenu, Page, Principale,
};
use estompe_plateforme::{application_de_chemin, ApplicationPremierPlan, PremierPlanFactice};
use serde_json::Value;
use slint::{ComponentHandle, Model};

fn activites_publiees(serveur: &ServeurMessages) -> Vec<String> {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .filter(|(r, _)| r == "PUT /me/activity")
        .map(|(_, c)| {
            serde_json::from_str::<Value>(c).unwrap()["name"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn application(chemin: &str, plein_ecran: bool) -> ApplicationPremierPlan {
    ApplicationPremierPlan {
        processus: 4242,
        plein_ecran,
        ..application_de_chemin(chemin)
    }
}

fn statut(f: &Principale) -> String {
    f.global::<DonneesCoquille>().get_statut().to_string()
}

fn exclus(f: &Principale) -> Vec<String> {
    f.global::<DonneesJeux>()
        .get_exclus()
        .iter()
        .map(|e| e.to_string())
        .collect()
}

fn ajoutes(f: &Principale) -> Vec<String> {
    f.global::<DonneesJeux>()
        .get_ajoutes()
        .iter()
        .map(|e| e.chemin.to_string())
        .collect()
}

struct Faux {
    fabrique: Arc<FausseFabrique>,
    serveur: Arc<ServeurMessages>,
    detection: PremierPlanFactice,
    memoire: Memoire,
}

fn scenario(faible: slint::Weak<Principale>, faux: Faux) {
    let Faux {
        fabrique,
        serveur,
        detection,
        memoire,
    } = faux;
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille,
        "coquille ouverte",
    );
    detection.mettre_au_premier_plan(Some(application(r"C:\Games\cs2.exe", true)));
    attendre_l_interface(
        &faible,
        |f| statut(f) == "Counter-Strike 2",
        "le jeu détecté s'affiche sous mon pseudo",
    );
    assert!(
        activites_publiees(&serveur).is_empty(),
        "hors ligne : rien n'est publié"
    );
    fabrique.reseau.send(EtatReseau::Connecte).unwrap();
    attendre(
        || activites_publiees(&serveur) == [""],
        "sans le réglage, l'activité publiée reste vide",
    );

    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_menu_moi(10.0, 500.0, 520.0);
        let entrees: Vec<String> = f
            .global::<DonneesMenu>()
            .get_entrees()
            .iter()
            .map(|e| e.id.to_string())
            .collect();
        assert!(entrees.contains(&"jeux".to_string()), "{entrees:?}");
        estompe_natif::menu::choisir(f, "jeux");
    });
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesJeux>().get_ouvert(),
        "« Jeux et activité » ouvre les réglages",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesJeux>().invoke_definir_partage(true)
    });
    attendre(
        || activites_publiees(&serveur) == ["", "Counter-Strike 2"],
        "le réglage publie le jeu en cours",
    );
    assert!(memoire.lire(|p| p.jeux.partager));

    sur_l_interface(&faible, |f| {
        f.global::<DonneesJeux>().invoke_exclure("CS2.exe".into())
    });
    attendre(
        || activites_publiees(&serveur) == ["", "Counter-Strike 2", ""],
        "un jeu exclu n'est plus publié",
    );
    attendre_l_interface(
        &faible,
        |f| statut(f) != "Counter-Strike 2" && exclus(f) == ["CS2.exe"],
        "un jeu exclu n'est plus affiché",
    );

    detection.mettre_au_premier_plan(Some(application(r"D:\Jeux\perso.exe", false)));
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesJeux>().get_derniere_application() == "perso.exe",
        "l'application au premier plan est proposée",
    );
    assert_eq!(
        activites_publiees(&serveur).len(),
        3,
        "une application fenêtrée n'est pas un jeu"
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesJeux>().invoke_ajouter_derniere()
    });
    attendre(
        || activites_publiees(&serveur).last().map(String::as_str) == Some("perso"),
        "le jeu ajouté à la main est détecté et publié",
    );
    attendre_l_interface(
        &faible,
        |f| {
            ajoutes(f) == [r"D:\Jeux\perso.exe"]
                && f.global::<DonneesJeux>().get_jeu_en_cours() == "perso"
        },
        "la liste des jeux ajoutés suit",
    );

    fabrique
        .fichiers
        .choix
        .lock()
        .unwrap()
        .push(r"C:\Outils\Launcher.exe".into());
    sur_l_interface(&faible, |f| {
        f.global::<DonneesJeux>().invoke_parcourir_exclu()
    });
    attendre_l_interface(
        &faible,
        |f| exclus(f) == ["CS2.exe", "Launcher.exe"],
        "« Parcourir… » exclut l'exécutable choisi par son nom",
    );
    sur_l_interface(&faible, |f| {
        let g = f.global::<DonneesJeux>();
        g.invoke_retirer_exclu("CS2.exe".into());
        g.invoke_retirer_ajoute(r"D:\Jeux\perso.exe".into());
    });
    assert_eq!(
        memoire.lire(|p| (p.jeux.ajoutes.clone(), p.jeux.exclus.clone())),
        (Vec::<String>::new(), vec!["Launcher.exe".to_string()])
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesJeux>().invoke_fermer();
        assert!(!f.global::<DonneesJeux>().get_ouvert());
    });
    let _ = slint::quit_event_loop();
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
fn c2_le_jeu_detecte_est_publie_selon_le_reglage_et_un_jeu_exclu_ne_l_est_jamais() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None);
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(
            FausseAuth::avec_session(true),
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let serveur = ServeurMessages::demarrer(10);
    let fabrique = FausseFabrique::sur(commun::coquille::etat(), vec![], &serveur.adresse);
    let detection = PremierPlanFactice::nouveau();
    let lecture = memoire.clone();
    let service = Arc::new(ServiceJeux::demarrer(
        &entree.runtime(),
        Arc::new(detection.clone()),
        Arc::new(move || lecture.lire(|p| p.jeux.clone())),
        Duration::from_millis(20),
    ));
    *fabrique.jeux.lock().unwrap() = Some(service.abonner());
    jeux::reglages::brancher(
        &fenetre,
        service.clone(),
        memoire.clone(),
        fabrique.fichiers.clone(),
        &entree.runtime(),
    );
    assert!(jeux::reglages::disponible(&fenetre));
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
    fenetre.show().unwrap();
    let faux = Faux {
        fabrique,
        serveur,
        detection,
        memoire,
    };
    let faible = fenetre.as_weak();
    piloter(move || scenario(faible, faux));
    drop(coquille);
    drop(entree);
}
