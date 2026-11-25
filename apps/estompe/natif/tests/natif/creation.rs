use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, evenement, png, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::route::Route;
use estompe_natif::coquille::{Coquille, Pont};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::menu::{self, Ancre};
use estompe_natif::session::Memoire;
use estompe_natif::{
    confirmation, creer_fenetre, ecrans, DonneesConfirmation, DonneesCoquille,
    DonneesCreationSalon, DonneesCreerRejoindre, DonneesMenu, DonneesProfil,
    DonneesReglagesServeur, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

fn appuyer(f: &Principale, texte: impl Into<SharedString>) {
    let texte = texte.into();
    f.window().dispatch_event(WindowEvent::KeyPressed {
        text: texte.clone(),
    });
    f.window()
        .dispatch_event(WindowEvent::KeyReleased { text: texte });
}

fn taper(faible: &slint::Weak<Principale>, texte: &str) {
    let texte = texte.to_string();
    sur_l_interface(faible, |_| slint::platform::update_timers_and_animations());
    sur_l_interface(faible, move |f| {
        for c in texte.chars() {
            appuyer(f, c.to_string());
        }
    });
}

fn touche(faible: &slint::Weak<Principale>, touche: Key) {
    sur_l_interface(faible, move |f| appuyer(f, touche));
}

fn boutons(f: &Principale, libelle: &str) -> usize {
    ElementHandle::find_by_accessible_label(f, libelle)
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .count()
}

fn cliquer(faible: &slint::Weak<Principale>, libelle: &str, role: AccessibleRole) {
    let libelle = libelle.to_string();
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, &libelle)
            .find(|e| e.accessible_role() == Some(role))
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"))
            .mock_single_click(PointerEventButton::Left);
    });
}

fn salons(f: &Principale) -> Vec<String> {
    f.global::<DonneesCoquille>()
        .get_salons()
        .iter()
        .map(|s| s.id.to_string())
        .collect()
}

fn serveurs(f: &Principale) -> Vec<String> {
    f.global::<DonneesCoquille>()
        .get_serveurs()
        .iter()
        .map(|s| s.id.to_string())
        .collect()
}

fn menu_ids(f: &Principale) -> Vec<String> {
    f.global::<DonneesMenu>()
        .get_entrees()
        .iter()
        .map(|e| e.id.to_string())
        .collect()
}

fn corps(serveur: &ServeurMessages, ligne: &str) -> Vec<serde_json::Value> {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .filter(|(r, _)| r == ligne)
        .map(|(_, c)| serde_json::from_str(c).unwrap_or_default())
        .collect()
}

fn requete_vue(serveur: &ServeurMessages, ligne: &str) -> bool {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .any(|(r, _)| r == ligne)
        || serveur.requetes.lock().unwrap().iter().any(|r| r == ligne)
}

fn changer_les_droits(fabrique: &FausseFabrique, permissions: serde_json::Value) {
    let store = fabrique.store();
    let mut etat = store.instantane();
    etat.details.get_mut("s1").unwrap().permissions =
        Some(serde_json::from_value(permissions).unwrap());
    store.installer(etat);
}

fn sans_le_droit_l_entree_est_absente(
    faible: &slint::Weak<Principale>,
    pont: &Pont,
    fabrique: &FausseFabrique,
) {
    attendre_l_interface(
        faible,
        |f| boutons(f, "Créer un salon texte") == 1 && boutons(f, "Créer un salon") == 1,
        "entrées de création avec le droit",
    );
    changer_les_droits(fabrique, json!(["view_channel", "send_messages"]));
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesCoquille>().get_panneau().peut_creer_salon
                && boutons(f, "Créer un salon texte") == 0
                && boutons(f, "Créer un salon vocal") == 0
                && boutons(f, "Créer un salon") == 0
        },
        "sans « Gérer les salons », aucune entrée de création",
    );
    let p = pont.clone();
    let entrees = sur_l_interface(faible, move |f| {
        p.menu_salon(f, "c1", Ancre::Point { x: 300.0, y: 300.0 });
        let salon = menu_ids(f);
        menu::fermer(f);
        p.menu_serveur(f, "s1", Ancre::Point { x: 30.0, y: 30.0 });
        let serveur = menu_ids(f);
        menu::fermer(f);
        p.ouvrir_creation_salon(f, "s1", estompe_client::store::GenreSalon::Text, None);
        (
            salon,
            serveur,
            f.global::<DonneesCreationSalon>().get_ouvert(),
        )
    });
    assert!(!entrees.0.contains(&"creer-salon".to_string()));
    assert!(!entrees.1.contains(&"creer-salon".to_string()));
    assert!(
        !entrees.1.contains(&"quitter".to_string()),
        "le créateur ne quitte pas son serveur"
    );
    assert!(!entrees.2, "la fenêtre ne s'ouvre pas sans le droit");
    changer_les_droits(
        fabrique,
        json!(["view_channel", "send_messages", "manage_channels"]),
    );
    attendre_l_interface(
        faible,
        |f| boutons(f, "Créer un salon texte") == 1 && boutons(f, "Créer un salon vocal") == 1,
        "le droit rendu, les entrées reviennent",
    );
    println!("363 C3 : sans le droit de gérer les salons, ni bouton ni entrée de menu");
}

