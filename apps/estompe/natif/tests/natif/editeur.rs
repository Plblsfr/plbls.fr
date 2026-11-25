use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use commun::{attendre, attendre_l_interface, sur_l_interface};
use estompe_client::preferences::Preferences;
use estompe_client::replay::galerie;
use estompe_client::replay::montage::{self, LOGICIELS};
use estompe_client::video::Conversions;
use estompe_natif::replay::editeur::{self, OutilsEditeur};
use estompe_natif::replay::{clips, Dependances, ServiceReplay};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, reglages_replay, DonneesClips, DonneesEditeur};
use estompe_plateforme::{
    EcransFactices, EncodageFactice, PremierPlanFactice, RaccourcisFactices, SonsFactices,
};
use slint::platform::Key;
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

fn encodeur_local() -> Option<String> {
    let liste = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .ok()?;
    let texte = String::from_utf8_lossy(&liste.stdout).into_owned();
    LOGICIELS
        .into_iter()
        .find(|e| texte.contains(e))
        .map(String::from)
}

fn fabriquer(chemin: &Path, encodeur: &str) {
    let sortie = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=4",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=4",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1500:sample_rate=48000:duration=4",
        ])
        .args([
            "-map", "0:v", "-map", "1:a", "-map", "2:a", "-c:v", encodeur, "-b:v", "500k",
        ])
        .args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-f", "mp4", "-y"])
        .arg(chemin)
        .output()
        .unwrap();
    assert!(
        sortie.status.success(),
        "{}",
        String::from_utf8_lossy(&sortie.stderr)
    );
}

fn duree(chemin: &Path) -> f64 {
    let commande = || {
        let mut c = Command::new("ffmpeg");
        c.args(["-hide_banner", "-nostdin"]);
        Ok(c)
    };
    montage::sonder(&commande, chemin).unwrap().duree_s
}

fn touche(faible: &slint::Weak<estompe_natif::Principale>, texte: String, ctrl: bool) -> bool {
    sur_l_interface(faible, move |f| {
        f.global::<DonneesEditeur>()
            .invoke_touche(texte.into(), ctrl, false, false)
    })
}

