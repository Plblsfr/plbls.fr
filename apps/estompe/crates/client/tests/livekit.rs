#![cfg(feature = "livekit")]

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod jeton_livekit;

use estompe_client::audio::factice::SystemeFactice;
use estompe_client::audio::{FormatAudio, SensAudio, ECHANTILLONS_PAR_TRAME};
use estompe_client::vocal::livekit::ConnecteurLiveKit;
use estompe_client::vocal::{
    CamerasFactices, CleVideo, DemandeVideo, Dependances, EtatVocal, Phase, Reglages,
    ReglagesPartage, ResolutionPartage, SessionVocale,
};
use estompe_plateforme::{CaptureEcran, EcransFactices, SourceCapture, TypeSource};
use jeton_livekit::{base64url, hmac_sha256, jeton, ApiDeTest};
use tokio::runtime::Handle;
use tokio::time::Instant;

const VARIABLE_URL: &str = "ESTOMPE_LIVEKIT_URL";

fn poste(url: &str, identite: &str) -> (SessionVocale, SystemeFactice) {
    let (session, audio, _) = poste_avec_camera(url, identite);
    (session, audio)
}

fn poste_avec_camera(
    url: &str,
    identite: &str,
) -> (SessionVocale, SystemeFactice, CamerasFactices) {
    let (session, audio, cameras, _) = poste_complet(url, identite);
    (session, audio, cameras)
}

fn source(id: &str, genre: TypeSource, largeur: u32, hauteur: u32) -> SourceCapture {
    SourceCapture {
        id: id.to_string(),
        nom: id.to_string(),
        application: String::new(),
        genre,
        largeur,
        hauteur,
        principale: false,
        processus: None,
        estompe: false,
    }
}

fn poste_complet(
    url: &str,
    identite: &str,
) -> (
    SessionVocale,
    SystemeFactice,
    CamerasFactices,
    EcransFactices,
) {
    let ecrans = EcransFactices::nouveaux();
    ecrans.brancher(source("ecran:0", TypeSource::Ecran, 1920, 1080));
    ecrans.brancher(source("fenetre:1", TypeSource::Fenetre, 1280, 720));
    let cameras = CamerasFactices::nouvelles();
    cameras.brancher("c1", "Caméra factice");
    cameras.brancher("c2", "Seconde caméra factice");
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
        SensAudio::Sortie,
        "hp",
        "Haut-parleurs",
        FormatAudio {
            frequence: 48_000,
            canaux: 1,
        },
    );
    let session = SessionVocale::demarrer(
        &Handle::current(),
        Dependances {
            api: Arc::new(ApiDeTest {
                url: url.to_string(),
                cle: std::env::var("ESTOMPE_LIVEKIT_CLE").unwrap_or_else(|_| "devkey".into()),
                secret: std::env::var("ESTOMPE_LIVEKIT_SECRET").unwrap_or_else(|_| "secret".into()),
                identite: identite.to_string(),
            }),
            connecteur: Arc::new(ConnecteurLiveKit),
            audio: Arc::new(audio.clone()),
            cameras: Arc::new(cameras.clone()),
            ecrans: Arc::new(ecrans.clone()),
            sons: Arc::new(estompe_plateforme::Indisponible),
            apm: Some(estompe_client::vocal::livekit::apm()),
            reglages: Reglages::default(),
        },
    );
    (session, audio, cameras, ecrans)
}

