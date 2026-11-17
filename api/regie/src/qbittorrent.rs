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

fn erreur(e: reqwest::Error) -> String {
    format!("injoignable ({})", commun::http::cause(&e.without_url()))
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

    async fn appeler(&self, chemin: &str, cookie: &str) -> Result<(u16, String), String> {
        let mut r = self
            .http
            .get(format!("{}/api/v2{chemin}", self.cfg.url))
            .header(header::REFERER, &self.cfg.url);
        if !cookie.is_empty() {
            r = r.header(header::COOKIE, cookie);
        }
        let rep = r.send().await.map_err(erreur)?;
        let statut = rep.status().as_u16();
        Ok((statut, rep.text().await.map_err(erreur)?))
    }

    async fn texte(&self, chemin: &str) -> Result<String, String> {
        let mut session = self.session.0.lock().await;
        if let Some(cookie) = session.as_deref() {
            match self.appeler(chemin, cookie).await? {
                (200..=299, t) => return Ok(t),
                (401 | 403, _) => {}
                (s, _) => return Err(format!("HTTP {s}")),
            }
        }
        *session = None;
        let cookie = self.connecter().await?;
        let (statut, t) = self.appeler(chemin, &cookie).await?;
        if !(200..=299).contains(&statut) {
            return Err(format!("HTTP {statut}"));
        }
        *session = Some(cookie);
        Ok(t)
    }

    pub async fn version(&self) -> Result<String, String> {
        let v = self.texte("/app/version").await?;
        let v = v.trim().trim_start_matches('v');
        if v.is_empty() || v.len() > 40 {
            return Err("réponse sans version".into());
        }
        Ok(v.to_string())
    }

    pub async fn transfert(&self) -> Result<Value, String> {
        let t = self.texte("/transfer/info").await?;
        let v: Value = serde_json::from_str(&t).map_err(|_| "réponse illisible".to_string())?;
        Ok(json!({
            "download_speed": v["dl_info_speed"].as_i64().unwrap_or(0),
            "upload_speed": v["up_info_speed"].as_i64().unwrap_or(0),
            "connection": v["connection_status"].as_str().unwrap_or("unknown"),
        }))
    }
}
