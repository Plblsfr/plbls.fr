#![allow(clippy::result_large_err)]

pub mod arr;
pub mod bibliotheque;
pub mod catalogue;
pub mod config;
pub mod demandes;
pub mod droits;
pub mod etat;
pub mod ffprobe;
pub mod filtre;
pub mod flux;
pub mod garde;
pub mod http;
pub mod identite;
pub mod images;
pub mod interne;
pub mod lecture;
pub mod materiel;
pub mod progression;
pub mod push;
pub mod routes;
pub mod sous_titres;
pub mod tmdb;
pub mod transcodage;

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
    state.transcodeur().preparer().await;
    state.demarrer_taches();
    axum::serve(
        listener,
        routes::routeur(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(arret)
    .await
}
