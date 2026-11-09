use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    LiaisonVue, RegleSaisie, RegleVue, SQL_LIAISONS, SQL_REGLES, Sujet, portee, valider_expiration,
    valider_regle, valider_sujet,
};
use crate::{
    AppState,
    auth::Caller,
    error::{ApiError, ApiResult, ErrorBody},
    journal,
    moteur::Effet,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(lister_liaisons, lier))
        .routes(routes!(delier))
        .routes(routes!(lister_regles, creer_regle))
        .routes(routes!(modifier_regle, supprimer_regle))
}

#[derive(Debug, Deserialize, IntoParams)]
struct FiltreSujet {
    subject_kind: Option<Sujet>,
    subject_id: Option<String>,
    role_id: Option<Uuid>,
}

#[utoipa::path(get, path = "/v1/admin/bindings", tag = "admin", params(FiltreSujet),
    summary = "Liaisons rôle → compte ou groupe",
    responses((status = 200, body = Vec<LiaisonVue>), (status = 403, body = ErrorBody)))]
async fn lister_liaisons(
    State(state): State<AppState>,
    caller: Caller,
    Query(f): Query<FiltreSujet>,
) -> ApiResult<Json<Vec<LiaisonVue>>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SQL_LIAISONS} WHERE ($1::text IS NULL OR b.subject_kind = $1) AND ($2::text IS NULL OR b.subject_id = $2)
             AND ($3::uuid IS NULL OR b.role_id = $3) ORDER BY b.created_at DESC LIMIT 500"
        )))
        .bind(f.subject_kind.map(Sujet::texte))
        .bind(f.subject_id)
        .bind(f.role_id)
        .fetch_all(&state.db)
        .await?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LiaisonSaisie {
    pub role_id: Uuid,
    pub subject_kind: Sujet,
    pub subject_id: String,
    #[serde(default)]
    pub scope_type: Option<String>,
    #[serde(default)]
    pub scope_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[utoipa::path(post, path = "/v1/admin/bindings", tag = "admin", request_body = LiaisonSaisie,
    summary = "Donne un rôle à un compte ou à un groupe",
    description = "Avec une portée (`scope_type` = `app:type`, `scope_id`), seules les règles du rôle portant sur ce type de \
ressource s'appliquent, restreintes à cette ressource. Le rôle « Tout le monde » ne se lie pas : il s'applique à tous.",
    responses((status = 201, body = LiaisonVue), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn lier(
    State(state): State<AppState>,
    caller: Caller,
    Json(l): Json<LiaisonSaisie>,
) -> ApiResult<(StatusCode, Json<LiaisonVue>)> {
    let acteur = caller.exiger_admin(&state).await?;
    if l.subject_kind == Sujet::Role {
        return Err(ApiError::BadRequest(
            "un rôle se lie à un compte ou à un groupe".into(),
        ));
    }
    valider_expiration(l.expires_at)?;
    let portee_ok = match (&l.scope_type, &l.scope_id) {
        (None, None) => true,
        (Some(t), Some(id)) => !id.trim().is_empty() && t.contains(':'),
        _ => false,
    };
    if !portee_ok {
        return Err(ApiError::BadRequest("portée incomplète".into()));
    }
    let mut tx = state.db.begin().await?;
    let systeme: Option<Option<String>> =
        sqlx::query_scalar("SELECT system FROM roles WHERE id = $1")
            .bind(l.role_id)
            .fetch_optional(&mut *tx)
            .await?;
    match systeme {
        None => return Err(ApiError::NotFound("rôle")),
        Some(Some(s)) if s == "tout-le-monde" => {
            return Err(ApiError::BadRequest(
                "« Tout le monde » s'applique déjà à tous".into(),
            ));
        }
        _ => {}
    }
    if let Some(t) = &l.scope_type {
        let existe: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM resource_types WHERE app || ':' || key = $1)",
        )
        .bind(t)
        .fetch_one(&mut *tx)
        .await?;
        if !existe {
            return Err(ApiError::BadRequest(format!(
                "type de ressource inconnu : {t}"
            )));
        }
    }
    valider_sujet(&mut tx, l.subject_kind, &l.subject_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO role_bindings (role_id, subject_kind, subject_id, scope_type, scope_id, expires_at, granted_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(l.role_id)
    .bind(l.subject_kind.texte())
    .bind(&l.subject_id)
    .bind(&l.scope_type)
    .bind(l.scope_id.as_deref().map(str::trim))
    .bind(l.expires_at)
    .bind(&acteur)
    .fetch_one(&mut *tx)
    .await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "binding.create",
            target_kind: "role",
            target_id: &l.role_id.to_string(),
            subject: (l.subject_kind == Sujet::Account).then_some(l.subject_id.as_str()),
            detail: json!({"binding": id, "subject_kind": l.subject_kind, "subject_id": l.subject_id,
                "scope_type": l.scope_type, "scope_id": l.scope_id, "expires_at": l.expires_at}),
        },
    )
    .await?;
    tx.commit().await?;
    state
        .cache
        .invalider(portee(l.subject_kind, &l.subject_id))
        .await;
    let vue = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_LIAISONS} WHERE b.id = $1"
    )))
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(vue)))
}

