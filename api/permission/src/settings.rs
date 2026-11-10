use commun::env::Env;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server: Server,
    pub database: Database,
    pub identity: Identity,
    #[serde(default)]
    pub services: Services,
    #[serde(default)]
    pub gateway: Gateway,
}

/// Route interne `/internal/users` d'auth-gateway (recherche de comptes) : désactivée tant que `internal_token` est vide.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Gateway {
    /// Ex. `http://gateway:8080` (réseau Docker).
    #[serde(default)]
    pub url: String,
    /// `GATEWAY_INTERNE_TOKEN`, commun avec le gateway.
    #[serde(default)]
    pub internal_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub bind: String,
    #[serde(default)]
    pub static_dir: String,
    pub admin_role: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    pub url: String,
}

/// Secret HMAC partagé avec le gateway (`GATEWAY_IDENTITE_SECRET`).
#[derive(Debug, Clone, Deserialize)]
pub struct Identity {
    pub secret: String,
}

/// Micro-services autorisés à interroger les permissions de n'importe quel compte.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Services {
    /// `nom:token,nom2:token2`
    #[serde(default)]
    pub tokens: String,
}

impl Services {
    /// Nom du service dont le token est `given`.
    pub fn find(&self, given: &str) -> Option<&str> {
        commun::services::trouver(&self.tokens, given)
    }
}

impl Settings {
    /// `config/default.toml`, puis les variables `PERMISSION__SECTION__CLE`.
    pub fn load() -> anyhow::Result<Self> {
        Self::depuis(&Env::charger())
    }

    pub fn depuis(env: &Env) -> anyhow::Result<Self> {
        let get = |k: &str| env.get(k);
        let present = |k: &str| env.get(k).filter(|v| !v.is_empty());
        let jetons = env.jetons("PERMISSION_TOKEN_");
        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(
                config::Environment::with_prefix("PERMISSION")
                    .separator("__")
                    .source(Some(env.vars().clone())),
            )
            .set_override(
                "database.url",
                commun::env::url_base(get, "permission").map_err(anyhow::Error::msg)?,
            )?
            .set_override_option("identity.secret", present("GATEWAY_IDENTITE_SECRET"))?
            .set_override_option("gateway.url", present("GATEWAY_INTERNE_URL"))?
            .set_override_option("gateway.internal_token", present("GATEWAY_INTERNE_TOKEN"))?
            .set_override_option("server.admin_role", present("PERMISSION_ADMIN_ROLE"))?
            .set_override_option("services.tokens", Some(jetons).filter(|v| !v.is_empty()))?
            .build()?;
        Ok(cfg.try_deserialize()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_lookup() {
        let s = Services {
            tokens: "chat:abc, radar:def,casse,vide:".into(),
        };
        assert_eq!(s.find("abc"), Some("chat"));
        assert_eq!(s.find("def"), Some("radar"));
        assert_eq!(s.find("nope"), None);
        assert_eq!(s.find(""), None);
    }

    #[test]
    fn variables_communes() {
        let s = Settings::depuis(&Env::depuis([
            ("DATABASE_URL", "postgres://postgres-main:5432"),
            ("DATABASE_USER", "plbls"),
            ("DATABASE_PASSWORD", "mdp"),
            ("GATEWAY_IDENTITE_SECRET", "id"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080"),
            ("GATEWAY_INTERNE_TOKEN", "int"),
            ("PERMISSION_TOKEN_ESTOMPE", "a"),
            ("PERMISSION_ADMIN_ROLE", "admin"),
        ]))
        .unwrap();
        assert_eq!(
            s.database.url,
            "postgres://plbls:mdp@postgres-main:5432/permission"
        );
        assert_eq!(s.identity.secret, "id");
        assert_eq!(s.gateway.url, "http://auth-gateway:8080");
        assert_eq!(s.gateway.internal_token, "int");
        assert_eq!(s.services.find("a"), Some("estompe"));
        assert_eq!(s.server.admin_role, "admin");
    }
}
