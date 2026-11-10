use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aws_lc_rs::encoding::AsDer;
use aws_lc_rs::rsa::{KeyPair, KeySize, PublicKeyComponents};
use aws_lc_rs::signature::KeyPair as _;
use axum::{
    Form, Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tower::ServiceExt;

use crate::AppState;
use crate::identity::Identity;
use crate::session::{self, Session, Store};
use crate::settings::{self, Settings};

pub const CLIENT: &str = "gw";
pub const AUDIENCE: &str = "api";
pub const COOKIE: &str = "sid";

struct Etat {
    issuer: String,
    cle: EncodingKey,
    jwk: Value,
    reponse: Mutex<(u16, Value)>,
    demandes: Mutex<Vec<HashMap<String, String>>>,
    revocations: AtomicUsize,
    porte: Mutex<Option<Arc<Semaphore>>>,
}

pub struct FauxKeycloak {
    etat: Arc<Etat>,
    tache: tokio::task::JoinHandle<()>,
}

impl Drop for FauxKeycloak {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

fn cle_rsa() -> (EncodingKey, Value) {
    let paire = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let der = paire.as_der().unwrap();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        STANDARD.encode(der.as_ref())
    );
    let publique = PublicKeyComponents::<Vec<u8>>::from(paire.public_key());
    let jwk = json!({
        "keys": [{
            "kty": "RSA", "kid": "k1", "alg": "RS256", "use": "sig",
            "n": URL_SAFE_NO_PAD.encode(&publique.n),
            "e": URL_SAFE_NO_PAD.encode(&publique.e),
        }]
    });
    (EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(), jwk)
}

async fn decouverte(State(e): State<Arc<Etat>>) -> Json<Value> {
    let i = &e.issuer;
    Json(json!({
        "issuer": i,
        "authorization_endpoint": format!("{i}/auth"),
        "token_endpoint": format!("{i}/token"),
        "jwks_uri": format!("{i}/certs"),
        "end_session_endpoint": format!("{i}/logout"),
        "revocation_endpoint": format!("{i}/revoke"),
    }))
}

async fn certs(State(e): State<Arc<Etat>>) -> Json<Value> {
    Json(e.jwk.clone())
}

async fn jeton(State(e): State<Arc<Etat>>, Form(f): Form<HashMap<String, String>>) -> Response {
    if f.get("grant_type").map(String::as_str) == Some("client_credentials") {
        return Json(json!({ "access_token": "admin", "expires_in": 60 })).into_response();
    }
    e.demandes.lock().unwrap().push(f);
    let porte = e.porte.lock().unwrap().clone();
    if let Some(p) = porte {
        let _ = p.acquire().await;
    }
    let (statut, corps) = e.reponse.lock().unwrap().clone();
    (StatusCode::from_u16(statut).unwrap(), Json(corps)).into_response()
}

async fn clients() -> Json<Value> {
    Json(json!([{ "clientId": "web", "defaultClientScopes": [AUDIENCE] }]))
}

async fn revoquer(State(e): State<Arc<Etat>>) -> StatusCode {
    e.revocations.fetch_add(1, Ordering::SeqCst);
    StatusCode::OK
}

impl FauxKeycloak {
    pub async fn demarrer() -> Self {
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}/realms/t", ecoute.local_addr().unwrap());
        let (cle, jwk) = cle_rsa();
        let etat = Arc::new(Etat {
            issuer,
            cle,
            jwk,
            reponse: Mutex::new((500, json!({}))),
            demandes: Mutex::default(),
            revocations: AtomicUsize::new(0),
            porte: Mutex::default(),
        });
        let app = Router::new()
            .route(
                "/realms/t/.well-known/openid-configuration",
                get(decouverte),
            )
            .route("/realms/t/certs", get(certs))
            .route("/realms/t/token", post(jeton))
            .route("/realms/t/revoke", post(revoquer))
            .route("/admin/realms/t/clients", get(clients))
            .with_state(etat.clone());
        let tache = tokio::spawn(async move {
            axum::serve(ecoute, app).await.unwrap();
        });
        Self { etat, tache }
    }

    pub fn issuer(&self) -> &str {
        &self.etat.issuer
    }

    pub fn signer(&self, aud: &str, nonce: Option<&str>, exp: u64) -> String {
        let mut h = Header::new(Algorithm::RS256);
        h.kid = Some("k1".into());
        let mut c = json!({
            "sub": "u1", "iss": self.etat.issuer, "aud": aud, "azp": "web", "exp": exp,
            "preferred_username": "alice", "email": "alice@example.org",
        });
        if let Some(n) = nonce {
            c["nonce"] = n.into();
        }
        encode(&h, &c, &self.etat.cle).unwrap()
    }

    pub fn repondre(&self, statut: u16, corps: Value) {
        *self.etat.reponse.lock().unwrap() = (statut, corps);
    }

    pub fn jetons(&self, nonce: &str, refresh: &str) -> Value {
        let exp = session::now() + 300;
        json!({
            "access_token": self.signer(AUDIENCE, None, exp),
            "id_token": self.signer(CLIENT, Some(nonce), exp),
            "refresh_token": refresh,
            "expires_in": 300,
            "refresh_expires_in": 1800,
        })
    }

    pub fn demandes(&self) -> Vec<HashMap<String, String>> {
        self.etat.demandes.lock().unwrap().clone()
    }

    pub fn revocations(&self) -> usize {
        self.etat.revocations.load(Ordering::SeqCst)
    }

    pub fn fermer_porte(&self) -> Arc<Semaphore> {
        let p = Arc::new(Semaphore::new(0));
        *self.etat.porte.lock().unwrap() = Some(p.clone());
        p
    }
}

