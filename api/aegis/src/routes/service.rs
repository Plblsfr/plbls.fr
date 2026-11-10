use std::collections::BTreeSet;

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState, annuaire,
    auth::Caller,
    cache::Portee,
    error::{ApiError, ApiResult, ErrorBody},
    journal,
    moteur::Effectifs,
    settings::Ressources,
    stockage,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(manifeste))
        .routes(routes!(effectifs))
        .routes(routes!(verifier))
        .routes(routes!(mes_droits))
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TypeRessource {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub list_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PermissionDeclaree {
    pub key: String,
    #[serde(default)]
    pub category: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub open_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Manifeste {
    pub app: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub resource_types: Vec<TypeRessource>,
    pub permissions: Vec<PermissionDeclaree>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResultatManifeste {
    pub changed: bool,
    pub added: Vec<String>,
    pub obsolete: Vec<String>,
    pub opened_to_everyone: Vec<String>,
}

fn cle_valide(k: &str, max: usize, points: bool) -> bool {
    let mut c = k.chars();
    c.next().is_some_and(|p| p.is_ascii_lowercase())
        && k.len() <= max
        && k.chars().all(|x| {
            x.is_ascii_lowercase()
                || x.is_ascii_digit()
                || x == '-'
                || (points && (x == '.' || x == '_'))
        })
}

fn valider(m: &Manifeste, ressources: &Ressources) -> Result<(), ApiError> {
    if !cle_valide(&m.app, 40, false) {
        return Err(ApiError::BadRequest("clé d'application invalide".into()));
    }
    let types: BTreeSet<&str> = m.resource_types.iter().map(|t| t.key.as_str()).collect();
    if types.len() != m.resource_types.len() {
        return Err(ApiError::BadRequest("type de ressource en double".into()));
    }
    for t in &m.resource_types {
        if !cle_valide(&t.key, 40, true) || t.name.trim().is_empty() {
            return Err(ApiError::BadRequest(format!(
                "type de ressource invalide : {}",
                t.key
            )));
        }
        if !t.list_url.is_empty() && !ressources.hote_autorise(&t.list_url) {
            return Err(ApiError::BadRequest(format!(
                "list_url du type {} refusée : hôte absent de AEGIS__RESSOURCES__ALLOWED_HOSTS",
                t.key
            )));
        }
    }
    let mut vues = BTreeSet::new();
    for p in &m.permissions {
        if !cle_valide(&p.key, 60, true) || p.label.trim().is_empty() {
            return Err(ApiError::BadRequest(format!(
                "permission invalide : {}",
                p.key
            )));
        }
        if !vues.insert(p.key.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "permission en double : {}",
                p.key
            )));
        }
        if let Some(t) = &p.resource_type
            && !types.contains(t.as_str())
        {
            return Err(ApiError::BadRequest(format!(
                "type de ressource non déclaré : {t}"
            )));
        }
    }
    Ok(())
}

