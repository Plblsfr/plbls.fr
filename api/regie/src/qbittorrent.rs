use axum::http::header;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::config::Qbittorrent;

#[derive(Default)]
pub struct Session(Mutex<Option<String>>);

pub struct Client<'a> {
    cfg: &'a Qbittorrent,
    http: &'a reqwest::Client,
    session: &'a Session,
}

pub type Champs<'a> = [(&'a str, &'a str)];

enum Envoi<'a> {
    Lecture(&'a Champs<'a>),
    Formulaire(&'a Champs<'a>),
    Brut(String, Vec<u8>),
}

fn multipart(limite: &str, champs: &Champs<'_>, torrent: &[u8]) -> Vec<u8> {
    let mut corps = Vec::with_capacity(torrent.len() + 1024);
    for (nom, valeur) in champs {
        corps.extend_from_slice(
            format!(
                "--{limite}\r\nContent-Disposition: form-data; name=\"{nom}\"\r\n\r\n{valeur}\r\n"
            )
            .as_bytes(),
        );
    }
    corps.extend_from_slice(
        format!("--{limite}\r\nContent-Disposition: form-data; name=\"torrents\"; filename=\"depot.torrent\"\r\nContent-Type: application/x-bittorrent\r\n\r\n")
            .as_bytes(),
    );
    corps.extend_from_slice(torrent);
    corps.extend_from_slice(format!("\r\n--{limite}--\r\n").as_bytes());
    corps
}

fn erreur(e: reqwest::Error) -> String {
    format!("injoignable ({})", commun::http::cause(&e.without_url()))
}

pub fn hash_valide(h: &str) -> bool {
    matches!(h.len(), 40 | 64) && h.bytes().all(|b| b.is_ascii_hexdigit())
}

fn torrent(t: &Value, hash: &str) -> Value {
    let texte = |k: &str| t[k].as_str().unwrap_or_default();
    let entier = |k: &str| t[k].as_i64().unwrap_or(0);
    let tags: Vec<&str> = texte("tags")
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .collect();
    json!({
        "hash": hash.to_ascii_lowercase(),
        "name": texte("name"),
        "state": texte("state"),
        "progress": t["progress"].as_f64().unwrap_or(0.0),
        "size": entier("size"),
        "download_speed": entier("dlspeed"),
        "upload_speed": entier("upspeed"),
        "eta": entier("eta"),
        "ratio": t["ratio"].as_f64().unwrap_or(0.0),
        "category": texte("category"),
        "tags": tags,
        "added_at": entier("added_on"),
        "seeds": entier("num_seeds"),
        "peers": entier("num_leechs"),
        "download_limit": entier("dl_limit").max(0),
        "upload_limit": entier("up_limit").max(0),
    })
}

