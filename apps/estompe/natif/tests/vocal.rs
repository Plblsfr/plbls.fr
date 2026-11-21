mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use commun::coquille::{evenement, FausseFabrique};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::audio::factice::SystemeFactice;
use estompe_client::audio::{FormatAudio, SensAudio};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::vocal::factice::{ApiVocaleFactice, ServeurVocalFactice};
use estompe_client::vocal::{Dependances, EvenementSalle, ReglagesPtt, SessionVocale, SourceAudio};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::vocal::OutilsVocaux;
use estompe_natif::{creer_fenetre, ecrans, DonneesVocal, Page, ParticipantVocal, Principale};
use estompe_plateforme::RaccourcisFactices;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

const RETARD_MAXIMAL: Duration = Duration::from_millis(150);

fn cliquer(faible: &slint::Weak<Principale>, libelle: &'static str) {
    sur_l_interface(faible, move |f| {
        let element = ElementHandle::find_by_accessible_label(f, libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button) && e.size().height > 0.0)
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"));
        element.mock_single_click(PointerEventButton::Left);
    });
}

fn tuiles(f: &Principale) -> Vec<ParticipantVocal> {
    f.global::<DonneesVocal>().get_tuiles().iter().collect()
}

fn tuile(f: &Principale, utilisateur: &str) -> Option<ParticipantVocal> {
    tuiles(f).into_iter().find(|t| t.utilisateur == utilisateur)
}

fn presser(f: &Principale, texte: impl Into<SharedString>) {
    f.window()
        .dispatch_event(WindowEvent::KeyPressed { text: texte.into() });
}

fn relacher(f: &Principale, texte: impl Into<SharedString>) {
    f.window()
        .dispatch_event(WindowEvent::KeyReleased { text: texte.into() });
}

fn raccourci(faible: &slint::Weak<Principale>, lettre: &'static str) {
    sur_l_interface(faible, move |f| {
        presser(f, Key::Alt);
        presser(f, Key::Shift);
        presser(f, lettre);
        relacher(f, lettre);
        relacher(f, Key::Shift);
        relacher(f, Key::Alt);
    });
}

fn mesurer(
    faible: &slint::Weak<Principale>,
    condition: impl Fn(&Principale) -> bool + Send + Sync + Clone + 'static,
    quoi: &str,
) -> Duration {
    let debut = Instant::now();
    attendre_l_interface(faible, condition, quoi);
    debut.elapsed()
}

struct Banc {
    serveur: ServeurVocalFactice,
    audio: SystemeFactice,
    api: Arc<ApiVocaleFactice>,
    session: Arc<SessionVocale>,
}

fn voix_forte() -> Vec<f32> {
    (0..4_800)
        .map(|n| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / 48_000.0).sin())
        .collect()
}

