use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use rand::Rng;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    auth::{PermissionChecker, client_http, fetch_identity, same_origin},
    config::{Config, TTLS},
    db,
    limit::{RateLimiter, client_ip},
};

pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'self' https://security.plbls.fr; frame-ancestors 'none'";

const SECURITE: [(&str, &str); 4] = [
    ("content-security-policy", CSP),
    ("referrer-policy", "no-referrer"),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
];

struct Inner {
    cfg: Config,
    db: PgPool,
    http: reqwest::Client,
    permission: Option<PermissionChecker>,
    lecture: Mutex<RateLimiter>,
    debut: Instant,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub fn new(cfg: Config, db: PgPool) -> Self {
        let http = client_http();
        let permission = match (&cfg.aegis, &cfg.permission) {
            (Some((url, token)), _) => Some(PermissionChecker::aegis(
                http.clone(),
                url,
                token,
                &cfg.deposit_permission,
            )),
            (None, Some((url, token))) => Some(PermissionChecker::new(
                http.clone(),
                url,
                token,
                &cfg.deposit_permission,
            )),
            (None, None) => None,
        };
        Self(Arc::new(Inner {
            cfg,
            db,
            http,
            permission,
            lecture: Mutex::new(RateLimiter::new(60, 60_000)),
            debut: Instant::now(),
        }))
    }

    pub fn demarrer_aegis(&self) {
        let etat = self.clone();
        let Some(p) = &self.0.permission else { return };
        if p.source() != "aegis" {
            return;
        }
        tokio::spawn(async move {
            let mut t = tokio::time::interval(std::time::Duration::from_secs(600));
            loop {
                t.tick().await;
                if let Some(p) = &etat.0.permission
                    && let Err(e) = p.envoyer_manifeste().await
                {
                    tracing::warn!("manifeste non envoyé à aegis : {e}");
                }
            }
        });
    }

