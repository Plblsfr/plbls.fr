use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use commun::{attendre, attendre_l_interface, sur_l_interface};
use estompe_client::preferences::Preferences;
use estompe_client::replay::types::QUOTA_MIN;
use estompe_client::video::Conversions;
use estompe_natif::confirmation;
use estompe_natif::replay::clips;
use estompe_natif::replay::{Dependances, ServiceReplay};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, reglages_replay, DonneesClips};
use estompe_plateforme::{
    EcransFactices, EncodageFactice, PremierPlanFactice, RaccourcisFactices, SonsFactices,
};
use slint::{ComponentHandle, Model};

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

fn clip_creux(chemin: &std::path::Path, taille: u64, age: u64) {
    let fichier = std::fs::File::create(chemin).unwrap();
    fichier.set_len(taille).unwrap();
    fichier
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + age))
        .unwrap();
}

#[test]
fn la_galerie_liste_renomme_supprime_et_purge_les_clips_sans_toucher_aux_proteges() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let mut preferences = Preferences::default();
    preferences.replay.quota_octets = QUOTA_MIN;
    let memoire = Memoire::nouvelle(preferences, dossier.path().join("config"), None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let execution = runtime.handle().clone();
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));
    let service = ServiceReplay::demarrer(
        Dependances {
            encodage: Arc::new(EncodageFactice::nouveau()),
            ecrans: Arc::new(EcransFactices::nouveaux()),
            sons: Arc::new(SonsFactices::nouveaux()),
            micro: None,
            premier_plan: Arc::new(PremierPlanFactice::nouveau()),
            raccourcis: Arc::new(RaccourcisFactices::nouveaux(true)),
            touches_prises: Arc::new(Vec::new),
            videos: Some(dossier.path().join("Vidéos")),
            journaux: None,
            version: "1.0.0".into(),
            horloge: None,
        },
        memoire.lire(reglages_replay),
    );
    let clips = service.dossier().unwrap();
    std::fs::create_dir_all(&clips).unwrap();
    let (ancien, moyen, recent) = (
        clips.join("Jeu 2026-01-01 10-00-00.mp4"),
        clips.join("Jeu 2026-01-02 10-00-00.mp4"),
        clips.join("Jeu 2026-01-03 10-00-00.mp4"),
    );
    clip_creux(&ancien, 600 << 20, 0);
    clip_creux(&moyen, 600 << 20, 60);
    clip_creux(&recent, 10, 120);
    let fenetre = creer_fenetre().unwrap();
    let galerie = clips::brancher(
        &fenetre,
        clips::OutilsGalerie {
            service: service.clone(),
            fichiers: Arc::new(estompe_plateforme::Indisponible),
            conversions: Arc::new(Conversions::aucune()),
            ffmpeg: None,
            messagerie: None,
            lecteur: None,
        },
        &execution,
    );
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    piloter(move || {
        sur_l_interface(&faible, |f| {
            f.global::<DonneesClips>().invoke_ouvrir();
        });
        attendre_l_interface(
            &faible,
            |f| f.global::<DonneesClips>().get_clips().row_count() == 3,
            "les trois clips sont listés",
        );
        sur_l_interface(&faible, |f| {
            let c = f.global::<DonneesClips>();
            assert!(c.get_ouvert() && c.get_visible() && c.get_depasse());
            assert!(!c.get_envoi_disponible() && !c.get_modifier_disponible());
            assert_eq!(c.get_liberer().as_str(), "Supprimer les plus anciens…");
            assert_eq!(c.get_espace().as_str(), "1,2 Go sur 1 Go");
            assert_eq!(
                c.get_clips().row_data(0).unwrap().nom.as_str(),
                "Jeu 2026-01-03 10-00-00"
            );
        });
        let protection = galerie.proteger(ancien.clone());
        sur_l_interface(&faible, |f| {
            f.global::<DonneesClips>().invoke_liberer_espace();
            let demande = confirmation::ouverte().unwrap();
            assert!(demande.destructif);
            assert!(demande.message.contains("le clip le plus ancien (600 Mo)"));
            confirmation::repondre(f, true);
        });
        attendre(
            || !moyen.exists(),
            "le plus ancien non protégé est supprimé",
        );
        attendre_l_interface(
            &faible,
            |f| f.global::<DonneesClips>().get_clips().row_count() == 2,
            "la galerie suit la purge",
        );
        assert!(ancien.exists() && recent.exists());
        let a_supprimer = ancien.clone();
        sur_l_interface(&faible, move |f| {
            let c = f.global::<DonneesClips>();
            assert!(!c.get_depasse());
            c.invoke_supprimer(a_supprimer.to_string_lossy().as_ref().into());
            assert!(confirmation::ouverte().is_none());
            assert!(c.get_erreur().contains("en cours d'utilisation"));
        });
        drop(protection);
        galerie.renommer_en(&recent, "Beau: tir".into());
        let renomme = clips.join("Beau tir.mp4");
        attendre(
            || renomme.exists() && !recent.exists(),
            "le clip est renommé",
        );
        let a_supprimer = ancien.clone();
        sur_l_interface(&faible, move |f| {
            f.global::<DonneesClips>()
                .invoke_supprimer(a_supprimer.to_string_lossy().as_ref().into());
            assert!(confirmation::ouverte().unwrap().message.contains("600 Mo"));
            confirmation::repondre(f, true);
        });
        attendre(
            || !ancien.exists(),
            "le clip est supprimé après confirmation",
        );
        attendre_l_interface(
            &faible,
            |f| {
                let c = f.global::<DonneesClips>().get_clips();
                c.row_count() == 1 && c.row_data(0).unwrap().nom.as_str() == "Beau tir"
            },
            "il ne reste que le clip renommé",
        );
        sur_l_interface(&faible, |f| {
            let c = f.global::<DonneesClips>();
            assert_eq!(c.get_erreur().as_str(), "");
            c.invoke_fermer();
            assert!(!c.get_ouvert());
        });
        std::thread::sleep(Duration::from_millis(10));
    });
}
