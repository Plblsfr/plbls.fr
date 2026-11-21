mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use commun::bureau::{EtatZone, FausseZone};
use commun::coquille::{evenement, message, FausseFabrique, ServeurMessages};
use commun::notifications::{EtatPastille, NotificationsFactices, PastilleFactice};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::journal::Journal;
use estompe_client::preferences::Preferences;
use estompe_client::store::CleConversation;
use estompe_natif::bureau::notifications::{Notifieur, OptionsNotifieur};
use estompe_natif::bureau::{Bureau, EtapeFermeture, OptionsBureau, DELAI_ARRET};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Evenement, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, ecrans, DonneesCoquille, Page, Principale};
use estompe_plateforme::instance::{MessageInstance, RecepteurInstance};
use estompe_plateforme::{ActionNotification, EvenementZone, Indisponible};
use serde_json::json;
use slint::platform::WindowEvent;
use slint::{ComponentHandle, Model};

fn visible(faible: &slint::Weak<Principale>) -> bool {
    sur_l_interface(faible, |f| f.window().is_visible())
}

fn demander_la_fermeture(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        f.window().dispatch_event(WindowEvent::CloseRequested);
    });
}

fn flux_ferme(fabrique: &FausseFabrique) -> bool {
    fabrique
        .signaux
        .lock()
        .unwrap()
        .as_ref()
        .is_none_or(|s| s.is_closed())
}

fn non_lus(f: &Principale, serveur: &str) -> i32 {
    f.global::<DonneesCoquille>()
        .get_serveurs()
        .iter()
        .find(|l| l.id == serveur)
        .map(|l| l.non_lus)
        .unwrap_or(-1)
}

struct Faux {
    fabrique: Arc<FausseFabrique>,
    zone: Arc<EtatZone>,
    notifications: Arc<NotificationsFactices>,
    pastille: Arc<EtatPastille>,
    serveur: Arc<ServeurMessages>,
    journaux: std::path::PathBuf,
    focus: Box<dyn Fn(bool) + Send>,
}

fn requete_recue(serveur: &ServeurMessages, requete: &str, contenu: &str) -> bool {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .any(|(r, c)| r == requete && c.contains(contenu))
}

