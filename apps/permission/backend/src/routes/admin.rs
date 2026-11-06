//! Administration : applications, permissions, rôles, attributions, audit. Réservé au rôle `server.admin_role`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    AppState,
    auth::Caller,
    error::{ApiError, ApiResult, ErrorBody},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_apps, create_app))
        .routes(routes!(update_app, delete_app))
        .routes(routes!(list_permissions, create_permission))
        .routes(routes!(delete_permission))
        .routes(routes!(list_roles, create_role))
        .routes(routes!(update_role, delete_role))
        .routes(routes!(list_assignments, grant, revoke))
        .routes(routes!(audit))
}

async fn audit_log(
    tx: &mut PgConnection,
    caller: &Caller,
    action: &str,
    target: &str,
    detail: Value,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO audit_log (actor, action, target, detail) VALUES ($1, $2, $3, $4)")
        .bind(caller.actor())
        .bind(action)
        .bind(target)
        .bind(detail)
        .execute(tx)
        .await?;
    Ok(())
}

async fn app_id(db: &mut PgConnection, key: &str) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT id FROM apps WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::NotFound("application"))
}

async fn role_id(db: &mut PgConnection, app: Uuid, key: &str) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT id FROM roles WHERE app_id = $1 AND key = $2")
        .bind(app)
        .bind(key)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::NotFound("rôle"))
}

// ---------- applications ----------

#[derive(Serialize, FromRow, ToSchema)]
struct App {
    key: String,
    name: String,
    description: String,
}

#[derive(Deserialize, ToSchema)]
struct NewApp {
    key: String,
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize, ToSchema)]
struct AppUpdate {
    name: String,
    #[serde(default)]
    description: String,
}

/// Liste des applications.
#[utoipa::path(get, path = "/v1/apps", tag = "admin", responses((status = 200, body = Vec<App>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn list_apps(State(s): State<AppState>, c: Caller) -> ApiResult<Json<Vec<App>>> {
    c.require_admin(&s).await?;
    Ok(Json(
        sqlx::query_as("SELECT key, name, description FROM apps ORDER BY key")
            .fetch_all(&s.db)
            .await?,
    ))
}

/// Crée une application.
#[utoipa::path(post, path = "/v1/apps", tag = "admin", request_body = NewApp, responses((status = 201, body = App), (status = 400, body = ErrorBody, description = "Clé invalide"), (status = 409, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn create_app(
    State(s): State<AppState>,
    c: Caller,
    Json(a): Json<NewApp>,
) -> ApiResult<(StatusCode, Json<App>)> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO apps (key, name, description) VALUES ($1, $2, $3)")
        .bind(&a.key)
        .bind(&a.name)
        .bind(&a.description)
        .execute(&mut *tx)
        .await?;
    audit_log(&mut tx, &c, "app.create", &a.key, json!({"name": a.name})).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(App {
            key: a.key,
            name: a.name,
            description: a.description,
        }),
    ))
}

/// Modifie le nom et la description d'une application.
#[utoipa::path(put, path = "/v1/apps/{app}", tag = "admin", request_body = AppUpdate, responses((status = 200, body = App), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn update_app(
    State(s): State<AppState>,
    c: Caller,
    Path(app): Path<String>,
    Json(u): Json<AppUpdate>,
) -> ApiResult<Json<App>> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let done = sqlx::query("UPDATE apps SET name = $2, description = $3 WHERE key = $1")
        .bind(&app)
        .bind(&u.name)
        .bind(&u.description)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::NotFound("application"));
    }
    audit_log(&mut tx, &c, "app.update", &app, json!({"name": u.name})).await?;
    tx.commit().await?;
    Ok(Json(App {
        key: app,
        name: u.name,
        description: u.description,
    }))
}

