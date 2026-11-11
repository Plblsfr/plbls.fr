use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Livekit {
    pub url: String,
    pub api_url: String,
    pub key: String,
    pub secret: String,
    pub max_screen_fps: i64,
    pub max_screen_height: i64,
    pub allow_video: bool,
}

#[derive(Clone, PartialEq, Eq, Default)]
pub struct Aegis {
    pub url: String,
    pub token: String,
    pub redis_url: String,
    pub redis_prefix: String,
    pub admin_role: String,
    pub resources_url: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub gateway_url: String,
    pub allowed_origins: Vec<String>,
    pub aegis: Aegis,
    pub data_dir: String,
    pub max_storage_bytes: i64,
    pub max_user_storage_bytes: i64,
    pub max_video_bytes: i64,
    pub metrics_token: String,
    pub giphy_key: String,
    pub youtube_key: String,
    pub vapid_public: String,
    pub vapid_private: String,
    pub vapid_subject: String,
    pub fcm_service_account: String,
    pub transcribe_url: String,
    pub transcribe_model: String,
    pub transcribe_alpha: bool,
    pub transcribe_language: String,
    pub entracte_url: String,
    pub entracte_token: String,
    pub livekit: Livekit,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Debug for Livekit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Livekit").finish_non_exhaustive()
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("listen_addr", &self.listen_addr)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn load(getenv: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |k: &str| getenv(k).unwrap_or_default();
        let env = |k: &str, def: &str| {
            getenv(k)
                .filter(|v| !v.is_empty())
                .unwrap_or(def.to_string())
        };
        let entier = |k: &str| get(k).parse::<i64>().unwrap_or(0);
        let octets = |k: &str, def: &str, facteur: i64| {
            env(k, def)
                .parse::<i64>()
                .ok()
                .filter(|n| *n >= 1)
                .and_then(|n| n.checked_mul(facteur))
                .ok_or_else(|| ConfigError(format!("{k} doit être un entier >= 1")))
        };
        let max_storage_bytes = octets("MAX_STORAGE_GB", "20", 1 << 30)?;
        let max_user_storage_bytes = octets("MAX_USER_STORAGE_MB", "2048", 1 << 20)?;
        let max_video_bytes = octets("MAX_VIDEO_SIZE_MB", "100", 1 << 20)?;
        let database_url = if get("DATABASE_URL").is_empty() {
            String::new()
        } else {
            commun::env::url_base(&getenv, "estompe").map_err(ConfigError)?
        };
        let redis_url = commun::env::url_redis(&getenv).map_err(ConfigError)?;
        let c = Config {
            listen_addr: env("LISTEN_ADDR", ":8080"),
            database_url,
            gateway_url: match get("GATEWAY_INTERNE_URL") {
                interne if interne.is_empty() => env("GATEWAY_URL", "https://security.plbls.fr"),
                interne => interne,
            },
            allowed_origins: get("ALLOWED_ORIGINS")
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect(),
            aegis: Aegis {
                url: env("AEGIS_URL", "http://aegis:8080")
                    .trim_end_matches('/')
                    .to_string(),
                token: get("AEGIS_TOKEN_ESTOMPE"),
                redis_url,
                redis_prefix: env("AEGIS_REDIS_PREFIX", "aegis"),
                admin_role: env("AEGIS_ADMIN_ROLE", "aegis-admin"),
                resources_url: env("AEGIS_RESOURCES_URL", "http://estompe-api:8080")
                    .trim_end_matches('/')
                    .to_string(),
            },
            data_dir: env("DATA_DIR", "/data"),
            max_storage_bytes,
            max_user_storage_bytes,
            max_video_bytes,
            metrics_token: get("METRICS_TOKEN"),
            giphy_key: get("GIPHY_API_KEY"),
            youtube_key: get("YOUTUBE_API_KEY"),
            vapid_public: get("VAPID_PUBLIC_KEY"),
            vapid_private: get("VAPID_PRIVATE_KEY"),
            vapid_subject: env("VAPID_SUBJECT", "mailto:admin@plbls.fr"),
            fcm_service_account: get("FCM_SERVICE_ACCOUNT"),
            transcribe_url: get("TRANSCRIBE_URL"),
            transcribe_model: env("TRANSCRIBE_MODEL", "Systran/faster-whisper-small"),
            transcribe_alpha: get("TRANSCRIBE_ALPHA") != "false",
            transcribe_language: get("TRANSCRIBE_LANGUAGE"),
            entracte_url: get("ENTRACTE_INTERNE_URL")
                .trim_end_matches('/')
                .to_string(),
            entracte_token: get("ENTRACTE_SERVICE_TOKEN"),
            livekit: Livekit {
                url: get("LIVEKIT_URL"),
                api_url: env("LIVEKIT_API_URL", "http://livekit:7880"),
                key: get("LIVEKIT_API_KEY"),
                secret: get("LIVEKIT_API_SECRET"),
                max_screen_fps: entier("VOICE_MAX_SCREEN_FPS"),
                max_screen_height: entier("VOICE_MAX_SCREEN_HEIGHT"),
                allow_video: get("LIVEKIT_ALLOW_VIDEO") == "true",
            },
        };
        for (nom, valeur) in [
            ("DATABASE_URL", &c.database_url),
            ("AEGIS_TOKEN_ESTOMPE", &c.aegis.token),
            ("LIVEKIT_URL", &c.livekit.url),
            ("LIVEKIT_API_KEY", &c.livekit.key),
            ("LIVEKIT_API_SECRET", &c.livekit.secret),
        ] {
            if valeur.is_empty() {
                return Err(ConfigError(format!("variable manquante : {nom}")));
            }
        }
        Ok(c)
    }