pub fn instantane(v: &Value) -> Value {
    let mut torrents: Vec<Value> = v["torrents"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(h, _)| hash_valide(h))
        .map(|(h, t)| torrent(t, h))
        .collect();
    torrents.sort_by(|a, b| {
        b["added_at"]
            .as_i64()
            .cmp(&a["added_at"].as_i64())
            .then_with(|| a["hash"].as_str().cmp(&b["hash"].as_str()))
    });
    let mut categories: Vec<&str> = v["categories"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.as_str())
        .collect();
    categories.sort_unstable();
    let mut tags: Vec<&str> = v["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    tags.sort_unstable();
    let s = &v["server_state"];
    let entier = |k: &str| s[k].as_i64().unwrap_or(0);
    json!({
        "torrents": torrents,
        "categories": categories,
        "tags": tags,
        "transfer": {
            "download_speed": entier("dl_info_speed"),
            "upload_speed": entier("up_info_speed"),
            "download_limit": entier("dl_rate_limit"),
            "upload_limit": entier("up_rate_limit"),
            "alternative": s["use_alt_speed_limits"].as_bool().unwrap_or(false),
            "free_space": s["free_space_on_disk"].as_i64(),
            "connection": s["connection_status"].as_str().unwrap_or("unknown"),
        },
    })
}

impl<'a> Client<'a> {
    pub fn new(cfg: &'a Qbittorrent, http: &'a reqwest::Client, session: &'a Session) -> Self {
        Self { cfg, http, session }
    }

    async fn connecter(&self) -> Result<String, String> {
        let rep = self
            .http
            .post(format!("{}/api/v2/auth/login", self.cfg.url))
            .header(header::REFERER, &self.cfg.url)
            .form(&[
                ("username", self.cfg.utilisateur.as_str()),
                ("password", self.cfg.mot_de_passe.as_str()),
            ])
            .send()
            .await
            .map_err(erreur)?;
        let statut = rep.status();
        let cookie = rep
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter_map(|v| v.split(';').next())
            .map(str::trim)
            .find(|c| c.contains('='))
            .map(String::from);
        let corps = rep.text().await.map_err(erreur)?;
        if statut.as_u16() == 403 {
            return Err("connexion refusée (adresse bannie après trop d'échecs)".into());
        }
        if !statut.is_success() {
            return Err(format!("HTTP {}", statut.as_u16()));
        }
        if corps.trim().eq_ignore_ascii_case("fails.") {
            return Err("identifiants refusés".into());
        }
        Ok(cookie.unwrap_or_default())
    }

    async fn appeler(
        &self,
        envoi: &Envoi<'_>,
        chemin: &str,
        cookie: &str,
    ) -> Result<(u16, String), String> {
        let url = format!("{}/api/v2{chemin}", self.cfg.url);
        let mut r = match envoi {
            Envoi::Lecture(champs) => self.http.get(url).query(champs),
            Envoi::Formulaire(champs) => self.http.post(url).form(champs),
            Envoi::Brut(genre, corps) => self
                .http
                .post(url)
                .header(header::CONTENT_TYPE, genre.as_str())
                .body(corps.clone()),
        }
        .header(header::REFERER, &self.cfg.url);
        if !cookie.is_empty() {
            r = r.header(header::COOKIE, cookie);
        }
        let rep = r.send().await.map_err(erreur)?;
        let statut = rep.status().as_u16();
        Ok((statut, rep.text().await.map_err(erreur)?))
    }

    async fn executer(&self, envoi: &Envoi<'_>, chemin: &str) -> Result<(u16, String), String> {
        let mut session = self.session.0.lock().await;
        if let Some(cookie) = session.as_deref() {
            match self.appeler(envoi, chemin, cookie).await? {
                (401 | 403, _) => {}
                r => return Ok(r),
            }
        }
        *session = None;
        let cookie = self.connecter().await?;
        let r = self.appeler(envoi, chemin, &cookie).await?;
        if !matches!(r.0, 401 | 403) {
            *session = Some(cookie);
        }
        Ok(r)
    }

    async fn lire(&self, chemin: &str, champs: &Champs<'_>) -> Result<String, String> {
        match self.executer(&Envoi::Lecture(champs), chemin).await? {
            (200..=299, t) => Ok(t),
            (s, _) => Err(format!("HTTP {s}")),
        }
    }

    async fn json(&self, chemin: &str, champs: &Champs<'_>) -> Result<Value, String> {
        let t = self.lire(chemin, champs).await?;
        serde_json::from_str(&t).map_err(|_| "réponse illisible".to_string())
    }

    pub async fn ecrire(&self, chemin: &str, champs: &Champs<'_>) -> Result<u16, String> {
        match self.executer(&Envoi::Formulaire(champs), chemin).await? {
            (s @ (200..=299 | 404 | 409), _) => Ok(s),
            (s, _) => Err(format!("HTTP {s}")),
        }
    }

    async fn ecrire_ou(
        &self,
        chemin: &str,
        ancien: &str,
        champs: &Champs<'_>,
    ) -> Result<(), String> {
        let statut = match self.ecrire(chemin, champs).await? {
            404 => self.ecrire(ancien, champs).await?,
            s => s,
        };
        match statut {
            200..=299 => Ok(()),
            s => Err(format!("HTTP {s}")),
        }
    }

    async fn appliquer(&self, chemin: &str, champs: &Champs<'_>) -> Result<(), String> {
        match self.ecrire(chemin, champs).await? {
            200..=299 => Ok(()),
            409 => Err("refusé par qBittorrent".into()),
            s => Err(format!("HTTP {s}")),
        }
    }

    pub async fn version(&self) -> Result<String, String> {
        let v = self.lire("/app/version", &[]).await?;
        let v = v.trim().trim_start_matches('v');
        if v.is_empty() || v.len() > 40 {
            return Err("réponse sans version".into());
        }
        Ok(v.to_string())
    }

    pub async fn transfert(&self) -> Result<Value, String> {
        let v = self.json("/transfer/info", &[]).await?;
        Ok(json!({
            "download_speed": v["dl_info_speed"].as_i64().unwrap_or(0),
            "upload_speed": v["up_info_speed"].as_i64().unwrap_or(0),
            "connection": v["connection_status"].as_str().unwrap_or("unknown"),
        }))
    }

    pub async fn etat(&self) -> Result<Value, String> {
        Ok(instantane(
            &self.json("/sync/maindata", &[("rid", "0")]).await?,
        ))
    }

    pub async fn info(&self, hash: &str) -> Result<Option<Value>, String> {
        let v = self.json("/torrents/info", &[("hashes", hash)]).await?;
        Ok(v.as_array().and_then(|l| l.first()).cloned())
    }

    pub async fn ajouter(&self, hash: &str, torrent: &[u8], dossier: &str) -> Result<(), String> {
        if dossier.chars().any(|c| c.is_control()) {
            return Err("dossier refusé".into());
        }
        let limite = format!("regie-{hash}-{}", torrent.len());
        let champs: &Champs<'_> = &[
            ("savepath", dossier),
            ("stopped", "true"),
            ("paused", "true"),
            ("autoTMM", "false"),
            ("contentLayout", "Original"),
            ("skip_checking", "false"),
            ("tags", "regie"),
        ];
        let envoi = Envoi::Brut(
            format!("multipart/form-data; boundary={limite}"),
            multipart(&limite, champs, torrent),
        );
        match self.executer(&envoi, "/torrents/add").await? {
            (200..=299, t) if !t.trim().eq_ignore_ascii_case("fails.") => Ok(()),
            (200..=299 | 409, _) => Err("torrent refusé par qBittorrent".into()),
            (415, _) => Err("fichier .torrent refusé par qBittorrent".into()),
            (s, _) => Err(format!("HTTP {s}")),
        }
    }

    pub async fn arreter(&self, hashes: &str) -> Result<(), String> {
        self.ecrire_ou("/torrents/stop", "/torrents/pause", &[("hashes", hashes)])
            .await
    }

    pub async fn demarrer(&self, hashes: &str) -> Result<(), String> {
        self.ecrire_ou("/torrents/start", "/torrents/resume", &[("hashes", hashes)])
            .await
    }

    pub async fn reverifier(&self, hashes: &str) -> Result<(), String> {
        self.appliquer("/torrents/recheck", &[("hashes", hashes)])
            .await
    }

    pub async fn supprimer(&self, hashes: &str, fichiers: bool) -> Result<(), String> {
        let f = if fichiers { "true" } else { "false" };
        self.appliquer(
            "/torrents/delete",
            &[("hashes", hashes), ("deleteFiles", f)],
        )
        .await
    }

    pub async fn categoriser(&self, hashes: &str, categorie: &str) -> Result<(), String> {
        self.appliquer(
            "/torrents/setCategory",
            &[("hashes", hashes), ("category", categorie)],
        )
        .await
    }

    pub async fn etiqueter(&self, hashes: &str, tags: &str, ajouter: bool) -> Result<(), String> {
        let chemin = if ajouter {
            "/torrents/addTags"
        } else {
            "/torrents/removeTags"
        };
        self.appliquer(chemin, &[("hashes", hashes), ("tags", tags)])
            .await
    }

    pub async fn limiter(&self, hashes: &str, reception: i64, envoi: i64) -> Result<(), String> {
        let (r, e) = (reception.to_string(), envoi.to_string());
        self.appliquer(
            "/torrents/setDownloadLimit",
            &[("hashes", hashes), ("limit", &r)],
        )
        .await?;
        self.appliquer(
            "/torrents/setUploadLimit",
            &[("hashes", hashes), ("limit", &e)],
        )
        .await
    }

    pub async fn limiter_tout(&self, reception: i64, envoi: i64) -> Result<(), String> {
        let (r, e) = (reception.to_string(), envoi.to_string());
        self.appliquer("/transfer/setDownloadLimit", &[("limit", &r)])
            .await?;
        self.appliquer("/transfer/setUploadLimit", &[("limit", &e)])
            .await
    }

    pub async fn mode_alternatif(&self, actif: bool) -> Result<(), String> {
        let courant = self.lire("/transfer/speedLimitsMode", &[]).await?;
        if (courant.trim() == "1") == actif {
            return Ok(());
        }
        self.appliquer("/transfer/toggleSpeedLimitsMode", &[]).await
    }

    pub async fn fichiers(&self, hash: &str) -> Result<Option<Vec<Value>>, String> {
        let v = match self
            .executer(&Envoi::Lecture(&[("hash", hash)]), "/torrents/files")
            .await?
        {
            (200..=299, t) => {
                serde_json::from_str::<Value>(&t).map_err(|_| "réponse illisible".to_string())?
            }
            (404, _) => return Ok(None),
            (s, _) => return Err(format!("HTTP {s}")),
        };
        Ok(Some(
            v.as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, f)| {
                    json!({
                        "index": f["index"].as_i64().unwrap_or(i as i64),
                        "name": f["name"].as_str().unwrap_or_default(),
                        "size": f["size"].as_i64().unwrap_or(0),
                        "progress": f["progress"].as_f64().unwrap_or(0.0),
                        "priority": f["priority"].as_i64().unwrap_or(1),
                    })
                })
                .collect(),
        ))
    }

    pub async fn prioriser(&self, hash: &str, ids: &str, priorite: i64) -> Result<bool, String> {
        let p = priorite.to_string();
        match self
            .ecrire(
                "/torrents/filePrio",
                &[("hash", hash), ("id", ids), ("priority", &p)],
            )
            .await?
        {
            200..=299 => Ok(true),
            404 => Ok(false),
            409 => Err("fichier inconnu ou métadonnées pas encore reçues".into()),
            s => Err(format!("HTTP {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_de_torrent() {
        assert!(hash_valide(&"a1".repeat(20)));
        assert!(hash_valide(&"AB".repeat(32)));
        assert!(!hash_valide("all"));
        assert!(!hash_valide(&"g1".repeat(20)));
        assert!(!hash_valide(&format!(
            "{}|{}",
            "a".repeat(20),
            "b".repeat(19)
        )));
    }

    #[test]
    fn instantane_trie_et_normalise() {
        let (a, b) = ("a".repeat(40), "B".repeat(40));
        let v = json!({
            "torrents": {
                a.clone(): {"name": "Ancien", "state": "stalledUP", "progress": 1.0, "size": 10, "added_on": 1, "tags": "vf, 4k", "dl_limit": -1},
                b.clone(): {"name": "Récent", "state": "downloading", "progress": 0.5, "dlspeed": 2048, "added_on": 9, "category": "radarr", "tags": ""},
                "pas-un-hash": {"name": "x"}
            },
            "categories": {"sonarr": {}, "radarr": {}},
            "tags": ["vf", "4k"],
            "server_state": {"dl_info_speed": 2048, "up_rate_limit": 100, "use_alt_speed_limits": true, "free_space_on_disk": 5}
        });
        let i = instantane(&v);
        let t = i["torrents"].as_array().unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(
            (&t[0]["hash"], &t[0]["name"]),
            (&json!("b".repeat(40)), &json!("Récent"))
        );
        assert_eq!(t[0]["tags"], json!([]));
        assert_eq!(t[1]["tags"], json!(["vf", "4k"]));
        assert_eq!(t[1]["download_limit"], 0);
        assert_eq!(i["categories"], json!(["radarr", "sonarr"]));
        assert_eq!(i["tags"], json!(["4k", "vf"]));
        assert_eq!(
            i["transfer"],
            json!({"download_speed": 2048, "upload_speed": 0, "download_limit": 0, "upload_limit": 100,
                "alternative": true, "free_space": 5, "connection": "unknown"})
        );
    }
}
