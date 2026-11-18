use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::identite::Identite;

pub const APPLICATION: &str = "sablier";
pub const ENTRACTE: &str = "entracte:access";
pub const SILLON: &str = "sillon:access";
const TTL_MAX: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Droit {
    Acces,
    Admin,
}

impl Droit {
    pub const TOUS: [Droit; 2] = [Droit::Acces, Droit::Admin];

    pub fn cle(self) -> &'static str {
        match self {
            Droit::Acces => "access",
            Droit::Admin => "admin",
        }
    }

    pub fn nom(self) -> String {
        format!("{APPLICATION}:{}", self.cle())
    }

    pub fn section(self) -> &'static str {
        match self {
            Droit::Acces => "stats",
            Droit::Admin => "admin",
        }
    }

    fn libelle(self) -> (&'static str, &'static str) {
        match self {
            Droit::Acces => (
                "Accès à Sablier",
                "Voir ses statistiques, ses trophées, ses séries de jours et ses récaps, et choisir ce qui paraît sur son profil",
            ),
            Droit::Admin => (
                "Administrer Sablier",
                "Configurer les trophées : créer, modifier, activer, désactiver",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Droits {
    pub access: bool,
    pub admin: bool,
    pub entracte: bool,
    pub sillon: bool,
}

impl Droits {
    pub fn a(&self, d: Droit) -> bool {
        match d {
            Droit::Acces => self.access,
            Droit::Admin => self.admin,
        }
    }

    pub fn sections(&self) -> Vec<&'static str> {
        Droit::TOUS
            .iter()
            .filter(|d| self.a(**d))
            .map(|d| d.section())
            .collect()
    }
}

pub fn manifeste() -> Value {
    json!({
        "app": APPLICATION,
        "name": "Sablier",
        "description": "Statistiques",
        "permissions": Droit::TOUS.iter().map(|d| {
            let (label, description) = d.libelle();
            json!({
                "key": d.cle(),
                "category": "Sablier",
                "label": label,
                "description": description,
                "open_default": false,
            })
        }).collect::<Vec<_>>(),
    })
}

pub fn accordee(v: &Value, permission: &str) -> Option<bool> {
    v["permissions"][permission].as_object().map(|o| {
        let contient = |k: &str| {
            o.get(k)
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|x| x == "*"))
        };
        contient("allow") && !contient("deny")
    })
}

