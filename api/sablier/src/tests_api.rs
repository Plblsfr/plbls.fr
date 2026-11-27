use axum::{Router, http::Method, http::StatusCode, routing::get};
use commun::evenements::{Donnees, Genre};
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

#[tokio::test]
async fn chaque_route_protegee_refuse_sans_session_et_sans_permission() {
    let Some(e) = env().await else { return };
    e.droits
        .lock()
        .unwrap()
        .insert("sub-sansacces".into(), vec!["admin"]);
    let (app, routes) = avec_sondes(&e);
    let mut droits_vus = Vec::new();
    let mut sessions = 0;
    for (methode, chemin, garde) in routes {
        let m = methode.as_str();
        let corps = (m != "GET" && m != "DELETE").then(|| json!({}));
        let appel = |qui: Option<&'static str>| appel_sur(&app, qui, m, chemin, corps.clone(), &[]);
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
                sessions += 1;
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
                let r = appel(Some("personne")).await;
                assert!(
                    r.statut == StatusCode::OK || r.json()["error"] == "invalid",
                    "{m} {chemin} : {}",
                    r.texte()
                );
            }
            Garde::Public(_) => {
                assert!(appel(None).await.statut.is_success(), "{m} {chemin}");
            }
        }
    }
    assert!(Droit::TOUS.iter().all(|d| droits_vus.contains(d)));
    assert_eq!(sessions, 2);
}

#[tokio::test]
async fn me_sans_permission_sablier_dit_si_la_section_est_a_afficher() {
    let Some(e) = env().await else { return };
    let r = e.get("personne", "/me").await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json(),
        json!({
            "sub": "sub-personne",
            "name": "personne",
            "email": "personne@x.fr",
            "permissions": {"access": false, "admin": false, "entracte": false, "sillon": false},
            "sections": [],
            "heartbeat_seconds": 30,
        })
    );
    let r = e.get("alice", "/me").await.json();
    assert_eq!(
        r["permissions"],
        json!({"access": true, "admin": false, "entracte": false, "sillon": false})
    );
    assert_eq!(r["sections"], json!(["stats"]));
    let r = e.get("bruno", "/me").await.json();
    assert_eq!(
        r["permissions"],
        json!({"access": true, "admin": false, "entracte": false, "sillon": true})
    );
    assert_eq!(
        e.get("admin", "/me").await.json()["sections"],
        json!(["stats", "admin"])
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
async fn stats_entracte_et_sillon_reservees_a_qui_a_l_acces_au_service() {
    let Some(e) = env().await else { return };
    let ecoute = Donnees::SillonEcoute {
        piste: "p1".into(),
        secondes: 120,
        comptee: true,
    };
    let lecture = Donnees::EntracteLecture {
        titre: "f1".into(),
        genre: Genre::Film,
        serie: None,
        secondes: 600,
    };
    let cinema = Donnees::EstompeCinemaTemps { secondes: 900 };
    for sub in ["sub-alice", "sub-bruno", "sub-diane"] {
        for (i, donnees) in [&ecoute, &lecture, &cinema].into_iter().enumerate() {
            e.recevoir(
                &format!("{sub}-{i}"),
                sub,
                "2026-03-10T10:00:00Z",
                false,
                donnees.clone(),
            )
            .await;
        }
    }
    let requete = "period=day&date=2026-03-10";
    let sans_acces = json!({"access": false});
    for (qui, entracte, sillon) in [
        ("alice", false, false),
        ("bruno", false, true),
        ("diane", true, false),
        ("admin", false, false),
    ] {
        let r = e.get(qui, &format!("/entracte?{requete}")).await;
        assert_eq!(r.statut, StatusCode::OK, "{qui}");
        if entracte {
            assert_eq!(
                (
                    r.json()["access"].clone(),
                    r.json()["seconds"].clone(),
                    r.json()["cinema_seconds"].clone()
                ),
                (json!(true), json!(600), json!(900)),
                "{qui}"
            );
        } else {
            assert_eq!(r.json(), sans_acces, "{qui}");
        }
        let r = e.get(qui, &format!("/sillon?{requete}")).await;
        assert_eq!(r.statut, StatusCode::OK, "{qui}");
        if sillon {
            assert_eq!(
                (r.json()["access"].clone(), r.json()["seconds"].clone()),
                (json!(true), json!(120)),
                "{qui}"
            );
        } else {
            assert_eq!(r.json(), sans_acces, "{qui}");
        }
    }
    for chemin in ["/entracte", "/sillon", "/messaging"] {
        let r = e.get("personne", chemin).await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN, "{chemin}");
        assert_eq!(r.json()["error"], "forbidden", "{chemin}");
        assert!(!r.texte().contains("seconds"), "{chemin}");
    }
    assert_eq!(e.get("alice", "/messaging").await.statut, StatusCode::OK);
    assert_eq!(
        e.get("alice", "/entracte?period=siecle").await.json(),
        sans_acces
    );
}

