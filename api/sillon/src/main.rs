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
    if std::env::args().nth(1).as_deref() == Some(sillon::sablier::COMMANDE) {
        match sillon::sablier::lancer(|k| env.get(k)).await {
            Ok(bilan) => {
                print!("{bilan}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    let cfg = match sillon::Config::from_env(|k| env.get(k)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(&cfg.database_url)?;
    sillon::MIGRATIONS.run(&pool).await?;
    let port = cfg.port;
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await?;
    tracing::info!("sillon sur :{port}");
    let emetteur = commun::emission::Emetteur::depuis_env(|k| env.get(k));
    let etat = sillon::AppState::avec_emetteur(cfg, pool, emetteur);
    sillon::serve(listener, etat, arret()).await?;
    Ok(())
}
