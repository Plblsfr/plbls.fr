use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::Identity;

const LOGIN_TTL: u64 = 600;
/// Plus long que le timeout des appels à Keycloak (10 s) : le verrou ne peut pas expirer pendant un refresh.
const LOCK_TTL: u64 = 15;

/// État d'un login en cours (clé `login:{state}`, TTL 10 min).
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingLogin {
    pub nonce: String,
    pub verifier: String,
    pub redirect: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub identity: Identity,
    pub access_token: String,
    pub access_exp: u64,
    pub refresh_token: String,
    pub id_token: Option<String>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `n` octets aléatoires en base64url.
pub fn random_token(n: usize) -> String {
    let mut buf = vec![0u8; n];
    rand::rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Redis ne stocke que le SHA-256 de l'identifiant de session.
pub fn session_key(sid: &str) -> String {
    let h = Sha256::digest(sid.as_bytes());
    format!(
        "sess:{}",
        h.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

#[derive(Clone)]
pub struct Store {
    redis: redis::aio::ConnectionManager,
}

impl Store {
    pub fn new(redis: redis::aio::ConnectionManager) -> Self {
        Self { redis }
    }

    pub async fn ping(&self) -> redis::RedisResult<String> {
        let mut c = self.redis.clone();
        redis::cmd("PING").query_async(&mut c).await
    }

    pub async fn put_login(&self, state: &str, login: &PendingLogin) -> redis::RedisResult<()> {
        let mut c = self.redis.clone();
        c.set_ex(
            format!("login:{state}"),
            serde_json::to_string(login).unwrap(),
            LOGIN_TTL,
        )
        .await
    }

    /// Lecture unique : le `state` ne sert qu'une fois.
    pub async fn take_login(&self, state: &str) -> redis::RedisResult<Option<PendingLogin>> {
        let mut c = self.redis.clone();
        let raw: Option<String> = redis::cmd("GETDEL")
            .arg(format!("login:{state}"))
            .query_async(&mut c)
            .await?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
    }

    pub async fn put_session(&self, sid: &str, s: &Session, ttl: u64) -> redis::RedisResult<()> {
        let mut c = self.redis.clone();
        c.set_ex(
            session_key(sid),
            serde_json::to_string(s).unwrap(),
            ttl.max(1),
        )
        .await
    }

    /// Réécrit la session en gardant son TTL restant (ou `ttl` si fourni).
    pub async fn update_session(
        &self,
        sid: &str,
        s: &Session,
        ttl: Option<u64>,
    ) -> redis::RedisResult<()> {
        let mut c = self.redis.clone();
        let key = session_key(sid);
        let payload = serde_json::to_string(s).unwrap();
        match ttl {
            Some(t) => c.set_ex(key, payload, t.max(1)).await,
            None => {
                redis::cmd("SET")
                    .arg(key)
                    .arg(payload)
                    .arg("KEEPTTL")
                    .query_async(&mut c)
                    .await
            }
        }
    }

    pub async fn get_session(&self, sid: &str) -> redis::RedisResult<Option<Session>> {
        let mut c = self.redis.clone();
        let raw: Option<String> = c.get(session_key(sid)).await?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
    }

    pub async fn delete_session(&self, sid: &str) -> redis::RedisResult<()> {
        let mut c = self.redis.clone();
        c.del(session_key(sid)).await
    }

    /// Compteur à fenêtre fixe : vrai tant que `key` a fait au plus `max` appels dans les `window` dernières secondes.
    pub async fn hit(&self, key: &str, max: u64, window: u64) -> redis::RedisResult<bool> {
        let mut c = self.redis.clone();
        let key = format!("rl:{key}");
        let n: u64 = c.incr(&key, 1).await?;
        if n == 1 {
            let _: () = c.expire(&key, window as i64).await?;
        }
        Ok(n <= max)
    }

    /// Verrou de refresh par session (`SET NX EX 15`).
    pub async fn try_lock(&self, sid: &str) -> redis::RedisResult<bool> {
        let mut c = self.redis.clone();
        let r: Option<String> = redis::cmd("SET")
            .arg(format!("lock:{}", session_key(sid)))
            .arg(1)
            .arg("NX")
            .arg("EX")
            .arg(LOCK_TTL)
            .query_async(&mut c)
            .await?;
        Ok(r.is_some())
    }

    pub async fn unlock(&self, sid: &str) {
        let mut c = self.redis.clone();
        let _: redis::RedisResult<()> = c.del(format!("lock:{}", session_key(sid))).await;
    }
}
