use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa_axum::router::OpenApiRouter;

use crate::{catalogue, http::write_err, state::AppState};

const MANIFESTE_PERIODE: Duration = Duration::from_secs(600);
const RECONNEXION: Duration = Duration::from_secs(5);
const TTL_SANS_ABONNEMENT: Duration = Duration::from_secs(5);
const TTL_ABONNE: Duration = Duration::from_secs(30);

pub fn demarrer(state: AppState) {
    if !state.permissions().aegis_actif() {
        return;
    }
    let s = state.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(MANIFESTE_PERIODE);
        loop {
            t.tick().await;
            envoyer_manifeste(&s).await;
        }
    });
    if state.config().aegis.redis_url.is_empty() {
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(e) = ecouter(&state).await {
                tracing::warn!("abonnement aux invalidations d'aegis : {e}");
            }
            state.permissions().ttl(TTL_SANS_ABONNEMENT);
            state.permissions().vider();
            tokio::time::sleep(RECONNEXION).await;
        }
    });
}

pub async fn envoyer_manifeste(state: &AppState) {
    let m = catalogue::manifeste(state.blocs_actifs(), &state.config().aegis.resources_url);
    match state.permissions().envoyer_manifeste(&m).await {
        Ok(r) if r["changed"] == true => tracing::info!(
            ajoutees = %r["added"], obsoletes = %r["obsolete"], "manifeste envoyé à aegis"
        ),
        Ok(_) => {}
        Err(_) => tracing::warn!("aegis injoignable : manifeste non envoyé"),
    }
}

async fn ecouter(state: &AppState) -> redis::RedisResult<()> {
    use futures_util::StreamExt;
    let a = &state.config().aegis;
    let client = redis::Client::open(a.redis_url.as_str())?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub
        .subscribe(format!("{}:invalidation", a.redis_prefix))
        .await?;
    state.permissions().vider();
    state.permissions().ttl(TTL_ABONNE);
    tracing::info!("abonné aux invalidations d'aegis");
    let mut flux = pubsub.into_on_message();
    while let Some(msg) = flux.next().await {
        let Ok(texte) = msg.get_payload::<String>() else {
            continue;
        };
        appliquer(state, &texte).await;
    }
    Ok(())
}

pub async fn appliquer(state: &AppState, message: &str) {
    let Ok(v) = serde_json::from_str::<Value>(message) else {
        return;
    };
    match (v["type"].as_str(), v["sub"].as_str()) {
        (Some("compte"), Some(sub)) => {
            state.permissions().invalider(sub);
            let Ok(pool) = state.pool() else { return };
            let Ok(Some((uid,))) = sqlx::query_as::<_, (String,)>(
                "SELECT id::text FROM users WHERE keycloak_sub = $1",
            )
            .bind(sub)
            .fetch_optional(pool)
            .await
            else {
                return;
            };
            let serveurs: Vec<String> = sqlx::query_scalar(
                "SELECT server_id::text FROM server_members WHERE user_id = $1::uuid",
            )
            .bind(&uid)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            for s in serveurs {
                state.droits_modifies(&s, &uid).await;
            }
        }
        (Some("tout"), _) => {
            state.permissions().vider();
            let Ok(pool) = state.pool() else { return };
            let serveurs: Vec<String> = sqlx::query_scalar("SELECT id::text FROM servers")
                .fetch_all(pool)
                .await
                .unwrap_or_default();
            for s in serveurs {
                state.droits_modifies(&s, "").await;
            }
        }
        _ => {}
    }
}

const CHEMIN_RESSOURCES: &str = "/internal/aegis/resources/{type}";

pub fn router() -> OpenApiRouter<AppState> {
    let routeur = OpenApiRouter::new().route(CHEMIN_RESSOURCES, get(ressources));
    let op = crate::openapi::operation(
        "interne",
        "Serveurs listés pour aegis (`q`, `ids`) : `[{id, name}]`. Jeton `AEGIS_TOKEN`, réseau interne ; 404 sans aegis",
        &[
            ("200", "ok"),
            ("401", "jeton d'aegis attendu"),
            ("404", "aegis non configuré ou type inconnu"),
        ],
        CHEMIN_RESSOURCES,
    );
    crate::openapi::documenter(
        routeur,
        CHEMIN_RESSOURCES,
        utoipa::openapi::HttpMethod::Get,
        op,
    )
}

#[derive(Deserialize)]
struct Filtre {
    q: Option<String>,
    ids: Option<String>,
}

fn jeton_valide(state: &AppState, h: &HeaderMap) -> bool {
    let attendu = &state.config().aegis.token;
    let donne = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    !attendu.is_empty() && crate::api::jetons_egaux(attendu, donne)
}

async fn ressources(
    State(state): State<AppState>,
    Path(type_): Path<String>,
    Query(f): Query<Filtre>,
    h: HeaderMap,
) -> Response {
    if !state.permissions().aegis_actif() || type_ != catalogue::SERVEUR {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !jeton_valide(&state, &h) {
        return write_err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "jeton d'aegis attendu",
        );
    }
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let ids: Vec<String> = f
        .ids
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .take(200)
        .map(String::from)
        .collect();
    let q = f.q.unwrap_or_default().trim().to_lowercase();
    let lignes: Result<Vec<(String, String)>, _> = sqlx::query_as(
        "SELECT id::text, name FROM servers
         WHERE (cardinality($1::text[]) = 0 OR id::text = ANY($1))
           AND ($2 = '' OR lower(name) LIKE '%' || $2 || '%' OR id::text = $2)
         ORDER BY lower(name) LIMIT 50",
    )
    .bind(&ids)
    .bind(&q)
    .fetch_all(pool)
    .await;
    match lignes {
        Ok(l) => Json(
            l.into_iter()
                .map(|(id, name)| json!({"id": id, "name": name}))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => crate::http::internal(),
    }
}
