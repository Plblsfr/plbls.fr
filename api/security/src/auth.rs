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

    use crate::testutil::{AUDIENCE, etat, session_de_test};

    fn entetes(origin: Option<&str>, upgrade: bool) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(o) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(o).unwrap());
        }
        if upgrade {
            h.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        }
        h
    }

    #[tokio::test]
    async fn csrf_origine_exigee_par_cookie_hors_methode_sure() {
        let Some((state, _kc)) = etat().await else {
            return;
        };
        let ok = |o: Option<&str>, up: bool, m: Method, cookie: bool| {
            csrf_ok(&state, &entetes(o, up), &m, cookie)
        };
        assert!(ok(None, false, Method::GET, true));
        assert!(ok(None, false, Method::HEAD, true));
        assert!(ok(None, false, Method::POST, false));
        assert!(!ok(None, false, Method::POST, true));
        assert!(!ok(
            Some("https://evil.example"),
            false,
            Method::DELETE,
            true
        ));
        assert!(!ok(Some("http://app.plbls.fr"), false, Method::POST, true));
        assert!(ok(Some("https://app.plbls.fr"), false, Method::POST, true));
        assert!(
            !ok(None, true, Method::GET, true),
            "upgrade WebSocket par cookie"
        );
        assert!(ok(Some("https://plbls.fr"), true, Method::GET, true));
    }

    async fn avec_session(state: &AppState, access_exp: u64) -> String {
        let sid = session::random_token(32);
        state
            .store
            .put_session(&sid, &session_de_test(access_exp), 60)
            .await
            .unwrap();
        sid
    }

    #[tokio::test]
    async fn session_fraiche_sans_refresh() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let sid = avec_session(&state, session::now() + 300).await;
        let s = load_fresh_session(&state, &sid).await.unwrap();
        assert_eq!(s.access_token, "ancien");
        assert!(kc.demandes().is_empty());
        assert_eq!(
            load_fresh_session(&state, "inconnue").await.unwrap_err(),
            AuthError::Unauthenticated
        );
    }

    #[tokio::test]
    async fn session_expiree_rafraichie_et_enregistree() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let sid = avec_session(&state, session::now()).await;
        let nouveau = kc.signer(AUDIENCE, None, session::now() + 300);
        kc.repondre(
            200,
            serde_json::json!({ "access_token": nouveau, "refresh_token": "r2", "expires_in": 300 }),
        );
        let s = load_fresh_session(&state, &sid).await.unwrap();
        assert_eq!(s.access_token, nouveau);
        assert_eq!(
            s.id_token.as_deref(),
            Some("id1"),
            "id_token absent : l'ancien est gardé"
        );
        let d = kc.demandes();
        assert_eq!(d.len(), 1);
        assert_eq!(
            (d[0]["grant_type"].as_str(), d[0]["refresh_token"].as_str()),
            ("refresh_token", "r1")
        );
        let lu = state.store.get_session(&sid).await.unwrap().unwrap();
        assert_eq!(
            (lu.access_token.as_str(), lu.refresh_token.as_str()),
            (nouveau.as_str(), "r2")
        );
        assert!(
            state.store.try_lock(&sid).await.unwrap(),
            "verrou rendu après le refresh"
        );
    }

    #[tokio::test]
    async fn refresh_refuse_supprime_la_session_panne_la_garde() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let sid = avec_session(&state, session::now()).await;
        kc.repondre(503, serde_json::json!({}));
        assert_eq!(
            load_fresh_session(&state, &sid).await.unwrap_err(),
            AuthError::Unavailable
        );
        assert!(state.store.get_session(&sid).await.unwrap().is_some());
        kc.repondre(400, serde_json::json!({ "error": "invalid_grant" }));
        assert_eq!(
            load_fresh_session(&state, &sid).await.unwrap_err(),
            AuthError::Unauthenticated
        );
        assert!(state.store.get_session(&sid).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn verrou_tenu_on_attend_le_resultat_sans_refresh() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let sid = avec_session(&state, session::now()).await;
        assert!(state.store.try_lock(&sid).await.unwrap());
        let (parti, depart) = tokio::sync::oneshot::channel();
        let attente = tokio::spawn({
            let (state, sid) = (state.clone(), sid.clone());
            async move {
                let _ = parti.send(());
                load_fresh_session(&state, &sid).await
            }
        });
        depart.await.unwrap();
        let mut frais = session_de_test(session::now() + 300);
        frais.access_token = "par-un-autre".into();
        state
            .store
            .update_session(&sid, &frais, None)
            .await
            .unwrap();
        state.store.unlock(&sid).await;
        assert_eq!(attente.await.unwrap().unwrap().access_token, "par-un-autre");
        assert!(kc.demandes().is_empty());
    }

    #[tokio::test]
    async fn refresh_unique_pour_des_appels_simultanes() {
        let Some((state, kc)) = etat().await else {
            return;
        };
        let sid = avec_session(&state, session::now()).await;
        let nouveau = kc.signer(AUDIENCE, None, session::now() + 300);
        kc.repondre(
            200,
            serde_json::json!({ "access_token": nouveau, "expires_in": 300 }),
        );
        let porte = kc.fermer_porte();
        let lancer = || {
            let (state, sid) = (state.clone(), sid.clone());
            tokio::spawn(async move { load_fresh_session(&state, &sid).await })
        };
        let premier = lancer();
        while kc.demandes().is_empty() {
            tokio::task::yield_now().await;
        }
        let second = lancer();
        porte.add_permits(10);
        assert_eq!(premier.await.unwrap().unwrap().access_token, nouveau);
        assert_eq!(second.await.unwrap().unwrap().access_token, nouveau);
        assert_eq!(kc.demandes().len(), 1);
    }
}
