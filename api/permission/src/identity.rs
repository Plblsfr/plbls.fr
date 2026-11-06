//! Vérification des en-têtes d'identité signés injectés par auth-gateway (voir docs/gateway.md de security.plbls.fr).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

fn mac(secret: &str, id: &str, roles: &str, client: &str, ts: u64) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("clé HMAC");
    m.update(format!("{id}\n{roles}\n{client}\n{ts}").as_bytes());
    m
}

/// HMAC-SHA256 (base64url) de `id\nroles\nclient\nts`.
pub fn sign(secret: &str, id: &str, roles: &str, client: &str, ts: u64) -> String {
    URL_SAFE_NO_PAD.encode(mac(secret, id, roles, client, ts).finalize().into_bytes())
}

/// Signature valide et `ts` de moins de 60 s.
pub fn verify(
    secret: &str,
    id: &str,
    roles: &str,
    client: &str,
    ts: u64,
    sig: &str,
    now: u64,
) -> bool {
    if secret.is_empty() || now.abs_diff(ts) > 60 {
        return false;
    }
    sign(secret, id, roles, client, ts)
        .as_bytes()
        .ct_eq(sig.as_bytes())
        .into()
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
    fn roundtrip_tamper_stale() {
        let secret = cle_aleatoire();
        let sig = sign(&secret, "u1", "user", "c", 1000);
        assert!(verify(&secret, "u1", "user", "c", 1000, &sig, 1030));
        assert!(!verify(&secret, "u1", "admin", "c", 1000, &sig, 1030));
        assert!(!verify(
            &cle_aleatoire(),
            "u1",
            "user",
            "c",
            1000,
            &sig,
            1030
        ));
        assert!(!verify(&secret, "u1", "user", "c", 1000, &sig, 1061));
    }

    #[test]
    fn empty_secret_never_valid() {
        let vide = String::new();
        let sig = sign(&vide, "u1", "user", "c", 1000);
        assert!(!verify(&vide, "u1", "user", "c", 1000, &sig, 1000));
    }
}
