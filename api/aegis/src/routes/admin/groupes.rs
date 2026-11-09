use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{LiaisonVue, RegleVue, SQL_LIAISONS, SQL_REGLES, texte_requis, valider_expiration};
use crate::{
    AppState,
    annuaire::{self, Compte},
    auth::Caller,
    cache::Portee,
    error::{ApiError, ApiResult, ErrorBody},
    journal,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(lister, creer))
        .routes(routes!(voir, modifier, supprimer))
        .routes(routes!(ajouter_membre, retirer_membre))
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct GroupeResume {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub members_count: i64,
    pub roles_count: i64,
}

type LigneMembre = (String, Option<DateTime<Utc>>, String, DateTime<Utc>);

const SQL_RESUME: &str = "SELECT g.id, g.name, g.description, g.created_at,
    (SELECT count(*) FROM group_members m WHERE m.group_id = g.id) AS members_count,
    (SELECT count(*) FROM role_bindings b WHERE b.subject_kind = 'group' AND b.subject_id = g.id::text) AS roles_count
    FROM groups g";

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Membre {
    pub account: Compte,
    pub expires_at: Option<DateTime<Utc>>,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GroupeDetail {
    #[serde(flatten)]
    pub group: GroupeResume,
    pub members: Vec<Membre>,
    pub bindings: Vec<LiaisonVue>,
    pub rules: Vec<RegleVue>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GroupeSaisi {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct MembreSaisi {
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[utoipa::path(get, path = "/v1/admin/groups", tag = "admin", summary = "Groupes de comptes",
    responses((status = 200, body = Vec<GroupeResume>), (status = 403, body = ErrorBody)))]
async fn lister(
    State(state): State<AppState>,
    caller: Caller,
) -> ApiResult<Json<Vec<GroupeResume>>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SQL_RESUME} ORDER BY lower(g.name)"
        )))
        .fetch_all(&state.db)
        .await?,
    ))
}

async fn detail(state: &AppState, id: Uuid) -> ApiResult<GroupeDetail> {
    let group: GroupeResume =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SQL_RESUME} WHERE g.id = $1")))
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound("groupe"))?;
    let lignes: Vec<LigneMembre> = sqlx::query_as(
        "SELECT m.sub, m.expires_at, m.added_by, m.added_at FROM group_members m
         LEFT JOIN accounts a ON a.sub = m.sub WHERE m.group_id = $1 ORDER BY lower(coalesce(a.email, m.sub))",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let mut comptes = annuaire::par_sub(
        &state.db,
        &lignes.iter().map(|l| l.0.clone()).collect::<Vec<_>>(),
    )
    .await?;
    let members = lignes
        .into_iter()
        .map(|(sub, expires_at, added_by, added_at)| Membre {
            account: comptes
                .remove(&sub)
                .unwrap_or_else(|| Compte::inconnu(&sub)),
            expires_at,
            added_by,
            added_at,
        })
        .collect();
    let bindings = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_LIAISONS} WHERE b.subject_kind = 'group' AND b.subject_id = $1 ORDER BY ro.name"
    )))
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;
    let rules = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_REGLES} WHERE r.subject_kind = 'group' AND r.subject_id = $1 ORDER BY p.app, p.position"
    )))
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;
    Ok(GroupeDetail {
        group,
        members,
        bindings,
        rules,
    })
}

#[utoipa::path(post, path = "/v1/admin/groups", tag = "admin", request_body = GroupeSaisi, summary = "Crée un groupe",
    responses((status = 201, body = GroupeDetail), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn creer(
    State(state): State<AppState>,
    caller: Caller,
    Json(g): Json<GroupeSaisi>,
) -> ApiResult<(StatusCode, Json<GroupeDetail>)> {
    let acteur = caller.exiger_admin(&state).await?;
    let nom = texte_requis(&g.name, "nom")?;
    let mut tx = state.db.begin().await?;
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO groups (name, description) VALUES ($1, $2) RETURNING id")
            .bind(&nom)
            .bind(g.description.trim())
            .fetch_one(&mut *tx)
            .await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "group.create",
            target_kind: "group",
            target_id: &id.to_string(),
            subject: None,
            detail: json!({"name": nom}),
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(detail(&state, id).await?)))
}

