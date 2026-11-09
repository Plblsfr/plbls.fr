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
    /// `GATEWAY__INTERNAL__TOKEN` côté gateway.
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

/// Secret HMAC partagé avec le gateway (`GATEWAY__IDENTITY__SECRET`).
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
        use subtle::ConstantTimeEq;
        self.tokens
            .split(',')
            .filter_map(|e| e.trim().split_once(':'))
            .filter(|(_, t)| !t.is_empty())
            .find(|(_, t)| bool::from(t.as_bytes().ct_eq(given.as_bytes())))
            .map(|(n, _)| n)
    }
}

impl Settings {
    /// `config/default.toml`, puis les variables `PERMISSION__SECTION__CLE`.
    pub fn load() -> anyhow::Result<Self> {
        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(config::Environment::with_prefix("PERMISSION").separator("__"))
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
}