/// Supprime l'application avec ses permissions, rôles et attributions.
#[utoipa::path(delete, path = "/v1/apps/{app}", tag = "admin", responses((status = 204), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn delete_app(
    State(s): State<AppState>,
    c: Caller,
    Path(app): Path<String>,
) -> ApiResult<StatusCode> {
    c.require_admin(&s).await?;
    if app == "permission" {
        return Err(ApiError::BadRequest(
            "application réservée : elle porte l'accès à permission".into(),
        ));
    }
    let mut tx = s.db.begin().await?;
    if sqlx::query("DELETE FROM apps WHERE key = $1")
        .bind(&app)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        == 0
    {
        return Err(ApiError::NotFound("application"));
    }
    audit_log(&mut tx, &c, "app.delete", &app, json!({})).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- permissions ----------

#[derive(Serialize, FromRow, ToSchema)]
struct Permission {
    key: String,
    description: String,
}

#[derive(Deserialize, ToSchema)]
struct NewPermission {
    key: String,
    #[serde(default)]
    description: String,
}

/// Permissions d'une application.
#[utoipa::path(get, path = "/v1/apps/{app}/permissions", tag = "admin", responses((status = 200, body = Vec<Permission>), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn list_permissions(
    State(s): State<AppState>,
    c: Caller,
    Path(app): Path<String>,
) -> ApiResult<Json<Vec<Permission>>> {
    c.require_admin(&s).await?;
    let mut conn = s.db.acquire().await?;
    let id = app_id(&mut conn, &app).await?;
    Ok(Json(
        sqlx::query_as("SELECT key, description FROM permissions WHERE app_id = $1 ORDER BY key")
            .bind(id)
            .fetch_all(&mut *conn)
            .await?,
    ))
}

/// Déclare une permission.
#[utoipa::path(post, path = "/v1/apps/{app}/permissions", tag = "admin", request_body = NewPermission, responses((status = 201, body = Permission), (status = 409, body = ErrorBody), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn create_permission(
    State(s): State<AppState>,
    c: Caller,
    Path(app): Path<String>,
    Json(p): Json<NewPermission>,
) -> ApiResult<(StatusCode, Json<Permission>)> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let id = app_id(&mut tx, &app).await?;
    sqlx::query("INSERT INTO permissions (app_id, key, description) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(&p.key)
        .bind(&p.description)
        .execute(&mut *tx)
        .await?;
    audit_log(
        &mut tx,
        &c,
        "permission.create",
        &format!("{app}:{}", p.key),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Permission {
            key: p.key,
            description: p.description,
        }),
    ))
}

/// Supprime une permission (elle est retirée des rôles).
#[utoipa::path(delete, path = "/v1/apps/{app}/permissions/{key}", tag = "admin", responses((status = 204), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn delete_permission(
    State(s): State<AppState>,
    c: Caller,
    Path((app, key)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let id = app_id(&mut tx, &app).await?;
    let done = sqlx::query("DELETE FROM permissions WHERE app_id = $1 AND key = $2")
        .bind(id)
        .bind(&key)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::NotFound("permission"));
    }
    audit_log(
        &mut tx,
        &c,
        "permission.delete",
        &format!("{app}:{key}"),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- rôles ----------

#[derive(Serialize, FromRow, ToSchema)]
struct Role {
    key: String,
    name: String,
    description: String,
    permissions: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
struct NewRole {
    key: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    permissions: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
struct RoleUpdate {
    name: String,
    #[serde(default)]
    description: String,
    /// Remplace l'ensemble des permissions du rôle.
    permissions: Vec<String>,
}

async fn set_role_permissions(
    tx: &mut PgConnection,
    app: Uuid,
    role: Uuid,
    keys: &[String],
) -> ApiResult<Vec<String>> {
    let mut keys = keys.to_vec();
    keys.sort();
    keys.dedup();
    sqlx::query("DELETE FROM role_permissions WHERE role_id = $1")
        .bind(role)
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query(
        "INSERT INTO role_permissions (role_id, permission_id, app_id)
         SELECT $1, id, app_id FROM permissions WHERE app_id = $2 AND key = ANY($3)",
    )
    .bind(role)
    .bind(app)
    .bind(&keys)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n as usize != keys.len() {
        return Err(ApiError::BadRequest(
            "permission inconnue pour cette application".into(),
        ));
    }
    Ok(keys)
}

/// Rôles d'une application, avec leurs permissions.
#[utoipa::path(get, path = "/v1/apps/{app}/roles", tag = "admin", responses((status = 200, body = Vec<Role>), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn list_roles(
    State(s): State<AppState>,
    c: Caller,
    Path(app): Path<String>,
) -> ApiResult<Json<Vec<Role>>> {
    c.require_admin(&s).await?;
    let mut conn = s.db.acquire().await?;
    let id = app_id(&mut conn, &app).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT r.key, r.name, r.description,
                    COALESCE(array_agg(p.key ORDER BY p.key) FILTER (WHERE p.key IS NOT NULL), '{}') AS permissions
             FROM roles r
             LEFT JOIN role_permissions rp ON rp.role_id = r.id
             LEFT JOIN permissions p ON p.id = rp.permission_id
             WHERE r.app_id = $1 GROUP BY r.id ORDER BY r.key",
        )
        .bind(id)
        .fetch_all(&mut *conn)
        .await?,
    ))
}

