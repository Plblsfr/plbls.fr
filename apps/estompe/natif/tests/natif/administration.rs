use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{
    etat, etat_administrateur, png, servir_l_administration, FausseFabrique, ServeurMessages,
};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::admin::Droit;
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    confirmation, creer_fenetre, ecrans, DonneesAdmin, DonneesCoquille, DonneesReglagesServeur,
    Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::{json, Value};
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, Model};

const ENTREE: &str = "Administration d'Estompe";

fn requetes_admin(serveur: &ServeurMessages) -> Vec<String> {
    serveur
        .requetes
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.contains(" /admin"))
        .cloned()
        .collect()
}

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

fn entree_du_rail(f: &Principale) -> Option<ElementHandle> {
    ElementHandle::find_by_accessible_label(f, ENTREE)
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
}

fn onglets(f: &Principale) -> Vec<String> {
    f.global::<DonneesAdmin>()
        .get_onglets()
        .iter()
        .map(|o| o.id.to_string())
        .collect()
}

fn choisir(faible: &slint::Weak<Principale>, id: &'static str) {
    sur_l_interface(faible, move |f| {
        f.global::<DonneesAdmin>().invoke_choisir(id.into())
    });
}

fn confirmer(faible: &slint::Weak<Principale>) {
    attendre_l_interface(
        faible,
        |_| confirmation::ouverte().is_some(),
        "confirmation ouverte",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
}

fn administrer(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
) {
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_disponible() && entree_du_rail(f).is_some(),
        "entrée de l'administration dans le rail",
    );
    sur_l_interface(faible, |f| {
        entree_du_rail(f)
            .unwrap()
            .mock_single_click(PointerEventButton::Left)
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesAdmin>();
            g.get_ouverte()
                && g.get_onglet() == "utilisateurs"
                && g.get_titre() == "Utilisateurs"
                && g.get_comptes().row_count() == 2
        },
        "espace d'administration ouvert sur les utilisateurs",
    );
    sur_l_interface(faible, |f| {
        assert_eq!(
            onglets(f),
            [
                "utilisateurs",
                "serveurs",
                "badges",
                "signalements",
                "bot",
                "statistiques",
                "journaux"
            ]
        );
        let c = f
            .global::<DonneesAdmin>()
            .get_comptes()
            .row_data(0)
            .unwrap();
        assert_eq!(c.inscrit, "inscrit le 03/02/2026");
        assert_eq!(c.activite, "3 serveur(s) · 1\u{202f}520 message(s)");
        f.global::<DonneesAdmin>().set_recherche("ali ce".into());
        f.global::<DonneesAdmin>().invoke_chercher_comptes();
    });
    attendre(
        || {
            requetes_admin(serveur)
                .iter()
                .any(|r| r == "GET /admin/users?q=ali%20ce")
        },
        "recherche de comptes",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesAdmin>().invoke_basculer_suspension(0)
    });
    confirmer(faible);
    a_recu(serveur, "PUT /admin/users/u2/suspension");
    sur_l_interface(faible, |f| {
        f.global::<DonneesAdmin>().invoke_basculer_suspension(1)
    });
    a_recu(serveur, "DELETE /admin/users/u3/suspension");
    println!("368 : utilisateurs cherchés, suspendus (avec confirmation) et rétablis");

    choisir(faible, "serveurs");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_serveurs().row_count() == 2,
        "serveurs chargés",
    );
    sur_l_interface(faible, |f| {
        let s = f
            .global::<DonneesAdmin>()
            .get_serveurs()
            .row_data(1)
            .unwrap();
        assert_eq!((s.nom.as_str(), s.dernier.as_str()), ("Atelier", "—"));
        f.global::<DonneesAdmin>().invoke_supprimer_serveur(1);
    });
    attendre_l_interface(
        faible,
        |_| {
            confirmation::ouverte()
                .and_then(|d| d.saisie)
                .and_then(|s| s.exiger)
                .as_deref()
                == Some("Atelier")
        },
        "suppression confirmée par la saisie du nom",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
    a_recu(serveur, "DELETE /admin/servers/s9");
    println!("368 : serveur supprimé après saisie de son nom");

    choisir(faible, "badges");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_badges().row_count() == 3,
        "badges chargés",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        let noms = |g: &DonneesAdmin| -> Vec<String> {
            g.get_badges().iter().map(|b| b.nom.to_string()).collect()
        };
        assert_eq!(noms(&g), ["Fondateur", "Étoile", "Artiste"]);
        g.invoke_choisir_tri(2);
        assert_eq!(noms(&g), ["Fondateur", "Artiste", "Étoile"]);
        g.invoke_chercher_badges("ETOILE".into());
        assert_eq!(noms(&g), ["Étoile"]);
        g.invoke_chercher_badges("".into());
        g.invoke_choisir_tri(0);
        g.invoke_ouvrir_badge(0);
        assert!(g.get_badge_ouvert() && g.get_badge_existant());
        assert_eq!(g.get_badge_nom(), "Fondateur");
        assert_eq!(g.get_compte_detenteurs(), "2 détenteurs");
        g.set_recherche_detenteurs("ca".into());
        g.invoke_chercher_detenteurs("ca".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_propositions().row_count() == 1,
        "proposition sans les détenteurs actuels",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        assert_eq!(g.get_propositions().row_data(0).unwrap().pseudo, "marc");
        g.invoke_choisir_detenteur(0);
        assert_eq!(g.get_attribuer(), "Attribuer à 1 compte");
        g.invoke_attribuer_badge();
    });
    a_recu(serveur, "POST /admin/badges/b1/users");
    assert_eq!(
        corps(serveur, "POST /admin/badges/b1/users"),
        Some(json!({"pseudos": ["marc"]}))
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesAdmin>().invoke_retirer_detenteur(0)
    });
    a_recu(serveur, "DELETE /admin/badges/b1/users/alice");
    let image = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    std::fs::write(image.path(), png([40, 120, 200])).unwrap();
    *fabrique.fichiers.choix.lock().unwrap() = vec![image.path().to_path_buf()];
    sur_l_interface(faible, |f| {
        f.global::<DonneesAdmin>().invoke_choisir_image_badge()
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_badge_a_apercu(),
        "image choisie et recadrée",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        g.set_badge_couleur("vert".into());
        g.invoke_badge_modifie();
        assert!(!g.get_badge_valide());
        g.set_badge_couleur("#22aa55".into());
        g.invoke_badge_modifie();
        assert!(g.get_badge_valide());
        g.invoke_enregistrer_badge();
    });
    a_recu(serveur, "PUT /admin/badges/b1/icon");
    assert_eq!(
        corps(serveur, "PUT /admin/badges/b1"),
        Some(
            json!({"name": "Fondateur", "color": "#22aa55", "description": "Présent depuis le premier jour", "sort_order": 1})
        )
    );
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesAdmin>().get_badge_ouvert(),
        "badge enregistré et fenêtre refermée",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        g.invoke_nouveau_badge();
        assert!(!g.get_badge_existant());
        assert_eq!(g.get_badge_couleur(), "#3b82f6");
        g.set_badge_nom("Pionnier".into());
        g.set_badge_ordre("12".into());
        g.invoke_badge_modifie();
        g.invoke_enregistrer_badge();
    });
    a_recu(serveur, "POST /admin/badges");
    assert_eq!(
        corps(serveur, "POST /admin/badges"),
        Some(json!({"name": "Pionnier", "color": "#3b82f6", "description": "", "sort_order": 12}))
    );
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesAdmin>().get_badge_ouvert(),
        "badge créé",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        g.invoke_ouvrir_badge(1);
        g.invoke_supprimer_badge();
    });
    attendre_l_interface(
        faible,
        |_| {
            confirmation::ouverte().is_some_and(|d| {
                d.message == "Supprimer le badge « Étoile » ? Personne ne le détient. Cette action est définitive."
            })
        },
        "suppression du badge confirmée",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
    a_recu(serveur, "DELETE /admin/badges/b2");
    println!("368 : badges triés, cherchés, créés, modifiés (image recadrée), attribués, retirés et supprimés");

    choisir(faible, "bot");
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesAdmin>()
                .get_versions_envoyees()
                .row_count()
                == 2
        },
        "notes de version envoyées",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        g.set_bot_message("Maintenance ce soir".into());
        g.invoke_envoyer_bot();
    });
    attendre_l_interface(
        faible,
        |_| {
            confirmation::ouverte()
                .is_some_and(|d| d.message == "Envoyer ce message en privé à tous les comptes ?")
        },
        "confirmation d'envoi à tous",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
    a_recu(serveur, "POST /admin/bot/messages");
    assert_eq!(
        corps(serveur, "POST /admin/bot/messages"),
        Some(json!({"content": "Maintenance ce soir", "to": ""}))
    );
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesAdmin>().get_info() == "0 message(s) envoyé(s)",
        "envoi annoncé",
    );

    choisir(faible, "statistiques");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesAdmin>();
            g.get_tuiles().row_count() == 4 && g.get_a_direct() && g.get_a_vue()
        },
        "statistiques, direct et configuration",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        let erreurs = g.get_tuiles().row_data(3).unwrap();
        assert!(erreurs.danger && erreurs.lien);
        assert_eq!(g.get_connexions(), "42 connexion(s) temps réel");
        let vocal = g.get_vocaux().row_data(0).unwrap();
        assert_eq!(vocal.titre, "Les amis › Salon vocal");
        assert_eq!(
            vocal
                .participants
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>(),
            ["alice · écran", "bob · caméra"]
        );
        g.invoke_choisir("journaux".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesAdmin>();
            g.get_actions().row_count() == 2 && g.get_erreurs().row_count() == 1
        },
        "journaux et erreurs",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesAdmin>();
        assert_eq!(g.get_actions().row_data(1).unwrap().cible, "—");
        assert_eq!(
            g.get_erreurs().row_data(0).unwrap().requete,
            "GET /servers/s1"
        );
        g.invoke_choisir("signalements".into());
        assert_eq!(g.get_onglet(), "signalements");
        g.invoke_choisir("acces".into());
        assert_eq!(g.get_onglet(), "utilisateurs");
        g.invoke_deplacer(1);
        assert_eq!(g.get_onglet(), "serveurs");
    });
    println!(
        "368 : bot, statistiques en direct, journaux ; onglet des signalements réservé à #367"
    );

    sur_l_interface(faible, |f| {
        f.global::<DonneesCoquille>().invoke_ouvrir_accueil()
    });
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesAdmin>().get_ouverte(),
        "la navigation referme l'administration",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesAdmin>().invoke_ouvrir();
        assert!(f.global::<DonneesAdmin>().get_ouverte());
        f.global::<DonneesReglagesServeur>()
            .invoke_ouvrir("s1".into());
        assert!(!f.global::<DonneesAdmin>().get_ouverte());
        assert!(f.global::<DonneesReglagesServeur>().get_ouverte());
    });
    println!("368 : toute navigation referme l'espace d'administration");
}

