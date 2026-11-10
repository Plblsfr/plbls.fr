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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};

    use crate::session;
    use crate::testutil::{COOKIE, appeler, etat, session_de_test};

    fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
        let mut r = Request::get(uri);
        if let Some(c) = cookie {
            r = r.header("cookie", c);
        }
        r.body(Body::empty()).unwrap()
    }

    async fn demarrer_login(state: &crate::AppState) -> (String, String, String) {
        let r = appeler(
            state,
            get("/auth/login?redirect=https://app.plbls.fr/x", None),
        )
        .await;
        assert_eq!(r.statut, StatusCode::SEE_OTHER);
        let lieu = url::Url::parse(&r.entete("location")).unwrap();
        let q: std::collections::HashMap<_, _> = lieu.query_pairs().into_owned().collect();
        let cookie = r
            .cookies()
            .into_iter()
            .find(|c| c.starts_with(&format!("{COOKIE}_login=")))
            .unwrap();
        assert!(
            cookie.contains("Path=/auth") && cookie.contains("HttpOnly"),
            "{cookie}"
        );
        assert!(
            cookie.starts_with(&format!("{COOKIE}_login={};", q["state"])),
            "{cookie}"
        );
        (
            q["state"].clone(),
            q["nonce"].clone(),
            q["code_challenge"].clone(),
        )
    }

    #[tokio::test]
    async fn callback_lie_au_navigateur_et_state_a_usage_unique() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let (s, nonce, defi) = demarrer_login(&state).await;
        kc.repondre(200, kc.jetons(&nonce, "r1"));
        let uri = format!("/auth/callback?code=c1&state={s}");

        let sans = appeler(&state, get(&uri, None)).await;
        assert_eq!(sans.statut, StatusCode::BAD_REQUEST);
        assert_eq!(sans.corps, "login non démarré dans ce navigateur");
        assert!(
            sans.cookies()
                .iter()
                .any(|c| c.starts_with(&format!("{COOKIE}_login=;")) && c.contains("Max-Age=0"))
        );
        let autre = appeler(&state, get(&uri, Some(&format!("{COOKIE}_login=autre")))).await;
        assert_eq!(autre.statut, StatusCode::BAD_REQUEST);
        assert!(
            kc.demandes().is_empty(),
            "aucun échange de code sans le cookie"
        );

        let cookie = format!("{COOKIE}_login={s}");
        let ok = appeler(&state, get(&uri, Some(&cookie))).await;
        assert_eq!(ok.statut, StatusCode::SEE_OTHER, "{}", ok.corps);
        assert_eq!(ok.entete("location"), "https://app.plbls.fr/x");
        let sid = ok
            .cookies()
            .into_iter()
            .find_map(|c| {
                c.strip_prefix(&format!("{COOKIE}="))
                    .map(|v| v.split(';').next().unwrap().to_string())
            })
            .unwrap();
        let sess = state.store.get_session(&sid).await.unwrap().unwrap();
        assert_eq!(
            (sess.identity.id.as_str(), sess.refresh_token.as_str()),
            ("u1", "r1")
        );
        let echanges = kc.demandes();
        assert_eq!(echanges.len(), 1);
        assert_eq!(echanges[0]["grant_type"], "authorization_code");
        assert_eq!(echanges[0]["code"], "c1");
        assert_eq!(session::pkce_challenge(&echanges[0]["code_verifier"]), defi);

        let rejoue = appeler(&state, get(&uri, Some(&cookie))).await;
        assert_eq!(rejoue.statut, StatusCode::BAD_REQUEST);
        assert_eq!(rejoue.corps, "login expiré ou déjà utilisé");
        assert_eq!(kc.demandes().len(), 1);
    }

    #[tokio::test]
    async fn callback_refuse_un_nonce_different() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let (s, _, _) = demarrer_login(&state).await;
        kc.repondre(200, kc.jetons(&session::random_token(16), "r1"));
        let r = appeler(
            &state,
            get(
                &format!("/auth/callback?code=c1&state={s}"),
                Some(&format!("{COOKIE}_login={s}")),
            ),
        )
        .await;
        assert_eq!(r.statut, StatusCode::UNAUTHORIZED);
        assert_eq!(r.corps, "ID token invalide");
        assert!(
            !r.cookies()
                .iter()
                .any(|c| c.starts_with(&format!("{COOKIE}=")))
        );
    }

    #[tokio::test]
    async fn logout_exige_le_cookie_et_une_origine_autorisee() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let sid = session::random_token(32);
        state
            .store
            .put_session(&sid, &session_de_test(session::now() + 300), 60)
            .await
            .unwrap();
        let post = |cookie: Option<String>, origin: Option<&str>| {
            let mut r = Request::post("/auth/logout?redirect=https://app.plbls.fr/bye");
            if let Some(c) = cookie {
                r = r.header("cookie", c);
            }
            if let Some(o) = origin {
                r = r.header("origin", o);
            }
            r.body(Body::empty()).unwrap()
        };
        let cookie = format!("{COOKIE}={sid}");

        assert_eq!(
            appeler(&state, post(None, Some("https://app.plbls.fr")))
                .await
                .statut,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            appeler(&state, post(Some(cookie.clone()), None))
                .await
                .statut,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            appeler(
                &state,
                post(Some(cookie.clone()), Some("https://evil.example"))
            )
            .await
            .statut,
            StatusCode::FORBIDDEN
        );
        assert!(state.store.get_session(&sid).await.unwrap().is_some());
        assert_eq!(kc.revocations(), 0);

        let r = appeler(&state, post(Some(cookie), Some("https://app.plbls.fr"))).await;
        assert_eq!(r.statut, StatusCode::SEE_OTHER);
        let lieu = url::Url::parse(&r.entete("location")).unwrap();
        assert_eq!(
            lieu.as_str().split('?').next().unwrap(),
            format!("{}/logout", kc.issuer())
        );
        let q: std::collections::HashMap<_, _> = lieu.query_pairs().into_owned().collect();
        assert_eq!(q["id_token_hint"], "id1");
        assert_eq!(q["post_logout_redirect_uri"], "https://app.plbls.fr/bye");
        assert!(
            r.cookies()
                .iter()
                .any(|c| c.starts_with(&format!("{COOKIE}=;")) && c.contains("Max-Age=0"))
        );
        assert!(state.store.get_session(&sid).await.unwrap().is_none());
        assert_eq!(kc.revocations(), 1);
    }
}