/// Crée un rôle.
#[utoipa::path(post, path = "/v1/apps/{app}/roles", tag = "admin", request_body = NewRole, responses((status = 201, body = Role), (status = 400, body = ErrorBody, description = "Permission inconnue"), (status = 409, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn create_role(
    State(s): State<AppState>,
    c: Caller,
    Path(app): Path<String>,
    Json(r): Json<NewRole>,
) -> ApiResult<(StatusCode, Json<Role>)> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let app_uuid = app_id(&mut tx, &app).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (app_id, key, name, description) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(app_uuid)
    .bind(&r.key)
    .bind(&r.name)
    .bind(&r.description)
    .fetch_one(&mut *tx)
    .await?;
    let permissions = set_role_permissions(&mut tx, app_uuid, id, &r.permissions).await?;
    audit_log(
        &mut tx,
        &c,
        "role.create",
        &format!("{app}/{}", r.key),
        json!({"permissions": permissions}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Role {
            key: r.key,
            name: r.name,
            description: r.description,
            permissions,
        }),
    ))
}

/// Remplace le nom, la description et l'ensemble des permissions d'un rôle.
#[utoipa::path(put, path = "/v1/apps/{app}/roles/{role}", tag = "admin", request_body = RoleUpdate, responses((status = 200, body = Role), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn update_role(
    State(s): State<AppState>,
    c: Caller,
    Path((app, role)): Path<(String, String)>,
    Json(u): Json<RoleUpdate>,
) -> ApiResult<Json<Role>> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let app_uuid = app_id(&mut tx, &app).await?;
    let id = role_id(&mut tx, app_uuid, &role).await?;
    sqlx::query("UPDATE roles SET name = $2, description = $3 WHERE id = $1")
        .bind(id)
        .bind(&u.name)
        .bind(&u.description)
        .execute(&mut *tx)
        .await?;
    let permissions = set_role_permissions(&mut tx, app_uuid, id, &u.permissions).await?;
    audit_log(
        &mut tx,
        &c,
        "role.update",
        &format!("{app}/{role}"),
        json!({"permissions": permissions}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Role {
        key: role,
        name: u.name,
        description: u.description,
        permissions,
    }))
}

/// Supprime un rôle et ses attributions.
#[utoipa::path(delete, path = "/v1/apps/{app}/roles/{role}", tag = "admin", responses((status = 204), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn delete_role(
    State(s): State<AppState>,
    c: Caller,
    Path((app, role)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let app_uuid = app_id(&mut tx, &app).await?;
    let id = role_id(&mut tx, app_uuid, &role).await?;
    sqlx::query("DELETE FROM roles WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit_log(
        &mut tx,
        &c,
        "role.delete",
        &format!("{app}/{role}"),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- attributions ----------

#[derive(Serialize, FromRow, ToSchema)]
struct Assignment {
    user_id: String,
    app: String,
    role: String,
    granted_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize, IntoParams)]
struct AssignmentFilter {
    user_id: Option<String>,
    app: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, ToSchema, IntoParams)]
