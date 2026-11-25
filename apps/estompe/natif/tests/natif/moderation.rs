use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, evenement, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::store::Etat;
use estompe_natif::coquille::{Coquille, Pont};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::menu::{self, Ancre};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesConfirmation, DonneesCoquille, DonneesMenu, DonneesMessagerie,
    DonneesModeration, DonneesModerationSalon, DonneesReglagesServeur, DonneesRoles, Page,
    Principale,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model};

const MOI: [&str; 10] = [
    "view_channel",
    "send_messages",
    "add_reactions",
    "timeout_members",
    "manage_nicknames",
    "kick_members",
    "ban_members",
    "manage_channels",
    "manage_messages",
    "view_audit_log",
];

const ICI: Ancre = Ancre::Point { x: 300.0, y: 300.0 };

fn etat_de_moderation() -> Etat {
    let mut etat = etat_avec_membres();
    for s in etat.serveurs.iter_mut() {
        if s.id.as_deref() == Some("s1") {
            s.owner_id = Some("eve".into());
        }
    }
    let detail = etat.details.get_mut("s1").unwrap();
    detail.server.as_mut().unwrap().owner_id = Some("eve".into());
    detail.permissions = Some(MOI.iter().map(|p| p.to_string()).collect());
    etat
}

fn menu_ids(f: &Principale) -> Vec<String> {
    f.global::<DonneesMenu>()
        .get_entrees()
        .iter()
        .map(|e| e.id.to_string())
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

fn dernier_corps(serveur: &ServeurMessages, ligne: &str) -> Value {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(r, _)| r == ligne)
        .map(|(_, c)| serde_json::from_str(c).unwrap_or_default())
        .unwrap_or_default()
}

fn menu_du_membre(
    faible: &slint::Weak<Principale>,
    pont: &Pont,
    membre: &'static str,
) -> Vec<String> {
    let p = pont.clone();
    attendre_l_interface(
        faible,
        move |f| p.menu_membre(f, membre, ICI),
        "menu du membre ouvert",
    );
    sur_l_interface(faible, menu_ids)
}

fn moderer(
    faible: &slint::Weak<Principale>,
    pont: &Pont,
    membre: &'static str,
    action: &'static str,
) {
    menu_du_membre(faible, pont, membre);
    sur_l_interface(faible, move |f| menu::choisir(f, action));
}

