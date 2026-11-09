use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{LiaisonVue, RegleVue, SQL_LIAISONS, SQL_REGLES};
use crate::{
    AppState,
    annuaire::{self, Compte},
    auth::Caller,
    cache::Portee,
    error::{ApiError, ApiResult, ErrorBody},
    journal,
    moteur::{Decision, Explication, decider, expliquer},
    stockage,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(lister))
        .routes(routes!(voir))
        .routes(routes!(effectifs))
        .routes(routes!(simuler))
        .routes(routes!(copier))
        .routes(routes!(purger))
}

#[derive(Debug, Deserialize, IntoParams)]
struct Recherche {
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct CompteResume {
    pub sub: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub in_keycloak: bool,
    pub roles_count: i64,
    pub groups_count: i64,
    pub allows_count: i64,
    pub denies_count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PageComptes {
    pub accounts: Vec<CompteResume>,
    pub total: i64,
    pub from_keycloak: usize,
}

const SQL_LISTE: &str = "SELECT a.sub, a.email, a.display_name, a.last_seen_at, a.in_keycloak,
    (SELECT count(*) FROM role_bindings b WHERE b.subject_kind = 'account' AND b.subject_id = a.sub) AS roles_count,
    (SELECT count(*) FROM group_members m WHERE m.sub = a.sub) AS groups_count,
    (SELECT count(*) FROM rules r WHERE r.subject_kind = 'account' AND r.subject_id = a.sub AND r.effect = 'allow') AS allows_count,
    (SELECT count(*) FROM rules r WHERE r.subject_kind = 'account' AND r.subject_id = a.sub AND r.effect = 'deny') AS denies_count,
    count(*) OVER () AS total
    FROM accounts a
    WHERE $1 = '' OR a.sub = $1 OR lower(a.email) LIKE '%' || $2 || '%' OR lower(a.display_name) LIKE '%' || $2 || '%'
    ORDER BY a.email IS NULL, lower(a.email), a.sub LIMIT $3 OFFSET $4";

fn echapper_like(s: &str) -> String {
    s.to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

async fn chercher(
    state: &AppState,
    q: &str,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<CompteResume>, i64)> {
    use sqlx::Row;
    let lignes = sqlx::query(SQL_LISTE)
        .bind(q)
        .bind(echapper_like(q))
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;
    let total = lignes
        .first()
        .map_or(Ok(0), |l| l.try_get::<i64, _>("total"))?;
    let comptes = lignes
        .iter()
        .map(|l| {
            Ok(CompteResume {
                sub: l.try_get("sub")?,
                email: l.try_get("email")?,
                display_name: l.try_get("display_name")?,
                last_seen_at: l.try_get("last_seen_at")?,
                in_keycloak: l.try_get("in_keycloak")?,
                roles_count: l.try_get("roles_count")?,
                groups_count: l.try_get("groups_count")?,
                allows_count: l.try_get("allows_count")?,
                denies_count: l.try_get("denies_count")?,
            })
        })
        .collect::<sqlx::Result<Vec<_>>>()?;
    Ok((comptes, total))
}

#[utoipa::path(get, path = "/v1/admin/accounts", tag = "admin", params(Recherche),
    summary = "Annuaire des comptes : recherche par e-mail, nom ou sub",
    description = "Si rien n'est trouvé localement, la recherche interroge Keycloak (via la passerelle) par début d'e-mail \
et enregistre les comptes trouvés.",
    responses((status = 200, body = PageComptes), (status = 403, body = ErrorBody)))]
async fn lister(
    State(state): State<AppState>,
    caller: Caller,
    Query(r): Query<Recherche>,
) -> ApiResult<Json<PageComptes>> {
    caller.exiger_admin(&state).await?;
    let q = r.q.unwrap_or_default().trim().to_string();
    let limit = r.limit.unwrap_or(50).clamp(1, 200);
    let offset = r.offset.unwrap_or(0).max(0);
    let (mut accounts, mut total) = chercher(&state, &q, limit, offset).await?;
    let mut from_keycloak = 0;
    if accounts.is_empty() && !q.is_empty() && offset == 0 {
        from_keycloak = annuaire::chercher_keycloak(&state, &q)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("recherche Keycloak : {e:?}");
                0
            });
        if from_keycloak > 0 {
            (accounts, total) = chercher(&state, &q, limit, offset).await?;
        }
    }
    Ok(Json(PageComptes {
        accounts,
        total,
        from_keycloak,
    }))
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct Appartenance {
    pub group_id: Uuid,
    pub name: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompteDetail {
    pub account: Compte,
    pub bindings: Vec<LiaisonVue>,
    pub groups: Vec<Appartenance>,
    pub rules: Vec<RegleVue>,
    pub group_bindings: Vec<LiaisonVue>,
}

pub async fn detail(state: &AppState, sub: &str) -> ApiResult<CompteDetail> {
    let account = annuaire::un(&state.db, sub).await?;
    let bindings = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_LIAISONS} WHERE b.subject_kind = 'account' AND b.subject_id = $1 ORDER BY ro.name"
    )))
    .bind(sub)
    .fetch_all(&state.db)
    .await?;
    let groups = sqlx::query_as(
        "SELECT g.id AS group_id, g.name, m.expires_at, m.added_by, m.added_at FROM group_members m
         JOIN groups g ON g.id = m.group_id WHERE m.sub = $1 ORDER BY lower(g.name)",
    )
    .bind(sub)
    .fetch_all(&state.db)
    .await?;
    let rules = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_REGLES} WHERE r.subject_kind = 'account' AND r.subject_id = $1 ORDER BY p.app, p.position, r.resource"
    )))
    .bind(sub)
    .fetch_all(&state.db)
    .await?;
    let group_bindings = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SQL_LIAISONS} WHERE b.subject_kind = 'group' AND b.subject_id IN
         (SELECT group_id::text FROM group_members WHERE sub = $1) ORDER BY ro.name"
    )))
    .bind(sub)
    .fetch_all(&state.db)
    .await?;
    Ok(CompteDetail {
        account,
        bindings,
        groups,
        rules,
        group_bindings,
    })
}