pub async fn etat() -> Option<(AppState, FauxKeycloak)> {
    let Ok(url) = std::env::var("TEST_REDIS_URL") else {
        eprintln!("ignoré : TEST_REDIS_URL absent");
        return None;
    };
    let redis = redis::Client::open(url.as_str())
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap();
    let kc = FauxKeycloak::demarrer().await;
    let settings = Settings {
        server: settings::Server {
            bind: String::new(),
            public_url: "https://security.plbls.fr".into(),
            default_redirect: "https://plbls.fr/".into(),
            allowed_origin_suffix: "plbls.fr".into(),
            extra_origins: vec![],
            ws_max_lifetime_secs: 900,
        },
        redis: settings::Redis { url },
        oidc: settings::Oidc {
            issuer: kc.issuer().to_string(),
            internal_issuer: String::new(),
            client_id: CLIENT.into(),
            client_secret: "secret".into(),
            audience: AUDIENCE.into(),
        },
        session: settings::Session {
            cookie_name: COOKIE.into(),
            cookie_domain: String::new(),
            cookie_secure: false,
        },
        identity: settings::Identity {
            secret: String::new(),
        },
        internal: Default::default(),
        rate_limit: Default::default(),
        routes: vec![],
    };
    let state = AppState {
        oidc: Arc::new(crate::oidc::Oidc::new(settings.oidc.clone()).unwrap()),
        settings: Arc::new(settings),
        store: Store::new(redis),
        proxy_http: reqwest::Client::new(),
    };
    Some((state, kc))
}

pub fn session_de_test(access_exp: u64) -> Session {
    Session {
        identity: Identity {
            id: "u1".into(),
            name: "alice".into(),
            email: "alice@example.org".into(),
            roles: vec![],
            client: "web".into(),
        },
        access_token: "ancien".into(),
        access_exp,
        refresh_token: "r1".into(),
        id_token: Some("id1".into()),
    }
}

pub struct Reponse {
    pub statut: StatusCode,
    pub entetes: axum::http::HeaderMap,
    pub corps: String,
}

impl Reponse {
    pub fn entete(&self, nom: &str) -> String {
        self.entetes
            .get(nom)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }

    pub fn cookies(&self) -> Vec<String> {
        self.entetes
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect()
    }
}

pub async fn appeler(state: &AppState, req: Request<Body>) -> Reponse {
    let r = crate::routes::web::router()
        .with_state(state.clone())
        .oneshot(req)
        .await
        .unwrap();
    let statut = r.status();
    let entetes = r.headers().clone();
    let corps = r.into_body().collect().await.unwrap().to_bytes();
    Reponse {
        statut,
        entetes,
        corps: String::from_utf8_lossy(&corps).to_string(),
    }
}
