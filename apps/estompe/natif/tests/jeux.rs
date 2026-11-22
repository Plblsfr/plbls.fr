mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use commun::coquille::{FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::overlay::{CoinOverlay, QuandOverlay, ReglagesOverlay, TailleOverlay};
use estompe_client::overlay_messages::{GenresMessages, ReglagesMessages, ToastMessage};
use estompe_client::preferences::Preferences;
use estompe_client::store::CleConversation;
use estompe_client::temps_reel::EtatReseau;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::jeux::{self, ServiceJeux};
use estompe_natif::overlay::messages::{Options as OptionsMessages, ServiceOverlayMessages};
use estompe_natif::overlay::voix::{LigneVoix, ServiceOverlayVoix};
use estompe_natif::overlay::{self, MoteurOverlay};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesJeux, DonneesMenu, DonneesOverlay,
    DonneesOverlayMessages, Page, Principale,
};
use estompe_plateforme::{
    application_de_chemin, ApplicationPremierPlan, OverlayFactice, PremierPlanFactice, ZoneEcran,
};
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

const ECRAN_DU_JEU: ZoneEcran = ZoneEcran {
    x: 1920,
    y: 0,
    largeur: 1920,
    hauteur: 1080,
};

fn application(chemin: &str, plein_ecran: bool) -> ApplicationPremierPlan {
    ApplicationPremierPlan {
        processus: 4242,
        plein_ecran,
        ecran: Some(ECRAN_DU_JEU),
        ..application_de_chemin(chemin)
    }
}

fn overlay_affiche(overlay: &OverlayFactice) -> Option<(i32, i32, u8)> {
    overlay
        .calques()
        .first()
        .cloned()
        .flatten()
        .map(|a| (a.x, a.y, a.opacite))
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
    overlay: OverlayFactice,
    overlay_voix: Arc<ServiceOverlayVoix>,
    calque_messages: OverlayFactice,
    overlay_messages: Arc<ServiceOverlayMessages>,
}

fn scenario(faible: slint::Weak<Principale>, faux: Faux) {
    let Faux {
        fabrique,
        serveur,
        detection,
        memoire,
        overlay,
        overlay_voix,
        calque_messages,
        overlay_messages,
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
    overlay_voix.definir_salon(Some(vec![LigneVoix {
        id: "u1".into(),
        pseudo: "bob".into(),
        initiales: "BO".into(),
        avatar: None,
        parle: true,
        muet: false,
        sourd: false,
    }]));
    attendre(
        || overlay_affiche(&overlay) == Some((1920 + 16, 540 - 24, 255)),
        "c353 : l'overlay s'affiche au centre gauche de l'écran du jeu",
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
        let o = f.global::<DonneesOverlay>();
        assert!(o.get_disponible() && o.get_actif() && o.get_en_jeu());
        o.invoke_definir_coin("tr".into());
        o.invoke_definir_taille(2);
        o.invoke_definir_opacite(50.0, true);
        assert_eq!(
            (o.get_coin().as_str(), o.get_taille(), o.get_opacite()),
            ("tr", 2, 50)
        );
    });
    assert_eq!(
        memoire.lire(|p| p.overlay),
        ReglagesOverlay {
            coin: CoinOverlay::HautDroite,
            taille: TailleOverlay::Grande,
            opacite: 50,
            ..ReglagesOverlay::default()
        }
    );
    attendre(
        || overlay_affiche(&overlay) == Some((1920 + 1920 - 276 - 16, 16, 128)),
        "c353 : coin, taille et opacité choisis s'appliquent à l'overlay",
    );
    sur_l_interface(&faible, |f| {
        let m = f.global::<DonneesOverlayMessages>();
        assert!(m.get_disponible() && m.get_saisie_disponible());
        assert_eq!((m.get_genres(), m.get_coin().as_str()), (0, "tr"));
        m.invoke_definir_coin("bl".into());
        m.invoke_definir_duree(15.0, true);
        m.invoke_definir_sourdine(false);
        m.invoke_definir_genres(2);
        assert!(m.invoke_touche_choisie(0, "r".into()));
        assert_eq!(m.get_touche_reponse().as_str(), "R");
        assert!(!m.invoke_touche_choisie(1, "r".into()));
        assert_ne!(m.get_erreur().as_str(), "");
        assert_eq!((m.get_duree(), m.get_coin().as_str()), (15, "bl"));
    });
    let attendus = ReglagesMessages {
        genres: GenresMessages::Mentions,
        coin: CoinOverlay::BasGauche,
        duree: 15,
        sourdine_ne_pas_deranger: false,
        touche_reponse: "KeyR".into(),
        ..ReglagesMessages::default()
    };
    assert_eq!(memoire.lire(|p| p.overlay_messages.clone()), attendus);
    assert_eq!(overlay_messages.reglages(), attendus);
    overlay_messages.recevoir(
        ToastMessage {
            id: "m1".into(),
            conversation: CleConversation::Prive("d1".into()),
            cible: "dm/d1".into(),
            titre: "alice".into(),
            corps: "on joue ?".into(),
            auteur: None,
        },
        None,
    );
    attendre(
        || {
            calque_messages
                .calques()
                .first()
                .cloned()
                .flatten()
                .is_some_and(|a| (a.x, a.y) == (1920 + 16, 1080 - 72 - 16))
        },
        "c354 : le bandeau s'affiche dans le coin choisi de l'écran du jeu",
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
    attendre(
        || overlay.calques() == vec![None],
        "c353 : hors jeu, l'overlay réglé « seulement en jeu » se masque",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesOverlay>().invoke_definir_en_jeu(false)
    });
    attendre(
        || overlay_affiche(&overlay).is_some_and(|(x, ..)| x == 1920 + 1920 - 276 - 16),
        "c353 : « toujours », l'overlay reste sur le dernier écran vu",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesOverlay>().invoke_definir_actif(false)
    });
    attendre(
        || overlay.calques() == vec![None],
        "c353 : overlay désactivé",
    );
    assert_eq!(memoire.lire(|p| p.overlay.quand), QuandOverlay::Jamais);

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
    let overlay = OverlayFactice::nouveau(None, 1.0);
    let overlay_voix = ServiceOverlayVoix::nouveau(
        &MoteurOverlay::nouveau(Arc::new(overlay.clone())),
        memoire.lire(|p| p.overlay),
    );
    overlay_voix.suivre_les_jeux(&entree.runtime(), service.abonner());
    overlay::reglages::brancher(&fenetre, overlay_voix.clone(), memoire.clone());
    let calque_messages = OverlayFactice::nouveau(None, 1.0);
    let overlay_messages = ServiceOverlayMessages::nouveau(
        &MoteurOverlay::nouveau(Arc::new(calque_messages.clone())),
        memoire.lire(|p| p.overlay_messages.clone()),
        OptionsMessages {
            raccourcis: Arc::new(estompe_plateforme::RaccourcisFactices::nouveaux(true)),
            touches_prises: Arc::new(Vec::new),
            horloge: None,
        },
    );
    overlay_messages.suivre_les_jeux(&entree.runtime(), service.abonner());
    overlay::reglages::brancher_messages(&fenetre, overlay_messages.clone(), memoire.clone());
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
        overlay,
        overlay_voix,
        calque_messages,
        overlay_messages,
    };
    let faible = fenetre.as_weak();
    piloter(move || scenario(faible, faux));
    drop(coquille);
    drop(entree);
}
