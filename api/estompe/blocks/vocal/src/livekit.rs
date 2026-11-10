use std::{
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use estompe_core::config;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Hmac256 = Hmac<Sha256>;

pub const MAX_PARTICIPANTS: usize = 10;
const VALIDITE_JETON: i64 = 3600;
const VALIDITE_API: i64 = 60;
const DELAI: Duration = Duration::from_secs(5);

pub fn capacite(limite: i32) -> usize {
    match usize::try_from(limite) {
        Ok(n) if (1..=MAX_PARTICIPANTS).contains(&n) => n,
        _ => MAX_PARTICIPANTS,
    }
}

pub fn salle(salon: &str) -> String {
    format!("channel:{salon}")
}

pub fn salon_de(salle: &str) -> Option<&str> {
    salle.strip_prefix("channel:").filter(|s| !s.is_empty())
}

pub fn proprietaire_ecran(identite: &str) -> Option<&str> {
    identite.strip_suffix("#screen")
}

pub fn source(v: &Value) -> &'static str {
    match v {
        Value::String(s) => match s.as_str() {
            "SCREEN_SHARE" => "screen",
            "CAMERA" => "camera",
            _ => "",
        },
        Value::Number(n) => match n.as_f64().map(|f| f as i64) {
            Some(3) => "screen",
            Some(1) => "camera",
            _ => "",
        },
        _ => "",
    }
}

pub fn maintenant() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn mac(secret: &str) -> Hmac256 {
    Hmac256::new_from_slice(secret.as_bytes()).expect("clé HMAC de toute longueur")
}

pub fn signer(secret: &str, claims: &Value) -> String {
    let msg = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let mut m = mac(secret);
    m.update(msg.as_bytes());
    format!(
        "{msg}.{}",
        URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())
    )
}

pub fn verifier(secret: &str, jeton: &str, now: i64) -> Result<Value, &'static str> {
    let parts: Vec<&str> = jeton.split('.').collect();
    let [entete, charge, signature] = parts[..] else {
        return Err("jeton mal formé");
    };
    let attendue = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| "signature invalide")?;
    let mut m = mac(secret);
    m.update(format!("{entete}.{charge}").as_bytes());
    m.verify_slice(&attendue)
        .map_err(|_| "signature invalide")?;
    let brut = URL_SAFE_NO_PAD
        .decode(charge)
        .map_err(|_| "jeton mal formé")?;
    let claims: Value = serde_json::from_slice(&brut).map_err(|_| "jeton mal formé")?;
    let exp = claims["exp"].as_f64().ok_or("jeton sans expiration")?;
    if now as f64 > exp {
        return Err("jeton expiré");
    }
    Ok(claims)
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mod {
    pub mic_off: bool,
    pub sound_off: bool,
    pub no_speak: bool,
    pub no_video: bool,
    pub no_screen: bool,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Participant {
    pub user_id: String,
    pub pseudo: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub screen: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub camera: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub since: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mic_off: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sound_off: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Erreur {
    Absent,
    Complet,
    Injoignable,
}

#[derive(Debug, Default, PartialEq)]
pub struct Evenement {
    pub event: String,
    pub salle: String,
    pub identite: String,
    pub nom: String,
    pub source: Value,
}

pub struct Lk {
    cfg: config::Livekit,
    http: Client,
}

fn client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(DELAI)
                .build()
                .expect("client HTTP")
        })
        .clone()
}

impl Lk {
    pub fn new(cfg: &config::Livekit) -> Self {
        Lk {
            cfg: cfg.clone(),
            http: client(),
        }
    }

    pub fn url_publique(&self) -> &str {
        &self.cfg.url
    }

    pub fn limites(&self) -> (i64, i64) {
        let ou = |v: i64, defaut: i64| if v <= 0 { defaut } else { v };
        (
            ou(self.cfg.max_screen_fps, 60),
            ou(self.cfg.max_screen_height, 2160),
        )
    }

