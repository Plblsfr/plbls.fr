mod fetcher;

use std::{sync::Arc, time::Duration};

use axum::{
    Extension,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};
use estompe_core::{
    api::ActiveUser,
    http::{internal, write_err, write_json},
    openapi,
    state::AppState,
};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

pub use fetcher::{Erreur, Fetcher, OEmbed, Preview};

const PAR_MINUTE: usize = 20;

pub fn routes(fetcher: Arc<Fetcher>) -> OpenApiRouter<AppState> {
    let op = openapi::operation(
        "messages",
        "Aperçu d'un lien (titre, description, image) — à afficher comme du texte, jamais comme du HTML",
        &[
            ("200", "ok"),
            ("400", "`invalid_url` ou `blocked_url`"),
            ("404", "no_preview"),
            ("429", "too_many_requests"),
        ],
        "/link-preview",
    );
    openapi::documenter(
        OpenApiRouter::new().route("/link-preview", get(link_preview)),
        "/link-preview",
        HttpMethod::Get,
        op,
    )
    .layer(Extension(fetcher))
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes(Arc::new(Fetcher::new(false))))
}

async fn link_preview(
    State(state): State<AppState>,
    Extension(fetcher): Extension<Arc<Fetcher>>,
    ActiveUser(user): ActiveUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Response {
    if !state
        .limiteurs()
        .apercu
        .allow(&user.id, PAR_MINUTE, Duration::from_secs(60))
    {
        let mut r = write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_requests",
            "trop de demandes d'aperçu",
        );
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        return r;
    }
    let param = |nom: &str| {
        params
            .iter()
            .find(|(k, _)| k == nom)
            .map_or("", |(_, v)| v.as_str())
    };
    if !param("refresh").is_empty() {
        fetcher.forget(param("url"));
    }
    match fetcher.fetch(param("url")).await {
        Ok(p) => {
            let Ok(valeur) = serde_json::to_value(&p) else {
                return internal();
            };
            let mut r = write_json(StatusCode::OK, &valeur);
            r.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=600"),
            );
            r
        }
        Err(Erreur::UrlInvalide) => {
            write_err(StatusCode::BAD_REQUEST, "invalid_url", "URL invalide")
        }
        Err(Erreur::Bloquee) => write_err(
            StatusCode::BAD_REQUEST,
            "blocked_url",
            "adresse non autorisée",
        ),
        Err(Erreur::SansApercu) => write_err(
            StatusCode::NOT_FOUND,
            "no_preview",
            "pas d'aperçu pour ce lien",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{Fetcher, PAR_MINUTE, routes};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
        response::IntoResponse,
    };
    use estompe_core::testutil::BasePostgres;
    use estompe_core::{
        db::migrate_embarquees,
        state::AppState,
        testutil::{self, Faux, faux, faux_aegis},
    };
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    struct Env {
        app: Router,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env(non_filtre: bool) -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            if nom == "anonyme" {
                return (401, vec![]);
            }
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux_aegis(|_, _| true).await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "bob"] {
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(&pool)
                .await
                .unwrap();
        }
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("AEGIS_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let app = Router::from(routes(Arc::new(Fetcher::new(non_filtre)))).with_state(state);
        Some(Env {
            app,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn appel(
        app: &Router,
        bearer: Option<&str>,
        requete: &str,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut r = Request::get(format!("/link-preview{requete}"));
        if let Some(b) = bearer {
            r = r.header(header::AUTHORIZATION, format!("Bearer {b}"));
        }
        let rep = app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (statut, entetes) = (rep.status(), rep.headers().clone());
        let corps = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (
            statut,
            entetes,
            serde_json::from_slice(&corps).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn link_preview_renvoie_l_apercu() {
        let Some(e) = env(true).await else { return };
        let site = Router::new().fallback(|req: Request<Body>| async move {
            if req.uri().path() == "/a.png" {
                return (
                    [(header::CONTENT_TYPE, "image/png")],
                    b"\x89PNG\r\n\x1a\n0000".to_vec(),
                )
                    .into_response();
            }
            (
                [(header::CONTENT_TYPE, "text/html")],
                r#"<html><head><title>Mon site</title><meta name="description" content="Bien"><meta property="og:image" content="/a.png"><meta property="og:video" content="https://cdn.exemple.fr/a.mp4"></head></html>"#,
            )
                .into_response()
        });
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", ecoute.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(ecoute, site).await.ok() });
        let q = format!("?url={}", utf8_percent_encode(&url, NON_ALPHANUMERIC));
        let (s, h, p) = appel(&e.app, Some("alice"), &q).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=600");
        assert!(
            p["image"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(p["video"], "https://cdn.exemple.fr/a.mp4");
        assert_eq!(p["title"], "Mon site");
        assert_eq!(p["description"], "Bien");
        let (s, _, _) = appel(&e.app, Some("alice"), &format!("{q}&refresh=1")).await;
        assert_eq!(s, StatusCode::OK);
    }

    #[tokio::test]
    async fn link_preview_exige_un_compte_actif() {
        let Some(e) = env(true).await else { return };
        assert_eq!(
            appel(&e.app, None, "?url=http://x").await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            appel(&e.app, Some("jamais-active"), "?url=http://x")
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn link_preview_refuse_le_local_et_limite_par_utilisateur() {
        let Some(e) = env(false).await else { return };
        for u in ["http://127.0.0.1/", "http://169.254.169.254/"] {
            let (s, _, c) = appel(&e.app, Some("alice"), &format!("?url={u}")).await;
            assert_eq!(
                (s, c["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("blocked_url")),
                "{u}"
            );
        }
        let (s, _, c) = appel(&e.app, Some("alice"), "?url=ftp://x").await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_url"))
        );
        assert_eq!(
            appel(&e.app, Some("alice"), "").await.0,
            StatusCode::BAD_REQUEST
        );
        for _ in 0..PAR_MINUTE - 4 {
            assert_eq!(
                appel(&e.app, Some("alice"), "?url=ftp://x").await.0,
                StatusCode::BAD_REQUEST
            );
        }
        let (s, h, c) = appel(&e.app, Some("alice"), "?url=ftp://x").await;
        assert_eq!(h[header::RETRY_AFTER], "60");
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::TOO_MANY_REQUESTS, Some("too_many_requests"))
        );
        assert_eq!(
            appel(&e.app, Some("bob"), "?url=ftp://x").await.0,
            StatusCode::BAD_REQUEST
        );
    }
}