pub fn lire_effectifs(v: &Value) -> Droits {
    let oui = |p: &str| accordee(v, p).unwrap_or(false);
    Droits {
        access: oui(&Droit::Acces.nom()),
        admin: oui(&Droit::Admin.nom()),
        entracte: oui(ENTRACTE),
        sillon: oui(SILLON),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Injoignable;

pub struct Aegis {
    url: String,
    jeton: String,
    http: reqwest::Client,
    cache: Mutex<HashMap<String, (Droits, Instant)>>,
}

impl Aegis {
    pub fn new(url: &str, jeton: &str, http: reqwest::Client) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            jeton: jeton.to_string(),
            http,
            cache: Mutex::default(),
        }
    }

    pub async fn envoyer_manifeste(&self) -> Result<(), String> {
        commun::aegis::envoyer_manifeste(&self.http, &self.url, &self.jeton, &manifeste()).await
    }

    async fn demander(&self, sub: &str, permission: &str) -> Result<bool, Injoignable> {
        let r = self
            .http
            .post(format!("{}/v1/check", self.url))
            .bearer_auth(&self.jeton)
            .json(&json!({ "sub": sub, "permission": permission }))
            .send()
            .await
            .map_err(|_| Injoignable)?;
        if !r.status().is_success() {
            tracing::warn!("aegis : /v1/check → HTTP {}", r.status().as_u16());
            return Err(Injoignable);
        }
        let v: Value = r.json().await.map_err(|_| Injoignable)?;
        Ok(v["allowed"] == json!(true))
    }

    async fn verifier(&self, sub: &str, permission: &str) -> bool {
        self.demander(sub, permission).await.unwrap_or(false)
    }

    pub async fn acces_services(&self, sub: &str) -> Result<Droits, Injoignable> {
        let (entracte, sillon) =
            tokio::try_join!(self.demander(sub, ENTRACTE), self.demander(sub, SILLON))?;
        Ok(Droits {
            entracte,
            sillon,
            ..Droits::default()
        })
    }

    pub async fn droits(&self, id: &Identite) -> Result<Droits, Injoignable> {
        if let Some((d, exp)) = self.cache.lock().unwrap().get(&id.sub)
            && Instant::now() < *exp
        {
            return Ok(*d);
        }
        let r = self
            .http
            .post(format!("{}/v1/effective", self.url))
            .bearer_auth(&self.jeton)
            .json(&json!({ "sub": id.sub, "email": id.email, "display_name": id.nom }))
            .send()
            .await
            .map_err(|_| Injoignable)?;
        if !r.status().is_success() {
            tracing::warn!("aegis : /v1/effective → HTTP {}", r.status().as_u16());
            return Err(Injoignable);
        }
        let v: Value = r.json().await.map_err(|_| Injoignable)?;
        let mut d = lire_effectifs(&v);
        let ailleurs = |p: &'static str| {
            let connue = accordee(&v, p);
            async move {
                match connue {
                    Some(b) => b,
                    None => self.verifier(&id.sub, p).await,
                }
            }
        };
        (d.entracte, d.sillon) = tokio::join!(ailleurs(ENTRACTE), ailleurs(SILLON));
        let ttl = v["valid_until"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .and_then(|t| {
                (t.with_timezone(&chrono::Utc) - chrono::Utc::now())
                    .to_std()
                    .ok()
            })
            .map_or(TTL_MAX, |t| t.min(TTL_MAX));
        let mut c = self.cache.lock().unwrap();
        if c.len() >= CACHE_MAX {
            c.clear();
        }
        c.insert(id.sub.clone(), (d, Instant::now() + ttl));
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifeste_ferme_par_defaut() {
        let m = manifeste();
        assert_eq!(m["app"], "sablier");
        let p = m["permissions"].as_array().unwrap();
        let cles: Vec<&str> = p.iter().map(|x| x["key"].as_str().unwrap()).collect();
        assert_eq!(cles, ["access", "admin"]);
        assert!(p.iter().all(|x| x["open_default"] == false));
        assert!(p.iter().all(|x| !x["label"].as_str().unwrap().is_empty()
            && !x["description"].as_str().unwrap().is_empty()));
        let noms: Vec<String> = Droit::TOUS.iter().map(|d| d.nom()).collect();
        assert_eq!(noms, ["sablier:access", "sablier:admin"]);
    }

    #[test]
    fn effectifs() {
        let v = json!({"permissions": {
            "sablier:access": {"allow": ["*"], "deny": []},
            "sablier:admin": {"allow": ["*"], "deny": ["*"]},
            "entracte:access": {"allow": ["*"], "deny": []},
            "entracte:admin": {"allow": ["*"], "deny": []},
            "sillon:access": {"allow": [], "deny": []},
            "sillon:admin": {"allow": ["*"], "deny": []}
        }});
        assert_eq!(
            lire_effectifs(&v),
            Droits {
                access: true,
                admin: false,
                entracte: true,
                sillon: false
            }
        );
        assert_eq!(accordee(&v, SILLON), Some(false));
        assert_eq!(accordee(&v, "regie:access"), None);
        assert_eq!(lire_effectifs(&json!({})), Droits::default());
    }

    #[test]
    fn sections_selon_les_droits() {
        assert!(Droits::default().sections().is_empty());
        let d = Droits {
            access: true,
            entracte: true,
            sillon: true,
            ..Droits::default()
        };
        assert_eq!(d.sections(), ["stats"]);
        let tout = Droits {
            access: true,
            admin: true,
            ..Droits::default()
        };
        assert_eq!(tout.sections(), ["stats", "admin"]);
    }
}