fn creer_un_salon(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
) {
    cliquer(faible, "Créer un salon texte", AccessibleRole::Button);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesCreationSalon>();
            g.get_ouvert() && g.get_serveur() == "Les amis" && g.get_genres().row_count() == 4
        },
        "fenêtre « Créer un salon » ouverte, sans cinéma hors Entracte",
    );
    taper(faible, "Général");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesCreationSalon>();
            g.get_erreur() == "Un salon de ce type porte déjà ce nom." && !g.get_peut_creer()
        },
        "nom déjà pris refusé avant l'envoi",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreationSalon>();
        g.set_nom("".into());
        g.invoke_nom_modifie("".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesCreationSalon>().get_erreur() == "Donne un nom au salon.",
        "nom vide",
    );
    taper(faible, "refusé");
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesCreationSalon>().get_erreur() == "Nom refusé par le serveur",
        "erreur du serveur affichée",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreationSalon>();
        g.set_nom("".into());
        g.invoke_nom_modifie("".into());
    });
    cliquer(faible, "Vocal", AccessibleRole::Button);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesCreationSalon>();
            g.get_vocal() && !g.get_texte() && g.get_icone() == "volume"
        },
        "type vocal choisi",
    );
    cliquer(faible, "Texte", AccessibleRole::Button);
    cliquer(faible, "Salon privé", AccessibleRole::Switch);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesCreationSalon>();
            g.get_prive() && g.get_roles().row_count() == 2 && g.get_membres().row_count() == 9
        },
        "rôles (sans everyone) et membres (sans moi) proposés",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreationSalon>();
        let roles: Vec<String> = g.get_roles().iter().map(|r| r.nom.to_string()).collect();
        let membres: Vec<String> = g.get_membres().iter().map(|m| m.nom.to_string()).collect();
        assert_eq!(roles, ["Modération", "Artistes"]);
        assert_eq!(&membres[..2], ["Ève", "Caro"], "surnom de serveur d'abord");
        g.invoke_basculer_role(1);
        g.invoke_basculer_membre(1);
    });
    sur_l_interface(faible, |f| {
        f.global::<DonneesCreationSalon>().set_avances(true);
    });
    sur_l_interface(faible, |f| {
        f.global::<DonneesCreationSalon>()
            .invoke_choisir_mode_lent(3);
        f.global::<DonneesCreationSalon>()
            .set_sujet("Les sorties du soir".into());
    });
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreationSalon>();
        g.set_demande_focus(g.get_demande_focus() + 1);
    });
    taper(faible, "jeux du soir");
    touche(faible, Key::Return);
    attendre(
        || corps(serveur, "POST /servers/s1/channels").len() == 2,
        "deuxième envoi",
    );
    assert_eq!(
        corps(serveur, "POST /servers/s1/channels")[1],
        json!({"name": "jeux du soir", "kind": "text", "topic": "Les sorties du soir", "slowmode": 30, "private": true, "roles": ["ra"], "members": ["carol"]})
    );
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesCreationSalon>().get_ouvert()
                && salons(f).contains(&"c9".to_string())
                && f.global::<DonneesCoquille>().get_vue().titre == "jeux du soir"
        },
        "salon créé ouvert et listé dans le panneau",
    );
    println!("363 : salon texte privé créé avec ses réglages, refus et nom pris affichés");

    fabrique.envoyer(evenement(
        "channel.created",
        json!({"server_id": "s1", "channel": {"id": "c10", "name": "créé par Ève", "kind": "voice", "position": 4}}),
    ));
    attendre_l_interface(
        faible,
        |f| salons(f).contains(&"c10".to_string()),
        "salon créé par un autre membre reçu en temps réel",
    );
    println!("363 C2 : un salon créé par un autre membre apparaît dans le panneau en temps réel");
}

