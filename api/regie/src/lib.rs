#![allow(clippy::result_large_err)]

pub mod arr;
pub mod config;
pub mod droits;
pub mod etat;
pub mod garde;
pub mod http;
pub mod identite;
pub mod interne;
pub mod qbittorrent;
pub mod routes;
pub mod sante;
pub mod suivi;
pub mod telechargements;

#[cfg(test)]
mod tests_api;
#[cfg(test)]
mod testutil;

use std::net::SocketAddr;

use tokio::net::TcpListener;

pub use config::Config;
pub use etat::AppState;

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
