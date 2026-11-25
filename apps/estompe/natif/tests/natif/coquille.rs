use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{evenement, message, FausseFabrique};
use commun::{attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::sections::Section;
use estompe_client::temps_reel::{EtatReseau, Signal};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesParametres, EtatBandeau, GenreVue, LigneServeur,
    Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, Model};

fn cliquer(faible: &slint::Weak<Principale>, libelle: &'static str) {
    sur_l_interface(faible, move |f| {
        let element = ElementHandle::find_by_accessible_label(f, libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"));
        element.mock_single_click(PointerEventButton::Left);
    });
}

fn description(faible: &slint::Weak<Principale>, libelle: &'static str) -> String {
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .and_then(|e| e.accessible_description())
            .map(|d| d.to_string())
            .unwrap_or_default()
    })
}

fn serveurs(f: &Principale) -> Vec<LigneServeur> {
    f.global::<DonneesCoquille>()
        .get_serveurs()
        .iter()
        .collect()
}

fn ligne(f: &Principale, id: &str) -> Option<LigneServeur> {
    serveurs(f).into_iter().find(|l| l.id == id)
}

fn ordre(f: &Principale) -> Vec<String> {
    serveurs(f).iter().map(|l| l.id.to_string()).collect()
}

fn titre(f: &Principale) -> String {
    f.global::<DonneesCoquille>().get_vue().titre.to_string()
}

