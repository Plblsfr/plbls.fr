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

use super::{
    LiaisonVue, RegleSaisie, RegleVue, SQL_LIAISONS, SQL_REGLES, texte_requis, valider_regle,
};
use crate::{
    AppState,
    auth::Caller,
    cache::Portee,
    error::{ApiError, ApiResult, ErrorBody},
    journal,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(lister, creer))
        .routes(routes!(voir, modifier, supprimer))
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct RoleResume {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub system: Option<String>,
    pub created_at: DateTime<Utc>,
    pub rules_count: i64,
    pub bindings_count: i64,
    pub accounts_count: i64,
}

const SQL_RESUME: &str = "SELECT ro.id, ro.name, ro.description, ro.system, ro.created_at,
    (SELECT count(*) FROM rules r WHERE r.subject_kind = 'role' AND r.subject_id = ro.id::text) AS rules_count,
    (SELECT count(*) FROM role_bindings b WHERE b.role_id = ro.id) AS bindings_count,
    CASE WHEN ro.system = 'tout-le-monde' THEN (SELECT count(*) FROM accounts)
    ELSE (SELECT count(DISTINCT s) FROM (
        SELECT b.subject_id AS s FROM role_bindings b WHERE b.role_id = ro.id AND b.subject_kind = 'account'
        UNION ALL
        SELECT m.sub FROM role_bindings b JOIN group_members m ON b.subject_kind = 'group' AND m.group_id::text = b.subject_id
        WHERE b.role_id = ro.id) x) END AS accounts_count
    FROM roles ro";

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoleDetail {
    #[serde(flatten)]
    pub role: RoleResume,
    pub rules: Vec<RegleVue>,
    pub bindings: Vec<LiaisonVue>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RoleSaisi {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub rules: Vec<RegleSaisie>,
}

#[utoipa::path(get, path = "/v1/admin/roles", tag = "admin", summary = "Rôles, avec le nombre de comptes concernés",
    responses((status = 200, body = Vec<RoleResume>), (status = 403, body = ErrorBody)))]
async fn lister(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Vec<RoleResume>>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SQL_RESUME} ORDER BY ro.system = 'tout-le-monde' DESC NULLS LAST, ro.system IS NOT NULL DESC, lower(ro.name)"
        )))
        .fetch_all(&state.db)
        .await?,
    ))
}

pub async fn detail(state: &AppState, id: Uuid) -> ApiResult<RoleDetail> {
    let role: RoleResume = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_RESUME} WHERE ro.id = $1"
    )))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("rôle"))?;
    let rules = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_REGLES} WHERE r.subject_kind = 'role' AND r.subject_id = $1 ORDER BY p.app, p.position, r.resource"
    )))
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;
    let bindings = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_LIAISONS} WHERE b.role_id = $1 ORDER BY b.created_at"
    )))
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(RoleDetail {
        role,
        rules,
        bindings,
    })
}

async fn ecrire_regles(
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    regles: &[RegleSaisie],
    acteur: &str,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM rules WHERE subject_kind = 'role' AND subject_id = $1")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    for r in regles {
        valider_regle(tx, r).await?;
        sqlx::query(
            "INSERT INTO rules (subject_kind, subject_id, permission, resource, effect, expires_at, note, granted_by)
             VALUES ('role', $1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id.to_string())
        .bind(&r.permission)
        .bind(r.resource.trim())
        .bind(r.effect.texte())
        .bind(r.expires_at)
        .bind(&r.note)
        .bind(acteur)
        .execute(&mut *tx)
        .await
        .map_err(|e| match ApiError::from(e) {
            ApiError::Conflict(_) => ApiError::BadRequest(format!(
                "règle en double : {} sur {}",
                r.permission, r.resource
            )),
            autre => autre,
        })?;
    }
    Ok(())
}

#[utoipa::path(post, path = "/v1/admin/roles", tag = "admin", request_body = RoleSaisi,
    summary = "Crée un rôle (règles de plusieurs applications possibles)",
    responses((status = 201, body = RoleDetail), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn creer(
    State(state): State<AppState>,
    caller: Caller,
    Json(r): Json<RoleSaisi>,
) -> ApiResult<(StatusCode, Json<RoleDetail>)> {
    let acteur = caller.exiger_admin(&state).await?;
    let nom = texte_requis(&r.name, "nom")?;
    let mut tx = state.db.begin().await?;
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO roles (name, description) VALUES ($1, $2) RETURNING id")
            .bind(&nom)
            .bind(r.description.trim())
            .fetch_one(&mut *tx)
            .await?;
    ecrire_regles(&mut tx, id, &r.rules, &acteur).await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "role.create",
            target_kind: "role",
            target_id: &id.to_string(),
            subject: None,
            detail: json!({"name": nom, "rules": r.rules}),
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(detail(&state, id).await?)))
}

#[utoipa::path(get, path = "/v1/admin/roles/{id}", tag = "admin", params(("id" = Uuid, Path)),
    summary = "Un rôle, ses règles et ses liaisons",
    responses((status = 200, body = RoleDetail), (status = 404, body = ErrorBody)))]
async fn voir(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RoleDetail>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(detail(&state, id).await?))
}

#[utoipa::path(put, path = "/v1/admin/roles/{id}", tag = "admin", params(("id" = Uuid, Path)), request_body = RoleSaisi,
    summary = "Renomme un rôle et remplace toutes ses règles",
    responses((status = 200, body = RoleDetail), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn modifier(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
    Json(r): Json<RoleSaisi>,
) -> ApiResult<Json<RoleDetail>> {
    let acteur = caller.exiger_admin(&state).await?;
    let nom = texte_requis(&r.name, "nom")?;
    let avant = detail(&state, id).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE roles SET name = $2, description = $3 WHERE id = $1")
        .bind(id)
        .bind(&nom)
        .bind(r.description.trim())
        .execute(&mut *tx)
        .await?;
    ecrire_regles(&mut tx, id, &r.rules, &acteur).await?;
    let anciennes: Vec<String> = avant
        .rules
        .iter()
        .map(|x| format!("{} {} {}", x.effect, x.permission, x.resource))
        .collect();
    let nouvelles: Vec<String> = r
        .rules
        .iter()
        .map(|x| {
            format!(
                "{} {} {}",
                x.effect.texte(),
                x.permission,
                x.resource.trim()
            )
        })
        .collect();
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "role.update",
            target_kind: "role",
            target_id: &id.to_string(),
            subject: None,
            detail: json!({
                "name": nom,
                "added": nouvelles.iter().filter(|x| !anciennes.contains(x)).collect::<Vec<_>>(),
                "removed": anciennes.iter().filter(|x| !nouvelles.contains(x)).collect::<Vec<_>>(),
            }),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Tout).await;
    Ok(Json(detail(&state, id).await?))
}

#[utoipa::path(delete, path = "/v1/admin/roles/{id}", tag = "admin", params(("id" = Uuid, Path)),
    summary = "Supprime un rôle, ses règles et ses liaisons (rôles système exclus)",
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn supprimer(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let ligne: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT name, system FROM roles WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let (nom, system) = ligne.ok_or(ApiError::NotFound("rôle"))?;
    if system.is_some() {
        return Err(ApiError::BadRequest(
            "un rôle système ne se supprime pas".into(),
        ));
    }
    sqlx::query("DELETE FROM roles WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "role.delete",
            target_kind: "role",
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