struct AssignmentRef {
    user_id: String,
    app: String,
    role: String,
}

/// Attributions de rôles (500 max, plus récentes d'abord).
#[utoipa::path(get, path = "/v1/assignments", tag = "admin", params(AssignmentFilter), responses((status = 200, body = Vec<Assignment>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn list_assignments(
    State(s): State<AppState>,
    c: Caller,
    Query(f): Query<AssignmentFilter>,
) -> ApiResult<Json<Vec<Assignment>>> {
    c.require_admin(&s).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT s.user_id, a.key AS app, r.key AS role, s.granted_by, s.created_at
             FROM assignments s JOIN roles r ON r.id = s.role_id JOIN apps a ON a.id = r.app_id
             WHERE ($1::text IS NULL OR s.user_id = $1) AND ($2::text IS NULL OR a.key = $2) AND ($3::text IS NULL OR r.key = $3)
             ORDER BY s.created_at DESC LIMIT 500",
        )
        .bind(f.user_id)
        .bind(f.app)
        .bind(f.role)
        .fetch_all(&s.db)
        .await?,
    ))
}

fn check_user_id(user_id: &str) -> ApiResult<()> {
    if user_id.is_empty() || user_id.len() > 128 {
        return Err(ApiError::BadRequest("user_id invalide".into()));
    }
    Ok(())
}

/// Donne un rôle à un compte (idempotent).
#[utoipa::path(post, path = "/v1/assignments", tag = "admin", request_body = AssignmentRef, responses((status = 204), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn grant(
    State(s): State<AppState>,
    c: Caller,
    Json(r): Json<AssignmentRef>,
) -> ApiResult<StatusCode> {
    c.require_admin(&s).await?;
    check_user_id(&r.user_id)?;
    let mut tx = s.db.begin().await?;
    let app = app_id(&mut tx, &r.app).await?;
    let role = role_id(&mut tx, app, &r.role).await?;
    sqlx::query("INSERT INTO assignments (user_id, role_id, granted_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(&r.user_id)
        .bind(role)
        .bind(c.actor())
        .execute(&mut *tx)
        .await?;
    audit_log(
        &mut tx,
        &c,
        "assignment.grant",
        &r.user_id,
        json!({"app": r.app, "role": r.role}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Retire un rôle à un compte.
#[utoipa::path(delete, path = "/v1/assignments", tag = "admin", params(AssignmentRef), responses((status = 204), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn revoke(
    State(s): State<AppState>,
    c: Caller,
    Query(r): Query<AssignmentRef>,
) -> ApiResult<StatusCode> {
    c.require_admin(&s).await?;
    let mut tx = s.db.begin().await?;
    let app = app_id(&mut tx, &r.app).await?;
    let role = role_id(&mut tx, app, &r.role).await?;
    if sqlx::query("DELETE FROM assignments WHERE user_id = $1 AND role_id = $2")
        .bind(&r.user_id)
        .bind(role)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        == 0
    {
        return Err(ApiError::NotFound("attribution"));
    }
    audit_log(
        &mut tx,
        &c,
        "assignment.revoke",
        &r.user_id,
        json!({"app": r.app, "role": r.role}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- audit ----------

#[derive(Serialize, FromRow, ToSchema)]
struct AuditEntry {
    at: chrono::DateTime<chrono::Utc>,
    actor: String,
    action: String,
    target: String,
    #[schema(value_type = Object)]
    detail: Value,
}

#[derive(Deserialize, IntoParams)]
struct AuditQuery {
    limit: Option<i64>,
}

/// Journal des modifications, plus récentes d'abord.
#[utoipa::path(get, path = "/v1/audit", tag = "admin", params(AuditQuery), responses((status = 200, body = Vec<AuditEntry>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn audit(
    State(s): State<AppState>,
    c: Caller,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<AuditEntry>>> {
    c.require_admin(&s).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT at, actor, action, target, detail FROM audit_log ORDER BY id DESC LIMIT $1",
        )
        .bind(q.limit.unwrap_or(100).clamp(1, 1000))
        .fetch_all(&s.db)
        .await?,
    ))
}