#[tokio::test]
async fn permissions_des_autres_applications_lues_dans_effective_ou_demandees() {
    let Some(e) = env().await else { return };
    e.droits
        .lock()
        .unwrap()
        .insert("sub-chloe".into(), vec!["entracte:access", "sillon:admin"]);
    let r = e.get("chloe", "/me").await.json();
    assert_eq!(
        r["permissions"],
        json!({"access": false, "admin": false, "entracte": true, "sillon": false})
    );
    let demandes: Vec<String> = e
        .aegis
        .requetes()
        .iter()
        .filter(|r| r.chemin == "/v1/check")
        .map(|r| {
            assert_eq!(r.entete("authorization"), "Bearer jeton-aegis");
            assert_eq!(r.json()["sub"], "sub-chloe");
            r.json()["permission"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(demandes.len(), 2);
    assert!(demandes.contains(&"entracte:access".to_string()));
    assert!(demandes.contains(&"sillon:access".to_string()));
    let avant = e.aegis.appels();
    e.get("chloe", "/me").await;
    assert_eq!(e.aegis.appels(), avant);
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
        json!({"access": false, "admin": false, "entracte": false, "sillon": false})
    );
    assert_eq!(r.json()["sections"], json!([]));
    let (app, _) = avec_sondes(&e);
    let r = appel_sur(&app, Some("admin"), "GET", "/sondes/access", None, &[]).await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json()["error"], "aegis_unavailable");
    let r = e
        .appel(
            Some("admin"),
            "POST",
            "/heartbeat",
            Some(json!({"platform": "web", "section": "sablier"})),
        )
        .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn sans_aegis_configure_tout_est_ferme() {
    let Some(e) = env_avec(|c| c.aegis = None).await else {
        return;
    };
    assert_eq!(
        e.get("admin", "/me").await.json()["permissions"],
        json!({"access": false, "admin": false, "entracte": false, "sillon": false})
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
        || async {
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
    assert_eq!(m["app"], "sablier");
    let p = m["permissions"].as_array().unwrap();
    let cles: Vec<&str> = p.iter().map(|x| x["key"].as_str().unwrap()).collect();
    assert_eq!(cles, ["access", "admin"]);
    assert!(p.iter().all(|x| x["open_default"] == json!(false)), "{m}");
}

#[tokio::test]
async fn origine_et_cors() {
    let Some(e) = env().await else { return };
    let battement = json!({"platform": "web", "section": "sablier"});
    let r = e
        .appel_avec(
            None,
            "POST",
            "/heartbeat",
            Some(battement.clone()),
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
            "POST",
            "/heartbeat",
            Some(battement),
            &[
                ("cookie", "__Secure-plbls_sid=alice"),
                ("origin", "https://estompe.plbls.fr"),
            ],
        )
        .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    assert_eq!(
        r.entetes["access-control-allow-origin"],
        "https://estompe.plbls.fr"
    );
    assert_eq!(r.entetes["access-control-allow-credentials"], "true");
    assert_eq!(
        r.entetes["access-control-expose-headers"],
        "Content-Disposition"
    );
    let r = e
        .appel_avec(
            None,
            "OPTIONS",
            "/heartbeat",
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
        r.entetes["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .contains("POST")
    );
    assert!(
        r.entetes["access-control-allow-headers"]
            .to_str()
            .unwrap()
            .contains("Content-Type")
    );
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
    let attendues: Vec<i64> = crate::MIGRATIONS.iter().map(|m| m.version).collect();
    assert_eq!(versions, attendues);
    assert_eq!(attendues, [1, 2, 3, 4, 5]);
    crate::MIGRATIONS.run(e.state.pool()).await.unwrap();
    e.state.pool().close().await;
    let r = e.appel(None, "GET", "/readyz", None).await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json()["error"], "not_ready");
}
