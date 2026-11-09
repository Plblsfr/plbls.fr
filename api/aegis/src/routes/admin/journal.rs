use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::libelles;
use crate::{
    AppState,
    annuaire::Compte,
    auth::Caller,
    error::{ApiResult, ErrorBody},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(journal))
}

#[derive(Debug, Deserialize, IntoParams)]
struct FiltreJournal {
    subject: Option<String>,
    actor: Option<String>,
    action: Option<String>,
    before: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct EntreeJournal {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub target_kind: String,
    pub target_id: String,
    pub target_label: Option<String>,
    pub subject: Option<String>,
    pub detail: Value,
    #[sqlx(skip)]
    pub actor_account: Option<Compte>,
    #[sqlx(skip)]
    pub subject_account: Option<Compte>,
}

#[utoipa::path(get, path = "/v1/admin/audit", tag = "admin", params(FiltreJournal),
    summary = "Journal des modifications, comptes rendus avec leur e-mail",
    description = "Plus récent d'abord ; `before` = `id` de la dernière entrée reçue pour la page suivante.",
    responses((status = 200, body = Vec<EntreeJournal>), (status = 403, body = ErrorBody)))]
async fn journal(
    State(state): State<AppState>,
    caller: Caller,
    Query(f): Query<FiltreJournal>,
) -> ApiResult<Json<Vec<EntreeJournal>>> {
    caller.exiger_admin(&state).await?;
    let mut lignes: Vec<EntreeJournal> = sqlx::query_as(
        "SELECT l.id, l.at, l.actor, l.action, l.target_kind, l.target_id, l.subject, l.detail,
            CASE l.target_kind WHEN 'role' THEN (SELECT name FROM roles WHERE id::text = l.target_id)
                 WHEN 'group' THEN (SELECT name FROM groups WHERE id::text = l.target_id)
                 WHEN 'permission' THEN (SELECT label FROM permissions WHERE name = l.target_id)
                 WHEN 'app' THEN (SELECT name FROM apps WHERE key = l.target_id) END AS target_label
         FROM audit_log l
         WHERE ($1::text IS NULL OR l.subject = $1 OR (l.target_kind = 'account' AND l.target_id = $1))
           AND ($2::text IS NULL OR l.actor = $2)
           AND ($3::text IS NULL OR l.action LIKE $3 || '%')
           AND ($4::bigint IS NULL OR l.id < $4)
         ORDER BY l.id DESC LIMIT $5",
    )
    .bind(f.subject)
    .bind(f.actor)
    .bind(f.action)
    .bind(f.before)
    .bind(f.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&state.db)
    .await?;
    let subs: Vec<String> = lignes
        .iter()
        .flat_map(|l| [Some(l.actor.clone()), l.subject.clone()])
        .flatten()
        .collect();
    let comptes = libelles(&state.db, subs).await?;
    for l in &mut lignes {
        l.actor_account = comptes.get(&l.actor).cloned();
        l.subject_account = l.subject.as_ref().and_then(|s| comptes.get(s).cloned());
    }
    Ok(Json(lignes))
}
