use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use reqwest::StatusCode;
use ring::{
    rand::SystemRandom,
    signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{sync::Mutex, time::Instant};

use super::webpush::client;

const URI_JETON: &str = "https://oauth2.googleapis.com/token";
const GRANT: &str = "urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer";

#[derive(Deserialize)]
struct CompteDeService {
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    client_email: String,
    #[serde(default)]
    private_key: String,
    #[serde(default)]
    token_uri: String,
}

pub struct Fcm {
    email: String,
    uri_jeton: String,
    cle: RsaKeyPair,
    url_envoi: String,
    acces: Mutex<Option<(String, Instant)>>,
}

impl Fcm {
    pub fn depuis_base64(compte_b64: &str) -> Result<Option<Self>, String> {
        let compte_b64 = compte_b64.trim();
        if compte_b64.is_empty() {
            return Ok(None);
        }
        let brut = STANDARD
            .decode(compte_b64)
            .map_err(|_| "FCM_SERVICE_ACCOUNT : base64 attendu".to_string())?;
        let compte: CompteDeService = serde_json::from_slice(&brut)
            .ok()
            .filter(|c: &CompteDeService| !c.project_id.is_empty() && !c.client_email.is_empty())
            .ok_or("FCM_SERVICE_ACCOUNT invalide")?;
        let cle = lire_cle(&compte.private_key)?;
        Ok(Some(Fcm {
            uri_jeton: if compte.token_uri.is_empty() {
                URI_JETON.to_string()
            } else {
                compte.token_uri
            },
            email: compte.client_email,
            cle,
            url_envoi: format!(
                "https://fcm.googleapis.com/v1/projects/{}/messages:send",
                compte.project_id
            ),
            acces: Mutex::new(None),
        }))
    }

    #[cfg(test)]
    fn avec_url_envoi(mut self, url: &str) -> Self {
        self.url_envoi = url.to_string();
        self
    }

    async fn jeton_acces(&self) -> Result<String, String> {
        let mut acces = self.acces.lock().await;
        if let Some((jeton, expire)) = acces.as_ref()
            && Instant::now() < *expire
        {
            return Ok(jeton.clone());
        }
        let maintenant = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let entete = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let revendications = URL_SAFE_NO_PAD.encode(
            json!({
                "iss": self.email,
                "scope": "https://www.googleapis.com/auth/firebase.messaging",
                "aud": self.uri_jeton,
                "iat": maintenant,
                "exp": maintenant + 3600,
            })
            .to_string(),
        );
        let a_signer = format!("{entete}.{revendications}");
        let mut signature = vec![0u8; self.cle.public().modulus_len()];
        self.cle
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                a_signer.as_bytes(),
                &mut signature,
            )
            .map_err(|_| "signature du jeton FCM impossible".to_string())?;
        let corps = format!(
            "grant_type={GRANT}&assertion={a_signer}.{}",
            URL_SAFE_NO_PAD.encode(&signature)
        );
        let reponse = client()
            .post(&self.uri_jeton)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(corps)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let statut = reponse.status();
        let octets = reponse.bytes().await.map_err(|e| e.to_string())?;
        let lu: Value = serde_json::from_slice(&octets).unwrap_or(Value::Null);
        let (Some(jeton), true) = (lu["access_token"].as_str(), statut == StatusCode::OK) else {
            return Err(format!("jeton FCM : {statut}"));
        };
        let duree = lu["expires_in"].as_i64().unwrap_or(0) - 60;
        let expire = Instant::now() + Duration::from_secs(duree.max(0) as u64);
        *acces = Some((jeton.to_string(), expire));
        Ok(jeton.to_string())
    }

    pub async fn envoyer(&self, jeton: &str, charge: &[u8]) -> Result<bool, String> {
        let champs: serde_json::Map<String, Value> =
            serde_json::from_slice(charge).map_err(|e| e.to_string())?;
        let donnees: serde_json::Map<String, Value> = champs
            .into_iter()
            .map(|(k, v)| {
                let texte = match v {
                    Value::String(s) => s,
                    autre => autre.to_string(),
                };
                (k, Value::String(texte))
            })
            .collect();
        let acces = self.jeton_acces().await?;
        let corps = json!({ "message": {
            "token": jeton,
            "data": donnees,
            "android": { "priority": "HIGH" },
        }});
        let reponse = client()
            .post(&self.url_envoi)
            .header("Authorization", format!("Bearer {acces}"))
            .header("Content-Type", "application/json")
            .body(corps.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let statut = reponse.status();
        if statut == StatusCode::NOT_FOUND || statut == StatusCode::GONE {
            return Ok(true);
        }
        if statut.as_u16() >= 300 {
            return Err(format!("FCM : {statut}"));
        }
        Ok(false)
    }
}

fn lire_cle(pem: &str) -> Result<RsaKeyPair, String> {
    let corps: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN PRIVATE KEY-----"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END"))
        .map(str::trim)
        .collect();
    if corps.is_empty() {
        return Err("FCM_SERVICE_ACCOUNT : clé privée illisible".to_string());
    }
    let der = STANDARD
        .decode(corps)
        .map_err(|_| "FCM_SERVICE_ACCOUNT : clé privée illisible".to_string())?;
    RsaKeyPair::from_pkcs8(&der)
        .map_err(|_| "FCM_SERVICE_ACCOUNT : clé privée RSA attendue".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::faux;
    use std::sync::Arc;

    const CLE: &str = include_str!("cle_rsa_test.pem");

    fn compte(uri: &str) -> String {
        STANDARD.encode(
            json!({ "project_id": "p", "client_email": "x@p.iam", "private_key": CLE, "token_uri": uri })
                .to_string(),
        )
    }

    #[tokio::test]
    async fn envoi_fcm_en_message_data() {
        let serveur = Arc::new(
            faux(|r| {
                if r.chemin == "/token" {
                    (200, br#"{"access_token":"AT","expires_in":3600}"#.to_vec())
                } else if String::from_utf8_lossy(&r.corps).contains("mort") {
                    (404, Vec::new())
                } else {
                    (200, Vec::new())
                }
            })
            .await,
        );
        let fcm = Fcm::depuis_base64(&compte(&format!("{}/token", serveur.url)))
            .unwrap()
            .unwrap()
            .avec_url_envoi(&format!("{}/send", serveur.url));
        let charge =
            br##"{"title":"T","body":"B","tag":"dm-1","url":"/#/dm/1","kind":"dm","silent":true}"##;
        assert_eq!(fcm.envoyer("vivant", charge).await, Ok(false));
        assert_eq!(fcm.envoyer("mort", charge).await, Ok(true));
        let requetes = serveur.requetes();
        let jetons: Vec<_> = requetes.iter().filter(|r| r.chemin == "/token").collect();
        let envois: Vec<_> = requetes.iter().filter(|r| r.chemin == "/send").collect();
        assert_eq!((jetons.len(), envois.len()), (1, 2));
        let formulaire = String::from_utf8_lossy(&jetons[0].corps).to_string();
        assert!(
            formulaire.starts_with("grant_type=urn%3Aietf"),
            "{formulaire}"
        );
        assert_eq!(formulaire.matches('.').count(), 2);
        assert_eq!(envois[0].entete("authorization"), "Bearer AT");
        let message: Value = serde_json::from_slice(&envois[0].corps).unwrap();
        assert_eq!(message["message"]["token"], "vivant");
        assert!(message["message"].get("notification").is_none());
        assert_eq!(message["message"]["android"]["priority"], "HIGH");
        assert_eq!(
            message["message"]["data"],
            json!({ "title": "T", "body": "B", "tag": "dm-1", "url": "/#/dm/1", "kind": "dm", "silent": "true" })
        );
    }

    #[tokio::test]
    async fn refus_du_jeton_et_erreur_fcm() {
        let serveur = faux(|r| {
            if r.chemin == "/token" {
                (401, Vec::new())
            } else {
                (500, Vec::new())
            }
        })
        .await;
        let fcm = Fcm::depuis_base64(&compte(&format!("{}/token", serveur.url)))
            .unwrap()
            .unwrap()
            .avec_url_envoi(&format!("{}/send", serveur.url));
        let e = fcm.envoyer("t", b"{}").await.unwrap_err();
        assert!(e.starts_with("jeton FCM"), "{e}");
    }

    #[test]
    fn configuration_du_compte() {
        let brut =
            json!({ "project_id": "p", "client_email": "x@p.iam", "private_key": CLE }).to_string();
        assert!(Fcm::depuis_base64(&brut).is_err());
        assert!(Fcm::depuis_base64("").unwrap().is_none());
        assert!(Fcm::depuis_base64("  ").unwrap().is_none());
        assert!(Fcm::depuis_base64(&STANDARD.encode("{}")).is_err());
        let sans_cle = STANDARD.encode(
            json!({ "project_id": "p", "client_email": "x@p.iam", "private_key": "nope" })
                .to_string(),
        );
        assert!(Fcm::depuis_base64(&sans_cle).is_err());
        assert!(Fcm::depuis_base64(&compte("")).unwrap().is_some());
    }
}
