use std::time::Duration;

use reqwest::Method;
use serde_json::{Value, json};

use crate::arr::Client;

const DELAI_RECHERCHE: Duration = Duration::from_secs(150);
const PAGE: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Radarr,
    Sonarr,
}

impl Service {
    pub const TOUS: [Service; 2] = [Service::Radarr, Service::Sonarr];

    pub fn depuis(s: &str) -> Option<Self> {
        match s {
            "radarr" => Some(Self::Radarr),
            "sonarr" => Some(Self::Sonarr),
            _ => None,
        }
    }

    pub fn cle(self) -> &'static str {
        match self {
            Self::Radarr => "radarr",
            Self::Sonarr => "sonarr",
        }
    }

    pub fn nom(self) -> &'static str {
        match self {
            Self::Radarr => "Radarr",
            Self::Sonarr => "Sonarr",
        }
    }

    fn bibliotheque(self) -> &'static str {
        match self {
            Self::Radarr => "/movie",
            Self::Sonarr => "/series",
        }
    }

    fn inclusions(self) -> &'static str {
        match self {
            Self::Radarr => "includeMovie=true",
            Self::Sonarr => "includeSeries=true&includeEpisode=true",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cible {
    Titre(i64),
    Saison(i64, i32),
    Episodes(Vec<i64>),
    Manquants,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Modification {
    pub surveille: Option<bool>,
    pub profil: Option<i64>,
    pub saisons: Vec<(i32, bool)>,
    pub nouvelles_saisons: Option<bool>,
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v[cle]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

fn noms(v: &Value, cle: &str) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| texte(x, cle))
        .collect()
}

fn chaines(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| match x {
            Value::String(s) => Some(s.trim().to_string()),
            autre => texte(autre, "reason").or_else(|| texte(autre, "message")),
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn positif(v: &Value, cle: &str) -> Option<i64> {
    v[cle].as_i64().filter(|n| *n > 0)
}

fn qualite(v: &Value) -> Option<String> {
    texte(&v["quality"]["quality"], "name")
}

fn annee(v: &Value) -> Option<i64> {
    v["year"].as_i64().filter(|a| *a > 0)
}

fn episode_de(e: &Value) -> Value {
    json!({
        "id": positif(e, "id"),
        "season": e["seasonNumber"].as_i64(),
        "number": e["episodeNumber"].as_i64(),
        "title": texte(e, "title"),
    })
}

fn porteur(service: Service, v: &Value) -> Value {
    let (objet, cle) = match service {
        Service::Radarr => (&v["movie"], "movieId"),
        Service::Sonarr => (&v["series"], "seriesId"),
    };
    json!({
        "service": service.cle(),
        "title_id": positif(v, cle).or_else(|| positif(objet, "id")),
        "tmdb_id": positif(objet, "tmdbId"),
        "title": texte(objet, "title"),
        "year": annee(objet),
    })
}

fn fusion(mut base: Value, plus: Value) -> Value {
    if let (Some(b), Value::Object(p)) = (base.as_object_mut(), plus) {
        b.extend(p);
    }
    base
}

pub fn release(v: &Value) -> Option<Value> {
    let rejets = chaines(&v["rejections"]);
    Some(json!({
        "guid": texte(v, "guid")?,
        "indexer_id": positif(v, "indexerId")?,
        "title": v["title"].as_str().unwrap_or_default(),
        "indexer": texte(v, "indexer"),
        "protocol": texte(v, "protocol"),
        "quality": qualite(v),
        "languages": noms(&v["languages"], "name"),
        "formats": noms(&v["customFormats"], "name"),
        "score": v["customFormatScore"].as_i64().unwrap_or(0),
        "size": v["size"].as_i64().unwrap_or(0),
        "seeders": v["seeders"].as_i64(),
        "leechers": v["leechers"].as_i64(),
        "age": v["age"].as_i64().unwrap_or(0),
        "approved": v["approved"] == true && rejets.is_empty(),
        "rejections": rejets,
        "full_season": v["fullSeason"] == true,
    }))
}

pub fn trier_releases(liste: &mut [Value]) {
    liste.sort_by(|a, b| {
        let cle = |v: &Value| {
            (
                v["approved"] == true,
                v["score"].as_i64().unwrap_or(0),
                v["seeders"].as_i64().unwrap_or(-1),
            )
        };
        cle(b).cmp(&cle(a))
    });
}

pub fn etat_file(v: &Value) -> &'static str {
    let suivi = v["trackedDownloadStatus"].as_str().unwrap_or_default();
    let phase = v["trackedDownloadState"].as_str().unwrap_or_default();
    let statut = v["status"].as_str().unwrap_or_default();
    let egal = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    if egal(suivi, "error")
        || egal(suivi, "warning")
        || egal(statut, "failed")
        || egal(statut, "warning")
        || egal(phase, "importBlocked")
        || egal(phase, "failedPending")
        || texte(v, "errorMessage").is_some()
    {
        "blocked"
    } else if egal(phase, "importPending") || egal(phase, "importing") || egal(statut, "completed")
    {
        "importing"
    } else if egal(statut, "queued") || egal(statut, "delay") {
        "queued"
    } else if egal(statut, "paused") {
        "paused"
    } else {
        "downloading"
    }
}

fn messages_de(v: &Value) -> Vec<String> {
    let mut m: Vec<String> = texte(v, "errorMessage").into_iter().collect();
    for s in v["statusMessages"].as_array().into_iter().flatten() {
        let details = chaines(&s["messages"]);
        if details.is_empty() {
            m.extend(texte(s, "title"));
        } else {
            m.extend(details);
        }
    }
    m.dedup();
    m
}

pub fn element_file(service: Service, v: &Value) -> Option<Value> {
    let taille = v["size"].as_f64().unwrap_or(0.0);
    let reste = v["sizeleft"].as_f64().unwrap_or(0.0);
    let avancement = if taille > 0.0 {
        (((taille - reste) / taille) * 100.0)
            .clamp(0.0, 100.0)
            .round()
    } else {
        0.0
    };
    Some(fusion(
        porteur(service, v),
        json!({
            "id": positif(v, "id")?,
            "episode": (service == Service::Sonarr && v["episode"].is_object()).then(|| episode_de(&v["episode"])),
            "release": v["title"].as_str().unwrap_or_default(),
            "quality": qualite(v),
            "indexer": texte(v, "indexer"),
            "client": texte(v, "downloadClient"),
            "protocol": texte(v, "protocol"),
            "size": taille,
            "left": reste,
            "progress": avancement,
            "state": etat_file(v),
            "messages": messages_de(v),
            "eta": texte(v, "estimatedCompletionTime"),
            "importable": texte(v, "downloadId").is_some(),
        }),
    ))
}

pub fn evenement(v: &Value) -> &'static str {
    match v["eventType"].as_str().unwrap_or_default() {
        "grabbed" => "grabbed",
        "downloadFolderImported" | "seriesFolderImported" | "movieFolderImported" => "imported",
        "downloadFailed" => "failed",
        "movieFileDeleted" | "episodeFileDeleted" => "deleted",
        "movieFileRenamed" | "episodeFileRenamed" => "renamed",
        "downloadIgnored" => "ignored",
        _ => "other",
    }
}

pub fn element_historique(service: Service, v: &Value) -> Value {
    fusion(
        porteur(service, v),
        json!({
            "event": evenement(v),
            "episode": (service == Service::Sonarr && v["episode"].is_object()).then(|| episode_de(&v["episode"])),
            "release": texte(v, "sourceTitle"),
            "quality": qualite(v),
            "date": texte(v, "date"),
            "indexer": texte(&v["data"], "indexer"),
            "reason": texte(&v["data"], "message").or_else(|| texte(&v["data"], "reason")),
        }),
    )
}

fn element_liste_noire(service: Service, v: &Value) -> Option<Value> {
    Some(fusion(
        porteur(service, v),
        json!({
            "id": positif(v, "id")?,
            "release": texte(v, "sourceTitle"),
            "quality": qualite(v),
            "date": texte(v, "date"),
            "indexer": texte(v, "indexer"),
            "reason": texte(v, "message"),
        }),
    ))
}

pub fn titre(service: Service, v: &Value) -> Option<Value> {
    let stats = &v["statistics"];
    let mut t = json!({
        "service": service.cle(),
        "id": positif(v, "id")?,
        "tmdb_id": positif(v, "tmdbId"),
        "title": v["title"].as_str().unwrap_or_default(),
        "year": annee(v),
        "monitored": v["monitored"] == true,
        "profile": positif(v, "qualityProfileId"),
        "folder": texte(v, "rootFolderPath"),
        "status": texte(v, "status"),
    });
    match service {
        Service::Radarr => {
            t["has_file"] = json!(v["hasFile"] == true);
            t["available"] = json!(v["isAvailable"] == true);
            t["size"] = json!(v["sizeOnDisk"].as_i64().unwrap_or(0));
            t["file"] = match &v["movieFile"] {
                f if f.is_object() => json!({ "quality": qualite(f), "size": f["size"].as_i64() }),
                _ => Value::Null,
            };
        }
        Service::Sonarr => {
            t["episodes"] = json!(stats["episodeCount"].as_i64().unwrap_or(0));
            t["episode_files"] = json!(stats["episodeFileCount"].as_i64().unwrap_or(0));
            t["size"] = json!(stats["sizeOnDisk"].as_i64().unwrap_or(0));
            t["monitor_new"] = json!(v["monitorNewItems"].as_str() != Some("none"));
            t["seasons"] = json!(
                v["seasons"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| {
                        let st = &s["statistics"];
                        Some(json!({
                            "number": s["seasonNumber"].as_i64()?,
                            "monitored": s["monitored"] == true,
                            "episodes": st["episodeCount"].as_i64().unwrap_or(0),
                            "episode_files": st["episodeFileCount"].as_i64().unwrap_or(0),
                            "size": st["sizeOnDisk"].as_i64().unwrap_or(0),
                        }))
                    })
                    .collect::<Vec<_>>()
            );
        }
    }
    Some(t)
}

fn episode_fiche(e: &Value) -> Option<Value> {
    let f = &e["episodeFile"];
    Some(json!({
        "id": positif(e, "id")?,
        "season": e["seasonNumber"].as_i64()?,
        "number": e["episodeNumber"].as_i64()?,
        "title": texte(e, "title"),
        "monitored": e["monitored"] == true,
        "has_file": e["hasFile"] == true,
        "air_date": texte(e, "airDateUtc"),
        "quality": qualite(f),
        "size": f["size"].as_i64(),
    }))
}

fn manquant(service: Service, v: &Value) -> Option<Value> {
    Some(match service {
        Service::Radarr => json!({
            "service": service.cle(),
            "title_id": positif(v, "id")?,
            "tmdb_id": positif(v, "tmdbId"),
            "title": texte(v, "title"),
            "year": annee(v),
            "profile": positif(v, "qualityProfileId"),
            "last_search": texte(v, "lastSearchTime"),
        }),
        Service::Sonarr => fusion(
            porteur(service, v),
            json!({
                "episode": episode_de(v),
                "air_date": texte(v, "airDateUtc"),
                "last_search": texte(v, "lastSearchTime"),
            }),
        ),
    })
}

fn fichier_importable(v: &Value) -> Option<Value> {
    let nom = texte(v, "relativePath").or_else(|| texte(v, "name"))?;
    Some(json!({
        "name": nom,
        "size": v["size"].as_i64().unwrap_or(0),
        "quality": qualite(v),
        "languages": noms(&v["languages"], "name"),
        "title": texte(&v["movie"], "title").or_else(|| texte(&v["series"], "title")),
        "episodes": v["episodes"].as_array().into_iter().flatten().map(episode_de).collect::<Vec<_>>(),
        "rejections": chaines(&v["rejections"]),
    }))
}

fn fichier_a_importer(service: Service, v: &Value, download: &str) -> Option<Value> {
    let mut f = json!({
        "path": texte(v, "path")?,
        "quality": v["quality"],
        "languages": v["languages"],
        "releaseGroup": v["releaseGroup"],
        "indexerFlags": v["indexerFlags"].as_i64().unwrap_or(0),
        "downloadId": download,
    });
    match service {
        Service::Radarr => f["movieId"] = json!(positif(&v["movie"], "id")?),
        Service::Sonarr => {
            let episodes: Vec<i64> = v["episodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| positif(e, "id"))
                .collect();
            if episodes.is_empty() {
                return None;
            }
            f["seriesId"] = json!(positif(&v["series"], "id")?);
            f["episodeIds"] = json!(episodes);
            f["releaseType"] = v["releaseType"].clone();
        }
    }
    Some(f)
}

impl Client<'_> {
    async fn page(&self, chemin: &str) -> Result<(Vec<Value>, i64), String> {
        let mut v = self.get(chemin).await?;
        let total = v["totalRecords"].as_i64().unwrap_or(0);
        match v["records"].take() {
            Value::Array(a) => Ok((a, total)),
            _ => Ok((Vec::new(), total)),
        }
    }

    pub async fn releases(&self, cible: &Cible) -> Result<Vec<Value>, String> {
        let requete = match cible {
            Cible::Titre(id) => format!("movieId={id}"),
            Cible::Saison(serie, n) => format!("seriesId={serie}&seasonNumber={n}"),
            Cible::Episodes(ids) => match ids.as_slice() {
                [id] => format!("episodeId={id}"),
                _ => return Err("un seul épisode par recherche".into()),
            },
            Cible::Manquants => return Err("cible invalide".into()),
        };
        let r = self
            .requete(Method::GET, &format!("/release?{requete}"))
            .timeout(DELAI_RECHERCHE);
        let mut liste: Vec<Value> = match self.envoyer(r).await? {
            Value::Array(a) => a.iter().filter_map(release).collect(),
            _ => return Err("/release : liste attendue".into()),
        };
        trier_releases(&mut liste);
        Ok(liste)
    }

    pub async fn recuperer(&self, guid: &str, indexeur: i64) -> Result<(), String> {
        let r = self
            .requete(Method::POST, "/release")
            .timeout(DELAI_RECHERCHE)
            .json(&json!({ "guid": guid, "indexerId": indexeur }));
        self.envoyer(r).await.map(|_| ())
    }

    pub async fn rechercher(&self, service: Service, cible: &Cible) -> Result<(), String> {
        let corps = match (service, cible) {
            (Service::Radarr, Cible::Titre(id)) => {
                json!({ "name": "MoviesSearch", "movieIds": [id] })
            }
            (Service::Radarr, Cible::Manquants) => {
                json!({ "name": "MissingMoviesSearch", "monitored": true })
            }
            (Service::Sonarr, Cible::Titre(id)) => {
                json!({ "name": "SeriesSearch", "seriesId": id })
            }
            (Service::Sonarr, Cible::Saison(id, n)) => {
                json!({ "name": "SeasonSearch", "seriesId": id, "seasonNumber": n })
            }
            (Service::Sonarr, Cible::Episodes(ids)) => {
                json!({ "name": "EpisodeSearch", "episodeIds": ids })
            }
            (Service::Sonarr, Cible::Manquants) => {
                json!({ "name": "MissingEpisodeSearch", "monitored": true })
            }
            (Service::Radarr, _) => return Err("cible invalide pour Radarr".into()),
        };
        self.commande(corps).await
    }

    async fn file_brute(&self, service: Service) -> Result<Vec<Value>, String> {
        let inconnus = match service {
            Service::Radarr => "includeUnknownMovieItems=true",
            Service::Sonarr => "includeUnknownSeriesItems=true",
        };
        Ok(self
            .page(&format!(
                "/queue?page=1&pageSize={PAGE}&{inconnus}&{}",
                service.inclusions()
            ))
            .await?
            .0)
    }

    pub async fn file(&self, service: Service) -> Result<Vec<Value>, String> {
        Ok(self
            .file_brute(service)
            .await?
            .iter()
            .filter_map(|e| element_file(service, e))
            .collect())
    }

    pub async fn retirer(&self, id: i64, liste_noire: bool, relancer: bool) -> Result<(), String> {
        let r = self
            .requete(Method::DELETE, &format!("/queue/{id}"))
            .query(&[
                ("removeFromClient", "true"),
                ("blocklist", if liste_noire { "true" } else { "false" }),
                ("skipRedownload", if relancer { "false" } else { "true" }),
            ]);
        self.envoyer(r).await.map(|_| ())
    }

    async fn telechargement(&self, service: Service, id: i64) -> Result<String, String> {
        self.file_brute(service)
            .await?
            .iter()
            .find(|e| e["id"].as_i64() == Some(id))
            .and_then(|e| texte(e, "downloadId"))
            .ok_or_else(|| "HTTP 404".to_string())
    }

    async fn import_brut(&self, download: &str) -> Result<Vec<Value>, String> {
        let r = self
            .requete(Method::GET, "/manualimport")
            .query(&[("downloadId", download), ("filterExistingFiles", "false")]);
        match self.envoyer(r).await? {
            Value::Array(a) => Ok(a),
            _ => Err("/manualimport : liste attendue".into()),
        }
    }

    pub async fn import_possible(&self, service: Service, id: i64) -> Result<Vec<Value>, String> {
        let download = self.telechargement(service, id).await?;
        Ok(self
            .import_brut(&download)
            .await?
            .iter()
            .filter_map(fichier_importable)
            .collect())
    }

    pub async fn importer(
        &self,
        service: Service,
        id: i64,
        noms: &[String],
    ) -> Result<usize, String> {
        let download = self.telechargement(service, id).await?;
        let fichiers: Vec<Value> = self
            .import_brut(&download)
            .await?
            .iter()
            .filter(|v| {
                texte(v, "relativePath")
                    .or_else(|| texte(v, "name"))
                    .is_some_and(|n| noms.contains(&n))
            })
            .filter_map(|v| fichier_a_importer(service, v, &download))
            .collect();
        if fichiers.is_empty() {
            return Ok(0);
        }
        let nombre = fichiers.len();
        self.commande(json!({ "name": "ManualImport", "importMode": "auto", "files": fichiers }))
            .await?;
        Ok(nombre)
    }

    pub async fn manquants(&self, service: Service) -> Result<(Vec<Value>, i64), String> {
        let (tri, plus) = match service {
            Service::Radarr => ("title", ""),
            Service::Sonarr => ("airDateUtc", "&includeSeries=true"),
        };
        let (liste, total) = self
            .page(&format!(
                "/wanted/missing?page=1&pageSize={PAGE}&sortKey={tri}&sortDirection=descending&monitored=true{plus}"
            ))
            .await?;
        Ok((
            liste.iter().filter_map(|v| manquant(service, v)).collect(),
            total,
        ))
    }

    pub async fn historique(
        &self,
        service: Service,
        titre: Option<i64>,
    ) -> Result<Vec<Value>, String> {
        let brut = match (service, titre) {
            (Service::Radarr, Some(id)) => {
                self.liste(&format!("/history/movie?movieId={id}&includeMovie=true"))
                    .await?
            }
            (Service::Sonarr, Some(id)) => {
                self.liste(&format!(
                    "/history/series?seriesId={id}&includeSeries=true&includeEpisode=true"
                ))
                .await?
            }
            (_, None) => {
                self.page(&format!(
                    "/history?page=1&pageSize=50&sortKey=date&sortDirection=descending&{}",
                    service.inclusions()
                ))
                .await?
                .0
            }
        };
        Ok(brut
            .iter()
            .take(PAGE)
            .map(|v| element_historique(service, v))
            .collect())
    }

    pub async fn liste_noire(&self, service: Service) -> Result<Vec<Value>, String> {
        Ok(self
            .page(&format!(
                "/blocklist?page=1&pageSize={PAGE}&sortKey=date&sortDirection=descending"
            ))
            .await?
            .0
            .iter()
            .filter_map(|v| element_liste_noire(service, v))
            .collect())
    }

    pub async fn retirer_de_la_liste_noire(&self, id: i64) -> Result<(), String> {
        self.envoyer(self.requete(Method::DELETE, &format!("/blocklist/{id}")))
            .await
            .map(|_| ())
    }

    pub async fn titres(&self, service: Service) -> Result<Vec<Value>, String> {
        Ok(self
            .liste(service.bibliotheque())
            .await?
            .iter()
            .filter_map(|v| titre(service, v))
            .collect())
    }

    pub async fn fiche(&self, service: Service, id: i64) -> Result<Value, String> {
        let brut = self
            .get(&format!("{}/{id}", service.bibliotheque()))
            .await?;
        let mut t = titre(service, &brut).ok_or_else(|| "HTTP 404".to_string())?;
        t["profiles"] = json!(
            self.liste("/qualityprofile")
                .await?
                .iter()
                .filter_map(|p| Some(json!({ "id": positif(p, "id")?, "name": texte(p, "name") })))
                .collect::<Vec<_>>()
        );
        if service == Service::Sonarr {
            t["episode_list"] = json!(
                self.liste(&format!("/episode?seriesId={id}&includeEpisodeFile=true"))
                    .await?
                    .iter()
                    .filter_map(episode_fiche)
                    .collect::<Vec<_>>()
            );
        }
        Ok(t)
    }

    pub async fn modifier(
        &self,
        service: Service,
        id: i64,
        m: &Modification,
    ) -> Result<(), String> {
        let chemin = format!("{}/{id}", service.bibliotheque());
        let mut v = self.get(&chemin).await?;
        if !v.is_object() {
            return Err("HTTP 404".into());
        }
        if let Some(s) = m.surveille {
            v["monitored"] = json!(s);
        }
        if let Some(p) = m.profil {
            let connu = self
                .liste("/qualityprofile")
                .await?
                .iter()
                .any(|x| x["id"].as_i64() == Some(p));
            if !connu {
                return Err("profil de qualité inconnu".into());
            }
            v["qualityProfileId"] = json!(p);
        }
        if service == Service::Sonarr {
            if let Some(n) = m.nouvelles_saisons {
                v["monitorNewItems"] = json!(if n { "all" } else { "none" });
            }
            if let Some(liste) = v["seasons"].as_array_mut() {
                for saison in liste {
                    let numero = saison["seasonNumber"].as_i64();
                    if let Some((_, s)) = m
                        .saisons
                        .iter()
                        .find(|(n, _)| Some(i64::from(*n)) == numero)
                    {
                        saison["monitored"] = json!(s);
                    }
                }
            }
        }
        self.envoyer(self.requete(Method::PUT, &chemin).json(&v))
            .await
            .map(|_| ())
    }

    pub async fn surveiller_episodes(&self, ids: &[i64], surveille: bool) -> Result<(), String> {
        self.envoyer(
            self.requete(Method::PUT, "/episode/monitor")
                .json(&json!({ "episodeIds": ids, "monitored": surveille })),
        )
        .await
        .map(|_| ())
    }

    pub async fn supprimer(
        &self,
        service: Service,
        id: i64,
        fichiers: bool,
        exclure: bool,
    ) -> Result<Option<i64>, String> {
        let chemin = format!("{}/{id}", service.bibliotheque());
        let tmdb = positif(&self.get(&chemin).await?, "tmdbId");
        let exclusion = match service {
            Service::Radarr => "addImportExclusion",
            Service::Sonarr => "addImportListExclusion",
        };
        let oui_non = |b: bool| if b { "true" } else { "false" };
        self.envoyer(self.requete(Method::DELETE, &chemin).query(&[
            ("deleteFiles", oui_non(fichiers)),
            (exclusion, oui_non(exclure)),
        ]))
        .await?;
        Ok(tmdb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Arr, testutil::faux};

    fn arr(url: &str) -> Arr {
        Arr {
            url: url.to_string(),
            cle: "k".into(),
            profil: None,
            racine: None,
        }
    }

    #[test]
    fn release_normalisee_et_triee() {
        let rejetee = release(
            &json!({"guid": "g1", "indexerId": 2, "title": "A", "approved": false,
            "rejections": ["Trop gros"], "customFormatScore": 500, "seeders": 90,
            "quality": {"quality": {"name": "Remux-2160p"}}, "languages": [{"name": "French"}]}),
        )
        .unwrap();
        let petite = release(
            &json!({"guid": "g2", "indexerId": 2, "title": "B", "approved": true,
            "rejections": [], "customFormatScore": 10, "seeders": 4}),
        )
        .unwrap();
        let grande = release(
            &json!({"guid": "g3", "indexerId": 3, "title": "C", "approved": true,
            "customFormatScore": 80, "seeders": 1, "fullSeason": true}),
        )
        .unwrap();
        assert_eq!(
            (
                rejetee["approved"].clone(),
                rejetee["rejections"].clone(),
                rejetee["quality"].clone(),
                rejetee["languages"].clone()
            ),
            (
                json!(false),
                json!(["Trop gros"]),
                json!("Remux-2160p"),
                json!(["French"])
            )
        );
        assert_eq!(grande["full_season"], true);
        assert!(release(&json!({"title": "sans guid", "indexerId": 1})).is_none());
        let mut liste = vec![rejetee, petite, grande];
        trier_releases(&mut liste);
        let ordre: Vec<&str> = liste.iter().map(|r| r["guid"].as_str().unwrap()).collect();
        assert_eq!(ordre, ["g3", "g2", "g1"]);
    }

    #[test]
    fn etat_et_avancement_de_la_file() {
        let e = element_file(
            Service::Sonarr,
            &json!({"id": 7, "seriesId": 3, "title": "S.S01E02", "size": 200.0, "sizeleft": 50.0,
                "status": "downloading", "downloadId": "ABC",
                "series": {"id": 3, "tmdbId": 201, "title": "S", "year": 2019},
                "episode": {"id": 40, "seasonNumber": 1, "episodeNumber": 2, "title": "Suite"}}),
        )
        .unwrap();
        assert_eq!(
            (
                e["state"].clone(),
                e["progress"].clone(),
                e["tmdb_id"].clone(),
                e["title_id"].clone(),
                e["episode"]["number"].clone(),
                e["importable"].clone()
            ),
            (
                json!("downloading"),
                json!(75.0),
                json!(201),
                json!(3),
                json!(2),
                json!(true)
            )
        );
        let bloque = json!({"id": 1, "status": "completed", "trackedDownloadStatus": "warning",
            "trackedDownloadState": "importPending",
            "statusMessages": [{"title": "f.mkv", "messages": ["Aucun fichier vidéo"]}]});
        assert_eq!(etat_file(&bloque), "blocked");
        assert_eq!(
            element_file(Service::Radarr, &bloque).unwrap()["messages"],
            json!(["Aucun fichier vidéo"])
        );
        assert_eq!(
            etat_file(&json!({"status": "completed", "trackedDownloadState": "importPending"})),
            "importing"
        );
        assert_eq!(etat_file(&json!({"status": "delay"})), "queued");
        assert_eq!(etat_file(&json!({"status": "paused"})), "paused");
    }

    #[test]
    fn evenements_d_historique() {
        for (brut, attendu) in [
            ("grabbed", "grabbed"),
            ("downloadFolderImported", "imported"),
            ("downloadFailed", "failed"),
            ("episodeFileDeleted", "deleted"),
            ("inconnu", "other"),
        ] {
            assert_eq!(evenement(&json!({ "eventType": brut })), attendu);
        }
        let h = element_historique(
            Service::Radarr,
            &json!({"eventType": "downloadFailed", "movieId": 4, "sourceTitle": "F.2020",
                "date": "2026-11-12T09:14:00Z", "data": {"message": "Aucune source"},
                "movie": {"id": 4, "tmdbId": 101, "title": "F"}}),
        );
        assert_eq!(
            (
                h["event"].clone(),
                h["reason"].clone(),
                h["tmdb_id"].clone()
            ),
            (json!("failed"), json!("Aucune source"), json!(101))
        );
    }

    #[tokio::test]
    async fn recherches_envoyees_en_commandes() {
        let s = faux(|_| (201, b"{}".to_vec())).await;
        let a = arr(&s.url);
        let http = reqwest::Client::new();
        let c = Client::new(&a, &http);
        c.rechercher(Service::Radarr, &Cible::Titre(4))
            .await
            .unwrap();
        c.rechercher(Service::Sonarr, &Cible::Saison(3, 2))
            .await
            .unwrap();
        c.rechercher(Service::Sonarr, &Cible::Episodes(vec![40, 41]))
            .await
            .unwrap();
        c.rechercher(Service::Sonarr, &Cible::Manquants)
            .await
            .unwrap();
        assert!(
            c.rechercher(Service::Radarr, &Cible::Saison(1, 1))
                .await
                .is_err()
        );
        let corps: Vec<Value> = s.requetes().iter().map(|r| r.json()).collect();
        assert_eq!(
            corps,
            vec![
                json!({"name": "MoviesSearch", "movieIds": [4]}),
                json!({"name": "SeasonSearch", "seriesId": 3, "seasonNumber": 2}),
                json!({"name": "EpisodeSearch", "episodeIds": [40, 41]}),
                json!({"name": "MissingEpisodeSearch", "monitored": true}),
            ]
        );
    }

    #[tokio::test]
    async fn release_recuperee_et_file_retiree() {
        let s = faux(|r| match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v3/release") => (
                200,
                br#"[{"guid": "g", "indexerId": 5, "title": "R", "approved": true}]"#.to_vec(),
            ),
            _ => (200, b"{}".to_vec()),
        })
        .await;
        let a = arr(&s.url);
        let http = reqwest::Client::new();
        let c = Client::new(&a, &http);
        let liste = c.releases(&Cible::Saison(3, 1)).await.unwrap();
        assert_eq!(liste[0]["guid"], "g");
        c.recuperer("g", 5).await.unwrap();
        c.retirer(9, true, true).await.unwrap();
        c.retirer(9, false, false).await.unwrap();
        let r = s.requetes();
        assert_eq!(r[0].query, "seriesId=3&seasonNumber=1");
        assert_eq!(r[1].json(), json!({"guid": "g", "indexerId": 5}));
        assert_eq!(
            (
                r[2].methode.as_str(),
                r[2].chemin.as_str(),
                r[2].query.as_str()
            ),
            (
                "DELETE",
                "/api/v3/queue/9",
                "removeFromClient=true&blocklist=true&skipRedownload=false"
            )
        );
        assert_eq!(
            r[3].query,
            "removeFromClient=true&blocklist=false&skipRedownload=true"
        );
    }

    #[tokio::test]
    async fn surveillance_des_saisons_modifiee_sans_toucher_au_reste() {
        let s = faux(|r| match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v3/series/3") => (
                200,
                br#"{"id": 3, "title": "S", "monitored": true, "qualityProfileId": 1, "path": "/tv/S",
                    "seasons": [{"seasonNumber": 1, "monitored": true}, {"seasonNumber": 2, "monitored": false}]}"#
                    .to_vec(),
            ),
            ("GET", "/api/v3/qualityprofile") => (200, br#"[{"id": 1}, {"id": 6}]"#.to_vec()),
            _ => (202, b"{}".to_vec()),
        })
        .await;
        let a = arr(&s.url);
        let http = reqwest::Client::new();
        let c = Client::new(&a, &http);
        c.modifier(
            Service::Sonarr,
            3,
            &Modification {
                surveille: None,
                profil: Some(6),
                saisons: vec![(2, true)],
                nouvelles_saisons: Some(false),
            },
        )
        .await
        .unwrap();
        let maj = s
            .requetes()
            .into_iter()
            .find(|r| r.methode == "PUT")
            .unwrap()
            .json();
        assert_eq!(
            maj,
            json!({"id": 3, "title": "S", "monitored": true, "qualityProfileId": 6, "path": "/tv/S",
                "monitorNewItems": "none",
                "seasons": [{"seasonNumber": 1, "monitored": true}, {"seasonNumber": 2, "monitored": true}]})
        );
        let refus = c
            .modifier(
                Service::Sonarr,
                3,
                &Modification {
                    profil: Some(99),
                    ..Modification::default()
                },
            )
            .await;
        assert_eq!(refus, Err("profil de qualité inconnu".to_string()));
    }

    #[tokio::test]
    async fn import_manuel_limite_aux_fichiers_du_telechargement() {
        let s = faux(|r| match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v3/queue") => (
                200,
                br#"{"records": [{"id": 9, "downloadId": "HASH"}]}"#.to_vec(),
            ),
            ("GET", "/api/v3/manualimport") => (
                200,
                br#"[{"path": "/dl/a.mkv", "relativePath": "a.mkv", "size": 5, "movie": {"id": 4, "title": "F"},
                      "quality": {"quality": {"name": "WEBDL-1080p"}}, "languages": [{"name": "French"}], "rejections": []},
                     {"path": "/dl/b.mkv", "relativePath": "b.mkv", "movie": {"id": 4}}]"#
                    .to_vec(),
            ),
            _ => (201, b"{}".to_vec()),
        })
        .await;
        let a = arr(&s.url);
        let http = reqwest::Client::new();
        let c = Client::new(&a, &http);
        let possibles = c.import_possible(Service::Radarr, 9).await.unwrap();
        assert_eq!(possibles.len(), 2);
        assert!(!possibles[0].to_string().contains("/dl/"));
        let n = c
            .importer(
                Service::Radarr,
                9,
                &["a.mkv".to_string(), "/etc/passwd".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(n, 1);
        let commande = s
            .requetes()
            .into_iter()
            .find(|r| r.chemin == "/api/v3/command")
            .unwrap()
            .json();
        assert_eq!(commande["name"], "ManualImport");
        assert_eq!(
            (
                commande["files"][0]["path"].clone(),
                commande["files"][0]["movieId"].clone(),
                commande["files"][0]["downloadId"].clone(),
                commande["files"].as_array().unwrap().len()
            ),
            (json!("/dl/a.mkv"), json!(4), json!("HASH"), 1)
        );
        assert_eq!(
            c.import_possible(Service::Radarr, 1).await,
            Err("HTTP 404".to_string())
        );
    }
}
