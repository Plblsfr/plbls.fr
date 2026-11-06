use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::json;

use crate::http::{champ, lire_corps, premier_objet};

const CACHE_TTL: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 1000;

#[derive(Debug, PartialEq, Eq)]
pub struct Indisponible;

type Horloge = Arc<dyn Fn() -> Instant + Send + Sync>;

struct Inner {
    base_url: String,
    token: String,
    permission: String,
    client: reqwest::Client,
    now: Horloge,
    cache: Mutex<HashMap<String, (bool, Instant)>>,
}

#[derive(Clone)]
pub struct Client(Arc<Inner>);

impl Client {
    pub fn new(base_url: &str, token: &str, permission: &str) -> Self {
        Self::with_now(base_url, token, permission, Arc::new(Instant::now))
    }

    pub fn with_now(base_url: &str, token: &str, permission: &str, now: Horloge) -> Self {
        Client(Arc::new(Inner {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            permission: permission.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client HTTP"),
            now,
            cache: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn allowed(&self, sub: &str) -> Result<bool, Indisponible> {
        let inner = &self.0;
        if let Some((ok, exp)) = inner.cache.lock().unwrap().get(sub)
            && (inner.now)() < *exp
        {
            return Ok(*ok);
        }
        let reponse = inner
            .client
            .post(format!("{}/v1/check", inner.base_url))
            .bearer_auth(&inner.token)
            .header("Content-Type", "application/json")
            .body(json!({ "user_id": sub, "permission": inner.permission }).to_string())
            .send()
            .await
            .map_err(|_| Indisponible)?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(Indisponible);
        }
        let corps = lire_corps(reponse).await.ok_or(Indisponible)?;
        let objet = premier_objet(&corps).ok_or(Indisponible)?;
        let ok = match champ(&objet, "allowed") {
            None | Some(serde_json::Value::Null) => false,
            Some(serde_json::Value::Bool(b)) => *b,
            Some(_) => return Err(Indisponible),
        };
        let mut c = inner.cache.lock().unwrap();
        if c.len() >= CACHE_MAX {
            c.clear();
        }
        c.insert(sub.to_string(), (ok, (inner.now)() + CACHE_TTL));
        Ok(ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::CORPS_MAX;
    use crate::testutil::faux;

    #[tokio::test]
    async fn appelle_check_et_met_en_cache() {
        let f = faux(|r| {
            let corps: serde_json::Value = serde_json::from_slice(&r.corps).unwrap_or_default();
            if r.chemin != "/v1/check"
                || r.entete("authorization") != "Bearer tok"
                || corps["permission"] != "estompe:access"
            {
                return (400, vec![]);
            }
            let ok = corps["user_id"] == "alice";
            (200, format!(r#"{{"allowed":{ok}}}"#).into_bytes())
        })
        .await;
        let t = Arc::new(Mutex::new(Instant::now()));
        let t2 = t.clone();
        let c = Client::with_now(
            &f.url,
            "tok",
            "estompe:access",
            Arc::new(move || *t2.lock().unwrap()),
        );
        assert_eq!(c.allowed("alice").await, Ok(true));
        assert_eq!(c.allowed("alice").await, Ok(true));
        assert_eq!(f.appels(), 1);
        assert_eq!(f.requetes()[0].methode, "POST");
        assert_eq!(c.allowed("bob").await, Ok(false));
        *t.lock().unwrap() += Duration::from_secs(60);
        c.allowed("alice").await.unwrap();
        assert_eq!(f.appels(), 3);
    }

    #[tokio::test]
    async fn plafond_du_cache() {
        let f = faux(|_| (200, br#"{"allowed":true}"#.to_vec())).await;
        let c = Client::new(&f.url, "tok", "p");
        for i in 0..CACHE_MAX {
            c.allowed(&i.to_string()).await.unwrap();
        }
        assert_eq!(c.0.cache.lock().unwrap().len(), CACHE_MAX);
        c.allowed("un de plus").await.unwrap();
        assert_eq!(c.0.cache.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn echoue_ferme() {
        for (statut, corps) in [
            (401, ""),
            (500, r#"{"allowed":true}"#),
            (200, "pas du json"),
            (200, r#"{"allowed":"oui"}"#),
        ] {
            let f = faux(move |_| (statut, corps.as_bytes().to_vec())).await;
            assert_eq!(
                Client::new(&f.url, "x", "p").allowed("a").await,
                Err(Indisponible),
                "{statut} {corps}"
            );
        }
        let gros = format!(r#"{{"allowed":true,"x":"{}"}}"#, "a".repeat(CORPS_MAX));
        let f = faux(move |_| (200, gros.clone().into_bytes())).await;
        assert_eq!(
            Client::new(&f.url, "x", "p").allowed("a").await,
            Err(Indisponible)
        );
        let f = faux(|_| (200, br#"{"Allowed":true}"#.to_vec())).await;
        assert_eq!(Client::new(&f.url, "x", "p").allowed("a").await, Ok(true));
        let f = faux(|_| (200, br#"{}"#.to_vec())).await;
        assert_eq!(Client::new(&f.url, "x", "p").allowed("a").await, Ok(false));
        let f = faux(|_| (200, vec![])).await;
        let url = f.url.clone();
        drop(f);
        assert_eq!(
            Client::new(&url, "x", "p").allowed("a").await,
            Err(Indisponible)
        );
    }
}
