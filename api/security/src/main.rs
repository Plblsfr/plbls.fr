mod auth;
mod identity;
mod oidc;
mod origin;
mod routes;
mod session;
mod settings;
#[cfg(test)]
mod testutil;

use std::sync::Arc;

use axum::{
    Router,
    http::{HeaderValue, Method, header},
    middleware,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

use settings::Settings;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub store: session::Store,
    pub oidc: Arc<oidc::Oidc>,
    /// Client du reverse proxy : pas de redirections suivies, pas de durée maximale (streaming).
    pub proxy_http: reqwest::Client,
}

fn cors(settings: Arc<Settings>) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin: &HeaderValue, _| {
            origin
                .to_str()
                .is_ok_and(|o| origin::origin_allowed(&settings.server, o))
        }))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let settings = Arc::new(Settings::load()?);
    let redis = redis::Client::open(settings.redis.url.as_str())?
        .get_connection_manager()
        .await?;
    let proxy_http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(5))
        // délai d'inactivité (pas de durée totale) : le streaming continue, un upstream bloqué est coupé
        .read_timeout(std::time::Duration::from_secs(60))
        .build()?;

    let state = AppState {
        store: session::Store::new(redis),
        oidc: Arc::new(oidc::Oidc::new(settings.oidc.clone())?),
        proxy_http,
        settings: settings.clone(),
    };

    let app = Router::new()
        .merge(routes::health::router())
        .merge(routes::assets::router())
        .merge(routes::web::router())
        .merge(routes::internal::router())
        .merge(routes::proxy::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            routes::web::rate_limit,
        ))
        .layer(cors(settings.clone()))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&settings.server.bind).await?;
    tracing::info!("auth-gateway à l'écoute sur {}", settings.server.bind);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
