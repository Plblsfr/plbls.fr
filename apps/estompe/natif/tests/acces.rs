mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use commun::acces::{comptes_de_base, comptes_nombreux, AegisFactice};
use commun::coquille::{etat_administrateur, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::admin::Droit;
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    confirmation, creer_fenetre, ecrans, menu, DonneesAcces, DonneesAdmin, Principale,
};
use i_slint_backend_testing::ElementHandle;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model};

const NOMBRE: usize = 10_000;
const LIGNES_MAX: usize = 40;
const LATENCE_MAX: Duration = Duration::from_millis(400);

fn corps(serveur: &ServeurMessages, ligne: &str) -> Option<Value> {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(r, _)| r == ligne)
        .map(|(_, c)| serde_json::from_str(c).unwrap_or_default())
}

fn a_recu(serveur: &ServeurMessages, ligne: &'static str) {
    attendre(
        || {
            serveur
                .corps
                .lock()
                .unwrap()
                .iter()
                .any(|(r, _)| r == ligne)
        },
        ligne,
    );
}

fn a_demande(serveur: &ServeurMessages, debut: &'static str) {
    attendre(
        || {
            serveur
                .requetes
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.starts_with(debut))
        },
        debut,
    );
}

fn ouvrir_les_acces(faible: &slint::Weak<Principale>) {
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_disponible(),
        "administration disponible",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        g.invoke_ouvrir();
        g.invoke_choisir("acces".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            f.global::<DonneesAdmin>().get_onglet() == "acces"
                && a.get_catalogue().charge
                && a.get_comptes_etat().charge
                && a.get_comptes().row_count() > 0
        },
        "onglet des accès chargé",
    );
}

fn libelles(modele: slint::ModelRc<estompe_natif::OptionAcces>) -> Vec<String> {
    modele.iter().map(|c| c.libelle.to_string()).collect()
}

fn puces(f: &Principale) -> Vec<String> {
    f.global::<DonneesAcces>()
        .get_rangees_puces()
        .iter()
        .flat_map(|r| r.puces.iter().collect::<Vec<_>>())
        .map(|p| format!("{} {}", p.cle, p.nom).trim().to_string())
        .collect()
}

