use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
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

pub struct Champs<'a> {
    pub id: &'a str,
    pub roles: &'a str,
    pub client: &'a str,
    pub ts: u64,
    pub name: &'a str,
    pub email: &'a str,
}

const A_ENCODER: &AsciiSet = &CONTROLS.add(b' ').add(b'%');

pub fn encoder(v: &str) -> String {
    utf8_percent_encode(v, A_ENCODER).to_string()
}

/// HMAC-SHA256 (base64url) de `id\nroles\nclient\nts\nname\nemail`.
pub fn sign(secret: &str, c: &Champs) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("clé HMAC");
    m.update(
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            c.id, c.roles, c.client, c.ts, c.name, c.email
        )
        .as_bytes(),
    );
    URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())
}

/// Côté micro-service : signature valide et `ts` de moins de 60 s.
#[allow(dead_code)]
pub fn verify(secret: &str, c: &Champs, sig: &str, now: u64) -> bool {
    if now.abs_diff(c.ts) > 60 {
        return false;
    }
    sign(secret, c).as_bytes().ct_eq(sig.as_bytes()).into()
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

    fn champs<'a>(roles: &'a str, name: &'a str, email: &'a str) -> Champs<'a> {
        Champs {
            id: "u1",
            roles,
            client: "plbls-mobile",
            ts: 1000,
            name,
            email,
        }
    }

    #[test]
    fn sign_verify_roundtrip() {
        let secret = cle_aleatoire();
        let c = champs("user,chat:admin", "j%C3%A9r%C3%B4me", "j@plbls.fr");
        let sig = sign(&secret, &c);
        assert!(verify(&secret, &c, &sig, 1030));
    }

    #[test]
    fn rejects_tampering_and_stale() {
        let secret = cle_aleatoire();
        let c = champs("user", "demo", "demo@plbls.fr");
        let sig = sign(&secret, &c);
        assert!(!verify(
            &secret,
            &champs("admin", "demo", "demo@plbls.fr"),
            &sig,
            1000
        ));
        assert!(!verify(
            &secret,
            &champs("user", "root", "demo@plbls.fr"),
            &sig,
            1000
        ));
        assert!(!verify(
            &secret,
            &champs("user", "demo", "root@plbls.fr"),
            &sig,
            1000
        ));
        assert!(!verify(&cle_aleatoire(), &c, &sig, 1000));
        assert!(!verify(&secret, &c, &sig, 1061));
    }

    #[test]
    fn encode_en_ascii_visible() {
        assert_eq!(encoder("demo@plbls.fr"), "demo@plbls.fr");
        assert_eq!(encoder("Jérôme Lœuf"), "J%C3%A9r%C3%B4me%20L%C5%93uf");
        assert_eq!(encoder("a%b\nc"), "a%25b%0Ac");
        assert!(axum::http::HeaderValue::from_str(&encoder("Zoé 😀")).is_ok());
    }
}