    pub fn entracte_actif(&self) -> bool {
        !self.entracte_url.is_empty() && !self.entracte_token.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const OBLIGATOIRES: [(&str, &str); 5] = [
        ("DATABASE_URL", "postgres://x"),
        ("AEGIS_TOKEN_ESTOMPE", "tok"),
        ("LIVEKIT_URL", "wss://lk"),
        ("LIVEKIT_API_KEY", "k"),
        ("LIVEKIT_API_SECRET", "s"),
    ];

    fn charge(extra: &[(&str, &str)]) -> Result<Config, ConfigError> {
        charge_sans(&[], extra)
    }

    #[test]
    fn debug_masque_les_secrets() {
        let c = charge(&[]).unwrap();
        let texte = format!("{c:?}");
        for secret in ["postgres://x", "tok", "wss://lk", "LIVEKIT"] {
            assert!(!texte.contains(secret), "{texte}");
        }
    }

    fn charge_sans(sans: &[&str], extra: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let mut m: HashMap<String, String> = OBLIGATOIRES
            .iter()
            .filter(|(k, _)| !sans.contains(k))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        m.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        Config::load(|k| m.get(k).cloned())
    }

    fn erreur(r: Result<Config, ConfigError>) -> String {
        r.unwrap_err().to_string()
    }

    #[test]
    fn valeurs_par_defaut() {
        let c = charge(&[]).unwrap();
        assert_eq!(c.listen_addr, ":8080");
        assert_eq!(c.database_url, "postgres://x/estompe");
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.allowed_origins.is_empty());
        assert_eq!(c.aegis.url, "http://aegis:8080");
        assert_eq!(c.aegis.token, "tok");
        assert_eq!(c.data_dir, "/data");
        assert_eq!(c.max_storage_bytes, 20 << 30);
        assert_eq!(c.max_user_storage_bytes, 2048 << 20);
        assert_eq!(c.max_video_bytes, 100 << 20);
        assert_eq!(c.metrics_token, "");
        assert_eq!(c.giphy_key, "");
        assert_eq!(c.youtube_key, "");
        assert_eq!(c.vapid_public, "");
        assert_eq!(c.vapid_private, "");
        assert_eq!(c.vapid_subject, "mailto:admin@plbls.fr");
        assert_eq!(c.fcm_service_account, "");
        assert_eq!(c.transcribe_url, "");
        assert_eq!(c.transcribe_model, "Systran/faster-whisper-small");
        assert!(c.transcribe_alpha);
        assert_eq!(c.transcribe_language, "");
        assert_eq!(c.livekit.url, "wss://lk");
        assert_eq!(c.livekit.api_url, "http://livekit:7880");
        assert_eq!(
            (c.livekit.key.as_str(), c.livekit.secret.as_str()),
            ("k", "s")
        );
        assert_eq!(
            (c.livekit.max_screen_fps, c.livekit.max_screen_height),
            (0, 0)
        );
        assert!(!c.livekit.allow_video);
    }

