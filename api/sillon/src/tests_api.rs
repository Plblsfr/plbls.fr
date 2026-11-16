use axum::{Router, http::Method, http::StatusCode, routing::get};
use serde_json::json;

use crate::{
    droits::Droit,
    garde::{Garde, Personne},
    http::ok,
    routes::{self, Entree, entree},
    testutil::{Env, appel_sur, attendre, env, env_avec},
};

async fn sonde(p: Personne) -> axum::response::Response {
    ok(&json!({ "sub": p.sub() }))
}

fn sondes() -> Vec<Entree> {
    Droit::TOUS
        .iter()
        .map(|d| {
            let chemin: &'static str = Box::leak(format!("/sondes/{}", d.cle()).into_boxed_str());
            entree(Method::GET, chemin, Garde::Droit(*d), get(sonde))
        })
        .collect()
}

fn avec_sondes(e: &Env) -> (Router, Vec<(Method, &'static str, Garde)>) {
    let entrees: Vec<Entree> = routes::table().into_iter().chain(sondes()).collect();
    let routes = entrees
        .iter()
        .map(|(r, _)| (r.methode.clone(), r.chemin, r.garde))
        .collect();
    (routes::construire(e.state.clone(), entrees), routes)
}

fn exemple(chemin: &str) -> String {
    chemin
        .split('/')
        .map(|s| {
            if s.starts_with('{') {
                "00000000-0000-0000-0000-000000000001"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn chaque_route_protegee_refuse_sans_session_et_sans_permission() {
    let Some(e) = env().await else { return };
    e.droits
        .lock()
        .unwrap()
        .insert("sub-sansacces".into(), vec!["add", "admin"]);
    let (app, routes) = avec_sondes(&e);
    let mut droits_vus = Vec::new();
    for (methode, chemin, garde) in routes {
        let chemin = exemple(chemin);
        let m = methode.as_str();
        let corps = (m != "GET" && m != "DELETE").then(|| json!({}));
        let appel =
            |qui: Option<&'static str>| appel_sur(&app, qui, m, &chemin, corps.clone(), &[]);
        match garde {
            Garde::Droit(d) => {
                droits_vus.push(d);
                let r = appel(None).await;
                assert_eq!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} sans session"
                );
                let r = appel(Some("invalide")).await;
                assert_eq!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} session invalide"
                );
                let r = appel(Some("personne")).await;
                assert_eq!(
                    r.statut,
                    StatusCode::FORBIDDEN,
                    "{m} {chemin} sans permission"
                );
                assert!(r.texte().contains(&d.nom()), "{m} {chemin} : {}", r.texte());
                let sauf = match d {
                    Droit::Acces => "sansacces",
                    Droit::Ajout => "alice",
                    Droit::Admin => "bruno",
                };
                let r = appel(Some(sauf)).await;
                assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin} pour {sauf}");
                assert!(r.texte().contains(&d.nom()));
                let r = appel(Some("admin")).await;
                assert_ne!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} pour admin"
                );
                assert_ne!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin} pour admin");
            }
            Garde::Session(_) => {
                assert_eq!(
                    appel(None).await.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin}"
                );
                assert_eq!(
                    appel(Some("invalide")).await.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin}"
                );
                assert_eq!(
                    appel(Some("personne")).await.statut,
                    StatusCode::OK,
                    "{m} {chemin}"
                );
            }
            Garde::Webhook(_) => {
                for qui in [None, Some("admin"), Some("jeton-faux")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
            }
            Garde::Public(_) => {
                assert!(appel(None).await.statut.is_success(), "{m} {chemin}");
            }
        }
    }
    assert!(Droit::TOUS.iter().all(|d| droits_vus.contains(d)));
}

#[tokio::test]
async fn me_sans_permission_sillon_annonce_ce_qu_on_peut_ouvrir() {
    let Some(e) = env().await else { return };
    let r = e.get("personne", "/me").await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json(),
        json!({
            "sub": "sub-personne",
            "name": "personne",
            "email": "personne@x.fr",
            "permissions": {"access": false, "add": false, "admin": false},
            "sections": [],
        })
    );
    let r = e.get("alice", "/me").await.json();
    assert_eq!(
        r["permissions"],
        json!({"access": true, "add": false, "admin": false})
    );
    assert_eq!(r["sections"], json!(["library"]));
    assert_eq!(
        e.get("bruno", "/me").await.json()["sections"],
        json!(["library", "add"])
    );
    assert_eq!(
        e.get("admin", "/me").await.json()["sections"],
        json!(["library", "add", "admin"])
    );
    let r = e
        .appel_avec(
            None,
            "GET",
            "/me",
            None,
            &[("cookie", "a=b; __Secure-plbls_sid=alice")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(r.json()["sub"], "sub-alice");
    assert_eq!(r.entetes["cache-control"], "no-store");
    assert_eq!(r.entetes["x-content-type-options"], "nosniff");
    let r = e.get("panne", "/me").await;
    assert_eq!(r.statut, StatusCode::BAD_GATEWAY);
    assert_eq!(r.json()["error"], "gateway_unavailable");
}

#[tokio::test]
async fn aegis_injoignable_me_repond_et_les_permissions_sont_refusees() {
    let Some(e) = env_avec(|c| c.aegis = Some(("http://127.0.0.1:9".into(), "jeton".into()))).await
    else {
        return;
    };
    let r = e.get("admin", "/me").await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json()["permissions"],
        json!({"access": false, "add": false, "admin": false})
    );
    assert_eq!(r.json()["sections"], json!([]));
    let (app, _) = avec_sondes(&e);
    let r = appel_sur(&app, Some("admin"), "GET", "/sondes/access", None, &[]).await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json()["error"], "aegis_unavailable");
}