fn sans_droit(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    attendre_l_interface(
        faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.global::<DonneesCoquille>().get_pret()
                && f.global::<DonneesCoquille>().get_serveurs().row_count() > 0
        },
        "coquille prête",
    );
    sur_l_interface(faible, |f| {
        assert!(!f.global::<DonneesAdmin>().get_disponible());
        assert!(entree_du_rail(f).is_none());
        f.global::<DonneesAdmin>().invoke_ouvrir();
        assert!(!f.global::<DonneesAdmin>().get_ouverte());
        f.global::<DonneesAdmin>()
            .invoke_choisir("utilisateurs".into());
        assert!(!f.global::<DonneesAdmin>().get_ouverte());
    });
    assert_eq!(requetes_admin(serveur), Vec::<String>::new());
    println!(
        "368 C2 : un compte sans le droit n'a ni l'entrée ni les données (aucun appel /admin)"
    );
}

fn seule(faible: &slint::Weak<Principale>, auth: &FausseAuth) {
    attendre_l_interface(
        faible,
        |f| {
            f.get_page() == Page::Administration
                && f.global::<DonneesAdmin>().get_seule()
                && f.global::<DonneesAdmin>().get_ouverte()
        },
        "coquille limitée à l'administration",
    );
    sur_l_interface(faible, |f| {
        assert_eq!(onglets(f), ["acces"]);
        assert_eq!(f.global::<DonneesAdmin>().get_onglet(), "acces");
        assert!(!f.global::<DonneesAdmin>().get_disponible());
        ElementHandle::find_by_accessible_label(f, "Se déconnecter")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("bouton de déconnexion")
            .mock_single_click(PointerEventButton::Left);
    });
    attendre_l_interface(
        faible,
        |f| f.get_page() == Page::Connexion && !f.global::<DonneesAdmin>().get_seule(),
        "déconnexion depuis l'administration seule",
    );
    assert_eq!(
        auth.deconnexions.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    println!("368 : un compte limité à l'administration ouvre la coquille d'administration");
}

enum Cas {
    Administrateur,
    SansDroit,
    Seule,
}

fn lancer(cas: Cas) {
    let serveur = ServeurMessages::demarrer(20);
    servir_l_administration(&serveur);
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None);
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let api = FausseApi::compte_actif();
    if matches!(cas, Cas::Seule) {
        let mut moi = api.moi.lock().unwrap();
        moi.allowed = Some(false);
        moi.activated = Some(false);
        moi.rights = Some(vec![Droit::Acces]);
    }
    let auth = FausseAuth::avec_session(true);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(auth.clone(), api, Arc::new(FauxNavigateur::default())),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let etat = match cas {
        Cas::SansDroit => etat(),
        _ => etat_administrateur(),
    };
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
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| match cas {
            Cas::Administrateur => administrer(&faible, &serveur, &fabrique),
            Cas::SansDroit => sans_droit(&faible, &serveur),
            Cas::Seule => seule(&faible, &auth),
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
fn administrer_estompe() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    lancer(Cas::SansDroit);
    lancer(Cas::Seule);
    lancer(Cas::Administrateur);
}
