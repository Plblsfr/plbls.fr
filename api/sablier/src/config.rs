use std::time::Duration;

use commun::config::nettoie;

pub const GROUPE: &str = "sablier";
pub const CONSOMMATEUR: &str = "sablier-api";

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub gateway_url: String,
    pub aegis: Option<(String, String)>,
    pub origines: Vec<String>,
    pub redis_url: Option<String>,
    pub flux: String,
    pub groupe: String,
    pub consommateur: String,
    pub fuseau: String,
    pub conservation_mois: u32,
    pub battement: Duration,
    pub estompe: Option<(String, String)>,
    pub recap_jours_min: u32,
    pub recap_jours_min_annee: u32,
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
        let database_url = commun::env::url_base(&brut, "sablier")?;
        let gateway_public =
            env("GATEWAY_URL").unwrap_or_else(|| "https://security.plbls.fr".into());
        let conservation_mois: u32 = nombre(
            "SABLIER_CONSERVATION_MOIS",
            env("SABLIER_CONSERVATION_MOIS"),
            13,
        )?;
        if conservation_mois == 0 {
            return Err("SABLIER_CONSERVATION_MOIS : 1 au moins".into());
        }
        let battement: u32 = nombre(
            "SABLIER_BATTEMENT_SECONDES",
            env("SABLIER_BATTEMENT_SECONDES"),
            30,
        )?;
        if !(5..=3600).contains(&battement) {
            return Err("SABLIER_BATTEMENT_SECONDES : entre 5 et 3600".into());
        }
        let recap_jours_min: u32 =
            nombre("SABLIER_RECAP_JOURS_MIN", env("SABLIER_RECAP_JOURS_MIN"), 3)?;
        let recap_jours_min_annee: u32 = nombre(
            "SABLIER_RECAP_JOURS_MIN_ANNEE",
            env("SABLIER_RECAP_JOURS_MIN_ANNEE"),
            10,
        )?;
        if recap_jours_min == 0 || recap_jours_min_annee == 0 {
            return Err(
                "SABLIER_RECAP_JOURS_MIN, SABLIER_RECAP_JOURS_MIN_ANNEE : 1 au moins".into(),
            );
        }
        Ok(Self {
            port: nombre("SABLIER_PORT", env("SABLIER_PORT"), 8080)?,
            database_url,
            gateway_url: sans_slash(env("GATEWAY_INTERNE_URL").unwrap_or(gateway_public)),
            aegis: env("AEGIS_URL")
                .map(sans_slash)
                .zip(env("AEGIS_TOKEN_SABLIER")),
            origines: env("SABLIER_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|o| o.trim().trim_end_matches('/').to_string())
                .filter(|o| !o.is_empty())
                .collect(),
            redis_url: Some(commun::env::url_file(&brut)).filter(|u| !u.is_empty()),
            flux: commun::emission::FLUX.to_string(),
            groupe: GROUPE.to_string(),
            consommateur: CONSOMMATEUR.to_string(),
            fuseau: env("SABLIER_FUSEAU").unwrap_or_else(|| "Europe/Paris".into()),
            conservation_mois,
            battement: Duration::from_secs(u64::from(battement)),
            estompe: env("SABLIER_ESTOMPE_URL")
                .map(sans_slash)
                .zip(env("SABLIER_ESTOMPE_TOKEN")),
            recap_jours_min,
            recap_jours_min_annee,
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
        assert_eq!(c.database_url, "postgres://x/sablier");
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.aegis.is_none());
        assert!(c.origines.is_empty());
        assert!(c.redis_url.is_none());
        assert_eq!(c.flux, "sablier:evenements");
        assert_eq!(
            (c.groupe.as_str(), c.consommateur.as_str()),
            ("sablier", "sablier-api")
        );
        assert_eq!(c.fuseau, "Europe/Paris");
        assert_eq!(c.conservation_mois, 13);
        assert_eq!(c.battement, Duration::from_secs(30));
        assert!(c.estompe.is_none());
        assert_eq!((c.recap_jours_min, c.recap_jours_min_annee), (3, 10));
    }

    #[test]
    fn valeurs_lues() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://postgres-main:5432"),
            ("DATABASE_USER", "plbls"),
            ("GATEWAY_URL", "https://g/"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080/"),
            ("AEGIS_URL", "http://aegis:8080/"),
            ("AEGIS_TOKEN_SABLIER", "\"tok\""),
            ("AEGIS_TOKEN_SILLON", "autre"),
            ("SABLIER_PORT", "9000"),
            (
                "SABLIER_ALLOWED_ORIGINS",
                "https://a.fr/, https://tauri.localhost,",
            ),
            ("SABLIER_REDIS_URL", "redis://sablier@redis:6379"),
            ("SABLIER_REDIS_PASSWORD", "secret"),
            ("SABLIER_FUSEAU", "Europe/Lisbon"),
            ("SABLIER_CONSERVATION_MOIS", "6"),
            ("SABLIER_BATTEMENT_SECONDES", "60"),
            ("SABLIER_ESTOMPE_URL", "http://estompe-api:8080/"),
            ("SABLIER_ESTOMPE_TOKEN", "jeton"),
            ("SABLIER_RECAP_JOURS_MIN", "5"),
            ("SABLIER_RECAP_JOURS_MIN_ANNEE", "20"),
        ])
        .unwrap();
        assert_eq!(c.port, 9000);
        assert_eq!(
            c.database_url,
            "postgres://plbls@postgres-main:5432/sablier"
        );
        assert_eq!(c.gateway_url, "http://auth-gateway:8080");
        assert_eq!(c.aegis, Some(("http://aegis:8080".into(), "tok".into())));
        assert_eq!(c.origines, vec!["https://a.fr", "https://tauri.localhost"]);
        assert_eq!(
            c.redis_url.as_deref(),
            Some("redis://sablier:secret@redis:6379")
        );
        assert_eq!(c.fuseau, "Europe/Lisbon");
        assert_eq!(c.conservation_mois, 6);
        assert_eq!(c.battement, Duration::from_secs(60));
        assert_eq!(
            c.estompe,
            Some(("http://estompe-api:8080".into(), "jeton".into()))
        );
        assert_eq!((c.recap_jours_min, c.recap_jours_min_annee), (5, 20));
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
    fn estompe_sans_jeton_ignore() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://x"),
            ("SABLIER_ESTOMPE_URL", "http://estompe-api:8080"),
        ])
        .unwrap();
        assert!(c.estompe.is_none());
    }

    #[test]
    fn erreurs() {
        let base = ("DATABASE_URL", "postgres://x");
        for faux in [
            ("SABLIER_PORT", "x"),
            ("SABLIER_CONSERVATION_MOIS", "0"),
            ("SABLIER_CONSERVATION_MOIS", "-1"),
            ("SABLIER_BATTEMENT_SECONDES", "0"),
            ("SABLIER_BATTEMENT_SECONDES", "x"),
            ("SABLIER_BATTEMENT_SECONDES", "86400"),
            ("SABLIER_RECAP_JOURS_MIN", "0"),
            ("SABLIER_RECAP_JOURS_MIN_ANNEE", "x"),
        ] {
            assert!(
                charge(&[base, faux]).unwrap_err().contains(faux.0),
                "{faux:?}"
            );
        }
        assert!(charge(&[("DATABASE_URL", "postgres-main")]).is_err());
    }
}
