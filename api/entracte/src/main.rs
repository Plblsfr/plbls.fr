use std::net::{Ipv4Addr, SocketAddr};

use sqlx::postgres::PgPoolOptions;

async fn arret() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("signal SIGTERM");
    tokio::select! {
        () = ctrl_c => {},
        _ = term.recv() => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let env = commun::env::Env::charger();
    if std::env::args().nth(1).as_deref() == Some(entracte::import_estompe::COMMANDE) {
        match entracte::import_estompe::lancer(|k| env.get(k)).await {
            Ok(rapport) => {
                print!("{rapport}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    let cfg = match entracte::Config::from_env(|k| env.get(k)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(&cfg.database_url)?;
    entracte::MIGRATIONS.run(&pool).await?;
    let port = cfg.port;
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await?;
    tracing::info!("entracte sur :{port}");
    entracte::serve(listener, entracte::AppState::new(cfg, pool), arret()).await?;
    Ok(())
}
