use std::{
    path::{Component, Path, PathBuf},
    time::Duration,
};

use commun::config::nettoie;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lidarr {
    pub url: String,
    pub cle: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correspondance {
    pub distant: String,
    pub local: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub gateway_url: String,
    pub aegis: Option<(String, String)>,
    pub origines: Vec<String>,
    pub lidarr: Option<Lidarr>,
    pub jeton_webhook: String,
    pub chemins: Vec<Correspondance>,
    pub medias: PathBuf,
    pub periode_synchro: Duration,
    pub ffprobe: String,
    pub ffmpeg: String,
    pub secret_flux: String,
    pub cache: PathBuf,
    pub cache_max_octets: u64,
    pub max_transcodages: usize,
    pub ajouts: crate::quota::Reglages,
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

pub fn correspondances(texte: &str) -> Result<Vec<Correspondance>, String> {
    texte
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (distant, local) = p
                .split_once('=')
                .ok_or_else(|| format!("SILLON_CHEMINS : « {p} » sans « = »"))?;
            let (distant, local) = (distant.trim(), local.trim());
            if distant.is_empty() || local.is_empty() {
                return Err(format!("SILLON_CHEMINS : « {p} » incomplet"));
            }
            Ok(Correspondance {
                distant: distant.trim_end_matches('/').to_string(),
                local: local.trim_end_matches('/').to_string(),
            })
        })
        .collect()
}

fn sous_medias(chemin: &Path, medias: &Path) -> bool {
    let racine = std::fs::canonicalize(medias).unwrap_or_else(|_| medias.to_path_buf());
    match std::fs::canonicalize(chemin) {
        Ok(reel) => reel.starts_with(&racine),
        Err(_) => {
            chemin.is_absolute()
                && !chemin
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
                && (chemin.starts_with(medias) || chemin.starts_with(&racine))
        }
    }
}

pub fn chemin_local(chemins: &[Correspondance], medias: &Path, distant: &str) -> Option<String> {
    let local = chemins
        .iter()
        .filter(|c| {
            distant == c.distant
                || distant
                    .strip_prefix(&c.distant)
                    .is_some_and(|r| r.starts_with('/'))
        })
        .max_by_key(|c| c.distant.len())
        .map(|c| format!("{}{}", c.local, &distant[c.distant.len()..]))
        .unwrap_or_else(|| distant.to_string());
    sous_medias(Path::new(&local), medias).then_some(local)
}