fn membres(faible: &slint::Weak<Principale>, pont: &Pont, serveur: &ServeurMessages) {
    let attendu = [
        "profil",
        "mentionner",
        "pseudo",
        "timeout",
        "warn",
        "nickname",
        "kick",
        "ban",
        "signaler",
    ];
    assert_eq!(menu_du_membre(faible, pont, "carol"), attendu);
    let libelles: Vec<(String, bool, bool)> = sur_l_interface(faible, |f| {
        f.global::<DonneesMenu>()
            .get_entrees()
            .iter()
            .skip(3)
            .map(|e| (e.libelle.to_string(), e.danger, e.separe))
            .collect()
    });
    assert_eq!(
        libelles,
        [
            ("Exclure temporairement…".to_string(), false, true),
            ("Avertir…".to_string(), false, false),
            ("Changer le pseudo…".to_string(), false, false),
            ("Expulser…".to_string(), true, false),
            ("Bannir…".to_string(), true, false),
            ("Signaler".to_string(), false, true),
        ]
    );
    sur_l_interface(faible, menu::fermer);
    assert_eq!(
        menu_du_membre(faible, pont, "eve"),
        ["profil", "mentionner", "pseudo", "signaler"]
    );
    sur_l_interface(faible, menu::fermer);
    assert_eq!(menu_du_membre(faible, pont, "frank")[3], "untimeout");
    sur_l_interface(faible, |f| menu::choisir(f, "untimeout"));
    attendre(
        || requetes(serveur, "DELETE /servers/s1/members/frank/timeout") == 1,
        "exclusion levée directement",
    );
    println!("366 C1 : menu d'un membre selon les permissions et le rang (créatrice et rang égal exclus)");

    moderer(faible, pont, "carol", "warn");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesModeration>();
            g.get_ouvert()
                && g.get_titre() == "Avertir carol"
                && g.get_raison_obligatoire()
                && !g.get_peut_valider()
        },
        "fenêtre d'avertissement, raison obligatoire",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesModeration>();
        g.set_raison("spam répété".into());
        g.invoke_raison_modifiee("spam répété".into());
        g.invoke_valider();
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesModeration>().get_annonce() == "Avertissement envoyé",
        "avertissement envoyé",
    );
    assert_eq!(
        dernier_corps(serveur, "POST /servers/s1/members/carol/warn"),
        json!({"reason": "spam répété"})
    );
    sur_l_interface(faible, |f| f.global::<DonneesModeration>().invoke_fermer());

    moderer(faible, pont, "dave", "ban");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesModeration>();
            g.get_ouvert()
                && g.get_danger()
                && g.get_bouton() == "Bannir dave"
                && g.get_avec_purge()
        },
        "fenêtre de bannissement",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesModeration>();
        g.invoke_choisir_duree(4);
        g.set_heures("2".into());
        g.invoke_heures_modifiees("2".into());
        g.invoke_choisir_purge(2);
        g.invoke_valider();
    });
    attendre(
        || requetes(serveur, "POST /servers/s1/members/dave/ban") == 1,
        "bannissement envoyé",
    );
    assert_eq!(
        dernier_corps(serveur, "POST /servers/s1/members/dave/ban"),
        json!({"reason": "", "duration": 7200, "delete_messages": 86400})
    );
    sur_l_interface(faible, |f| f.global::<DonneesModeration>().invoke_fermer());

    moderer(faible, pont, "carol", "nickname");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesModeration>();
        assert_eq!(g.get_surnom(), "Caro");
        let long = "x".repeat(40);
        g.set_surnom(long.as_str().into());
        g.invoke_surnom_modifie(long.as_str().into());
        assert_eq!(g.get_surnom().chars().count(), 32);
        g.set_surnom("Caroline".into());
        g.invoke_surnom_modifie("Caroline".into());
        g.invoke_valider();
    });
    attendre(
        || requetes(serveur, "PUT /servers/s1/members/carol/nickname") == 1,
        "pseudo envoyé",
    );
    assert_eq!(
        dernier_corps(serveur, "PUT /servers/s1/members/carol/nickname"),
        json!({"nickname": "Caroline"})
    );
    sur_l_interface(faible, |f| f.global::<DonneesModeration>().invoke_fermer());

    sur_l_interface(faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_ouvrir_membre("ines".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesCoquille>();
            g.get_carte_ouverte() && g.get_carte().moderable
        },
        "carte de profil modérable",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_menu_moderation("ines".into(), 300.0, 300.0, 330.0)
    });
    assert_eq!(
        sur_l_interface(faible, menu_ids),
        ["timeout", "warn", "nickname", "kick", "ban"]
    );
    sur_l_interface(faible, |f| menu::choisir(f, "kick"));
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesCoquille>().get_carte_ouverte()
                && f.global::<DonneesModeration>().get_titre() == "Expulser Inès"
        },
        "expulsion depuis la carte de profil",
    );
    sur_l_interface(faible, |f| f.global::<DonneesModeration>().invoke_fermer());
    println!("366 : avertir, bannir, renommer, lever l'exclusion, modérer depuis la carte");
}

fn salon(faible: &slint::Weak<Principale>, pont: &Pont, serveur: &ServeurMessages) {
    let p = pont.clone();
    attendre_l_interface(faible, move |f| p.menu_salon(f, "c1", ICI), "menu du salon");
    assert!(sur_l_interface(faible, menu_ids).contains(&"moderer-salon".to_string()));
    sur_l_interface(faible, |f| menu::choisir(f, "moderer-salon"));
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesModerationSalon>();
            g.get_ouvert() && g.get_gerer() && g.get_nettoyer() && g.get_nom() == "général"
        },
        "modération du salon ouverte",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesModerationSalon>();
        g.set_nom("annonces".into());
        g.invoke_nom_modifie("annonces".into());
        assert_eq!(g.get_erreur_nom(), "Un salon de ce type porte déjà ce nom.");
        assert!(!g.get_peut_enregistrer());
        g.set_nom("discussions".into());
        g.invoke_nom_modifie("discussions".into());
        g.invoke_choisir_mode_lent(3);
        g.invoke_verrouiller(true);
        g.invoke_enregistrer();
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesModerationSalon>().get_annonce() == "Salon mis à jour.",
        "salon enregistré",
    );
    assert_eq!(
        dernier_corps(serveur, "PATCH /servers/s1/channels/c1"),
        json!({"name": "discussions"})
    );
    assert_eq!(
        dernier_corps(serveur, "PUT /servers/s1/channels/c1/moderation"),
        json!({"slowmode": 30, "locked": true})
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesModerationSalon>();
        g.set_nombre("101".into());
        g.invoke_nombre_modifie("101".into());
        assert!(!g.get_peut_supprimer());
        g.set_nombre("5".into());
        g.invoke_nombre_modifie("5".into());
        g.invoke_supprimer_messages();
    });
    attendre_l_interface(
        faible,
        |f| {
            let c = f.global::<DonneesConfirmation>();
            c.get_ouverte()
                && c.get_demande()
                    .message
                    .contains("les 5 derniers messages de « discussions »")
        },
        "confirmation de la suppression en masse",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesConfirmation>().invoke_repondre(true)
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesModerationSalon>().get_annonce() == "3 message(s) supprimé(s).",
        "nombre de messages supprimés affiché",
    );
    assert_eq!(
        dernier_corps(serveur, "POST /channels/c1/messages/bulk-delete"),
        json!({"last": 5})
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesModerationSalon>().invoke_fermer()
    });
    println!("366 : salon renommé, mode lent, verrouillé, messages supprimés");
}

