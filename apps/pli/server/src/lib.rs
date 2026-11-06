pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod limit;

use std::{net::SocketAddr, time::Duration};

use tokio::net::TcpListener;

pub use app::{AppState, router};
pub use config::Config;

pub fn purge_periodique(db: sqlx::PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let periode = Duration::from_secs(60);
        let mut minuterie =
            tokio::time::interval_at(tokio::time::Instant::now() + periode, periode);
        loop {
            minuterie.tick().await;
            if let Err(e) = db::purge_expired(&db).await {
                tracing::error!("purge {e}");
            }
        }
    })
}

pub async fn serve(
    listener: TcpListener,
    state: AppState,
    arret: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(arret)
    .await
}
