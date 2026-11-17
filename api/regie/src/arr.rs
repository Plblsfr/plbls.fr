use serde_json::{Value, json};

use crate::config::Arr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Radarr,
    Sonarr,
    Lidarr,
    Prowlarr,
}

impl Service {
    pub const BIBLIOTHEQUES: [Service; 3] = [Service::Radarr, Service::Sonarr, Service::Lidarr];

    pub fn cle(self) -> &'static str {
        match self {
            Service::Radarr => "radarr",
            Service::Sonarr => "sonarr",
            Service::Lidarr => "lidarr",
            Service::Prowlarr => "prowlarr",
        }
    }

    pub fn nom(self) -> &'static str {
        match self {
            Service::Radarr => "Radarr",
            Service::Sonarr => "Sonarr",
            Service::Lidarr => "Lidarr",
            Service::Prowlarr => "Prowlarr",
        }
    }

    pub fn bibliotheque(cle: &str) -> Option<Service> {
        Self::BIBLIOTHEQUES.into_iter().find(|s| s.cle() == cle)
    }

    fn api(self) -> &'static str {
        match self {
            Service::Radarr | Service::Sonarr => "/api/v3",
            Service::Lidarr | Service::Prowlarr => "/api/v1",
        }
    }

    fn inconnus(self) -> &'static str {
        match self {
            Service::Radarr => "includeUnknownMovieItems",
            Service::Sonarr => "includeUnknownSeriesItems",
            Service::Lidarr | Service::Prowlarr => "includeUnknownArtistItems",
        }
    }
}

pub struct Client<'a> {
    service: Service,
    arr: &'a Arr,
    http: &'a reqwest::Client,
}

