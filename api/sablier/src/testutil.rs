#![allow(dead_code)]
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::{net::TcpListener, task::JoinHandle};
use tower::ServiceExt;

use commun::evenements::{Donnees, Evenement};

use crate::{
    config::Config,
    etat::AppState,
    file::{self, Issue},
    routes,
};

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
        "sablier_test_{}_{}",
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

pub type DroitsFaux = Arc<Mutex<HashMap<String, Vec<&'static str>>>>;

pub fn droits_par_defaut() -> DroitsFaux {
    Arc::new(Mutex::new(HashMap::from([
        ("sub-personne".to_string(), vec![]),
        ("sub-alice".to_string(), vec!["access"]),
        ("sub-bruno".to_string(), vec!["access", "sillon:access"]),
        ("sub-diane".to_string(), vec!["access", "entracte:access"]),
        ("sub-admin".to_string(), vec!["access", "admin"]),
    ])))
}

pub async fn faux_gateway() -> Faux {
    faux_avec_entetes(|r| {
        let qui = r
            .entete("authorization")
            .strip_prefix("Bearer ")
            .unwrap_or_default()
            .to_string();
        let qui = if qui.is_empty() {
            r.entete("cookie")
                .split("plbls_sid=")
                .nth(1)
                .unwrap_or_default()
                .to_string()
        } else {
            qui
        };
        match qui.as_str() {
            "" | "invalide" => return (401, vec![], vec![]),
            "panne" => return (500, vec![], vec![]),
            _ => {}
        }
        (
            200,
            vec![("content-type", "application/json".to_string())],
            json!({"id": format!("sub-{qui}"), "name": qui, "email": format!("{qui}@x.fr")})
                .to_string()
                .into_bytes(),
        )
    })
    .await
}

pub type AnnuaireFaux = Arc<Mutex<Option<HashMap<String, crate::droits::Compte>>>>;

pub async fn faux_aegis(droits: DroitsFaux, annuaire: AnnuaireFaux) -> Faux {
    faux_avec_entetes(move |r| {
        if r.chemin == "/v1/manifest" {
            return (200, vec![], b"{}".to_vec());
        }
        if r.chemin == "/v1/accounts/lookup" {
            let Some(comptes) = annuaire.lock().unwrap().clone() else {
                return (404, vec![], vec![]);
            };
            let demandes: Vec<String> =
                serde_json::from_value(r.json()["subs"].clone()).unwrap_or_default();
            let connus: Vec<Value> = demandes
                .iter()
                .filter_map(|sub| comptes.get(sub).map(|c| (sub, c)))
                .map(
                    |(sub, c)| json!({"sub": sub, "display_name": c.nom, "in_keycloak": c.present}),
                )
                .collect();
            return (
                200,
                vec![("content-type", "application/json".to_string())],
                json!({ "accounts": connus }).to_string().into_bytes(),
            );
        }
        let sub = r.json()["sub"].as_str().unwrap_or_default().to_string();
        let accordees = droits
            .lock()
            .unwrap()
            .get(&sub)
            .cloned()
            .unwrap_or_default();
        let corps = if r.chemin == "/v1/check" {
            let demandee = r.json()["permission"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            json!({"allowed": accordees.iter().any(|p| *p == demandee)})
        } else {
            let perms: serde_json::Map<String, Value> = accordees
                .iter()
                .filter(|p| !p.contains(':'))
                .map(|p| (format!("sablier:{p}"), json!({"allow": ["*"], "deny": []})))
                .collect();
            json!({"sub": sub, "permissions": perms})
        };
        (
            200,
            vec![("content-type", "application/json".to_string())],
            corps.to_string().into_bytes(),
        )
    })
    .await
}

pub struct Env {
    pub _base: BasePostgres,
    pub app: Router,
    pub state: AppState,
    pub droits: DroitsFaux,
    pub annuaire: AnnuaireFaux,
    pub gateway: Faux,
    pub aegis: Faux,
}

pub async fn env_avec(regler: impl FnOnce(&mut Config)) -> Option<Env> {
    let (base, url) = postgres().await?;
    let droits = droits_par_defaut();
    let gateway = faux_gateway().await;
    let annuaire = AnnuaireFaux::default();
    let aegis = faux_aegis(droits.clone(), annuaire.clone()).await;
    let mut cfg = Config::pour_tests(&url);
    cfg.gateway_url = gateway.url.clone();
    cfg.aegis = Some((aegis.url.clone(), "jeton-aegis".into()));
    cfg.origines = vec!["https://estompe.plbls.fr".into()];
    regler(&mut cfg);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    crate::MIGRATIONS.run(&pool).await.unwrap();
    let state = AppState::new(cfg, pool);
    Some(Env {
        _base: base,
        app: routes::routeur(state.clone()),
        state,
        droits,
        annuaire,
        gateway,
        aegis,
    })
}

pub async fn env() -> Option<Env> {
    env_avec(|_| {}).await
}

pub struct Rep {
    pub statut: StatusCode,
    pub entetes: HeaderMap,
    pub corps: Vec<u8>,
}

impl Rep {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.corps).unwrap_or(Value::Null)
    }

    pub fn texte(&self) -> String {
        String::from_utf8_lossy(&self.corps).into_owned()
    }
}

pub async fn requete(app: &Router, req: axum::http::Request<Body>) -> Rep {
    let r = app.clone().oneshot(req).await.unwrap();
    let statut = r.status();
    let entetes = r.headers().clone();
    let corps = to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec();
    Rep {
        statut,
        entetes,
        corps,
    }
}

