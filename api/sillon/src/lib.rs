#![allow(clippy::result_large_err)]

pub mod bibliotheque;
pub mod catalogue;
pub mod config;
pub mod droits;
pub mod etat;
pub mod ffprobe;
pub mod filtre;
pub mod garde;
pub mod http;
pub mod identite;
pub mod images;
pub mod interne;
pub mod lidarr;
pub mod routes;

pub mod flux;
pub mod lecture;
pub mod preferences;
pub mod transcodage;
pub mod volume;

pub mod analyse;

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