fn gerer(faible: &slint::Weak<Principale>, serveur: &ServeurMessages, aegis: &AegisFactice) {
    ouvrir_les_acces(faible);
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        let noms: Vec<String> = a.get_comptes().iter().map(|c| c.nom.to_string()).collect();
        assert_eq!(noms, ["alice@exemple.fr", "bob@exemple.fr", "Carol"]);
        let carol = a.get_comptes().row_data(2).unwrap();
        assert!(carol.supprime);
        assert_eq!(carol.initiales, "C");
        assert_eq!(a.get_comptes().row_data(1).unwrap().resume, "1 refus");
        a.invoke_ouvrir_compte("s-bob".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            a.get_fiche_etat().charge && a.get_effectifs_etat().charge
        },
        "fiche de bob",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        assert_eq!(a.get_fiche_nom(), "bob@exemple.fr");
        assert!(a.get_comptes().row_data(1).unwrap().choisi);
        assert_eq!(puces(f), ["Tout le monde", "refus direct Créer un serveur"]);
        let filtres: Vec<String> = a.get_filtres().iter().map(|s| s.to_string()).collect();
        assert_eq!(
            filtres,
            [
                "Ce qui le distingue · 1",
                "Autorisées · 2",
                "Refusées · 1",
                "Toutes · 5"
            ]
        );
        let droit = a.get_droits().row_data(1).unwrap();
        assert_eq!(droit.libelle, "Créer un serveur");
        assert_eq!(droit.phrase, "refusé par règle directe");
        a.invoke_pourquoi("estompe:servers.create".into());
    });
    a_recu(serveur, "POST /admin/acces/simulate");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_a_verdict(),
        "verdict de la simulation",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        let chaine: Vec<String> = a.get_chaine().iter().map(|m| m.texte.to_string()).collect();
        assert_eq!(
            chaine,
            ["Tout le monde · partout", "règle directe · partout"]
        );
        a.invoke_choisir_filtre(3);
        assert_eq!(a.get_droits().iter().filter(|d| !d.en_tete).count(), 5);
        a.invoke_basculer_formulaire("regle".into());
        a.set_regle_recherche("gif".into());
        a.invoke_chercher_permissions("gif".into());
        assert_eq!(
            libelles(a.get_permissions_trouvees()),
            ["Messages · Utiliser les GIF"]
        );
        a.invoke_choisir_permission(0);
        assert!(a.get_regle_a_type());
        assert_eq!(a.get_regle_ressource(), "partout");
    });
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesAcces>()
                .get_ressources_trouvees()
                .row_count()
                == 2
        },
        "ressources proposées pour la portée",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.invoke_choisir_ressource(0);
        assert_eq!(a.get_regle_ressource(), "Les amis");
        a.set_regle_note("ticket 42".into());
        a.set_regle_expire("31/12/2026 23:30".into());
        a.invoke_ajouter_regle();
    });
    a_recu(serveur, "POST /admin/acces/rules");
    let mut envoye = corps(serveur, "POST /admin/acces/rules").unwrap();
    let expire = envoye["expires_at"].take();
    let expire = expire.as_str().unwrap();
    assert!(
        (expire.starts_with("2026-12-31T") || expire.starts_with("2027-01-01T"))
            && expire.ends_with(":30:00.000Z"),
        "{expire}"
    );
    assert_eq!(
        envoye,
        json!({
            "subject_kind": "account", "subject_id": "s-bob", "permission": "estompe:gifs.use",
            "resource": "s1", "effect": "allow", "expires_at": null, "note": "ticket 42"
        })
    );
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesAcces>().get_message() == "Permission accordée."
                && puces(f).contains(&"accord direct Utiliser les GIF".to_string())
        },
        "règle ajoutée à la fiche",
    );
    println!("369 : règle directe ajoutée depuis la fiche (permission, portée, expiration, note)");

    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>()
            .invoke_choisir_onglet("journal".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            a.get_journal_etat().charge && a.get_journal().row_count() == 3
        },
        "journal chargé",
    );
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesAcces>()
                .get_journal()
                .row_data(0)
                .is_some_and(|l| l.ressource == "sur Les amis")
        },
        "nom de la ressource résolu",
    );
    sur_l_interface(faible, |f| {
        let l = f
            .global::<DonneesAcces>()
            .get_journal()
            .row_data(0)
            .unwrap();
        assert_eq!(l.action, "permission accordée");
        assert_eq!(l.ton, "ok");
        assert_eq!(l.cible, "Utiliser les GIF");
        assert_eq!(l.pour, "bob@exemple.fr");
        assert_eq!(l.acteur, "moi@exemple.fr");
        assert_eq!(l.extra, "jusqu'au 31/12/2026 23:30 · « ticket 42 »");
        let manifeste = f
            .global::<DonneesAcces>()
            .get_journal()
            .row_data(2)
            .unwrap();
        assert_eq!(manifeste.acteur, "manifeste:estompe");
        assert_eq!(
            manifeste.extra,
            "+ estompe:vocal.screen · − estompe:ancienne"
        );
    });
    println!("369 C2 : la règle ajoutée apparaît en tête du journal des accès");

    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>()
            .invoke_ouvrir_choix("action".into(), 10.0, 10.0, 40.0)
    });
    attendre_l_interface(
        faible,
        |f| f.global::<estompe_natif::DonneesMenu>().get_ouvert(),
        "menu des actions",
    );
    sur_l_interface(faible, |f| {
        assert_eq!(
            f.global::<estompe_natif::DonneesMenu>()
                .get_entrees()
                .row_count(),
            20
        );
        menu::choisir(f, "rule.deny");
    });
    a_demande(serveur, "GET /admin/acces/audit?limit=300&action=rule.deny");
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            a.get_action() == "permission refusée"
                && a.get_journal_etat().charge
                && a.get_journal().row_count() == 0
        },
        "journal filtré par action",
    );

    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.invoke_choisir_onglet("comptes".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_fiche_etat().charge,
        "retour à la fiche",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>().invoke_basculer_historique()
    });
    a_demande(serveur, "GET /admin/acces/audit?subject=s-bob&limit=200");
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            a.get_historique_etat().charge && a.get_historique_lignes().row_count() == 1
        },
        "historique du compte",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.invoke_basculer_historique();
        let i = puces(f)
            .iter()
            .position(|p| p == "accord direct Utiliser les GIF")
            .unwrap();
        a.invoke_retirer_puce(i as i32);
    });
    attendre_l_interface(
        faible,
        |_| confirmation::ouverte().is_some(),
        "confirmation du retrait",
    );
    sur_l_interface(faible, |f| {
        assert_eq!(
            confirmation::ouverte().unwrap().message,
            "Retirer « Utiliser les GIF » ?"
        );
        confirmation::repondre(f, true);
    });
    attendre(
        || {
            serveur
                .requetes
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.starts_with("DELETE /admin/acces/rules/r"))
        },
        "règle supprimée",
    );
    println!("369 : historique du compte et retrait d'une règle avec confirmation");

    aegis.refuser("/roles", true);
    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>()
            .invoke_choisir_onglet("roles".into())
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_roles_etat().erreur == "aegis est injoignable",
        "erreur de chargement des rôles",
    );
    aegis.refuser("/roles", false);
    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>().invoke_recharger("roles".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            a.get_roles_etat().charge && a.get_colonnes().row_count() == 3
        },
        "rôles rechargés après « Réessayer »",
    );
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesAcces>()
                .get_grille()
                .iter()
                .any(|l| l.cases.iter().any(|c| c.effet == "deny"))
        },
        "grille des rôles remplie",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        let ligne = a
            .get_grille()
            .iter()
            .find(|l| l.permission == "estompe:vocal.screen")
            .unwrap();
        let modo = ligne.cases.row_data(2).unwrap();
        assert_eq!(modo.effet, "allow");
        assert_eq!(modo.exceptions, "1 exception");
        assert_eq!(
            modo.description,
            "Modération autorise Partager son écran, 1 exception"
        );
        a.invoke_ouvrir_role("r-modo".into(), "estompe:vocal.screen".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_role_etat().charge,
        "éditeur du rôle",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        assert_eq!(a.get_role_nom(), "Modération");
        assert!(a.get_role_cible() >= 0);
        assert!(!a.get_role_modifie());
        a.invoke_regler("estompe:access".into(), 2);
        assert!(a.get_role_modifie());
        assert_eq!(
            a.get_role_resume(),
            "1 modification non enregistrée · Utiliser Estompe"
        );
        a.invoke_enregistrer_role();
    });
    a_recu(serveur, "PUT /admin/acces/roles/r-modo");
    let envoye = corps(serveur, "PUT /admin/acces/roles/r-modo").unwrap();
    assert_eq!(envoye["name"], "Modération");
    assert!(envoye["rules"]
        .as_array()
        .unwrap()
        .contains(&json!({"permission": "estompe:access", "resource": "*", "effect": "deny"})));
    assert!(envoye["rules"].as_array().unwrap().contains(
        &json!({"permission": "estompe:vocal.screen", "resource": "s9", "effect": "deny"})
    ));
    println!(
        "369 : erreur et « Réessayer » d'une liste, grille des rôles et enregistrement d'un rôle"
    );

    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>()
            .invoke_choisir_onglet("groupes".into())
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_groupes().row_count() == 2,
        "groupes",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>().invoke_choisir_groupe(0)
    });
    attendre_l_interface(
        faible,
        |f| {
            let a = f.global::<DonneesAcces>();
            a.get_groupe_etat().charge && a.get_membres().row_count() == 2
        },
        "détail du groupe",
    );
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesAcces>()
                .get_liaisons()
                .row_data(0)
                .is_some_and(|l| l.portee == "Les amis")
        },
        "portée nommée",
    );
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.set_recherche_compte("bob".into());
        a.invoke_chercher_compte("bob".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_comptes_trouves().row_count() == 1,
        "compte proposé",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesAcces>().invoke_choisir_compte(0)
    });
    a_recu(serveur, "PUT /admin/acces/groups/g-equipe/members/s-bob");
    println!("369 : groupe ouvert, membre ajouté par le choix d'un compte");

    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.invoke_choisir_onglet("catalogue".into());
        assert_eq!(
            a.get_obsoletes(),
            "1 permission(s) ne sont plus déclarées par leur application : leurs règles sont gardées mais ignorées."
        );
        a.invoke_deplier("estompe:gifs.use".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAcces>().get_detenteurs().row_count() == 2,
        "détenteurs d'une permission",
    );
    sur_l_interface(faible, |f| {
        let d = f
            .global::<DonneesAcces>()
            .get_detenteurs()
            .row_data(0)
            .unwrap();
        assert_eq!(
            (d.sujet.as_str(), d.nom.as_str()),
            ("rôle", "Tout le monde")
        );
    });
    println!("369 : catalogue, permissions obsolètes et détenteurs");
}

