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
        let n: u64 = redis::Script::new(COMPTER)
            .key(format!("rl:{key}"))
            .arg(window)
            .invoke_async(&mut c)
            .await?;
        Ok(n <= max)
    }

    /// Verrou de refresh par session (`SET NX EX 15`), avec une valeur propre au détenteur.
    pub async fn try_lock(&self, sid: &str) -> redis::RedisResult<Option<String>> {
        let mut c = self.redis.clone();
        let jeton = random_token(16);
        let r: Option<String> = redis::cmd("SET")
            .arg(format!("lock:{}", session_key(sid)))
            .arg(&jeton)
            .arg("NX")
            .arg("EX")
            .arg(LOCK_TTL)
            .query_async(&mut c)
            .await?;
        Ok(r.map(|_| jeton))
    }

    pub async fn unlock(&self, sid: &str, jeton: &str) {
        let mut c = self.redis.clone();
        let _: redis::RedisResult<i64> = redis::Script::new(LIBERER)
            .key(format!("lock:{}", session_key(sid)))
            .arg(jeton)
            .invoke_async(&mut c)
            .await;
    }
}

const COMPTER: &str = "local n = redis.call('INCR', KEYS[1])
if n == 1 or redis.call('TTL', KEYS[1]) < 0 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
return n";

const LIBERER: &str =
    "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('DEL', KEYS[1]) end
return 0";

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> Option<Store> {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("ignoré : TEST_REDIS_URL absent");
            return None;
        };
        let client = redis::Client::open(url).expect("URL Redis");
        Some(Store::new(
            redis::aio::ConnectionManager::new(client)
                .await
                .expect("connexion Redis"),
        ))
    }

    #[tokio::test]
    async fn lock_released_only_by_its_holder() {
        let Some(s) = store().await else { return };
        let sid = random_token(16);
        let mien = s.try_lock(&sid).await.unwrap().expect("verrou pris");
        assert!(s.try_lock(&sid).await.unwrap().is_none());
        s.unlock(&sid, "autre").await;
        assert!(s.try_lock(&sid).await.unwrap().is_none());
        s.unlock(&sid, &mien).await;
        let suivant = s.try_lock(&sid).await.unwrap().expect("verrou libéré");
        assert_ne!(suivant, mien);
        s.unlock(&sid, &suivant).await;
    }

    #[tokio::test]
    async fn rate_counter_always_expires() {
        let Some(s) = store().await else { return };
        let key = format!("test:{}", random_token(8));
        let mut c = s.redis.clone();
        let _: () = c.set(format!("rl:{key}"), 5).await.unwrap();
        assert!(!s.hit(&key, 5, 30).await.unwrap());
        let ttl: i64 = c.ttl(format!("rl:{key}")).await.unwrap();
        assert!((1..=30).contains(&ttl), "{ttl}");
        let autre = format!("test:{}", random_token(8));
        assert!(s.hit(&autre, 1, 30).await.unwrap());
        assert!(!s.hit(&autre, 1, 30).await.unwrap());
        let ttl: i64 = c.ttl(format!("rl:{autre}")).await.unwrap();
        assert!((1..=30).contains(&ttl), "{ttl}");
        let _: () = c
            .del(&[format!("rl:{key}"), format!("rl:{autre}")])
            .await
            .unwrap();
    }
}
