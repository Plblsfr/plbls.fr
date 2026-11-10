use commun::env::Env;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server: Server,
    pub redis: Redis,
    pub oidc: Oidc,
    pub session: Session,
    pub identity: Identity,
    #[serde(default)]
    pub internal: Internal,
    #[serde(default)]
    pub rate_limit: RateLimit,
    #[serde(default)]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub bind: String,
    pub public_url: String,
    pub default_redirect: String,
    pub allowed_origin_suffix: String,
    /// Origines supplémentaires autorisées (extensions, dev local).
    #[serde(default)]
    pub extra_origins: Vec<String>,
    /// Durée de vie maximale d'un WebSocket relayé : à l'échéance le client se reconnecte, donc se réauthentifie.
    #[serde(default = "default_ws_lifetime")]
    pub ws_max_lifetime_secs: u64,
}

fn default_ws_lifetime() -> u64 {
    900
}

#[derive(Debug, Clone, Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Oidc {
    /// URL publique du realm : `iss` des tokens et redirections du navigateur.
    pub issuer: String,
    /// URL du même realm joignable depuis le gateway (ex. `http://keycloak:8080/realms/plbls`) pour les appels
    /// serveur à serveur. Vide = on passe par `issuer`.
    #[serde(default)]
    pub internal_issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// Audience exigée des access tokens, et nom du scope Keycloak qui autorise un client à appeler l'API.
    pub audience: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    pub cookie_name: String,
    pub cookie_domain: String,
    #[serde(default = "default_true")]
    pub cookie_secure: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct Identity {
    pub secret: String,
}

/// Routes `/internal/*` (réseau interne uniquement) : désactivées (404) tant que `token` est vide.
#[derive(Debug, Clone, Deserialize)]
pub struct Internal {
    #[serde(default)]
    pub token: String,
    /// Plafond de `max` sur `/internal/users` (`GATEWAY__INTERNAL__MAX_PAGE_SIZE`).
    #[serde(default = "default_max_page_size")]
    pub max_page_size: u32,
    #[serde(default)]
    pub list_all: bool,
}

impl Default for Internal {
    fn default() -> Self {
        Self {
            token: String::new(),
            max_page_size: default_max_page_size(),
            list_all: false,
        }
    }
}

fn default_max_page_size() -> u32 {
    100
}

/// Limite de débit par IP cliente sur `/auth/login`, `/auth/register` et `/auth/callback` (0 = désactivée).
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimit {
    #[serde(default = "default_auth_per_minute")]
    pub auth_per_minute: u64,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            auth_per_minute: default_auth_per_minute(),
        }
    }
}

fn default_auth_per_minute() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    pub prefix: String,
    pub upstream: String,
    #[serde(default)]
    pub strip_prefix: bool,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub forward_token: bool,
}

impl Settings {
    /// `config/default.toml`, puis le fichier optionnel `GATEWAY_CONFIG` (routes, dev),
    /// puis les variables `GATEWAY__SECTION__CLE`.
    pub fn load() -> anyhow::Result<Self> {
        Self::depuis(&Env::charger())
    }

    pub fn depuis(env: &Env) -> anyhow::Result<Self> {
        let present = |k: &str| env.get(k).filter(|v| !v.is_empty());
        let mut builder =
            config::Config::builder().add_source(config::File::with_name("config/default"));
        if let Some(extra) = present("GATEWAY_CONFIG") {
            builder = builder.add_source(config::File::with_name(&extra));
        }
        let redis = commun::env::url_redis(|k| env.get(k)).map_err(anyhow::Error::msg)?;
        let cfg = builder
            .add_source(
                config::Environment::with_prefix("GATEWAY")
                    .separator("__")
                    .source(Some(env.vars().clone())),
            )
            .set_override_option("redis.url", Some(redis).filter(|v| !v.is_empty()))?
            .set_override_option("identity.secret", present("GATEWAY_IDENTITE_SECRET"))?
            .set_override_option("internal.token", present("GATEWAY_INTERNE_TOKEN"))?
            .build()?;
        Ok(cfg.try_deserialize()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_communes() {
        let s = Settings::depuis(&Env::depuis([
            ("REDIS_URL", "redis://redis:6379"),
            ("REDIS_PASSWORD", "r"),
            ("GATEWAY_IDENTITE_SECRET", "id"),
            ("GATEWAY_INTERNE_TOKEN", "int"),
            ("GATEWAY__OIDC__ISSUER", "https://auth/realms/x"),
            ("GATEWAY__OIDC__CLIENT_SECRET", "cs"),
        ]))
        .unwrap();
        assert_eq!(s.redis.url, "redis://:r@redis:6379");
        assert_eq!(s.identity.secret, "id");
        assert_eq!(s.internal.token, "int");
        assert_eq!(s.oidc.issuer, "https://auth/realms/x");
        assert!(Settings::depuis(&Env::depuis([("GATEWAY_IDENTITE_SECRET", "id")])).is_err());
    }
}