#[utoipa::path(get, path = "/v1/admin/groups/{id}", tag = "admin", params(("id" = Uuid, Path)),
    summary = "Un groupe : membres, rôles liés, règles",
    responses((status = 200, body = GroupeDetail), (status = 404, body = ErrorBody)))]
async fn voir(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GroupeDetail>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(detail(&state, id).await?))
}

#[utoipa::path(put, path = "/v1/admin/groups/{id}", tag = "admin", params(("id" = Uuid, Path)), request_body = GroupeSaisi,
    summary = "Renomme un groupe", responses((status = 200, body = GroupeDetail), (status = 404, body = ErrorBody)))]
async fn modifier(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
    Json(g): Json<GroupeSaisi>,
) -> ApiResult<Json<GroupeDetail>> {
    let acteur = caller.exiger_admin(&state).await?;
    let nom = texte_requis(&g.name, "nom")?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE groups SET name = $2, description = $3 WHERE id = $1")
        .bind(id)
        .bind(&nom)
        .bind(g.description.trim())
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound("groupe"));
    }
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "group.update",
            target_kind: "group",
            target_id: &id.to_string(),
            subject: None,
            detail: json!({"name": nom}),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(detail(&state, id).await?))
}

#[utoipa::path(delete, path = "/v1/admin/groups/{id}", tag = "admin", params(("id" = Uuid, Path)),
    summary = "Supprime un groupe, ses règles et ses liaisons",
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn supprimer(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let nom: Option<String> = sqlx::query_scalar("DELETE FROM groups WHERE id = $1 RETURNING name")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let nom = nom.ok_or(ApiError::NotFound("groupe"))?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "group.delete",
            target_kind: "group",
            target_id: &id.to_string(),
            subject: None,
            detail: json!({"name": nom}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Tout).await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(put, path = "/v1/admin/groups/{id}/members/{sub}", tag = "admin",
    params(("id" = Uuid, Path), ("sub" = String, Path)), request_body = MembreSaisi,
    summary = "Ajoute un compte au groupe (ou change l'expiration de son appartenance)",
    responses((status = 200, body = GroupeDetail), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn ajouter_membre(
    State(state): State<AppState>,
    caller: Caller,
    Path((id, sub)): Path<(Uuid, String)>,
    Json(m): Json<MembreSaisi>,
) -> ApiResult<Json<GroupeDetail>> {
    let acteur = caller.exiger_admin(&state).await?;
    valider_expiration(m.expires_at)?;
    let mut tx = state.db.begin().await?;
    annuaire::assurer(&mut tx, &sub).await?;
    sqlx::query(
        "INSERT INTO group_members (group_id, sub, expires_at, added_by) VALUES ($1, $2, $3, $4)
         ON CONFLICT (group_id, sub) DO UPDATE SET expires_at = EXCLUDED.expires_at",
    )
    .bind(id)
    .bind(&sub)
    .bind(m.expires_at)
    .bind(&acteur)
    .execute(&mut *tx)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::BadRequest(_) => ApiError::NotFound("groupe"),
        autre => autre,
    })?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "group.member.add",
            target_kind: "group",
            target_id: &id.to_string(),
            subject: Some(&sub),
            detail: json!({"expires_at": m.expires_at}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Compte(&sub)).await;
    Ok(Json(detail(&state, id).await?))
}

#[utoipa::path(delete, path = "/v1/admin/groups/{id}/members/{sub}", tag = "admin",
    params(("id" = Uuid, Path), ("sub" = String, Path)), summary = "Retire un compte du groupe",
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn retirer_membre(
    State(state): State<AppState>,
    caller: Caller,
    Path((id, sub)): Path<(Uuid, String)>,
) -> ApiResult<StatusCode> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND sub = $2")
        .bind(id)
        .bind(&sub)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound("membre"));
    }
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "group.member.remove",
            target_kind: "group",
            target_id: &id.to_string(),
            subject: Some(&sub),
            detail: json!({}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Compte(&sub)).await;
    Ok(StatusCode::NO_CONTENT)
}
