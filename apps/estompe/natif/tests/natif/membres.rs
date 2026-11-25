use crate::commun;

use std::collections::BTreeSet;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_nombreux, evenement, FausseFabrique};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::modeles::Operation;
use estompe_natif::coquille::{CleAvatar, Coquille, Pont};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, EtatPresence, LigneMembre, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, Model};

const NOMBRE: usize = 5000;
const BORNE_PAR_FENETRE: usize = 40;

fn lignes(f: &Principale) -> Vec<LigneMembre> {
    f.global::<DonneesCoquille>().get_membres().iter().collect()
}

fn avec_portrait(f: &Principale) -> BTreeSet<String> {
    lignes(f)
        .into_iter()
        .filter(|l| l.portrait.size().width > 0)
        .map(|l| l.utilisateur.to_string())
        .collect()
}

fn lignes_instanciees(f: &Principale) -> usize {
    ElementHandle::find_by_element_type_name(f, "LigneMembreVue").count()
}

fn pomper(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        ElementHandle::find_by_element_type_name(f, "LigneMembreVue").count();
        slint::platform::update_timers_and_animations();
    });
}

fn attendre_en_pompant(
    faible: &slint::Weak<Principale>,
    mut condition: impl FnMut() -> bool,
    quoi: &str,
) {
    attendre(
        || {
            pomper(faible);
            condition()
        },
        quoi,
    );
}

fn demandes(fabrique: &FausseFabrique) -> Vec<CleAvatar> {
    fabrique.source.requetes_avatars()
}

fn membres_de_la_fenetre(faible: &slint::Weak<Principale>, pont: &Pont) -> BTreeSet<String> {
    let fenetre = pont.fenetre_membres();
    sur_l_interface(faible, move |f| {
        lignes(f)[fenetre.debut..fenetre.fin]
            .iter()
            .filter(|l| !l.en_tete)
            .map(|l| l.utilisateur.to_string())
            .collect()
    })
}

fn attendre_la_fenetre_chargee(
    faible: &slint::Weak<Principale>,
    pont: &Pont,
    quoi: &str,
) -> BTreeSet<String> {
    attendre_en_pompant(
        faible,
        || {
            let visibles = membres_de_la_fenetre(faible, pont);
            let portraits = sur_l_interface(faible, avec_portrait);
            !visibles.is_empty() && portraits == visibles
        },
        quoi,
    );
    membres_de_la_fenetre(faible, pont)
}

fn defiler(faible: &slint::Weak<Principale>, decalage: f32) {
    sur_l_interface(faible, move |f| {
        f.global::<DonneesCoquille>()
            .set_defilement_membres(-decalage)
    });
}

fn cliquer(faible: &slint::Weak<Principale>, libelle: &'static str) {
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"))
            .mock_single_click(PointerEventButton::Left);
    });
}

