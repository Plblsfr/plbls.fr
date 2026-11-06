use std::time::Duration;

use axum::http::{HeaderMap, Method, header};

use crate::AppState;
use crate::identity::Identity;
use crate::oidc::Rejected;
use crate::origin::origin_allowed;
use crate::session::{self, Session};

/// Marge avant expiration de l'access token pour déclencher le refresh.
const REFRESH_SKEW: u64 = 30;

pub struct Authn {
    pub identity: Identity,
    pub access_token: String,
    /// `Some(sid)` si l'authentification vient du cookie de session.
    pub sid: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum AuthError {
    Unauthenticated,
    Unavailable,
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

/// Bearer d'abord (clients natifs), sinon cookie de session (web).
pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Authn, AuthError> {
    if let Some(token) = bearer(headers) {
        let claims = state.oidc.verify_access_token(token).await.map_err(|e| {
            tracing::debug!("bearer refusé: {e}");
            AuthError::Unauthenticated
        })?;
        return Ok(Authn {
            identity: claims.identity(),
            access_token: token.to_string(),
            sid: None,
        });
    }
    let sid = cookie_value(headers, &state.settings.session.cookie_name)
        .ok_or(AuthError::Unauthenticated)?;
    let session = load_fresh_session(state, &sid).await?;
    Ok(Authn {
        identity: session.identity,
        access_token: session.access_token,
        sid: Some(sid),
    })
}

fn fresh(s: &Session) -> bool {
    s.access_exp > session::now() + REFRESH_SKEW
}

/// Charge la session, la rafraîchit si l'access token expire (un seul refresh à la fois par session).
async fn load_fresh_session(state: &AppState, sid: &str) -> Result<Session, AuthError> {
    let store = &state.store;
    let mut session = store
        .get_session(sid)
        .await
        .map_err(|_| AuthError::Unavailable)?
        .ok_or(AuthError::Unauthenticated)?;
    // 150 x 100 ms = durée du verrou : un autre appel rafraîchit, on attend son résultat ou on prend sa place
    for _ in 0..150 {
        if fresh(&session) {
            return Ok(session);
        }
        if store
            .try_lock(sid)
            .await
            .map_err(|_| AuthError::Unavailable)?
        {
            // le verrou précédent a pu finir juste avant : relire avant de refaire un refresh
            let current = store.get_session(sid).await;
            let result = match current {
                Ok(Some(s)) if fresh(&s) => Ok(s),
                Ok(Some(s)) => refresh(state, sid, &s).await,
                Ok(None) => Err(AuthError::Unauthenticated),
                Err(_) => Err(AuthError::Unavailable),
            };
            store.unlock(sid).await;
            return result;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        session = store
            .get_session(sid)
            .await
            .map_err(|_| AuthError::Unavailable)?
            .ok_or(AuthError::Unauthenticated)?;
    }
    Err(AuthError::Unavailable)
}

async fn refresh(state: &AppState, sid: &str, old: &Session) -> Result<Session, AuthError> {
    let tokens = match state.oidc.refresh(&old.refresh_token).await {
        Ok(t) => t,
        // refresh token expiré ou révoqué : la session est morte
        Err(e) if e.is::<Rejected>() => {
            tracing::info!("refresh refusé, session supprimée: {e}");
            let _ = state.store.delete_session(sid).await;
            return Err(AuthError::Unauthenticated);
        }
        // panne de Keycloak (réseau, 5xx) : on garde la session, le client réessaiera
        Err(e) => {
            tracing::warn!("refresh impossible, session conservée: {e}");
            return Err(AuthError::Unavailable);
        }
    };
    let claims = state
        .oidc
        .verify_access_token(&tokens.access_token)
        .await
        .map_err(|e| {
            tracing::warn!("access token rafraîchi invalide: {e}");
            AuthError::Unauthenticated
        })?;
    let session = Session {
        identity: claims.identity(),
        access_token: tokens.access_token,
        access_exp: claims.exp,
        refresh_token: tokens
            .refresh_token
            .unwrap_or_else(|| old.refresh_token.clone()),
        id_token: tokens.id_token.or_else(|| old.id_token.clone()),
    };
    state
        .store
        .update_session(sid, &session, tokens.refresh_expires_in.filter(|t| *t > 0))
        .await
        .map_err(|_| AuthError::Unavailable)?;
    Ok(session)
}

/// CSRF : requête par cookie avec méthode non sûre (ou upgrade WebSocket) → `Origin` autorisé requis.
pub fn csrf_ok(state: &AppState, headers: &HeaderMap, method: &Method, via_cookie: bool) -> bool {
    let safe = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
        && !headers.contains_key(header::UPGRADE);
    if !via_cookie || safe {
        return true;
    }
    headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|o| origin_allowed(&state.settings.server, o))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn parses_cookies_and_bearer() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; __Secure-plbls_sid=abc; b=2"),
        );
        assert_eq!(
            cookie_value(&h, "__Secure-plbls_sid").as_deref(),
            Some("abc")
        );
        assert_eq!(cookie_value(&h, "zzz"), None);
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer tok"),
        );
        assert_eq!(bearer(&h), Some("tok"));
    }
}
