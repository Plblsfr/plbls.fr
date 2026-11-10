pub mod annuaire;
pub mod auth;
pub mod cache;
pub mod error;
pub mod journal;
pub mod moteur;
pub mod routes;
pub mod settings;
pub mod stockage;
pub mod taches;

use std::sync::Arc;

use settings::Settings;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: sqlx::PgPool,
    pub cache: cache::Cache,
    pub spec: Arc<str>,
    pub http: reqwest::Client,
}

impl AppState {
    pub async fn new(settings: Settings, db: sqlx::PgPool) -> anyhow::Result<Self> {
        let cache = cache::Cache::connecter(
            &settings.redis.url,
            &settings.redis.prefix,
            settings.cache.ttl_secs,
        )
        .await;
        let (_, doc) = routes::api().split_for_parts();
        Ok(AppState {
            settings: Arc::new(settings),
            db,
            cache,
            spec: doc.to_pretty_json()?.into(),
            http: commun::http::client(std::time::Duration::from_secs(10)),
        })
    }

    pub fn routeur(&self) -> axum::Router {
        let (api, _) = routes::api().split_for_parts();
        api.merge(routes::openapi::router())
            .layer(tower_http::trace::TraceLayer::new_for_http())
            .with_state(self.clone())
    }
}

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
