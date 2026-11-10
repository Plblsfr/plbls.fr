use std::{
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD},
};
use reqwest::{Client, StatusCode};
use ring::{
    aead, agreement, hkdf,
    rand::{SecureRandom, SystemRandom},
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair},
};
use serde_json::json;

const TAILLE_ENREGISTREMENT: usize = 4096;
const TTL: &str = "3600";
const SUJET_PAR_DEFAUT: &str = "admin@plbls.fr";

pub fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .expect("client HTTP")
    })
}

#[derive(Clone)]
pub struct Vapid {
    pub public: String,
    pub private: String,
    pub sujet: String,
}

impl Vapid {
    pub fn nouveau(public: &str, private: &str, sujet: &str) -> Option<Self> {
        if public.is_empty() || private.is_empty() {
            return None;
        }
        let sujet = if sujet.is_empty() {
            SUJET_PAR_DEFAUT
        } else {
            sujet
        };
        Some(Vapid {
            public: public.to_string(),
            private: private.to_string(),
            sujet: sujet.strip_prefix("mailto:").unwrap_or(sujet).to_string(),
        })
    }
}

pub struct Abonnement<'a> {
    pub endpoint: &'a str,
    pub p256dh: &'a str,
    pub auth: &'a str,
    pub urgent: bool,
}

fn base64_souple(cle: &str) -> Result<Vec<u8>, String> {
    let sans_bourrage = cle.trim_end_matches('=');
    URL_SAFE_NO_PAD
        .decode(sans_bourrage)
        .or_else(|_| STANDARD_NO_PAD.decode(sans_bourrage))
        .map_err(|_| "clé base64 invalide".to_string())
}

struct Longueur(usize);

impl hkdf::KeyType for Longueur {
    fn len(&self) -> usize {
        self.0
    }
}

fn deriver(sel: &[u8], secret: &[u8], info: &[u8], taille: usize) -> Result<Vec<u8>, String> {
    let mut sortie = vec![0u8; taille];
    hkdf::Salt::new(hkdf::HKDF_SHA256, sel)
        .extract(secret)
        .expand(&[info], Longueur(taille))
        .and_then(|okm| okm.fill(&mut sortie))
        .map_err(|_| "dérivation de clé impossible".to_string())?;
    Ok(sortie)
}

pub fn chiffrer(message: &[u8], p256dh: &[u8], auth: &[u8]) -> Result<Vec<u8>, String> {
    let rng = SystemRandom::new();
    let eph = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| "génération de clé impossible".to_string())?;
    let publique = eph
        .compute_public_key()
        .map_err(|_| "clé publique impossible".to_string())?;
    let publique = publique.as_ref().to_vec();
    let secret = agreement::agree_ephemeral(
        eph,
        &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, p256dh),
        |s| s.to_vec(),
    )
    .map_err(|_| "clé publique du navigateur invalide".to_string())?;
    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(p256dh);
    info.extend_from_slice(&publique);
    let ikm = deriver(auth, &secret, &info, 32)?;
    let mut sel = [0u8; 16];
    rng.fill(&mut sel)
        .map_err(|_| "aléa indisponible".to_string())?;
    let cle = deriver(&sel, &ikm, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = deriver(&sel, &ikm, b"Content-Encoding: nonce\0", 12)?;

    let mut sortie = sel.to_vec();
    sortie.extend_from_slice(&(TAILLE_ENREGISTREMENT as u32).to_be_bytes());
    sortie.push(publique.len() as u8);
    sortie.extend_from_slice(&publique);

    let place = TAILLE_ENREGISTREMENT - 16 - sortie.len();
    if message.len() + 1 > place {
        return Err("message trop long pour un enregistrement".to_string());
    }
    let mut donnees = message.to_vec();
    donnees.push(2);
    donnees.resize(place, 0);

    let cle = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cle)
            .map_err(|_| "clé de chiffrement invalide".to_string())?,
    );
    let nonce =
        aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| "nonce invalide".to_string())?;
    cle.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut donnees)
        .map_err(|_| "chiffrement impossible".to_string())?;
    sortie.extend_from_slice(&donnees);
    Ok(sortie)
}

fn origine(endpoint: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(endpoint).map_err(|e| e.to_string())?;
    let hote = url.host_str().ok_or("endpoint sans hôte")?;
    Ok(match url.port() {
        Some(p) => format!("{}://{hote}:{p}", url.scheme()),
        None => format!("{}://{hote}", url.scheme()),
    })
}

