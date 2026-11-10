mod common;

use axum::http::HeaderMap;
use common::{faux, url_injoignable};
use pli::auth::{PermissionChecker, client_http, fetch_identity, same_origin};
use serde_json::{Value, json};

fn corps(v: Value) -> Vec<u8> {
    serde_json::to_vec(&v).unwrap()
}

#[tokio::test]
async fn identite_via_auth_me() {
    let http = client_http();
    let ok = faux(|_| {
        (
            200,
            vec![],
            corps(json!({ "id": "u1", "name": "a", "roles": [] })),
        )
    })
    .await;
    let user = fetch_identity(&http, Some("sid=abc"), &format!("{}/", ok.url))
        .await
        .unwrap();
    assert_eq!(user["id"], "u1");
    let vu = ok.requetes();
    assert_eq!(vu[0].chemin, "/auth/me");
    assert_eq!(vu[0].entete("cookie"), "sid=abc");
    assert_eq!(vu[0].entete("user-agent"), "pli.plbls.fr");

    assert!(
        fetch_identity(&http, Some(""), &ok.url)
            .await
            .unwrap_err()
            .contains("cookie")
    );
    assert!(
        fetch_identity(&http, None, &ok.url)
            .await
            .unwrap_err()
            .contains("cookie")
    );

    let refus = faux(|_| {
        (
            403,
            vec![("server", "nginx".into())],
            b"  interdit \n ici ".to_vec(),
        )
    })
    .await;
    let raison = fetch_identity(&http, Some("sid=x"), &refus.url)
        .await
        .unwrap_err();
    assert_eq!(
        raison,
        "le gateway a répondu HTTP 403 à /auth/me (serveur : nginx, corps :  interdit ici )"
    );
    let vide = faux(|_| (401, vec![], vec![])).await;
    let raison = fetch_identity(&http, Some("sid=x"), &vide.url)
        .await
        .unwrap_err();
    assert!(
        raison.contains("HTTP 401")
            && raison.contains("serveur : ?")
            && raison.contains("corps : vide")
    );

    let raison = fetch_identity(&http, Some("sid=x"), &url_injoignable().await)
        .await
        .unwrap_err();
    assert!(raison.contains("injoignable"), "{raison}");

    let sans_id = faux(|_| (200, vec![], corps(json!({ "name": "a" })))).await;
    assert_eq!(
        fetch_identity(&http, Some("sid=x"), &sans_id.url)
            .await
            .unwrap_err(),
        "réponse /auth/me sans id"
    );
}

fn entetes(paires: &[(&'static str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in paires {
        h.insert(*k, v.parse().unwrap());
    }
    h
}

#[test]
fn meme_origine() {
    assert!(same_origin(&entetes(&[
        ("origin", "https://pli.plbls.fr"),
        ("host", "pli.plbls.fr")
    ])));
    assert!(!same_origin(&entetes(&[
        ("origin", "https://evil.plbls.fr"),
        ("host", "pli.plbls.fr")
    ])));
    assert!(!same_origin(&entetes(&[("host", "pli.plbls.fr")])));
    assert!(same_origin(&entetes(&[
        ("origin", "https://pli.plbls.fr"),
        ("host", "pli:8080"),
        ("x-forwarded-host", "pli.plbls.fr"),
    ])));
    assert!(same_origin(&entetes(&[
        ("origin", "http://127.0.0.1:8080"),
        ("host", "127.0.0.1:8080")
    ])));
    assert!(same_origin(&entetes(&[
        ("origin", "https://pli.plbls.fr:443"),
        ("host", "pli.plbls.fr")
    ])));
    assert!(!same_origin(&entetes(&[
        ("origin", "null"),
        ("host", "pli.plbls.fr")
    ])));
}

#[tokio::test]
async fn permission_autorise_refuse_cache_erreur_vaut_refus() {
    let http = client_http();
    let p = faux(|r| {
        let demande: Value = serde_json::from_slice(&r.corps).unwrap();
        (
            200,
            vec![],
            corps(json!({ "allowed": demande["user_id"] == "u1" })),
        )
    })
    .await;
    let check = PermissionChecker::new(http.clone(), &p.url, "t", "pli:deposit");
    assert!(check.check(&json!("u1"), 0).await.allowed);
    assert!(check.check(&json!("u1"), 1000).await.allowed);
    assert_eq!(p.requetes().len(), 1);
    let r = &p.requetes()[0];
    assert_eq!(
        (r.methode.as_str(), r.chemin.as_str()),
        ("POST", "/v1/check")
    );
    assert_eq!(r.entete("authorization"), "Bearer t");
    assert_eq!(
        serde_json::from_slice::<Value>(&r.corps).unwrap(),
        json!({ "user_id": "u1", "permission": "pli:deposit" })
    );
    assert!(!check.check(&json!("u2"), 0).await.allowed);
    check.check(&json!("u1"), 30_001).await;
    assert_eq!(p.requetes().len(), 3);

    let down = PermissionChecker::new(http.clone(), &url_injoignable().await, "t", "pli:deposit");
    let r = down.check(&json!("u1"), 0).await;
    assert!(!r.allowed);
    assert!(r.reason.unwrap().contains("injoignable"));

    let unauth = faux(|_| (401, vec![], vec![])).await;
    let r = PermissionChecker::new(http, &unauth.url, "t", "pli:deposit")
        .check(&json!("u1"), 0)
        .await;
    let raison = r.reason.unwrap();
    assert!(
        raison.contains("401") && raison.contains("PERMISSION_SERVICE_TOKEN"),
        "{raison}"
    );
}

#[tokio::test]
async fn controle_et_manifeste_via_aegis() {
    let http = client_http();
    let a = faux(|r| match r.chemin.as_str() {
        "/v1/check" => {
            let c: Value = serde_json::from_slice(&r.corps).unwrap();
            (200, vec![], corps(json!({ "allowed": c["sub"] == "u1" })))
        }
        _ => (200, vec![], corps(json!({ "changed": true }))),
    })
    .await;
    let check = PermissionChecker::aegis(http.clone(), &a.url, "jeton-pli", "pli:deposit");
    assert_eq!(check.source(), "aegis");
    assert!(check.check(&json!("u1"), 0).await.allowed);
    assert!(!check.check(&json!("u2"), 0).await.allowed);
    check.envoyer_manifeste().await.unwrap();
    let vu = a.requetes();
    assert_eq!(
        serde_json::from_slice::<Value>(&vu[0].corps).unwrap(),
        json!({ "sub": "u1", "permission": "pli:deposit" })
    );
    let m = vu.iter().find(|r| r.chemin == "/v1/manifest").unwrap();
    assert_eq!(
        (m.methode.as_str(), m.entete("authorization").as_str()),
        ("PUT", "Bearer jeton-pli")
    );
    let m: Value = serde_json::from_slice(&m.corps).unwrap();
    assert_eq!(
        (m["app"].as_str(), m["permissions"][0]["key"].as_str()),
        (Some("pli"), Some("deposit"))
    );

    let refuse = faux(|_| (401, vec![], vec![])).await;
    let raison = PermissionChecker::aegis(http, &refuse.url, "t", "pli:deposit")
        .check(&json!("u1"), 0)
        .await
        .reason
        .unwrap();
    assert!(
        raison.contains("aegis") && raison.contains("PLI_AEGIS_TOKEN"),
        "{raison}"
    );
}