    fn maintenant(&self) -> u64 {
        u64::try_from(self.0.debut.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new().fallback(traite).with_state(state)
}

fn avec_securite(mut rep: Response) -> Response {
    for (nom, valeur) in SECURITE {
        rep.headers_mut().insert(
            HeaderName::from_static(nom),
            HeaderValue::from_static(valeur),
        );
    }
    rep
}

fn json_rep(statut: u16, corps: &impl Serialize) -> Response {
    let corps = serde_json::to_vec(corps).unwrap_or_default();
    let mut rep = (
        StatusCode::from_u16(statut).unwrap_or(StatusCode::OK),
        corps,
    )
        .into_response();
    let h = rep.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    avec_securite(rep)
}

fn introuvable() -> Response {
    json_rep(404, &json!({ "error": "introuvable" }))
}

enum Erreur {
    TropGros,
    Interne(String),
}

impl From<sqlx::Error> for Erreur {
    fn from(e: sqlx::Error) -> Self {
        Self::Interne(e.to_string())
    }
}

impl IntoResponse for Erreur {
    fn into_response(self) -> Response {
        match self {
            Self::TropGros => json_rep(413, &json!({ "error": "fichier trop gros" })),
            Self::Interne(e) => {
                tracing::error!("{e}");
                json_rep(500, &json!({ "error": "erreur interne" }))
            }
        }
    }
}

async fn traite(State(state): State<AppState>, req: Request) -> Response {
    let chemin = req.uri().path().to_string();
    if chemin.starts_with("/api/") {
        api(&state, req, &chemin)
            .await
            .unwrap_or_else(IntoResponse::into_response)
    } else if req.method() == Method::GET {
        statique(&state.0.cfg.public_dir, &chemin).await
    } else {
        StatusCode::METHOD_NOT_ALLOWED.into_response()
    }
}

fn cookie(entetes: &HeaderMap) -> Option<String> {
    let valeurs: Vec<&str> = entetes
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    (!valeurs.is_empty()).then(|| valeurs.join("; "))
}

fn texte(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        autre => autre.to_string(),
    }
}

async fn auth_depot(
    state: &AppState,
    methode: &Method,
    entetes: &HeaderMap,
) -> Result<Value, Box<Response>> {
    let s = &state.0;
    let Some(permission) = &s.permission else {
        return Err(Box::new(json_rep(
            503,
            &json!({ "error": "dépôt non configuré (AEGIS_URL et PLI_AEGIS_TOKEN, ou PERMISSION_URL et PERMISSION_SERVICE_TOKEN)" }),
        )));
    };
    let user = match fetch_identity(
        &s.http,
        cookie(entetes).as_deref(),
        &s.cfg.gateway_internal_url,
    )
    .await
    {
        Ok(u) => u,
        Err(reason) => {
            tracing::warn!("dépôt 401 : {reason}");
            return Err(Box::new(json_rep(
                401,
                &json!({ "error": "non authentifié", "reason": reason }),
            )));
        }
    };
    if methode != Method::GET && !same_origin(entetes) {
        return Err(Box::new(json_rep(
            403,
            &json!({ "error": "origine refusée" }),
        )));
    }
    let decision = permission.check(&user["id"], state.maintenant()).await;
    if !decision.allowed {
        let perm = &s.cfg.deposit_permission;
        tracing::warn!(
            "dépôt 403 : compte {} {}",
            texte(&user["id"]),
            decision
                .reason
                .clone()
                .unwrap_or_else(|| format!("pas de permission {perm}"))
        );
        let mut corps = json!({ "error": format!("permission {perm} requise") });
        if let Some(r) = decision.reason {
            corps["reason"] = Value::String(r);
        }
        corps["user_id"] = user["id"].clone();
        return Err(Box::new(json_rep(403, &corps)));
    }
    Ok(user)
}

fn parametre(uri: &axum::http::Uri, nom: &str) -> Option<String> {
    url::form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
        .find(|(k, _)| k == nom)
        .map(|(_, v)| v.into_owned())
}

fn duree(brut: Option<String>) -> Option<u32> {
    let v = brut.unwrap_or_default();
    let v = v.trim();
    let n: f64 = if v.is_empty() { 0.0 } else { v.parse().ok()? };
    TTLS.into_iter().find(|t| f64::from(*t) == n)
}

async fn corps(body: Body, entetes: &HeaderMap, max: usize) -> Result<Vec<u8>, Erreur> {
    let annonce = entetes
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    if annonce.is_some_and(|n| n > max as u64) {
        return Err(Erreur::TropGros);
    }
    to_bytes(body, max)
        .await
        .map(|b| b.to_vec())
        .map_err(|_| Erreur::TropGros)
}

fn identifiant_valide(id: &str) -> bool {
    id.len() == 22
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigPublique<'a> {
    gateway_url: &'a str,
    max_bytes: usize,
    ttls: [u32; 4],
    permission: &'a str,
}

async fn api(state: &AppState, req: Request, chemin: &str) -> Result<Response, Erreur> {
    let s = &state.0;
    let (parts, body) = req.into_parts();
    let methode = &parts.method;
    let entetes = &parts.headers;
    let get = methode == Method::GET;

    if get && chemin == "/api/config" {
        return Ok(json_rep(
            200,
            &ConfigPublique {
                gateway_url: &s.cfg.gateway_url,
                max_bytes: s.cfg.max_bytes,
                ttls: TTLS,
                permission: &s.cfg.deposit_permission,
            },
        ));
    }

    if get && chemin == "/api/me" {
        let user = match auth_depot(state, methode, entetes).await {
            Ok(u) => u,
            Err(r) => return Ok(*r),
        };
        let mut corps = json!({ "id": user["id"] });
        if let Some(nom) = user.get("name") {
            corps["name"] = nom.clone();
        }
        return Ok(json_rep(200, &corps));
    }
    if chemin == "/api/shares" && (get || methode == Method::POST) {
        let user = match auth_depot(state, methode, entetes).await {
            Ok(u) => u,
            Err(r) => return Ok(*r),
        };
        let owner = texte(&user["id"]);
        if get {
            return Ok(json_rep(200, &db::list_shares(&s.db, &owner).await?));
        }
        let Some(ttl) = duree(parametre(&parts.uri, "ttl")) else {
            return Ok(json_rep(400, &json!({ "error": "durée invalide" })));
        };
        if db::list_shares(&s.db, &owner).await?.len() >= s.cfg.max_pending {
            let max = s.cfg.max_pending;
            return Ok(json_rep(
                429,
                &json!({ "error": format!("{max} partages en attente au maximum : supprimez-en") }),
            ));
        }
        let data = corps(body, entetes, s.cfg.max_bytes).await?;
        if data.len() < 30 || data[0] != 1 {
            return Ok(json_rep(
                400,
                &json!({ "error": "contenu chiffré invalide" }),
            ));
        }
        let mut octets = [0u8; 16];
        rand::rng().fill_bytes(&mut octets);
        let id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(octets);
        let expires_at = db::insert_share(&s.db, &id, &owner, &data, ttl).await?;
        return Ok(json_rep(
            201,
            &db::Depot {
                id,
                expires_at,
                size: data.len(),
            },
        ));
    }
    if let Some(id) = chemin
        .strip_prefix("/api/shares/")
        .filter(|r| !r.is_empty() && !r.contains('/'))
        && methode == Method::DELETE
    {
        let user = match auth_depot(state, methode, entetes).await {
            Ok(u) => u,
            Err(r) => return Ok(*r),
        };
        let supprime = db::delete_share(&s.db, id, &texte(&user["id"])).await?;
        return Ok(json_rep(
            if supprime { 200 } else { 404 },
            &json!({ "ok": true }),
        ));
    }

    if let Some(reste) = chemin.strip_prefix("/api/s/") {
        let (id, consomme) = match reste.strip_suffix("/consume") {
            Some(id) => (id, true),
            None => (reste, false),
        };
        if !id.is_empty() && !id.contains('/') {
            let socket = parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0);
            let ip = client_ip(entetes, socket);
            if !s.lecture.lock().unwrap().check(&ip, state.maintenant()) {
                return Ok(json_rep(
                    429,
                    &json!({ "error": "trop de requêtes, réessayez dans une minute" }),
                ));
            }
            if !identifiant_valide(id) {
                return Ok(introuvable());
            }
            if get && !consomme {
                return Ok(match db::share_meta(&s.db, id).await? {
                    Some(meta) => json_rep(200, &meta),
                    None => introuvable(),
                });
            }
            if methode == Method::POST && consomme {
                let Some(data) = db::consume_share(&s.db, id).await? else {
                    return Ok(introuvable());
                };
                let mut rep = (StatusCode::OK, data).into_response();
                let h = rep.headers_mut();
                h.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                );
                h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                return Ok(avec_securite(rep));
            }
        }
    }
    Ok(introuvable())
}