fn scenario(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    coquille: Arc<Coquille>,
    memoire: Memoire,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille && lignes(f).len() == NOMBRE + 2,
        "5 000 membres et leurs deux groupes dans le modèle",
    );
    attendre(|| coquille.pont().is_some(), "pont de la session");
    let pont = coquille.pont().unwrap();

    let premiere = attendre_la_fenetre_chargee(&faible, &pont, "avatars de la première fenêtre");
    let ouverture = demandes(&fabrique);
    let instanciees = sur_l_interface(&faible, lignes_instanciees);
    println!(
        "C2 ouverture : {} lignes dans le modèle, {} lignes instanciées, {} requêtes d'avatar",
        NOMBRE + 2,
        instanciees,
        ouverture.len()
    );
    assert!(
        instanciees > 0 && instanciees <= BORNE_PAR_FENETRE,
        "{instanciees} lignes instanciées"
    );
    let demandes_ouverture: BTreeSet<String> =
        ouverture.iter().map(|c| c.utilisateur.clone()).collect();
    println!(
        "C2 ouverture : fenêtre de {} membres, demandés en plus pendant la mise en page : {:?}",
        premiere.len(),
        demandes_ouverture.difference(&premiere).collect::<Vec<_>>()
    );
    assert_eq!(
        demandes_ouverture.len(),
        ouverture.len(),
        "aucun avatar demandé deux fois"
    );
    assert!(demandes_ouverture.is_superset(&premiere));
    assert!(
        ouverture.len() <= BORNE_PAR_FENETRE,
        "{} requêtes",
        ouverture.len()
    );

    defiler(&faible, 3000.0 * 44.0);
    attendre_en_pompant(
        &faible,
        || pont.fenetre_membres().debut > 2900,
        "fenêtre déplacée par le défilement",
    );
    let loin = attendre_la_fenetre_chargee(&faible, &pont, "avatars après défilement");
    let apres = demandes(&fabrique);
    let nouvelles: BTreeSet<String> = apres[ouverture.len()..]
        .iter()
        .map(|c| c.utilisateur.clone())
        .collect();
    println!(
        "C2 défilement : fenêtre {:?}, {} nouvelles requêtes ({} au total), {} lignes instanciées, {} portraits gardés dans le modèle",
        pont.fenetre_membres().debut..pont.fenetre_membres().fin,
        apres.len() - ouverture.len(),
        apres.len(),
        sur_l_interface(&faible, lignes_instanciees),
        sur_l_interface(&faible, avec_portrait).len()
    );
    assert!(
        nouvelles.is_superset(&loin),
        "les nouvelles lignes visibles"
    );
    assert!(nouvelles.is_disjoint(&premiere));
    assert!(apres.len() - ouverture.len() <= BORNE_PAR_FENETRE);
    assert!(apres.len() <= 2 * BORNE_PAR_FENETRE);
    assert!(sur_l_interface(&faible, lignes_instanciees) <= BORNE_PAR_FENETRE);
    assert_eq!(
        sur_l_interface(&faible, avec_portrait),
        loin,
        "portraits libérés hors fenêtre"
    );

    defiler(&faible, 0.0);
    attendre_en_pompant(
        &faible,
        || pont.fenetre_membres().debut == 0,
        "retour en haut de la liste",
    );
    let retour = attendre_la_fenetre_chargee(&faible, &pont, "avatars repris du cache");
    assert_eq!(retour, premiere);
    assert_eq!(
        demandes(&fabrique).len(),
        apres.len(),
        "aucune nouvelle requête : cache"
    );
    println!(
        "C2 retour en haut : {} requêtes au total (inchangé)",
        demandes(&fabrique).len()
    );

    let absent = "m00003";
    fabrique.envoyer(evenement(
        "status.updated",
        json!({"user_id": absent, "status": "idle"}),
    ));
    attendre_l_interface(
        &faible,
        move |f| {
            lignes(f)
                .iter()
                .any(|l| l.utilisateur == absent && l.presence == EtatPresence::Absent)
        },
        "statut absent reçu en temps réel",
    );
    let operations = pont.operations_membres();
    println!("C3 absent : {operations:?}");
    assert!(matches!(operations.as_slice(), [(Operation::Remplacer(_), cle)] if cle == "m:m00003"));

    let arrive = "m00001";
    let rang_avant = sur_l_interface(&faible, move |f| {
        lignes(f)
            .iter()
            .position(|l| l.utilisateur == arrive)
            .unwrap()
    });
    fabrique.envoyer(evenement(
        "presence",
        json!({"user_id": arrive, "online": true}),
    ));
    attendre_l_interface(
        &faible,
        move |f| {
            let l = lignes(f);
            let hors_ligne = l.iter().position(|l| l.cle == "g:hors-ligne").unwrap();
            l.iter()
                .position(|l| l.utilisateur == arrive)
                .is_some_and(|i| i < hors_ligne && l[i].presence == EtatPresence::EnLigne)
        },
        "membre passé en ligne en temps réel",
    );
    let operations = pont.operations_membres();
    println!("C3 en ligne (rang {rang_avant} avant) : {operations:?}");
    let deplacements: Vec<&(Operation, String)> = operations
        .iter()
        .filter(|(o, _)| !matches!(o, Operation::Remplacer(_)))
        .collect();
    assert_eq!(deplacements.len(), 2, "{operations:?}");
    assert!(deplacements.iter().all(|(_, cle)| cle == "m:m00001"));
    assert!(matches!(deplacements[0].0, Operation::Retirer(_)));
    assert!(matches!(deplacements[1].0, Operation::Inserer(_)));
    assert!(operations
        .iter()
        .filter(|(o, _)| matches!(o, Operation::Remplacer(_)))
        .all(|(_, cle)| cle.starts_with("g:")));
    assert_eq!(sur_l_interface(&faible, |f| lignes(f).len()), NOMBRE + 2);

    cliquer(&faible, "Fermer la marge");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesCoquille>().get_marge_ouverte(),
        "marge fermée",
    );
    assert!(!memoire.lire(|p| p.marge_ouverte));
    assert_eq!(sur_l_interface(&faible, lignes_instanciees), 0);
    cliquer(&faible, "Membres");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_marge_ouverte(),
        "marge rouverte par le bouton de l'en-tête",
    );
    assert!(memoire.lire(|p| p.marge_ouverte));
    attendre_en_pompant(
        &faible,
        || sur_l_interface(&faible, lignes_instanciees) > 0,
        "liste instanciée à nouveau",
    );
    let toutes = demandes(&fabrique);
    println!(
        "C2 réouverture de la marge : {} requêtes au total, nouvelles : {:?}",
        toutes.len(),
        &toutes[apres.len()..]
    );
    let distinctes: BTreeSet<&CleAvatar> = toutes.iter().collect();
    assert_eq!(distinctes.len(), toutes.len(), "aucun avatar redemandé");
    assert!(toutes.len() - apres.len() <= 2);

    cliquer(&faible, "membre 00003");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesCoquille>().get_carte_ouverte(),
        "carte ouverte",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_ouvrir_membre("m00003".into())
    });
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesCoquille>();
            g.get_carte_ouverte()
                && g.get_carte().nom == "membre 00003"
                && g.get_carte().statut == "Absent"
        },
        "carte de profil ouverte au clic",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>().invoke_fermer_carte()
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesCoquille>().get_carte_ouverte(),
        "carte fermée",
    );
}

#[test]
fn c2_c3_la_liste_des_membres_est_virtualisee_et_suit_le_temps_reel() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
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
    let fabrique = FausseFabrique::nouvelle(etat_nombreux(NOMBRE), Vec::new());
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let coquille = Arc::new(Coquille::brancher(
        &fenetre,
        &entree,
        fabrique.clone(),
        memoire.clone(),
        Arc::new(move || midi),
    ));
    fenetre.show().unwrap();

    let faible = fenetre.as_weak();
    let c = coquille.clone();
    let m = memoire.clone();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, fabrique, c, m)));
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