    fn sources(&self) -> &'static [&'static str] {
        if self.cfg.allow_video {
            &["microphone", "camera", "screen_share", "screen_share_audio"]
        } else {
            &["microphone"]
        }
    }

    fn permissions(&self, m: Mod, majuscules: bool) -> Value {
        let sources: Vec<String> = self
            .sources()
            .iter()
            .filter(|s| {
                !((**s == "microphone" && (m.mic_off || m.no_speak))
                    || (**s == "camera" && m.no_video)
                    || (s.starts_with("screen_share") && m.no_screen))
            })
            .map(|s| {
                if majuscules {
                    s.to_uppercase()
                } else {
                    s.to_string()
                }
            })
            .collect();
        json!({
            "canSubscribe": !m.sound_off,
            "canPublish": true,
            "canPublishData": false,
            "canUpdateOwnMetadata": true,
            "canPublishSources": sources,
        })
    }

    pub fn jeton(&self, salon: &str, identite: &str, pseudo: &str, m: Mod) -> String {
        self.signer_jeton(salon, identite, pseudo, self.permissions(m, false))
    }

    pub fn jeton_ecran(&self, salon: &str, user: &str, pseudo: &str, m: Mod) -> String {
        let mut video = self.permissions(m, false);
        let sources: Vec<Value> = video["canPublishSources"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s.as_str().is_some_and(|s| s.starts_with("screen_share")))
            .cloned()
            .collect();
        video["canPublishSources"] = json!(sources);
        video["canSubscribe"] = json!(false);
        video["canUpdateOwnMetadata"] = json!(false);
        self.signer_jeton(salon, &format!("{user}#screen"), pseudo, video)
    }

    fn signer_jeton(&self, salon: &str, identite: &str, pseudo: &str, mut video: Value) -> String {
        let now = maintenant();
        video["room"] = json!(salle(salon));
        video["roomJoin"] = json!(true);
        signer(
            &self.cfg.secret,
            &json!({
                "iss": self.cfg.key, "sub": identite, "name": pseudo,
                "nbf": now - 10, "exp": now + VALIDITE_JETON,
                "video": video,
            }),
        )
    }

    async fn rpc(&self, methode: &str, salle: &str, entree: Value) -> Result<Value, Erreur> {
        let now = maintenant();
        let jeton = signer(
            &self.cfg.secret,
            &json!({
                "iss": self.cfg.key, "nbf": now - 10, "exp": now + VALIDITE_API,
                "video": { "roomCreate": true, "roomList": true, "roomAdmin": true, "room": salle },
            }),
        );
        let reponse = self
            .http
            .post(format!(
                "{}/twirp/livekit.RoomService/{methode}",
                self.cfg.api_url.trim_end_matches('/')
            ))
            .bearer_auth(jeton)
            .header("content-type", "application/json")
            .body(entree.to_string())
            .send()
            .await
            .map_err(|_| Erreur::Injoignable)?;
        match reponse.status() {
            StatusCode::NOT_FOUND => Err(Erreur::Absent),
            StatusCode::OK => Ok(reponse
                .bytes()
                .await
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or(Value::Null)),
            _ => Err(Erreur::Injoignable),
        }
    }

    pub async fn definir_mod(&self, salon: &str, user: &str, m: Mod) -> Result<(), Erreur> {
        self.rpc(
            "UpdateParticipant",
            &salle(salon),
            json!({ "room": salle(salon), "identity": user, "permission": self.permissions(m, true) }),
        )
        .await
        .map(|_| ())
    }

    pub async fn retirer(&self, salon: &str, identite: &str) -> Result<(), Erreur> {
        self.rpc(
            "RemoveParticipant",
            &salle(salon),
            json!({ "room": salle(salon), "identity": identite }),
        )
        .await
        .map(|_| ())
    }

    pub async fn participants(&self, salon: &str) -> Result<Vec<Participant>, Erreur> {
        let sortie = self
            .rpc(
                "ListParticipants",
                &salle(salon),
                json!({ "room": salle(salon) }),
            )
            .await?;
        let mut ps = Vec::new();
        let mut ecrans = Vec::new();
        for p in sortie["participants"].as_array().into_iter().flatten() {
            let identite = p["identity"].as_str().unwrap_or_default();
            if let Some(proprio) = proprietaire_ecran(identite) {
                ecrans.push(proprio.to_string());
                continue;
            }
            let mut q = Participant {
                user_id: identite.to_string(),
                pseudo: p["name"].as_str().unwrap_or_default().to_string(),
                ..Default::default()
            };
            for t in p["tracks"].as_array().into_iter().flatten() {
                match source(&t["source"]) {
                    "screen" => q.screen = true,
                    "camera" => q.camera = true,
                    _ => {}
                }
            }
            ps.push(q);
        }
        for p in &mut ps {
            p.screen = p.screen || ecrans.contains(&p.user_id);
        }
        Ok(ps)
    }

    pub async fn admettre(&self, salon: &str, user: &str, limite: i32) -> Result<(), Erreur> {
        self.rpc(
            "CreateRoom",
            &salle(salon),
            json!({ "name": salle(salon), "max_participants": MAX_PARTICIPANTS + 1, "empty_timeout": 300 }),
        )
        .await?;
        let mut personnes = 0;
        for p in self.participants(salon).await? {
            if p.user_id == user {
                return Ok(());
            }
            if p.user_id != estompe_core::messages::SYSTEME {
                personnes += 1;
            }
        }
        if personnes >= capacite(limite) {
            return Err(Erreur::Complet);
        }
        Ok(())
    }

    pub fn webhook(&self, autorisation: &str, corps: &[u8]) -> Result<Evenement, &'static str> {
        let jeton = autorisation.strip_prefix("Bearer ").unwrap_or(autorisation);
        let claims = verifier(&self.cfg.secret, jeton, maintenant())?;
        let attendu = STANDARD.encode(Sha256::digest(corps));
        let recu = claims["sha256"].as_str().unwrap_or_default();
        if recu.len() != attendu.len() || !constant_eq(recu.as_bytes(), attendu.as_bytes()) {
            return Err("empreinte du corps invalide");
        }
        let v: Value = serde_json::from_slice(corps).map_err(|_| "corps illisible")?;
        let racine = objet(&v)?;
        let room = objet(champ(racine, "room"))?;
        let participant = objet(champ(racine, "participant"))?;
        let track = objet(champ(racine, "track"))?;
        Ok(Evenement {
            event: texte(champ(racine, "event"))?,
            salle: texte(champ(room, "name"))?,
            identite: texte(champ(participant, "identity"))?,
            nom: texte(champ(participant, "name"))?,
            source: champ(track, "source").clone(),
        })
    }

    #[cfg(test)]
    pub fn signer_webhook(&self, corps: &[u8]) -> String {
        signer(
            &self.cfg.secret,
            &json!({ "iss": self.cfg.key, "exp": maintenant() + 60, "sha256": STANDARD.encode(Sha256::digest(corps)) }),
        )
    }
}

