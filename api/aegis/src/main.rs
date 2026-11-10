use sqlx::postgres::PgPoolOptions;

use aegis::{AppState, MIGRATIONS, settings::Settings, taches};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let settings = Settings::load()?;
    if settings.identity.secret.is_empty() {
        anyhow::bail!("GATEWAY_IDENTITE_SECRET est vide");
    }
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.database.url)
        .await?;
    MIGRATIONS.run(&db).await?;

    let state = AppState::new(settings, db).await?;
    taches::lancer(state.clone());
    let listener = tokio::net::TcpListener::bind(&state.settings.server.bind).await?;
    tracing::info!("aegis à l'écoute sur {}", state.settings.server.bind);
    axum::serve(listener, state.routeur()).await?;
    Ok(())
}
