use std::net::SocketAddr;

use axum::{
    Json, Router,
    extract::{ConnectInfo, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::AppState;
use crate::auth::{self, AuthError};
use crate::origin::safe_redirect;
use crate::session::{self, PendingLogin, Session};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/register", get(register))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/session", get(session_status))
}

/// Limite de débit par IP sur le début et la fin du flow de login. L'IP est celle que le reverse proxy
/// de confiance ajoute en dernier dans `X-Forwarded-For`, sinon l'adresse du pair.
pub async fn rate_limit(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let max = state.settings.rate_limit.auth_per_minute;
    if max > 0
        && matches!(
            req.uri().path(),
            "/auth/login" | "/auth/register" | "/auth/callback"
        )
    {
        let ip = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| peer.ip().to_string());
        match state.store.hit(&format!("auth:{ip}"), max, 60).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!("rate limit /auth dépassé pour {ip}");
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(header::RETRY_AFTER, "60")],
                    "trop de tentatives",
                )
                    .into_response();
            }
            // Redis indisponible : on laisse passer, le login échouera de toute façon plus loin
            Err(e) => tracing::error!("rate limit redis: {e}"),
        }
    }
    next.run(req).await
}

fn callback_uri(state: &AppState) -> String {
    format!(
        "{}/auth/callback",
        state.settings.server.public_url.trim_end_matches('/')
    )
}

fn server_error(what: &str, e: impl std::fmt::Display) -> Response {
    tracing::error!("{what}: {e}");
    StatusCode::BAD_GATEWAY.into_response()
}

#[derive(Deserialize)]
struct LoginQuery {
    redirect: Option<String>,
}

async fn login(State(state): State<AppState>, Query(q): Query<LoginQuery>) -> Response {
    start_login(state, q, false).await
}

/// Même flow que le login, mais sur la page d'inscription Keycloak ; le callback est identique.
async fn register(State(state): State<AppState>, Query(q): Query<LoginQuery>) -> Response {
    start_login(state, q, true).await
}

async fn start_login(state: AppState, q: LoginQuery, register: bool) -> Response {
    let redirect = safe_redirect(&state.settings.server, q.redirect.as_deref());
    let login_state = session::random_token(32);
    let nonce = session::random_token(32);
    let verifier = session::random_token(48);
    let challenge = session::pkce_challenge(&verifier);

    let url = match state
        .oidc
        .authorization_url(
            &callback_uri(&state),
            &login_state,
            &nonce,
            &challenge,
            register,
        )
        .await
    {
        Ok(u) => u,
        Err(e) => return server_error("discovery Keycloak", e),
    };
    if let Err(e) = state
        .store
        .put_login(
            &login_state,
            &PendingLogin {
                nonce,
                verifier,
                redirect,
            },
        )
        .await
    {
        return server_error("redis", e);
    }
    let mut resp = Redirect::to(&url).into_response();
    resp.headers_mut()
        .append(header::SET_COOKIE, login_cookie(&state, &login_state, 600));
    resp
}

/// Lie le login au navigateur qui l'a démarré (anti login-CSRF) : le callback exige ce cookie.
/// Cookie propre à l'hôte du gateway, limité à `/auth`, `Lax` pour survivre à la redirection depuis Keycloak.
fn login_cookie_name(state: &AppState) -> String {
    format!("{}_login", state.settings.session.cookie_name)
}

fn login_cookie(state: &AppState, value: &str, max_age: u64) -> HeaderValue {
    let secure = if state.settings.session.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{}={value}; Path=/auth; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}",
        login_cookie_name(state)
    ))
    .expect("cookie valide")
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let mut resp = callback_inner(&state, &headers, q).await;
    resp.headers_mut()
        .append(header::SET_COOKIE, login_cookie(&state, "", 0));
    resp
}

