use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};

use crate::{AppState, error::ApiError, identity, stockage};

pub const PERMISSION_ADMIN: &str = "aegis:admin";

#[derive(Debug, Clone)]
pub enum Caller {
    User {
        sub: String,
        roles: Vec<String>,
        email: Option<String>,
        name: Option<String>,
    },
    Service(String),
}

fn entete<'a>(h: &'a HeaderMap, nom: &str) -> Option<&'a str> {
    h.get(nom).and_then(|v| v.to_str().ok())
}

pub fn bearer(h: &HeaderMap) -> Option<&str> {
    entete(h, header::AUTHORIZATION.as_str())
        .and_then(|v| v.split_once(' '))
        .filter(|(s, _)| s.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim())
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let h = &parts.headers;
        if let Some(jeton) = bearer(h) {
            return state
                .settings
                .services
                .trouver(jeton)
                .map(|n| Caller::Service(n.to_string()))
                .ok_or(ApiError::Unauthorized);
        }
        let sub = entete(h, "x-user-id").ok_or(ApiError::Unauthorized)?;
        let roles = entete(h, "x-user-roles").unwrap_or_default();
        let client = entete(h, "x-client-id").unwrap_or_default();
        let ts = entete(h, "x-identity-ts")
            .and_then(|v| v.parse().ok())
            .ok_or(ApiError::Unauthorized)?;
        let sig = entete(h, "x-identity-sig").ok_or(ApiError::Unauthorized)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if !identity::verify(
            &state.settings.identity.secret,
            sub,
            roles,
            client,
            ts,
            sig,
            now,
        ) {
            return Err(ApiError::Unauthorized);
        }
        Ok(Caller::User {
            sub: sub.to_string(),
            roles: roles
                .split(',')
                .filter(|r| !r.is_empty())
                .map(str::to_string)
                .collect(),
            email: entete(h, "x-user-email").map(str::to_string),
            name: entete(h, "x-user-name").map(str::to_string),
        })
    }
}

impl Caller {
    pub fn actor(&self) -> String {
        match self {
            Caller::User { sub, .. } => sub.clone(),
            Caller::Service(n) => format!("service:{n}"),
        }
    }

    pub fn service(&self) -> Result<&str, ApiError> {
        match self {
            Caller::Service(n) => Ok(n),
            Caller::User { .. } => Err(ApiError::Forbidden),
        }
    }

    pub async fn est_admin(&self, state: &AppState) -> Result<bool, ApiError> {
        let Caller::User { sub, roles, .. } = self else {
            return Ok(false);
        };
        if roles.iter().any(|r| r == &state.settings.server.admin_role) {
            return Ok(true);
        }
        Ok(stockage::effectifs(state, sub)
            .await?
            .autorise(PERMISSION_ADMIN, None))
    }

    pub async fn exiger_admin(&self, state: &AppState) -> Result<String, ApiError> {
        if !self.est_admin(state).await? {
            return Err(ApiError::Forbidden);
        }
        if let Caller::User {
            sub, email, name, ..
        } = self
        {
            let _ = crate::annuaire::vu(&state.db, sub, email.as_deref(), name.as_deref()).await;
        }
        Ok(self.actor())
    }
}
