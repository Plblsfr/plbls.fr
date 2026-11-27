pub mod badge;
mod fetcher;
pub mod sillon;

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

pub fn brancher(state: &AppState) {
    sillon::brancher(state);
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
    if let Some(musiques) = state.musiques()
        && let Some(cible) = sillon::lien(param("url"), &state.config().allowed_origins)
    {
        return sillon::carte(
            &state,
            &*musiques,
            &user,
            cible,
            param("message"),
            param("url"),
        )
        .await;
    }
    if let Some(cle) = badge::lien(param("url"), &state.config().allowed_origins) {
        return badge::carte(&state, &user, cle, param("message"), param("url")).await;
    }
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
        state::{AppState, MusiqueResolue, Musiques, RefusMusique},
        testutil::{self, Faux, faux, faux_aegis},
    };
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use serde_json::{Value, json};
    use std::{
        pin::Pin,
        sync::{Arc, Mutex},
    };
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    struct Env {
        app: Router,
        state: AppState,
        pool: sqlx::PgPool,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env(non_filtre: bool) -> Option<Env> {
        env_avec(non_filtre, "").await
    }

    async fn env_avec(non_filtre: bool, origines: &str) -> Option<Env> {
        env_config(non_filtre, origines, &[]).await
    }

    async fn env_config(non_filtre: bool, origines: &str, extra: &[(&str, &str)]) -> Option<Env> {
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
        for nom in ["alice", "bob", "carole"] {
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(&pool)
                .await
                .unwrap();
        }
        let state = AppState::new(
            testutil::config_avec(
                &[
                    ("GATEWAY_URL", gateway.url.as_str()),
                    ("AEGIS_URL", permission.url.as_str()),
                    ("ALLOWED_ORIGINS", origines),
                ]
                .into_iter()
                .chain(extra.iter().copied())
                .collect::<Vec<_>>(),
            ),
            vec![],
        );
        state.set_pool(pool.clone());
        let app =
            Router::from(routes(Arc::new(Fetcher::new(non_filtre)))).with_state(state.clone());
        Some(Env {
            app,
            state,
            pool,
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

    const ALBUM: &str = "11111111-2222-3333-4444-555555555555";
    const LISTE: &str = "11111111-2222-3333-4444-666666666666";

    struct FauxMusiques(Mutex<Vec<String>>);

    impl Musiques for FauxMusiques {
        fn apercu<'a>(
            &'a self,
            sub: &'a str,
            genre: &'a str,
            id: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<MusiqueResolue, RefusMusique>> + Send + 'a>>
        {
            self.0.lock().unwrap().push(format!("{sub} {genre} {id}"));
            Box::pin(async move {
                if sub != "sub-alice" {
                    return Err(RefusMusique::Permission);
                }
                Ok(MusiqueResolue {
                    genre: genre.into(),
                    id: id.into(),
                    titre: "Lemonade".into(),
                    artiste: "Beyoncé".into(),
                    album: Some(id.into()),
                    image: Some(("album".into(), id.into())),
                })
            })
        }
    }

    async fn utilisateur(pool: &sqlx::PgPool, nom: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM users WHERE pseudo = $1")
            .bind(nom)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn message(pool: &sqlx::PgPool, dm: &str, auteur: &str, contenu: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO messages (dm_id, author_id, content) VALUES ($1::uuid, $2::uuid, $3) RETURNING id",
        )
        .bind(dm)
        .bind(utilisateur(pool, auteur).await)
        .bind(contenu)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn q(url: &str, message: i64) -> String {
        format!(
            "?url={}&message={message}",
            utf8_percent_encode(url, NON_ALPHANUMERIC)
        )
    }

    #[tokio::test]
    async fn lien_sillon_donne_une_carte_selon_l_acces_de_l_auteur() {
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origine = format!("http://{}", ecoute.local_addr().unwrap());
        let site = Router::new().fallback(|| async {
            (
                [(header::CONTENT_TYPE, "text/html")],
                "<html><head><title>Estompe</title></head></html>",
            )
        });
        tokio::spawn(async move { axum::serve(ecoute, site).await.ok() });
        let Some(e) = env_avec(true, &format!("{origine}/")).await else {
            return;
        };
        let (a, b) = (
            utilisateur(&e.pool, "alice").await,
            utilisateur(&e.pool, "bob").await,
        );
        let (ua, ub) = if a < b { (a, b) } else { (b, a) };
        let dm: String = sqlx::query_scalar(
            "INSERT INTO dm_conversations (user_a, user_b) VALUES ($1::uuid, $2::uuid) RETURNING id::text",
        )
        .bind(ua)
        .bind(ub)
        .fetch_one(&e.pool)
        .await
        .unwrap();
        let album = format!("{origine}/#/musique/album/{ALBUM}");
        let liste = format!("{origine}/#/musique/playlist/{LISTE}");
        let de_alice = message(&e.pool, &dm, "alice", &format!("écoute {album} !")).await;
        let de_bob = message(&e.pool, &dm, "bob", &format!("et {album}")).await;
        let playlist = message(&e.pool, &dm, "alice", &liste).await;

        let (s, _, p) = appel(&e.app, Some("alice"), &q(&album, de_alice)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            p["title"], "Estompe",
            "sans Sillon : aperçu de page ordinaire"
        );
        assert!(p.get("sillon").is_none());

        let faux = Arc::new(FauxMusiques(Mutex::default()));
        e.state.definir_musiques(faux.clone());
        let (s, h, p) = appel(&e.app, Some("alice"), &q(&album, de_alice)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=600");
        assert_eq!(
            p,
            json!({
                "url": album, "title": "Lemonade", "description": "Beyoncé", "site_name": "Sillon",
                "sillon": {"type": "album", "id": ALBUM, "title": "Lemonade", "artist": "Beyoncé",
                           "album": ALBUM, "cover": {"kind": "album", "id": ALBUM}, "listen": true}
            })
        );
        let (s, _, p) = appel(&e.app, Some("bob"), &q(&album, de_alice)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(p["sillon"]["title"], "Lemonade");
        assert_eq!(p["sillon"]["listen"], false);

        for (qui, requete) in [
            ("alice", q(&album, de_bob)),
            ("bob", q(&album, de_bob)),
            ("carole", q(&album, de_alice)),
            (
                "alice",
                q(&format!("{origine}/#/musique/album/{LISTE}"), de_alice),
            ),
            (
                "alice",
                format!("?url={}", utf8_percent_encode(&album, NON_ALPHANUMERIC)),
            ),
            ("alice", q(&liste, playlist)),
        ] {
            let (s, _, c) = appel(&e.app, Some(qui), &requete).await;
            assert_eq!(
                (s, c["error"].as_str()),
                (StatusCode::NOT_FOUND, Some("no_preview")),
                "{qui} {requete}"
            );
            assert!(c.get("title").is_none());
        }
        let appels = faux.0.lock().unwrap().clone();
        assert!(appels.iter().all(|a| !a.contains("playlist")), "{appels:?}");
        assert!(appels.iter().all(|a| !a.contains(LISTE)), "{appels:?}");
    }

    #[tokio::test]
    async fn lien_de_badge_donne_la_carte_du_badge_de_l_auteur() {
        let origine = "https://estompe.test";
        let entracte = faux(|r| {
            let attendu = format!("/internal/badges/season:{ALBUM}:2");
            if r.entete("authorization") != "Bearer jeton-entracte"
                || r.chemin != attendu
                || r.query != "sub=sub-alice"
            {
                return (404, vec![]);
            }
            (
                200,
                json!({"kind": "season", "title_id": ALBUM, "season": 2, "title": "La Série",
                       "season_title": "Saison 2", "year": 2019, "poster": true, "season_poster": false,
                       "episodes": 8, "duration": 21600.0, "obtained_at": "2026-11-26T23:10:00.000Z"})
                .to_string()
                .into_bytes(),
            )
        })
        .await;
        let Some(e) = env_config(
            true,
            &format!("{origine}/"),
            &[
                ("ENTRACTE_INTERNE_URL", entracte.url.as_str()),
                ("ENTRACTE_SERVICE_TOKEN", "jeton-entracte"),
            ],
        )
        .await
        else {
            return;
        };
        let (a, b) = (
            utilisateur(&e.pool, "alice").await,
            utilisateur(&e.pool, "bob").await,
        );
        let (ua, ub) = if a < b { (a, b) } else { (b, a) };
        let dm: String = sqlx::query_scalar(
            "INSERT INTO dm_conversations (user_a, user_b) VALUES ($1::uuid, $2::uuid) RETURNING id::text",
        )
        .bind(ua)
        .bind(ub)
        .fetch_one(&e.pool)
        .await
        .unwrap();
        let lien = format!("{origine}/#/films/badge/season%3A{ALBUM}%3A2");
        let de_alice = message(&e.pool, &dm, "alice", &lien).await;
        let de_bob = message(&e.pool, &dm, "bob", &lien).await;
        for qui in ["alice", "bob"] {
            let (s, h, p) = appel(&e.app, Some(qui), &q(&lien, de_alice)).await;
            assert_eq!(s, StatusCode::OK, "{qui}");
            assert_eq!(h[header::CACHE_CONTROL], "private, max-age=600");
            assert_eq!(p["site_name"], "Entracte");
            assert_eq!(p["description"], "Saison 2 terminée");
            assert_eq!(p["entracte_badge"]["title"], "La Série");
            assert_eq!(p["entracte_badge"]["season"], 2);
        }
        for (qui, requete) in [
            ("bob", q(&lien, de_bob)),
            ("carole", q(&lien, de_alice)),
            (
                "alice",
                format!("?url={}", utf8_percent_encode(&lien, NON_ALPHANUMERIC)),
            ),
        ] {
            let (s, _, c) = appel(&e.app, Some(qui), &requete).await;
            assert_eq!(
                (s, c["error"].as_str()),
                (StatusCode::NOT_FOUND, Some("no_preview")),
                "{qui} {requete}"
            );
        }
        let subs: Vec<String> = entracte.requetes().into_iter().map(|r| r.query).collect();
        assert!(
            subs.iter().all(|q| q != "sub=sub-carole"),
            "le lecteur n'est jamais demandé : {subs:?}"
        );
    }
}