#[utoipa::path(get, path = "/v1/admin/accounts/{sub}", tag = "admin", params(("sub" = String, Path)),
    summary = "Fiche d'un compte : rôles, groupes, règles directes",
    responses((status = 200, body = CompteDetail), (status = 403, body = ErrorBody)))]
async fn voir(
    State(state): State<AppState>,
    caller: Caller,
    Path(sub): Path<String>,
) -> ApiResult<Json<CompteDetail>> {
    caller.exiger_admin(&state).await?;
    Ok(Json(detail(&state, &sub).await?))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EffectifsExpliques {
    pub account: Compte,
    pub permissions: Vec<Explication>,
}

#[utoipa::path(get, path = "/v1/admin/accounts/{sub}/effective", tag = "admin", params(("sub" = String, Path)),
    summary = "Droits effectifs expliqués",
    description = "Pour chaque permission active du catalogue : l'état (autorisé, partiel, refusé, non défini), les ressources \
autorisées et refusées, et les règles qui y mènent avec leur origine (Tout le monde, rôle, groupe, règle directe) et leur expiration.",
    responses((status = 200, body = EffectifsExpliques), (status = 403, body = ErrorBody)))]
async fn effectifs(
    State(state): State<AppState>,
    caller: Caller,
    Path(sub): Path<String>,
) -> ApiResult<Json<EffectifsExpliques>> {
    caller.exiger_admin(&state).await?;
    let regles = stockage::regles(&state.db, &sub).await?;
    let noms: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM permissions WHERE obsolete_at IS NULL ORDER BY app = 'aegis', app, position, key",
    )
    .fetch_all(&state.db)
    .await?;
    let maintenant = Utc::now();
    Ok(Json(EffectifsExpliques {
        account: annuaire::un(&state.db, &sub).await?,
        permissions: noms
            .iter()
            .map(|n| expliquer(n, &regles, maintenant))
            .collect(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Simulation {
    pub sub: String,
    pub permission: String,
    #[serde(default)]
    pub resource: Option<String>,
}

#[utoipa::path(post, path = "/v1/admin/simulate", tag = "admin", request_body = Simulation,
    summary = "Ce compte peut-il faire ceci (sur cette ressource) ? Décision et raison",
    responses((status = 200, body = Decision), (status = 403, body = ErrorBody)))]
async fn simuler(
    State(state): State<AppState>,
    caller: Caller,
    Json(s): Json<Simulation>,
) -> ApiResult<Json<Decision>> {
    caller.exiger_admin(&state).await?;
    let regles = stockage::regles(&state.db, &s.sub).await?;
    let res = s
        .resource
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty());
    Ok(Json(decider(&s.permission, res, &regles, Utc::now())))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModeCopie {
    Ajouter,
    Remplacer,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Copie {
    pub from: String,
    pub mode: ModeCopie,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResultatCopie {
    pub bindings: i64,
    pub groups: i64,
    pub rules: i64,
    pub removed: i64,
    pub applied: bool,
}

#[utoipa::path(post, path = "/v1/admin/accounts/{sub}/copy", tag = "admin", params(("sub" = String, Path)), request_body = Copie,
    summary = "Copie les rôles, groupes et règles directes d'un autre compte",
    description = "`ajouter` garde l'existant ; `remplacer` supprime d'abord tous les droits du compte. `dry_run` donne les nombres sans rien écrire.",
    responses((status = 200, body = ResultatCopie), (status = 400, body = ErrorBody)))]
async fn copier(
    State(state): State<AppState>,
    caller: Caller,
    Path(sub): Path<String>,
    Json(c): Json<Copie>,
) -> ApiResult<Json<ResultatCopie>> {
    let acteur = caller.exiger_admin(&state).await?;
    if c.from == sub {
        return Err(ApiError::BadRequest("source et cible identiques".into()));
    }
    let mut tx = state.db.begin().await?;
    annuaire::assurer(&mut tx, &sub).await?;
    let removed = if c.mode == ModeCopie::Remplacer {
        effacer(&mut tx, &sub).await?
    } else {
        0
    };
    let bindings = sqlx::query(
        "INSERT INTO role_bindings (role_id, subject_kind, subject_id, scope_type, scope_id, expires_at, granted_by)
         SELECT role_id, 'account', $2, scope_type, scope_id, expires_at, $3 FROM role_bindings
         WHERE subject_kind = 'account' AND subject_id = $1 AND (expires_at IS NULL OR expires_at > now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&c.from)
    .bind(&sub)
    .bind(&acteur)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    let groups = sqlx::query(
        "INSERT INTO group_members (group_id, sub, expires_at, added_by)
         SELECT group_id, $2, expires_at, $3 FROM group_members
         WHERE sub = $1 AND (expires_at IS NULL OR expires_at > now()) ON CONFLICT DO NOTHING",
    )
    .bind(&c.from)
    .bind(&sub)
    .bind(&acteur)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    let rules = sqlx::query(
        "INSERT INTO rules (subject_kind, subject_id, permission, resource, effect, expires_at, note, granted_by)
         SELECT 'account', $2, permission, resource, effect, expires_at, note, $3 FROM rules
         WHERE subject_kind = 'account' AND subject_id = $1 AND (expires_at IS NULL OR expires_at > now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&c.from)
    .bind(&sub)
    .bind(&acteur)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    let resultat = ResultatCopie {
        bindings,
        groups,
        rules,
        removed,
        applied: !c.dry_run,
    };
    if c.dry_run {
        tx.rollback().await?;
        return Ok(Json(resultat));
    }
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "account.copy",
            target_kind: "account",
            target_id: &sub,
            subject: Some(&sub),
            detail: json!({"from": c.from, "mode": match c.mode { ModeCopie::Ajouter => "ajouter", ModeCopie::Remplacer => "remplacer" },
                "bindings": bindings, "groups": groups, "rules": rules, "removed": removed}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Compte(&sub)).await;
    Ok(Json(resultat))
}

async fn effacer(tx: &mut sqlx::PgConnection, sub: &str) -> sqlx::Result<i64> {
    let mut n = 0;
    for sql in [
        "DELETE FROM role_bindings WHERE subject_kind = 'account' AND subject_id = $1",
        "DELETE FROM group_members WHERE sub = $1",
        "DELETE FROM rules WHERE subject_kind = 'account' AND subject_id = $1",
    ] {
        n += sqlx::query(sql)
            .bind(sub)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;
    }
    Ok(n)
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Effacement {
    pub removed: i64,
}

#[utoipa::path(delete, path = "/v1/admin/accounts/{sub}/grants", tag = "admin", params(("sub" = String, Path)),
    summary = "Retire au compte tous ses rôles, groupes et règles directes",
    responses((status = 200, body = Effacement), (status = 403, body = ErrorBody)))]
async fn purger(
    State(state): State<AppState>,
    caller: Caller,
    Path(sub): Path<String>,
) -> ApiResult<Json<Effacement>> {
    let acteur = caller.exiger_admin(&state).await?;
    let mut tx = state.db.begin().await?;
    let removed = effacer(&mut tx, &sub).await?;
    journal::ecrire(
        &mut tx,
        &acteur,
        journal::Entree {
            action: "account.purge",
            target_kind: "account",
            target_id: &sub,
            subject: Some(&sub),
            detail: json!({"removed": removed}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Compte(&sub)).await;
    Ok(Json(Effacement { removed }))
}