    #[test]
    fn gateway_joint_par_le_reseau_interne() {
        let c = charge(&[
            ("GATEWAY_URL", "https://public"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080"),
        ])
        .unwrap();
        assert_eq!(c.gateway_url, "http://auth-gateway:8080");
    }

    #[test]
    fn valeurs_lues() {
        let c = charge(&[
            ("LISTEN_ADDR", ":9000"),
            ("GATEWAY_URL", "http://gw"),
            ("ALLOWED_ORIGINS", " https://a , ,https://b,"),
            ("DATA_DIR", "/tmp/d"),
            ("MAX_STORAGE_GB", "3"),
            ("MAX_USER_STORAGE_MB", "5"),
            ("MAX_VIDEO_SIZE_MB", "7"),
            ("METRICS_TOKEN", "m"),
            ("GIPHY_API_KEY", "g"),
            ("YOUTUBE_API_KEY", "y"),
            ("VAPID_PUBLIC_KEY", "pub"),
            ("VAPID_PRIVATE_KEY", "priv"),
            ("VAPID_SUBJECT", "mailto:x@y.z"),
            ("FCM_SERVICE_ACCOUNT", "fcm"),
            ("TRANSCRIBE_URL", "http://t"),
            ("TRANSCRIBE_MODEL", "mod"),
            ("TRANSCRIBE_ALPHA", "false"),
            ("TRANSCRIBE_LANGUAGE", "fr"),
            ("LIVEKIT_API_URL", "http://lk:1"),
            ("VOICE_MAX_SCREEN_FPS", "30"),
            ("VOICE_MAX_SCREEN_HEIGHT", "1080"),
            ("LIVEKIT_ALLOW_VIDEO", "true"),
        ])
        .unwrap();
        assert_eq!(c.listen_addr, ":9000");
        assert_eq!(c.gateway_url, "http://gw");
        assert_eq!(c.allowed_origins, ["https://a", "https://b"]);
        assert_eq!(c.data_dir, "/tmp/d");
        assert_eq!(c.max_storage_bytes, 3 << 30);
        assert_eq!(c.max_user_storage_bytes, 5 << 20);
        assert_eq!(c.max_video_bytes, 7 << 20);
        assert_eq!(c.metrics_token, "m");
        assert_eq!(c.giphy_key, "g");
        assert_eq!(c.youtube_key, "y");
        assert_eq!(
            (c.vapid_public.as_str(), c.vapid_private.as_str()),
            ("pub", "priv")
        );
        assert_eq!(c.vapid_subject, "mailto:x@y.z");
        assert_eq!(c.fcm_service_account, "fcm");
        assert_eq!(c.transcribe_url, "http://t");
        assert_eq!(c.transcribe_model, "mod");
        assert!(!c.transcribe_alpha);
        assert_eq!(c.transcribe_language, "fr");
        assert_eq!(c.livekit.api_url, "http://lk:1");
        assert_eq!(
            (c.livekit.max_screen_fps, c.livekit.max_screen_height),
            (30, 1080)
        );
        assert!(c.livekit.allow_video);
    }

    #[test]
    fn cas_limites_lus_comme_le_go() {
        let c = charge(&[
            ("VOICE_MAX_SCREEN_FPS", "abc"),
            ("VOICE_MAX_SCREEN_HEIGHT", "+720"),
            ("LIVEKIT_ALLOW_VIDEO", "1"),
            ("TRANSCRIBE_ALPHA", "autre"),
            ("MAX_STORAGE_GB", ""),
        ])
        .unwrap();
        assert_eq!(
            (c.livekit.max_screen_fps, c.livekit.max_screen_height),
            (0, 720)
        );
        assert!(!c.livekit.allow_video);
        assert!(c.transcribe_alpha);
        assert_eq!(c.max_storage_bytes, 20 << 30);
    }

    #[test]
    fn plafonds_invalides() {
        for (var, facteur) in [
            ("MAX_STORAGE_GB", "doit être un entier >= 1"),
            ("MAX_USER_STORAGE_MB", "doit être un entier >= 1"),
            ("MAX_VIDEO_SIZE_MB", "doit être un entier >= 1"),
        ] {
            for v in ["abc", "0", "-1", "1.5", "99999999999999"] {
                assert_eq!(
                    erreur(charge(&[(var, v)])),
                    format!("{var} {facteur}"),
                    "{var}={v}"
                );
            }
        }
    }

    #[test]
    fn ordre_de_validation() {
        assert_eq!(
            erreur(charge_sans(
                &["DATABASE_URL"],
                &[("MAX_STORAGE_GB", "0"), ("MAX_USER_STORAGE_MB", "0")]
            )),
            "MAX_STORAGE_GB doit être un entier >= 1"
        );
        assert_eq!(
            erreur(charge_sans(
                &["DATABASE_URL"],
                &[("MAX_USER_STORAGE_MB", "0"), ("MAX_VIDEO_SIZE_MB", "0")]
            )),
            "MAX_USER_STORAGE_MB doit être un entier >= 1"
        );
        assert_eq!(
            erreur(charge_sans(
                &["DATABASE_URL"],
                &[("MAX_VIDEO_SIZE_MB", "0")]
            )),
            "MAX_VIDEO_SIZE_MB doit être un entier >= 1"
        );
    }

    #[test]
    fn variable_manquante() {
        for (nom, _) in OBLIGATOIRES {
            assert_eq!(
                erreur(charge_sans(&[nom], &[])),
                format!("variable manquante : {nom}")
            );
        }
        assert_eq!(
            erreur(charge_sans(
                &["LIVEKIT_API_SECRET", "LIVEKIT_URL", "DATABASE_URL"],
                &[]
            )),
            "variable manquante : DATABASE_URL"
        );
        assert_eq!(
            erreur(charge_sans(&["LIVEKIT_API_SECRET", "LIVEKIT_API_KEY"], &[])),
            "variable manquante : LIVEKIT_API_KEY"
        );
        assert_eq!(
            erreur(charge_sans(&["DATABASE_URL"], &[("DATABASE_URL", "")])),
            "variable manquante : DATABASE_URL"
        );
    }

    #[test]
    fn aegis() {
        let c = charge(&[]).unwrap();
        assert_eq!(c.aegis.admin_role, "aegis-admin");
        assert_eq!(c.aegis.redis_prefix, "aegis");
        let c = charge(&[
            ("AEGIS_URL", "http://aegis:9000/"),
            ("AEGIS_RESOURCES_URL", "http://api:8080/"),
        ])
        .unwrap();
        assert_eq!(c.aegis.url, "http://aegis:9000");
        assert_eq!(c.aegis.resources_url, "http://api:8080");
        assert_eq!(c.aegis.redis_url, "");
        let c = charge(&[
            ("REDIS_URL", "redis://redis:6379"),
            ("REDIS_PASSWORD", "mdp"),
        ])
        .unwrap();
        assert_eq!(c.aegis.redis_url, "redis://:mdp@redis:6379");
    }
}