async fn callback_inner(state: &AppState, headers: &HeaderMap, q: CallbackQuery) -> Response {
    if let Some(err) = q.error {
        tracing::info!("login refusé par Keycloak: {err}");
        return (StatusCode::BAD_REQUEST, "connexion refusée").into_response();
    }
    let (Some(code), Some(login_state)) = (q.code, q.state) else {
        return (StatusCode::BAD_REQUEST, "paramètres manquants").into_response();
    };
    // le login doit avoir été démarré par ce navigateur, sinon l'URL de callback vient peut-être d'un tiers
    if auth::cookie_value(headers, &login_cookie_name(state)).as_deref()
        != Some(login_state.as_str())
    {
        tracing::warn!("callback sans le cookie de login : login CSRF possible");
        return (
            StatusCode::BAD_REQUEST,
            "login non démarré dans ce navigateur",
        )
            .into_response();
    }
    let pending = match state.store.take_login(&login_state).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (StatusCode::BAD_REQUEST, "login expiré ou déjà utilisé").into_response();
        }
        Err(e) => return server_error("redis", e),
    };

    let tokens = match state
        .oidc
        .exchange_code(&code, &pending.verifier, &callback_uri(state))
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("échange du code refusé: {e}");
            return (StatusCode::BAD_GATEWAY, "échange du code impossible").into_response();
        }
    };
    let (Some(id_token), Some(refresh_token)) =
        (tokens.id_token.clone(), tokens.refresh_token.clone())
    else {
        return server_error("réponse token", "id_token ou refresh_token absent");
    };
    if let Err(e) = state.oidc.verify_id_token(&id_token, &pending.nonce).await {
        tracing::warn!("ID token refusé: {e}");
        return (StatusCode::UNAUTHORIZED, "ID token invalide").into_response();
    }
    let claims = match state.oidc.verify_access_token(&tokens.access_token).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("access token refusé: {e}");
            return (StatusCode::UNAUTHORIZED, "access token invalide").into_response();
        }
    };

    let sess = Session {
        identity: claims.identity(),
        access_token: tokens.access_token,
        access_exp: claims.exp,
        refresh_token,
        id_token: Some(id_token),
    };
    let ttl = tokens.refresh_expires_in.filter(|t| *t > 0).unwrap_or(1800);
    let sid = session::random_token(32);
    if let Err(e) = state.store.put_session(&sid, &sess, ttl).await {
        return server_error("redis", e);
    }
    tracing::info!(user = %sess.identity.id, client = %sess.identity.client, "connexion");

    let mut resp = Redirect::to(&pending.redirect).into_response();
    resp.headers_mut()
        .append(header::SET_COOKIE, set_cookie(state, &sid, ttl));
    resp
}

fn cookie_attrs(state: &AppState) -> String {
    let s = &state.settings.session;
    let domain = if s.cookie_domain.is_empty() {
        String::new()
    } else {
        format!("; Domain={}", s.cookie_domain)
    };
    format!(
        "; Path=/{domain}; HttpOnly; SameSite=Lax{}",
        if s.cookie_secure { "; Secure" } else { "" }
    )
}

fn set_cookie(state: &AppState, sid: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}={sid}{}; Max-Age={max_age}",
        state.settings.session.cookie_name,
        cookie_attrs(state)
    ))
    .expect("cookie valide")
}

fn clear_cookie(state: &AppState) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}={}; Max-Age=0",
        state.settings.session.cookie_name,
        cookie_attrs(state)
    ))
    .expect("cookie valide")
}

fn auth_error(e: AuthError) -> Response {
    match e {
        AuthError::Unauthenticated => StatusCode::UNAUTHORIZED.into_response(),
        AuthError::Unavailable => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn logout(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    Query(q): Query<LoginQuery>,
) -> Response {
    let redirect = safe_redirect(&state.settings.server, q.redirect.as_deref());
    let Some(sid) = auth::cookie_value(&headers, &state.settings.session.cookie_name) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !auth::csrf_ok(&state, &headers, &method, true) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let session = match state.store.get_session(&sid).await {
        Ok(s) => s,
        Err(e) => return server_error("redis", e),
    };
    if let Some(s) = &session {
        state.oidc.revoke(&s.refresh_token).await;
        tracing::info!(user = %s.identity.id, "déconnexion");
    }
    if let Err(e) = state.store.delete_session(&sid).await {
        return server_error("redis", e);
    }

    let target = state
        .oidc
        .logout_url(
            session.as_ref().and_then(|s| s.id_token.as_deref()),
            &redirect,
        )
        .await
        .unwrap_or(redirect);
    let mut resp = Redirect::to(&target).into_response();
    resp.headers_mut()
        .append(header::SET_COOKIE, clear_cookie(&state));
    resp
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match auth::authenticate(&state, &headers).await {
        Ok(a) => {
            let mut resp = Json(a.identity).into_response();
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp
        }
        Err(e) => auth_error(e),
    }
}

async fn session_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match auth::authenticate(&state, &headers).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => auth_error(e),
    }
}
