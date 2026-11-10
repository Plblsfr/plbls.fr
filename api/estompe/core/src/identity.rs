use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::http::{HeaderMap, HeaderValue, header};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};

use crate::http::{champ, champ_texte, lire_corps, premier_objet};

pub const COOKIE_NAME: &str = "__Secure-plbls_sid";
const CACHE_TTL: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub sub: String,
    pub username: String,
    pub email: String,
    pub roles: Vec<String>,
    pub via_cookie: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Erreur {
    Invalid,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cred {
    pub value: String,
    pub cookie: bool,
}

pub fn cred_of(headers: &HeaderMap) -> Option<Cred> {
    if let Some(a) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && a.starts_with("Bearer ")
    {
        return Some(Cred {
            value: a.to_string(),
            cookie: false,
        });
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|ligne| ligne.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(nom, _)| *nom == COOKIE_NAME)
        .map(|(_, v)| {
            v.strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(v)
        })
        .filter(|v| !v.is_empty())
        .map(|v| Cred {
            value: v.to_string(),
            cookie: true,
        })
}

type Horloge = Arc<dyn Fn() -> Instant + Send + Sync>;

struct Inner {
    base_url: String,
    client: reqwest::Client,
    now: Horloge,
    cache: Mutex<HashMap<[u8; 32], (Identity, Instant)>>,
}

#[derive(Clone)]
pub struct Resolver(Arc<Inner>);

impl Resolver {
    pub fn new(gateway_url: &str) -> Self {
        Self::with_now(gateway_url, Arc::new(Instant::now))
    }

    pub fn with_now(gateway_url: &str, now: Horloge) -> Self {
        Resolver(Arc::new(Inner {
            base_url: gateway_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client HTTP"),
            now,
            cache: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn resolve(&self, cred: Option<&Cred>) -> Result<Identity, Erreur> {
        self.resolve_avec(cred, true).await
    }

    pub async fn revalidate(&self, cred: Option<&Cred>) -> Result<Identity, Erreur> {
        self.resolve_avec(cred, false).await
    }

    async fn resolve_avec(&self, cred: Option<&Cred>, cache: bool) -> Result<Identity, Erreur> {
        let cred = cred
            .filter(|c| !c.value.is_empty())
            .ok_or(Erreur::Invalid)?;
        let cle: [u8; 32] = Sha256::digest(cred.value.as_bytes()).into();
        let inner = &self.0;
        if cache {
            let c = inner.cache.lock().unwrap();
            if let Some((id, exp)) = c.get(&cle)
                && (inner.now)() < *exp
            {
                return Ok(id.clone());
            }
        }
        let requete = inner.client.get(format!("{}/auth/me", inner.base_url));
        let valeur = if cred.cookie {
            format!("{COOKIE_NAME}={}", cred.value)
        } else {
            cred.value.clone()
        };
        let entete = if cred.cookie {
            header::COOKIE
        } else {
            header::AUTHORIZATION
        };
        let valeur = HeaderValue::from_str(&valeur).map_err(|_| Erreur::Unavailable)?;
        let reponse = requete
            .header(entete, valeur)
            .send()
            .await
            .map_err(|_| Erreur::Unavailable)?;
        match reponse.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(Erreur::Invalid),
            StatusCode::OK => {}
            _ => return Err(Erreur::Unavailable),
        }
        let corps = lire_corps(reponse).await.ok_or(Erreur::Unavailable)?;
        let objet = premier_objet(&corps).ok_or(Erreur::Unavailable)?;
        let texte = |nom| champ_texte(&objet, nom).ok_or(Erreur::Unavailable);
        let id = Identity {
            sub: texte("id")?,
            username: texte("name")?,
            email: texte("email")?,
            roles: match champ(&objet, "roles") {
                Some(serde_json::Value::Array(r)) => r
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            },
            via_cookie: cred.cookie,
        };
        if id.sub.is_empty() {
            return Err(Erreur::Unavailable);
        }
        let mut c = inner.cache.lock().unwrap();
        if c.len() >= CACHE_MAX {
            let maintenant = (inner.now)();
            c.retain(|_, (_, exp)| maintenant < *exp);
            if c.len() >= CACHE_MAX {
                c.clear();
            }
        }
        c.insert(cle, (id.clone(), (inner.now)() + CACHE_TTL));
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Requete, faux};

    fn gateway(r: &Requete) -> (u16, Vec<u8>) {
        let bearer = r.entete("authorization") == "Bearer ok";
        let cookie = r.entete("cookie") == format!("{COOKIE_NAME}=good");
        if r.chemin != "/auth/me" || !(bearer || cookie) {
            return (401, vec![]);
        }
        (
            200,
            br#"{"id":"u1","name":"alice","email":"a@x.fr","roles":["user","permission-admin",3]}"#
                .to_vec(),
        )
    }

    fn bearer(v: &str) -> Cred {
        Cred {
            value: format!("Bearer {v}"),
            cookie: false,
        }
    }

    fn cookie(v: &str) -> Cred {
        Cred {
            value: v.to_string(),
            cookie: true,
        }
    }

    fn horloge() -> (Horloge, Arc<Mutex<Instant>>) {
        let t = Arc::new(Mutex::new(Instant::now()));
        let t2 = t.clone();
        (Arc::new(move || *t2.lock().unwrap()), t)
    }

    #[tokio::test]
    async fn resolve_bearer_et_cookie() {
        let g = faux(gateway).await;
        let v = Resolver::new(&g.url);
        let id = v.resolve(Some(&bearer("ok"))).await.unwrap();
        assert_eq!(
            id,
            Identity {
                sub: "u1".into(),
                username: "alice".into(),
                email: "a@x.fr".into(),
                roles: vec!["user".into(), "permission-admin".into()],
                via_cookie: false
            }
        );
        assert_eq!(g.requetes()[0].entete("authorization"), "Bearer ok");
        let id = v.resolve(Some(&cookie("good"))).await.unwrap();
        assert!(id.via_cookie);
        assert_eq!(
            g.requetes()[1].entete("cookie"),
            format!("{COOKIE_NAME}=good")
        );
        assert_eq!(v.resolve(Some(&bearer("faux"))).await, Err(Erreur::Invalid));
        let avant = g.appels();
        assert_eq!(v.resolve(None).await, Err(Erreur::Invalid));
        assert_eq!(g.appels(), avant);
    }

    #[tokio::test]
    async fn statuts_et_corps_inattendus_sont_indisponibles() {
        for (statut, corps) in [
            (500, "".to_string()),
            (502, "".to_string()),
            (200, "pas du json".to_string()),
            (200, "[]".to_string()),
            (200, r#"{"id":""}"#.to_string()),
            (200, r#"{"name":"x"}"#.to_string()),
            (200, r#"{"id":12}"#.to_string()),
            (
                200,
                format!(r#"{{"id":"u1","name":"{}"}}"#, "a".repeat(70_000)),
            ),
        ] {
            let g = faux(move |_| (statut, corps.clone().into_bytes())).await;
            let v = Resolver::new(&g.url);
            assert_eq!(
                v.resolve(Some(&bearer("ok"))).await,
                Err(Erreur::Unavailable),
                "{statut}"
            );
        }
        for statut in [401, 403] {
            let g = faux(move |_| (statut, vec![])).await;
            assert_eq!(
                Resolver::new(&g.url).resolve(Some(&bearer("ok"))).await,
                Err(Erreur::Invalid)
            );
        }
    }

    #[tokio::test]
    async fn json_lu_comme_le_go() {
        let g = faux(|_| (200, br#"{"ID":"u1","Name":null} {"autre":1}"#.to_vec())).await;
        let id = Resolver::new(&g.url)
            .resolve(Some(&bearer("ok")))
            .await
            .unwrap();
        assert_eq!((id.sub.as_str(), id.username.as_str()), ("u1", ""));
    }

    #[tokio::test]
    async fn cache_trente_secondes() {
        let g = faux(gateway).await;
        let (now, t) = horloge();
        let v = Resolver::with_now(&g.url, now);
        v.resolve(Some(&bearer("ok"))).await.unwrap();
        v.resolve(Some(&bearer("ok"))).await.unwrap();
        assert_eq!(g.appels(), 1);
        v.revalidate(Some(&bearer("ok"))).await.unwrap();
        assert_eq!(g.appels(), 2);
        *t.lock().unwrap() += CACHE_TTL + Duration::from_secs(1);
        v.resolve(Some(&bearer("ok"))).await.unwrap();
        assert_eq!(g.appels(), 3);
        v.resolve(Some(&bearer("faux"))).await.unwrap_err();
        v.resolve(Some(&bearer("faux"))).await.unwrap_err();
        assert_eq!(g.appels(), 5);
    }

    #[tokio::test]
    async fn plafond_du_cache() {
        let g = faux(|_| (200, br#"{"id":"u1"}"#.to_vec())).await;
        let (now, t) = horloge();
        let v = Resolver::with_now(&g.url, now);
        for i in 0..CACHE_MAX {
            v.resolve(Some(&bearer(&i.to_string()))).await.unwrap();
        }
        assert_eq!(v.0.cache.lock().unwrap().len(), CACHE_MAX);
        *t.lock().unwrap() += CACHE_TTL + Duration::from_secs(1);
        v.resolve(Some(&bearer("nouveau"))).await.unwrap();
        assert_eq!(v.0.cache.lock().unwrap().len(), 1);
        for i in 0..CACHE_MAX {
            v.resolve(Some(&bearer(&format!("n{i}")))).await.unwrap();
        }
        assert_eq!(v.0.cache.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn gateway_arrete() {
        let g = faux(gateway).await;
        let url = g.url.clone();
        drop(g);
        let v = Resolver::new(&url);
        assert_eq!(
            v.resolve(Some(&bearer("ok"))).await,
            Err(Erreur::Unavailable)
        );
    }

    #[test]
    fn justificatif_du_client() {
        let mut h = HeaderMap::new();
        assert_eq!(cred_of(&h), None);
        h.insert(header::AUTHORIZATION, "Basic x".parse().unwrap());
        assert_eq!(cred_of(&h), None);
        h.insert(
            header::COOKIE,
            format!("a=b; {COOKIE_NAME}=\"abc\"; c=d").parse().unwrap(),
        );
        assert_eq!(cred_of(&h), Some(cookie("abc")));
        h.insert(header::AUTHORIZATION, "Bearer tok".parse().unwrap());
        assert_eq!(cred_of(&h), Some(bearer("tok")));
        let mut h = HeaderMap::new();
        h.append(
            header::COOKIE,
            format!("x=1; {COOKIE_NAME}=premier").parse().unwrap(),
        );
        h.append(
            header::COOKIE,
            format!("{COOKIE_NAME}=second").parse().unwrap(),
        );
        assert_eq!(cred_of(&h), Some(cookie("premier")));
        let mut h = HeaderMap::new();
        h.append(header::COOKIE, format!("{COOKIE_NAME}=").parse().unwrap());
        h.append(
            header::COOKIE,
            format!("{COOKIE_NAME}=second").parse().unwrap(),
        );
        assert_eq!(cred_of(&h), None);
    }
}