pub async fn appel_sur(
    app: &Router,
    qui: Option<&str>,
    methode: &str,
    chemin: &str,
    corps: Option<Value>,
    entetes: &[(&str, &str)],
) -> Rep {
    let mut b = axum::http::Request::builder().method(methode).uri(chemin);
    if let Some(q) = qui {
        b = b.header("authorization", format!("Bearer {q}"));
    }
    for (k, v) in entetes {
        b = b.header(*k, *v);
    }
    let corps = match corps {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    requete(app, b.body(corps).unwrap()).await
}

impl Env {
    pub async fn appel(
        &self,
        qui: Option<&str>,
        methode: &str,
        chemin: &str,
        corps: Option<Value>,
    ) -> Rep {
        appel_sur(&self.app, qui, methode, chemin, corps, &[]).await
    }

    pub async fn appel_avec(
        &self,
        qui: Option<&str>,
        methode: &str,
        chemin: &str,
        corps: Option<Value>,
        entetes: &[(&str, &str)],
    ) -> Rep {
        appel_sur(&self.app, qui, methode, chemin, corps, entetes).await
    }

    pub async fn get(&self, qui: &str, chemin: &str) -> Rep {
        self.appel(Some(qui), "GET", chemin, None).await
    }

    pub async fn recevoir(
        &self,
        id: &str,
        sub: &str,
        date: &str,
        importe: bool,
        donnees: Donnees,
    ) -> Issue {
        let mut e = Evenement::importe(id, sub, instant(date), donnees);
        e.importe = importe;
        file::traiter(self.state.pool(), &self.state.config().fuseau, &e.json())
            .await
            .unwrap()
    }
}

impl Env {
    pub async fn remplir(&self, qui: &str) {
        let sub = format!("sub-{qui}");
        let pool = self.state.pool();
        let fuseau = &self.state.config().fuseau;
        let evenements = [
            Donnees::EstompeMessage {
                serveur: Some(format!("serveur-{sub}")),
                salon: Some(format!("salon-{sub}")),
                prive: false,
            },
            Donnees::SillonEcoute {
                piste: format!("piste-{sub}"),
                secondes: 120,
                comptee: true,
            },
            Donnees::EntracteLecture {
                titre: format!("titre-{sub}"),
                genre: commun::evenements::Genre::Film,
                serie: None,
                secondes: 600,
            },
        ];
        for (i, donnees) in evenements.into_iter().enumerate() {
            let id = format!("{sub}-evt-{i}");
            let issue = self
                .recevoir(&id, &sub, "2026-03-10T10:00:00Z", false, donnees)
                .await;
            assert_eq!(issue, Issue::Applique);
        }
        let identite = crate::identite::Identite {
            sub: sub.clone(),
            nom: qui.to_string(),
            email: format!("{qui}@x.fr"),
        };
        for date in ["2026-03-10T10:00:00Z", "2026-03-10T10:00:20Z"] {
            let battement = crate::battement::Battement {
                platform: crate::battement::Plateforme::Web,
                section: crate::battement::Section::Sablier,
            };
            let intervalle = Duration::from_secs(30);
            crate::battement::enregistrer(pool, fuseau, intervalle, &sub, battement, instant(date))
                .await
                .unwrap();
            crate::personnes::noter(pool, &identite, instant(date))
                .await
                .unwrap();
        }
        let aujourd_hui = self.state.aujourd_hui().await.unwrap();
        crate::trophees::evaluer(pool, &sub, aujourd_hui)
            .await
            .unwrap();
        for sql in [
            "INSERT INTO recaps (sub, genre, periode, contenu) VALUES ($1, 'mois', '2026-02',
                jsonb_build_object('kind', 'month', 'period', '2026-02', 'marque', 'recap-' || $1))",
            "INSERT INTO partages_stats (sub, stat) VALUES ($1, 'messaging.messages')",
            "INSERT INTO partages_trophees (sub, trophee)
                SELECT $1, id FROM trophees WHERE cle = 'premier_jour'",
        ] {
            sqlx::query(sql).bind(&sub).execute(pool).await.unwrap();
        }
    }

    pub async fn lignes_de(&self, sub: &str) -> Vec<(&'static str, i64)> {
        let mut lignes = Vec::new();
        for table in crate::effacement::tables_personnelles() {
            let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM {table} WHERE sub = $1"
            )))
            .bind(sub)
            .fetch_one(self.state.pool())
            .await
            .unwrap();
            lignes.push((*table, n));
        }
        lignes
    }

    pub async fn lectures(&self, qui: &str) -> Vec<(String, StatusCode, Value)> {
        let mut lectures = Vec::new();
        for (route, _) in routes::table() {
            let lecture = route.methode == "GET"
                && route.garde == crate::garde::Garde::Droit(crate::droits::Droit::Acces)
                && route.chemin != "/views";
            if !lecture {
                continue;
            }
            let chemin = route
                .chemin
                .replace("{sub}", &format!("sub-{qui}"))
                .replace("{id}", "1");
            let r = self.get(qui, &format!("{chemin}?period=all")).await;
            let mut corps = r.json();
            if chemin == "/export" {
                let export = corps.as_object_mut().expect("export");
                export.remove("exported_at");
                export.remove("views");
            }
            lectures.push((chemin, r.statut, corps));
        }
        lectures
    }
}

pub async fn attendre<F: Future<Output = bool>>(mut condition: impl FnMut() -> F, quoi: &str) {
    let limite = Instant::now() + Duration::from_secs(20);
    while !condition().await {
        assert!(Instant::now() < limite, "délai dépassé : {quoi}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub fn instant(texte: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(texte)
        .expect("instant rfc3339")
        .with_timezone(&chrono::Utc)
}

pub fn jour(texte: &str) -> chrono::NaiveDate {
    texte.parse().expect("jour AAAA-MM-JJ")
}
