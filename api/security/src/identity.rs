use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Identité propagée aux micro-services.
/// `roles` = rôles realm tels quels + rôles client sous la forme `client:role`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub email: String,
    pub roles: Vec<String>,
    pub client: String,
}

impl Identity {
    pub fn has_any_role(&self, wanted: &[String]) -> bool {
        wanted.iter().any(|w| self.roles.contains(w))
    }
}

fn mac(secret: &str, id: &str, roles: &str, client: &str, ts: u64) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("clé HMAC");
    m.update(format!("{id}\n{roles}\n{client}\n{ts}").as_bytes());
    m
}

/// HMAC-SHA256 (base64url) de `id\nroles\nclient\nts`.
pub fn sign(secret: &str, id: &str, roles: &str, client: &str, ts: u64) -> String {
    URL_SAFE_NO_PAD.encode(mac(secret, id, roles, client, ts).finalize().into_bytes())
}

/// Côté micro-service : signature valide et `ts` de moins de 60 s.
#[allow(dead_code)]
pub fn verify(
    secret: &str,
    id: &str,
    roles: &str,
    client: &str,
    ts: u64,
    sig: &str,
    now: u64,
) -> bool {
    if now.abs_diff(ts) > 60 {
        return false;
    }
    let expected = sign(secret, id, roles, client, ts);
    expected.as_bytes().ct_eq(sig.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cle_aleatoire() -> String {
        use std::hash::BuildHasher;
        std::collections::hash_map::RandomState::new()
            .hash_one(std::process::id())
            .to_string()
    }

    #[test]
    fn sign_verify_roundtrip() {
        let secret = cle_aleatoire();
        let sig = sign(&secret, "u1", "user,chat:admin", "plbls-mobile", 1000);
        assert!(verify(
            &secret,
            "u1",
            "user,chat:admin",
            "plbls-mobile",
            1000,
            &sig,
            1030
        ));
    }

    #[test]
    fn rejects_tampering_and_stale() {
        let secret = cle_aleatoire();
        let sig = sign(&secret, "u1", "user", "c", 1000);
        assert!(!verify(&secret, "u1", "admin", "c", 1000, &sig, 1000));
        assert!(!verify(
            &cle_aleatoire(),
            "u1",
            "user",
            "c",
            1000,
            &sig,
            1000
        ));
        assert!(!verify(&secret, "u1", "user", "c", 1000, &sig, 1061));
    }
}
