#![cfg(feature = "livekit")]

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod jeton_livekit;

use estompe_client::audio::factice::SystemeFactice;
use estompe_client::audio::{FormatAudio, SensAudio, ECHANTILLONS_PAR_TRAME};
use estompe_client::vocal::livekit::ConnecteurLiveKit;
use estompe_client::vocal::{Dependances, EtatVocal, Phase, Reglages, SessionVocale};
use jeton_livekit::{base64url, hmac_sha256, jeton, ApiDeTest};
use tokio::runtime::Handle;
use tokio::time::Instant;

const VARIABLE_URL: &str = "ESTOMPE_LIVEKIT_URL";

fn poste(url: &str, identite: &str) -> (SessionVocale, SystemeFactice) {
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
            apm: Some(estompe_client::vocal::livekit::apm()),
            reglages: Reglages::default(),
        },
    );
    (session, audio)
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
    let salle = format!(
        "essai-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
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
