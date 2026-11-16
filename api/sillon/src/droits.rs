use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::identite::Identite;

pub const APPLICATION: &str = "sillon";
const TTL_MAX: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Droit {
    Acces,
    Ajout,
    Admin,
}

impl Droit {
    pub const TOUS: [Droit; 3] = [Droit::Acces, Droit::Ajout, Droit::Admin];

    pub fn cle(self) -> &'static str {
        match self {
            Droit::Acces => "access",
            Droit::Ajout => "add",
            Droit::Admin => "admin",
        }
    }

    pub fn nom(self) -> String {
        format!("{APPLICATION}:{}", self.cle())
    }

    pub fn section(self) -> &'static str {
        match self {
            Droit::Acces => "library",
            Droit::Ajout => "add",
            Droit::Admin => "admin",
        }
    }

    fn libelle(self) -> (&'static str, &'static str) {
        match self {
            Droit::Acces => (
                "Accès à Sillon",
                "Écouter la bibliothèque, chercher, gérer ses playlists, ses likes, sa file et son historique",
            ),
            Droit::Ajout => (
                "Ajouter de la musique",
                "Chercher un album ou un artiste absent et l'ajouter ; découverte automatique",
            ),
            Droit::Admin => (
                "Administrer Sillon",
                "Voir l'occupation du disque, les échecs, l'état des synchronisations et de l'analyse ; supprimer",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Droits {
    pub access: bool,
    pub add: bool,
    pub admin: bool,
}

impl Droits {
    pub fn a(&self, d: Droit) -> bool {
        match d {
            Droit::Acces => self.access,
            Droit::Ajout => self.add,
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
        "name": "Sillon",
        "description": "Musique",
        "permissions": Droit::TOUS.iter().map(|d| {
            let (label, description) = d.libelle();
            json!({
                "key": d.cle(),
                "category": "Sillon",
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
        access: accorde(Droit::Acces),
        add: accorde(Droit::Ajout),
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
        assert_eq!(m["app"], "sillon");
        let p = m["permissions"].as_array().unwrap();
        let cles: Vec<&str> = p.iter().map(|x| x["key"].as_str().unwrap()).collect();
        assert_eq!(cles, ["access", "add", "admin"]);
        assert!(p.iter().all(|x| x["open_default"] == false));
        assert!(p.iter().all(|x| !x["label"].as_str().unwrap().is_empty()
            && !x["description"].as_str().unwrap().is_empty()));
        let noms: Vec<String> = Droit::TOUS.iter().map(|d| d.nom()).collect();
        assert_eq!(noms, ["sillon:access", "sillon:add", "sillon:admin"]);
    }

    #[test]
    fn effectifs() {
        let v = json!({"permissions": {
            "sillon:access": {"allow": ["*"], "deny": []},
            "sillon:add": {"allow": ["*"], "deny": ["*"]},
            "sillon:admin": {"allow": [], "deny": []},
            "entracte:admin": {"allow": ["*"], "deny": []}
        }});
        assert_eq!(
            lire_effectifs(&v),
            Droits {
                access: true,
                add: false,
                admin: false
            }
        );
        assert_eq!(lire_effectifs(&json!({})), Droits::default());
    }

    #[test]
    fn sections_selon_les_droits() {
        assert!(Droits::default().sections().is_empty());
        let d = Droits {
            access: true,
            add: false,
            admin: true,
        };
        assert_eq!(d.sections(), ["library", "admin"]);
        let tout = Droits {
            access: true,
            add: true,
            admin: true,
        };
        assert_eq!(tout.sections(), ["library", "add", "admin"]);
    }
}