fn salle_d_essai() -> String {
    format!(
        "essai-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_camera_est_publiee_changee_puis_retiree() {
    let Ok(url) = std::env::var(VARIABLE_URL) else {
        eprintln!(
            "ignoré : {VARIABLE_URL} absent (serveur LiveKit de deploy/docker-compose.dev.yml)"
        );
        return;
    };
    let salle = salle_d_essai();
    let (alice, _, cameras) = poste_avec_camera(&url, "alice");
    let (bob, _, _) = poste_avec_camera(&url, "bob");
    alice.rejoindre(&salle);
    attendre(&alice, Duration::from_secs(30), |e| {
        e.phase == Phase::Connecte
    })
    .await;
    bob.rejoindre(&salle);
    attendre(&bob, Duration::from_secs(30), |e| {
        e.participants.contains_key("alice")
    })
    .await;
    let cle = CleVideo::camera("alice");
    bob.definir_affichage(
        [(
            cle.clone(),
            DemandeVideo {
                largeur: 640,
                hauteur: 360,
            },
        )]
        .into_iter()
        .collect(),
    );
    alice.activer_camera(true);
    attendre(&alice, Duration::from_secs(30), |e| e.camera_active).await;
    assert_eq!(cameras.ouvertes(), vec!["c1".to_string()]);
    attendre(&bob, Duration::from_secs(30), |e| e.video(&cle)).await;
    let recepteur = bob.videos().recepteur(&cle);
    let debut = Instant::now();
    while recepteur.images_recues() == 0 && debut.elapsed() < Duration::from_secs(30) {
        cameras.pousser(1);
        tokio::time::sleep(Duration::from_millis(33)).await;
    }
    assert!(
        recepteur.images_recues() > 0,
        "bob n'a reçu aucune image d'alice en 30 s"
    );
    alice.choisir_camera(Some("c2".into()));
    attendre(&alice, Duration::from_secs(30), |e| {
        e.camera.as_deref() == Some("c2")
    })
    .await;
    assert_eq!(cameras.ouvertes(), vec!["c2".to_string()]);
    alice.activer_camera(false);
    attendre(&alice, Duration::from_secs(30), |e| !e.camera_active).await;
    assert!(cameras.ouvertes().is_empty());
    attendre(&bob, Duration::from_secs(30), |e| !e.video(&cle)).await;
    alice.fermer().await;
    bob.fermer().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn le_partage_est_publie_change_de_source_et_de_qualite_puis_retire() {
    let Ok(url) = std::env::var(VARIABLE_URL) else {
        eprintln!(
            "ignoré : {VARIABLE_URL} absent (serveur LiveKit de deploy/docker-compose.dev.yml)"
        );
        return;
    };
    let salle = salle_d_essai();
    let (alice, _, _, ecrans) = poste_complet(&url, "alice");
    let (bob, _, _, _) = poste_complet(&url, "bob");
    alice.rejoindre(&salle);
    attendre(&alice, Duration::from_secs(30), |e| {
        e.phase == Phase::Connecte
    })
    .await;
    bob.rejoindre(&salle);
    attendre(&bob, Duration::from_secs(30), |e| {
        e.participants.contains_key("alice")
    })
    .await;
    let cle = CleVideo::partage("alice");
    bob.regarder("alice", Some(true));
    bob.definir_affichage(
        [(
            cle.clone(),
            DemandeVideo {
                largeur: 1280,
                hauteur: 720,
            },
        )]
        .into_iter()
        .collect(),
    );
    let sources = ecrans.sources().unwrap();
    alice.partager(sources[0].clone());
    let etat = attendre(&alice, Duration::from_secs(30), |e| e.partage.is_some()).await;
    assert!(etat.pistes.contains_key(&cle), "piste du partage connue");
    attendre(&bob, Duration::from_secs(30), |e| e.video(&cle)).await;
    let recepteur = bob.videos().recepteur(&cle);
    let pousser_jusqu_a = |cible: u64, source: &'static str| {
        let ecrans = ecrans.clone();
        let recepteur = recepteur.clone();
        async move {
            let debut = Instant::now();
            while recepteur.images_recues() < cible && debut.elapsed() < Duration::from_secs(30) {
                ecrans.pousser(source);
                tokio::time::sleep(Duration::from_millis(33)).await;
            }
            assert!(
                recepteur.images_recues() >= cible,
                "bob n'a pas reçu le partage d'alice ({source}) en 30 s"
            );
        }
    };
    pousser_jusqu_a(1, "ecran:0").await;
    alice.changer_source(sources[1].clone());
    attendre(&alice, Duration::from_secs(30), |e| {
        e.partage
            .as_ref()
            .is_some_and(|p| p.source.id == "fenetre:1")
    })
    .await;
    assert_eq!(ecrans.ouverts(), vec![("fenetre:1".to_string(), 30)]);
    let deja = recepteur.images_recues();
    pousser_jusqu_a(deja + 5, "fenetre:1").await;
    alice.regler_partage(ReglagesPartage {
        resolution: ResolutionPartage::R720,
        ips: 60,
        ..ReglagesPartage::default()
    });
    attendre(&alice, Duration::from_secs(30), |e| {
        e.partage.as_ref().is_some_and(|p| p.qualite.ips == 60)
    })
    .await;
    assert_eq!(ecrans.ouverts(), vec![("fenetre:1".to_string(), 60)]);
    let deja = recepteur.images_recues();
    pousser_jusqu_a(deja + 5, "fenetre:1").await;
    alice.arreter_partage();
    attendre(&alice, Duration::from_secs(30), |e| e.partage.is_none()).await;
    assert!(ecrans.ouverts().is_empty());
    attendre(&bob, Duration::from_secs(30), |e| !e.video(&cle)).await;
    alice.fermer().await;
    bob.fermer().await;
}

async fn attendre(
    session: &SessionVocale,
    delai: Duration,
    condition: impl Fn(&EtatVocal) -> bool,
) -> EtatVocal {
    let mut etat = session.etat();
    tokio::time::timeout(delai, async {
        loop {
            let courant = etat.borrow_and_update().clone();
            if condition(&courant) {
                return courant;
            }
            etat.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|_| panic!("état attendu jamais atteint : {:?}", session.instantane()))
}

#[test]
fn le_jeton_de_test_est_un_jwt_hs256_valide() {
    assert_eq!(base64url(b"f"), "Zg");
    assert_eq!(base64url(b"fo"), "Zm8");
    assert_eq!(base64url(b"foo"), "Zm9v");
    let signature = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
    assert_eq!(
        signature
            .iter()
            .map(|o| format!("{o:02x}"))
            .collect::<String>(),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(jeton("k", "s", "u", "r").split('.').count(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deux_clients_se_voient_s_entendent_et_la_fermeture_retire_le_participant() {
    let Ok(url) = std::env::var(VARIABLE_URL) else {
        eprintln!(
            "ignoré : {VARIABLE_URL} absent (serveur LiveKit de deploy/docker-compose.dev.yml)"
        );
        return;
    };
    let salle = salle_d_essai();
    let (alice, micro_alice) = poste(&url, "alice");
    let (bob, sortie_bob) = poste(&url, "bob");
    alice.rejoindre(&salle);
    attendre(&alice, Duration::from_secs(30), |e| {
        e.phase == Phase::Connecte
    })
    .await;
    bob.rejoindre(&salle);
    let etat = attendre(&bob, Duration::from_secs(30), |e| {
        e.participants.contains_key("alice") && e.participants.contains_key("bob")
    })
    .await;
    assert_eq!(etat.identite.as_deref(), Some("bob"));
    attendre(&alice, Duration::from_secs(30), |e| {
        e.participants.contains_key("bob")
    })
    .await;

    alice.basculer_micro();
    attendre(&bob, Duration::from_secs(30), |e| {
        e.affiches().get("alice").is_some_and(|a| a.muet)
    })
    .await;
    alice.basculer_micro();

    let debut = Instant::now();
    let mut entendu = false;
    while debut.elapsed() < Duration::from_secs(30) && !entendu {
        let trame: Vec<f32> = (0..ECHANTILLONS_PAR_TRAME)
            .map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin())
            .collect();
        micro_alice.pousser("micro", &trame);
        entendu = sortie_bob
            .tirer("hp", ECHANTILLONS_PAR_TRAME)
            .is_some_and(|s| s.iter().any(|e| e.abs() > 0.05));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(entendu, "bob n'a reçu aucun son d'alice en 30 s");

    let fermeture = Instant::now();
    bob.fermer().await;
    attendre(&alice, Duration::from_secs(5), |e| {
        !e.participants.contains_key("bob")
    })
    .await;
    assert!(fermeture.elapsed() < Duration::from_secs(5));
    alice.fermer().await;
}
