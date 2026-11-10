//! Qui appelle ? Soit un utilisateur (en-têtes d'identité signés par le gateway), soit un micro-service (Bearer).

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};
use commun::identite as identity;

use crate::{AppState, error::ApiError};

#[derive(Debug, Clone)]
pub enum Caller {
    User { id: String, roles: Vec<String> },
    Service(String),
}

fn header_str<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let h = &parts.headers;
        if let Some(token) = header_str(h, header::AUTHORIZATION.as_str())
            .and_then(|v| v.split_once(' '))
            .filter(|(s, _)| s.eq_ignore_ascii_case("bearer"))
            .map(|(_, t)| t.trim())
        {
            return state
                .settings
                .services
                .find(token)
                .map(|n| Caller::Service(n.to_string()))
                .ok_or(ApiError::Unauthorized);
        }
        let (id, roles, client) = (
            header_str(h, "x-user-id").ok_or(ApiError::Unauthorized)?,
            header_str(h, "x-user-roles").unwrap_or_default(),
            header_str(h, "x-client-id").unwrap_or_default(),
        );
        let ts = header_str(h, "x-identity-ts")
            .and_then(|v| v.parse().ok())
            .ok_or(ApiError::Unauthorized)?;
        let sig = header_str(h, "x-identity-sig").ok_or(ApiError::Unauthorized)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let champs = identity::Champs {
            id,
            roles,
            client,
            ts,
            name: header_str(h, "x-user-name").unwrap_or_default(),
            email: header_str(h, "x-user-email").unwrap_or_default(),
        };
        if !identity::verify(&state.settings.identity.secret, &champs, sig, now) {
            return Err(ApiError::Unauthorized);
        }
        Ok(Caller::User {
            id: id.to_string(),
            roles: roles
                .split(',')
                .filter(|r| !r.is_empty())
                .map(str::to_string)
                .collect(),
        })
    }
}

impl Caller {
    /// Identifiant écrit dans le journal d'audit.
    pub fn actor(&self) -> String {
        match self {
            Caller::User { id, .. } => id.clone(),
            Caller::Service(n) => format!("service:{n}"),
        }
    }

    /// Admin = rôle Keycloak `server.admin_role` (point de départ et secours) ou permission `permission:admin`.
    /// Un service n'est jamais admin.
    pub async fn is_admin(&self, state: &AppState) -> Result<bool, ApiError> {
        let Caller::User { roles, .. } = self else {
            return Ok(false);
        };
        Ok(roles.iter().any(|r| r == &state.settings.server.admin_role)
            || self.has(state, "permission", "admin").await?)
    }

    /// Le compte porte-t-il `app:perm` via un de ses rôles ? (jamais pour un service)
    pub async fn has(&self, state: &AppState, app: &str, perm: &str) -> Result<bool, ApiError> {
        let Caller::User { id, .. } = self else {
            return Ok(false);
        };
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (
               SELECT 1 FROM assignments s
               JOIN role_permissions rp ON rp.role_id = s.role_id
               JOIN permissions p ON p.id = rp.permission_id
               JOIN apps a ON a.id = p.app_id
               WHERE s.user_id = $1 AND a.key = $2 AND p.key = $3)",
        )
        .bind(id)
        .bind(app)
        .bind(perm)
        .fetch_one(&state.db)
        .await?)
    }

    pub async fn require_admin(&self, state: &AppState) -> Result<(), ApiError> {
        if self.is_admin(state).await? {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }

    /// Peut lire les permissions de `user_id` : soi-même, un service, ou un admin.
    pub async fn can_read(&self, user_id: &str, state: &AppState) -> Result<bool, ApiError> {
        Ok(match self {
            Caller::Service(_) => true,
            Caller::User { id, .. } => id == user_id || self.is_admin(state).await?,
        })
    }
}
