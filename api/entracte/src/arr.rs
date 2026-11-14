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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reglages {
    pub racine: Option<String>,
    pub profil: Option<i64>,
    pub tags: Option<Vec<i64>>,
}

struct Envoi {
    profil: i64,
    racine: String,
    tags: Vec<i64>,
    disponibilite: String,
}

fn habituel(titres: &[Value], cle: &str, valable: impl Fn(&Value) -> bool) -> Option<Value> {
    let mut comptes: Vec<(Value, usize)> = Vec::new();
    for v in titres.iter().map(|t| &t[cle]).filter(|v| valable(v)) {
        match comptes.iter_mut().find(|(x, _)| x == v) {
            Some((_, n)) => *n += 1,
            None => comptes.push((v.clone(), 1)),
        }
    }
    let plus = comptes.iter().map(|(_, n)| *n).max()?;
    comptes
        .into_iter()
        .find(|(_, n)| *n == plus)
        .map(|(v, _)| v)
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

    pub async fn options(&self, bibliotheque: &str) -> Result<Value, String> {
        let dossiers: Vec<Value> = self
            .liste("/rootfolder")
            .await?
            .iter()
            .filter_map(|r| {
                let chemin = r["path"].as_str().filter(|p| !p.is_empty())?;
                Some(json!({ "path": chemin, "free": r["freeSpace"].as_i64() }))
            })
            .collect();
        let profils: Vec<Value> = self
            .liste("/qualityprofile")
            .await?
            .iter()
            .filter_map(|p| {
                let id = entier(p, "id")?;
                let nom = p["name"].as_str().unwrap_or_default();
                Some(json!({ "id": id, "name": nom }))
            })
            .collect();
        let tags: Vec<Value> = self
            .liste("/tag")
            .await?
            .iter()
            .filter_map(|t| {
                let id = entier(t, "id")?;
                let nom = t["label"].as_str().unwrap_or_default();
                Some(json!({ "id": id, "label": nom }))
            })
            .collect();
        let titres = self.liste(bibliotheque).await?;
        let connu = |liste: &[Value], cle: &str, v: &Value| liste.iter().any(|x| &x[cle] == v);
        let dossier = self
            .arr
            .racine
            .as_ref()
            .map(|r| json!(r))
            .filter(|r| connu(&dossiers, "path", r))
            .or_else(|| habituel(&titres, "rootFolderPath", |v| connu(&dossiers, "path", v)))
            .or_else(|| dossiers.first().map(|d| d["path"].clone()));
        let profil = self
            .arr
            .profil
            .map(|p| json!(p))
            .filter(|p| connu(&profils, "id", p))
            .or_else(|| habituel(&titres, "qualityProfileId", |v| connu(&profils, "id", v)))
            .or_else(|| profils.first().map(|p| p["id"].clone()));
        let habituels: Vec<Value> = tags
            .iter()
            .map(|t| t["id"].clone())
            .filter(|id| {
                let avec = titres
                    .iter()
                    .filter(|t| t["tags"].as_array().is_some_and(|l| l.contains(id)))
                    .count();
                avec * 2 > titres.len()
            })
            .collect();
        let disponibilite = habituel(&titres, "minimumAvailability", Value::is_string);
        Ok(json!({
            "folders": dossiers,
            "profiles": profils,
            "tags": tags,
            "defaults": {
                "folder": dossier,
                "profile": profil,
                "tags": habituels,
                "availability": disponibilite,
            },
        }))
    }

    async fn reglages(&self, choix: &Reglages, bibliotheque: &str) -> Result<Envoi, String> {
        let o = self.options(bibliotheque).await?;
        let connu = |liste: &str, cle: &str, v: &Value| {
            o[liste]
                .as_array()
                .is_some_and(|l| l.iter().any(|x| &x[cle] == v))
        };
        let profil = match choix.profil {
            Some(p) if connu("profiles", "id", &json!(p)) => p,
            Some(_) => return Err("profil de qualité inconnu".into()),
            None => o["defaults"]["profile"]
                .as_i64()
                .ok_or("aucun profil de qualité")?,
        };
        let racine = match &choix.racine {
            Some(r) if connu("folders", "path", &json!(r)) => r.clone(),
            Some(_) => return Err("emplacement inconnu".into()),
            None => o["defaults"]["folder"]
                .as_str()
                .ok_or("aucun dossier racine")?
                .to_string(),
        };
        let tags = match &choix.tags {
            Some(t) if t.iter().all(|x| connu("tags", "id", &json!(x))) => t.clone(),
            Some(_) => return Err("tag inconnu".into()),
            None => o["defaults"]["tags"]
                .as_array()
                .map(|l| l.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default(),
        };
        Ok(Envoi {
            profil,
            racine,
            tags,
            disponibilite: o["defaults"]["availability"]
                .as_str()
                .unwrap_or("released")
                .to_string(),
        })
    }

    async fn commande(&self, corps: Value) -> Result<(), String> {
        self.envoyer(self.requete(reqwest::Method::POST, "/command").json(&corps))
            .await
            .map(|_| ())
    }

    pub async fn ajouter_film(&self, tmdb_id: i64, choix: &Reglages) -> Result<(), String> {
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
        let e = self.reglages(choix, "/movie").await?;
        m["qualityProfileId"] = json!(e.profil);
        m["rootFolderPath"] = json!(e.racine);
        m["tags"] = json!(e.tags);
        m["monitored"] = json!(true);
        m["minimumAvailability"] = json!(e.disponibilite);
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
        choix: &Reglages,
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
        let e = self.reglages(choix, "/series").await?;
        v["qualityProfileId"] = json!(e.profil);
        v["rootFolderPath"] = json!(e.racine);
        v["tags"] = json!(e.tags);
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
        Client::new(&arr, &http)
            .ajouter_film(103, &Reglages::default())
            .await
            .unwrap();
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
            ("GET", "/api/v3/rootfolder") => (200, br#"[{"path": "/tv"}]"#.to_vec()),
            ("GET", "/api/v3/tag") => (200, b"[]".to_vec()),
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
            .ajouter_serie(201, Some(9), Some(&[2]), &Reglages::default())
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

    fn radarr_complet(r: &crate::testutil::Requete) -> (u16, Vec<u8>) {
        let film = |id: i64, dossier: &str, profil: i64, tags: &[i64], dispo: &str| {
            json!({"id": id, "tmdbId": id, "title": "x", "rootFolderPath": dossier,
                "qualityProfileId": profil, "tags": tags, "minimumAvailability": dispo})
        };
        let corps = |v: Value| (200, v.to_string().into_bytes());
        match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v3/movie") => corps(json!([
                film(1, "/films", 1, &[], "released"),
                film(2, "/films-4k", 6, &[3], "announced"),
                film(3, "/films-4k", 6, &[3], "announced"),
                film(4, "/disparu", 9, &[3, 8], "announced"),
            ])),
            ("GET", "/api/v3/movie/lookup/tmdb") => corps(json!({"title": "C", "tmdbId": 103})),
            ("GET", "/api/v3/rootfolder") => corps(json!([
                {"path": "/films", "freeSpace": 1000}, {"path": "/films-4k", "freeSpace": 50}
            ])),
            ("GET", "/api/v3/qualityprofile") => {
                corps(json!([{"id": 1, "name": "Any"}, {"id": 6, "name": "HD-1080p"}]))
            }
            ("GET", "/api/v3/tag") => {
                corps(json!([{"id": 3, "label": "vf"}, {"id": 5, "label": "rare"}]))
            }
            _ => (201, b"{}".to_vec()),
        }
    }

    fn ajouts(s: &crate::testutil::Faux) -> Vec<Value> {
        s.requetes()
            .iter()
            .filter(|x| x.methode == "POST" && x.chemin == "/api/v3/movie")
            .map(|x| x.json())
            .collect()
    }

    fn sans_config(url: &str) -> Arr {
        Arr {
            url: url.to_string(),
            cle: "k".into(),
            profil: None,
            racine: None,
        }
    }

    #[tokio::test]
    async fn film_ajoute_avec_les_reglages_habituels_de_radarr() {
        let s = faux(radarr_complet).await;
        let arr = sans_config(&s.url);
        let http = reqwest::Client::new();
        let client = Client::new(&arr, &http);
        client
            .ajouter_film(103, &Reglages::default())
            .await
            .unwrap();
        let ajout = &ajouts(&s)[0];
        assert_eq!(
            (
                ajout["rootFolderPath"].clone(),
                ajout["qualityProfileId"].clone(),
                ajout["tags"].clone(),
                ajout["minimumAvailability"].clone()
            ),
            (json!("/films-4k"), json!(6), json!([3]), json!("announced")),
            "les réglages les plus utilisés dans la bibliothèque de Radarr"
        );
    }

    #[tokio::test]
    async fn film_ajoute_avec_l_emplacement_le_profil_et_les_tags_choisis() {
        let s = faux(radarr_complet).await;
        let arr = sans_config(&s.url);
        let http = reqwest::Client::new();
        let client = Client::new(&arr, &http);
        let choix = Reglages {
            racine: Some("/films".into()),
            profil: Some(1),
            tags: Some(vec![]),
        };
        client.ajouter_film(103, &choix).await.unwrap();
        let ajout = &ajouts(&s)[0];
        assert_eq!(
            (
                ajout["rootFolderPath"].clone(),
                ajout["qualityProfileId"].clone(),
                ajout["tags"].clone()
            ),
            (json!("/films"), json!(1), json!([]))
        );
        let inconnus = [
            (Some("/ailleurs"), None, None, "emplacement inconnu"),
            (None, Some(9), None, "profil de qualité inconnu"),
            (None, None, Some(vec![3, 8]), "tag inconnu"),
        ];
        for (racine, profil, tags, raison) in inconnus {
            let refuse = Reglages {
                racine: racine.map(String::from),
                profil,
                tags,
            };
            assert_eq!(
                client.ajouter_film(103, &refuse).await,
                Err(raison.to_string())
            );
        }
        assert_eq!(ajouts(&s).len(), 1, "un réglage inconnu n'ajoute rien");
    }

    #[tokio::test]
    async fn options_proposees_et_valeurs_par_defaut() {
        let s = faux(radarr_complet).await;
        let http = reqwest::Client::new();
        let arr = sans_config(&s.url);
        let o = Client::new(&arr, &http).options("/movie").await.unwrap();
        assert_eq!(
            o["folders"],
            json!([{"path": "/films", "free": 1000}, {"path": "/films-4k", "free": 50}])
        );
        assert_eq!(o["profiles"][1], json!({"id": 6, "name": "HD-1080p"}));
        assert_eq!(o["tags"][0], json!({"id": 3, "label": "vf"}));
        assert_eq!(
            o["defaults"],
            json!({"folder": "/films-4k", "profile": 6, "tags": [3], "availability": "announced"})
        );
        let arr = Arr {
            profil: Some(1),
            racine: Some("/films".into()),
            ..sans_config(&s.url)
        };
        let o = Client::new(&arr, &http).options("/movie").await.unwrap();
        assert_eq!(
            (
                o["defaults"]["folder"].clone(),
                o["defaults"]["profile"].clone()
            ),
            (json!("/films"), json!(1)),
            "la configuration d'Entracte l'emporte"
        );
        let arr = Arr {
            profil: Some(99),
            racine: Some("/disparu".into()),
            ..sans_config(&s.url)
        };
        let o = Client::new(&arr, &http).options("/movie").await.unwrap();
        assert_eq!(
            (
                o["defaults"]["folder"].clone(),
                o["defaults"]["profile"].clone()
            ),
            (json!("/films-4k"), json!(6)),
            "une configuration inconnue de Radarr est ignorée"
        );
    }
}
