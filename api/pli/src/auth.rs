use std::{collections::HashMap, sync::Mutex, time::Duration};

use axum::http::HeaderMap;
use serde_json::{Value, json};

const DELAI: Duration = Duration::from_secs(5);

pub fn client_http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(DELAI)
        .build()
        .expect("client HTTP")
}

pub fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn sans_slash_final(url: &str) -> &str {
    url.strip_suffix('/').unwrap_or(url)
}

fn compacte(texte: &str) -> String {
    let mut sortie = String::new();
    let mut blanc = false;
    for c in texte.chars() {
        if c.is_whitespace() {
            if !blanc {
                sortie.push(' ');
            }
            blanc = true;
        } else {
            sortie.push(c);
            blanc = false;
        }
    }
    sortie.chars().take(100).collect()
}

fn cause(e: &reqwest::Error) -> String {
    let mut source: Option<&dyn std::error::Error> = Some(e);
    let mut dernier = e.to_string();
    while let Some(s) = source {
        dernier = s.to_string();
        source = s.source();
    }
    dernier
}

pub async fn fetch_identity(
    http: &reqwest::Client,
    cookie: Option<&str>,
    gateway_url: &str,
) -> Result<Value, String> {
    let Some(cookie) = cookie.filter(|c| !c.is_empty()) else {
        return Err("aucun cookie de session reçu par pli".into());
    };
    let injoignable = |e: String| format!("gateway injoignable depuis pli ({e})");
    let res = http
        .get(format!("{}/auth/me", sans_slash_final(gateway_url)))
        .header("cookie", cookie)
        .header("user-agent", "pli.plbls.fr")
        .send()
        .await
        .map_err(|e| injoignable(cause(&e)))?;
    let statut = res.status();
    if !statut.is_success() {
        let serveur = res
            .headers()
            .get("server")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .unwrap_or("?")
            .to_string();
        let corps = compacte(&res.text().await.unwrap_or_default());
        let corps = if corps.is_empty() { "vide" } else { &corps };
        return Err(format!(
            "le gateway a répondu HTTP {} à /auth/me (serveur : {serveur}, corps : {corps})",
            statut.as_u16()
        ));
    }
    let me: Value = res.json().await.map_err(|e| injoignable(cause(&e)))?;
    if vrai(&me["id"]) {
        Ok(me)
    } else {
        Err("réponse /auth/me sans id".into())
    }
}

pub fn same_origin(entetes: &HeaderMap) -> bool {
    let lire = |nom: &str| {
        let valeurs: Vec<&str> = entetes
            .get_all(nom)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        (!valeurs.is_empty()).then(|| valeurs.join(", "))
    };
    let Some(origine) = lire("origin").filter(|o| !o.is_empty()) else {
        return false;
    };
    let Some(hote) = lire("x-forwarded-host")
        .filter(|h| !h.is_empty())
        .or_else(|| lire("host"))
    else {
        return false;
    };
    let Ok(url) = url::Url::parse(&origine) else {
        return false;
    };
    let Some(nom) = url.host_str() else {
        return false;
    };
    let attendu = match url.port() {
        Some(p) => format!("{nom}:{p}"),
        None => nom.to_string(),
    };
    attendu == hote
}

#[derive(Debug, PartialEq)]
pub struct Decision {
    pub allowed: bool,
    pub reason: Option<String>,
}

pub struct PermissionChecker {
    http: reqwest::Client,
    url: String,
    token: String,
    permission: String,
    ttl_ms: u64,
    cache: Mutex<HashMap<String, (bool, u64)>>,
}

impl PermissionChecker {
    pub fn new(http: reqwest::Client, url: &str, token: &str, permission: &str) -> Self {
        Self {
            http,
            url: sans_slash_final(url).to_string(),
            token: token.to_string(),
            permission: permission.to_string(),
            ttl_ms: 30_000,
            cache: Mutex::default(),
        }
    }

    pub async fn envoyer_manifeste(&self) -> Result<(), String> {
        let cle = self
            .permission
            .strip_prefix("pli:")
            .ok_or_else(|| format!("{} n'appartient pas à pli", self.permission))?;
        let manifeste = json!({
            "app": "pli",
            "name": "pli",
            "description": "Dépôt de fichiers chiffrés",
            "permissions": [{
                "key": cle,
                "category": "Dépôts",
                "label": "Déposer des fichiers",
                "description": "Déposer des fichiers chiffrés à partager par lien",
                "open_default": false,
            }],
        });
        let res = self
            .http
            .put(format!("{}/v1/manifest", self.url))
            .bearer_auth(&self.token)
            .json(&manifeste)
            .send()
            .await
            .map_err(|e| format!("aegis injoignable ({})", cause(&e)))?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(format!("aegis a répondu HTTP {}", res.status().as_u16()))
        }
    }

    pub async fn check(&self, user_id: &Value, maintenant: u64) -> Decision {
        let cle = user_id.to_string();
        if let Some((allowed, jusqua)) = self.cache.lock().unwrap().get(&cle).copied()
            && jusqua > maintenant
        {
            return Decision {
                allowed,
                reason: None,
            };
        }
        let refus = |reason: String| Decision {
            allowed: false,
            reason: Some(reason),
        };
        let injoignable =
            |e: &reqwest::Error| refus(format!("aegis injoignable depuis pli ({})", cause(e)));
        let corps = json!({ "sub": user_id, "permission": self.permission });
        let res = match self
            .http
            .post(format!("{}/v1/check", self.url))
            .bearer_auth(&self.token)
            .json(&corps)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return injoignable(&e),
        };
        let statut = res.status().as_u16();
        if !res.status().is_success() {
            let indice = if statut == 401 {
                " (jeton refusé : vérifier PLI_AEGIS_TOKEN et AEGIS__SERVICES__TOKENS)"
            } else {
                ""
            };
            return refus(format!("aegis a répondu HTTP {statut}{indice}"));
        }
        let corps: Value = match res.json().await {
            Ok(v) => v,
            Err(e) => return injoignable(&e),
        };
        let allowed = corps["allowed"] == Value::Bool(true);
        self.cache
            .lock()
            .unwrap()
            .insert(cle, (allowed, maintenant + self.ttl_ms));
        Decision {
            allowed,
            reason: None,
        }
    }
}