fn texte_des_journaux(dossier: &std::path::Path) -> String {
    std::fs::read_dir(dossier)
        .map(|entrees| {
            entrees
                .filter_map(Result::ok)
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn notification_serveur(tag: &str, conv: &str, url: &str, msg: &str) -> serde_json::Value {
    json!({
        "title": "carol",
        "body": "corps secret 7f3a",
        "tag": tag,
        "url": url,
        "conv": conv,
        "msg": msg,
        "kind": "mention",
        "silent": false,
    })
}

fn titre(f: &Principale) -> String {
    f.global::<DonneesCoquille>().get_vue().titre.to_string()
}

fn scenario_notifications(faible: &slint::Weak<Principale>, faux: &Faux) {
    attendre(
        || faux.pastille.derniere() == Some((None, String::new())),
        "pastille vide au départ",
    );
    let icones_avant = faux.zone.icones.lock().unwrap().len();
    let mut mention = message("91", "t1", "carol");
    mention["group_mention"] = json!(false);
    faux.fabrique.envoyer(evenement("mention", mention));
    attendre(
        || {
            faux.pastille
                .derniere()
                .is_some_and(|(image, texte)| image.is_some() && texte == "1 mention")
        },
        "pastille à 1 sur la barre des tâches",
    );
    attendre(
        || faux.zone.icones.lock().unwrap().len() > icones_avant,
        "pastille sur l'icône de la zone",
    );
    {
        let icones = faux.zone.icones.lock().unwrap();
        assert_ne!(icones.last(), icones.first(), "icône marquée");
    }

    faux.fabrique.envoyer(evenement(
        "notification",
        notification_serveur("m-91", "c:t1", "/#/s/s2/t1", "91"),
    ));
    attendre(|| faux.notifications.nombre() == 1, "notification affichée");
    let affichee = faux.notifications.affichees.lock().unwrap()[0].clone();
    assert_eq!(affichee.titre, "carol");
    assert_eq!(affichee.etiquette, "m-91");
    assert!(affichee.actions && affichee.reponse && !affichee.silencieuse);
    let journal = texte_des_journaux(&faux.journaux);
    assert!(journal.contains("notification affichée"), "{journal}");
    assert!(
        !journal.contains("corps secret") && !journal.contains("carol"),
        "aucun contenu de message dans le journal : {journal}"
    );

    faux.notifications
        .declencher("m-91", ActionNotification::MarquerLu);
    attendre(
        || requete_recue(&faux.serveur, "POST /channels/t1/read", ""),
        "lecture envoyée au serveur",
    );
    attendre(
        || faux.pastille.derniere() == Some((None, String::new())),
        "« Marquer comme lu » efface la pastille",
    );
    assert!(faux
        .notifications
        .retirees
        .lock()
        .unwrap()
        .contains(&"m-91".to_string()));

    faux.notifications
        .declencher("m-91", ActionNotification::Repondre("coucou".into()));
    attendre(
        || requete_recue(&faux.serveur, "POST /channels/t1/messages", "coucou"),
        "réponse envoyée par la file d'envoi",
    );

    sur_l_interface(faible, |f| f.hide().unwrap());
    faux.notifications
        .declencher("m-91", ActionNotification::Ouvrir);
    attendre_l_interface(
        faible,
        |f| f.window().is_visible() && titre(f) == "équipe",
        "le clic ramène la fenêtre sur la conversation",
    );
    let store = faux.fabrique.store();
    attendre(
        || store.lire(|e| e.courante == Some(CleConversation::Salon("t1".into()))),
        "conversation affichée",
    );
    (faux.focus)(true);
    faux.fabrique.envoyer(evenement(
        "notification",
        notification_serveur("m-92", "c:t1", "/#/s/s2/t1", "92"),
    ));
    faux.fabrique.envoyer(evenement(
        "notification",
        notification_serveur("m-93", "d:d1", "/#/dm/d1", "93"),
    ));
    attendre(
        || faux.notifications.nombre() == 2,
        "notification d'une autre conversation",
    );
    let etiquettes: Vec<String> = faux
        .notifications
        .affichees
        .lock()
        .unwrap()
        .iter()
        .map(|n| n.etiquette.clone())
        .collect();
    assert_eq!(
        etiquettes,
        ["m-91", "m-93"],
        "rien pour la conversation affichée, fenêtre active"
    );
}

fn scenario_zone_presente(
    faible: slint::Weak<Principale>,
    faux: Faux,
    recepteur: RecepteurInstance,
    recus: Arc<Mutex<Vec<Evenement>>>,
) {
    let (fabrique, zone) = (faux.fabrique.clone(), faux.zone.clone());
    attendre_l_interface(
        &faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.global::<DonneesCoquille>().get_serveurs().row_count() == 4
        },
        "coquille ouverte",
    );
    assert!(zone.installee(), "icône posée dans la zone");
    assert_eq!(zone.icones.lock().unwrap()[0].largeur, 64);

    sur_l_interface(&faible, |f| f.hide().unwrap());
    assert!(!visible(&faible));
    recepteur(MessageInstance {
        arguments: vec!["--invitation".into(), "ABCDE-23456".into()],
        jeton_activation: None,
    });
    attendre(|| visible(&faible), "le second lancement ramène la fenêtre");
    attendre(
        || {
            recus
                .lock()
                .unwrap()
                .contains(&Evenement::Rejoindre("ABCDE-23456".into()))
        },
        "invitation transmise à l'entrée",
    );

    demander_la_fermeture(&faible);
    attendre(|| !visible(&faible), "fermer masque la fenêtre");
    assert!(
        !flux_ferme(&fabrique),
        "la connexion temps réel reste ouverte"
    );
    assert_eq!(sur_l_interface(&faible, |f| f.get_page()), Page::Coquille);
    fabrique.envoyer(evenement("message.created", message("90", "t1", "carol")));
    attendre_l_interface(
        &faible,
        |f| non_lus(f, "s2") == 1,
        "un message reçu fenêtre masquée arrive encore",
    );

    zone.declencher(EvenementZone::Clic);
    attendre(|| visible(&faible), "un clic sur l'icône ramène la fenêtre");
    demander_la_fermeture(&faible);
    attendre(|| !visible(&faible), "masquée à nouveau");
    zone.declencher(EvenementZone::Ouvrir);
    attendre(|| visible(&faible), "« Ouvrir » ramène la fenêtre");

    scenario_notifications(&faible, &faux);

    zone.declencher(EvenementZone::Quitter);
}

fn scenario_zone_absente(faible: slint::Weak<Principale>) {
    attendre(|| visible(&faible), "fenêtre affichée");
    demander_la_fermeture(&faible);
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
fn c3_fermer_garde_la_connexion_quitter_la_ferme_et_les_notifications_suivent_la_pastille() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None);
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
            auth,
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let serveur = ServeurMessages::demarrer(10);
    let mut etat = commun::coquille::etat();
    etat.mentions.clear();
    let fabrique = FausseFabrique::sur(etat, vec![], &serveur.adresse);
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
    let (fausse_zone, zone) = FausseZone::nouvelle(true);
    let (fausse_pastille, pastille) = PastilleFactice::nouvelle();
    let notifications = Arc::new(NotificationsFactices::default());
    let journaux = dossier.path().join("journaux");
    let journal_debogage = Journal::ouvrir(&journaux);
    journal_debogage.activer(true);
    coquille.observer(Notifieur::nouveau(OptionsNotifieur {
        systeme: notifications.clone(),
        memoire: memoire.clone(),
        fenetre_active: coquille.fenetre_active_partagee(),
        source: coquille.source_pont(),
        runtime: entree.runtime(),
        journal: Some(Arc::new(journal_debogage)),
    }));
    let recus = Arc::new(Mutex::new(Vec::new()));
    let journal = recus.clone();
    let bureau = Bureau::installer(
        &fenetre,
        OptionsBureau {
            zone: fausse_zone,
            pastille: fausse_pastille,
            demarrage: Box::new(Indisponible),
            memoire: memoire.clone(),
            entree: Arc::new(move |e| journal.lock().unwrap().push(e)),
            journal: None,
            suivi_du_focus: Some(Box::new(coquille.suivi_du_focus())),
        },
    );
    let arret = coquille.arret();
    bureau
        .fermeture()
        .avant_de_quitter(EtapeFermeture::Coquille, move || {
            assert!(arret.arreter(DELAI_ARRET), "coquille arrêtée à temps");
        });
    assert!(!fenetre.get_demarrage_disponible());
    let recepteur = bureau.recepteur_instance();
    fenetre.show().unwrap();
    let faux = Faux {
        fabrique: fabrique.clone(),
        zone: zone.clone(),
        notifications,
        pastille,
        serveur,
        journaux,
        focus: Box::new(coquille.suivi_du_focus()),
    };
    let faible = fenetre.as_weak();
    piloter(move || scenario_zone_presente(faible, faux, recepteur, recus));
    assert!(bureau.fermeture().quittee(), "« Quitter » ferme vraiment");
    assert!(zone.retiree.load(Ordering::SeqCst), "icône retirée");
    attendre(
        || flux_ferme(&fabrique),
        "« Quitter » ferme la connexion temps réel",
    );
    assert!(!fenetre.window().is_visible());
    drop(coquille);
    drop(entree);

    let seconde = creer_fenetre().unwrap();
    let (zone_absente, etat_absente) = FausseZone::nouvelle(false);
    let bureau_sans_zone = Bureau::installer(
        &seconde,
        OptionsBureau {
            zone: zone_absente,
            pastille: Box::new(Indisponible),
            demarrage: Box::new(Indisponible),
            memoire,
            entree: Arc::new(|_| {}),
            journal: None,
            suivi_du_focus: None,
        },
    );
    seconde.show().unwrap();
    let faible = seconde.as_weak();
    piloter(move || scenario_zone_absente(faible));
    assert!(
        bureau_sans_zone.fermeture().quittee(),
        "sans zone de notification, fermer quitte"
    );
    assert!(etat_absente.retiree.load(Ordering::SeqCst));
}
