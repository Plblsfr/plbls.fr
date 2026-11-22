mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use commun::{attendre, attendre_l_interface, sur_l_interface};
use estompe_client::preferences::{Preferences, UsageTouche};
use estompe_client::replay::types::{
    Qualite, Resolution, DUREE_MAX_S, DUREE_MIN_S, QUOTA_MIN, TOUCHE_DEFAUT,
};
use estompe_natif::overlay::replay::ServiceOverlayReplay;
use estompe_natif::overlay::MoteurOverlay;
use estompe_natif::replay::reglages::{self, OutilsReglages};
use estompe_natif::replay::{Dependances, ServiceReplay};
use estompe_natif::session::Memoire;
use estompe_natif::vocal::touches::texte_de_code;
use estompe_natif::{creer_fenetre, reglages_replay, DonneesReplay};
use estompe_plateforme::{
    EcransFactices, EncodageFactice, OverlayFactice, PremierPlanFactice, RaccourcisFactices,
    SonsFactices,
};
use slint::{ComponentHandle, Model};

fn texte(code: &str) -> String {
    texte_de_code(code).unwrap()
}

fn piloter(scenario: impl FnOnce() + Send + 'static) {
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(scenario));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    if let Err(panique) = pilote.join().unwrap() {
        resume_unwind(panique);
    }
}

#[test]
fn les_reglages_du_replay_restent_dans_les_bornes_et_les_touches_globales_ne_se_chevauchent_pas() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let mut preferences = Preferences::default();
    preferences.ptt.actif = true;
    preferences.ptt.touche = "KeyV".into();
    let memoire = Memoire::nouvelle(preferences, dossier.path().join("config"), None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let execution = runtime.handle().clone();
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));
    let raccourcis = Arc::new(RaccourcisFactices::nouveaux(true));
    let lecture = memoire.clone();
    let service = ServiceReplay::demarrer(
        Dependances {
            encodage: Arc::new(EncodageFactice::nouveau()),
            ecrans: Arc::new(EcransFactices::nouveaux()),
            sons: Arc::new(SonsFactices::nouveaux()),
            micro: None,
            premier_plan: Arc::new(PremierPlanFactice::nouveau()),
            raccourcis: raccourcis.clone(),
            touches_prises: Arc::new(move || {
                lecture.lire(|p| p.touches_prioritaires(UsageTouche::Replay))
            }),
            videos: Some(dossier.path().join("Vidéos")),
            journaux: Some(dossier.path().join("journaux")),
            version: "1.0.0".into(),
            horloge: None,
        },
        memoire.lire(reglages_replay),
    );
    let calque = OverlayFactice::nouveau(None, 1.0);
    let overlay =
        ServiceOverlayReplay::nouveau(&MoteurOverlay::nouveau(Arc::new(calque)), None, None);
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    reglages::brancher(
        &fenetre,
        OutilsReglages {
            service: service.clone(),
            memoire: memoire.clone(),
            fichiers: Arc::new(estompe_plateforme::Indisponible),
            overlay: Some(overlay.clone()),
            touches_globales: true,
        },
        &execution,
    );
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    let lecture = memoire.clone();
    piloter(move || {
        let memoire = lecture;
        sur_l_interface(&faible, |f| {
            let g = f.global::<DonneesReplay>();
            assert!(!g.get_actif());
            assert_eq!(g.get_resume().as_str(), "Désactivé.");
            g.invoke_ouvrir();
            assert!(g.get_ouvert());
        });
        attendre_l_interface(
            &faible,
            |f| f.global::<DonneesReplay>().get_materiel_lu(),
            "le matériel est lu à l'ouverture",
        );
        sur_l_interface(&faible, |f| {
            let g = f.global::<DonneesReplay>();
            assert!(g.get_disponible());
            assert!(g.get_etat().contains("Encodeur : H.264 factice"));
            assert_eq!(g.get_resolutions().row_count(), 5);
            g.invoke_definir_actif(true);
            g.invoke_definir_duree(1000.0, true);
            g.invoke_definir_resolution(3);
            g.invoke_definir_resolution(0);
            g.invoke_definir_cadence(3);
            g.invoke_definir_qualite(0);
            g.invoke_definir_quota(0.0, true);
            g.invoke_definir_micro(true);
            g.invoke_definir_curseur(true);
            g.invoke_definir_resolution(99);
            assert!(g.get_actif());
            assert_eq!(g.get_duree(), DUREE_MAX_S as i32);
            assert_eq!(g.get_quota(), 1);
            assert!(g.get_resolutions().row_data(0).unwrap().choisi);
            assert!(!g.get_resolutions().row_data(3).unwrap().disponible);
        });
        let r = memoire.lire(|p| p.replay.clone());
        assert!(r.actif && r.micro && r.curseur);
        assert_eq!(
            (r.duree_s, r.resolution, r.qualite, r.quota_octets),
            (DUREE_MAX_S, Resolution::R720, Qualite::Basse, QUOTA_MIN)
        );
        assert_eq!(r.ips, 144);
        assert_eq!(service.reglages().replay, r);
        sur_l_interface(&faible, |f| {
            f.global::<DonneesReplay>().invoke_definir_duree(-3.0, true);
        });
        assert_eq!(memoire.lire(|p| p.replay.duree_s), DUREE_MIN_S);
        attendre(
            || raccourcis.enregistres() == [TOUCHE_DEFAUT],
            "la touche du replay est prise une fois le replay actif",
        );
        let (f8, v, f9) = (texte("F8"), texte("KeyV"), texte("F9"));
        sur_l_interface(&faible, move |f| {
            let g = f.global::<DonneesReplay>();
            assert!(!g.invoke_touche_choisie(f8.into()));
            assert_ne!(g.get_erreur_touche().as_str(), "");
            assert!(!g.invoke_touche_choisie(v.into()));
            assert!(g.invoke_touche_choisie(f9.into()));
            assert_eq!(
                (g.get_erreur_touche().as_str(), g.get_touche().as_str()),
                ("", "F9")
            );
        });
        assert_eq!(memoire.lire(|p| p.replay.touche.clone()), "F9");
        attendre(
            || raccourcis.enregistres() == ["F9"],
            "la nouvelle touche remplace l'ancienne",
        );
        assert!(memoire
            .lire(|p| p.touches_des_autres(UsageTouche::Messages))
            .contains(&"F9".to_string()));
        sur_l_interface(&faible, |f| {
            let g = f.global::<DonneesReplay>();
            g.invoke_definir_actif(false);
            assert_eq!(g.get_resume().as_str(), "Désactivé.");
            g.invoke_fermer();
            assert!(!g.get_ouvert());
        });
        attendre(
            || raccourcis.enregistres().is_empty(),
            "replay éteint : touche rendue",
        );
        std::thread::sleep(Duration::from_millis(10));
    });
}