fn creer_et_rejoindre_un_serveur(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
    dossier: &std::path::Path,
) {
    cliquer(
        faible,
        "Créer ou rejoindre un serveur",
        AccessibleRole::Button,
    );
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesCreerRejoindre>();
            g.get_ouvert() && g.get_peut_creer() && g.get_peut_rejoindre()
        },
        "fenêtre « Créer ou rejoindre un serveur » ouverte",
    );
    taper(faible, "Atelier vélo");

    attendre_l_interface(
        faible,
        |f| f.global::<DonneesCreerRejoindre>().get_initiales() == "AT",
        "aperçu des initiales",
    );
    let image = dossier.join("icone.png");
    std::fs::write(&image, png([30, 200, 60])).unwrap();
    fabrique.fichiers.choix.lock().unwrap().push(image);
    cliquer(faible, "Choisir une icône", AccessibleRole::Button);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesProfil>();
            g.get_recadrage_ouvert() && g.get_recadrage_titre() == "Recadrer l'icône du serveur"
        },
        "recadrage de l'icône ouvert",
    );
    cliquer(faible, "Valider", AccessibleRole::Button);
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesProfil>().get_recadrage_ouvert()
                && f.global::<DonneesCreerRejoindre>().get_a_icone()
        },
        "icône recadrée dans l'aperçu",
    );
    cliquer(faible, "Créer", AccessibleRole::Button);
    attendre(
        || requete_vue(serveur, "PUT /servers/s9/icon"),
        "icône envoyée au nouveau serveur",
    );
    assert_eq!(
        corps(serveur, "POST /servers"),
        [json!({"name": "Atelier vélo"})]
    );
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesCreerRejoindre>().get_ouvert()
                && serveurs(f).contains(&"s9".to_string())
                && f.global::<DonneesCoquille>().get_panneau().titre == "Atelier vélo"
        },
        "serveur créé ouvert",
    );
    println!("363 : serveur créé avec son icône recadrée, puis ouvert");

    cliquer(
        faible,
        "Créer ou rejoindre un serveur",
        AccessibleRole::Button,
    );
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesCreerRejoindre>().get_ouvert(),
        "fenêtre rouverte",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreerRejoindre>();
        g.set_code("zzzzz-11111".into());
        g.invoke_code_modifie("zzzzz-11111".into());
        g.invoke_rejoindre();
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesCreerRejoindre>().get_erreur() == "Invitation invalide ou révoquée.",
        "invitation invalide",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreerRejoindre>();
        g.set_code("".into());
        g.invoke_code_modifie("".into());
    });
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesCreerRejoindre>();
        let lien = "https://chat.plbls.fr/#/join/abcde-23456";
        g.set_code(lien.into());
        g.invoke_code_modifie(lien.into());
        g.invoke_rejoindre();
    });
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesCreerRejoindre>().get_ouvert()
                && serveurs(f).contains(&"s8".to_string())
                && f.global::<DonneesCoquille>().get_panneau().titre == "Club photo"
        },
        "serveur rejoint par son lien et ouvert",
    );
    assert!(requete_vue(
        serveur,
        "POST /server-invites/ABCDE-23456/join"
    ));
    println!("363 : rejoindre par un lien complet, code invalide signalé");
}

fn quitter_un_serveur(faible: &slint::Weak<Principale>, serveur: &ServeurMessages, pont: &Pont) {
    let p = pont.clone();
    let ids = sur_l_interface(faible, move |f| {
        p.menu_serveur(f, "s8", Ancre::Point { x: 30.0, y: 30.0 });
        let ids = menu_ids(f);
        menu::choisir(f, "reglages-serveur");
        ids
    });
    assert!(!ids.contains(&"quitter".to_string()));
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesReglagesServeur>().get_ouverte()
                && boutons(f, "Quitter le serveur") == 1
        },
        "« Quitter le serveur » dans les réglages du serveur",
    );
    cliquer(faible, "Quitter le serveur", AccessibleRole::Button);
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "confirmation demandée",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
    attendre(
        || requete_vue(serveur, "POST /servers/s8/leave"),
        "départ envoyé",
    );
    attendre_l_interface(
        faible,
        |f| {
            !serveurs(f).contains(&"s8".to_string())
                && !f.global::<DonneesReglagesServeur>().get_ouverte()
        },
        "serveur retiré du rail, réglages refermés",
    );
    println!("363 : quitter un serveur depuis ses réglages, après confirmation");
}

fn scenario(
    faible: slint::Weak<Principale>,
    serveur: Arc<ServeurMessages>,
    coquille: Arc<Coquille>,
    fabrique: Arc<FausseFabrique>,
    dossier: std::path::PathBuf,
) {
    attendre_l_interface(
        &faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.global::<DonneesCoquille>().get_vue().titre == "général"
        },
        "salon ouvert",
    );
    attendre(|| coquille.pont().is_some(), "pont de la session");
    let pont = coquille.pont().unwrap();
    attendre(
        || {
            pont.route()
                == Route::Salon {
                    serveur: "s1".into(),
                    salon: Some("c1".into()),
                    message: None,
                }
        },
        "route du salon",
    );
    sans_le_droit_l_entree_est_absente(&faible, &pont, &fabrique);
    creer_un_salon(&faible, &serveur, &fabrique);
    creer_et_rejoindre_un_serveur(&faible, &serveur, &fabrique, &dossier);
    quitter_un_serveur(&faible, &serveur, &pont);
}

#[test]
fn creer_rejoindre_quitter_et_creer_un_salon() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(20);
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(
        Preferences {
            derniere_vue: Some(DerniereVue::enregistrer("u1", "#/s/s1/c1")),
            ..Preferences::default()
        },
        dossier.path().to_path_buf(),
        None,
    );
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
    let fabrique = FausseFabrique::sur(etat_avec_membres(), Vec::new(), &serveur.adresse);
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let coquille = Arc::new(Coquille::brancher(
        &fenetre,
        &entree,
        fabrique.clone(),
        memoire,
        Arc::new(move || midi),
    ));
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    let c = coquille.clone();
    let chemin = dossier.path().to_path_buf();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| {
            scenario(faible, serveur, c, fabrique, chemin)
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
