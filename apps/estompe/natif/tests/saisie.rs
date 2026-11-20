mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::brouillons::Brouillons;
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesMessagerie, DonneesSaisie, GenreBandeau,
    LigneMessage, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

const NOMBRE: u64 = 300;

fn lignes(f: &Principale) -> Vec<LigneMessage> {
    f.global::<DonneesMessagerie>()
        .get_lignes()
        .iter()
        .collect()
}

fn derniere_ligne(f: &Principale) -> Option<LigneMessage> {
    lignes(f).into_iter().rev().find(|l| !l.id.is_empty())
}

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
    sur_l_interface(faible, move |f| {
        for c in texte.chars() {
            appuyer(f, c.to_string());
        }
    });
}

fn touche(faible: &slint::Weak<Principale>, touche: Key) {
    sur_l_interface(faible, move |f| appuyer(f, touche));
}

fn maj_entree(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        f.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        appuyer(f, Key::Return);
        f.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    });
}

fn texte(faible: &slint::Weak<Principale>) -> String {
    sur_l_interface(faible, |f| {
        f.global::<DonneesSaisie>().get_texte().to_string()
    })
}

fn cliquer_le_champ(faible: &slint::Weak<Principale>, indice: &'static str) {
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, indice)
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .expect("champ de saisie")
            .mock_single_click(PointerEventButton::Left);
    });
}

fn brouillon_du_salon(f: &Principale, salon: &str) -> bool {
    f.global::<DonneesCoquille>()
        .get_salons()
        .iter()
        .find(|s| s.id == salon)
        .is_some_and(|s| s.brouillon)
}

fn scenario(
    faible: slint::Weak<Principale>,
    serveur: Arc<ServeurMessages>,
    coquille: Arc<Coquille>,
    dossier: std::path::PathBuf,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille && f.global::<DonneesMessagerie>().get_active(),
        "vue des messages ouverte",
    );
    attendre(|| coquille.pont().is_some(), "pont de la session");
    let pont = coquille.pont().unwrap();
    attendre(
        || pont.messagerie().nombre_de_lignes() > 40,
        "première page chargée",
    );
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesSaisie>().get_indice() == "Écrire dans #général",
        "indice du champ",
    );

    cliquer_le_champ(&faible, "Écrire dans #général");
    taper(&faible, "Bonjour à tous");
    maj_entree(&faible);
    taper(&faible, "deuxième ligne");
    assert_eq!(texte(&faible), "Bonjour à tous\ndeuxième ligne");
    touche(&faible, Key::Return);
    assert_eq!(texte(&faible), "", "le champ se vide à l'envoi");
    attendre(
        || serveur.envois() == ["Bonjour à tous\ndeuxième ligne"],
        "message reçu par le serveur",
    );
    attendre_l_interface(
        &faible,
        |f| derniere_ligne(f).is_some_and(|l| l.id == "301" && l.etat_envoi.is_empty()),
        "message du serveur en bas de la liste",
    );
    attendre(|| pont.messagerie().colle(), "liste collée en bas");
    println!("314 C2 : envoi par Entrée, Maj+Entrée ajoute une ligne, message 301 en bas");

    touche(&faible, Key::UpArrow);
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesSaisie>();
            g.get_bandeau().genre == GenreBandeau::Modification
                && g.get_texte() == "Bonjour à tous\ndeuxième ligne"
        },
        "flèche haut : modification du dernier message",
    );
    touche(&faible, Key::Escape);
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesSaisie>();
            g.get_bandeau().genre == GenreBandeau::Aucun && g.get_texte().is_empty()
        },
        "Échap annule la modification",
    );
    touche(&faible, Key::UpArrow);
    sur_l_interface(&faible, |f| {
        f.global::<DonneesSaisie>()
            .set_texte("Bonjour à tous, modifié".into());
    });
    touche(&faible, Key::Return);
    attendre(
        || serveur.nombre_de("PATCH /messages/301") == 1,
        "modification envoyée",
    );
    attendre_l_interface(
        &faible,
        |f| {
            derniere_ligne(f)
                .is_some_and(|l| l.modifie && l.texte_brut == "Bonjour à tous, modifié")
        },
        "message modifié dans la liste",
    );

    *serveur.refus_d_envoi.lock().unwrap() = Some(403);
    taper(&faible, "pas le droit");
    touche(&faible, Key::Return);
    attendre_l_interface(
        &faible,
        |f| {
            derniere_ligne(f).is_some_and(|l| {
                l.envoi_echec
                    && l.etat_envoi.starts_with("Non envoyé")
                    && l.id.starts_with("local:")
            })
        },
        "message non envoyé avec sa raison",
    );
    let local = sur_l_interface(&faible, |f| derniere_ligne(f).unwrap().id.to_string());
    *serveur.refus_d_envoi.lock().unwrap() = None;
    sur_l_interface(&faible, move |f| {
        f.global::<DonneesMessagerie>()
            .invoke_reessayer_envoi(local.as_str().into())
    });
    attendre_l_interface(
        &faible,
        |f| derniere_ligne(f).is_some_and(|l| l.id == "302" && !l.envoi_echec),
        "réessayer envoie le message",
    );

    taper(&faible, "brouillon à garder");
    attendre_l_interface(
        &faible,
        |f| brouillon_du_salon(f, "c1"),
        "marque du brouillon dans le panneau",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_ouvrir_salon("c4".into())
    });
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesSaisie>().get_indice() == "Écrire dans #photos",
        "autre salon ouvert",
    );
    assert_eq!(texte(&faible), "");
    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_ouvrir_salon("c1".into())
    });
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesSaisie>().get_texte() == "brouillon à garder",
        "brouillon retrouvé au retour",
    );
    attendre(
        || Brouillons::charger(&dossier).texte("c:c1") == "brouillon à garder",
        "brouillon enregistré sur le disque pour le prochain lancement",
    );
    println!("314 C3 : brouillon retrouvé au retour dans le salon et relu depuis le disque");
}

#[test]
fn saisie_envoi_modification_et_brouillons() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(NOMBRE);
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
    let fabrique = FausseFabrique::sur(etat(), Vec::new(), &serveur.adresse);
    *fabrique.dossier_brouillons.lock().unwrap() = Some(dossier.path().to_path_buf());
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
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, serveur, c, chemin)));
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
