use commun::config::nettoie;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub gateway_url: String,
    pub aegis: Option<(String, String)>,
    pub origines: Vec<String>,
}

fn nombre<T: std::str::FromStr>(nom: &str, valeur: Option<String>, defaut: T) -> Result<T, String> {
    match valeur {
        None => Ok(defaut),
        Some(v) => v.parse().map_err(|_| format!("{nom} invalide : {v}")),
    }
}

fn sans_slash(url: String) -> String {
    url.trim_end_matches('/').to_string()
}

impl Config {
    pub fn from_env(brut: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let env = |k: &str| brut(k).map(|v| nettoie(&v)).filter(|v| !v.is_empty());
        let database_url = commun::env::url_base(&brut, "tirelire")?;
        let gateway_public =
            env("GATEWAY_URL").unwrap_or_else(|| "https://security.plbls.fr".into());
        Ok(Self {
            port: nombre("TIRELIRE_PORT", env("TIRELIRE_PORT"), 8080)?,
            database_url,
            gateway_url: sans_slash(env("GATEWAY_INTERNE_URL").unwrap_or(gateway_public)),
            aegis: env("AEGIS_URL")
                .map(sans_slash)
                .zip(env("AEGIS_TOKEN_TIRELIRE")),
            origines: env("TIRELIRE_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|o| o.trim().trim_end_matches('/').to_string())
                .filter(|o| !o.is_empty())
                .collect(),
        })
    }

    pub fn pour_tests(database_url: &str) -> Self {
        let mut c =
            Self::from_env(|k| (k == "DATABASE_URL").then(|| "postgres://test".to_string()))
                .expect("configuration de test");
        c.database_url = database_url.to_string();
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charge(paires: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_env(|k| {
            paires
                .iter()
                .find(|(c, _)| *c == k)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn valeurs_par_defaut() {
        assert!(charge(&[]).unwrap_err().contains("DATABASE_URL"));
        let c = charge(&[("DATABASE_URL", "postgres://x")]).unwrap();
        assert_eq!(c.port, 8080);
        assert_eq!(c.database_url, "postgres://x/tirelire");
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.aegis.is_none());
        assert!(c.origines.is_empty());
    }

    #[test]
    fn valeurs_lues() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://postgres-main:5432"),
            ("DATABASE_USER", "plbls"),
            ("GATEWAY_URL", "https://g/"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080/"),
            ("AEGIS_URL", "http://aegis:8080/"),
            ("AEGIS_TOKEN_TIRELIRE", "\"tok\""),
            ("AEGIS_TOKEN_SILLON", "autre"),
            ("TIRELIRE_PORT", "9000"),
            (
                "TIRELIRE_ALLOWED_ORIGINS",
                "https://a.fr/, https://tauri.localhost,",
            ),
        ])
        .unwrap();
        assert_eq!(c.port, 9000);
        assert_eq!(
            c.database_url,
            "postgres://plbls@postgres-main:5432/tirelire"
        );
        assert_eq!(c.gateway_url, "http://auth-gateway:8080");
        assert_eq!(c.aegis, Some(("http://aegis:8080".into(), "tok".into())));
        assert_eq!(c.origines, vec!["https://a.fr", "https://tauri.localhost"]);
    }

    #[test]
    fn aegis_sans_jeton_ignore() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://x"),
            ("AEGIS_URL", "http://aegis:8080"),
        ])
        .unwrap();
        assert!(c.aegis.is_none());
    }

    #[test]
    fn erreurs() {
        let base = ("DATABASE_URL", "postgres://x");
        assert!(
            charge(&[base, ("TIRELIRE_PORT", "x")])
                .unwrap_err()
                .contains("TIRELIRE_PORT")
        );
        assert!(charge(&[("DATABASE_URL", "postgres-main")]).is_err());
    }
}
