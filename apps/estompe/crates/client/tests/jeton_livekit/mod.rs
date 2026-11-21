use estompe_client::http::ErreurApi;
use estompe_client::vocal::{ApiVocale, JetonSalle};
use futures_util::future::BoxFuture;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn base64url(octets: &[u8]) -> String {
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

pub fn hmac_sha256(cle: &[u8], message: &[u8]) -> Vec<u8> {
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

pub fn jeton(cle: &str, secret: &str, identite: &str, salle: &str) -> String {
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

pub struct ApiDeTest {
    pub url: String,
    pub cle: String,
    pub secret: String,
    pub identite: String,
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
