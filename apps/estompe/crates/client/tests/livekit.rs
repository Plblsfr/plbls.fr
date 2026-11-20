#![cfg(feature = "livekit")]

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use estompe_client::audio::factice::SystemeFactice;
use estompe_client::audio::{FormatAudio, SensAudio, ECHANTILLONS_PAR_TRAME};
use estompe_client::http::ErreurApi;
use estompe_client::vocal::livekit::ConnecteurLiveKit;
use estompe_client::vocal::{
    ApiVocale, Dependances, EtatVocal, JetonSalle, Phase, Reglages, SessionVocale,
};
use futures_util::future::BoxFuture;
use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use tokio::time::Instant;

const VARIABLE_URL: &str = "ESTOMPE_LIVEKIT_URL";

fn base64url(octets: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut sortie = String::new();
    for bloc in octets.chunks(3) {
        let n = bloc
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, o)| n | (u32::from(*o) << (16 - 8 * i)));
        for i in 0..=bloc.len() {
            sortie.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    sortie
}

fn hmac_sha256(cle: &[u8], message: &[u8]) -> Vec<u8> {
    let mut bloc = [0u8; 64];
    if cle.len() > 64 {
        bloc[..32].copy_from_slice(&Sha256::digest(cle));
    } else {
        bloc[..cle.len()].copy_from_slice(cle);
    }
    let interne: Vec<u8> = bloc.iter().map(|o| o ^ 0x36).collect();
    let externe: Vec<u8> = bloc.iter().map(|o| o ^ 0x5c).collect();
    let h = Sha256::new()
        .chain_update(&interne)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(&externe)
        .chain_update(h)
        .finalize()
        .to_vec()
}

fn jeton(cle: &str, secret: &str, identite: &str, salle: &str) -> String {
    let maintenant = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let entete = base64url(br#"{"alg":"HS256","typ":"JWT"}"#);
    let charge = base64url(
        serde_json::json!({
            "iss": cle,
            "sub": identite,
            "name": identite,
            "nbf": maintenant - 10,
            "exp": maintenant + 600,
            "video": {
                "roomJoin": true,
                "room": salle,
                "canPublish": true,
                "canSubscribe": true,
                "canUpdateOwnMetadata": true
            }
        })
        .to_string()
        .as_bytes(),
    );
    let signe = format!("{entete}.{charge}");
    let signature = base64url(&hmac_sha256(secret.as_bytes(), signe.as_bytes()));
    format!("{signe}.{signature}")
}

struct ApiDeTest {
    url: String,
    cle: String,
    secret: String,
    identite: String,
}

impl ApiVocale for ApiDeTest {
    fn jeton(&self, salon: &str) -> BoxFuture<'_, Result<JetonSalle, ErreurApi>> {
        let salon = salon.to_string();
        Box::pin(async move {
            Ok(JetonSalle {
                url: self.url.clone(),
                jeton: jeton(&self.cle, &self.secret, &self.identite, &salon),
                salle: salon,
                max_participants: Some(10),
            })
        })
    }
}

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
