mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{
    contenu_de, etat, evenement, message, message_numero, FausseFabrique, ServeurMessages,
};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::store::CleConversation;
use estompe_natif::coquille::modeles::Operation;
use estompe_natif::coquille::{Coquille, Pont};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesMessagerie, LigneMessage, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, Model};

const NOMBRE: u64 = 300;
const NON_LUS: i64 = 40;

fn lignes(f: &Principale) -> Vec<LigneMessage> {
    f.global::<DonneesMessagerie>()
        .get_lignes()
        .iter()
        .collect()
}

fn pomper(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        ElementHandle::find_by_element_type_name(f, "LigneMessageVue").count();
        slint::platform::update_timers_and_animations();
    });
}

fn instantanee(faible: &slint::Weak<Principale>, pont: &Pont) -> (i64, usize, usize, usize) {
    let lignes = sur_l_interface(faible, |f| {
        ElementHandle::find_by_element_type_name(f, "LigneMessageVue").count()
    });
    let m = pont.messagerie();
    (
        m.position().round() as i64,
        lignes,
        m.nombre_de_lignes(),
        m.mesures(),
    )
}

fn stabiliser(faible: &slint::Weak<Principale>, pont: &Pont) {
    let mut precedent = None;
    let mut identiques = 0;
    attendre(
        || {
            pomper(faible);
            let courant = instantanee(faible, pont);
            if Some(courant) == precedent && !pont.messagerie().chargement_en_cours() {
                identiques += 1;
            } else {
                identiques = 0;
            }
            precedent = Some(courant);
            identiques >= 3
        },
        "liste de messages stable",
    );
}

fn ecran_de(faible: &slint::Weak<Principale>, n: u64) -> Option<f32> {
    let premiere = format!("message n°{n}");
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_element_type_name(f, "CorpsMessage")
            .find(|e| {
                e.accessible_description()
                    .is_some_and(|d| d == premiere || d.starts_with(&format!("{premiere}\n")))
            })
            .map(|e| e.absolute_position().y)
    })
}

fn non_lus_du_salon(f: &Principale, salon: &str) -> i32 {
    f.global::<DonneesCoquille>()
        .get_salons()
        .iter()
        .find(|l| l.id == salon)
        .map_or(-1, |l| l.non_lus)
}

fn envoyer_message(fabrique: &FausseFabrique, id: &str) {
    let mut m = message(id, "c1", "carol");
    m["content"] = json!(format!("message n°{id}"));
    fabrique.envoyer(evenement("message.created", m));
}