fn type_mime(fichier: &Path) -> &'static str {
    match fichier.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn segment_point(seg: &str) -> Option<u8> {
    match seg.to_ascii_lowercase().as_str() {
        "." | "%2e" => Some(1),
        ".." | ".%2e" | "%2e." | "%2e%2e" => Some(2),
        _ => None,
    }
}

fn resout(base: &Path, chemin: &str) -> PathBuf {
    let mut segments: Vec<&str> = Vec::new();
    for seg in chemin.split('/').filter(|s| !s.is_empty()) {
        match segment_point(seg) {
            Some(1) => {}
            Some(_) => {
                segments.pop();
            }
            None => segments.push(seg),
        }
    }
    segments.iter().fold(base.to_path_buf(), |p, s| p.join(s))
}

async fn statique(base: &Path, chemin: &str) -> Response {
    let chemin = if chemin == "/" {
        "/index.html"
    } else if chemin
        .strip_prefix("/s/")
        .is_some_and(|id| !id.is_empty() && !id.contains('/'))
    {
        "/s.html"
    } else {
        chemin
    };
    let fichier = resout(base, chemin);
    match tokio::fs::read(&fichier).await {
        Ok(data) => {
            let mut rep = (StatusCode::OK, data).into_response();
            let h = rep.headers_mut();
            h.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(type_mime(&fichier)),
            );
            h.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, max-age=0, must-revalidate"),
            );
            avec_securite(rep)
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Introuvable",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_contenus_dans_public() {
        let base = Path::new("/srv/public");
        assert_eq!(
            resout(base, "/a/../../etc/passwd"),
            Path::new("/srv/public/etc/passwd")
        );
        assert_eq!(resout(base, "/%2e%2e/x"), Path::new("/srv/public/x"));
        assert_eq!(
            resout(base, "/..%2fserver/index.js"),
            Path::new("/srv/public/..%2fserver/index.js")
        );
        assert_eq!(
            resout(base, "/vendor/./a.css"),
            Path::new("/srv/public/vendor/a.css")
        );
    }

    #[test]
    fn duree_comme_number_js() {
        assert_eq!(duree(Some("3600".into())), Some(3600));
        assert_eq!(duree(Some(" 86400 ".into())), Some(86400));
        assert_eq!(duree(Some("3600.0".into())), Some(3600));
        assert_eq!(duree(Some("5".into())), None);
        assert_eq!(duree(Some("abc".into())), None);
        assert_eq!(duree(None), None);
    }

    #[test]
    fn identifiants() {
        assert!(identifiant_valide("AAAAAAAAAAAAAAAAAAAA_-"));
        assert!(!identifiant_valide("nimportequoi"));
        assert!(!identifiant_valide("AAAAAAAAAAAAAAAAAAAA_é"));
    }
}
