use std::sync::{Arc, Mutex};

use crate::config::Config;
use axum::{
    Router,
    body::to_bytes,
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::{net::TcpListener, task::JoinHandle};

pub fn config() -> Config {
    config_avec(&[])
}

pub fn config_avec(extra: &[(&str, &str)]) -> Config {
    Config::load(|k| {
        if let Some((_, v)) = extra.iter().find(|(c, _)| *c == k) {
            return Some(v.to_string());
        }
        match k {
            "DATABASE_URL" => Some("postgres://factice"),
            "AEGIS_TOKEN_ESTOMPE" => Some("jeton"),
            "LIVEKIT_URL" => Some("wss://livekit.test"),
            "LIVEKIT_API_KEY" => Some("cle"),
            "LIVEKIT_API_SECRET" => Some("secret"),
            _ => None,
        }
        .map(String::from)
    })
    .expect("configuration de test")
}

#[derive(Clone)]
pub struct Requete {
    pub methode: String,
    pub chemin: String,
    pub query: String,
    pub entetes: HeaderMap,
    pub corps: Vec<u8>,
}

impl Requete {
    pub fn entete(&self, nom: &str) -> String {
        self.entetes
            .get(nom)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    }
}

pub struct Faux {
    pub url: String,
    requetes: Arc<Mutex<Vec<Requete>>>,
    tache: JoinHandle<()>,
}

impl Faux {
    pub fn requetes(&self) -> Vec<Requete> {
        self.requetes.lock().unwrap().clone()
    }

    pub fn appels(&self) -> usize {
        self.requetes.lock().unwrap().len()
    }
}

impl Drop for Faux {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

pub async fn faux(reponse: impl Fn(&Requete) -> (u16, Vec<u8>) + Send + Sync + 'static) -> Faux {
    faux_avec_entetes(move |r| {
        let (statut, corps) = reponse(r);
        (statut, Vec::new(), corps)
    })
    .await
}

pub fn permissions_connues() -> impl Iterator<Item = String> {
    crate::catalogue::CATALOGUE
        .iter()
        .map(|e| e.nom())
        .chain([crate::catalogue::ADMIN_AEGIS.to_string()])
}

pub fn est_admin(permission: &str) -> bool {
    crate::catalogue::CATALOGUE
        .iter()
        .any(|e| e.admin() && e.nom() == permission)
}

pub fn ouverte(permission: &str) -> bool {
    crate::catalogue::ouvertes().any(|e| e.nom() == permission)
}

pub fn sub_aegis(r: &Requete) -> Option<String> {
    if r.methode != "POST" || r.chemin != "/v1/effective" {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&r.corps)
        .ok()?
        .get("sub")?
        .as_str()
        .map(str::to_string)
}

pub fn reponse_droits(r: &Requete, accorde: impl Fn(&str, &str) -> bool) -> (u16, Vec<u8>) {
    let Some(sub) = sub_aegis(r) else {
        return (404, vec![]);
    };
    if !r.entete("authorization").starts_with("Bearer ") {
        return (401, vec![]);
    }
    let permissions: serde_json::Map<String, serde_json::Value> = permissions_connues()
        .filter(|p| accorde(&sub, p))
        .map(|p| (p, serde_json::json!({"allow": ["*"], "deny": []})))
        .collect();
    (
        200,
        serde_json::json!({ "sub": sub, "permissions": permissions })
            .to_string()
            .into_bytes(),
    )
}

pub async fn faux_aegis(accorde: impl Fn(&str, &str) -> bool + Send + Sync + 'static) -> Faux {
    faux(move |r| reponse_droits(r, &accorde)).await
}

pub type EntetesFaux = Vec<(&'static str, String)>;

pub async fn faux_avec_entetes(
    reponse: impl Fn(&Requete) -> (u16, EntetesFaux, Vec<u8>) + Send + Sync + 'static,
) -> Faux {
    let requetes: Arc<Mutex<Vec<Requete>>> = Arc::default();
    let enregistre = requetes.clone();
    let reponse = Arc::new(reponse);
    let app = Router::new().fallback(move |req: Request| {
        let enregistre = enregistre.clone();
        let reponse = reponse.clone();
        async move {
            let (parts, body) = req.into_parts();
            let requete = Requete {
                methode: parts.method.to_string(),
                chemin: parts.uri.path().to_string(),
                query: parts.uri.query().unwrap_or_default().to_string(),
                entetes: parts.headers,
                corps: to_bytes(body, usize::MAX)
                    .await
                    .unwrap_or_default()
                    .to_vec(),
            };
            let (statut, entetes, corps) = reponse(&requete);
            enregistre.lock().unwrap().push(requete);
            let mut rep = (StatusCode::from_u16(statut).expect("statut"), corps).into_response();
            for (nom, valeur) in entetes {
                rep.headers_mut()
                    .insert(nom, valeur.parse().expect("en-tête"));
            }
            rep
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port éphémère");
    let url = format!("http://{}", listener.local_addr().expect("adresse"));
    let tache = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    Faux {
        url,
        requetes,
        tache,
    }
}

pub async fn postgres_conteneur() -> Option<(ContainerAsync<Postgres>, String)> {
    let conteneur = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ignoré : Docker indisponible ({e})");
            return None;
        }
    };
    let hote = conteneur.get_host().await.expect("hôte du conteneur");
    let port = conteneur
        .get_host_port_ipv4(5432)
        .await
        .expect("port du conteneur");
    let url = format!("postgres://postgres:postgres@{hote}:{port}/postgres");
    Some((conteneur, url))
}

pub struct BasePostgres {
    _conteneur: Option<ContainerAsync<Postgres>>,
    suppression: Option<(String, String)>,
    annexes: std::sync::Mutex<Vec<String>>,
}

impl BasePostgres {
    pub fn supprimer_aussi(&self, nom: &str) {
        self.annexes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(nom.to_string());
    }
}

impl Drop for BasePostgres {
    fn drop(&mut self) {
        let Some((admin, nom)) = self.suppression.take() else {
            return;
        };
        let mut noms = std::mem::take(self.annexes.get_mut().unwrap_or_else(|e| e.into_inner()));
        noms.push(nom);
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async {
                use sqlx::Connection;
                if let Ok(mut c) = sqlx::PgConnection::connect(&admin).await {
                    for nom in noms {
                        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                            "DROP DATABASE IF EXISTS \"{nom}\" WITH (FORCE)"
                        )))
                        .execute(&mut c)
                        .await;
                    }
                }
            });
        })
        .join();
    }
}