fn journal(faible: &slint::Weak<Principale>, pont: &Pont, serveur: &ServeurMessages) {
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesReglagesServeur>();
        g.invoke_ouvrir("s1".into());
        g.invoke_choisir("journal".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_onglet() == "journal" && g.get_journal().row_count() == 5
        },
        "journal chargé à l'ouverture",
    );
    let premiere = sur_l_interface(faible, |f| {
        let g = f.global::<DonneesReglagesServeur>();
        assert_eq!(g.get_filtre_journal(), "Toutes les actions");
        g.get_journal().row_data(0).unwrap()
    });
    assert_eq!(premiere.acteur, "bob");
    assert_eq!(premiere.phrase, "a banni « hugo » — spam");
    assert_eq!(premiere.icone, "interdit");
    let p = pont.clone();
    attendre_l_interface(
        faible,
        move |f| p.menu_filtre_journal(f, ICI),
        "menu du filtre",
    );
    sur_l_interface(faible, |f| menu::choisir(f, "member"));
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesReglagesServeur>();
            g.get_journal().row_count() == 2 && g.get_filtre_journal() == "Modération des membres"
        },
        "journal filtré",
    );
    assert_eq!(
        requetes(serveur, "GET /servers/s1/audit-log?action=member"),
        1
    );
    println!("366 : journal d'administration chargé puis filtré");

    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesReglagesServeur>();
        g.invoke_choisir("salons".into());
        f.global::<DonneesRoles>()
            .invoke_supprimer_salon("c4".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "confirmation de suppression du salon",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesConfirmation>().invoke_repondre(true)
    });
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesRoles>()
                .get_salons()
                .iter()
                .any(|s| s.id == "c4")
        },
        "salon supprimé depuis les paramètres",
    );
    assert_eq!(requetes(serveur, "DELETE /servers/s1/channels/c4"), 1);
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>().invoke_fermer()
    });
}

fn exclusion(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
) {
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesMessagerie>().get_lecture_seule(),
        "j'écris dans le salon",
    );
    {
        let mut detail = serveur.detail_s1.lock().unwrap();
        let d = detail.as_mut().unwrap();
        d["permissions"] = json!(["view_channel"]);
        for m in d["members"].as_array_mut().unwrap() {
            if m["id"] == "u1" {
                m["timeout_until"] = json!("2099-01-01T00:00:00Z");
            }
        }
    }
    fabrique.envoyer(evenement(
        "member.timeout",
        json!({"server_id": "s1", "user_id": "u1", "until": "2099-01-01T00:00:00Z", "reason": "calme"}),
    ));
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesMessagerie>();
            g.get_lecture_seule()
                && g.get_lecture_seule_texte()
                    .starts_with("Tu es exclu temporairement de ce serveur jusqu'au ")
                && g.get_lecture_seule_texte().contains("2099")
        },
        "zone de saisie en lecture seule avec la fin de l'exclusion",
    );
    println!(
        "366 C2 : membre exclu en lecture seule : « {} »",
        sur_l_interface(faible, |f| f
            .global::<DonneesMessagerie>()
            .get_lecture_seule_texte()
            .to_string())
    );
}

fn scenario(
    faible: slint::Weak<Principale>,
    serveur: Arc<ServeurMessages>,
    coquille: Arc<Coquille>,
    fabrique: Arc<FausseFabrique>,
) {
    attendre_l_interface(
        &faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.global::<DonneesCoquille>().get_vue().titre == "général"
                && f.global::<DonneesCoquille>().get_membres().row_count() > 0
        },
        "salon ouvert",
    );
    attendre(|| coquille.pont().is_some(), "pont de la session");
    let pont = coquille.pont().unwrap();
    membres(&faible, &pont, &serveur);
    salon(&faible, &pont, &serveur);
    journal(&faible, &pont, &serveur);
    exclusion(&faible, &serveur, &fabrique);
}

#[test]
fn moderer_les_membres_et_les_salons() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(20);
    let etat = etat_de_moderation();
    *serveur.detail_s1.lock().unwrap() =
        Some(serde_json::to_value(etat.details.get("s1").unwrap()).unwrap());
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
    let fabrique = FausseFabrique::sur(etat, Vec::new(), &serveur.adresse);
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
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, serveur, c, fabrique)));
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
