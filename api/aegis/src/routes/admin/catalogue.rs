use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::{RegleVue, SQL_REGLES, libelles};
use crate::{
    AppState,
    annuaire::Compte,
    auth::Caller,
    cache::Portee,
    error::{ApiError, ApiResult, ErrorBody},
    journal,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(catalogue))
        .routes(routes!(ressources))
        .routes(routes!(detenteurs))
        .routes(routes!(purger))
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct PermissionVue {
    pub name: String,
    pub app: String,
    pub key: String,
    pub category: String,
    pub label: String,
    pub description: String,
    pub resource_type: Option<String>,
    pub open_default: bool,
    pub obsolete_at: Option<DateTime<Utc>>,
    pub rules_count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct TypeVue {
    pub app: String,
    pub key: String,
    pub name: String,
    pub listable: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AppVue {
    pub key: String,
    pub name: String,
    pub description: String,
    pub seen_at: DateTime<Utc>,
    pub resource_types: Vec<TypeVue>,
    pub permissions: Vec<PermissionVue>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Catalogue {
    pub apps: Vec<AppVue>,
}

#[utoipa::path(get, path = "/v1/admin/catalog", tag = "admin",
    summary = "Applications, types de ressource et permissions déclarés (obsolètes comprises)",
    responses((status = 200, body = Catalogue), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn catalogue(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Catalogue>> {
    caller.exiger_admin(&state).await?;
    let apps: Vec<(String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT key, name, description, seen_at FROM apps ORDER BY key = 'aegis', name",
    )
    .fetch_all(&state.db)
    .await?;
    let types: Vec<TypeVue> = sqlx::query_as(
        "SELECT app, key, name, list_url <> '' AS listable FROM resource_types ORDER BY app, key",
    )
    .fetch_all(&state.db)
    .await?;
    let perms: Vec<PermissionVue> = sqlx::query_as(
        "SELECT p.name, p.app, p.key, p.category, p.label, p.description, p.resource_type, p.open_default, p.obsolete_at,
                (SELECT count(*) FROM rules r WHERE r.permission = p.name) AS rules_count
         FROM permissions p ORDER BY p.app, p.obsolete_at IS NOT NULL, p.position, p.key",
    )
    .fetch_all(&state.db)
    .await?;
    let mut types = types;
    let mut perms = perms;
    let apps = apps
        .into_iter()
        .map(|(key, name, description, seen_at)| {
            let (t, reste): (Vec<_>, Vec<_>) = types.drain(..).partition(|x| x.app == key);
            types = reste;
            let (p, reste): (Vec<_>, Vec<_>) = perms.drain(..).partition(|x| x.app == key);
            perms = reste;
            AppVue {
                key,
                name,
                description,
                seen_at,
                resource_types: t,
                permissions: p,
            }
        })
        .collect();
    Ok(Json(Catalogue { apps }))
}

#[derive(Debug, Deserialize, IntoParams)]
struct FiltreRessources {
    q: Option<String>,
    ids: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Ressource {
    pub id: String,
    pub name: String,
}

#[utoipa::path(get, path = "/v1/admin/resources/{app}/{type}", tag = "admin", params(FiltreRessources,
        ("app" = String, Path), ("type" = String, Path)),
    summary = "Instances d'un type de ressource (relais vers l'application propriétaire)",
    responses((status = 200, body = Vec<Ressource>), (status = 404, body = ErrorBody), (status = 502, body = ErrorBody)))]
async fn ressources(
    State(state): State<AppState>,
    caller: Caller,
    Path((app, type_)): Path<(String, String)>,
    Query(f): Query<FiltreRessources>,
) -> ApiResult<Json<Vec<Ressource>>> {
    caller.exiger_admin(&state).await?;
    let url: Option<String> =
        sqlx::query_scalar("SELECT list_url FROM resource_types WHERE app = $1 AND key = $2")
            .bind(&app)
            .bind(&type_)
            .fetch_optional(&state.db)
            .await?;
    let url = url
        .filter(|u| !u.is_empty())
        .ok_or(ApiError::NotFound("liste de ressources"))?;
    if !state.settings.ressources.hote_autorise(&url) {
        return Err(ApiError::Upstream(
            "hôte de la liste de ressources non autorisé".into(),
        ));
    }
    let jeton = state
        .settings
        .services
        .jeton(&app)
        .ok_or(ApiError::NotFound("jeton de l'application"))?;
    let mut req = state.http.get(&url).bearer_auth(jeton);
    if let Some(q) = f.q.as_deref().filter(|q| !q.is_empty()) {
        req = req.query(&[("q", q)]);
    }
    if let Some(ids) = f.ids.as_deref().filter(|i| !i.is_empty()) {
        req = req.query(&[("ids", ids)]);
    }
    let res = req
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("{app} injoignable : {e}")))?;
    if !res.status().is_success() {
        return Err(ApiError::Upstream(format!(
            "{app} a répondu {}",
            res.status()
        )));
    }
    let octets = res
        .bytes()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;
    if octets.len() > 1 << 20 {
        return Err(ApiError::Upstream("réponse trop grosse".into()));
    }
    let mut liste: Vec<Ressource> = serde_json::from_slice(&octets)
        .map_err(|e| ApiError::Upstream(format!("réponse illisible : {e}")))?;
    liste.truncate(200);
    Ok(Json(liste))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Detenteur {
    pub rule: RegleVue,
    pub subject_label: Option<String>,
    pub account: Option<Compte>,
}

#[utoipa::path(get, path = "/v1/admin/permissions/{name}/holders", tag = "admin", params(("name" = String, Path)),
    summary = "Règles qui mentionnent cette permission (rôles, groupes, comptes)",
    responses((status = 200, body = Vec<Detenteur>), (status = 403, body = ErrorBody)))]
