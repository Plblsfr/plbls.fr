use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::{
    config::{Arr, Qbittorrent},
    garde::Garde,
    routes,
    testutil::{Faux, Requete, appel_sur, env, env_avec, faux_avec_entetes, faux_json},
};

fn exemple(chemin: &str) -> String {
    chemin
        .split('/')
        .map(|s| match s {
            "{service}" => "radarr",
            s if s.starts_with('{') => "1",
            s => s,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn arr(f: &Faux) -> Option<Arr> {
    Some(Arr {
        url: f.url.clone(),
        cle: "cle".into(),
    })
}

#[tokio::test]
async fn chaque_route_protegee_refuse_sans_session_et_sans_permission() {
    let e = env().await;
    let mut protegees = 0;
    for (route, _) in routes::table() {
        let chemin = exemple(route.chemin);
        let m = route.methode.as_str();
        let appel = |qui: Option<&'static str>| appel_sur(&e.app, qui, m, &chemin, &[]);
        match route.garde {
            Garde::Droit(d) => {
                protegees += 1;
                for qui in [None, Some("invalide")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
                let r = appel(Some("personne")).await;
                assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin}");
                assert!(r.texte().contains(&d.nom()), "{m} {chemin} : {}", r.texte());
                let r = appel(Some("admin")).await;
                assert_ne!(r.statut, StatusCode::UNAUTHORIZED, "{m} {chemin}");
                assert_ne!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin}");
            }
            Garde::Session(_) => {
                for qui in [None, Some("invalide")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
                assert_eq!(appel(Some("personne")).await.statut, StatusCode::OK);
            }
            Garde::Public(_) => {
                assert_eq!(appel(None).await.statut, StatusCode::OK, "{m} {chemin}");
            }
        }
    }
    assert_eq!(protegees, 3);
}

#[tokio::test]
async fn moi_annonce_la_permission() {
    let e = env().await;
    let r = e.get("admin", "/me").await;
    assert_eq!(
        r.json(),
        json!({"sub": "sub-admin", "name": "admin", "email": "admin@x.fr", "permissions": {"admin": true}})
    );
    assert_eq!(
        e.get("personne", "/me").await.json()["permissions"]["admin"],
        false
    );
    assert_eq!(
        e.get("panne", "/me").await.statut,
        StatusCode::BAD_GATEWAY,
        "gateway en panne"
    );
}

#[tokio::test]
async fn droits_injoignables_refuses_sur_une_route_protegee() {
    let e = env_avec(|c| c.aegis = Some(("http://127.0.0.1:1".into(), "j".into()))).await;
    let r = e.get("admin", "/admin/status").await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    let r = e.get("admin", "/me").await;
    assert_eq!(r.json()["permissions"]["admin"], false);
}

#[tokio::test]
async fn origine_inconnue_refusee_avec_un_cookie() {
    let e = env().await;
    let supprimer = |entetes: &'static [(&'static str, &'static str)]| {
        appel_sur(&e.app, None, "DELETE", "/admin/queue/radarr/1", entetes)
    };
    let r = supprimer(&[
        ("cookie", "__Secure-plbls_sid=admin"),
        ("origin", "https://ailleurs.fr"),
    ])
    .await;
    assert_eq!(r.statut, StatusCode::FORBIDDEN);
    assert_eq!(r.json()["error"], "bad_origin");
    let r = supprimer(&[
        ("cookie", "__Secure-plbls_sid=admin"),
        ("origin", "https://estompe.plbls.fr"),
    ])
    .await;
    assert_eq!(r.statut, StatusCode::BAD_GATEWAY, "{}", r.texte());
    assert_eq!(r.json()["error"], "not_configured");
    assert_eq!(
        r.entetes.get("access-control-allow-origin").unwrap(),
        "https://estompe.plbls.fr"
    );
}

fn radarr(r: &Requete) -> (u16, Value) {
    if r.entete("x-api-key") != "cle" {
        return (401, json!({}));
    }
    match (r.methode.as_str(), r.chemin.as_str()) {
        ("GET", "/api/v3/system/status") => (200, json!({"version": "5.14.0"})),
        ("GET", "/api/v3/health") => (
            200,
            json!([
                {"source": "IndexerStatusCheck", "type": "warning", "message": "Indexeurs indisponibles"},
                {"source": "Autre", "type": "ok", "message": "rien"}
            ]),
        ),
        ("GET", "/api/v3/rootfolder") => (
            200,
            json!([{"path": "/films", "freeSpace": 1000, "accessible": true}, {"path": "", "freeSpace": 1}]),
        ),
        ("GET", "/api/v3/queue") => (
            200,
            json!({"records": [
                {"id": 7, "title": "Film.2024.1080p", "status": "completed", "trackedDownloadStatus": "warning",
                 "trackedDownloadState": "importBlocked", "statusMessages": [{"title": "Film.mkv", "messages": ["Qualité inférieure"]}]},
                {"id": 8, "title": "Sain", "status": "downloading", "trackedDownloadStatus": "ok"}
            ]}),
        ),
        ("DELETE", "/api/v3/queue/7") => (200, json!({})),
        ("DELETE", _) => (404, json!({})),
        _ => (500, json!({})),
    }
}

fn prowlarr(r: &Requete) -> (u16, Value) {
    match (r.methode.as_str(), r.chemin.as_str()) {
        ("GET", "/api/v1/system/status") => (200, json!({"version": "1.30.2"})),
        ("GET", "/api/v1/health") => (200, json!([])),
        ("GET", "/api/v1/indexer") => (
            200,
            json!([
                {"id": 1, "name": "Alpha", "enable": true, "protocol": "torrent", "privacy": "private"},
                {"id": 2, "name": "Bêta", "enable": false, "protocol": "torrent", "privacy": "public"}
            ]),
        ),
        ("GET", "/api/v1/indexerstatus") => (
            200,
            json!([{"indexerId": 1, "disabledTill": "2026-11-17T12:00:00Z", "mostRecentFailure": "2026-11-17T10:00:00Z"}]),
        ),
        ("GET", "/api/v1/indexer/1") => (200, json!({"id": 1, "name": "Alpha"})),
        ("GET", "/api/v1/indexer/2") => (200, json!({"id": 2, "name": "Bêta"})),
        ("POST", "/api/v1/indexer/test") if r.json()["id"] == 1 => (
            400,
            json!([{"propertyName": "", "errorMessage": "Connexion refusée par le site"}]),
        ),
        ("POST", "/api/v1/indexer/test") => (200, json!({})),
        _ => (404, json!({})),
    }
}

async fn qbittorrent(mot_de_passe: &'static str) -> Faux {
    faux_avec_entetes(move |r| match r.chemin.as_str() {
        "/api/v2/auth/login" => {
            if r.texte() == format!("username=admin&password={mot_de_passe}") {
                (
                    200,
                    vec![("set-cookie", "QBT_SID_8080=abc; HttpOnly; path=/".into())],
                    b"Ok.".to_vec(),
                )
            } else {
                (200, vec![], b"Fails.".to_vec())
            }
        }
        _ if r.entete("cookie") != "QBT_SID_8080=abc" => (403, vec![], b"Forbidden".to_vec()),
        "/api/v2/app/version" => (200, vec![], b"v5.1.0".to_vec()),
        "/api/v2/transfer/info" => (
            200,
            vec![],
            json!({"dl_info_speed": 2048, "up_info_speed": 512, "connection_status": "connected"})
                .to_string()
                .into_bytes(),
        ),
        _ => (404, vec![], vec![]),
    })
    .await
}

fn service<'a>(v: &'a Value, cle: &str) -> &'a Value {
    v["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == cle)
        .unwrap()
}

#[tokio::test]
async fn etat_agrege_les_services() {
    let (f_radarr, f_prowlarr) = (faux_json(radarr).await, faux_json(prowlarr).await);
    let f_qbit = qbittorrent("secret").await;
    let e = env_avec(|c| {
        c.radarr = arr(&f_radarr);
        c.sonarr = Some(Arr {
            url: "http://127.0.0.1:1".into(),
            cle: "k".into(),
        });
        c.prowlarr = arr(&f_prowlarr);
        c.qbittorrent = Some(Qbittorrent {
            url: f_qbit.url.clone(),
            utilisateur: "admin".into(),
            mot_de_passe: "secret".into(),
        });
    })
    .await;
    let r = e.get("admin", "/admin/status").await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let v = r.json();
    let cles: Vec<&str> = v["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        cles,
        ["radarr", "sonarr", "lidarr", "prowlarr", "qbittorrent"]
    );
    assert_eq!(
        service(&v, "radarr"),
        &json!({"key": "radarr", "name": "Radarr", "configured": true, "reachable": true, "version": "5.14.0", "error": null,
            "alerts": [{"level": "warning", "source": "IndexerStatusCheck", "message": "Indexeurs indisponibles"}]})
    );
    let sonarr = service(&v, "sonarr");
    assert_eq!(
        (&sonarr["configured"], &sonarr["reachable"]),
        (&json!(true), &json!(false))
    );
    let erreur = sonarr["error"].as_str().unwrap();
    assert!(erreur.starts_with("injoignable"), "{erreur}");
    assert!(!erreur.contains("127.0.0.1"), "{erreur}");
    assert_eq!(service(&v, "lidarr")["configured"], false);
    assert_eq!(
        service(&v, "qbittorrent")["transfer"],
        json!({"download_speed": 2048, "upload_speed": 512, "connection": "connected"})
    );
    assert_eq!(service(&v, "qbittorrent")["version"], "5.1.0");
    assert_eq!(
        v["disks"],
        json!([{"service": "radarr", "path": "/films", "free": 1000, "accessible": true}])
    );
    assert_eq!(
        v["queue"],
        json!([{"service": "radarr", "id": 7, "title": "Film.2024.1080p", "level": "warning", "state": "importBlocked",
            "messages": ["Qualité inférieure"]}])
    );
    assert_eq!(
        v["indexers"],
        json!([
            {"id": 1, "name": "Alpha", "enabled": true, "protocol": "torrent", "privacy": "private", "failing": true,
             "disabled_until": "2026-11-17T12:00:00Z", "last_failure": "2026-11-17T10:00:00Z"},
            {"id": 2, "name": "Bêta", "enabled": false, "protocol": "torrent", "privacy": "public", "failing": false,
             "disabled_until": null, "last_failure": null}
        ])
    );
    let texte = r.texte();
    assert!(!texte.contains(&f_radarr.url) && !texte.contains("cle") && !texte.contains("secret"));
    e.get("admin", "/admin/status").await;
    let connexions = f_qbit
        .requetes()
        .iter()
        .filter(|q| q.chemin == "/api/v2/auth/login")
        .count();
    assert_eq!(connexions, 1, "la session de qBittorrent est réutilisée");
}

#[tokio::test]
async fn qbittorrent_identifiants_refuses() {
    let f = qbittorrent("autre").await;
    let e = env_avec(|c| {
        c.qbittorrent = Some(Qbittorrent {
            url: f.url.clone(),
            utilisateur: "admin".into(),
            mot_de_passe: "secret".into(),
        });
    })
    .await;
    let v = e.get("admin", "/admin/status").await.json();
    let q = service(&v, "qbittorrent");
    assert_eq!(
        (&q["reachable"], &q["error"]),
        (&json!(false), &json!("identifiants refusés"))
    );
}

#[tokio::test]
async fn cle_refusee_signalee() {
    let f = faux_json(radarr).await;
    let e = env_avec(|c| {
        c.radarr = Some(Arr {
            url: f.url.clone(),
            cle: "fausse".into(),
        });
    })
    .await;
    let v = e.get("admin", "/admin/status").await.json();
    assert_eq!(service(&v, "radarr")["error"], "clé d'API refusée");
}

#[tokio::test]
async fn test_d_un_indexeur() {
    let f = faux_json(prowlarr).await;
    let e = env_avec(|c| c.prowlarr = arr(&f)).await;
    let tester = |id: &'static str| {
        let chemin = format!("/admin/indexers/{id}/test");
        let e = &e;
        async move { e.appel(Some("admin"), "POST", &chemin).await }
    };
    assert_eq!(
        tester("2").await.json(),
        json!({"ok": true, "message": null})
    );
    assert_eq!(
        tester("1").await.json(),
        json!({"ok": false, "message": "Connexion refusée par le site"})
    );
    assert_eq!(tester("9").await.statut, StatusCode::NOT_FOUND);
    assert_eq!(tester("abc").await.statut, StatusCode::BAD_REQUEST);
    let sans = env().await;
    let r = sans
        .appel(Some("admin"), "POST", "/admin/indexers/1/test")
        .await;
    assert_eq!(r.json()["error"], "not_configured");
}

#[tokio::test]
async fn retrait_d_un_element_de_la_file() {
    let f = faux_json(radarr).await;
    let e = env_avec(|c| c.radarr = arr(&f)).await;
    let supprimer = |chemin: &'static str| e.appel(Some("admin"), "DELETE", chemin);
    assert_eq!(
        supprimer("/admin/queue/radarr/7").await.statut,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        supprimer("/admin/queue/radarr/7?retry=true").await.statut,
        StatusCode::NO_CONTENT
    );
    let envoyees: Vec<String> = f
        .requetes()
        .iter()
        .filter(|r| r.methode == "DELETE")
        .map(|r| r.query.clone())
        .collect();
    assert_eq!(
        envoyees,
        [
            "removeFromClient=true&blocklist=false&skipRedownload=false",
            "removeFromClient=true&blocklist=true&skipRedownload=false"
        ]
    );
    assert_eq!(
        supprimer("/admin/queue/radarr/99").await.statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        supprimer("/admin/queue/prowlarr/7").await.statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        supprimer("/admin/queue/sonarr/7").await.json()["error"],
        "not_configured"
    );
}
