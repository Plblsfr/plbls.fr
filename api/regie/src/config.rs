use std::{path::PathBuf, time::Duration};

use commun::config::nettoie;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arr {
    pub url: String,
    pub cle: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qbittorrent {
    pub url: String,
    pub cle: Option<String>,
    pub utilisateur: String,
    pub mot_de_passe: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Depots {
    pub dossier: PathBuf,
    pub vu_par_qbittorrent: String,
    pub vu_par_arr: String,
    pub plafond: u64,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub gateway_url: String,
    pub aegis: Option<(String, String)>,
    pub origines: Vec<String>,
    pub radarr: Option<Arr>,
    pub sonarr: Option<Arr>,
    pub lidarr: Option<Arr>,
    pub prowlarr: Option<Arr>,
    pub qbittorrent: Option<Qbittorrent>,
    pub periode_suivi: Duration,
    pub depots: Option<Depots>,
}

fn sans_slash(url: String) -> String {
    url.trim_end_matches('/').to_string()
}

impl Config {
    pub fn from_env(brut: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let env = |k: &str| brut(k).map(|v| nettoie(&v)).filter(|v| !v.is_empty());
        let arr = |nom: &str| {
            env(&format!("REGIE_{nom}_URL"))
                .zip(env(&format!("REGIE_{nom}_CLE")))
                .map(|(url, cle)| Arr {
                    url: sans_slash(url),
                    cle,
                })
        };
        let gateway_public =
            env("GATEWAY_URL").unwrap_or_else(|| "https://security.plbls.fr".into());
        let port = match env("REGIE_PORT") {
            None => 8080,
            Some(v) => v
                .parse()
                .map_err(|_| format!("REGIE_PORT invalide : {v}"))?,
        };
        let suivi_ms: u64 = match env("REGIE_SUIVI_MS") {
            None => 1500,
            Some(v) => v
                .parse()
                .ok()
                .filter(|n| *n >= 500)
                .ok_or_else(|| format!("REGIE_SUIVI_MS invalide (500 au moins) : {v}"))?,
        };
        let plafond_go: u64 = match env("REGIE_DEPOTS_GO") {
            None => 200,
            Some(v) => v
                .parse()
                .map_err(|_| format!("REGIE_DEPOTS_GO invalide : {v}"))?,
        };
        let depots = match env("REGIE_DEPOTS") {
            None => None,
            Some(d) if !d.starts_with('/') || d.contains("..") => {
                return Err(format!(
                    "REGIE_DEPOTS : chemin absolu sans « .. » attendu ({d})"
                ));
            }
            Some(d) => {
                let d = sans_slash(d);
                Some(Depots {
                    vu_par_qbittorrent: env("REGIE_DEPOTS_QBITTORRENT")
                        .map_or_else(|| d.clone(), sans_slash),
                    vu_par_arr: env("REGIE_DEPOTS_ARR").map_or_else(|| d.clone(), sans_slash),
                    dossier: PathBuf::from(d),
                    plafond: plafond_go.saturating_mul(1 << 30),
                })
            }
        };
        Ok(Self {
            port,
            depots,
            periode_suivi: Duration::from_millis(suivi_ms),
            gateway_url: sans_slash(env("GATEWAY_INTERNE_URL").unwrap_or(gateway_public)),
            aegis: env("AEGIS_URL")
                .map(sans_slash)
                .zip(env("AEGIS_TOKEN_REGIE")),
            origines: env("REGIE_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|o| o.trim().trim_end_matches('/').to_string())
                .filter(|o| !o.is_empty())
                .collect(),
            radarr: arr("RADARR"),
            sonarr: arr("SONARR"),
            lidarr: arr("LIDARR"),
            prowlarr: arr("PROWLARR"),
            qbittorrent: env("REGIE_QBITTORRENT_URL").map(|url| Qbittorrent {
                url: sans_slash(url),
                cle: env("REGIE_QBITTORRENT_CLE"),
                utilisateur: env("REGIE_QBITTORRENT_UTILISATEUR").unwrap_or_default(),
                mot_de_passe: env("REGIE_QBITTORRENT_MOT_DE_PASSE").unwrap_or_default(),
            }),
        })
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
        let c = charge(&[]).unwrap();
        assert_eq!(c.port, 8080);
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert!(c.aegis.is_none() && c.origines.is_empty());
        assert!(c.radarr.is_none() && c.sonarr.is_none() && c.lidarr.is_none());
        assert!(c.prowlarr.is_none() && c.qbittorrent.is_none());
        assert_eq!(c.periode_suivi, Duration::from_millis(1500));
        assert!(c.depots.is_none());
    }

    #[test]
    fn valeurs_lues() {
        let c = charge(&[
            ("GATEWAY_URL", "https://g/"),
            ("GATEWAY_INTERNE_URL", "http://auth-gateway:8080/"),
            ("AEGIS_URL", "http://aegis:8080/"),
            ("AEGIS_TOKEN_REGIE", "\"tok\""),
            ("AEGIS_TOKEN_SILLON", "autre"),
            ("REGIE_PORT", "9000"),
            (
                "REGIE_ALLOWED_ORIGINS",
                "https://a.fr/, https://tauri.localhost,",
            ),
            ("REGIE_RADARR_URL", "http://radarr:7878/"),
            ("REGIE_RADARR_CLE", "r"),
            ("REGIE_SONARR_URL", "http://sonarr:8989"),
            ("REGIE_LIDARR_URL", "http://lidarr:8686"),
            ("REGIE_LIDARR_CLE", "l"),
            ("REGIE_PROWLARR_CLE", "p"),
            ("REGIE_QBITTORRENT_URL", "http://qbittorrent:8080/"),
            ("REGIE_QBITTORRENT_CLE", "qbt_abc"),
            ("REGIE_QBITTORRENT_UTILISATEUR", "admin"),
            ("REGIE_QBITTORRENT_MOT_DE_PASSE", "secret"),
        ])
        .unwrap();
        assert_eq!(c.port, 9000);
        assert_eq!(c.gateway_url, "http://auth-gateway:8080");
        assert_eq!(c.aegis, Some(("http://aegis:8080".into(), "tok".into())));
        assert_eq!(c.origines, vec!["https://a.fr", "https://tauri.localhost"]);
        assert_eq!(
            c.radarr,
            Some(Arr {
                url: "http://radarr:7878".into(),
                cle: "r".into()
            })
        );
        assert!(c.sonarr.is_none(), "une URL sans clé est ignorée");
        assert!(c.lidarr.is_some());
        assert!(c.prowlarr.is_none(), "une clé sans URL est ignorée");
        assert_eq!(
            c.qbittorrent,
            Some(Qbittorrent {
                url: "http://qbittorrent:8080".into(),
                cle: Some("qbt_abc".into()),
                utilisateur: "admin".into(),
                mot_de_passe: "secret".into()
            })
        );
    }

    #[test]
    fn erreurs() {
        let c = charge(&[("REGIE_DEPOTS", "/srv/medias/depots/")]).unwrap();
        assert_eq!(
            c.depots,
            Some(Depots {
                dossier: "/srv/medias/depots".into(),
                vu_par_qbittorrent: "/srv/medias/depots".into(),
                vu_par_arr: "/srv/medias/depots".into(),
                plafond: 200 << 30,
            })
        );
        let c = charge(&[
            ("REGIE_DEPOTS", "/depots"),
            ("REGIE_DEPOTS_QBITTORRENT", "/downloads/depots/"),
            ("REGIE_DEPOTS_ARR", "/data/depots"),
            ("REGIE_DEPOTS_GO", "5"),
        ])
        .unwrap()
        .depots
        .unwrap();
        assert_eq!(
            (
                c.vu_par_qbittorrent.as_str(),
                c.vu_par_arr.as_str(),
                c.plafond
            ),
            ("/downloads/depots", "/data/depots", 5 << 30)
        );
        for faux in [
            ("REGIE_PORT", "x"),
            ("REGIE_SUIVI_MS", "100"),
            ("REGIE_DEPOTS", "relatif"),
            ("REGIE_DEPOTS", "/srv/medias/../etc"),
            ("REGIE_DEPOTS_GO", "x"),
        ] {
            assert!(charge(&[faux]).unwrap_err().contains(faux.0), "{faux:?}");
        }
        let c = charge(&[("REGIE_SUIVI_MS", "3000")]).unwrap();
        assert_eq!(c.periode_suivi, Duration::from_millis(3000));
    }
}