fn scenario(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    memoire: Memoire,
    auth: Arc<FausseAuth>,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille && serveurs(f).len() == 4,
        "coquille remplie depuis le store",
    );
    attendre_l_interface(
        &faible,
        |f| titre(f) == "annonces",
        "dernière vue restaurée",
    );
    assert_eq!(
        sur_l_interface(&faible, ordre),
        ["s2", "s1", "s3", "s4"],
        "ordre des serveurs des préférences"
    );
    assert_eq!(fabrique.ouvertures.load(Ordering::SeqCst), 1);
    let resume = |id: &'static str| {
        sur_l_interface(&faible, move |f| {
            let l = ligne(f, id).unwrap();
            (l.actif, l.non_lus, l.mentions)
        })
    };
    assert_eq!(resume("s1"), (true, 5, 1));
    assert_eq!(resume("s2"), (false, 0, 0));
    assert_eq!(description(&faible, "Travail"), "");
    fabrique.envoyer(evenement("message.created", message("90", "t1", "carol")));
    attendre_l_interface(
        &faible,
        |f| ligne(f, "s2").is_some_and(|l| l.non_lus == 1),
        "non-lu reçu en temps réel sur le rail",
    );
    assert_eq!(description(&faible, "Travail"), "1 nouveau message");
    let mut mention = message("91", "t1", "carol");
    mention["group_mention"] = json!(false);
    fabrique.envoyer(evenement("mention", mention));
    attendre_l_interface(
        &faible,
        |f| ligne(f, "s2").is_some_and(|l| l.mentions == 1),
        "mention reçue en temps réel",
    );
    assert_eq!(description(&faible, "Travail"), "1 mention");
    let pastilles = sur_l_interface(&faible, |f| {
        ElementHandle::find_by_accessible_label(f, "1 mention").count()
    });
    assert!(pastilles >= 1, "pastille de mention affichée");

    cliquer(&faible, "Travail");
    attendre_l_interface(&faible, |f| titre(f) == "équipe", "salon du serveur ouvert");
    assert_eq!(
        sur_l_interface(&faible, |f| f
            .global::<DonneesCoquille>()
            .get_panneau()
            .titre
            .to_string()),
        "Travail"
    );
    attendre_l_interface(
        &faible,
        |f| ligne(f, "s2").is_some_and(|l| l.non_lus == 0 && l.mentions == 0 && l.actif),
        "le salon ouvert est lu",
    );

    cliquer(&faible, "Accueil");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_vue().genre == GenreVue::Accueil,
        "accueil",
    );
    cliquer(&faible, "carol");
    attendre_l_interface(&faible, |f| titre(f) == "carol", "message privé ouvert");
    assert_eq!(
        memoire.lire(|p| p.derniere_vue.clone()),
        Some(DerniereVue::enregistrer("u1", "#/dm/d1"))
    );

    cliquer(&faible, "Les amis");
    attendre_l_interface(
        &faible,
        |f| titre(f) == "annonces",
        "retour au dernier salon vu",
    );
    cliquer(&faible, "Salon vocal");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_vue().genre == GenreVue::Vocal,
        "salon vocal marqué bientôt",
    );
    assert_eq!(
        memoire.lire(|p| p.derniere_vue.clone()),
        Some(DerniereVue::enregistrer("u1", "#/s/s1/v1"))
    );

    cliquer(&faible, "Replier le panneau");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_panneau_replie(),
        "panneau replié",
    );
    assert!(memoire.lire(|p| p.panneau_replie));
    cliquer(&faible, "Déplier le panneau");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesCoquille>().get_panneau_replie(),
        "panneau déplié",
    );
    assert!(!memoire.lire(|p| p.panneau_replie));

    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_deplacer_serveur("s4".into(), "s2".into())
    });
    attendre_l_interface(
        &faible,
        |f| ordre(f) == ["s4", "s2", "s1", "s3"],
        "serveur déplacé sur le rail",
    );
    assert_eq!(
        memoire.lire(|p| p.ordre_serveurs.clone()),
        ["s4", "s2", "s1", "s3"]
    );

    let bandeau = |attendu: EtatBandeau| {
        move |f: &Principale| f.global::<DonneesCoquille>().get_bandeau() == attendu
    };
    assert_eq!(
        sur_l_interface(&faible, |f| f.global::<DonneesCoquille>().get_bandeau()),
        EtatBandeau::Aucun
    );
    fabrique.reseau.send_replace(EtatReseau::HorsLigne);
    attendre_l_interface(
        &faible,
        bandeau(EtatBandeau::HorsLigne),
        "bandeau hors ligne",
    );
    fabrique.reseau.send_replace(EtatReseau::Connecte);
    attendre_l_interface(&faible, bandeau(EtatBandeau::Aucun), "bandeau retiré");
    fabrique.reseau.send_replace(EtatReseau::Reconnexion);
    attendre_l_interface(
        &faible,
        bandeau(EtatBandeau::Reconnexion),
        "bandeau de reconnexion",
    );
    fabrique.reseau.send_replace(EtatReseau::Connecte);
    attendre_l_interface(
        &faible,
        bandeau(EtatBandeau::Aucun),
        "bandeau retiré à nouveau",
    );

    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_sections().row_count() == 1,
        "entrée de section annoncée par son /me",
    );
    cliquer(&faible, "Entracte · films et séries");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_vue().genre == GenreVue::Section,
        "section marquée bientôt",
    );

    sur_l_interface(&faible, |f| {
        let g = f.global::<DonneesParametres>();
        g.set_onglet("notifications".into());
        g.set_ouverte(true);
    });
    assert!(sur_l_interface(&faible, |f| f
        .global::<DonneesParametres>()
        .get_type_prive()));
    fabrique.envoyer(evenement(
        "notify_settings.updated",
        json!({"levels": {}, "types": {"dm": false, "mention": true, "channel": false, "voice": true}, "preview": true, "sound": true}),
    ));
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesParametres>();
            !g.get_type_prive() && g.get_type_mention() && !g.get_type_salon() && g.get_type_vocal()
        },
        "réglage changé sur un autre appareil reflété dans les paramètres ouverts",
    );
    let interrupteur = sur_l_interface(&faible, |f| {
        ElementHandle::find_by_accessible_label(f, "Messages de salon")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
            .and_then(|e| e.accessible_checked())
    });
    assert_eq!(
        interrupteur,
        Some(false),
        "interrupteur à jour sans relancer"
    );
    cliquer(&faible, "Accueil");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesParametres>().get_ouverte(),
        "naviguer quitte les paramètres",
    );

    fabrique.envoyer(evenement("server.deleted", json!({"id": "s1"})));
    attendre_l_interface(
        &faible,
        |f| ligne(f, "s1").is_none(),
        "serveur retiré du rail",
    );

    fabrique.envoyer(Signal::SessionExpiree);
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Connexion && f.get_connexion_erreur().contains("expiré"),
        "retour à la connexion à la session expirée",
    );
    attendre_l_interface(&faible, |f| serveurs(f).is_empty(), "coquille vidée");
    assert_eq!(auth.deconnexions.load(Ordering::SeqCst), 0);
}

#[test]
fn c3_la_coquille_suit_le_store_en_temps_reel_et_s_arrete_avec_la_session() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(
        Preferences {
            ordre_serveurs: vec!["s2".into()],
            derniere_vue: Some(DerniereVue::enregistrer("u1", "#/s/s1/c2")),
            ..Preferences::default()
        },
        dossier.path().to_path_buf(),
        None,
    );
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
            auth.clone(),
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let fabrique = FausseFabrique::nouvelle(commun::coquille::etat(), vec![Section::Entracte]);
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

    let faible = fenetre.as_weak();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| {
            scenario(faible, fabrique, memoire, auth)
        }));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    let resultat = pilote.join().unwrap();
    drop(coquille);
    drop(entree);
    if let Err(panique) = resultat {
        resume_unwind(panique);
    }
}
