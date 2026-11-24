mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, png, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    confirmation, creer_fenetre, ecrans, DonneesConfirmation, DonneesCoquille,
    DonneesReglagesServeur, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, Model};

fn cliquer(faible: &slint::Weak<Principale>, libelle: &str) {
    let libelle = libelle.to_string();
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, &libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"))
            .mock_single_click(PointerEventButton::Left);
    });
}

fn onglets(f: &Principale) -> Vec<String> {
    f.global::<DonneesReglagesServeur>()
        .get_onglets()
        .iter()
        .map(|o| o.id.to_string())
        .collect()
}

fn requetes(serveur: &ServeurMessages, ligne: &str) -> usize {
    serveur
        .requetes
        .lock()
        .unwrap()
        .iter()
        .filter(|r| *r == ligne)
        .count()
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

fn changer_les_droits(fabrique: &FausseFabrique, permissions: serde_json::Value) {
    let store = fabrique.store();
    let mut etat = store.instantane();
    etat.details.get_mut("s1").unwrap().permissions =
        Some(serde_json::from_value(permissions).unwrap());
    store.installer(etat);
}

fn image(largeur: u32, hauteur: u32) -> Vec<u8> {
    let mut octets = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(largeur, hauteur, image::Rgba([200, 80, 120, 255]))
        .write_to(&mut octets, image::ImageFormat::Png)
        .unwrap();
    octets.into_inner()
}

fn onglets_selon_le_role(faible: &slint::Weak<Principale>, fabrique: &FausseFabrique) {
    changer_les_droits(fabrique, json!(["view_channel", "send_messages"]));
    cliquer(faible, "Paramètres du serveur");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_ouverte()
                && onglets(f) == ["vue", "emojis", "membres"]
                && !g.get_peut_renommer()
                && !g.get_peut_icone()
                && g.get_role() == "Tu es propriétaire"
        },
        "membre ordinaire : vue d'ensemble, emojis et membres seulement",
    );
    changer_les_droits(
        fabrique,
        json!(["view_channel", "create_invites", "ban_members"]),
    );
    attendre_l_interface(
        faible,
        |f| onglets(f) == ["vue", "invitations", "emojis", "membres", "bannis"],
        "invitations et bannis suivent les droits en temps réel",
    );
    changer_les_droits(
        fabrique,
        json!([
            "view_channel",
            "manage_server",
            "create_invites",
            "manage_channels",
            "manage_roles",
            "view_audit_log",
            "ban_members"
        ]),
    );
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            onglets(f).len() == 8 && g.get_peut_renommer() && g.get_peut_icone()
        },
        "tous les droits : huit onglets, nom et icône modifiables",
    );
    println!("364 C2 : chaque onglet n'apparaît que si le rôle le permet");
}

fn renommer(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesReglagesServeur>();
        g.set_nom("Les copains".into());
        g.invoke_nom_modifie("Les copains".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_modifie() && g.get_compte_nom() == "11 / 50"
        },
        "modification signalée",
    );
    cliquer(faible, "Enregistrer");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            !g.get_modifie()
                && g.get_annonce() == "Modifications enregistrées."
                && f.global::<DonneesCoquille>().get_panneau().titre == "Les copains"
        },
        "serveur renommé",
    );
    assert_eq!(
        corps(serveur, "PATCH /servers/s1"),
        [json!({"name": "Les copains"})]
    );
    println!("364 : nom du serveur enregistré");
}

fn invitations(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("invitations".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_titre() == "Invitations" && g.get_invitations().row_count() == 1
        },
        "liens actifs chargés",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesReglagesServeur>();
        g.invoke_choisir_expiration(2);
        g.invoke_choisir_utilisation(2);
    });
    cliquer(faible, "Créer un lien");
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesReglagesServeur>()
                .get_invitations()
                .row_count()
                == 2
        },
        "lien créé",
    );
    assert_eq!(
        corps(serveur, "POST /servers/s1/invites"),
        [json!({"max_uses": 5, "expires_in": 3600})]
    );
    let detail = sur_l_interface(faible, |f| {
        let i = f
            .global::<DonneesReglagesServeur>()
            .get_invitations()
            .row_data(1)
            .unwrap();
        (i.lien.to_string(), i.detail.to_string())
    });
    assert!(detail.0.ends_with("/#/join/code2"), "{}", detail.0);
    assert!(detail
        .1
        .starts_with("Par bob · 0 / 5 utilisation(s) · expire le 19/11/2026"));
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_copier_invitation(1)
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesReglagesServeur>().get_annonce() == "Lien copié.",
        "lien copié",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_revoquer_invitation(0)
    });
    attendre_l_interface(
        faible,
        |f| {
            let l = f.global::<DonneesReglagesServeur>().get_invitations();
            l.row_count() == 1 && l.row_data(0).unwrap().code == "code2"
        },
        "lien révoqué",
    );
    assert_eq!(requetes(serveur, "DELETE /servers/s1/invites/code1"), 1);
    println!("364 : invitations créées, listées, copiées et révoquées");
}