fn erreur(e: reqwest::Error) -> String {
    format!("injoignable ({})", commun::http::cause(&e.without_url()))
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v[cle]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

fn messages_de(v: &Value) -> Vec<String> {
    let mut m: Vec<String> = texte(v, "errorMessage").into_iter().collect();
    for s in v["statusMessages"].as_array().into_iter().flatten() {
        let details: Vec<String> = s["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str())
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect();
        if details.is_empty() {
            m.extend(texte(s, "title"));
        } else {
            m.extend(details);
        }
    }
    m.dedup();
    m
}

fn bloque(v: &Value) -> Option<&'static str> {
    let suivi = v["trackedDownloadStatus"].as_str().unwrap_or_default();
    let statut = v["status"].as_str().unwrap_or_default();
    if suivi.eq_ignore_ascii_case("error") || statut.eq_ignore_ascii_case("failed") {
        Some("error")
    } else if suivi.eq_ignore_ascii_case("warning")
        || statut.eq_ignore_ascii_case("warning")
        || texte(v, "errorMessage").is_some()
    {
        Some("warning")
    } else {
        None
    }
}

impl<'a> Client<'a> {
    pub fn new(service: Service, arr: &'a Arr, http: &'a reqwest::Client) -> Self {
        Self { service, arr, http }
    }

    fn requete(&self, methode: reqwest::Method, chemin: &str) -> reqwest::RequestBuilder {
        self.http
            .request(
                methode,
                format!("{}{}{chemin}", self.arr.url, self.service.api()),
            )
            .header("X-Api-Key", &self.arr.cle)
    }

    async fn envoyer(&self, r: reqwest::RequestBuilder) -> Result<(u16, Value), String> {
        let rep = r.send().await.map_err(erreur)?;
        let statut = rep.status().as_u16();
        let octets = rep.bytes().await.map_err(erreur)?;
        Ok((
            statut,
            serde_json::from_slice(&octets).unwrap_or(Value::Null),
        ))
    }

    async fn get(&self, chemin: &str) -> Result<Value, String> {
        match self
            .envoyer(self.requete(reqwest::Method::GET, chemin))
            .await?
        {
            (200..=299, v) => Ok(v),
            (401 | 403, _) => Err("clé d'API refusée".into()),
            (s, _) => Err(format!("HTTP {s}")),
        }
    }

    async fn liste(&self, chemin: &str) -> Result<Vec<Value>, String> {
        match self.get(chemin).await? {
            Value::Array(a) => Ok(a),
            _ => Err(format!("{chemin} : liste attendue")),
        }
    }

    pub async fn version(&self) -> Result<String, String> {
        let v = self.get("/system/status").await?;
        texte(&v, "version").ok_or_else(|| "réponse sans version".into())
    }

    pub async fn alertes(&self) -> Result<Vec<Value>, String> {
        Ok(self
            .liste("/health")
            .await?
            .iter()
            .filter_map(|a| {
                let niveau = a["type"].as_str()?.to_ascii_lowercase();
                (niveau != "ok").then(|| {
                    json!({
                        "level": niveau,
                        "source": a["source"].as_str().unwrap_or_default(),
                        "message": a["message"].as_str().unwrap_or_default(),
                    })
                })
            })
            .collect())
    }

    pub async fn dossiers(&self) -> Result<Vec<Value>, String> {
        Ok(self
            .liste("/rootfolder")
            .await?
            .iter()
            .filter_map(|d| {
                Some(json!({
                    "service": self.service.cle(),
                    "path": texte(d, "path")?,
                    "free": d["freeSpace"].as_i64(),
                    "accessible": d["accessible"].as_bool().unwrap_or(true),
                }))
            })
            .collect())
    }

    pub async fn bloques(&self) -> Result<Vec<Value>, String> {
        let v = self
            .get(&format!(
                "/queue?page=1&pageSize=500&{}=true",
                self.service.inconnus()
            ))
            .await?;
        Ok(v["records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let niveau = bloque(e)?;
                Some(json!({
                    "service": self.service.cle(),
                    "id": e["id"].as_i64()?,
                    "title": e["title"].as_str().unwrap_or_default(),
                    "level": niveau,
                    "state": texte(e, "trackedDownloadState").or_else(|| texte(e, "status")),
                    "messages": messages_de(e),
                }))
            })
            .collect())
    }

    pub async fn retirer(&self, id: i64, relancer: bool) -> Result<bool, String> {
        let r = self
            .requete(reqwest::Method::DELETE, &format!("/queue/{id}"))
            .query(&[
                ("removeFromClient", "true"),
                ("blocklist", if relancer { "true" } else { "false" }),
                ("skipRedownload", "false"),
            ]);
        match self.envoyer(r).await? {
            (200..=299, _) => Ok(true),
            (404, _) => Ok(false),
            (401 | 403, _) => Err("clé d'API refusée".into()),
            (s, _) => Err(format!("HTTP {s}")),
        }
    }

    pub async fn indexeurs(&self) -> Result<Vec<Value>, String> {
        let etats = self.liste("/indexerstatus").await?;
        Ok(self
            .liste("/indexer")
            .await?
            .iter()
            .filter_map(|i| {
                let id = i["id"].as_i64()?;
                let etat = etats.iter().find(|e| e["indexerId"].as_i64() == Some(id));
                let jusqu_a = etat.and_then(|e| texte(e, "disabledTill"));
                let echec = etat.and_then(|e| texte(e, "mostRecentFailure"));
                Some(json!({
                    "id": id,
                    "name": i["name"].as_str().unwrap_or_default(),
                    "enabled": i["enable"].as_bool().unwrap_or(false),
                    "protocol": texte(i, "protocol"),
                    "privacy": texte(i, "privacy"),
                    "failing": etat.is_some(),
                    "disabled_until": jusqu_a,
                    "last_failure": echec,
                }))
            })
            .collect())
    }

    pub async fn tester_indexeur(&self, id: i64) -> Result<Option<Result<(), String>>, String> {
        let indexeur = match self
            .envoyer(self.requete(reqwest::Method::GET, &format!("/indexer/{id}")))
            .await?
        {
            (200..=299, v) if v.is_object() => v,
            (404, _) => return Ok(None),
            (401 | 403, _) => return Err("clé d'API refusée".into()),
            (s, _) => return Err(format!("HTTP {s}")),
        };
        let r = self
            .requete(reqwest::Method::POST, "/indexer/test")
            .json(&indexeur);
        match self.envoyer(r).await? {
            (200..=299, _) => Ok(Some(Ok(()))),
            (400, v) => {
                let raisons: Vec<&str> = v
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x["errorMessage"].as_str())
                    .collect();
                Ok(Some(Err(if raisons.is_empty() {
                    "test refusé par Prowlarr".to_string()
                } else {
                    raisons.join(" ; ")
                })))
            }
            (401 | 403, _) => Err("clé d'API refusée".into()),
            (s, _) => Err(format!("HTTP {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_bloque_et_ses_messages() {
        let sain = json!({"status": "downloading", "trackedDownloadStatus": "ok"});
        assert_eq!(bloque(&sain), None);
        let avert = json!({"status": "completed", "trackedDownloadStatus": "warning",
        "statusMessages": [
            {"title": "film.mkv", "messages": ["Aucun fichier vidéo", " "]},
            {"title": "Titre seul", "messages": []}
        ]});
        assert_eq!(bloque(&avert), Some("warning"));
        assert_eq!(messages_de(&avert), ["Aucun fichier vidéo", "Titre seul"]);
        let echec = json!({"status": "failed", "errorMessage": "Le téléchargement a échoué"});
        assert_eq!(bloque(&echec), Some("error"));
        assert_eq!(messages_de(&echec), ["Le téléchargement a échoué"]);
        let suivi = json!({"status": "completed", "trackedDownloadStatus": "Error"});
        assert_eq!(bloque(&suivi), Some("error"));
        let message = json!({"status": "queued", "errorMessage": "Indisponible"});
        assert_eq!(bloque(&message), Some("warning"));
    }

    #[test]
    fn bibliotheque_par_cle() {
        assert_eq!(Service::bibliotheque("sonarr"), Some(Service::Sonarr));
        assert_eq!(Service::bibliotheque("prowlarr"), None);
        assert_eq!(Service::bibliotheque("../radarr"), None);
    }
}