#[utoipa::path(put, path = "/v1/manifest", tag = "service", request_body = Manifeste,
    summary = "Déclare les permissions de l'application du jeton",
    description = "Appelé par chaque application à son démarrage puis régulièrement. Idempotent : un manifeste inchangé n'écrit rien. \
Une permission absente devient obsolète (ses règles sont gardées mais ignorées). À la première apparition d'une permission \
`open_default`, une règle `allow *` est ajoutée au rôle « Tout le monde ». Un `list_url` dont l'hôte n'est pas dans \
`AEGIS__RESSOURCES__ALLOWED_HOSTS` (vide = aucun) est refusé.",
    security(("serviceToken" = [])),
    responses((status = 200, body = ResultatManifeste), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn manifeste(
    State(state): State<AppState>,
    caller: Caller,
    Json(m): Json<Manifeste>,
) -> ApiResult<Json<ResultatManifeste>> {
    let service = caller.service()?;
    if service != m.app || m.app == "aegis" {
        return Err(ApiError::Forbidden);
    }
    valider(&m, &state.settings.ressources)?;
    let canon = serde_json::to_vec(&m).map_err(|e| ApiError::Internal(e.to_string()))?;
    let sha: String = Sha256::digest(&canon)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut tx = state.db.begin().await?;
    let ancien: Option<String> =
        sqlx::query_scalar("SELECT manifest_sha FROM apps WHERE key = $1 FOR UPDATE")
            .bind(&m.app)
            .fetch_optional(&mut *tx)
            .await?;
    if ancien.as_deref() == Some(sha.as_str()) {
        sqlx::query("UPDATE apps SET seen_at = now() WHERE key = $1")
            .bind(&m.app)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(Json(ResultatManifeste {
            changed: false,
            added: vec![],
            obsolete: vec![],
            opened_to_everyone: vec![],
        }));
    }
    sqlx::query(
        "INSERT INTO apps (key, name, description, manifest, manifest_sha, seen_at) VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (key) DO UPDATE SET name = EXCLUDED.name, description = EXCLUDED.description,
             manifest = EXCLUDED.manifest, manifest_sha = EXCLUDED.manifest_sha, seen_at = now()",
    )
    .bind(&m.app)
    .bind(&m.name)
    .bind(&m.description)
    .bind(serde_json::to_value(&m).unwrap_or_default())
    .bind(&sha)
    .execute(&mut *tx)
    .await?;
    for t in &m.resource_types {
        sqlx::query(
            "INSERT INTO resource_types (app, key, name, list_url) VALUES ($1, $2, $3, $4)
             ON CONFLICT (app, key) DO UPDATE SET name = EXCLUDED.name, list_url = EXCLUDED.list_url",
        )
        .bind(&m.app)
        .bind(&t.key)
        .bind(&t.name)
        .bind(&t.list_url)
        .execute(&mut *tx)
        .await?;
    }
    let (mut added, mut ouvertes) = (vec![], vec![]);
    for (i, p) in m.permissions.iter().enumerate() {
        let nom = format!("{}:{}", m.app, p.key);
        let ligne = sqlx::query(
            "INSERT INTO permissions (name, app, key, category, label, description, resource_type, open_default, position)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (name) DO UPDATE SET category = EXCLUDED.category, label = EXCLUDED.label,
                 description = EXCLUDED.description, resource_type = EXCLUDED.resource_type,
                 open_default = EXCLUDED.open_default, position = EXCLUDED.position, obsolete_at = NULL
             RETURNING (xmax = 0) AS nouvelle",
        )
        .bind(&nom)
        .bind(&m.app)
        .bind(&p.key)
        .bind(&p.category)
        .bind(&p.label)
        .bind(&p.description)
        .bind(&p.resource_type)
        .bind(p.open_default)
        .bind(i as i32)
        .fetch_one(&mut *tx)
        .await?;
        if ligne.try_get::<bool, _>("nouvelle")? {
            added.push(nom.clone());
            if p.open_default {
                sqlx::query(
                    "INSERT INTO rules (subject_kind, subject_id, permission, resource, effect, granted_by)
                     SELECT 'role', id::text, $1, '*', 'allow', $2 FROM roles WHERE system = 'tout-le-monde'
                     ON CONFLICT DO NOTHING",
                )
                .bind(&nom)
                .bind(format!("manifeste:{}", m.app))
                .execute(&mut *tx)
                .await?;
                ouvertes.push(nom);
            }
        }
    }
    let cles: Vec<String> = m.permissions.iter().map(|p| p.key.clone()).collect();
    let obsolete: Vec<String> = sqlx::query_scalar(
        "UPDATE permissions SET obsolete_at = now() WHERE app = $1 AND obsolete_at IS NULL AND key <> ALL($2) RETURNING name",
    )
    .bind(&m.app)
    .bind(&cles)
    .fetch_all(&mut *tx)
    .await?;
    let types: Vec<String> = m.resource_types.iter().map(|t| t.key.clone()).collect();
    sqlx::query(
        "DELETE FROM resource_types t WHERE t.app = $1 AND t.key <> ALL($2)
         AND NOT EXISTS (SELECT 1 FROM permissions p WHERE p.app = t.app AND p.resource_type = t.key)",
    )
    .bind(&m.app)
    .bind(&types)
    .execute(&mut *tx)
    .await?;
    journal::ecrire(
        &mut tx,
        &caller.actor(),
        journal::Entree {
            action: "manifest",
            target_kind: "app",
            target_id: &m.app,
            subject: None,
            detail: json!({"added": added, "obsolete": obsolete, "opened_to_everyone": ouvertes}),
        },
    )
    .await?;
    tx.commit().await?;
    state.cache.invalider(Portee::Tout).await;
    Ok(Json(ResultatManifeste {
        changed: true,
        added,
        obsolete,
        opened_to_everyone: ouvertes,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DemandeEffectifs {
    pub sub: String,
    pub app: Option<String>,
}

const APP_AEGIS: &str = "aegis";

fn apps_lisibles<'a>(service: &'a str, app: Option<&'a str>) -> Result<Vec<&'a str>, ApiError> {
    match app {
        None => Ok(vec![service, APP_AEGIS]),
        Some(a) if a == service || a == APP_AEGIS => Ok(vec![a]),
        Some(_) => Err(ApiError::Forbidden),
    }
}

#[utoipa::path(post, path = "/v1/effective", tag = "service", request_body = DemandeEffectifs,
    summary = "Droits effectifs d'un compte",
    description = "Forme calculée : pour chaque permission accordée, les ressources autorisées (`*` = toutes) et refusées. \
Le refus l'emporte. `valid_until` borne la durée de mise en cache. Seules les permissions de l'application du jeton et celles \
d'aegis (`aegis:admin`) sont rendues ; `app` restreint à l'une des deux, toute autre valeur est refusée (403). \
L'appel note la dernière activité du compte ; l'e-mail et le nom de l'annuaire ne viennent que de la synchronisation Keycloak.",
    security(("serviceToken" = [])),
    responses((status = 200, body = Effectifs), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn effectifs(
    State(state): State<AppState>,
    caller: Caller,
    Json(d): Json<DemandeEffectifs>,
) -> ApiResult<Json<Effectifs>> {
    let service = caller.service()?;
    if d.sub.trim().is_empty() {
        return Err(ApiError::BadRequest("sub vide".into()));
    }
    let apps = apps_lisibles(service, d.app.as_deref())?;
    let (db, sub) = (state.db.clone(), d.sub.clone());
    tokio::spawn(async move {
        if let Err(e) = annuaire::vu(&db, &sub, None, None).await {
            tracing::warn!("annuaire : {e}");
        }
    });
    let e = stockage::effectifs(&state, &d.sub).await?;
    Ok(Json(e.garder_apps(&apps)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DemandeVerification {
    pub sub: String,
    pub permission: String,
    pub resource: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Verdict {
    pub allowed: bool,
}

#[utoipa::path(post, path = "/v1/check", tag = "service", request_body = DemandeVerification,
    summary = "Le compte a-t-il cette permission (sur cette ressource) ?",
    security(("serviceToken" = [])),
    responses((status = 200, body = Verdict), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn verifier(
    State(state): State<AppState>,
    caller: Caller,
    Json(d): Json<DemandeVerification>,
) -> ApiResult<Json<Verdict>> {
    caller.service()?;
    let e = stockage::effectifs(&state, &d.sub).await?;
    Ok(Json(Verdict {
        allowed: e.autorise(&d.permission, d.resource.as_deref()),
    }))
}

#[derive(Deserialize, IntoParams)]
struct FiltreApp {
    app: Option<String>,
}

#[utoipa::path(get, path = "/v1/me/effective", tag = "utilisateur", params(FiltreApp),
    summary = "Droits effectifs du compte connecté",
    security(("gatewayIdentity" = [])),
    responses((status = 200, body = Effectifs), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn mes_droits(
    State(state): State<AppState>,
    caller: Caller,
    Query(f): Query<FiltreApp>,
) -> ApiResult<Json<Effectifs>> {
    let Caller::User { sub, .. } = &caller else {
        return Err(ApiError::Forbidden);
    };
    let e = stockage::effectifs(&state, sub).await?;
    Ok(Json(e.filtrer(f.app.as_deref())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Manifeste {
        serde_json::from_value(json!({
            "app": "estompe", "name": "Estompe",
            "resource_types": [{"key": "server", "name": "Serveur"}],
            "permissions": [
                {"key": "gifs.use", "label": "GIF", "open_default": true},
                {"key": "vocal.screen", "label": "Écran", "resource_type": "server"}
            ]
        }))
        .unwrap()
    }

    fn hotes() -> Ressources {
        Ressources {
            allowed_hosts: "estompe-api".into(),
        }
    }

    #[test]
    fn list_url_limitee_aux_hotes_autorises() {
        let mut x = m();
        x.resource_types[0].list_url =
            "http://estompe-api:8080/internal/aegis/resources/server".into();
        assert!(valider(&x, &hotes()).is_ok());
        assert!(
            valider(&x, &Ressources::default()).is_err(),
            "liste vide : refus"
        );
        x.resource_types[0].list_url = "http://169.254.169.254/latest/meta-data".into();
        assert!(valider(&x, &hotes()).is_err());
        x.resource_types[0].list_url = "file:///etc/passwd".into();
        assert!(valider(&x, &hotes()).is_err());
        assert!(
            valider(&m(), &Ressources::default()).is_ok(),
            "sans list_url"
        );
    }

    #[test]
    fn un_service_ne_lit_que_ses_droits_et_ceux_d_aegis() {
        assert_eq!(
            apps_lisibles("estompe", None).unwrap(),
            ["estompe", "aegis"]
        );
        assert_eq!(apps_lisibles("pli", Some("pli")).unwrap(), ["pli"]);
        assert_eq!(apps_lisibles("pli", Some("aegis")).unwrap(), ["aegis"]);
        assert!(matches!(
            apps_lisibles("pli", Some("estompe")),
            Err(ApiError::Forbidden)
        ));
    }

    #[test]
    fn validation_du_manifeste() {
        assert!(valider(&m(), &hotes()).is_ok());
        let mut x = m();
        x.permissions[1].resource_type = Some("inconnu".into());
        assert!(valider(&x, &hotes()).is_err());
        let mut x = m();
        x.permissions.push(x.permissions[0].clone());
        assert!(valider(&x, &hotes()).is_err());
        let mut x = m();
        x.app = "Estompe".into();
        assert!(valider(&x, &hotes()).is_err());
        let mut x = m();
        x.permissions[0].key = "a b".into();
        assert!(valider(&x, &hotes()).is_err());
    }
}