fn scenario(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    banc: Banc,
    memoire: Memoire,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille,
        "coquille affichée",
    );
    attendre_l_interface(
        &faible,
        |f| f.global::<estompe_natif::DonneesCoquille>().get_vue().titre == "général",
        "dernière vue restaurée",
    );

    cliquer(&faible, "Salon vocal");
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_vue().ici && !v.get_vue().connexion && v.get_barre().visible
        },
        "c2 : rejoint en cliquant le salon vocal",
    );
    attendre(
        || banc.serveur.participants("v1").contains("u1"),
        "présent dans la salle",
    );
    attendre_l_interface(
        &faible,
        |f| tuiles(f).len() == 3,
        "une tuile par participant",
    );
    let (premiere, lieu, compte) = sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        (
            tuiles(f)[0].clone(),
            v.get_barre().lieu.to_string(),
            v.get_vue().compte.to_string(),
        )
    });
    assert!(premiere.moi && premiere.nom == "bob");
    assert_eq!(lieu, "Salon vocal · Les amis");
    assert_eq!(compte, "3 / 10");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>()
                .get_participants()
                .iter()
                .filter(|p| p.salon == "v1")
                .count()
                == 3
        },
        "participants listés sous le salon dans le panneau",
    );

    banc.serveur
        .envoyer("v1", "u1", EvenementSalle::Parole(vec!["carol".into()]));
    let retard = mesurer(
        &faible,
        |f| tuile(f, "carol").is_some_and(|t| t.parle),
        "c4 : carol parle",
    );
    eprintln!(
        "indicateur de parole d'un autre participant : {retard:?} après l'évènement de LiveKit"
    );
    assert!(retard < RETARD_MAXIMAL, "{retard:?}");
    banc.serveur
        .envoyer("v1", "u1", EvenementSalle::Parole(Vec::new()));
    attendre_l_interface(
        &faible,
        |f| tuile(f, "carol").is_some_and(|t| !t.parle),
        "carol se tait",
    );

    let voix = voix_forte();
    let debut = Instant::now();
    let mut retard_moi = None;
    while retard_moi.is_none() {
        for trame in voix.chunks(480) {
            banc.audio.pousser("micro", trame);
        }
        if sur_l_interface(&faible, |f| tuile(f, "u1").is_some_and(|t| t.parle)) {
            retard_moi = Some(debut.elapsed());
        }
        assert!(
            debut.elapsed() < Duration::from_secs(10),
            "sa propre voix jamais détectée"
        );
    }
    let retard_moi = retard_moi.unwrap();
    eprintln!("indicateur de sa propre parole : {retard_moi:?} après la première trame du micro");
    assert!(retard_moi < RETARD_MAXIMAL, "{retard_moi:?}");

    cliquer(&faible, "Couper le micro");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_barre().micro_coupe,
        "c2 : micro coupé",
    );
    attendre(
        || banc.serveur.micro_coupe("u1") == Some(true),
        "piste coupée côté salle",
    );
    attendre(
        || {
            banc.serveur
                .attributs("v1", "u1")
                .get("muted")
                .map(String::as_str)
                == Some("1")
        },
        "attribut muted publié",
    );
    attendre_l_interface(
        &faible,
        |f| tuile(f, "u1").is_some_and(|t| t.muet && !t.parle),
        "sa tuile affiche le micro coupé",
    );

    raccourci(&faible, "M");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().micro_coupe,
        "c3 : Alt+Maj+M réactive le micro",
    );
    raccourci(&faible, "D");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_barre().casque_coupe,
        "c3 : Alt+Maj+D coupe le son",
    );
    raccourci(&faible, "D");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().casque_coupe,
        "son rétabli",
    );

    cliquer(&faible, "carol");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_menu_ouvert()
                && f.global::<DonneesVocal>().get_menu().nom == "carol"
        },
        "menu de carol ouvert",
    );
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        v.invoke_volume(40, true);
        v.invoke_muet_pour_moi(true);
    });
    attendre(
        || {
            memoire.lire(|p| {
                p.ecoute.volume("carol", SourceAudio::Micro) == 40
                    && p.ecoute.muet("carol", SourceAudio::Micro)
            })
        },
        "volume et muet de carol retenus",
    );
    attendre(
        || {
            banc.session
                .instantane()
                .ecoute
                .muet("carol", SourceAudio::Micro)
        },
        "muet appliqué à la session",
    );
    sur_l_interface(&faible, |f| {
        presser(f, Key::Escape);
        relacher(f, Key::Escape);
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_menu_ouvert(),
        "menu fermé par Échap",
    );

    fabrique.envoyer(evenement(
        "voice.state",
        json!({"server_id": "s1", "channel_id": "v1", "user_id": "u1", "pseudo": "bob", "joined": true}),
    ));
    fabrique.envoyer(evenement(
        "voice.mod",
        json!({"server_id": "s1", "channel_id": "v1", "user_id": "u1", "mic_off": true, "sound_off": false}),
    ));
    attendre(
        || banc.session.instantane().micro_impose,
        "coupure du modérateur appliquée",
    );
    attendre_l_interface(
        &faible,
        |f| tuile(f, "u1").is_some_and(|t| t.micro_impose),
        "coupure du modérateur affichée",
    );
    fabrique.envoyer(evenement(
        "voice.mod",
        json!({"server_id": "s1", "channel_id": "v1", "user_id": "u1", "mic_off": false, "sound_off": false}),
    ));
    attendre(|| !banc.session.instantane().micro_impose, "coupure levée");

    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_ouvrir_reglages()
    });
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_reglages_ouverts()
                && f.global::<DonneesVocal>().get_micros().row_count() == 3
        },
        "réglages ouverts avec les micros",
    );
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        let mut r = v.get_reglages();
        r.ptt = true;
        v.invoke_regler(r, true);
    });
    attendre_l_interface(
        &faible,
        |f| {
            let r = f.global::<DonneesVocal>().get_reglages();
            r.ptt && r.ptt_message.contains("indisponible")
        },
        "c2 (#330) : le réglage dit que le raccourci global est indisponible",
    );
    attendre(
        || !banc.session.instantane().porte_ouverte,
        "porte fermée en appuyer pour parler",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_fermer_reglages()
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_reglages_ouverts(),
        "réglages fermés",
    );
    sur_l_interface(&faible, |f| presser(f, "v"));
    attendre(
        || banc.session.instantane().porte_ouverte,
        "c2 (#330) : la touche ouvre le micro fenêtre active",
    );
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_ptt_parle(),
        "« tu parles » affiché",
    );
    sur_l_interface(&faible, |f| relacher(f, "v"));
    attendre(
        || !banc.session.instantane().porte_ouverte,
        "relâchée, la porte se ferme",
    );
    assert!(memoire.lire(|p| p.ptt.actif));
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        let mut r = v.get_reglages();
        r.ptt = false;
        v.invoke_regler(r, true);
    });
    attendre(
        || banc.session.instantane().porte_ouverte,
        "appuyer pour parler désactivé",
    );

    fabrique.envoyer(evenement(
        "voice.moved",
        json!({"server_id": "s1", "from": "v1", "to": "k1", "by": "eve", "to_name": "Ciné du dimanche"}),
    ));
    attendre(
        || banc.serveur.participants("k1").contains("u1"),
        "déplacé dans le salon cinéma",
    );
    attendre_l_interface(
        &faible,
        |f| {
            let b = f.global::<DonneesVocal>().get_barre();
            b.salon == "k1" && b.message.contains("eve t'a déplacé")
        },
        "la barre suit le déplacement",
    );

    cliquer(&faible, "Quitter le vocal");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().visible,
        "c2 : raccroché",
    );
    attendre(
        || banc.serveur.participants("k1").is_empty(),
        "retiré de la salle",
    );
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_vue().ici,
        "écran pour rejoindre",
    );

    cliquer(&faible, "Rejoindre le salon");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_vue().ici
                && f.global::<DonneesVocal>().get_barre().visible
        },
        "rejoint depuis le bouton",
    );
    cliquer(&faible, "Quitter le vocal");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().visible,
        "raccroché à nouveau",
    );
    banc.api.refuser("v1", 409, "voice_full");
    cliquer(&faible, "Rejoindre le salon");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>()
                .get_vue()
                .erreur
                .contains("complet")
        },
        "refus du serveur affiché sur l'écran pour rejoindre",
    );
}

