pub mod catalogue;
pub mod comptes;
pub mod droits;
pub mod groupes;
pub mod journal;
pub mod roles;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

use crate::{
    AppState,
    annuaire::{self, Compte},
    cache::Portee,
    error::ApiError,
    moteur::{Effet, JOKER},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(catalogue::router())
        .merge(comptes::router())
        .merge(roles::router())
        .merge(groupes::router())
        .merge(droits::router())
        .merge(journal::router())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Sujet {
    Role,
    Group,
    Account,
}

impl Sujet {
    pub fn texte(self) -> &'static str {
        match self {
            Sujet::Role => "role",
            Sujet::Group => "group",
            Sujet::Account => "account",
        }
    }

    pub fn depuis(s: &str) -> Self {
        match s {
            "role" => Sujet::Role,
            "group" => Sujet::Group,
            _ => Sujet::Account,
        }
    }
}

pub fn portee<'a>(kind: Sujet, id: &'a str) -> Portee<'a> {
    match kind {
        Sujet::Account => Portee::Compte(id),
        _ => Portee::Tout,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct RegleVue {
    pub id: Uuid,
    pub subject_kind: String,
    pub subject_id: String,
    pub permission: String,
    pub resource: String,
    pub effect: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub note: String,
    pub granted_by: String,
    pub created_at: DateTime<Utc>,
    #[sqlx(default)]
    pub obsolete: bool,
}

pub const SQL_REGLES: &str = "SELECT r.id, r.subject_kind, r.subject_id, r.permission, r.resource, r.effect, r.expires_at, r.note,
    r.granted_by, r.created_at, (p.obsolete_at IS NOT NULL) AS obsolete
    FROM rules r JOIN permissions p ON p.name = r.permission";

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct LiaisonVue {
    pub id: Uuid,
    pub role_id: Uuid,
    pub role: String,
    pub subject_kind: String,
    pub subject_id: String,
    #[sqlx(default)]
    pub subject_label: Option<String>,
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub granted_by: String,
    pub created_at: DateTime<Utc>,
}

pub const SQL_LIAISONS: &str = "SELECT b.id, b.role_id, ro.name AS role, b.subject_kind, b.subject_id,
    CASE b.subject_kind WHEN 'group' THEN g.name ELSE coalesce(a.email, a.display_name) END AS subject_label,
    b.scope_type, b.scope_id, b.expires_at, b.granted_by, b.created_at
    FROM role_bindings b JOIN roles ro ON ro.id = b.role_id
    LEFT JOIN groups g ON b.subject_kind = 'group' AND g.id::text = b.subject_id
    LEFT JOIN accounts a ON b.subject_kind = 'account' AND a.sub = b.subject_id";

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RegleSaisie {
    pub permission: String,
    #[serde(default = "joker")]
    pub resource: String,
    pub effect: Effet,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub note: String,
}

fn joker() -> String {
    JOKER.to_string()
}

pub async fn valider_regle(c: &mut PgConnection, r: &RegleSaisie) -> Result<(), ApiError> {
    let ligne: Option<(Option<String>, bool)> = sqlx::query_as(
        "SELECT resource_type, obsolete_at IS NOT NULL FROM permissions WHERE name = $1",
    )
    .bind(&r.permission)
    .fetch_optional(c)
    .await?;
    let Some((type_ressource, obsolete)) = ligne else {
        return Err(ApiError::BadRequest(format!(
            "permission inconnue : {}",
            r.permission
        )));
    };
    if obsolete {
        return Err(ApiError::BadRequest(format!(
            "permission obsolète : {}",
            r.permission
        )));
    }
    let res = r.resource.trim();
    if res.is_empty() || res.len() > 200 {
        return Err(ApiError::BadRequest("ressource invalide".into()));
    }
    if res != JOKER && type_ressource.is_none() {
        return Err(ApiError::BadRequest(format!(
            "{} ne se restreint pas à une ressource",
            r.permission
        )));
    }
    valider_expiration(r.expires_at)
}

pub fn valider_expiration(e: Option<DateTime<Utc>>) -> Result<(), ApiError> {
    if e.is_some_and(|e| e <= Utc::now()) {
        return Err(ApiError::BadRequest("expiration dans le passé".into()));
    }
    Ok(())
}

pub async fn valider_sujet(c: &mut PgConnection, kind: Sujet, id: &str) -> Result<(), ApiError> {
    match kind {
        Sujet::Account => annuaire::assurer(c, id).await,
        Sujet::Group => {
            let uuid =
                Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("groupe invalide".into()))?;
            let existe: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM groups WHERE id = $1)")
                    .bind(uuid)
                    .fetch_one(c)
                    .await?;
            if existe {
                Ok(())
            } else {
                Err(ApiError::NotFound("groupe"))
            }
        }
        Sujet::Role => {
            let uuid =
                Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("rôle invalide".into()))?;
            let existe: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM roles WHERE id = $1)")
                    .bind(uuid)
                    .fetch_one(c)
                    .await?;
            if existe {
                Ok(())
            } else {
                Err(ApiError::NotFound("rôle"))
            }
        }
    }
}

pub async fn libelles(db: &PgPool, subs: Vec<String>) -> sqlx::Result<HashMap<String, Compte>> {
    annuaire::par_sub(db, &subs).await
}

pub fn texte_requis(v: &str, quoi: &str) -> Result<String, ApiError> {
    let t = v.trim();
    if t.is_empty() || t.chars().count() > 80 {
        return Err(ApiError::BadRequest(format!(
            "{quoi} invalide (1 à 80 caractères)"
        )));
    }
    Ok(t.to_string())
}
