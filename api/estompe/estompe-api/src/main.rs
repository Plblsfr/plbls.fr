use std::net::SocketAddr;

mod journal_json;

use axum::Router;
use estompe_block::{Block, Set};
use estompe_core::{config::Config, state::AppState};
use tokio::{net::TcpListener, signal};
use utoipa::openapi::OpenApi;

fn block_list() -> Vec<Block<AppState>> {
    vec![
        Block {
            name: "gifs",
            needs: &[],
            mount: estompe_block_gifs::mount,
        },
        Block {
            name: "musique",
            needs: &[],
            mount: estompe_block_musique::mount,
        },
        Block {
            name: "vocal",
            needs: &[],
            mount: estompe_block_vocal::mount,
        },
        Block {
            name: "regarder-ensemble",
            needs: &["vocal"],
            mount: estompe_block_regarder_ensemble::mount,
        },
        Block {
            name: "draw2gether",
            needs: &["vocal"],
            mount: estompe_block_draw2gether::mount,
        },
        Block {
            name: "transcription",
            needs: &["vocal"],
            mount: estompe_block_transcription::mount,
        },
        Block {
            name: "videos",
            needs: &[],
            mount: estompe_block_videos::mount,
        },
        Block {
            name: "apercu-liens",
            needs: &[],
            mount: estompe_block_apercu_liens::mount,
        },
        Block {
            name: "notifications",
            needs: &[],
            mount: estompe_block_notifications::mount,
        },
        Block {
            name: "emojis",
            needs: &[],
            mount: estompe_block_emojis::mount,
        },
        Block {
            name: "avatars",
            needs: &[],
            mount: estompe_block_avatars::mount,
        },
        Block {
            name: "badges",
            needs: &[],
            mount: estompe_block_badges::mount,
        },
        Block {
            name: "signalements",
            needs: &[],
            mount: estompe_block_signalements::mount,
        },
        Block {
            name: "activite-statut",
            needs: &[],
            mount: estompe_block_activite_statut::mount,
        },
        Block {
            name: "films",
            needs: &["vocal", "regarder-ensemble"],
            mount: estompe_block_films::mount,
        },
    ]
}

fn variable_bloc(nom: &str, valeur: Option<String>, jellyfin_actif: bool) -> Option<String> {
    if nom != "BLOCK_FILMS" {
        return valeur;
    }
    match valeur.as_deref() {
        None | Some("") => Some("false".into()),
        Some("true") if !jellyfin_actif => Some("false".into()),
        _ => valeur,
    }
}

fn routes(set: &Set) -> (Router<AppState>, OpenApi) {
    estompe_block::mount(&block_list(), set, estompe_core::api::router()).split_for_parts()
}

fn app(state: AppState, set: &Set) -> Router {
    estompe_core::middleware::envelopper(routes(set).0, state)
}

fn listen_addr(valeur: &str) -> Result<SocketAddr, std::net::AddrParseError> {
    match valeur.strip_prefix(':') {
        Some(port) => format!("0.0.0.0:{port}").parse(),
        None => valeur.parse(),
    }
}

async fn arret() {
    let ctrl_c = async { signal::ctrl_c().await.expect("signal Ctrl-C") };
    let sigterm = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("signal SIGTERM")
            .recv()
            .await;
    };
    tokio::select! { _ = ctrl_c => {}, _ = sigterm => {} }
}