impl Config {
    pub fn from_env(brut: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let env = |k: &str| brut(k).map(|v| nettoie(&v)).filter(|v| !v.is_empty());
        let database_url = commun::env::url_base(&brut, "sillon")?;
        let gateway_public =
            env("GATEWAY_URL").unwrap_or_else(|| "https://security.plbls.fr".into());
        let minutes: u64 = nombre("SILLON_SYNC_MINUTES", env("SILLON_SYNC_MINUTES"), 15)?;
        let secret_flux = match env("SILLON_STREAM_SECRET") {
            Some(s) if s.len() < 32 => {
                return Err("SILLON_STREAM_SECRET : 32 caractères au moins".into());
            }
            Some(s) => s,
            None => format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
        };
        let cache_go: u64 = nombre("SILLON_CACHE_GO", env("SILLON_CACHE_GO"), 10)?;
        let max_transcodages: usize =
            nombre("SILLON_MAX_TRANSCODAGES", env("SILLON_MAX_TRANSCODAGES"), 2)?;
        if max_transcodages == 0 {
            return Err("SILLON_MAX_TRANSCODAGES : 1 au moins".into());
        }
        Ok(Self {
            port: nombre("SILLON_PORT", env("SILLON_PORT"), 8080)?,
            database_url,
            gateway_url: sans_slash(env("GATEWAY_INTERNE_URL").unwrap_or(gateway_public)),
            aegis: env("AEGIS_URL")
                .map(sans_slash)
                .zip(env("AEGIS_TOKEN_SILLON")),
            origines: env("SILLON_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|o| o.trim().trim_end_matches('/').to_string())
                .filter(|o| !o.is_empty())
                .collect(),
            lidarr: env("SILLON_LIDARR_URL")
                .zip(env("SILLON_LIDARR_CLE"))
                .map(|(url, cle)| Lidarr {
                    url: sans_slash(url),
                    cle,
                }),
            jeton_webhook: env("SILLON_WEBHOOK_TOKEN").unwrap_or_default(),
            chemins: correspondances(&env("SILLON_CHEMINS").unwrap_or_default())?,
            medias: PathBuf::from(env("SILLON_MEDIAS").unwrap_or_else(|| "/srv/musique".into())),
            periode_synchro: Duration::from_secs(minutes * 60),
            ffprobe: env("SILLON_FFPROBE").unwrap_or_else(|| "ffprobe".into()),
            ffmpeg: env("SILLON_FFMPEG").unwrap_or_else(|| "ffmpeg".into()),
            secret_flux,
            cache: PathBuf::from(env("SILLON_CACHE").unwrap_or_else(|| "/tmp/sillon".into())),
            cache_max_octets: cache_go.saturating_mul(1 << 30),
            max_transcodages,
            ajouts: crate::quota::Reglages::from_env(&env)?,
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
        assert_eq!(c.database_url, "postgres://x/sillon");
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.aegis.is_none());
        assert!(c.origines.is_empty());
        assert!(c.lidarr.is_none() && c.jeton_webhook.is_empty() && c.chemins.is_empty());
        assert_eq!(c.medias, Path::new("/srv/musique"));
        assert_eq!(c.periode_synchro, Duration::from_secs(900));
        assert_eq!(c.ffprobe, "ffprobe");
        assert_eq!(c.ffmpeg, "ffmpeg");
        assert_eq!(c.secret_flux.len(), 64);
        assert_ne!(
            c.secret_flux,
            charge(&[("DATABASE_URL", "postgres://x")])
                .unwrap()
                .secret_flux
        );
        assert_eq!(c.cache, Path::new("/tmp/sillon"));
        assert_eq!(c.cache_max_octets, 10 << 30);
        assert_eq!(c.max_transcodages, 2);
    }

    #[test]
    fn variables_de_lecture() {
        let base = ("DATABASE_URL", "postgres://x");
        let secret = "s".repeat(32);
        let c = charge(&[
            base,
            ("SILLON_FFMPEG", "/usr/bin/ffmpeg"),
            ("SILLON_STREAM_SECRET", &secret),
            ("SILLON_CACHE", "/var/cache/sillon"),
            ("SILLON_CACHE_GO", "3"),
            ("SILLON_MAX_TRANSCODAGES", "4"),
        ])
        .unwrap();
        assert_eq!(c.ffmpeg, "/usr/bin/ffmpeg");
        assert_eq!(c.secret_flux, secret);
        assert_eq!(c.cache, Path::new("/var/cache/sillon"));
        assert_eq!(c.cache_max_octets, 3 << 30);
        assert_eq!(c.max_transcodages, 4);
        for faux in [
            ("SILLON_STREAM_SECRET", "court"),
            ("SILLON_CACHE_GO", "x"),
            ("SILLON_MAX_TRANSCODAGES", "0"),
        ] {
            assert!(
                charge(&[base, faux]).unwrap_err().contains(faux.0),
                "{faux:?}"
            );
        }
    }

    #[test]
    fn valeurs_lues() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://postgres-main:5432"),
            ("DATABASE_USER", "plbls"),
            ("GATEWAY_URL", "https://g/"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080/"),
            ("AEGIS_URL", "http://aegis:8080/"),
            ("AEGIS_TOKEN_SILLON", "\"tok\""),
            ("AEGIS_TOKEN_ENTRACTE", "autre"),
            ("SILLON_PORT", "9000"),
            (
                "SILLON_ALLOWED_ORIGINS",
                "https://a.fr/, https://tauri.localhost,",
            ),
            ("SILLON_LIDARR_URL", "http://lidarr:8686/"),
            ("SILLON_LIDARR_CLE", "k"),
            ("SILLON_WEBHOOK_TOKEN", "w"),
            ("SILLON_CHEMINS", "/music=/medias/musique"),
            ("SILLON_MEDIAS", "/medias"),
            ("SILLON_SYNC_MINUTES", "0"),
        ])
        .unwrap();
        assert_eq!(
            c.lidarr,
            Some(Lidarr {
                url: "http://lidarr:8686".into(),
                cle: "k".into()
            })
        );
        assert_eq!(c.jeton_webhook, "w");
        assert!(c.periode_synchro.is_zero());
        assert_eq!(
            chemin_local(&c.chemins, &c.medias, "/music/A/B/01.flac").as_deref(),
            Some("/medias/musique/A/B/01.flac")
        );
        assert_eq!(chemin_local(&c.chemins, &c.medias, "/musicX/a.flac"), None);
        assert_eq!(chemin_local(&c.chemins, &c.medias, "/autre/a.flac"), None);
        assert_eq!(
            chemin_local(&c.chemins, &c.medias, "/music/../../etc/passwd"),
            None
        );
        assert_eq!(c.port, 9000);
        assert_eq!(c.database_url, "postgres://plbls@postgres-main:5432/sillon");
        assert_eq!(c.gateway_url, "http://auth-gateway:8080");
        assert_eq!(c.aegis, Some(("http://aegis:8080".into(), "tok".into())));
        assert_eq!(c.origines, vec!["https://a.fr", "https://tauri.localhost"]);
    }

    #[test]
    fn lidarr_sans_cle_ignore() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://x"),
            ("SILLON_LIDARR_URL", "http://lidarr:8686"),
        ])
        .unwrap();
        assert!(c.lidarr.is_none());
    }

    #[test]
    fn chemin_hors_des_medias_refuse_meme_par_lien() {
        let racine = tempfile::tempdir().unwrap();
        let (medias, dehors) = (racine.path().join("medias"), racine.path().join("dehors"));
        std::fs::create_dir_all(&medias).unwrap();
        std::fs::create_dir_all(&dehors).unwrap();
        std::fs::write(medias.join("a.flac"), b"").unwrap();
        std::fs::write(dehors.join("b.flac"), b"").unwrap();
        std::os::unix::fs::symlink(&dehors, medias.join("lien")).unwrap();
        std::os::unix::fs::symlink(dehors.join("b.flac"), medias.join("c.flac")).unwrap();
        let chemins = correspondances(&format!("/music={}", medias.display())).unwrap();
        let local = |d: &str| chemin_local(&chemins, &medias, d);
        assert_eq!(
            local("/music/a.flac"),
            Some(format!("{}/a.flac", medias.display()))
        );
        assert_eq!(local("/music/lien/b.flac"), None);
        assert_eq!(local("/music/c.flac"), None);
        assert_eq!(local(&format!("{}/b.flac", dehors.display())), None);
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
            charge(&[base, ("SILLON_PORT", "x")])
                .unwrap_err()
                .contains("SILLON_PORT")
        );
        assert!(charge(&[("DATABASE_URL", "postgres-main")]).is_err());
        assert!(charge(&[base, ("SILLON_CHEMINS", "/a")]).is_err());
        assert!(charge(&[base, ("SILLON_CHEMINS", "=/a")]).is_err());
        assert!(charge(&[base, ("SILLON_SYNC_MINUTES", "x")]).is_err());
    }
}
