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

#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub gateway_url: String,
    pub allowed_origins: Vec<String>,
    pub permission_url: String,
    pub permission_token: String,
    pub permission_name: String,
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
    pub jellyfin_url: String,
    pub jellyfin_api_key: String,
    pub stream_secret: String,
    pub films_max_flux: i64,
    pub films_max_transcodages: i64,
    pub films_cache_secondes: i64,
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
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::load(|k| std::env::var(k).ok())
    }

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
        let films_max_flux = octets("FILMS_MAX_FLUX", "2", 1)?;
        let films_max_transcodages = octets("FILMS_MAX_TRANSCODAGES", "2", 1)?;
        let films_cache_secondes = octets("FILMS_CACHE_SECONDES", "60", 1)?;
        let c = Config {
            listen_addr: env("LISTEN_ADDR", ":8080"),
            database_url: get("DATABASE_URL"),
            gateway_url: env("GATEWAY_URL", "https://security.plbls.fr"),
            allowed_origins: get("ALLOWED_ORIGINS")
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect(),
            permission_url: env("PERMISSION_URL", "https://permission.plbls.fr"),
            permission_token: get("PERMISSION_TOKEN"),
            permission_name: env("PERMISSION_NAME", "estompe:access"),
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
            jellyfin_url: get("JELLYFIN_URL").trim_end_matches('/').to_string(),
            jellyfin_api_key: get("JELLYFIN_API_KEY"),
            stream_secret: get("STREAM_SECRET"),
            films_max_flux,
            films_max_transcodages,
            films_cache_secondes,
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
            ("PERMISSION_TOKEN", &c.permission_token),
            ("LIVEKIT_URL", &c.livekit.url),
            ("LIVEKIT_API_KEY", &c.livekit.key),
            ("LIVEKIT_API_SECRET", &c.livekit.secret),
        ] {
            if valeur.is_empty() {
                return Err(ConfigError(format!("variable manquante : {nom}")));
            }
        }
        if c.jellyfin_actif() && c.stream_secret.len() < 32 {
            return Err(ConfigError(
                "STREAM_SECRET doit compter au moins 32 caractères".into(),
            ));
        }
        Ok(c)
    }

    pub fn jellyfin_actif(&self) -> bool {
        !self.jellyfin_url.is_empty() && !self.jellyfin_api_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const OBLIGATOIRES: [(&str, &str); 5] = [
        ("DATABASE_URL", "postgres://x"),
        ("PERMISSION_TOKEN", "tok"),
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
    fn jellyfin_exige_un_secret_de_flux() {
        let c = charge(&[]).unwrap();
        assert!(!c.jellyfin_actif());
        assert_eq!(
            (
                c.films_max_flux,
                c.films_max_transcodages,
                c.films_cache_secondes
            ),
            (2, 2, 60)
        );
        let jellyfin = [
            ("JELLYFIN_URL", "http://jellyfin:8096/"),
            ("JELLYFIN_API_KEY", "k"),
        ];
        assert_eq!(
            erreur(charge(&jellyfin)),
            "STREAM_SECRET doit compter au moins 32 caractères"
        );
        let secret = "s".repeat(32);
        let mut avec = jellyfin.to_vec();
        avec.push(("STREAM_SECRET", &secret));
        let c = charge(&avec).unwrap();
        assert!(c.jellyfin_actif());
        assert_eq!(c.jellyfin_url, "http://jellyfin:8096");
        assert!(!format!("{c:?}").contains(&secret));
        assert_eq!(
            erreur(charge(&[("FILMS_MAX_FLUX", "0")])),
            "FILMS_MAX_FLUX doit être un entier >= 1"
        );
    }

    #[test]
    fn valeurs_par_defaut() {
        let c = charge(&[]).unwrap();
        assert_eq!(c.listen_addr, ":8080");
        assert_eq!(c.database_url, "postgres://x");
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.allowed_origins.is_empty());
        assert_eq!(c.permission_url, "https://permission.plbls.fr");
        assert_eq!(c.permission_token, "tok");
        assert_eq!(c.permission_name, "estompe:access");
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
    fn valeurs_lues() {
        let c = charge(&[
            ("LISTEN_ADDR", ":9000"),
            ("GATEWAY_URL", "http://gw"),
            ("ALLOWED_ORIGINS", " https://a , ,https://b,"),
            ("PERMISSION_URL", "http://perm"),
            ("PERMISSION_NAME", "app:perm"),
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
        assert_eq!(c.permission_url, "http://perm");
        assert_eq!(c.permission_name, "app:perm");
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
}