#[test]
fn l_editeur_coupe_regle_exporte_et_rouvre_un_montage_sans_toucher_au_clip() {
    let Some(encodeur) = encodeur_local() else {
        eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
        return;
    };
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().join("config"), None);
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
    let rangement = service.dossier().unwrap();
    std::fs::create_dir_all(rangement.join(galerie::ANNEXES)).unwrap();
    std::fs::write(
        rangement.join(galerie::ANNEXES).join("export-1-1.part"),
        b"reste",
    )
    .unwrap();
    let clip = rangement.join("Jeu 2026-01-01 10-00-00.mp4");
    fabriquer(&clip, &encodeur);
    let original = std::fs::read(&clip).unwrap();
    let fenetre = creer_fenetre().unwrap();
    let galerie_clips = clips::brancher(
        &fenetre,
        clips::OutilsGalerie {
            service: service.clone(),
            fichiers: Arc::new(estompe_plateforme::Indisponible),
            conversions: Arc::new(Conversions::aucune()),
            ffmpeg: Some(PathBuf::from("ffmpeg")),
            messagerie: None,
        },
        &execution,
    );
    let _editeur = editeur::brancher(
        &fenetre,
        OutilsEditeur {
            service: service.clone(),
            galerie: galerie_clips.clone(),
            ffmpeg: PathBuf::from("ffmpeg"),
            sortie_son: None,
            encodeurs: Some(vec!["encodeur_absent".into(), encodeur]),
            horloge: None,
        },
    );
    assert!(!rangement
        .join(galerie::ANNEXES)
        .join("export-1-1.part")
        .exists());
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    piloter(move || {
        sur_l_interface(&faible, |f| f.global::<DonneesClips>().invoke_ouvrir());
        attendre_l_interface(
            &faible,
            |f| f.global::<DonneesClips>().get_clips().row_count() == 1,
            "le clip est listé",
        );
        let chemin = clip.to_string_lossy().into_owned();
        sur_l_interface(&faible, move |f| {
            let c = f.global::<DonneesClips>();
            assert!(c.get_modifier_disponible());
            c.invoke_modifier(chemin.into());
            assert!(!c.get_ouvert());
            let d = f.global::<DonneesEditeur>();
            assert!(d.get_ouvert() && !d.get_pret());
            assert_eq!(
                d.get_preparation().as_str(),
                "Ouverture de « Jeu 2026-01-01 10-00-00 »…"
            );
        });
        attendre_l_interface(
            &faible,
            |f| {
                let d = f.global::<DonneesEditeur>();
                d.get_pret() && d.get_a_image()
            },
            "l'éditeur s'ouvre sur la première image",
        );
        assert!(galerie_clips.est_protege(&clip));
        sur_l_interface(&faible, |f| {
            let d = f.global::<DonneesEditeur>();
            assert_eq!(
                (
                    d.get_nom().as_str(),
                    d.get_duree().as_str(),
                    d.get_segments().row_count()
                ),
                ("Jeu 2026-01-01 10-00-00", "0:04,00", 1)
            );
            let pistes: Vec<String> = d.get_pistes().iter().map(|p| p.nom.to_string()).collect();
            assert_eq!(pistes, ["Jeu", "Micro"]);
            assert_eq!(
                (d.get_image().size().width, d.get_image().size().height),
                (320, 240)
            );
            d.invoke_placer(0.25);
            d.invoke_couper();
            d.invoke_placer(0.5);
            d.invoke_couper();
            d.invoke_placer(0.3);
            d.invoke_basculer();
            d.invoke_muet_piste(1, true);
            assert_eq!(
                (d.get_duree().as_str(), d.get_segments().row_count()),
                ("0:03,00", 3)
            );
            assert!(
                !d.get_segments().row_data(1).unwrap().garde
                    && d.get_pistes().row_data(1).unwrap().muet
            );
        });
        attendre_l_interface(
            &faible,
            |f| {
                f.global::<DonneesEditeur>()
                    .get_pistes()
                    .iter()
                    .all(|p| !p.onde.is_empty())
            },
            "les formes d'onde des pistes sont tracées",
        );
        let projets = rangement.join(galerie::ANNEXES).join("projets");
        attendre(
            || std::fs::read_dir(&projets).is_ok_and(|mut l| l.next().is_some()),
            "le projet est enregistré",
        );
        assert!(touche(&faible, "z".into(), true));
        assert!(touche(&faible, "z".into(), true));
        sur_l_interface(&faible, |f| {
            assert_eq!(f.global::<DonneesEditeur>().get_duree().as_str(), "0:04,00")
        });
        assert!(touche(&faible, "y".into(), true));
        assert!(touche(&faible, "y".into(), true));
        let avant = sur_l_interface(&faible, |f| {
            f.global::<DonneesEditeur>().get_temps().to_string()
        });
        assert!(touche(
            &faible,
            char::from(Key::RightArrow).to_string(),
            false
        ));
        assert!(!touche(&faible, "x".into(), false));
        sur_l_interface(&faible, move |f| {
            let d = f.global::<DonneesEditeur>();
            assert_eq!(d.get_duree().as_str(), "0:03,00");
            assert_ne!(d.get_temps().as_str(), avant);
            d.invoke_demander_export();
            assert!(d.get_export_ouvert());
            let hauteurs: Vec<String> = d
                .get_hauteurs()
                .iter()
                .map(|h| h.libelle.to_string())
                .collect();
            assert_eq!(hauteurs, ["Cadre (320 × 240)"]);
            assert_eq!(d.get_export_format().as_str(), "320 × 240, 30 i/s, 0:03,00");
            d.invoke_exporter();
            assert!(!d.get_export_ouvert() && d.get_export_en_cours());
        });
        attendre_l_interface(
            &faible,
            |f| !f.global::<DonneesEditeur>().get_export_en_cours(),
            "l'export se termine",
        );
        let exporte = rangement.join("Jeu 2026-01-01 10-00-00 (montage).mp4");
        sur_l_interface(&faible, |f| {
            let d = f.global::<DonneesEditeur>();
            assert!(!d.get_message_erreur(), "{}", d.get_message());
            assert!(
                d.get_message().contains("Montage exporté"),
                "{}",
                d.get_message()
            );
        });
        assert!((duree(&exporte) - 3.0).abs() <= 1.0 / 30.0 + 0.011);
        assert_eq!(std::fs::read(&clip).unwrap(), original);
        assert_eq!(
            galerie::lire_annexe(&rangement, &exporte)
                .unwrap()
                .origine
                .as_deref(),
            Some("Jeu 2026-01-01 10-00-00")
        );
        sur_l_interface(&faible, |f| {
            f.global::<DonneesEditeur>().invoke_fermer();
            assert!(
                !f.global::<DonneesEditeur>().get_ouvert()
                    && f.global::<DonneesClips>().get_ouvert()
            );
        });
        assert!(!galerie_clips.est_protege(&clip));
        attendre_l_interface(
            &faible,
            |f| {
                let c = f.global::<DonneesClips>();
                c.get_clips().row_count() == 2 && c.get_travaux().row_count() == 0
            },
            "la galerie montre le montage exporté",
        );
        sur_l_interface(&faible, |f| {
            f.global::<DonneesEditeur>().invoke_lister_projets()
        });
        attendre_l_interface(
            &faible,
            |f| f.global::<DonneesEditeur>().get_projets().row_count() == 1,
            "le montage est listé dans la galerie",
        );
        sur_l_interface(&faible, |f| {
            let d = f.global::<DonneesEditeur>();
            let p = d.get_projets().row_data(0).unwrap();
            assert_eq!(p.nom.as_str(), "Jeu 2026-01-01 10-00-00");
            assert!(p.infos.starts_with("0:03,00 · 1 clip") && p.manquants.is_empty());
            d.invoke_ouvrir_projet(p.id);
        });
        attendre_l_interface(
            &faible,
            |f| f.global::<DonneesEditeur>().get_pret(),
            "le montage se rouvre",
        );
        sur_l_interface(&faible, |f| {
            let d = f.global::<DonneesEditeur>();
            assert_eq!(
                (d.get_duree().as_str(), d.get_segments().row_count()),
                ("0:03,00", 3)
            );
            d.invoke_fermer();
        });
        std::thread::sleep(Duration::from_millis(10));
    });
}
