use axum::http::StatusCode;
use serde_json::{Value, json};

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

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
    assert_eq!(protegees, 23);
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
                 "downloadId": "A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1", "movie": {"title": "Le Film", "year": 2024},
                 "trackedDownloadState": "importBlocked", "statusMessages": [{"title": "Film.mkv", "messages": ["Qualité inférieure"]}]},
                {"id": 8, "title": "Sain", "status": "downloading", "trackedDownloadStatus": "ok"}
            ]}),
        ),
        ("GET", "/api/v3/history") if r.query.contains("downloadId=B2B2") => (
            200,
            json!({"records": [{"movie": {"title": "Ancien Film", "year": 1999}}]}),
        ),
        ("GET", "/api/v3/history") => (200, json!({"records": []})),
        ("GET", "/api/v3/manualimport") => {
            let dossier = r
                .query
                .split('&')
                .find_map(|p| p.strip_prefix("folder="))
                .unwrap_or_default()
                .replace("%2F", "/");
            (
                200,
                json!([
                    {"path": format!("{dossier}/Film.2024/film.mkv"), "folderName": "Film.2024", "size": 5,
                     "movie": {"id": 4, "title": "Le Film", "year": 2024}, "quality": {"quality": {"id": 7}},
                     "languages": [{"id": 2}], "releaseGroup": "GRP", "rejections": []},
                    {"path": format!("{dossier}/Film.2024/Subs/fr.srt"), "size": 3,
                     "rejections": [{"reason": "Unknown Movie", "type": "permanent"}]},
                    {"path": "/etc/passwd", "movie": {"id": 4, "title": "Ailleurs"}, "rejections": []}
                ]),
            )
        }
        ("POST", "/api/v3/command") => (201, json!({})),
        ("GET", "/api/v3/movie") => (
            200,
            json!([
                {"id": 4, "title": "Le Film", "year": 2024},
                {"id": 5, "title": "Le Film du dimanche", "year": 1987},
                {"id": 6, "title": "Autre chose", "year": 2001}
            ]),
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

const H1: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const H2: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

struct FauxQbit {
    faux: Faux,
    progression: Arc<Mutex<f64>>,
}

impl FauxQbit {
    fn ecritures(&self) -> Vec<(String, String)> {
        self.faux
            .requetes()
            .iter()
            .filter(|r| r.methode == "POST" && r.chemin != "/api/v2/auth/login")
            .map(|r| (r.chemin.replace("/api/v2", ""), r.texte()))
            .collect()
    }
}

async fn qbit_complet() -> FauxQbit {
    let progression = Arc::new(Mutex::new(0.25));
    let lue = progression.clone();
    let faux = faux_avec_entetes(move |r| {
        let json = |v: Value| (200, vec![], v.to_string().into_bytes());
        match (r.methode.as_str(), r.chemin.as_str()) {
            ("POST", "/api/v2/auth/login") => (
                200,
                vec![("set-cookie", "SID=abc; HttpOnly".into())],
                b"Ok.".to_vec(),
            ),
            _ if r.entete("cookie") != "SID=abc" => (403, vec![], vec![]),
            ("GET", "/api/v2/sync/maindata") => json(json!({
                "torrents": {
                    H1: {"name": "Film.2024.1080p", "state": "downloading", "progress": *lue.lock().unwrap(), "size": 4000,
                         "dlspeed": 1024, "added_on": 20, "category": "radarr", "tags": "vf"},
                    H2: {"name": "Ancien.Film.1999", "state": "stalledUP", "progress": 1.0, "size": 700, "added_on": 10, "ratio": 2.5}
                },
                "categories": {"radarr": {}, "sonarr": {}},
                "tags": ["vf"],
                "server_state": {"dl_info_speed": 1024, "use_alt_speed_limits": false, "free_space_on_disk": 9000}
            })),
            ("GET", "/api/v2/torrents/files") if r.query == format!("hash={H2}") => json(json!([
                {"index": 0, "name": "Ancien.Film.1999/film.mkv", "size": 690, "progress": 1.0, "priority": 1},
                {"index": 1, "name": "Ancien.Film.1999/sample.mkv", "size": 10, "progress": 0.0, "priority": 0}
            ])),
            ("GET", "/api/v2/torrents/files") => (404, vec![], vec![]),
            ("GET", "/api/v2/transfer/speedLimitsMode") => (200, vec![], b"0".to_vec()),
            ("POST", "/api/v2/torrents/stop" | "/api/v2/torrents/start") => (404, vec![], vec![]),
            ("POST", "/api/v2/torrents/filePrio") if !r.texte().contains(H2) => (404, vec![], vec![]),
            ("POST", _) => (200, vec![], vec![]),
            _ => (404, vec![], vec![]),
        }
    })
    .await;
    FauxQbit { faux, progression }
}

fn avec_qbit(c: &mut crate::config::Config, f: &FauxQbit) {
    c.periode_suivi = Duration::from_millis(20);
    c.qbittorrent = Some(Qbittorrent {
        url: f.faux.url.clone(),
        utilisateur: "admin".into(),
        mot_de_passe: "secret".into(),
    });
}

#[tokio::test]
async fn telechargements_rattaches_et_suivis_en_direct() {
    let (q, f_radarr) = (qbit_complet().await, faux_json(radarr).await);
    let e = env_avec(|c| {
        avec_qbit(c, &q);
        c.radarr = arr(&f_radarr);
    })
    .await;
    let r = e.get("admin", "/admin/downloads").await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let v = r.json();
    let version = v["version"].as_u64().unwrap();
    assert!(version >= 1);
    assert_eq!(v["error"], Value::Null);
    let t = v["torrents"].as_array().unwrap();
    assert_eq!((&t[0]["hash"], &t[1]["hash"]), (&json!(H1), &json!(H2)));
    assert_eq!(
        t[0]["title"],
        json!({"service": "radarr", "title": "Le Film (2024)", "detail": null})
    );
    assert_eq!(t[1]["title"], Value::Null);
    assert_eq!(
        (&t[0]["progress"], &t[0]["tags"]),
        (&json!(0.25), &json!(["vf"]))
    );
    assert_eq!(v["categories"], json!(["radarr", "sonarr"]));
    assert_eq!(v["transfer"]["free_space"], 9000);
    let attente = {
        let app = e.app.clone();
        let chemin = format!("/admin/downloads?since={version}");
        tokio::spawn(async move { appel_sur(&app, Some("admin"), "GET", &chemin, &[]).await })
    };
    *q.progression.lock().unwrap() = 0.5;
    let v = attente.await.unwrap().json();
    assert!(v["version"].as_u64().unwrap() > version);
    assert_eq!(v["torrents"][0]["progress"], 0.5);
    let sans = env().await;
    assert_eq!(
        sans.get("admin", "/admin/downloads").await.json()["error"],
        "not_configured"
    );
}

#[tokio::test]
async fn qbittorrent_injoignable_annonce_dans_le_suivi() {
    let e = env_avec(|c| {
        c.periode_suivi = Duration::from_millis(20);
        c.qbittorrent = Some(Qbittorrent {
            url: "http://127.0.0.1:1".into(),
            utilisateur: String::new(),
            mot_de_passe: String::new(),
        });
    })
    .await;
    let v = e.get("admin", "/admin/downloads").await.json();
    let erreur = v["error"].as_str().unwrap();
    assert!(erreur.starts_with("injoignable"), "{erreur}");
    assert!(!erreur.contains("127.0.0.1"), "{erreur}");
}

#[tokio::test]
async fn actions_sur_les_telechargements() {
    let q = qbit_complet().await;
    let e = env_avec(|c| avec_qbit(c, &q)).await;
    let envoyer = |methode: &'static str, chemin: &'static str, corps: Value| {
        let app = e.app.clone();
        async move {
            let req = axum::http::Request::builder()
                .method(methode)
                .uri(chemin)
                .header("authorization", "Bearer admin")
                .body(axum::body::Body::from(corps.to_string()))
                .unwrap();
            tower::ServiceExt::oneshot(app, req).await.unwrap().status()
        }
    };
    let deux = json!([H1, H2.to_ascii_uppercase()]);
    let cible = format!("{H1}|{H2}");
    let ok = StatusCode::NO_CONTENT;
    assert_eq!(
        envoyer("POST", "/admin/downloads/pause", json!({"hashes": deux})).await,
        ok
    );
    assert_eq!(
        envoyer("POST", "/admin/downloads/resume", json!({"hashes": [H1]})).await,
        ok
    );
    assert_eq!(
        envoyer("POST", "/admin/downloads/recheck", json!({"hashes": [H1]})).await,
        ok
    );
    assert_eq!(
        envoyer(
            "POST",
            "/admin/downloads/delete",
            json!({"hashes": [H2], "delete_files": true})
        )
        .await,
        ok
    );
    assert_eq!(
        envoyer(
            "POST",
            "/admin/downloads/category",
            json!({"hashes": [H1], "category": "sonarr"})
        )
        .await,
        ok
    );
    assert_eq!(
        envoyer(
            "POST",
            "/admin/downloads/tags",
            json!({"hashes": [H1], "add": ["4k", "hdr"], "remove": ["vf"]})
        )
        .await,
        ok
    );
    assert_eq!(
        envoyer(
            "POST",
            "/admin/downloads/limits",
            json!({"hashes": [H1], "download": 1000, "upload": 0})
        )
        .await,
        ok
    );
    assert_eq!(
        envoyer(
            "PUT",
            "/admin/transfer",
            json!({"download": 5000, "upload": 100, "alternative": true})
        )
        .await,
        ok
    );
    let attendu: Vec<(String, String)> = [
        (
            "/torrents/stop",
            format!("hashes={}", cible.replace('|', "%7C")),
        ),
        (
            "/torrents/pause",
            format!("hashes={}", cible.replace('|', "%7C")),
        ),
        ("/torrents/start", format!("hashes={H1}")),
        ("/torrents/resume", format!("hashes={H1}")),
        ("/torrents/recheck", format!("hashes={H1}")),
        ("/torrents/delete", format!("hashes={H2}&deleteFiles=true")),
        (
            "/torrents/setCategory",
            format!("hashes={H1}&category=sonarr"),
        ),
        ("/torrents/removeTags", format!("hashes={H1}&tags=vf")),
        ("/torrents/addTags", format!("hashes={H1}&tags=4k%2Chdr")),
        (
            "/torrents/setDownloadLimit",
            format!("hashes={H1}&limit=1000"),
        ),
        ("/torrents/setUploadLimit", format!("hashes={H1}&limit=0")),
        ("/transfer/setDownloadLimit", "limit=5000".to_string()),
        ("/transfer/setUploadLimit", "limit=100".to_string()),
        ("/transfer/toggleSpeedLimitsMode", String::new()),
    ]
    .into_iter()
    .map(|(c, t)| (c.to_string(), t))
    .collect();
    assert_eq!(q.ecritures(), attendu);
    let refuses = [
        ("POST", "/admin/downloads/pause", json!({"hashes": []})),
        ("POST", "/admin/downloads/pause", json!({"hashes": ["all"]})),
        ("POST", "/admin/downloads/pause", json!({"hashes": [H1, 3]})),
        ("POST", "/admin/downloads/delete", json!({"hashes": [H1]})),
        (
            "POST",
            "/admin/downloads/category",
            json!({"hashes": [H1], "category": "ailleurs"}),
        ),
        ("POST", "/admin/downloads/tags", json!({"hashes": [H1]})),
        (
            "POST",
            "/admin/downloads/tags",
            json!({"hashes": [H1], "add": ["a,b"]}),
        ),
        (
            "POST",
            "/admin/downloads/limits",
            json!({"hashes": [H1], "download": -1, "upload": 0}),
        ),
        (
            "PUT",
            "/admin/transfer",
            json!({"download": 0, "upload": 0}),
        ),
        ("PUT", "/admin/transfer", json!([])),
    ];
    for (m, chemin, corps) in refuses {
        assert_eq!(
            envoyer(m, chemin, corps.clone()).await,
            StatusCode::BAD_REQUEST,
            "{m} {chemin} {corps}"
        );
    }
    assert_eq!(q.ecritures().len(), attendu.len(), "un refus n'écrit rien");
    assert_eq!(
        envoyer(
            "PUT",
            "/admin/transfer",
            json!({"download": 0, "upload": 0, "alternative": false})
        )
        .await,
        ok
    );
    assert_eq!(
        q.ecritures().len(),
        attendu.len() + 2,
        "mode déjà en place : pas de bascule"
    );
}

#[tokio::test]
async fn detail_et_priorites_des_fichiers() {
    let (q, f_radarr) = (qbit_complet().await, faux_json(radarr).await);
    let e = env_avec(|c| {
        avec_qbit(c, &q);
        c.radarr = arr(&f_radarr);
    })
    .await;
    let r = e
        .get(
            "admin",
            &format!("/admin/downloads/{}", H2.to_ascii_uppercase()),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let v = r.json();
    assert_eq!(v["hash"], H2);
    assert_eq!(
        v["title"],
        json!({"service": "radarr", "title": "Ancien Film (1999)", "detail": null})
    );
    assert_eq!(
        v["files"][1],
        json!({"index": 1, "name": "Ancien.Film.1999/sample.mkv", "size": 10, "progress": 0.0, "priority": 0})
    );
    assert_eq!(
        e.get("admin", &format!("/admin/downloads/{H1}"))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get("admin", "/admin/downloads/..%2Fapp").await.statut,
        StatusCode::NOT_FOUND
    );
    let prioriser = |hash: &'static str, corps: Value| {
        let app = e.app.clone();
        async move {
            let req = axum::http::Request::builder()
                .method("POST")
                .uri(format!("/admin/downloads/{hash}/files"))
                .header("authorization", "Bearer admin")
                .body(axum::body::Body::from(corps.to_string()))
                .unwrap();
            tower::ServiceExt::oneshot(app, req).await.unwrap().status()
        }
    };
    assert_eq!(
        prioriser(H2, json!({"ids": [0, 1], "priority": 7})).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        q.ecritures().last().unwrap(),
        &(
            "/torrents/filePrio".to_string(),
            format!("hash={H2}&id=0%7C1&priority=7")
        )
    );
    assert_eq!(
        prioriser(H1, json!({"ids": [0], "priority": 1})).await,
        StatusCode::NOT_FOUND
    );
    for corps in [
        json!({"ids": [], "priority": 1}),
        json!({"ids": [0], "priority": 3}),
        json!({"ids": [-1], "priority": 1}),
    ] {
        assert_eq!(
            prioriser(H2, corps.clone()).await,
            StatusCode::BAD_REQUEST,
            "{corps}"
        );
    }
}

type EtatTorrent = Arc<Mutex<Option<(&'static str, f64)>>>;

async fn qbit_depots(etat: EtatTorrent) -> Faux {
    faux_avec_entetes(move |r| {
        let json = |v: Value| (200, vec![], v.to_string().into_bytes());
        match (r.methode.as_str(), r.chemin.as_str()) {
            ("POST", "/api/v2/auth/login") => {
                (200, vec![("set-cookie", "SID=abc".into())], b"Ok.".to_vec())
            }
            ("GET", "/api/v2/torrents/info") => match *etat.lock().unwrap() {
                Some((e, p)) => json(json!([{"state": e, "progress": p}])),
                None => json(json!([])),
            },
            ("GET", "/api/v2/torrents/files") => json(json!([
                {"index": 0, "name": "Film.2024/film.mkv", "size": 5, "progress": 1.0, "priority": 1},
                {"index": 1, "name": "Film.2024/Subs/fr.srt", "size": 3, "progress": 0.0, "priority": 1}
            ])),
            ("GET", "/api/v2/sync/maindata") => json(json!({})),
            ("POST", "/api/v2/torrents/add") => {
                *etat.lock().unwrap() = Some(("checkingResumeData", 0.0));
                (200, vec![], b"Ok.".to_vec())
            }
            ("POST", "/api/v2/torrents/delete") => {
                *etat.lock().unwrap() = None;
                (200, vec![], vec![])
            }
            ("POST", "/api/v2/torrents/start") => (404, vec![], vec![]),
            ("POST", _) => (200, vec![], vec![]),
            _ => (404, vec![], vec![]),
        }
    })
    .await
}

async fn brut(
    app: &axum::Router,
    methode: &str,
    chemin: &str,
    corps: &[u8],
) -> crate::testutil::Rep {
    let req = axum::http::Request::builder()
        .method(methode)
        .uri(chemin)
        .header("authorization", "Bearer admin")
        .body(axum::body::Body::from(corps.to_vec()))
        .unwrap();
    let r = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    let statut = r.status();
    let entetes = r.headers().clone();
    let corps = axum::body::to_bytes(r.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    crate::testutil::Rep {
        statut,
        entetes,
        corps,
    }
}

fn avec_depots(c: &mut crate::config::Config, dossier: &std::path::Path, q: &Faux) {
    c.periode_suivi = Duration::from_millis(20);
    c.qbittorrent = Some(Qbittorrent {
        url: q.url.clone(),
        utilisateur: "admin".into(),
        mot_de_passe: "secret".into(),
    });
    c.depots = Some(crate::config::Depots {
        dossier: dossier.to_path_buf(),
        vu_par_qbittorrent: "/downloads/depots".into(),
        vu_par_arr: "/data/depots".into(),
        plafond: 1 << 20,
    });
}

fn ecritures(q: &Faux) -> Vec<String> {
    q.requetes()
        .iter()
        .filter(|r| r.methode == "POST" && r.chemin != "/api/v2/auth/login")
        .map(|r| r.chemin.replace("/api/v2/torrents/", ""))
        .collect()
}

async fn envoyer_fichiers(app: &axum::Router, id: &str) {
    let fichier = |i: usize, suite: &str| format!("/admin/deposits/{id}/files/{i}{suite}");
    let r = brut(app, "PUT", &fichier(0, "?offset=0"), b"fil").await;
    assert_eq!(
        (r.statut, r.json()),
        (StatusCode::OK, json!({"received": 3}))
    );
    let r = brut(app, "PUT", &fichier(0, "?offset=0"), b"fil").await;
    assert_eq!(r.statut, StatusCode::CONFLICT);
    assert_eq!(
        (&r.json()["error"], &r.json()["received"]),
        (&json!("bad_offset"), &json!(3))
    );
    assert_eq!(
        brut(app, "PUT", &fichier(0, "?offset=3"), b"m.x")
            .await
            .statut,
        StatusCode::BAD_REQUEST,
        "au-delà de la taille annoncée"
    );
    assert_eq!(
        brut(app, "PUT", &fichier(0, ""), b"m.").await.statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        brut(app, "PUT", &fichier(2, "?offset=0"), b"x")
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    let r = brut(app, "POST", &format!("/admin/deposits/{id}/seed"), b"").await;
    assert_eq!(r.json()["error"], "incomplete");
    assert_eq!(
        brut(app, "PUT", &fichier(0, "?offset=3"), b"m.")
            .await
            .json(),
        json!({"received": 5})
    );
    assert_eq!(
        brut(app, "PUT", &fichier(1, "?offset=0"), b"srt")
            .await
            .json(),
        json!({"received": 3})
    );
}

#[tokio::test]
async fn depot_verifie_partage_puis_importe() {
    let dossier = tempfile::tempdir().unwrap();
    let etat: EtatTorrent = Arc::default();
    let (q, f_radarr) = (qbit_depots(etat.clone()).await, faux_json(radarr).await);
    let e = env_avec(|c| {
        avec_depots(c, dossier.path(), &q);
        c.radarr = arr(&f_radarr);
    })
    .await;
    let source = crate::bencode::tests::torrent_multi();
    let r = brut(&e.app, "POST", "/admin/deposits", &source).await;
    assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
    let v = r.json();
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(
        (
            &v["name"],
            &v["state"],
            &v["size"],
            &v["received"],
            &v["tracker"],
            &v["private"]
        ),
        (
            &json!("Film.2024"),
            &json!("upload"),
            &json!(8),
            &json!(0),
            &json!("tracker.exemple.fr"),
            &json!(true)
        )
    );
    assert_eq!(
        v["files"],
        json!([
            {"index": 0, "path": "Film.2024/film.mkv", "size": 5, "received": 0, "valid": null},
            {"index": 1, "path": "Film.2024/Subs/fr.srt", "size": 3, "received": 0, "valid": null}
        ])
    );
    envoyer_fichiers(&e.app, &id).await;
    let repris = brut(&e.app, "POST", "/admin/deposits", &source).await;
    assert_eq!(
        (repris.statut, &repris.json()["received"]),
        (StatusCode::OK, &json!(8))
    );
    let contenu = dossier.path().join(&id).join("contenu/Film.2024");
    assert_eq!(std::fs::read(contenu.join("film.mkv")).unwrap(), b"film.");
    assert_eq!(std::fs::read(contenu.join("Subs/fr.srt")).unwrap(), b"srt");
    let attendu = format!("/admin/deposits/{id}/import?service=radarr");
    assert_eq!(e.get("admin", &attendu).await.json()["error"], "bad_state");

    let r = brut(&e.app, "POST", &format!("/admin/deposits/{id}/seed"), b"").await;
    assert_eq!(
        (r.statut, &r.json()["state"]),
        (StatusCode::OK, &json!("checking"))
    );
    let ajout = q
        .requetes()
        .into_iter()
        .find(|x| x.chemin == "/api/v2/torrents/add")
        .unwrap();
    let envoye = ajout.texte();
    assert!(
        ajout
            .entete("content-type")
            .starts_with("multipart/form-data; boundary=regie-")
    );
    assert!(
        envoye.contains(&format!(
            "name=\"savepath\"\r\n\r\n/downloads/depots/{id}/contenu\r\n"
        )),
        "{envoye}"
    );
    assert!(
        envoye.contains("name=\"stopped\"\r\n\r\ntrue")
            && envoye.contains("name=\"tags\"\r\n\r\nregie")
    );
    assert!(envoye.contains(std::str::from_utf8(&source).unwrap()));
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            (mode(&contenu.join("film.mkv")), mode(&contenu.join("Subs"))),
            (0o664, 0o775)
        );
    }
    assert_eq!(
        brut(&e.app, "POST", &format!("/admin/deposits/{id}/seed"), b"")
            .await
            .json()["error"],
        "bad_state"
    );
    assert_eq!(
        brut(
            &e.app,
            "PUT",
            &format!("/admin/deposits/{id}/files/0?offset=0&reset=true"),
            b"x"
        )
        .await
        .json()["error"],
        "bad_state"
    );

    let attente = {
        let app = e.app.clone();
        let chemin = format!("/admin/deposits/{id}?wait=checking");
        tokio::spawn(async move { appel_sur(&app, Some("admin"), "GET", &chemin, &[]).await })
    };
    *etat.lock().unwrap() = Some(("stoppedUP", 1.0));
    let v = attente.await.unwrap().json();
    assert_eq!(
        (&v["state"], &v["error"]),
        (&json!("seeding"), &Value::Null)
    );
    assert_eq!(ecritures(&q), ["add", "recheck", "start", "resume"]);

    let v = e.get("admin", &attendu).await.json();
    assert_eq!(
        v,
        json!({"service": "radarr", "items": [
            {"path": "Film.2024/film.mkv", "size": 5, "title": {"service": "radarr", "title": "Le Film (2024)", "detail": null},
             "recognized": true, "reasons": []},
            {"path": "Film.2024/Subs/fr.srt", "size": 3, "title": null, "recognized": false, "reasons": ["Unknown Movie"]}
        ]})
    );
    assert_eq!(
        f_radarr
            .requetes()
            .iter()
            .find(|x| x.chemin == "/api/v3/manualimport")
            .unwrap()
            .query,
        format!("folder=%2Fdata%2Fdepots%2F{id}%2Fcontenu&filterExistingFiles=false")
    );
    let importer = |corps: Value| {
        let (app, chemin) = (e.app.clone(), format!("/admin/deposits/{id}/import"));
        async move { brut(&app, "POST", &chemin, corps.to_string().as_bytes()).await }
    };
    let r = importer(json!({"service": "radarr"})).await;
    assert_eq!(
        (r.statut, r.json()),
        (StatusCode::OK, json!({"service": "radarr", "files": 1}))
    );
    let commande = f_radarr
        .requetes()
        .into_iter()
        .rev()
        .find(|x| x.chemin == "/api/v3/command")
        .unwrap()
        .json();
    assert_eq!(
        commande,
        json!({"name": "ManualImport", "importMode": "copy", "files": [{
            "path": format!("/data/depots/{id}/contenu/Film.2024/film.mkv"), "folderName": "Film.2024", "movieId": 4,
            "quality": {"quality": {"id": 7}}, "languages": [{"id": 2}], "releaseGroup": "GRP", "indexerFlags": 0}]})
    );
    let r = importer(json!({"service": "radarr", "movie_id": 5})).await;
    assert_eq!(
        r.json()["files"],
        2,
        "un film choisi à la main prend tous les fichiers du dépôt"
    );
    let commande = f_radarr
        .requetes()
        .into_iter()
        .rev()
        .find(|x| x.chemin == "/api/v3/command")
        .unwrap()
        .json();
    assert!(
        commande["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["movieId"] == 5)
    );
    for refuse in [
        json!({"service": "prowlarr"}),
        json!({"service": "radarr", "movie_id": "x"}),
        json!({}),
    ] {
        assert_eq!(
            importer(refuse.clone()).await.statut,
            StatusCode::BAD_REQUEST,
            "{refuse}"
        );
    }
    assert_eq!(
        importer(json!({"service": "sonarr"})).await.json()["error"],
        "not_configured"
    );
    let v = e
        .get("admin", &format!("/admin/deposits/{id}"))
        .await
        .json();
    assert_eq!(v["imported"], json!(["radarr"]));
    let liste = e.get("admin", "/admin/deposits").await.json();
    assert_eq!(
        (&liste["items"][0]["id"], &liste["used"], &liste["limit"]),
        (&json!(id), &json!(8), &json!(1 << 20))
    );

    let r = e
        .appel(Some("admin"), "DELETE", &format!("/admin/deposits/{id}"))
        .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    let suppression = q
        .requetes()
        .into_iter()
        .rev()
        .find(|x| x.chemin == "/api/v2/torrents/delete")
        .unwrap();
    assert_eq!(
        suppression.texte(),
        format!("hashes={id}&deleteFiles=false")
    );
    assert!(!dossier.path().join(&id).exists());
    assert_eq!(
        e.get("admin", &format!("/admin/deposits/{id}"))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn depot_dont_un_fichier_ne_correspond_pas() {
    let dossier = tempfile::tempdir().unwrap();
    let etat: EtatTorrent = Arc::default();
    let q = qbit_depots(etat.clone()).await;
    let e = env_avec(|c| avec_depots(c, dossier.path(), &q)).await;
    let v = brut(
        &e.app,
        "POST",
        "/admin/deposits",
        &crate::bencode::tests::torrent_multi(),
    )
    .await
    .json();
    let id = v["id"].as_str().unwrap().to_string();
    envoyer_fichiers(&e.app, &id).await;
    let partager = || {
        let (app, chemin) = (e.app.clone(), format!("/admin/deposits/{id}/seed"));
        async move { brut(&app, "POST", &chemin, b"").await }
    };
    let attendre = || {
        let (app, chemin) = (e.app.clone(), format!("/admin/deposits/{id}?wait=checking"));
        tokio::spawn(async move { appel_sur(&app, Some("admin"), "GET", &chemin, &[]).await })
    };
    assert_eq!(partager().await.statut, StatusCode::OK);
    let attente = attendre();
    *etat.lock().unwrap() = Some(("stoppedDL", 0.625));
    let v = attente.await.unwrap().json();
    assert_eq!(v["state"], "mismatch");
    assert_eq!(
        (&v["files"][0]["valid"], &v["files"][1]["valid"]),
        (&json!(1.0), &json!(0.0))
    );
    assert_eq!(
        ecritures(&q),
        ["add", "recheck"],
        "un dépôt non conforme n'est pas mis en partage"
    );

    let r = brut(
        &e.app,
        "PUT",
        &format!("/admin/deposits/{id}/files/1?offset=0&reset=true"),
        b"SRT",
    )
    .await;
    assert_eq!(r.json(), json!({"received": 3}));
    assert_eq!(
        std::fs::read(
            dossier
                .path()
                .join(&id)
                .join("contenu/Film.2024/Subs/fr.srt")
        )
        .unwrap(),
        b"SRT"
    );
    let v = e
        .get("admin", &format!("/admin/deposits/{id}"))
        .await
        .json();
    assert_eq!(
        (&v["files"][0]["valid"], &v["files"][1]["valid"]),
        (&json!(1.0), &Value::Null)
    );
    *etat.lock().unwrap() = Some(("checkingDL", 0.7));
    assert_eq!(partager().await.json()["state"], "checking");
    let attente = attendre();
    *etat.lock().unwrap() = Some(("stoppedUP", 1.0));
    assert_eq!(attente.await.unwrap().json()["state"], "seeding");
    assert_eq!(
        ecritures(&q),
        ["add", "recheck", "recheck", "start", "resume"],
        "le torrent n'est pas ajouté deux fois"
    );
}

#[tokio::test]
async fn depots_refuses() {
    let dossier = tempfile::tempdir().unwrap();
    let q = qbit_depots(Arc::default()).await;
    let e = env_avec(|c| {
        avec_depots(c, dossier.path(), &q);
        c.depots.as_mut().unwrap().plafond = 10;
    })
    .await;
    let r = brut(&e.app, "POST", "/admin/deposits", b"pas un torrent").await;
    assert_eq!(
        (r.statut, &r.json()["error"]),
        (StatusCode::BAD_REQUEST, &json!("invalid"))
    );
    let simple = crate::bencode::tests::torrent_simple();
    assert_eq!(
        brut(&e.app, "POST", "/admin/deposits", &simple)
            .await
            .statut,
        StatusCode::CREATED
    );
    let r = brut(
        &e.app,
        "POST",
        "/admin/deposits",
        &crate::bencode::tests::torrent_multi(),
    )
    .await;
    assert_eq!(
        (r.statut, &r.json()["error"]),
        (StatusCode::CONFLICT, &json!("storage_full"))
    );
    assert_eq!(std::fs::read_dir(dossier.path()).unwrap().count(), 1);
    for chemin in [
        "/admin/deposits/inconnu",
        "/admin/deposits/..%2F..%2Fetc",
        &format!("/admin/deposits/{}", "c".repeat(40)),
    ] {
        assert_eq!(
            e.get("admin", chemin).await.statut,
            StatusCode::NOT_FOUND,
            "{chemin}"
        );
    }
    let trop = vec![b'x'; crate::depots::TORRENT_MAX + 1];
    assert_eq!(
        brut(&e.app, "POST", "/admin/deposits", &trop).await.statut,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let sans = env().await;
    assert_eq!(
        sans.get("admin", "/admin/deposits").await.json()["error"],
        "not_configured"
    );
}

#[tokio::test]
async fn recherche_dans_une_bibliotheque() {
    let f = faux_json(radarr).await;
    let e = env_avec(|c| c.radarr = arr(&f)).await;
    let v = e
        .get("admin", "/admin/library/radarr?q=film%20DU")
        .await
        .json();
    assert_eq!(
        v,
        json!({"items": [{"id": 5, "title": "Le Film du dimanche", "detail": null, "year": 1987}]})
    );
    assert_eq!(
        e.get("admin", "/admin/library/radarr?q=film").await.json()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        e.get("admin", "/admin/library/radarr").await.json(),
        json!({"items": []})
    );
    assert_eq!(
        e.get("admin", "/admin/library/prowlarr?q=x").await.statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get("admin", "/admin/library/sonarr?q=x").await.json()["error"],
        "not_configured"
    );
}