#[test]
fn c2_rejoindre_couper_le_micro_raccrocher_et_commandes_au_clavier() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(
        Preferences {
            derniere_vue: Some(DerniereVue::enregistrer("u1", "#/s/s1/c1")),
            ptt: ReglagesPtt::default(),
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
    let serveur = ServeurVocalFactice::nouveau();
    let audio = SystemeFactice::nouveau();
    audio.brancher(
        SensAudio::Entree,
        "micro",
        "Micro",
        FormatAudio {
            frequence: 48_000,
            canaux: 1,
        },
    );
    audio.brancher(
        SensAudio::Entree,
        "casque",
        "Casque",
        FormatAudio {
            frequence: 16_000,
            canaux: 1,
        },
    );
    audio.brancher(
        SensAudio::Sortie,
        "hp",
        "Haut-parleurs",
        FormatAudio {
            frequence: 48_000,
            canaux: 2,
        },
    );
    let api = Arc::new(ApiVocaleFactice::nouvelle("u1"));
    let session = Arc::new(SessionVocale::demarrer(
        &entree.runtime(),
        Dependances {
            api: api.clone(),
            connecteur: Arc::new(serveur.clone()),
            audio: Arc::new(audio.clone()),
            apm: None,
            reglages: estompe_client::vocal::Reglages::default(),
        },
    ));
    let fabrique = FausseFabrique::nouvelle(commun::coquille::etat(), Vec::new());
    *fabrique.vocal.lock().unwrap() = Some(OutilsVocaux {
        session: session.clone(),
        audio: Arc::new(audio.clone()),
        raccourcis: Arc::new(RaccourcisFactices::nouveaux(false)),
        apm: None,
    });
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
    let banc = Banc {
        serveur,
        audio,
        api,
        session: session.clone(),
    };
    let faible = fenetre.as_weak();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| {
            scenario(faible, fabrique, banc, memoire)
        }));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    let resultat = pilote.join().unwrap();
    session.fermer_bloquant(Duration::from_secs(3));
    drop(coquille);
    drop(entree);
    if let Err(panique) = resultat {
        resume_unwind(panique);
    }
}