async fn arret_hub(state: AppState) {
    arret().await;
    state.hub().close_all();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--openapi") {
        print!("{}", routes(&Set::all()).1.to_yaml()?);
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--manifeste-aegis") {
        let blocs = estompe_block::actifs(&block_list(), &Set::all());
        let url = std::env::var("AEGIS_RESOURCES_URL")
            .unwrap_or_else(|_| "http://estompe-api:8080".into());
        println!(
            "{:#}",
            estompe_core::catalogue::manifeste(&blocs, url.trim_end_matches('/'))
        );
        return Ok(());
    }
    tracing_subscriber::fmt()
        .event_format(journal_json::FormatJson)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let config = Config::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let set = estompe_block::select(&block_list(), |k| {
        variable_bloc(k, std::env::var(k).ok(), config.jellyfin_actif())
    })
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let adresse = listen_addr(&config.listen_addr)?;
    let listener = TcpListener::bind(adresse).await?;
    tracing::info!(%adresse, "estompe-api à l'écoute");
    let state = AppState::new(config.clone(), estompe_block::actifs(&block_list(), &set));
    if set.has("notifications")
        && let Err(e) = estompe_block_notifications::installer(&state)
    {
        eprintln!("{e}");
        std::process::exit(1);
    }
    if set.has("vocal") {
        estompe_block_vocal::brancher(&state);
    }
    estompe_core::aegis::demarrer(state.clone());
    if set.has("videos") {
        estompe_block_videos::brancher(&state);
    }
    if set.has("films") {
        estompe_block_films::brancher(&state);
    }
    let ouverture = {
        let state = state.clone();
        async move {
            match estompe_core::db::open(&config.database_url).await {
                Ok(pool) => {
                    estompe_core::notes_version::annoncer_toutes(&pool).await;
                    state.set_pool(pool);
                    tracing::info!("base de données prête");
                    std::future::pending().await
                }
                Err(e) => e,
            }
        }
    };
    let (stop_purge, recu_purge) = tokio::sync::watch::channel(false);
    let purge = estompe_core::fichiers::lancer_purge(
        state.clone(),
        estompe_core::fichiers::INTERVALLE_PURGE,
        estompe_core::fichiers::AGE_ORPHELINS,
        recu_purge,
    );
    tokio::select! {
        r = axum::serve(listener, app(state.clone(), &set)).with_graceful_shutdown(arret_hub(state.clone())) => r?,
        e = ouverture => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    }
    let _ = stop_purge.send(true);
    let _ = purge.await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{app, block_list, listen_addr, routes, variable_bloc};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use estompe_block::{Set, select};
    use estompe_core::{state::AppState, testutil};
    use tower::ServiceExt;

    async fn get(chemin: &str) -> (StatusCode, String) {
        let reponse = app(AppState::new(testutil::config(), vec![]), &Set::all())
            .oneshot(Request::get(chemin).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let statut = reponse.status();
        let corps = to_bytes(reponse.into_body(), 1024).await.unwrap();
        (statut, String::from_utf8(corps.to_vec()).unwrap())
    }

    fn spec() -> serde_json::Value {
        serde_json::to_value(routes(&Set::all()).1).unwrap()
    }

    fn operations(doc: &serde_json::Value) -> std::collections::BTreeSet<(String, String)> {
        let methodes = ["get", "post", "put", "patch", "delete"];
        doc["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(chemin, item)| {
                methodes
                    .iter()
                    .filter(|m| item.get(**m).is_some())
                    .map(|m| (chemin.clone(), m.to_string()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn chaque_route_releve_d_une_permission_ou_d_une_exception() {
        use estompe_core::catalogue::{CATALOGUE, HORS_PERMISSION, classer, motif_couvre};
        let ops: Vec<(axum::http::Method, String)> = operations(&spec())
            .into_iter()
            .map(|(chemin, m)| {
                (
                    axum::http::Method::from_bytes(m.to_uppercase().as_bytes()).unwrap(),
                    chemin,
                )
            })
            .collect();
        let mut erreurs = Vec::new();
        for (m, chemin) in &ops {
            let c = classer(m, chemin);
            match (c.permissions.len(), c.exceptions.len()) {
                (1, 0) | (0, 1) => {}
                (0, 0) => erreurs.push(format!(
                    "{m} {chemin} : aucune permission ; l'ajouter aux routes d'une entrée de CATALOGUE ou, avec un motif, à HORS_PERMISSION"
                )),
                _ => erreurs.push(format!("{m} {chemin} : ambigu, couvert par {:?} et {:?}", c.permissions, c.exceptions)),
            }
        }
        let couvre = |motif: &'static str| ops.iter().any(|(m, c)| motif_couvre(motif, m, c));
        for e in CATALOGUE {
            for r in e.routes.iter().filter(|r| !couvre(r)) {
                erreurs.push(format!(
                    "{} : le motif « {r} » ne correspond à aucune route",
                    e.cle
                ));
            }
        }
        for (r, _) in HORS_PERMISSION.iter().filter(|(r, _)| !couvre(r)) {
            erreurs.push(format!(
                "HORS_PERMISSION : « {r} » ne correspond à aucune route"
            ));
        }
        assert!(
            erreurs.is_empty(),
            "{} écart(s) :\n{}",
            erreurs.len(),
            erreurs.join("\n")
        );
    }

    #[test]
    fn routes_et_openapi_se_correspondent() {
        let (routeur, _) = routes(&Set::all());
        let debug = format!("{routeur:?}");
        let noeud = debug.split("node: Node").nth(1).unwrap();
        let reelles: std::collections::BTreeSet<String> = noeud
            .split("RouteId(")
            .skip(1)
            .filter_map(|t| t.split_once("): \"")?.1.split_once('"'))
            .map(|(chemin, _)| chemin.to_string())
            .collect();
        let documentees: std::collections::BTreeSet<String> =
            operations(&spec()).into_iter().map(|(c, _)| c).collect();
        assert!(!reelles.is_empty(), "{debug}");
        assert_eq!(reelles, documentees);
    }

    #[test]
    fn openapi_compatible_avec_le_contrat_go() {
        let go: serde_json::Value = serde_norway::from_str(
            &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../openapi.yaml"))
                .unwrap(),
        )
        .unwrap();
        let rust = spec();
        let go_ops = operations(&go);
        for (chemin, methode) in operations(&rust) {
            assert!(
                go_ops.contains(&(chemin.clone(), methode.clone())),
                "{methode} {chemin} absent du contrat Go"
            );
            let (r, g) = (
                &rust["paths"][&chemin][&methode],
                &go["paths"][&chemin][&methode],
            );
            let codes = |op: &serde_json::Value| -> std::collections::BTreeSet<String> {
                op["responses"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect()
            };
            assert!(
                codes(r).is_superset(&codes(g)),
                "{methode} {chemin} : statuts {:?} / {:?}",
                codes(r),
                codes(g)
            );
            assert_eq!(r["tags"], g["tags"], "{methode} {chemin} : tags");
            assert_eq!(
                r["security"], g["security"],
                "{methode} {chemin} : sécurité"
            );
        }
        assert_eq!(rust["info"]["title"], go["info"]["title"]);
        assert_eq!(
            rust["components"]["securitySchemes"]["bearer"]["scheme"],
            "bearer"
        );
        assert_eq!(
            rust["components"]["securitySchemes"]["session"]["name"],
            go["components"]["securitySchemes"]["session"]["name"]
        );
    }

    #[tokio::test]
    async fn healthz_repond_ok() {
        assert_eq!(get("/healthz").await, (StatusCode::OK, "ok".to_string()));
    }

    #[tokio::test]
    async fn les_routes_sont_enveloppees() {
        let config = testutil::config_avec(&[
            ("ALLOWED_ORIGINS", "https://app.test"),
            ("METRICS_TOKEN", "sekret"),
        ]);
        let routeur = app(AppState::new(config, vec![]), &Set::all());
        let appeler = |r: axum::http::request::Builder| {
            let routeur = routeur.clone();
            async move {
                let rep = routeur
                    .oneshot(r.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                let statut = rep.status();
                let entetes = rep.headers().clone();
                let corps = to_bytes(rep.into_body(), 1 << 16).await.unwrap();
                (statut, entetes, String::from_utf8(corps.to_vec()).unwrap())
            }
        };
        let (s, h, _) =
            appeler(Request::get("/healthz").header("Origin", "https://app.test")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h["access-control-allow-origin"], "https://app.test");
        let (s, _, _) = appeler(Request::get("/me")).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _, c) =
            appeler(Request::get("/metrics").header("Authorization", "Bearer sekret")).await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            c.contains("estompe_http_requests_total{code=\"2xx\"} 1\n"),
            "{c}"
        );
        assert!(
            c.contains("estompe_http_requests_total{code=\"4xx\"} 1\n"),
            "{c}"
        );
    }

    #[tokio::test]
    async fn route_inconnue_404() {
        assert_eq!(get("/inconnue").await.0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn adresse_d_ecoute() {
        assert_eq!(listen_addr(":8080").unwrap().to_string(), "0.0.0.0:8080");
        assert_eq!(listen_addr(":9000").unwrap().to_string(), "0.0.0.0:9000");
        assert_eq!(
            listen_addr("127.0.0.1:0").unwrap().to_string(),
            "127.0.0.1:0"
        );
        assert!(listen_addr("pas une adresse").is_err());
    }

    #[test]
    fn les_seize_blocs() {
        let blocs = block_list();
        let noms: Vec<_> = blocs.iter().map(|b| b.name).collect();
        assert_eq!(
            noms,
            [
                "gifs",
                "musique",
                "vocal",
                "regarder-ensemble",
                "draw2gether",
                "transcription",
                "videos",
                "apercu-liens",
                "notifications",
                "emojis",
                "avatars",
                "badges",
                "signalements",
                "activite-statut",
                "films"
            ]
        );
        for b in &blocs {
            let attendu: &[&str] = match b.name {
                "regarder-ensemble" | "draw2gether" | "transcription" => &["vocal"],
                "films" => &["vocal", "regarder-ensemble"],
                _ => &[],
            };
            assert_eq!(b.needs, attendu, "{}", b.name);
        }
    }

    #[tokio::test]
    async fn healthz_sans_aucun_bloc() {
        let set = select(&block_list(), |k| {
            k.starts_with("BLOCK_").then(|| "false".to_string())
        })
        .unwrap();
        let reponse = app(AppState::new(testutil::config(), vec![]), &set)
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(reponse.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn musique_desactivee_retire_ses_routes() {
        let appel = |set: Set| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(Request::get("/music/resolve").body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        };
        let sans = select(&block_list(), |k| {
            (k == "BLOCK_MUSIQUE").then(|| "false".to_string())
        })
        .unwrap();
        assert_eq!(appel(sans).await, StatusCode::NOT_FOUND);
        assert_eq!(appel(Set::all()).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn activite_statut_desactive_retire_ses_routes() {
        let appel = |set: Set| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(Request::get("/statuses").body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        };
        let sans = select(&block_list(), |k| {
            (k == "BLOCK_ACTIVITE_STATUT").then(|| "false".to_string())
        })
        .unwrap();
        assert_eq!(appel(sans).await, StatusCode::NOT_FOUND);
        assert_eq!(appel(Set::all()).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn emojis_desactive_retire_ses_routes() {
        let appel = |set: Set| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(Request::get("/emojis").body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        };
        let sans = select(&block_list(), |k| {
            (k == "BLOCK_EMOJIS").then(|| "false".to_string())
        })
        .unwrap();
        assert_eq!(appel(sans).await, StatusCode::NOT_FOUND);
        assert_eq!(appel(Set::all()).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn vocal_desactive_retire_ses_routes() {
        let appel = |set: Set| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(Request::post("/voice/webhook").body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        };
        let sans = select(&block_list(), |k| {
            (k == "BLOCK_VOCAL"
                || k == "BLOCK_REGARDER_ENSEMBLE"
                || k == "BLOCK_DRAW2GETHER"
                || k == "BLOCK_TRANSCRIPTION"
                || k == "BLOCK_FILMS")
                .then(|| "false".to_string())
        })
        .unwrap();
        assert_eq!(appel(sans).await, StatusCode::NOT_FOUND);
        assert_eq!(appel(Set::all()).await, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn transcription_sans_vocal_refusee() {
        let e = select(&block_list(), |k| match k {
            "BLOCK_VOCAL" => Some("false".into()),
            "BLOCK_TRANSCRIPTION" => Some("true".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            e.to_string()
                .contains("le bloc transcription dépend de vocal")
        );
    }

    #[test]
    fn regarder_ensemble_sans_vocal_refuse() {
        let e = select(&block_list(), |k| match k {
            "BLOCK_VOCAL" => Some("false".into()),
            "BLOCK_REGARDER_ENSEMBLE" => Some("true".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            e.to_string()
                .contains("le bloc regarder-ensemble dépend de vocal")
        );
    }

    #[tokio::test]
    async fn regarder_ensemble_desactive_retire_ses_routes() {
        let appel = |set: Set| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(
                    Request::get("/servers/00000000-0000-0000-0000-000000000000/watch")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        };
        let sans = select(&block_list(), |k| {
            (k == "BLOCK_REGARDER_ENSEMBLE" || k == "BLOCK_FILMS").then(|| "false".to_string())
        })
        .unwrap();
        assert_eq!(appel(sans).await, StatusCode::NOT_FOUND);
        assert_eq!(appel(Set::all()).await, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn draw2gether_sans_vocal_refuse() {
        let e = select(&block_list(), |k| match k {
            "BLOCK_VOCAL" => Some("false".into()),
            "BLOCK_DRAW2GETHER" => Some("true".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            e.to_string()
                .contains("le bloc draw2gether dépend de vocal")
        );
    }

    #[tokio::test]
    async fn draw2gether_desactive_retire_ses_routes() {
        let appel = |set: Set| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(
                    Request::get("/voice/00000000-0000-0000-0000-000000000000/draw")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        };
        let sans = select(&block_list(), |k| {
            (k == "BLOCK_DRAW2GETHER").then(|| "false".to_string())
        })
        .unwrap();
        assert_eq!(appel(sans).await, StatusCode::NOT_FOUND);
        assert_eq!(appel(Set::all()).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn notifications_desactivees_retire_ses_routes() {
        let appel = |set: Set, methode: &'static str, chemin: &'static str| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(
                    Request::builder()
                        .method(methode)
                        .uri(chemin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        };
        let sans = || {
            select(&block_list(), |k| {
                (k == "BLOCK_NOTIFICATIONS").then(|| "false".to_string())
            })
            .unwrap()
        };
        for (methode, chemin) in [
            ("GET", "/push/key"),
            ("PUT", "/push/subscription"),
            ("POST", "/push/test"),
            ("PUT", "/push/fcm"),
            ("PUT", "/push/desktop"),
            ("GET", "/me/notify-settings"),
            ("PUT", "/me/notify-settings"),
        ] {
            assert_eq!(
                appel(sans(), methode, chemin).await,
                StatusCode::NOT_FOUND,
                "{chemin}"
            );
            assert_eq!(
                appel(Set::all(), methode, chemin).await,
                StatusCode::UNAUTHORIZED,
                "{chemin}"
            );
        }
    }

    #[test]
    fn films_inactif_par_defaut_et_sans_jellyfin() {
        let v = |valeur: Option<&str>, jellyfin: bool| {
            variable_bloc("BLOCK_FILMS", valeur.map(String::from), jellyfin)
        };
        assert_eq!(v(None, true).as_deref(), Some("false"));
        assert_eq!(v(Some(""), true).as_deref(), Some("false"));
        assert_eq!(v(Some("true"), false).as_deref(), Some("false"));
        assert_eq!(v(Some("true"), true).as_deref(), Some("true"));
        assert_eq!(v(Some("false"), true).as_deref(), Some("false"));
        assert_eq!(v(Some("oui"), true).as_deref(), Some("oui"));
        assert_eq!(variable_bloc("BLOCK_GIFS", None, false), None);
        let set = select(&block_list(), |k| variable_bloc(k, None, true)).unwrap();
        assert!(!set.has("films"));
        assert!(set.has("gifs"));
    }

    #[tokio::test]
    async fn films_absent_quand_le_bloc_est_desactive() {
        let appeler = |set: Set, chemin: &'static str| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(Request::get(chemin).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        };
        let sans = || {
            select(&block_list(), |k| {
                (k == "BLOCK_FILMS").then(|| "false".to_string())
            })
            .unwrap()
        };
        for chemin in [
            "/films/library",
            "/films/continue",
            "/admin/films/rejets",
            "/voice/00000000-0000-0000-0000-000000000000/cinema",
            "/servers/00000000-0000-0000-0000-000000000000/cinema",
        ] {
            assert_eq!(
                appeler(sans(), chemin).await,
                StatusCode::NOT_FOUND,
                "{chemin}"
            );
            assert_eq!(
                appeler(Set::all(), chemin).await,
                StatusCode::UNAUTHORIZED,
                "{chemin}"
            );
        }
    }

    #[tokio::test]
    async fn gifs_absent_quand_le_bloc_est_desactive() {
        let appeler = |set: Set, methode: &'static str, chemin: &'static str| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(
                    Request::builder()
                        .method(methode)
                        .uri(chemin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        };
        let sans = || {
            select(&block_list(), |k| {
                (k == "BLOCK_GIFS").then(|| "false".to_string())
            })
            .unwrap()
        };
        for (methode, chemin) in [
            ("GET", "/gifs/search"),
            ("GET", "/gifs/trending"),
            ("POST", "/gifs/import"),
        ] {
            assert_eq!(
                appeler(sans(), methode, chemin).await,
                StatusCode::NOT_FOUND,
                "{chemin}"
            );
            assert_eq!(
                appeler(Set::all(), methode, chemin).await,
                StatusCode::UNAUTHORIZED,
                "{chemin}"
            );
        }
    }

    #[tokio::test]
    async fn signalements_absents_quand_le_bloc_est_desactive() {
        let appeler = |set: Set, methode: &'static str, chemin: &'static str| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(
                    Request::builder()
                        .method(methode)
                        .uri(chemin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        };
        let sans = || {
            select(&block_list(), |k| {
                (k == "BLOCK_SIGNALEMENTS").then(|| "false".to_string())
            })
            .unwrap()
        };
        for (methode, chemin) in [
            ("POST", "/reports"),
            ("GET", "/reports"),
            ("GET", "/admin/reports"),
            ("PATCH", "/admin/reports/x"),
        ] {
            assert_eq!(
                appeler(sans(), methode, chemin).await,
                StatusCode::NOT_FOUND,
                "{chemin}"
            );
            assert_eq!(
                appeler(Set::all(), methode, chemin).await,
                StatusCode::UNAUTHORIZED,
                "{chemin}"
            );
        }
    }

    #[tokio::test]
    async fn badges_absent_quand_le_bloc_est_desactive() {
        let appeler = |set: Set, chemin: &'static str| async move {
            app(AppState::new(testutil::config(), vec![]), &set)
                .oneshot(Request::get(chemin).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        };
        let sans = || {
            select(&block_list(), |k| {
                (k == "BLOCK_BADGES").then(|| "false".to_string())
            })
            .unwrap()
        };
        for chemin in ["/admin/badges", "/users/x/badges", "/badges/x/icon"] {
            assert_eq!(
                appeler(sans(), chemin).await,
                StatusCode::NOT_FOUND,
                "{chemin}"
            );
            assert_eq!(
                appeler(Set::all(), chemin).await,
                StatusCode::UNAUTHORIZED,
                "{chemin}"
            );
        }
    }
}