pub fn en_tete_vapid(vapid: &Vapid, endpoint: &str) -> Result<String, String> {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        + 12 * 3600;
    let sujet = if vapid.sujet.starts_with("https:") {
        vapid.sujet.clone()
    } else {
        format!("mailto:{}", vapid.sujet)
    };
    let entete = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let revendications = URL_SAFE_NO_PAD
        .encode(json!({ "aud": origine(endpoint)?, "exp": exp, "sub": sujet }).to_string());
    let a_signer = format!("{entete}.{revendications}");
    let privee = base64_souple(&vapid.private)?;
    let publique = base64_souple(&vapid.public)?;
    let rng = SystemRandom::new();
    let paire = EcdsaKeyPair::from_private_key_and_public_key(
        &ECDSA_P256_SHA256_FIXED_SIGNING,
        &privee,
        &publique,
        &rng,
    )
    .map_err(|_| "clés VAPID invalides".to_string())?;
    let signature = paire
        .sign(&rng, a_signer.as_bytes())
        .map_err(|_| "signature VAPID impossible".to_string())?;
    Ok(format!(
        "vapid t={a_signer}.{}, k={}",
        URL_SAFE_NO_PAD.encode(signature.as_ref()),
        URL_SAFE_NO_PAD.encode(&publique)
    ))
}