fn latence(faible: &slint::Weak<Principale>) -> Duration {
    let debut = Instant::now();
    sur_l_interface(faible, |_| ());
    debut.elapsed()
}

fn volume(faible: &slint::Weak<Principale>, aegis: &AegisFactice) {
    let _ = aegis;
    let debut = Instant::now();
    ouvrir_les_acces(faible);
    let mut pire_latence = Duration::ZERO;
    let mut pire_etape = Duration::ZERO;
    let mut lignes_max = 0;
    let mut pages = 1;
    loop {
        let (n, total) = sur_l_interface(faible, |f| {
            let a = f.global::<DonneesAcces>();
            (a.get_comptes().row_count(), NOMBRE)
        });
        if n >= total {
            break;
        }
        let t = Instant::now();
        sur_l_interface(faible, move |f| {
            let a = f.global::<DonneesAcces>();
            a.set_defilement_comptes(-(n as f32 * 52.0 - 560.0).max(0.0));
            slint::platform::update_timers_and_animations();
            let _ = f.window().size();
        });
        let duree = t.elapsed();
        let lignes = sur_l_interface(faible, |f| {
            ElementHandle::find_by_element_type_name(f, "LigneCompte").count()
        });
        pire_etape = pire_etape.max(duree);
        lignes_max = lignes_max.max(lignes);
        let limite = Instant::now() + commun::DELAI;
        loop {
            pire_latence = pire_latence.max(latence(faible));
            let m = sur_l_interface(faible, |f| {
                f.global::<DonneesAcces>().get_comptes().row_count()
            });
            if m > n {
                pages += 1;
                break;
            }
            assert!(
                Instant::now() < limite,
                "page suivante non chargée après {n} comptes"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let (n, dernier) = sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.set_defilement_comptes(-(NOMBRE as f32 * 52.0 - 560.0));
        slint::platform::update_timers_and_animations();
        let visibles: Vec<String> = ElementHandle::find_by_element_type_name(f, "LigneCompte")
            .filter_map(|e| e.accessible_label().map(|s| s.to_string()))
            .collect();
        (
            a.get_comptes().row_count(),
            visibles.iter().any(|v| v == "compte09999@exemple.fr"),
        )
    });
    let (lignes, hauteur) = sur_l_interface(faible, |f| {
        let hauteur = ElementHandle::find_by_accessible_label(f, "Annuaire des comptes")
            .next()
            .map_or(0.0, |e| e.size().height);
        (
            ElementHandle::find_by_element_type_name(f, "LigneCompte").count(),
            hauteur,
        )
    });
    assert_eq!(hauteur, 560.0);
    lignes_max = lignes_max.max(lignes);
    let t = Instant::now();
    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAcces>();
        a.invoke_ouvrir_compte("s-09999".into());
    });
    let ouverture = t.elapsed();
    println!(
        "369 C3 : {n} comptes en {pages} pages ({:.1} s) ; lignes instanciées au plus {lignes_max} ; pire latence de l'interface {:.1} ms ; pire étape de défilement {:.1} ms ; ouverture d'une fiche {:.1} ms",
        debut.elapsed().as_secs_f64(),
        pire_latence.as_secs_f64() * 1000.0,
        pire_etape.as_secs_f64() * 1000.0,
        ouverture.as_secs_f64() * 1000.0
    );
    assert_eq!(n, NOMBRE);
    assert!(dernier, "le dernier compte est affiché en bas de la liste");
    assert!(lignes_max <= LIGNES_MAX, "{lignes_max} lignes instanciées");
    assert!(
        pire_latence < LATENCE_MAX,
        "interface figée {pire_latence:?}"
    );
    assert!(pire_etape < LATENCE_MAX, "défilement figé {pire_etape:?}");
}

enum Cas {
    Gestion,
    Volume,
}

fn lancer(cas: Cas) {
    let serveur = ServeurMessages::demarrer(20);
    commun::coquille::servir_l_administration(&serveur);
    let aegis = AegisFactice::nouveau(match cas {
        Cas::Gestion => comptes_de_base(),
        Cas::Volume => comptes_nombreux(NOMBRE),
    });
    aegis.brancher(&serveur);
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
    let mut etat = etat_administrateur();
    etat.compte.rights = Some(vec![Droit::Access, Droit::Admin, Droit::Acces]);
    let fabrique = FausseFabrique::sur(etat, Vec::new(), &serveur.adresse);
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let coquille = Arc::new(Coquille::brancher(
        &fenetre,
        &entree,
        fabrique,
        memoire,
        Arc::new(move || midi),
    ));
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| match cas {
            Cas::Gestion => gerer(&faible, &serveur, &aegis),
            Cas::Volume => volume(&faible, &aegis),
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

#[test]
fn administrer_les_acces() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    lancer(Cas::Gestion);
    lancer(Cas::Volume);
}
