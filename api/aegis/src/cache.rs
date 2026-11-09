use redis::{AsyncCommands, aio::ConnectionManager};
use serde_json::json;

use crate::moteur::Effectifs;

pub const PREFIXE: &str = "aegis";

#[derive(Clone)]
pub struct Cache {
    redis: Option<ConnectionManager>,
    ttl: u64,
    prefixe: String,
}

pub fn canal(prefixe: &str) -> String {
    format!("{prefixe}:invalidation")
}

impl Cache {
    pub async fn connecter(url: &str, prefixe: &str, ttl: u64) -> Self {
        let redis = if url.is_empty() {
            None
        } else {
            match redis::Client::open(url) {
                Ok(c) => match c.get_connection_manager().await {
                    Ok(m) => Some(m),
                    Err(e) => {
                        tracing::warn!("Redis injoignable, cache désactivé : {e}");
                        None
                    }
                },
                Err(e) => {
                    tracing::warn!("URL Redis invalide, cache désactivé : {e}");
                    None
                }
            }
        };
        Cache {
            redis,
            ttl,
            prefixe: if prefixe.is_empty() { PREFIXE } else { prefixe }.to_string(),
        }
    }

    fn cle(&self, generation: i64, sub: &str) -> String {
        format!("{}:eff:{generation}:{sub}", self.prefixe)
    }

    fn generation_cle(&self) -> String {
        format!("{}:generation", self.prefixe)
    }

    pub fn canal(&self) -> String {
        canal(&self.prefixe)
    }

    pub fn actif(&self) -> bool {
        self.redis.is_some()
    }

    pub fn ttl(&self) -> chrono::Duration {
        chrono::Duration::seconds(self.ttl as i64)
    }

    pub async fn ping(&self) -> Option<bool> {
        let mut c = self.redis.clone()?;
        Some(
            redis::cmd("PING")
                .query_async::<String>(&mut c)
                .await
                .is_ok(),
        )
    }

    pub async fn generation(&self) -> i64 {
        let Some(mut c) = self.redis.clone() else {
            return 0;
        };
        c.get::<_, Option<i64>>(self.generation_cle())
            .await
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    pub async fn lire(&self, generation: i64, sub: &str) -> Option<Effectifs> {
        let mut c = self.redis.clone()?;
        let brut: Option<String> = c.get(self.cle(generation, sub)).await.ok()?;
        let e: Effectifs = serde_json::from_str(&brut?).ok()?;
        (e.valid_until > chrono::Utc::now()).then_some(e)
    }

    pub async fn ecrire(&self, e: &Effectifs) {
        let Some(mut c) = self.redis.clone() else {
            return;
        };
        let reste = (e.valid_until - chrono::Utc::now()).num_seconds();
        let ttl = reste.clamp(1, self.ttl as i64) as u64;
        if let Ok(brut) = serde_json::to_string(e) {
            let r: redis::RedisResult<()> =
                c.set_ex(self.cle(e.generation, &e.sub), brut, ttl).await;
            if let Err(err) = r {
                tracing::warn!("écriture du cache : {err}");
            }
        }
    }

    pub async fn invalider_compte(&self, sub: &str) {
        let Some(mut c) = self.redis.clone() else {
            return;
        };
        let generation = self.generation().await;
        let _: redis::RedisResult<()> = c.del(self.cle(generation, sub)).await;
        let message = json!({"type": "compte", "sub": sub}).to_string();
        let _: redis::RedisResult<()> = c.publish(self.canal(), message).await;
    }

    pub async fn invalider_tout(&self) {
        let Some(mut c) = self.redis.clone() else {
            return;
        };
        let generation: i64 = c.incr(self.generation_cle(), 1).await.unwrap_or(0);
        let message = json!({"type": "tout", "generation": generation}).to_string();
        let _: redis::RedisResult<()> = c.publish(self.canal(), message).await;
    }
}

pub enum Portee<'a> {
    Compte(&'a str),
    Tout,
}

impl Cache {
    pub async fn invalider(&self, p: Portee<'_>) {
        match p {
            Portee::Compte(s) => self.invalider_compte(s).await,
            Portee::Tout => self.invalider_tout().await,
        }
    }
}
