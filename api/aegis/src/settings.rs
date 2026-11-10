use commun::env::Env;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server: Server,
    pub database: Database,
    pub identity: Identity,
    #[serde(default)]
    pub redis: Redis,
    #[serde(default)]
    pub cache: Cache,
    #[serde(default)]
    pub services: Services,
    #[serde(default)]
    pub gateway: Gateway,
    #[serde(default)]
    pub annuaire: Annuaire,
    #[serde(default)]
    pub ressources: Ressources,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub bind: String,
    pub admin_role: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Identity {
    pub secret: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Redis {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cache {
    #[serde(default = "ttl_defaut")]
    pub ttl_secs: u64,
}

impl Default for Cache {
    fn default() -> Self {
        Cache {
            ttl_secs: ttl_defaut(),
        }
    }
}

fn ttl_defaut() -> u64 {
    600
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Gateway {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub internal_token: String,
}

impl Gateway {
    pub fn configure(&self) -> bool {
        !self.url.is_empty() && !self.internal_token.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Annuaire {
    #[serde(default = "sync_defaut")]
    pub sync_minutes: u64,
}

impl Default for Annuaire {
    fn default() -> Self {
        Annuaire {
            sync_minutes: sync_defaut(),
        }
    }
}

fn sync_defaut() -> u64 {
    15
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Ressources {
    #[serde(default)]
    pub allowed_hosts: String,
}

impl Ressources {
    pub fn hote_autorise(&self, url: &str) -> bool {
        let Ok(u) = url::Url::parse(url) else {
            return false;
        };
        if !matches!(u.scheme(), "http" | "https") {
            return false;
        }
        let hotes: Vec<&str> = self
            .allowed_hosts
            .split(',')
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .collect();
        u.host_str().is_some_and(|h| hotes.contains(&h))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Services {
    #[serde(default)]
    pub tokens: String,
}

impl Services {
    pub fn trouver(&self, donne: &str) -> Option<&str> {
        commun::services::trouver(&self.tokens, donne)
    }

    pub fn jeton(&self, app: &str) -> Option<&str> {
        self.tokens
            .split(',')
            .filter_map(|e| e.trim().split_once(':'))
            .find(|(n, t)| *n == app && !t.is_empty())
            .map(|(_, t)| t)
    }
}

impl Settings {
    pub fn load() -> anyhow::Result<Self> {
        Self::depuis(&Env::charger())
    }

    pub fn depuis(env: &Env) -> anyhow::Result<Self> {
        let get = |k: &str| env.get(k);
        let present = |k: &str| env.get(k).filter(|v| !v.is_empty());
        let redis = commun::env::url_redis(get).map_err(anyhow::Error::msg)?;
        let jetons = env.jetons("AEGIS_TOKEN_");
        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(
                config::Environment::with_prefix("AEGIS")
                    .separator("__")
                    .source(Some(env.vars().clone())),
            )
            .set_override(
                "database.url",
                commun::env::url_base(get, "aegis").map_err(anyhow::Error::msg)?,
            )?
            .set_override_option("identity.secret", present("GATEWAY_IDENTITE_SECRET"))?
            .set_override_option("redis.url", Some(redis).filter(|v| !v.is_empty()))?
            .set_override_option("redis.prefix", present("AEGIS_REDIS_PREFIX"))?
            .set_override_option("gateway.url", present("GATEWAY_INTERNE_URL"))?
            .set_override_option("gateway.internal_token", present("GATEWAY_INTERNE_TOKEN"))?
            .set_override_option("server.admin_role", present("AEGIS_ADMIN_ROLE"))?
            .set_override_option("services.tokens", Some(jetons).filter(|v| !v.is_empty()))?
            .build()?;
        Ok(cfg.try_deserialize()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jetons_de_service() {
        let s = Services {
            tokens: "estompe:abc, pli:def,casse,vide:".into(),
        };
        assert_eq!(s.trouver("abc"), Some("estompe"));
        assert_eq!(s.trouver("def"), Some("pli"));
        assert_eq!(s.trouver("non"), None);
        assert_eq!(s.trouver(""), None);
        assert_eq!(s.jeton("pli"), Some("def"));
        assert_eq!(s.jeton("vide"), None);
    }

    #[test]
    fn hotes_des_ressources() {
        let vide = Ressources::default();
        assert!(!vide.hote_autorise("http://estompe-api:8080/x"));
        assert!(!vide.hote_autorise("file:///etc/passwd"));
        let r = Ressources {
            allowed_hosts: "estompe-api, pli".into(),
        };
        assert!(r.hote_autorise("http://estompe-api:8080/x"));
        assert!(!r.hote_autorise("http://ailleurs/x"));
        assert!(!r.hote_autorise("file://estompe-api/etc/passwd"));
        assert!(!r.hote_autorise("http://estompe-api.ailleurs/x"));
    }

    #[test]
    fn variables_communes() {
        let s = Settings::depuis(&Env::depuis([
            ("DATABASE_URL", "postgres://postgres-main:5432"),
            ("DATABASE_USER", "plbls"),
            ("DATABASE_PASSWORD", "mdp"),
            ("REDIS_URL", "redis://redis:6379"),
            ("REDIS_PASSWORD", "r"),
            ("GATEWAY_IDENTITE_SECRET", "id"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080"),
            ("GATEWAY_INTERNE_TOKEN", "int"),
            ("AEGIS_TOKEN_ESTOMPE", "a"),
            ("AEGIS_TOKEN_PLI", "b"),
            ("AEGIS__RESSOURCES__ALLOWED_HOSTS", "estompe-api"),
        ]))
        .unwrap();
        assert_eq!(
            s.database.url,
            "postgres://plbls:mdp@postgres-main:5432/aegis"
        );
        assert_eq!(s.redis.url, "redis://:r@redis:6379");
        assert_eq!(s.identity.secret, "id");
        assert_eq!(s.gateway.url, "http://auth-gateway:8080");
        assert_eq!(s.gateway.internal_token, "int");
        assert_eq!(s.services.tokens, "estompe:a,pli:b");
        assert_eq!(s.ressources.allowed_hosts, "estompe-api");
        assert_eq!(s.server.admin_role, "aegis-admin");
    }
}
