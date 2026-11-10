use serde_json::{Value, json};

use crate::config::Arr;

#[derive(Clone, Debug, PartialEq)]
pub struct FichierArr {
    pub chemin: String,
    pub taille: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilmRadarr {
    pub id: i64,
    pub tmdb_id: i64,
    pub titre: String,
    pub annee: Option<i32>,
    pub fichier: Option<FichierArr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SerieSonarr {
    pub id: i64,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub titre: String,
    pub annee: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeSonarr {
    pub saison: i32,
    pub episode: i32,
    pub titre: String,
    pub fichier: Option<FichierArr>,
}

pub struct Client<'a> {
    arr: &'a Arr,
    http: &'a reqwest::Client,
}

fn erreur(e: reqwest::Error) -> String {
    e.without_url().to_string()
}

fn entier(v: &Value, cle: &str) -> Option<i64> {
    v[cle].as_i64().filter(|n| *n > 0)
}

fn annee(v: &Value) -> Option<i32> {
    v["year"]
        .as_i64()
        .filter(|a| *a > 0)
        .and_then(|a| i32::try_from(a).ok())
}

fn fichier(v: &Value) -> Option<FichierArr> {
    Some(FichierArr {
        chemin: v["path"].as_str().filter(|p| !p.is_empty())?.to_string(),
        taille: v["size"].as_i64().unwrap_or(0),
    })
}

impl<'a> Client<'a> {
    pub fn new(arr: &'a Arr, http: &'a reqwest::Client) -> Self {
        Self { arr, http }
    }

    fn requete(&self, methode: reqwest::Method, chemin: &str) -> reqwest::RequestBuilder {
        self.http
            .request(methode, format!("{}/api/v3{chemin}", self.arr.url))
            .header("X-Api-Key", &self.arr.cle)
    }

    async fn envoyer(&self, r: reqwest::RequestBuilder) -> Result<Value, String> {
        let rep = r.send().await.map_err(erreur)?;
        let statut = rep.status();
        if !statut.is_success() {
            let corps: String = rep
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect();
            tracing::warn!("Radarr/Sonarr : HTTP {} {}", statut.as_u16(), corps.trim());
            return Err(format!("HTTP {}", statut.as_u16()));
        }
        let octets = rep.bytes().await.map_err(erreur)?;
        if octets.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&octets).map_err(|e| format!("réponse illisible : {e}"))
    }

    async fn get(&self, chemin: &str) -> Result<Value, String> {
        self.envoyer(self.requete(reqwest::Method::GET, chemin))
            .await
    }

    async fn liste(&self, chemin: &str) -> Result<Vec<Value>, String> {
        match self.get(chemin).await? {
            Value::Array(a) => Ok(a),
            _ => Err(format!("{chemin} : liste attendue")),
        }
    }

    pub async fn films(&self) -> Result<Vec<FilmRadarr>, String> {
        Ok(self
            .liste("/movie")
            .await?
            .iter()
            .filter_map(|m| {
                Some(FilmRadarr {
                    id: entier(m, "id")?,
                    tmdb_id: entier(m, "tmdbId")?,
                    titre: m["title"].as_str().unwrap_or_default().to_string(),
                    annee: annee(m),
                    fichier: m.get("movieFile").and_then(fichier),
                })
            })
            .collect())
    }

    pub async fn series(&self) -> Result<Vec<SerieSonarr>, String> {
        Ok(self
            .liste("/series")
            .await?
            .iter()
            .filter_map(|s| {
                Some(SerieSonarr {
                    id: entier(s, "id")?,
                    tmdb_id: entier(s, "tmdbId"),
                    tvdb_id: entier(s, "tvdbId"),
                    titre: s["title"].as_str().unwrap_or_default().to_string(),
                    annee: annee(s),
                })
            })
            .collect())
    }

    pub async fn episodes(&self, serie: i64) -> Result<Vec<EpisodeSonarr>, String> {
        let fichiers: std::collections::HashMap<i64, FichierArr> = self
            .liste(&format!("/episodefile?seriesId={serie}"))
            .await?
            .iter()
            .filter_map(|f| Some((entier(f, "id")?, fichier(f)?)))
            .collect();
        Ok(self
            .liste(&format!("/episode?seriesId={serie}"))
            .await?
            .iter()
            .filter_map(|e| {
                let saison = i32::try_from(e["seasonNumber"].as_i64()?).ok()?;
                let episode = i32::try_from(e["episodeNumber"].as_i64()?).ok()?;
                let f = (e["hasFile"] == true)
                    .then(|| entier(e, "episodeFileId").and_then(|id| fichiers.get(&id).cloned()))
                    .flatten();
                Some(EpisodeSonarr {
                    saison,
                    episode,
                    titre: e["title"].as_str().unwrap_or_default().to_string(),
                    fichier: f,
                })
            })
            .collect())
    }

    async fn profil_et_racine(&self) -> Result<(i64, String), String> {
        let profil = match self.arr.profil {
            Some(p) => p,
            None => self
                .liste("/qualityprofile")
                .await?
                .first()
                .and_then(|p| entier(p, "id"))
                .ok_or("aucun profil de qualité")?,
        };
        let racine = match &self.arr.racine {
            Some(r) => r.clone(),
            None => self
                .liste("/rootfolder")
                .await?
                .first()
                .and_then(|r| r["path"].as_str().map(String::from))
                .ok_or("aucun dossier racine")?,
        };
        Ok((profil, racine))
    }

    async fn commande(&self, corps: Value) -> Result<(), String> {
        self.envoyer(self.requete(reqwest::Method::POST, "/command").json(&corps))
            .await
            .map(|_| ())
    }

    pub async fn ajouter_film(&self, tmdb_id: i64) -> Result<(), String> {
        let existant = self
            .films()
            .await?
            .into_iter()
            .find(|f| f.tmdb_id == tmdb_id);
        if let Some(f) = existant {
            let mut m = self.get(&format!("/movie/{}", f.id)).await?;
            m["monitored"] = json!(true);
            self.envoyer(
                self.requete(reqwest::Method::PUT, &format!("/movie/{}", f.id))
                    .json(&m),
            )
            .await?;
            return self
                .commande(json!({ "name": "MoviesSearch", "movieIds": [f.id] }))
                .await;
        }
        let mut m = self
            .get(&format!("/movie/lookup/tmdb?tmdbId={tmdb_id}"))
            .await?;
        if !m.is_object() {
            return Err("film inconnu de Radarr".into());
        }
        let (profil, racine) = self.profil_et_racine().await?;
        m["qualityProfileId"] = json!(profil);
        m["rootFolderPath"] = json!(racine);
        m["monitored"] = json!(true);
        m["minimumAvailability"] = json!("released");
        m["addOptions"] = json!({ "searchForMovie": true });
        self.envoyer(self.requete(reqwest::Method::POST, "/movie").json(&m))
            .await
            .map(|_| ())
    }

    pub async fn ajouter_serie(
        &self,
        tmdb_id: i64,
        tvdb_id: Option<i64>,
        saisons: Option<&[i32]>,
    ) -> Result<(), String> {
        let surveillee =
            |n: i64| n > 0 && saisons.is_none_or(|s| s.iter().any(|x| i64::from(*x) == n));
        let existant =
            self.series().await?.into_iter().find(|s| {
                s.tmdb_id == Some(tmdb_id) || (tvdb_id.is_some() && s.tvdb_id == tvdb_id)
            });
        if let Some(s) = existant {
            let mut v = self.get(&format!("/series/{}", s.id)).await?;
            v["monitored"] = json!(true);
            if let Some(liste) = v["seasons"].as_array_mut() {
                for saison in liste {
                    let n = saison["seasonNumber"].as_i64().unwrap_or(0);
                    if surveillee(n) {
                        saison["monitored"] = json!(true);
                    }
                }
            }
            self.envoyer(
                self.requete(reqwest::Method::PUT, &format!("/series/{}", s.id))
                    .json(&v),
            )
            .await?;
            return match saisons {
                None => {
                    self.commande(json!({ "name": "SeriesSearch", "seriesId": s.id }))
                        .await
                }
                Some(liste) => {
                    for n in liste {
                        self.commande(
                            json!({ "name": "SeasonSearch", "seriesId": s.id, "seasonNumber": n }),
                        )
                        .await?;
                    }
                    Ok(())
                }
            };
        }
        let terme = match tvdb_id {
            Some(t) => format!("tvdb:{t}"),
            None => format!("tmdb:{tmdb_id}"),
        };
        let trouvees = self
            .liste(&format!(
                "/series/lookup?term={}",
                url::form_urlencoded::byte_serialize(terme.as_bytes()).collect::<String>()
            ))
            .await?;
        let mut v = trouvees
            .into_iter()
            .next()
            .ok_or("série inconnue de Sonarr")?;
        let (profil, racine) = self.profil_et_racine().await?;
        v["qualityProfileId"] = json!(profil);
        v["rootFolderPath"] = json!(racine);
        v["monitored"] = json!(true);
        v["seasonFolder"] = json!(true);
        if let Some(liste) = v["seasons"].as_array_mut() {
            for saison in liste {
                let n = saison["seasonNumber"].as_i64().unwrap_or(0);
                saison["monitored"] = json!(surveillee(n));
            }
        }
        v["addOptions"] = json!({ "searchForMissingEpisodes": true });
        self.envoyer(self.requete(reqwest::Method::POST, "/series").json(&v))
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::faux;

    #[tokio::test]
    async fn film_deja_connu_surveille_et_recherche() {
        let s = faux(|r| match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v3/movie") => (
                200,
                br#"[{"id": 4, "tmdbId": 103, "title": "C", "monitored": false}]"#.to_vec(),
            ),
            ("GET", "/api/v3/movie/4") => (
                200,
                br#"{"id": 4, "tmdbId": 103, "monitored": false}"#.to_vec(),
            ),
            _ => (200, b"{}".to_vec()),
        })
        .await;
        let arr = Arr {
            url: s.url.clone(),
            cle: "k".into(),
            profil: None,
            racine: None,
        };
        let http = reqwest::Client::new();
        Client::new(&arr, &http).ajouter_film(103).await.unwrap();
        let r = s.requetes();
        assert_eq!(r[0].entete("x-api-key"), "k");
        let maj = r.iter().find(|x| x.methode == "PUT").unwrap();
        assert_eq!(
            (maj.chemin.as_str(), maj.json()["monitored"].clone()),
            ("/api/v3/movie/4", json!(true))
        );
        let cmd = r.iter().find(|x| x.chemin == "/api/v3/command").unwrap();
        assert_eq!(cmd.json(), json!({"name": "MoviesSearch", "movieIds": [4]}));
        assert!(
            !r.iter()
                .any(|x| x.methode == "POST" && x.chemin == "/api/v3/movie")
        );
    }

