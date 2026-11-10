use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::percent_decode_str;
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub struct Champs<'a> {
    pub id: &'a str,
    pub roles: &'a str,
    pub client: &'a str,
    pub ts: u64,
    pub name: &'a str,
    pub email: &'a str,
}

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

pub fn verify(secret: &str, c: &Champs, sig: &str, now: u64) -> bool {
    if secret.is_empty() || now.abs_diff(c.ts) > 60 {
        return false;
    }
    sign(secret, c).as_bytes().ct_eq(sig.as_bytes()).into()
}

pub fn decoder(v: &str) -> String {
    percent_decode_str(v).decode_utf8_lossy().into_owned()
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
            client: "c",
            ts: 1000,
            name,
            email,
        }
    }

    #[test]
    fn signature_valide_alteree_perimee() {
        let secret = cle_aleatoire();
        let c = champs("user", "demo", "demo@plbls.fr");
        let sig = sign(&secret, &c);
        assert!(verify(&secret, &c, &sig, 1030));
        assert!(!verify(
            &secret,
            &champs("admin", "demo", "demo@plbls.fr"),
            &sig,
            1030
        ));
        assert!(!verify(
            &secret,
            &champs("user", "root", "demo@plbls.fr"),
            &sig,
            1030
        ));
        assert!(!verify(
            &secret,
            &champs("user", "demo", "root@plbls.fr"),
            &sig,
            1030
        ));
        assert!(!verify(&cle_aleatoire(), &c, &sig, 1030));
        assert!(!verify(&secret, &c, &sig, 1061));
    }

    #[test]
    fn signature_roles_composes_et_nom_encode() {
        let secret = cle_aleatoire();
        let c = Champs {
            id: "u1",
            roles: "user,chat:admin",
            client: "plbls-mobile",
            ts: 1000,
            name: "j%C3%A9r%C3%B4me",
            email: "j@plbls.fr",
        };
        let sig = sign(&secret, &c);
        assert!(verify(&secret, &c, &sig, 1030));
    }

    #[test]
    fn secret_vide_jamais_valide() {
        let c = champs("user", "", "");
        let sig = sign("", &c);
        assert!(!verify("", &c, &sig, 1000));
    }

    #[test]
    fn decode_le_percent_encoding_utf8() {
        assert_eq!(decoder("J%C3%A9r%C3%B4me%20L%C5%93uf"), "Jérôme Lœuf");
        assert_eq!(decoder("demo@plbls.fr"), "demo@plbls.fr");
        assert_eq!(decoder("a%25b"), "a%b");
        assert_eq!(decoder("%FF"), "\u{FFFD}");
    }
}