static NUL: Value = Value::Null;

fn champ<'a>(o: &'a serde_json::Map<String, Value>, nom: &str) -> &'a Value {
    o.get(nom)
        .or_else(|| {
            o.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(nom))
                .map(|(_, v)| v)
        })
        .unwrap_or(&NUL)
}

fn objet(v: &Value) -> Result<&serde_json::Map<String, Value>, &'static str> {
    static VIDE: std::sync::LazyLock<serde_json::Map<String, Value>> =
        std::sync::LazyLock::new(Default::default);
    match v {
        Value::Object(o) => Ok(o),
        Value::Null => Ok(&VIDE),
        _ => Err("corps illisible"),
    }
}

fn texte(v: &Value) -> Result<String, &'static str> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Ok(String::new()),
        _ => Err("corps illisible"),
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;

    static SECRET: LazyLock<String> = LazyLock::new(cle_aleatoire);

    fn cle_aleatoire() -> String {
        use std::hash::BuildHasher;
        std::collections::hash_map::RandomState::new()
            .hash_one(std::process::id())
            .to_string()
    }

    #[test]
    fn webhook_decode_comme_le_go() {
        let lk = Lk::new(&config::Livekit {
            url: "wss://lk".into(),
            api_url: "http://127.0.0.1:1".into(),
            key: "k".into(),
            secret: SECRET.clone(),
            max_screen_fps: 0,
            max_screen_height: 0,
            allow_video: false,
        });
        let ok = |c: &str| {
            lk.webhook(&lk.signer_webhook(c.as_bytes()), c.as_bytes())
                .is_ok()
        };
        assert!(ok("null"));
        assert!(ok(r#"{"EVENT":"x","room":null}"#));
        assert!(!ok(r#"{"room":"salon"}"#));
        assert!(!ok(r#"{"participant":3}"#));
        assert!(!ok(r#"{"event":1}"#));
        assert!(!ok("[]"));
    }

    fn lk(video: bool) -> Lk {
        Lk::new(&config::Livekit {
            url: "wss://lk".into(),
            api_url: "http://127.0.0.1:1".into(),
            key: "k".into(),
            secret: SECRET.clone(),
            max_screen_fps: 0,
            max_screen_height: 0,
            allow_video: video,
        })
    }

    fn charge(jeton: &str) -> Value {
        serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(jeton.split('.').nth(1).unwrap())
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn jeton_de_connexion() {
        let j = lk(false).jeton("c1", "u1", "alice", Mod::default());
        let c = charge(&j);
        assert_eq!(
            (c["iss"].as_str(), c["sub"].as_str(), c["name"].as_str()),
            (Some("k"), Some("u1"), Some("alice"))
        );
        assert_eq!(
            c["exp"].as_i64().unwrap() - c["nbf"].as_i64().unwrap(),
            3610
        );
        assert_eq!(c["video"]["room"], "channel:c1");
        assert_eq!(c["video"]["roomJoin"], true);
        assert_eq!(c["video"]["canPublishSources"], json!(["microphone"]));
        assert_eq!(c["video"]["canSubscribe"], true);
        assert_eq!(c["video"]["canPublishData"], false);
        assert!(verifier(&SECRET, &j, maintenant()).is_ok());
        assert!(verifier(&cle_aleatoire(), &j, maintenant()).is_err());
    }

    #[test]
    fn jeton_ecran_limite_au_partage() {
        let c = charge(&lk(true).jeton_ecran("c1", "u1", "alice", Mod::default()));
        assert_eq!(c["sub"], "u1#screen");
        assert_eq!(c["video"]["room"], "channel:c1");
        assert_eq!(
            c["video"]["canPublishSources"],
            json!(["screen_share", "screen_share_audio"])
        );
        assert_eq!(
            (
                &c["video"]["canSubscribe"],
                &c["video"]["canUpdateOwnMetadata"],
                &c["video"]["canPublishData"]
            ),
            (&json!(false), &json!(false), &json!(false))
        );
        let tout = Mod {
            mic_off: true,
            sound_off: true,
            no_speak: true,
            no_video: true,
            no_screen: false,
        };
        let c = charge(&lk(true).jeton_ecran("c1", "u1", "alice", tout));
        assert_eq!(
            c["video"]["canPublishSources"],
            json!(["screen_share", "screen_share_audio"])
        );
        let m = Mod {
            no_screen: true,
            ..Default::default()
        };
        let c = charge(&lk(true).jeton_ecran("c1", "u1", "alice", m));
        assert_eq!(c["video"]["canPublishSources"], json!([]));
        let c = charge(&lk(false).jeton_ecran("c1", "u1", "alice", Mod::default()));
        assert_eq!(c["video"]["canPublishSources"], json!([]));
    }

    #[test]
    fn droits_selon_video_et_moderation() {
        let sources =
            |video: bool, m: Mod| lk(video).permissions(m, false)["canPublishSources"].clone();
        assert_eq!(
            sources(true, Mod::default()),
            json!(["microphone", "camera", "screen_share", "screen_share_audio"])
        );
        let m = Mod {
            no_video: true,
            no_screen: true,
            ..Default::default()
        };
        assert_eq!(sources(true, m), json!(["microphone"]));
        let m = Mod {
            mic_off: true,
            ..Default::default()
        };
        assert_eq!(
            sources(true, m),
            json!(["camera", "screen_share", "screen_share_audio"])
        );
        let m = Mod {
            no_speak: true,
            ..Default::default()
        };
        assert_eq!(sources(false, m), json!([]));
        let p = lk(true).permissions(
            Mod {
                sound_off: true,
                ..Default::default()
            },
            true,
        );
        assert_eq!(p["canSubscribe"], false);
        assert_eq!(p["canPublishSources"][0], "MICROPHONE");
    }

    #[test]
    fn verification_refuse_les_jetons_invalides() {
        let valide = signer(&SECRET, &json!({ "exp": 100 }));
        assert!(verifier(&SECRET, &valide, 100).is_ok());
        assert_eq!(verifier(&SECRET, &valide, 101), Err("jeton expiré"));
        assert_eq!(verifier(&SECRET, "a.b", 0), Err("jeton mal formé"));
        assert_eq!(verifier(&SECRET, "a.b.c", 0), Err("signature invalide"));
        assert_eq!(
            verifier(&SECRET, &signer(&SECRET, &json!({})), 0),
            Err("jeton sans expiration")
        );
        let mut altere = valide.clone();
        altere.push('x');
        assert!(verifier(&SECRET, &altere, 0).is_err());
    }

    #[test]
    fn webhook_signe() {
        let l = lk(false);
        let corps = br#"{"event":"track_published","room":{"name":"channel:c"},"participant":{"identity":"u","name":"a"},"track":{"source":"CAMERA"}}"#;
        let e = l.webhook(&l.signer_webhook(corps), corps).unwrap();
        assert_eq!(
            (
                e.event.as_str(),
                e.salle.as_str(),
                e.identite.as_str(),
                e.nom.as_str()
            ),
            ("track_published", "channel:c", "u", "a")
        );
        assert_eq!(source(&e.source), "camera");
        let bearer = format!("Bearer {}", l.signer_webhook(corps));
        assert!(l.webhook(&bearer, corps).is_ok());
        assert!(l.webhook(&l.signer_webhook(corps), b"{}").is_err());
        assert!(l.webhook("faux.jeton.ici", corps).is_err());
        let vide = b"[]";
        assert!(l.webhook(&l.signer_webhook(vide), vide).is_err());
    }

    #[test]
    fn utilitaires() {
        assert_eq!(
            (
                capacite(0),
                capacite(-1),
                capacite(4),
                capacite(10),
                capacite(99)
            ),
            (10, 10, 4, 10, 10)
        );
        assert_eq!(salon_de("channel:abc"), Some("abc"));
        assert_eq!((salon_de("channel:"), salon_de("autre")), (None, None));
        assert_eq!(proprietaire_ecran("u#screen"), Some("u"));
        assert_eq!(proprietaire_ecran("u"), None);
        assert_eq!(
            (
                source(&json!(3)),
                source(&json!(3.0)),
                source(&json!(1)),
                source(&json!(2)),
                source(&json!("SCREEN_SHARE")),
                source(&json!(null))
            ),
            ("screen", "screen", "camera", "", "screen", "")
        );
        assert_eq!(lk(false).limites(), (60, 2160));
    }
}
