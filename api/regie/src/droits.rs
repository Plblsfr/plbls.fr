use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::identite::Identite;

pub const APPLICATION: &str = "regie";
const TTL_MAX: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Droit {
    Admin,
}

impl Droit {
    pub const TOUS: [Droit; 1] = [Droit::Admin];

    pub fn cle(self) -> &'static str {
        match self {
            Droit::Admin => "admin",
        }
    }

    pub fn nom(self) -> String {
        format!("{APPLICATION}:{}", self.cle())
    }

    fn libelle(self) -> (&'static str, &'static str) {
        match self {
            Droit::Admin => (
                "Administrer les médias",
                "Voir l'état de Radarr, Sonarr, Lidarr, Prowlarr et qBittorrent, tester les indexeurs, retirer ou relancer un téléchargement bloqué",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Droits {
    pub admin: bool,
}

impl Droits {
    pub fn a(&self, d: Droit) -> bool {
        match d {
            Droit::Admin => self.admin,
        }
    }
}

pub fn manifeste() -> Value {
    json!({
        "app": APPLICATION,
        "name": "Régie",
        "description": "Administration des médias",
        "permissions": Droit::TOUS.iter().map(|d| {
            let (label, description) = d.libelle();
            json!({
                "key": d.cle(),
                "category": "Régie",
                "label": label,
                "description": description,
                "open_default": false,
            })
        }).collect::<Vec<_>>(),
    })
}

pub fn lire_effectifs(v: &Value) -> Droits {
    let accorde = |d: Droit| {
        v["permissions"][d.nom()].as_object().is_some_and(|o| {
            let contient = |k: &str| {
                o.get(k)
                    .and_then(Value::as_array)
                    .is_some_and(|a| a.iter().any(|x| x == "*"))
            };
            contient("allow") && !contient("deny")
        })
    };
    Droits {
        admin: accorde(Droit::Admin),
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
        let d = lire_effectifs(&v);
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
        assert_eq!(m["app"], "regie");
        let p = m["permissions"].as_array().unwrap();
        let cles: Vec<&str> = p.iter().map(|x| x["key"].as_str().unwrap()).collect();
        assert_eq!(cles, ["admin"]);
        assert!(p.iter().all(|x| x["open_default"] == false));
        assert!(p.iter().all(|x| !x["label"].as_str().unwrap().is_empty()
            && !x["description"].as_str().unwrap().is_empty()));
        assert_eq!(Droit::Admin.nom(), "regie:admin");
    }

    #[test]
    fn effectifs() {
        let accorde = |allow: Value, deny: Value| {
            lire_effectifs(&json!({"permissions": {
                "regie:admin": {"allow": allow, "deny": deny},
            }}))
            .admin
        };
        assert!(accorde(json!(["*"]), json!([])));
        assert!(!accorde(json!(["*"]), json!(["*"])));
        assert!(!accorde(json!([]), json!([])));
        let ailleurs = json!({"permissions": {"sillon:admin": {"allow": ["*"], "deny": []}}});
        assert_eq!(lire_effectifs(&ailleurs), Droits::default());
        assert_eq!(lire_effectifs(&json!({})), Droits::default());
    }
}
