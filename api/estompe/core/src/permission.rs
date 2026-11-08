use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;

use crate::http::{champ, lire_corps, premier_objet};

const CACHE_TTL: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 1000;

pub const ADMIN: &str = "estompe:admin";
pub const FILMS: &str = "estompe:movies";
pub const ADMIN_PERMISSION: &str = "permission:admin";
pub const COMPTES: &str = "permission:users.read";

#[derive(Debug, PartialEq, Eq)]
pub struct Indisponible;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Droits(Arc<BTreeSet<String>>);

impl Droits {
    pub fn de<I: IntoIterator<Item = S>, S: Into<String>>(noms: I) -> Self {
        Droits(Arc::new(noms.into_iter().map(Into::into).collect()))
    }

    pub fn a(&self, nom: &str) -> bool {
        self.0.contains(nom)
    }
}

type Horloge = Arc<dyn Fn() -> Instant + Send + Sync>;

struct Inner {
    base_url: String,
    token: String,
    client: reqwest::Client,
    now: Horloge,
    cache: Mutex<HashMap<String, (Droits, Instant)>>,
}

#[derive(Clone)]
pub struct Client(Arc<Inner>);

impl Client {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self::with_now(base_url, token, Arc::new(Instant::now))
    }

    pub fn with_now(base_url: &str, token: &str, now: Horloge) -> Self {
        Client(Arc::new(Inner {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client HTTP"),
            now,
            cache: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn droits(&self, sub: &str) -> Result<Droits, Indisponible> {
        let inner = &self.0;
        if let Some((d, exp)) = inner.cache.lock().unwrap().get(sub)
            && (inner.now)() < *exp
        {
            return Ok(d.clone());
        }
        let reponse = inner
            .client
            .get(format!(
                "{}/v1/users/{}/permissions",
                inner.base_url,
                utf8_percent_encode(sub, NON_ALPHANUMERIC)
            ))
            .bearer_auth(&inner.token)
            .send()
            .await
            .map_err(|_| Indisponible)?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(Indisponible);
        }
        let corps = lire_corps(reponse).await.ok_or(Indisponible)?;
        let objet = premier_objet(&corps).ok_or(Indisponible)?;
        let droits = match champ(&objet, "permissions") {
            None | Some(Value::Null) => Droits::default(),
            Some(Value::Array(noms)) => Droits::de(
                noms.iter()
                    .map(|n| n.as_str().map(str::to_string).ok_or(Indisponible))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Some(_) => return Err(Indisponible),
        };
        let mut c = inner.cache.lock().unwrap();
        if c.len() >= CACHE_MAX {
            c.clear();
        }
        c.insert(sub.to_string(), (droits.clone(), (inner.now)() + CACHE_TTL));
        Ok(droits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::CORPS_MAX;
    use crate::testutil::{faux, faux_permission};

    #[tokio::test]
    async fn lit_les_droits_du_compte_et_met_en_cache() {
        let f = faux_permission(|sub, p| sub == "alice" && p != ADMIN).await;
        let t = Arc::new(Mutex::new(Instant::now()));
        let t2 = t.clone();
        let c = Client::with_now(&f.url, "jeton", Arc::new(move || *t2.lock().unwrap()));
        let d = c.droits("alice").await.unwrap();
        assert!(d.a("estompe:access") && d.a(FILMS) && !d.a(ADMIN));
        c.droits("alice").await.unwrap();
        assert_eq!(f.appels(), 1);
        let r = &f.requetes()[0];
        assert_eq!(
            (r.methode.as_str(), r.chemin.as_str()),
            ("GET", "/v1/users/alice/permissions")
        );
        assert_eq!(r.entete("authorization"), "Bearer jeton");
        assert!(!c.droits("bob").await.unwrap().a("estompe:access"));
        *t.lock().unwrap() += Duration::from_secs(60);
        c.droits("alice").await.unwrap();
        assert_eq!(f.appels(), 3);
    }

    #[tokio::test]
    async fn sub_encode_dans_le_chemin() {
        let f = faux(|_| (200, br#"{"permissions":[]}"#.to_vec())).await;
        Client::new(&f.url, "x").droits("a/b c").await.unwrap();
        assert_eq!(f.requetes()[0].chemin, "/v1/users/a%2Fb%20c/permissions");
    }

    #[tokio::test]
    async fn plafond_du_cache() {
        let f = faux(|_| (200, br#"{"permissions":["a:b"]}"#.to_vec())).await;
        let c = Client::new(&f.url, "tok");
        for i in 0..CACHE_MAX {
            c.droits(&i.to_string()).await.unwrap();
        }
        assert_eq!(c.0.cache.lock().unwrap().len(), CACHE_MAX);
        c.droits("un de plus").await.unwrap();
        assert_eq!(c.0.cache.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn echoue_ferme() {
        for (statut, corps) in [
            (401, ""),
            (500, r#"{"permissions":["estompe:access"]}"#),
            (200, "pas du json"),
            (200, r#"{"permissions":"estompe:access"}"#),
            (200, r#"{"permissions":["estompe:access",1]}"#),
        ] {
            let f = faux(move |_| (statut, corps.as_bytes().to_vec())).await;
            assert_eq!(
                Client::new(&f.url, "x").droits("a").await,
                Err(Indisponible),
                "{statut} {corps}"
            );
        }
        let gros = format!(r#"{{"permissions":["{}"]}}"#, "a".repeat(CORPS_MAX));
        let f = faux(move |_| (200, gros.clone().into_bytes())).await;
        assert_eq!(
            Client::new(&f.url, "x").droits("a").await,
            Err(Indisponible)
        );
        let f = faux(|_| (200, br#"{"Permissions":["x:y"]}"#.to_vec())).await;
        assert!(Client::new(&f.url, "x").droits("a").await.unwrap().a("x:y"));
        let f = faux(|_| (200, br#"{}"#.to_vec())).await;
        assert_eq!(
            Client::new(&f.url, "x").droits("a").await,
            Ok(Droits::default())
        );
        let f = faux(|_| (200, vec![])).await;
        let url = f.url.clone();
        drop(f);
        assert_eq!(Client::new(&url, "x").droits("a").await, Err(Indisponible));
    }
}
