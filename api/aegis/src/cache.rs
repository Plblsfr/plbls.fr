use redis::{AsyncCommands, aio::ConnectionManager};
use serde_json::json;

use crate::moteur::Effectifs;

pub const PREFIXE: &str = "aegis";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Version {
    pub generation: i64,
    pub compte: i64,
}

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

    fn cle(&self, v: Version, sub: &str) -> String {
        format!("{}:eff:{}:{}:{sub}", self.prefixe, v.generation, v.compte)
    }

    fn version_cle(&self, sub: &str) -> String {
        format!("{}:ver:{sub}", self.prefixe)
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

    pub async fn version(&self, sub: &str) -> Option<Version> {
        let mut c = self.redis.clone()?;
        match redis::cmd("MGET")
            .arg(self.generation_cle())
            .arg(self.version_cle(sub))
            .query_async::<(Option<i64>, Option<i64>)>(&mut c)
            .await
        {
            Ok((g, v)) => Some(Version {
                generation: g.unwrap_or(0),
                compte: v.unwrap_or(0),
            }),
            Err(e) => {
                tracing::warn!("lecture de la génération du cache, cache ignoré : {e}");
                None
            }
        }
    }

    pub async fn lire(&self, v: Version, sub: &str) -> Option<Effectifs> {
        let mut c = self.redis.clone()?;
        let brut: Option<String> = c.get(self.cle(v, sub)).await.ok()?;
        let e: Effectifs = serde_json::from_str(&brut?).ok()?;
        (e.valid_until > chrono::Utc::now()).then_some(e)
    }

    pub async fn ecrire(&self, v: Version, e: &Effectifs) {
        let Some(mut c) = self.redis.clone() else {
            return;
        };
        let reste = (e.valid_until - chrono::Utc::now()).num_seconds();
        let ttl = reste.clamp(1, self.ttl as i64) as u64;
        if let Ok(brut) = serde_json::to_string(e) {
            let r: redis::RedisResult<()> = c.set_ex(self.cle(v, &e.sub), brut, ttl).await;
            if let Err(err) = r {
                tracing::warn!("écriture du cache : {err}");
            }
        }
    }

    async fn publier(&self, c: &mut ConnectionManager, message: serde_json::Value) {
        let r: redis::RedisResult<()> = c.publish(self.canal(), message.to_string()).await;
        if let Err(e) = r {
            tracing::warn!("publication de l'invalidation {message} : {e}");
        }
    }

    pub async fn invalider_compte(&self, sub: &str) {
        let Some(mut c) = self.redis.clone() else {
            return;
        };
        let r: redis::RedisResult<i64> = c.incr(self.version_cle(sub), 1).await;
        if let Err(e) = r {
            tracing::warn!("invalidation du cache du compte {sub} : {e}");
        }
        self.publier(&mut c, json!({"type": "compte", "sub": sub}))
            .await;
    }

    pub async fn invalider_tout(&self) {
        let Some(mut c) = self.redis.clone() else {
            return;
        };
        match c.incr::<_, _, i64>(self.generation_cle(), 1).await {
            Ok(generation) => {
                self.publier(&mut c, json!({"type": "tout", "generation": generation}))
                    .await
            }
            Err(e) => tracing::warn!("invalidation de tout le cache : {e}"),
        }
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