async fn detenteurs(
    State(state): State<AppState>,
    caller: Caller,
    Path(nom): Path<String>,
) -> ApiResult<Json<Vec<Detenteur>>> {
    caller.exiger_admin(&state).await?;
    let regles: Vec<RegleVue> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_REGLES} WHERE r.permission = $1 ORDER BY r.subject_kind, r.created_at"
    )))
    .bind(&nom)
    .fetch_all(&state.db)
    .await?;
    let noms: std::collections::HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text, name FROM roles UNION ALL SELECT id::text, name FROM groups",
    )
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .collect();
    let subs = regles
        .iter()
        .filter(|r| r.subject_kind == "account")
        .map(|r| r.subject_id.clone())
        .collect();
    let mut comptes = libelles(&state.db, subs).await?;
    Ok(Json(
        regles
            .into_iter()
            .map(|r| {
                let account = (r.subject_kind == "account")
                    .then(|| comptes.remove(&r.subject_id))
                    .flatten();
                let subject_label = match r.subject_kind.as_str() {
                    "account" => account.as_ref().and_then(|c| c.email.clone()),
                    _ => noms.get(&r.subject_id).cloned(),
                };
                Detenteur {
                    rule: r,
                    subject_label,
                    account,
                }
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Purge {
    pub removed: Vec<String>,
}

#[utoipa::path(post, path = "/v1/admin/permissions/purge-obsolete", tag = "admin",
    summary = "Supprime les permissions obsolètes et leurs règles",
    responses((status = 200, body = Purge), (status = 403, body = ErrorBody)))]
async fn purger(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Purge>> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let removed: Vec<String> =
        sqlx::query_scalar("DELETE FROM permissions WHERE obsolete_at IS NOT NULL RETURNING name")
            .fetch_all(&mut *tx)
            .await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "permissions.purge",
            target_kind: "permission",
            target_id: "*",
            subject: None,
            detail: json!({"removed": removed}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Tout).await;
    Ok(Json(Purge { removed }))
}
