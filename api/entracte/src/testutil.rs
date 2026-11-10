#![allow(dead_code)]
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

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

#[derive(Clone, Debug)]
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

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.corps).unwrap_or_default()
    }
}

pub type EntetesFaux = Vec<(&'static str, String)>;

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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let tache = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    Faux {
        url,
        requetes,
        tache,
    }
}

pub async fn faux(reponse: impl Fn(&Requete) -> (u16, Vec<u8>) + Send + Sync + 'static) -> Faux {
    faux_avec_entetes(move |r| {
        let (s, c) = reponse(r);
        (s, Vec::new(), c)
    })
    .await
}

pub struct BasePostgres {
    _conteneur: Option<ContainerAsync<Postgres>>,
    suppression: Option<(String, String)>,
}

impl Drop for BasePostgres {
    fn drop(&mut self) {
        let Some((admin, nom)) = self.suppression.take() else {
            return;
        };
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
                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                        "DROP DATABASE IF EXISTS \"{nom}\" WITH (FORCE)"
                    )))
                    .execute(&mut c)
                    .await;
                }
            });
        })
        .join();
    }
}

static COMPTEUR: AtomicUsize = AtomicUsize::new(0);

pub async fn postgres() -> Option<(BasePostgres, String)> {
    let Ok(admin) = std::env::var("TEST_DATABASE_URL") else {
        let conteneur = match Postgres::default().with_tag("17-alpine").start().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("ignoré : TEST_DATABASE_URL absent et Docker indisponible ({e})");
                return None;
            }
        };
        let hote = conteneur.get_host().await.unwrap();
        let port = conteneur.get_host_port_ipv4(5432).await.unwrap();
        return Some((
            BasePostgres {
                _conteneur: Some(conteneur),
                suppression: None,
            },
            format!("postgres://postgres:postgres@{hote}:{port}/postgres"),
        ));
    };
    use sqlx::Connection;
    let nom = format!(
        "entracte_test_{}_{}",
        std::process::id(),
        COMPTEUR.fetch_add(1, Ordering::Relaxed)
    );
    let (debut, requete) = match admin.split_once('?') {
        Some((d, r)) => (d.to_string(), format!("?{r}")),
        None => (admin.clone(), String::new()),
    };
    let racine = debut
        .rsplit_once('/')
        .map(|(r, _)| r.to_string())
        .expect("TEST_DATABASE_URL en /<base>");
    let url = format!("{racine}/{nom}{requete}");
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
        },
        url,
    ))
}