#[utoipa::path(delete, path = "/v1/admin/bindings/{id}", tag = "admin", params(("id" = Uuid, Path)),
    summary = "Retire une liaison de rôle", responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delier(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let ligne: Option<(Uuid, String, String)> = sqlx::query_as(
        "DELETE FROM role_bindings WHERE id = $1 RETURNING role_id, subject_kind, subject_id",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (role, kind, sujet) = ligne.ok_or(ApiError::NotFound("liaison"))?;
    let kind = Sujet::depuis(&kind);
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "binding.delete",
            target_kind: "role",
            target_id: &role.to_string(),
            subject: (kind == Sujet::Account).then_some(sujet.as_str()),
            detail: json!({"binding": id, "subject_kind": kind, "subject_id": sujet}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(portee(kind, &sujet)).await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/v1/admin/rules", tag = "admin", params(FiltreSujet),
    summary = "Règles directes (comptes, groupes, rôles)",
    responses((status = 200, body = Vec<RegleVue>), (status = 403, body = ErrorBody)))]
async fn lister_regles(
    State(state): State<AppState>,
    caller: Caller,
    Query(f): Query<FiltreSujet>,
) -> ApiResult<Json<Vec<RegleVue>>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SQL_REGLES} WHERE ($1::text IS NULL OR r.subject_kind = $1) AND ($2::text IS NULL OR r.subject_id = $2)
             ORDER BY r.created_at DESC LIMIT 500"
        )))
        .bind(f.subject_kind.map(Sujet::texte))
        .bind(f.subject_id)
        .fetch_all(&state.db)
        .await?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegleDirecte {
    pub subject_kind: Sujet,
    pub subject_id: String,
    #[serde(flatten)]
    pub rule: RegleSaisie,
}

#[utoipa::path(post, path = "/v1/admin/rules", tag = "admin", request_body = RegleDirecte,
    summary = "Accorde ou refuse une permission à un compte ou à un groupe",
    description = "Remplace la règle existante pour le même sujet, la même permission et la même ressource.",
    responses((status = 201, body = RegleVue), (status = 400, body = ErrorBody)))]
