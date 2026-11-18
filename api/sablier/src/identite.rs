use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::{HeaderMap, HeaderValue, header};
use sha2::{Digest, Sha256};

pub const COOKIE: &str = "__Secure-plbls_sid";
const TTL: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identite {
    pub sub: String,
    pub nom: String,
    pub email: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refus {
    Absente,
    Invalide,
    Indisponible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Justificatif {
    pub valeur: String,
    pub cookie: bool,
}

pub fn justificatif(entetes: &HeaderMap) -> Option<Justificatif> {
    if let Some(a) = entetes
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|a| a.starts_with("Bearer ") && a.len() > 7)
    {
        return Some(Justificatif {
            valeur: a.to_string(),
            cookie: false,
        });
    }
    entetes
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|l| l.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(nom, _)| *nom == COOKIE)
        .map(|(_, v)| v.trim_matches('"'))
        .filter(|v| !v.is_empty())
        .map(|v| Justificatif {
            valeur: v.to_string(),
            cookie: true,
        })
}

pub struct Resolveur {
    url: String,
    http: reqwest::Client,
    cache: Mutex<HashMap<[u8; 32], (Identite, Instant)>>,
}

impl Resolveur {
    pub fn new(url: &str, http: reqwest::Client) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            http,
            cache: Mutex::default(),
        }
    }

    pub async fn resoudre(&self, j: Option<&Justificatif>) -> Result<Identite, Refus> {
        let j = j.ok_or(Refus::Absente)?;
        let cle: [u8; 32] = Sha256::digest(j.valeur.as_bytes()).into();
        if let Some((id, exp)) = self.cache.lock().unwrap().get(&cle)
            && Instant::now() < *exp
        {
            return Ok(id.clone());
        }
        let (nom, valeur) = if j.cookie {
            (header::COOKIE, format!("{COOKIE}={}", j.valeur))
        } else {
            (header::AUTHORIZATION, j.valeur.clone())
        };
        let valeur = HeaderValue::from_str(&valeur).map_err(|_| Refus::Invalide)?;
        let rep = self
            .http
            .get(format!("{}/auth/me", self.url))
            .header(nom, valeur)
            .send()
            .await
            .map_err(|_| Refus::Indisponible)?;
        match rep.status().as_u16() {
            200 => {}
            401 | 403 => return Err(Refus::Invalide),
            _ => return Err(Refus::Indisponible),
        }
        let v: serde_json::Value = rep.json().await.map_err(|_| Refus::Indisponible)?;
        let texte = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let id = Identite {
            sub: texte("id"),
            nom: texte("name"),
            email: texte("email"),
        };
        if id.sub.is_empty() {
            return Err(Refus::Indisponible);
        }
        let mut c = self.cache.lock().unwrap();
        if c.len() >= CACHE_MAX {
            let maintenant = Instant::now();
            c.retain(|_, (_, e)| maintenant < *e);
            if c.len() >= CACHE_MAX {
                c.clear();
            }
        }
        c.insert(cle, (id.clone(), Instant::now() + TTL));
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn justificatif_bearer_puis_cookie() {
        let mut h = HeaderMap::new();
        assert_eq!(justificatif(&h), None);
        h.insert(
            header::COOKIE,
            format!("a=b; {COOKIE}=\"abc\"").parse().unwrap(),
        );
        assert_eq!(
            justificatif(&h),
            Some(Justificatif {
                valeur: "abc".into(),
                cookie: true
            })
        );
        h.insert(header::AUTHORIZATION, "Bearer t".parse().unwrap());
        assert_eq!(
            justificatif(&h),
            Some(Justificatif {
                valeur: "Bearer t".into(),
                cookie: false
            })
        );
        h.insert(header::AUTHORIZATION, "Basic x".parse().unwrap());
        assert!(justificatif(&h).unwrap().cookie);
    }
}