    #[tokio::test]
    async fn serie_inconnue_ajoutee_avec_les_saisons_demandees() {
        let s = faux(|r| match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v3/series") => (200, b"[]".to_vec()),
            ("GET", "/api/v3/series/lookup") => (200, br#"[{"title": "S", "tvdbId": 9, "seasons": [{"seasonNumber": 0}, {"seasonNumber": 1}, {"seasonNumber": 2}]}]"#.to_vec()),
            ("GET", "/api/v3/qualityprofile") => (200, br#"[{"id": 2}]"#.to_vec()),
            _ => (201, b"{}".to_vec()),
        })
        .await;
        let arr = Arr {
            url: s.url.clone(),
            cle: "k".into(),
            profil: None,
            racine: Some("/tv".into()),
        };
        let http = reqwest::Client::new();
        Client::new(&arr, &http)
            .ajouter_serie(201, Some(9), Some(&[2]))
            .await
            .unwrap();
        let r = s.requetes();
        assert!(
            r.iter()
                .any(|x| x.chemin == "/api/v3/series/lookup" && x.query == "term=tvdb%3A9")
        );
        let ajout = r
            .iter()
            .find(|x| x.methode == "POST" && x.chemin == "/api/v3/series")
            .unwrap()
            .json();
        assert_eq!(
            ajout["seasons"],
            json!([{"seasonNumber": 0, "monitored": false}, {"seasonNumber": 1, "monitored": false}, {"seasonNumber": 2, "monitored": true}])
        );
        assert_eq!(
            (
                ajout["qualityProfileId"].clone(),
                ajout["rootFolderPath"].clone()
            ),
            (json!(2), json!("/tv"))
        );
        assert_eq!(ajout["addOptions"]["searchForMissingEpisodes"], true);
    }
}