fn emojis(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
    dossier: &std::path::Path,
) {
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("emojis".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_emojis().row_count() == 2 && g.get_compte_emojis() == "2 / 50 emojis"
        },
        "emojis du serveur listés",
    );
    let trop_grand = dossier.join("grand.png");
    std::fs::write(&trop_grand, image(300, 40)).unwrap();
    fabrique.fichiers.choix.lock().unwrap().push(trop_grand);
    cliquer(faible, "Ajouter un emoji");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_erreur() == "Image trop grande : 256 px au plus" && !g.get_emoji_en_attente()
        },
        "image trop grande refusée",
    );
    let trop_lourd = dossier.join("lourd.gif");
    let mut gif = b"GIF89a\x10\x00\x10\x00".to_vec();
    gif.resize(130 << 10, 0);
    std::fs::write(&trop_lourd, gif).unwrap();
    fabrique.fichiers.choix.lock().unwrap().push(trop_lourd);
    cliquer(faible, "Ajouter un emoji");
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesReglagesServeur>().get_erreur()
                == "Image trop lourde : 128 Ko au plus"
        },
        "image trop lourde refusée",
    );
    assert!(!serveur
        .requetes
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.starts_with("POST /servers/s1/emojis")));
    println!("364 C1 : un emoji au-dessus des limites est refusé avant l'envoi");
    let bonne = dossier.join("Chat Roux.png");
    std::fs::write(&bonne, png([30, 200, 60])).unwrap();
    fabrique.fichiers.choix.lock().unwrap().push(bonne);
    cliquer(faible, "Ajouter un emoji");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_emoji_en_attente()
                && g.get_nom_emoji() == "chat_roux"
                && g.get_nom_emoji_valide()
                && g.get_erreur() == ""
        },
        "emoji en attente avec son nom proposé",
    );
    cliquer(faible, "Ajouter");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            !g.get_emoji_en_attente() && g.get_emojis().row_count() == 3
        },
        "emoji ajouté",
    );
    assert_eq!(
        requetes(serveur, "POST /servers/s1/emojis?name=chat_roux"),
        1
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_renommer_emoji(2)
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "saisie du nouveau nom",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesConfirmation>()
            .set_valeur("Chat_Gris".into());
        confirmation::repondre(f, true);
    });
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesReglagesServeur>()
                .get_emojis()
                .iter()
                .any(|e| e.nom == "chat_gris")
        },
        "emoji renommé",
    );

    assert_eq!(
        corps(serveur, "PATCH /servers/s1/emojis/e13"),
        [json!({"name": "chat_gris"})]
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_supprimer_emoji(0)
    });
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesReglagesServeur>()
                .get_emojis()
                .row_count()
                == 2
        },
        "emoji supprimé",
    );
    assert_eq!(requetes(serveur, "DELETE /servers/s1/emojis/e1"), 1);
    println!("364 : emoji ajouté, renommé et supprimé");
}

fn membres_et_bannis(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("membres".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_membres().row_count() == 10
                && g.get_compte_membres() == "10 membres"
                && g.get_membres().row_data(0).unwrap().createur
        },
        "membres listés, créateur en tête",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesReglagesServeur>();
        g.set_recherche("elise".into());
        g.invoke_chercher_membres("elise".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_membres().row_count() == 1 && g.get_membres().row_data(0).unwrap().nom == "Élise"
        },
        "recherche sans accents",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("bannis".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let l = f.global::<DonneesReglagesServeur>().get_bannis();
            l.row_count() == 1
                && l.row_data(0).unwrap().detail == "par bob · 18/11/2026 · définitif"
        },
        "bannis chargés",
    );
    cliquer(faible, "Lever le ban");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_bannis().row_count() == 0 && g.get_annonce() == "Bannissement levé."
        },
        "ban levé",
    );
    assert_eq!(requetes(serveur, "DELETE /servers/s1/bans/zed"), 1);
    println!("364 : membres cherchés, ban levé");
}

fn supprimer(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("vue".into())
    });
    cliquer(faible, "Supprimer le serveur…");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesConfirmation>();
            g.get_ouverte()
                && !g.invoke_peut_valider(g.get_demande(), "Les amis".into(), false)
                && g.invoke_peut_valider(g.get_demande(), "Les copains".into(), false)
        },
        "suppression confirmée par la saisie exacte du nom",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesConfirmation>()
            .set_valeur("Les copains".into());
        confirmation::repondre(f, true);
    });
    attendre(
        || requetes(serveur, "DELETE /servers/s1") == 1,
        "suppression envoyée",
    );
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesReglagesServeur>().get_ouverte()
                && !f
                    .global::<DonneesCoquille>()
                    .get_serveurs()
                    .iter()
                    .any(|s| s.id == "s1")
        },
        "serveur supprimé, page refermée",
    );
    println!("364 : serveur supprimé après saisie de son nom");
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
    onglets_selon_le_role(&faible, &fabrique);
    renommer(&faible, &serveur);
    invitations(&faible, &serveur);
    emojis(&faible, &serveur, &fabrique, &dossier);
    membres_et_bannis(&faible, &serveur);
    supprimer(&faible, &serveur);
}

#[test]
fn administrer_un_serveur() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(20);
    *serveur.invitations.lock().unwrap() =
        vec![json!({"code": "code1", "created_by": "bob", "uses": 3})];
    *serveur.bannis.lock().unwrap() =
        vec![json!({"user_id": "zed", "pseudo": "zed", "by": "bob", "at": "2026-11-18T10:00:00Z"})];
    *serveur.emojis.lock().unwrap() = vec![
        json!({"id": "e1", "server_id": "s1", "name": "chat", "created_by": "u1"}),
        json!({"id": "e2", "server_id": "s1", "name": "fete", "created_by": "carol"}),
        json!({"id": "e3", "server_id": "s2", "name": "autre", "created_by": "eve"}),
    ];
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
