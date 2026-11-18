#![allow(clippy::result_large_err)]

pub mod admin;
pub mod agregats;
pub mod battement;
pub mod catalogue;
pub mod config;
pub mod domaines;
pub mod droits;
pub mod effacement;
pub mod etat;
pub mod exploits;
pub mod export;
pub mod file;
pub mod garde;
pub mod http;
pub mod identite;
pub mod interne;
pub mod journal;
pub mod partage;
pub mod periode;
pub mod personnes;
pub mod purge;
pub mod recaps;
pub mod routes;
pub mod series;
pub mod trophees;
pub mod utilisation;

#[cfg(test)]
mod tests_api;
#[cfg(test)]
mod testutil;

use std::net::SocketAddr;

use tokio::net::TcpListener;

pub use config::Config;
pub use etat::AppState;

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn serve(
    listener: TcpListener,
    state: AppState,
    arret: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    state.demarrer_taches();
    axum::serve(
        listener,
        routes::routeur(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(arret)
    .await
}
