mod auth;
mod error;
mod routes;
mod settings;

use std::{path::Path, sync::Arc};

use sqlx::postgres::PgPoolOptions;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use settings::Settings;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: sqlx::PgPool,
    /// Spec OpenAPI générée depuis les handlers.
    pub spec: Arc<str>,
    /// Appels sortants vers le gateway.
    pub http: reqwest::Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let settings = Arc::new(Settings::load()?);
    if settings.identity.secret.is_empty() {
        anyhow::bail!("PERMISSION__IDENTITY__SECRET est vide");
    }
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.database.url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    let (api_router, api_doc) = routes::api().split_for_parts();
    let state = AppState {
        settings: settings.clone(),
        db,
        spec: api_doc.to_pretty_json()?.into(),
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?,
    };
    let mut app = api_router.merge(routes::openapi::router());

    // SPA : fichiers statiques, repli sur index.html pour le routage côté client.
    let dir = &settings.server.static_dir;
    if !dir.is_empty() && Path::new(dir).join("index.html").exists() {
        app = app.fallback_service(
            ServeDir::new(dir).fallback(ServeFile::new(Path::new(dir).join("index.html"))),
        );
    }
    let app = app.layer(TraceLayer::new_for_http()).with_state(state);

    let listener = tokio::net::TcpListener::bind(&settings.server.bind).await?;
    tracing::info!("permission à l'écoute sur {}", settings.server.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