#[tokio::test]
async fn sans_aegis_configure_tout_est_ferme() {
    let Some(e) = env_avec(|c| c.aegis = None).await else {
        return;
    };
    assert_eq!(
        e.get("admin", "/me").await.json()["permissions"],
        json!({"access": false, "add": false, "admin": false})
    );
    let (app, _) = avec_sondes(&e);
    let r = appel_sur(&app, Some("admin"), "GET", "/sondes/admin", None, &[]).await;
    assert_eq!(r.statut, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn manifeste_envoye_a_aegis_au_demarrage() {
    let Some(e) = env().await else { return };
    e.state.demarrer_taches();
    attendre(
        || {
            e.aegis
                .requetes()
                .iter()
                .any(|r| r.chemin == "/v1/manifest")
        },
        "manifeste reçu par aegis",
    )
    .await;
    let r = e
        .aegis
        .requetes()
        .into_iter()
        .find(|r| r.chemin == "/v1/manifest")
        .unwrap();
    assert_eq!(r.methode, "PUT");
    assert_eq!(r.entete("authorization"), "Bearer jeton-aegis");
    let m = r.json();
    assert_eq!(m["app"], "sillon");
    let p = m["permissions"].as_array().unwrap();
    let cles: Vec<&str> = p.iter().map(|x| x["key"].as_str().unwrap()).collect();
    assert_eq!(cles, ["access", "add", "admin"]);
    assert!(p.iter().all(|x| x["open_default"] == json!(false)), "{m}");
}

#[tokio::test]
async fn origine_et_cors() {
    let Some(e) = env().await else { return };
    let r = e
        .appel_avec(
            None,
            "POST",
            "/me",
            Some(json!({})),
            &[
                ("cookie", "__Secure-plbls_sid=alice"),
                ("origin", "https://ailleurs.fr"),
            ],
        )
        .await;
    assert_eq!(
        (r.statut, r.json()["error"].clone()),
        (StatusCode::FORBIDDEN, json!("bad_origin"))
    );
    let r = e
        .appel_avec(
            None,
            "OPTIONS",
            "/me",
            None,
            &[("origin", "https://estompe.plbls.fr")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    assert_eq!(
        r.entetes["access-control-allow-origin"],
        "https://estompe.plbls.fr"
    );
    assert!(
        r.entetes["access-control-allow-headers"]
            .to_str()
            .unwrap()
            .contains("Authorization")
    );
    let r = e
        .appel_avec(
            None,
            "GET",
            "/me",
            None,
            &[
                ("cookie", "__Secure-plbls_sid=alice"),
                ("origin", "https://estompe.plbls.fr"),
            ],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(r.entetes["access-control-allow-credentials"], "true");
    let r = e
        .appel_avec(
            Some("alice"),
            "GET",
            "/me",
            None,
            &[("origin", "https://ailleurs.fr")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert!(!r.entetes.contains_key("access-control-allow-origin"));
    let r = e.appel(None, "GET", "/inconnue", None).await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
    assert_eq!(r.json()["error"], "not_found");
}

#[tokio::test]
async fn sante_disponibilite_et_migrations() {
    let Some(e) = env().await else { return };
    let r = e.appel(None, "GET", "/healthz", None).await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json(),
        json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")})
    );
    let r = e.appel(None, "GET", "/readyz", None).await;
    assert_eq!(
        (r.statut, r.json()),
        (StatusCode::OK, json!({"status": "ready"}))
    );
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(e.state.pool())
            .await
            .unwrap();
    assert_eq!(versions, [1, 2]);
    crate::MIGRATIONS.run(e.state.pool()).await.unwrap();
    e.state.pool().close().await;
    let r = e.appel(None, "GET", "/readyz", None).await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json()["error"], "not_ready");
}
