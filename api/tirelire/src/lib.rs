#![allow(clippy::result_large_err)]

pub mod bilan;
pub mod categories;
pub mod classement;
pub mod cmb;
pub mod commercants;
pub mod config;
pub mod droits;
pub mod etat;
pub mod evolution;
pub mod export;
pub mod extraction;
pub mod garde;
pub mod http;
pub mod identite;
pub mod import;
pub mod interne;
pub mod metriques;
pub mod mois;
pub mod mots;
pub mod normalisation;
pub mod regles;
pub mod releves;
pub mod routes;
pub mod selection;
pub mod stats;

#[cfg(test)]
mod tests_api;
#[cfg(test)]
mod tests_categories;
#[cfg(test)]
mod tests_evolution;
#[cfg(test)]
mod tests_recherche;
#[cfg(test)]
mod tests_stats;
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
