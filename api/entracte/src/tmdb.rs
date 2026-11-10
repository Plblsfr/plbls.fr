use serde_json::Value;

use crate::config::Config;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fiche {
    pub titre: String,
    pub titre_original: Option<String>,
    pub annee: Option<i32>,
    pub resume: Option<String>,
    pub genres: Vec<String>,
    pub affiche: Option<String>,
    pub fond: Option<String>,
    pub duree: Option<f64>,
    pub saisons: Vec<SaisonTmdb>,
    pub tvdb_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaisonTmdb {
    pub numero: i32,
    pub titre: String,
    pub episodes: i32,
    pub affiche: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeTmdb {
    pub numero: i32,
    pub titre: String,
    pub resume: Option<String>,
    pub image: Option<String>,
    pub duree: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resultat {
    pub genre: &'static str,
    pub tmdb_id: i64,
    pub titre: String,
    pub annee: Option<i32>,
    pub resume: Option<String>,
    pub affiche: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Erreur {
    NonConfigure,
    Introuvable,
    Injoignable(String),
}

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erreur::NonConfigure => write!(f, "TMDB non configuré"),
            Erreur::Introuvable => write!(f, "titre inconnu de TMDB"),
            Erreur::Injoignable(e) => write!(f, "TMDB injoignable : {e}"),
        }
    }
}

pub struct Tmdb<'a> {
    cfg: &'a Config,
    http: &'a reqwest::Client,
}

