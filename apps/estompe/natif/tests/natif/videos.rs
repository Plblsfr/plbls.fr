use crate::commun;

use std::sync::{Arc, OnceLock};

use chrono::NaiveDate;
use commun::coquille::{etat_videos, SourceFactice, VIDEO_SOLO};
use commun::FauxNavigateur;
use estompe_client::lecteur::{CommandeMoteur, Lecteur, MediaFactice, Origine, Phase};
use estompe_client::route::Route;
use estompe_natif::coquille::{Navigation, Pont, Profils};
use estompe_natif::video::lecteur::GestionLecteur;
use estompe_natif::videos::{OutilsVideos, ORIGINE};
use estompe_natif::{
    creer_fenetre, DonneesCoquille, DonneesVideos, GenreVue, Page, Principale, VueSoloVideo,
};
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, Model};

fn runtime() -> tokio::runtime::Handle {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("runtime des vidéos")
        })
        .handle()
        .clone()
}

fn route(serveur: &str, salon: &str) -> Route {
    Route::Salon {
        serveur: serveur.into(),
        salon: Some(salon.into()),
        message: None,
    }
}

fn solo(f: &Principale) -> VueSoloVideo {
    f.global::<DonneesVideos>().get_solo()
}

fn minis(f: &Principale) -> usize {
    ElementHandle::find_by_element_type_name(f, "MiniLecteurSolo").count()
}

fn interruptions(commandes: &[CommandeMoteur]) -> Vec<CommandeMoteur> {
    commandes
        .iter()
        .filter(|c| matches!(c, CommandeMoteur::Arreter | CommandeMoteur::Pause(true)))
        .cloned()
        .collect()
}

#[test]
fn changer_de_salon_garde_la_lecture_dans_le_mini_lecteur() {
    i_slint_backend_testing::init_no_event_loop();
    let f = creer_fenetre().unwrap();
    f.window().set_size(slint::LogicalSize::new(1180.0, 900.0));
    f.set_page(Page::Coquille);
    f.global::<DonneesCoquille>().set_pret(true);
    f.show().unwrap();
    let matin = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(9, 30, 0)
        .unwrap();
    let pont = Pont::nouveau(
        commun::coquille::store(etat_videos()),
        f.as_weak(),
        runtime(),
        None,
        Arc::new(move || matin),
        Navigation::nouvelle(route("s1", "w1")),
        Profils::nouveaux(Arc::new(SourceFactice::default()), None),
    );
    let (lecteur, moteur) = Lecteur::factice();
    moteur.ajouter_media(VIDEO_SOLO, MediaFactice::video(300.0));
    let gestion = Arc::new(GestionLecteur::nouvelle(&f, lecteur));
    let navigateur = Arc::new(FauxNavigateur::default());
    pont.videos().definir_outils(OutilsVideos {
        lecteur: Some(gestion.clone()),
        navigateur: navigateur.clone(),
        journal: None,
    });
    pont.videos().brancher(&f);
    pont.demarrer(&f);
    assert_eq!(
        f.global::<DonneesCoquille>().get_vue().genre,
        GenreVue::Esquisse
    );
    assert_eq!(f.global::<DonneesVideos>().get_cartes().row_count(), 5);
    let origine = Origine::Autre(ORIGINE.to_string());

    f.global::<DonneesVideos>().invoke_lire("1006".into());
    let v = solo(&f);
    assert!(
        v.actif && v.ancre && v.lecture,
        "lecture ancrée dans le salon : {v:?}"
    );
    assert_eq!(v.titre.as_str(), "Vacances à la mer 2026");
    assert!(gestion.reservee_par(&origine));
    assert!(moteur
        .commandes()
        .iter()
        .any(|c| matches!(c, CommandeMoteur::Charger(s) if s.adresse == VIDEO_SOLO)));
    assert_eq!(
        minis(&f),
        0,
        "pas de mini-lecteur dans le salon de la vidéo"
    );
    moteur.vider_commandes();

    for (serveur, salon) in [("s1", "c1"), ("s2", "t1")] {
        pont.naviguer(&f, route(serveur, salon));
        let v = solo(&f);
        assert!(
            v.actif && !v.ancre && v.lecture,
            "{salon} : la lecture continue dans le mini-lecteur : {v:?}"
        );
        assert_eq!(minis(&f), 1, "{salon} : mini-lecteur dans le panneau");
        assert!(gestion.reservee_par(&origine));
        assert_eq!(gestion.lecteur().etat().phase, Phase::Pret);
    }
    let commandes = moteur.commandes();
    println!("372 C2 : commandes du moteur en changeant de salon {commandes:?}");
    assert!(
        interruptions(&commandes).is_empty(),
        "ni arrêt ni pause en changeant de salon : {commandes:?}"
    );
    assert!(commandes.contains(&CommandeMoteur::Video(false)));

    pont.naviguer(&f, route("s1", "w1"));
    let v = solo(&f);
    assert!(v.actif && v.ancre, "retour dans le salon : lecteur ancré");
    assert_eq!(minis(&f), 0);

    f.global::<DonneesVideos>().invoke_lire("1004".into());
    assert_eq!(
        navigateur.ouvertes.lock().unwrap().as_slice(),
        ["https://vimeo.com/76979871#t=0s"],
        "Vimeo s'ouvre dans le navigateur"
    );
    assert!(solo(&f).actif, "la lecture en cours n'est pas coupée");

    moteur.vider_commandes();
    f.global::<DonneesVideos>().invoke_solo_arreter();
    assert!(!solo(&f).actif);
    assert!(!gestion.reservee_par(&origine));
    let commandes = moteur.commandes();
    assert!(commandes.contains(&CommandeMoteur::Arreter));
    assert!(commandes.contains(&CommandeMoteur::Video(false)));
    assert_eq!(minis(&f), 0);
}
