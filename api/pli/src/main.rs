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
    let cfg = match pli::Config::from_env(|k| env.get(k)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(&cfg.database_url)?;
    pli::db::migrate(&db).await?;
    pli::purge_periodique(db.clone());

    let port = cfg.port;
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await?;
    tracing::info!("pli sur :{port}");
    pli::serve(listener, pli::AppState::new(cfg, db), arret()).await?;
    Ok(())
}