fn scenario(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    coquille: Arc<Coquille>,
    serveur: Arc<ServeurMessages>,
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
    stabiliser(&faible, &pont);

    let premier = format!("{}", NOMBRE as i64 - NON_LUS + 1);
    assert_eq!(pont.messagerie().nouveau_depuis(), Some(premier.clone()));
    assert!(pont.messagerie().cles().contains(&format!("new-{premier}")));
    assert!(
        !pont.messagerie().colle(),
        "ouverture sur le premier non-lu"
    );
    let haut_liste = sur_l_interface(&faible, |f| {
        ElementHandle::find_by_accessible_label(f, "Messages")
            .find(|e| e.accessible_role() == Some(AccessibleRole::List))
            .map(|e| e.absolute_position().y)
            .unwrap()
    });
    let separateur = sur_l_interface(&faible, |f| {
        ElementHandle::find_by_accessible_label(f, "Nouveaux messages")
            .next()
            .map(|e| e.absolute_position().y)
    })
    .expect("séparateur des non-lus instancié");
    println!(
        "299 saut au premier non-lu : séparateur à {:.0} px du haut de la liste",
        separateur - haut_liste
    );
    assert!(
        (separateur - haut_liste - 24.0).abs() < 2.0,
        "{separateur} {haut_liste}"
    );

    fabrique.envoyer(evenement(
        "typing",
        json!({"channel_id": "c1", "dm_id": null, "user_id": "carol", "pseudo": "carol"}),
    ));
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesMessagerie>().get_saisie() == "carol est en train d'écrire…",
        "indicateur de saisie",
    );

    let repere = NOMBRE - 30;
    let avant = ecran_de(&faible, repere).expect("repère visible");
    let position = pont.messagerie().position();
    envoyer_message(&fabrique, "301");
    attendre(
        || pont.messagerie().cles().contains(&"301".to_string()),
        "message reçu en temps réel",
    );
    stabiliser(&faible, &pont);
    let apres = ecran_de(&faible, repere).expect("repère toujours visible");
    println!(
        "306 C3 : repère à {avant} px avant, {apres} px après ; défilement {position} puis {}",
        pont.messagerie().position()
    );
    assert_eq!(avant, apres, "la vue n'a pas défilé");
    assert_eq!(position, pont.messagerie().position());
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesMessagerie>();
            g.get_bouton_bas() && g.get_nouveaux() == 1 && g.get_saisie().is_empty()
        },
        "bouton « 1 nouveau message » et saisie terminée",
    );

    let modifie = NOMBRE - 35;
    let mut m = message_numero(modifie, NOMBRE);
    m["content"] = json!(format!("{} (corrigé)", contenu_de(modifie)));
    m["edited_at"] = json!("2026-11-19T10:00:00Z");
    fabrique.envoyer(evenement("message.updated", m));
    attendre(
        || {
            pont.messagerie()
                .operations()
                .iter()
                .any(|(o, c)| matches!(o, Operation::Remplacer(_)) && *c == modifie.to_string())
        },
        "message modifié en temps réel",
    );
    let operations = pont.messagerie().operations();
    println!("306 modification : {operations:?}");
    assert_eq!(operations.len(), 1, "{operations:?}");
    assert!(sur_l_interface(&faible, move |f| lignes(f)
        .iter()
        .any(|l| l.id == modifie.to_string() && l.modifie)));

    let epingle = NOMBRE - 34;
    let mut m = message_numero(epingle, NOMBRE);
    m["pinned_at"] = json!("2026-11-19T10:01:00Z");
    fabrique.envoyer(evenement("message.updated", m));
    attendre_l_interface(
        &faible,
        move |f| {
            lignes(f)
                .iter()
                .any(|l| l.id == epingle.to_string() && l.epingle)
        },
        "message épinglé en temps réel",
    );
    let operations = pont.messagerie().operations();
    println!("306 épinglage : {operations:?}");
    assert_eq!(operations.len(), 1, "{operations:?}");
    assert!(matches!(operations[0], (Operation::Remplacer(_), ref c) if *c == epingle.to_string()));

    let supprime = NOMBRE - 33;
    fabrique.envoyer(evenement(
        "message.deleted",
        json!({"id": supprime.to_string(), "channel_id": "c1", "dm_id": null}),
    ));
    attendre_l_interface(
        &faible,
        move |f| !lignes(f).iter().any(|l| l.id == supprime.to_string()),
        "message supprimé en temps réel",
    );
    let operations = pont.messagerie().operations();
    println!("306 suppression : {operations:?}");
    assert!(operations
        .iter()
        .any(|(o, c)| matches!(o, Operation::Retirer(_)) && *c == supprime.to_string()));
    assert!(
        operations
            .iter()
            .filter(|(o, _)| matches!(o, Operation::Remplacer(_)))
            .count()
            <= 1,
        "{operations:?}"
    );

    let navigateur = fabrique.navigateur.clone();
    sur_l_interface(&faible, |f| {
        let g = f.global::<DonneesMessagerie>();
        for cible in [
            "u:javascript:alert(1)",
            "u:file:///etc/passwd",
            "u:JaVaScRiPt:alert(1)",
            "autre:https://plbls.fr/",
        ] {
            g.invoke_lien_clique("265".into(), cible.into());
        }
    });
    assert!(
        navigateur.ouvertes.lock().unwrap().is_empty(),
        "aucun lien dangereux ouvert"
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesMessagerie>()
            .invoke_lien_clique("265".into(), "u:https://plbls.fr/doc".into())
    });
    assert_eq!(
        *navigateur.ouvertes.lock().unwrap(),
        ["https://plbls.fr/doc"]
    );
    println!(
        "300 C2 : ouvertes = {:?}",
        navigateur.ouvertes.lock().unwrap()
    );

    sur_l_interface(&faible, |f| {
        f.global::<DonneesMessagerie>().invoke_aller_en_bas()
    });
    stabiliser(&faible, &pont);
    assert!(pont.messagerie().colle() && pont.messagerie().en_bas());
    let sauts_avant = serveur.nombre_de("GET /channels/c1/messages?limit=100&after=2");
    sur_l_interface(&faible, |f| {
        ElementHandle::find_by_accessible_label(f, "Aller au message de carol")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("réponse citée visible")
            .mock_single_click(PointerEventButton::Left);
    });
    attendre(
        || pont.messagerie().mis_en_evidence().as_deref() == Some("3"),
        "message cité chargé puis mis en évidence",
    );
    stabiliser(&faible, &pont);
    assert!(sur_l_interface(&faible, |f| lignes(f)
        .iter()
        .any(|l| l.id == "3" && l.mis_en_evidence)));
    let ecran = ecran_de(&faible, 3).expect("message cité instancié");
    println!(
        "301 C3 : requêtes de saut {} -> {}, message 3 à y = {ecran}",
        sauts_avant,
        serveur.nombre_de("GET /channels/c1/messages?limit=100&after=2")
    );
    assert!(serveur.nombre_de("GET /channels/c1/messages?limit=100&after=2") > sauts_avant);

    sur_l_interface(&faible, |f| {
        f.global::<DonneesMessagerie>().invoke_aller_en_bas()
    });
    attendre(
        || {
            pomper(&faible);
            pont.messagerie().fin_atteinte() && pont.messagerie().en_bas()
        },
        "retour au présent",
    );
    stabiliser(&faible, &pont);
    let store = fabrique.store();
    attendre(
        || store.lire(|e| e.focus && e.en_bas),
        "lecture en bas de liste signalée",
    );
    attendre_l_interface(&faible, |f| non_lus_du_salon(f, "c1") == 0, "salon lu");
    coquille.fenetre_active(false);
    attendre(
        || store.lire(|e| !e.focus),
        "fenêtre inactive transmise au store",
    );
    let lectures = serveur.nombre_de("POST /channels/c1/read");
    envoyer_message(&fabrique, "302");
    attendre_l_interface(
        &faible,
        |f| non_lus_du_salon(f, "c1") == 1,
        "message non lu fenêtre inactive",
    );
    stabiliser(&faible, &pont);
    assert!(pont.messagerie().en_bas(), "toujours en bas");
    assert_eq!(sur_l_interface(&faible, |f| non_lus_du_salon(f, "c1")), 1);
    assert_eq!(serveur.nombre_de("POST /channels/c1/read"), lectures);
    coquille.fenetre_active(true);
    attendre_l_interface(
        &faible,
        |f| non_lus_du_salon(f, "c1") == 0,
        "lu au retour sur la fenêtre",
    );
    attendre(
        || serveur.nombre_de("POST /channels/c1/read") > lectures,
        "lecture envoyée au serveur",
    );
    println!(
        "305 C2 : non lu fenêtre inactive, lu au retour ({} -> {} POST /read)",
        lectures,
        serveur.nombre_de("POST /channels/c1/read")
    );

    assert_eq!(sur_l_interface(&faible, |f| non_lus_du_salon(f, "c4")), 3);
    fabrique.envoyer(evenement(
        "read.updated",
        json!({"channel_id": "c4", "user_id": "u1", "pseudo": "bob", "last_id": "9", "read_at": "2026-11-19T10:00:00Z"}),
    ));
    attendre_l_interface(
        &faible,
        |f| non_lus_du_salon(f, "c4") == 0,
        "lu sur un autre appareil",
    );
    println!("305 C3 : pastille de c4 retirée après read.updated");
}

#[test]
fn messagerie_lecture_temps_reel_et_marquage_comme_lu() {
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
    let mut depart = etat();
    depart
        .non_lus
        .insert(CleConversation::Salon("c1".into()), NON_LUS);
    let fabrique = FausseFabrique::sur(depart, Vec::new(), &serveur.adresse);
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
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, fabrique, c, serveur)));
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