pub fn url_de_base(url: &str, nom: &str) -> String {
    let (debut, requete) = match url.split_once('?') {
        Some((d, r)) => (d, format!("?{r}")),
        None => (url, String::new()),
    };
    let racine = match debut.rsplit_once('/') {
        Some((r, _)) if r.contains("://") => r,
        _ => panic!(
            "TEST_DATABASE_URL doit se terminer par /<base>, ex. postgres://u:p@hôte:5432/postgres"
        ),
    };
    format!("{racine}/{nom}{requete}")
}

static COMPTEUR_BASES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub async fn postgres() -> Option<(BasePostgres, String)> {
    let Ok(admin) = std::env::var("TEST_DATABASE_URL") else {
        let (c, url) = postgres_conteneur().await?;
        return Some((
            BasePostgres {
                _conteneur: Some(c),
                suppression: None,
                annexes: Default::default(),
            },
            url,
        ));
    };
    use sqlx::Connection;
    let nom = format!(
        "test_{}_{}",
        std::process::id(),
        COMPTEUR_BASES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let url = url_de_base(&admin, &nom);
    let mut c = sqlx::PgConnection::connect(&admin)
        .await
        .expect("connexion à TEST_DATABASE_URL");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{nom}\"")))
        .execute(&mut c)
        .await
        .expect("création de la base de test");
    Some((
        BasePostgres {
            _conteneur: None,
            suppression: Some((admin, nom)),
            annexes: Default::default(),
        },
        url,
    ))
}

pub fn conforme_openapi(
    spec: &utoipa::openapi::OpenApi,
    methode: &str,
    chemin: &str,
    statut: u16,
    corps: &serde_json::Value,
) -> Result<(), String> {
    let doc = serde_json::to_value(spec).map_err(|e| e.to_string())?;
    let reponse = &doc["paths"][chemin][methode.to_lowercase()]["responses"][statut.to_string()];
    if reponse.is_null() {
        return Err(format!(
            "{methode} {chemin} : statut {statut} non documenté"
        ));
    }
    let mut schema = reponse["content"]["application/json"]["schema"].clone();
    let Some(objet) = schema.as_object_mut() else {
        return Err(format!(
            "{methode} {chemin} {statut} : corps JSON sans schéma"
        ));
    };
    objet.insert("components".into(), doc["components"].clone());
    jsonschema::validate(&schema, corps)
        .map_err(|e| format!("{methode} {chemin} {statut} : {e} dans {corps}"))
}

#[cfg(test)]
mod tests {
    use super::postgres;
    use sqlx::{Connection, PgConnection};

    #[tokio::test]
    async fn postgres_repond_a_select_1() {
        let Some((_conteneur, url)) = postgres().await else {
            return;
        };
        let mut conn = PgConnection::connect(&url).await.unwrap();
        let un: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&mut conn)
            .await
            .unwrap();
        assert_eq!(un, 1);
    }
}