async fn creer_regle(
    State(state): State<AppState>,
    caller: Caller,
    Json(r): Json<RegleDirecte>,
) -> ApiResult<(StatusCode, Json<RegleVue>)> {
    let acteur = caller.exiger_admin(&state).await?;
    if r.subject_kind == Sujet::Role {
        return Err(ApiError::BadRequest(
            "les règles d'un rôle se modifient avec le rôle".into(),
        ));
    }
    let mut tx = state.db.begin().await?;
    valider_regle(&mut tx, &r.rule).await?;
    valider_sujet(&mut tx, r.subject_kind, &r.subject_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO rules (subject_kind, subject_id, permission, resource, effect, expires_at, note, granted_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (subject_kind, subject_id, permission, resource) DO UPDATE SET effect = EXCLUDED.effect,
             expires_at = EXCLUDED.expires_at, note = EXCLUDED.note, granted_by = EXCLUDED.granted_by, created_at = now()
         RETURNING id",
    )
    .bind(r.subject_kind.texte())
    .bind(&r.subject_id)
    .bind(&r.rule.permission)
    .bind(r.rule.resource.trim())
    .bind(r.rule.effect.texte())
    .bind(r.rule.expires_at)
    .bind(&r.rule.note)
    .bind(&acteur)
    .fetch_one(&mut *tx)
    .await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: match r.rule.effect {
                Effet::Allow => "rule.allow",
                Effet::Deny => "rule.deny",
            },
            target_kind: "permission",
            target_id: &r.rule.permission,
            subject: (r.subject_kind == Sujet::Account).then_some(r.subject_id.as_str()),
            detail: json!({"rule": id, "subject_kind": r.subject_kind, "subject_id": r.subject_id,
                "resource": r.rule.resource, "expires_at": r.rule.expires_at, "note": r.rule.note}),
        },
    )
    .await?;
    tx.commit().await?;
    state
        .cache
        .invalider(portee(r.subject_kind, &r.subject_id))
        .await;
    let vue = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SQL_REGLES} WHERE r.id = $1")))
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(vue)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegleModifiee {
    pub effect: Effet,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub note: String,
}

#[utoipa::path(put, path = "/v1/admin/rules/{id}", tag = "admin", params(("id" = Uuid, Path)), request_body = RegleModifiee,
    summary = "Change l'effet, l'expiration ou la note d'une règle directe",
    responses((status = 200, body = RegleVue), (status = 404, body = ErrorBody)))]
async fn modifier_regle(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
    Json(m): Json<RegleModifiee>,
) -> ApiResult<Json<RegleVue>> {
    let acteur = caller.exiger_admin(&state).await?;
    valider_expiration(m.expires_at)?;
    let mut tx = state.db.begin().await?;
    let ligne: Option<(String, String, String)> = sqlx::query_as(
        "UPDATE rules SET effect = $2, expires_at = $3, note = $4 WHERE id = $1 AND subject_kind <> 'role'
         RETURNING subject_kind, subject_id, permission",
    )
    .bind(id)
    .bind(m.effect.texte())
    .bind(m.expires_at)
    .bind(&m.note)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, sujet, perm) = ligne.ok_or(ApiError::NotFound("règle"))?;
    let kind = Sujet::depuis(&kind);
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "rule.update",
            target_kind: "permission",
            target_id: &perm,
            subject: (kind == Sujet::Account).then_some(sujet.as_str()),
            detail: json!({"rule": id, "effect": m.effect, "expires_at": m.expires_at, "note": m.note}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(portee(kind, &sujet)).await;
    Ok(Json(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SQL_REGLES} WHERE r.id = $1")))
            .bind(id)
            .fetch_one(&state.db)
            .await?,
    ))
}

#[utoipa::path(delete, path = "/v1/admin/rules/{id}", tag = "admin", params(("id" = Uuid, Path)),
    summary = "Supprime une règle directe", responses((status = 204), (status = 404, body = ErrorBody)))]
async fn supprimer_regle(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let ligne: Option<(String, String, String, String, String)> = sqlx::query_as(
        "DELETE FROM rules WHERE id = $1 AND subject_kind <> 'role' RETURNING subject_kind, subject_id, permission, resource, effect",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, sujet, perm, res, effet) = ligne.ok_or(ApiError::NotFound("règle"))?;
    let kind = Sujet::depuis(&kind);
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "rule.delete",
            target_kind: "permission",
            target_id: &perm,
            subject: (kind == Sujet::Account).then_some(sujet.as_str()),
            detail: json!({"rule": id, "subject_kind": kind, "subject_id": sujet, "resource": res, "effect": effet}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(portee(kind, &sujet)).await;
    Ok(StatusCode::NO_CONTENT)
}