fn annee(texte: Option<&str>) -> Option<i32> {
    texte.and_then(|t| t.get(..4)).and_then(|a| a.parse().ok())
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v[cle]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

pub fn chemin_image_valide(chemin: &str) -> bool {
    chemin.len() <= 80
        && chemin.starts_with('/')
        && chemin[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        && !chemin.contains("..")
}

fn image(v: &Value, cle: &str) -> Option<String> {
    texte(v, cle).filter(|c| chemin_image_valide(c))
}

impl<'a> Tmdb<'a> {
    pub fn new(cfg: &'a Config, http: &'a reqwest::Client) -> Self {
        Self { cfg, http }
    }

    pub fn configure(&self) -> bool {
        !self.cfg.tmdb_cle.is_empty()
    }

    async fn lire(&self, chemin: &str, params: &[(&str, &str)]) -> Result<Value, Erreur> {
        if !self.configure() {
            return Err(Erreur::NonConfigure);
        }
        let mut r = self
            .http
            .get(format!("{}/3{chemin}", self.cfg.tmdb_url))
            .query(&[("language", self.cfg.langue.as_str())])
            .query(params);
        let cle = &self.cfg.tmdb_cle;
        r = if cle.starts_with("ey") {
            r.bearer_auth(cle)
        } else {
            r.query(&[("api_key", cle.as_str())])
        };
        let rep = r
            .send()
            .await
            .map_err(|e| Erreur::Injoignable(e.without_url().to_string()))?;
        match rep.status().as_u16() {
            200 => rep
                .json()
                .await
                .map_err(|e| Erreur::Injoignable(e.without_url().to_string())),
            404 => Err(Erreur::Introuvable),
            s => Err(Erreur::Injoignable(format!("HTTP {s}"))),
        }
    }

    fn fiche(v: &Value, film: bool) -> Fiche {
        let (titre, original, date) = if film {
            ("title", "original_title", "release_date")
        } else {
            ("name", "original_name", "first_air_date")
        };
        let duree = if film {
            v["runtime"].as_f64()
        } else {
            v["episode_run_time"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_f64)
        };
        Fiche {
            titre: texte(v, titre).unwrap_or_default(),
            titre_original: texte(v, original),
            annee: annee(v[date].as_str()),
            resume: texte(v, "overview"),
            genres: v["genres"]
                .as_array()
                .map(|g| g.iter().filter_map(|x| texte(x, "name")).collect())
                .unwrap_or_default(),
            affiche: image(v, "poster_path"),
            fond: image(v, "backdrop_path"),
            duree: duree.filter(|d| *d > 0.0).map(|m| m * 60.0),
            saisons: v["seasons"]
                .as_array()
                .map(|s| {
                    s.iter()
                        .filter_map(|x| {
                            Some(SaisonTmdb {
                                numero: i32::try_from(x["season_number"].as_i64()?).ok()?,
                                titre: texte(x, "name").unwrap_or_default(),
                                episodes: i32::try_from(x["episode_count"].as_i64().unwrap_or(0))
                                    .unwrap_or(0),
                                affiche: image(x, "poster_path"),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            tvdb_id: v["external_ids"]["tvdb_id"].as_i64(),
        }
    }

    pub async fn film(&self, id: i64) -> Result<Fiche, Erreur> {
        let v = self.lire(&format!("/movie/{id}"), &[]).await?;
        Ok(Self::fiche(&v, true))
    }

    pub async fn serie(&self, id: i64) -> Result<Fiche, Erreur> {
        let v = self
            .lire(
                &format!("/tv/{id}"),
                &[("append_to_response", "external_ids")],
            )
            .await?;
        Ok(Self::fiche(&v, false))
    }

    pub async fn saison(&self, id: i64, numero: i32) -> Result<Vec<EpisodeTmdb>, Erreur> {
        let v = self.lire(&format!("/tv/{id}/season/{numero}"), &[]).await?;
        Ok(v["episodes"]
            .as_array()
            .map(|e| {
                e.iter()
                    .filter_map(|x| {
                        Some(EpisodeTmdb {
                            numero: i32::try_from(x["episode_number"].as_i64()?).ok()?,
                            titre: texte(x, "name").unwrap_or_default(),
                            resume: texte(x, "overview"),
                            image: image(x, "still_path"),
                            duree: x["runtime"].as_f64().filter(|d| *d > 0.0).map(|m| m * 60.0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn rechercher(&self, q: &str) -> Result<Vec<Resultat>, Erreur> {
        let v = self
            .lire("/search/multi", &[("query", q), ("include_adult", "false")])
            .await?;
        Ok(v["results"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter_map(|x| {
                        let (genre, titre, date) = match x["media_type"].as_str()? {
                            "movie" => ("film", "title", "release_date"),
                            "tv" => ("serie", "name", "first_air_date"),
                            _ => return None,
                        };
                        Some(Resultat {
                            genre,
                            tmdb_id: x["id"].as_i64()?,
                            titre: texte(x, titre)?,
                            annee: annee(x[date].as_str()),
                            resume: texte(x, "overview"),
                            affiche: image(x, "poster_path"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn image(&self, chemin: &str, largeur: u32) -> Result<(String, Vec<u8>), Erreur> {
        if !chemin_image_valide(chemin) {
            return Err(Erreur::Introuvable);
        }
        let taille = match largeur {
            0..=342 => "w342",
            343..=500 => "w500",
            _ => "w1280",
        };
        let rep = self
            .http
            .get(format!("{}/t/p/{taille}{chemin}", self.cfg.tmdb_images))
            .send()
            .await
            .map_err(|e| Erreur::Injoignable(e.without_url().to_string()))?;
        if rep.status().as_u16() == 404 {
            return Err(Erreur::Introuvable);
        }
        if !rep.status().is_success() {
            return Err(Erreur::Injoignable(format!(
                "HTTP {}",
                rep.status().as_u16()
            )));
        }
        let mime = rep
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .filter(|m| matches!(*m, "image/jpeg" | "image/png" | "image/webp"))
            .unwrap_or("image/jpeg")
            .to_string();
        let octets = rep
            .bytes()
            .await
            .map_err(|e| Erreur::Injoignable(e.without_url().to_string()))?;
        if octets.len() > 8 << 20 {
            return Err(Erreur::Injoignable("image trop grosse".into()));
        }
        Ok((mime, octets.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fiche_de_film_et_de_serie() {
        let f = Tmdb::fiche(
            &json!({"title": "Le Film", "original_title": "The Film", "release_date": "2021-05-02",
                    "overview": "", "genres": [{"name": "Drame"}], "poster_path": "/a.jpg",
                    "backdrop_path": "javascript:x", "runtime": 100}),
            true,
        );
        assert_eq!(f.titre, "Le Film");
        assert_eq!(f.annee, Some(2021));
        assert_eq!(f.resume, None);
        assert_eq!(f.affiche.as_deref(), Some("/a.jpg"));
        assert_eq!(f.fond, None);
        assert_eq!(f.duree, Some(6000.0));
        let s = Tmdb::fiche(
            &json!({"name": "S", "first_air_date": "2019", "seasons": [
                {"season_number": 0, "name": "Bonus", "episode_count": 2},
                {"season_number": 1, "name": "Saison 1", "episode_count": 8, "poster_path": "/s1.jpg"}],
                "external_ids": {"tvdb_id": 42}}),
            false,
        );
        assert_eq!(s.saisons.len(), 2);
        assert_eq!(s.saisons[1].episodes, 8);
        assert_eq!(s.tvdb_id, Some(42));
    }

    #[test]
    fn chemins_d_image() {
        assert!(chemin_image_valide("/abc-DEF_1.jpg"));
        for c in ["abc.jpg", "/../x", "/a/b.jpg", "/a?b", "", "/x y.jpg"] {
            assert!(!chemin_image_valide(c), "{c}");
        }
    }
}