pub async fn envoyer(
    vapid: &Vapid,
    abonnement: &Abonnement<'_>,
    charge: &[u8],
) -> Result<bool, String> {
    let p256dh = base64_souple(abonnement.p256dh)?;
    let auth = base64_souple(abonnement.auth)?;
    let corps = chiffrer(charge, &p256dh, &auth)?;
    let reponse = client()
        .post(abonnement.endpoint)
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("TTL", TTL)
        .header("Urgency", if abonnement.urgent { "high" } else { "normal" })
        .header("Authorization", en_tete_vapid(vapid, abonnement.endpoint)?)
        .body(corps)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let statut = reponse.status();
    if statut == StatusCode::GONE || statut == StatusCode::NOT_FOUND {
        return Ok(true);
    }
    if statut.as_u16() >= 300 {
        let hote = reponse.url().host_str().unwrap_or_default().to_string();
        let mut reponse = reponse;
        let mut corps = Vec::new();
        while corps.len() < 512 {
            match reponse.chunk().await {
                Ok(Some(morceau)) => corps.extend_from_slice(&morceau),
                _ => break,
            }
        }
        tracing::warn!(
            "service push {hote} : {statut} : {}",
            String::from_utf8_lossy(&corps[..corps.len().min(512)]).trim()
        );
        return Err(format!("service push {hote} : {statut}"));
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::faux_avec_entetes;
    use ring::signature::KeyPair;

    fn cles_vapid() -> (String, String) {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let paire =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
                .unwrap();
        let fin = pkcs8.as_ref().len() - 65 - 5;
        (
            URL_SAFE_NO_PAD.encode(&pkcs8.as_ref()[fin - 32..fin]),
            URL_SAFE_NO_PAD.encode(paire.public_key().as_ref()),
        )
    }

    fn recepteur() -> (agreement::EphemeralPrivateKey, Vec<u8>) {
        let rng = SystemRandom::new();
        let k = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let p = k.compute_public_key().unwrap().as_ref().to_vec();
        (k, p)
    }

    fn dechiffrer(
        corps: &[u8],
        recu: agreement::EphemeralPrivateKey,
        p256dh: &[u8],
        auth: &[u8],
    ) -> Vec<u8> {
        let sel = &corps[..16];
        let longueur_cle = corps[20] as usize;
        let emetteur = &corps[21..21 + longueur_cle];
        let secret = agreement::agree_ephemeral(
            recu,
            &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, emetteur),
            |s| s.to_vec(),
        )
        .unwrap();
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(p256dh);
        info.extend_from_slice(emetteur);
        let ikm = deriver(auth, &secret, &info, 32).unwrap();
        let cle = deriver(sel, &ikm, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = deriver(sel, &ikm, b"Content-Encoding: nonce\0", 12).unwrap();
        let cle = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &cle).unwrap());
        let mut chiffre = corps[21 + longueur_cle..].to_vec();
        let clair = cle
            .open_in_place(
                aead::Nonce::try_assume_unique_for_key(&nonce).unwrap(),
                aead::Aad::empty(),
                &mut chiffre,
            )
            .unwrap();
        let fin = clair.iter().rposition(|o| *o != 0).unwrap();
        assert_eq!(clair[fin], 2);
        clair[..fin].to_vec()
    }

    #[test]
    fn chiffrement_rfc8291_dechiffrable_et_de_taille_fixe() {
        let (recu, p256dh) = recepteur();
        let auth = [7u8; 16];
        let corps = chiffrer(br#"{"title":"a"}"#, &p256dh, &auth).unwrap();
        assert_eq!(corps.len(), TAILLE_ENREGISTREMENT);
        assert_eq!(
            &corps[16..20],
            &(TAILLE_ENREGISTREMENT as u32).to_be_bytes()
        );
        assert_eq!(corps[20], 65);
        assert_eq!(
            dechiffrer(&corps, recu, &p256dh, &auth),
            br#"{"title":"a"}"#
        );
        assert!(chiffrer(&[b'x'; 4000], &p256dh, &auth).is_err());
        assert!(chiffrer(b"x", &[1, 2, 3], &auth).is_err());
    }

    #[test]
    fn cles_en_base64_souple() {
        assert_eq!(base64_souple("-_8").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(base64_souple("-_8=").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(base64_souple("+/8").unwrap(), vec![0xfb, 0xff]);
        assert!(base64_souple("é").is_err());
    }

    #[tokio::test]
    async fn envoi_vapid() {
        let (privee, publique) = cles_vapid();
        let (recu, p256dh) = recepteur();
        let auth = [9u8; 16];
        let statut = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(201));
        let s = statut.clone();
        let serveur = faux_avec_entetes(move |_| {
            (
                s.load(std::sync::atomic::Ordering::SeqCst),
                vec![],
                b"BadJwtToken".to_vec(),
            )
        })
        .await;
        let abonnement = Abonnement {
            endpoint: &serveur.url,
            p256dh: &URL_SAFE_NO_PAD.encode(&p256dh),
            auth: &URL_SAFE_NO_PAD.encode(auth),
            urgent: true,
        };
        for sujet in ["", "mailto:admin@plbls.fr", "admin@plbls.fr"] {
            let vapid = Vapid::nouveau(&publique, &privee, sujet).unwrap();
            assert_eq!(
                envoyer(&vapid, &abonnement, br#"{"title":"a"}"#).await,
                Ok(false)
            );
            let r = serveur.requetes().pop().unwrap();
            assert_eq!(r.entete("content-encoding"), "aes128gcm");
            assert_eq!(r.entete("ttl"), "3600");
            assert_eq!(r.entete("urgency"), "high");
            let autorisation = r.entete("authorization");
            assert!(autorisation.starts_with("vapid t="), "{autorisation}");
            assert!(autorisation.ends_with(&format!(", k={publique}")));
            let jwt = autorisation
                .trim_start_matches("vapid t=")
                .split(',')
                .next()
                .unwrap();
            let revendications: serde_json::Value = serde_json::from_slice(
                &URL_SAFE_NO_PAD
                    .decode(jwt.split('.').nth(1).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(revendications["sub"], "mailto:admin@plbls.fr");
            assert_eq!(revendications["aud"], serveur.url);
            assert!(revendications["exp"].as_u64().unwrap() > 0);
            assert_eq!(
                URL_SAFE_NO_PAD
                    .decode(jwt.split('.').nth(2).unwrap())
                    .unwrap()
                    .len(),
                64
            );
        }
        let r = serveur.requetes().pop().unwrap();
        let vapid = Vapid::nouveau(&publique, &privee, "").unwrap();
        assert_eq!(
            dechiffrer(&r.corps, recu, &p256dh, &auth),
            br#"{"title":"a"}"#
        );
        statut.store(410, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(envoyer(&vapid, &abonnement, b"x").await, Ok(true));
        statut.store(404, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(envoyer(&vapid, &abonnement, b"x").await, Ok(true));
        statut.store(403, std::sync::atomic::Ordering::SeqCst);
        let e = envoyer(&vapid, &abonnement, b"x").await.unwrap_err();
        assert!(e.contains("403"), "{e}");
    }

    #[test]
    fn vapid_sans_cles_desactive() {
        assert!(Vapid::nouveau("", "x", "").is_none());
        assert!(Vapid::nouveau("x", "", "").is_none());
    }
}
