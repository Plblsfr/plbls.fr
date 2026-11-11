use std::{
    path::{Component, Path, PathBuf},
    time::Duration,
};

use commun::config::nettoie;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arr {
    pub url: String,
    pub cle: String,
    pub profil: Option<i64>,
    pub racine: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correspondance {
    pub distant: String,
    pub local: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeMateriel {
    Cpu,
    Vaapi,
    Qsv,
    Nvenc,
}

impl ModeMateriel {
    pub fn code(self) -> &'static str {
        match self {
            ModeMateriel::Cpu => "cpu",
            ModeMateriel::Vaapi => "vaapi",
            ModeMateriel::Qsv => "qsv",
            ModeMateriel::Nvenc => "nvenc",
        }
    }

    pub fn lire(texte: &str) -> Option<Self> {
        match texte.trim().to_ascii_lowercase().as_str() {
            "" | "cpu" => Some(ModeMateriel::Cpu),
            "vaapi" => Some(ModeMateriel::Vaapi),
            "qsv" => Some(ModeMateriel::Qsv),
            "nvenc" => Some(ModeMateriel::Nvenc),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub gateway_url: String,
    pub aegis: Option<(String, String)>,
    pub origines: Vec<String>,
    pub secret_flux: String,
    pub jeton_service: String,
    pub jeton_webhook: String,
    pub radarr: Option<Arr>,
    pub sonarr: Option<Arr>,
    pub tmdb_cle: String,
    pub tmdb_url: String,
    pub tmdb_images: String,
    pub tvmaze_url: String,
    pub langue: String,
    pub chemins: Vec<Correspondance>,
    pub medias: PathBuf,
    pub periode_synchro: Duration,
    pub ffmpeg: String,
    pub ffprobe: String,
    pub dossier_transcodage: PathBuf,
    pub transcodage_max_octets: u64,
    pub mode_materiel: ModeMateriel,
    pub peripherique: String,
    pub max_flux: usize,
    pub max_transcodages: usize,
    pub inactivite: Duration,
    pub vapid_publique: String,
    pub vapid_privee: String,
    pub vapid_sujet: String,
    pub fcm: String,
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
                .ok_or_else(|| format!("ENTRACTE_CHEMINS : « {p} » sans « = »"))?;
            let (distant, local) = (distant.trim(), local.trim());
            if distant.is_empty() || local.is_empty() {
                return Err(format!("ENTRACTE_CHEMINS : « {p} » incomplet"));
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
        let database_url = commun::env::url_base(&brut, "entracte")?;
        let gateway_public =
            env("GATEWAY_URL").unwrap_or_else(|| "https://security.plbls.fr".into());
        let gateway_url = sans_slash(env("GATEWAY_INTERNE_URL").unwrap_or(gateway_public));
        let secret_flux = match env("ENTRACTE_STREAM_SECRET") {
            Some(s) if s.len() < 32 => {
                return Err("ENTRACTE_STREAM_SECRET : 32 caractères au moins".into());
            }
            Some(s) => s,
            None => {
                use rand::RngExt;
                let octets: [u8; 32] = rand::rng().random();
                octets.iter().map(|o| format!("{o:02x}")).collect()
            }
        };
        let arr = |prefixe: &str| -> Result<Option<Arr>, String> {
            let (Some(url), Some(cle)) = (
                env(&format!("{prefixe}_URL")),
                env(&format!("{prefixe}_API_KEY")),
            ) else {
                return Ok(None);
            };
            let profil = env(&format!("{prefixe}_QUALITY_PROFILE"))
                .map(|v| {
                    v.parse::<i64>()
                        .map_err(|_| format!("{prefixe}_QUALITY_PROFILE invalide : {v}"))
                })
                .transpose()?;
            Ok(Some(Arr {
                url: sans_slash(url),
                cle,
                profil,
                racine: env(&format!("{prefixe}_ROOT_FOLDER")),
            }))
        };
        let mode = env("ENTRACTE_TRANSCODE_MODE").unwrap_or_default();
        let mode_materiel = ModeMateriel::lire(&mode).ok_or_else(|| {
            format!("ENTRACTE_TRANSCODE_MODE inconnu : {mode} (cpu, vaapi, qsv, nvenc)")
        })?;
        let go: u64 = nombre(
            "ENTRACTE_TRANSCODE_MAX_GO",
            env("ENTRACTE_TRANSCODE_MAX_GO"),
            20,
        )?;
        let minutes: u64 = nombre("ENTRACTE_SYNC_MINUTES", env("ENTRACTE_SYNC_MINUTES"), 15)?;
        let inactivite: u64 = nombre(
            "ENTRACTE_INACTIVITE_SECONDES",
            env("ENTRACTE_INACTIVITE_SECONDES"),
            300,
        )?;
        Ok(Self {
            port: nombre("ENTRACTE_PORT", env("ENTRACTE_PORT"), 8080)?,
            database_url,
            gateway_url,
            aegis: env("AEGIS_URL")
                .map(sans_slash)
                .zip(env("AEGIS_TOKEN_ENTRACTE")),
            origines: env("ENTRACTE_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|o| o.trim().trim_end_matches('/').to_string())
                .filter(|o| !o.is_empty())
                .collect(),
            secret_flux,
            jeton_service: env("ENTRACTE_SERVICE_TOKEN").unwrap_or_default(),
            jeton_webhook: env("ENTRACTE_WEBHOOK_TOKEN").unwrap_or_default(),
            radarr: arr("ENTRACTE_RADARR")?,
            sonarr: arr("ENTRACTE_SONARR")?,
            tmdb_cle: env("ENTRACTE_TMDB_API_KEY").unwrap_or_default(),
            tmdb_url: sans_slash(
                env("ENTRACTE_TMDB_URL").unwrap_or_else(|| "https://api.themoviedb.org".into()),
            ),
            tmdb_images: sans_slash(
                env("ENTRACTE_TMDB_IMAGES_URL").unwrap_or_else(|| "https://image.tmdb.org".into()),
            ),
            tvmaze_url: sans_slash(
                env("ENTRACTE_TVMAZE_URL").unwrap_or_else(|| "https://api.tvmaze.com".into()),
            ),
            langue: env("ENTRACTE_LANGUE").unwrap_or_else(|| "fr-FR".into()),
            chemins: correspondances(&env("ENTRACTE_CHEMINS").unwrap_or_default())?,
            medias: PathBuf::from(env("ENTRACTE_MEDIAS").unwrap_or_else(|| "/srv/medias".into())),
            periode_synchro: Duration::from_secs(minutes * 60),
            ffmpeg: env("ENTRACTE_FFMPEG").unwrap_or_else(|| "ffmpeg".into()),
            ffprobe: env("ENTRACTE_FFPROBE").unwrap_or_else(|| "ffprobe".into()),
            dossier_transcodage: PathBuf::from(
                env("ENTRACTE_TRANSCODE_DIR").unwrap_or_else(|| "/tmp/entracte".into()),
            ),
            transcodage_max_octets: go.saturating_mul(1 << 30),
            mode_materiel,
            peripherique: env("ENTRACTE_TRANSCODE_DEVICE")
                .unwrap_or_else(|| "/dev/dri/renderD128".into()),
            max_flux: nombre("ENTRACTE_MAX_FLUX", env("ENTRACTE_MAX_FLUX"), 2)?,
            max_transcodages: nombre(
                "ENTRACTE_MAX_TRANSCODAGES",
                env("ENTRACTE_MAX_TRANSCODAGES"),
                2,
            )?,
            inactivite: Duration::from_secs(inactivite.max(1)),
            vapid_publique: env("VAPID_PUBLIC_KEY").unwrap_or_default(),
            vapid_privee: env("VAPID_PRIVATE_KEY").unwrap_or_default(),
            vapid_sujet: env("VAPID_SUBJECT").unwrap_or_default(),
            fcm: env("FCM_SERVICE_ACCOUNT").unwrap_or_default(),
        })
    }

    pub fn pour_tests(database_url: &str) -> Self {
        let mut c =
            Self::from_env(|k| (k == "DATABASE_URL").then(|| "postgres://test".to_string()))
                .expect("configuration de test");
        c.database_url = database_url.to_string();
        c.tvmaze_url = String::new();
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
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.aegis.is_none() && c.radarr.is_none() && c.sonarr.is_none());
        assert_eq!(c.secret_flux.len(), 64);
        assert_eq!(c.mode_materiel, ModeMateriel::Cpu);
        assert_eq!((c.max_flux, c.max_transcodages), (2, 2));
        assert_eq!(c.periode_synchro, Duration::from_secs(900));
        assert_eq!(c.langue, "fr-FR");
    }

    #[test]
    fn valeurs_lues() {
        let c = charge(&[
            ("DATABASE_URL", "postgres://x"),
            ("GATEWAY_URL", "https://g/"),
            ("AEGIS_URL", "http://aegis:8080/"),
            ("AEGIS_TOKEN_ENTRACTE", "\"tok\""),
            (
                "ENTRACTE_ALLOWED_ORIGINS",
                "https://a.fr/, https://tauri.localhost",
            ),
            ("ENTRACTE_RADARR_URL", "http://radarr:7878/"),
            ("ENTRACTE_RADARR_API_KEY", "k"),
            ("ENTRACTE_RADARR_QUALITY_PROFILE", "4"),
            ("ENTRACTE_SONARR_URL", "http://sonarr:8989"),
            ("ENTRACTE_TRANSCODE_MODE", "VAAPI"),
            (
                "ENTRACTE_CHEMINS",
                "/movies=/medias/films, /tv=/medias/series",
            ),
            ("ENTRACTE_MEDIAS", "/medias"),
        ])
        .unwrap();
        assert_eq!(c.gateway_url, "https://g");
        assert_eq!(c.aegis, Some(("http://aegis:8080".into(), "tok".into())));
        assert_eq!(c.origines, vec!["https://a.fr", "https://tauri.localhost"]);
        let r = c.radarr.unwrap();
        assert_eq!((r.url.as_str(), r.profil), ("http://radarr:7878", Some(4)));
        assert!(c.sonarr.is_none());
        assert_eq!(c.mode_materiel, ModeMateriel::Vaapi);
        assert_eq!(c.medias, Path::new("/medias"));
        assert_eq!(
            chemin_local(&c.chemins, &c.medias, "/movies/A (2020)/a.mkv").as_deref(),
            Some("/medias/films/A (2020)/a.mkv")
        );
        assert_eq!(chemin_local(&c.chemins, &c.medias, "/moviesX/a.mkv"), None);
        assert_eq!(chemin_local(&c.chemins, &c.medias, "/autre/a.mkv"), None);
        assert_eq!(
            chemin_local(&c.chemins, &c.medias, "/medias/series/b.mkv").as_deref(),
            Some("/medias/series/b.mkv")
        );
        assert_eq!(
            chemin_local(&c.chemins, &c.medias, "/movies/../../etc/passwd"),
            None
        );
    }

    #[test]
    fn chemin_hors_des_medias_refuse_meme_par_lien() {
        let racine = std::env::temp_dir().join(format!("entracte-medias-{}", std::process::id()));
        let (medias, dehors) = (racine.join("medias"), racine.join("dehors"));
        let _ = std::fs::remove_dir_all(&racine);
        std::fs::create_dir_all(&medias).unwrap();
        std::fs::create_dir_all(&dehors).unwrap();
        std::fs::write(medias.join("a.mkv"), b"").unwrap();
        std::fs::write(dehors.join("b.mkv"), b"").unwrap();
        std::os::unix::fs::symlink(&dehors, medias.join("lien")).unwrap();
        let chemins = correspondances(&format!("/movies={}", medias.display())).unwrap();
        let local = |d: &str| chemin_local(&chemins, &medias, d);
        assert_eq!(
            local("/movies/a.mkv"),
            Some(format!("{}/a.mkv", medias.display()))
        );
        assert_eq!(local("/movies/lien/b.mkv"), None);
        assert_eq!(local(&format!("{}/b.mkv", dehors.display())), None);
        assert_eq!(
            local("/movies/absent.mkv"),
            Some(format!("{}/absent.mkv", medias.display()))
        );
        std::fs::remove_dir_all(&racine).unwrap();
    }

    #[test]
    fn erreurs() {
        let base = ("DATABASE_URL", "postgres://x");
        assert!(charge(&[base, ("ENTRACTE_STREAM_SECRET", "court")]).is_err());
        assert!(charge(&[base, ("ENTRACTE_TRANSCODE_MODE", "amd")]).is_err());
        assert!(charge(&[base, ("ENTRACTE_CHEMINS", "/a")]).is_err());
        assert!(charge(&[base, ("ENTRACTE_MAX_FLUX", "x")]).is_err());
    }
}
