use std::{sync::OnceLock, time::Duration};

use axum::{
    body::{Body, to_bytes},
    extract::{FromRequestParts, Path, Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{MethodFilter, on},
};
use estompe_core::{
    api::ActiveUser,
    http::{text_error, write_err},
    identity::{COOKIE_NAME, cred_of},
    openapi,
    state::AppState,
};
use futures_util::stream;
use utoipa::openapi::{HttpMethod, path::Operation};
use utoipa_axum::router::OpenApiRouter;

const CORPS_MAX: usize = 6 << 20;

const ROUTES: &[(MethodFilter, &str)] = &[
    (MethodFilter::GET, "/me/permissions"),
    (MethodFilter::GET, "/users"),
    (MethodFilter::GET, "/spaces"),
    (MethodFilter::GET, "/search"),
    (MethodFilter::GET, "/me/tickets"),
    (MethodFilter::GET, "/spaces/{key}/tickets"),
    (MethodFilter::POST, "/spaces/{key}/tickets"),
    (MethodFilter::GET, "/tickets/{key}"),
    (MethodFilter::PATCH, "/tickets/{key}"),
    (MethodFilter::DELETE, "/tickets/{key}"),
    (MethodFilter::GET, "/tickets/{key}/comments"),
    (MethodFilter::POST, "/tickets/{key}/comments"),
    (MethodFilter::PATCH, "/tickets/{key}/comments/{id}"),
    (MethodFilter::DELETE, "/tickets/{key}/comments/{id}"),
    (MethodFilter::GET, "/tickets/{key}/events"),
    (MethodFilter::GET, "/tickets/{key}/pages"),
    (MethodFilter::GET, "/tickets/{key}/dependencies"),
    (MethodFilter::POST, "/tickets/{key}/dependencies"),
    (MethodFilter::DELETE, "/tickets/{key}/dependencies/{other}"),
    (MethodFilter::GET, "/spaces/{key}/pages"),
    (MethodFilter::POST, "/spaces/{key}/pages"),
    (MethodFilter::GET, "/spaces/{key}/trash"),
    (MethodFilter::GET, "/pages/{id}"),
    (MethodFilter::PATCH, "/pages/{id}"),
    (MethodFilter::DELETE, "/pages/{id}"),
    (MethodFilter::POST, "/pages/{id}/move"),
    (MethodFilter::POST, "/pages/{id}/restore"),
    (MethodFilter::GET, "/pages/{id}/versions"),
    (MethodFilter::GET, "/pages/{id}/versions/{n}"),
    (MethodFilter::POST, "/pages/{id}/versions/{n}/restore"),
    (MethodFilter::GET, "/pages/{id}/tickets"),
    (MethodFilter::POST, "/pages/{id}/tickets"),
    (MethodFilter::DELETE, "/pages/{id}/tickets/{key}"),
    (MethodFilter::POST, "/pages/{id}/files"),
    (MethodFilter::GET, "/files/{id}"),
];

const FICHIERS: &str = "/files/{id}";

const MIMES_EN_LIGNE: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "video/mp4",
    "video/webm",
    "audio/mp4",
    "application/ogg",
];

const ENTETES_RELAYES: &[HeaderName] = &[
    header::CONTENT_TYPE,
    header::CACHE_CONTROL,
    header::CONTENT_SECURITY_POLICY,
    header::X_CONTENT_TYPE_OPTIONS,
    header::RETRY_AFTER,
];

fn operation(chemin: &str) -> Operation {
    openapi::operation(
        "cairn",
        "",
        &[
            ("200", "Réponse de cairn-service, renvoyée telle quelle"),
            (
                "403",
                "`forbidden` : permission `estompe:cairn.admin` requise",
            ),
            (
                "503",
                "`permission_unavailable` : service de permission injoignable",
            ),
        ],
        chemin,
    )
}

fn methode(filtre: MethodFilter) -> HttpMethod {
    [
        (MethodFilter::GET, HttpMethod::Get),
        (MethodFilter::POST, HttpMethod::Post),
        (MethodFilter::PATCH, HttpMethod::Patch),
        (MethodFilter::DELETE, HttpMethod::Delete),
    ]
    .into_iter()
    .find(|(f, _)| *f == filtre)
    .map(|(_, m)| m)
    .expect("méthode relayée")
}

pub fn mount(mut routeur: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    for (filtre, modele) in ROUTES {
        let modele: &'static str = modele;
        let chemin = format!("/cairn{modele}");
        routeur.get_openapi_mut().paths.add_path_operation(
            &chemin,
            vec![methode(*filtre)],
            operation(modele),
        );
        routeur = routeur.route(
            &chemin,
            on(
                *filtre,
                move |State(etat): State<AppState>, req: Request| relayer(etat, modele, req),
            ),
        );
    }
    routeur
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("client HTTP")
    })
}

fn segment_valide(v: &str) -> bool {
    (1..=64).contains(&v.len())
        && v.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn chemin_amont(modele: &str, params: &[(String, String)]) -> Option<String> {
    let mut chemin = String::new();
    for morceau in modele.split('/').skip(1) {
        chemin.push('/');
        match morceau.strip_prefix('{').and_then(|m| m.strip_suffix('}')) {
            None => chemin.push_str(morceau),
            Some(nom) => {
                let (_, v) = params.iter().find(|(k, _)| k == nom)?;
                if !segment_valide(v) {
                    return None;
                }
                chemin.push_str(v);
            }
        }
    }
    Some(chemin)
}

async fn relayer(etat: AppState, modele: &'static str, req: Request) -> Response {
    if etat.config().cairn_url.is_empty() {
        return text_error(StatusCode::NOT_FOUND, "404 page not found");
    }
    let (mut parts, corps) = req.into_parts();
    let ActiveUser(utilisateur) = match ActiveUser::from_request_parts(&mut parts, &etat).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if !matches!(
        etat.cairn_permission().allowed(&utilisateur.sub).await,
        Ok(true)
    ) {
        return write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "accès à Cairn non accordé",
        );
    }
    let Some(cred) = cred_of(&parts.headers) else {
        return text_error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let params = match Path::<Vec<(String, String)>>::from_request_parts(&mut parts, &etat).await {
        Ok(Path(p)) => p,
        Err(_) => Vec::new(),
    };
    let Some(chemin) = chemin_amont(modele, &params) else {
        return write_err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "identifiant invalide",
        );
    };
    let trop_gros = || {
        write_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "requête trop volumineuse",
        )
    };
    let annonce = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if annonce.is_some_and(|n| n > CORPS_MAX) {
        return trop_gros();
    }
    let octets = match to_bytes(corps, CORPS_MAX).await {
        Ok(o) => o,
        Err(_) => return trop_gros(),
    };
    let mut cible = format!("{}{chemin}", etat.config().cairn_url.trim_end_matches('/'));
    if let Some(q) = parts.uri.query().filter(|q| !q.is_empty()) {
        cible.push('?');
        cible.push_str(q);
    }
    let mut amont = client().request(parts.method.clone(), cible);
    if !octets.is_empty() {
        if let Some(ct) = parts.headers.get(header::CONTENT_TYPE) {
            amont = amont.header(header::CONTENT_TYPE, ct);
        }
        amont = amont.body(octets);
    }
    let secret = if cred.cookie {
        (header::COOKIE, format!("{COOKIE_NAME}={}", cred.value))
    } else {
        (header::AUTHORIZATION, cred.value)
    };
    let Ok(secret_valeur) = HeaderValue::from_str(&secret.1) else {
        return estompe_core::http::internal();
    };
    amont = amont.header(secret.0, secret_valeur);
    if parts.method != Method::GET && parts.method != Method::HEAD {
        let Ok(origine) = HeaderValue::from_str(&etat.config().cairn_origin) else {
            return estompe_core::http::internal();
        };
        amont = amont.header(header::ORIGIN, origine);
    }
    let debut = std::time::Instant::now();
    let reponse = match amont.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "cairn : {} {chemin} → injoignable après {:?} : {e}",
                parts.method,
                debut.elapsed()
            );
            return write_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "cairn_unavailable",
                "Cairn est injoignable",
            );
        }
    };
    let statut = reponse.status();
    if statut.as_u16() >= 400 {
        tracing::warn!(
            "cairn : {} {chemin} → {} en {:?}",
            parts.method,
            statut.as_u16(),
            debut.elapsed()
        );
    }
    let mut sortie = Response::new(Body::empty());
    *sortie.status_mut() = statut;
    for nom in ENTETES_RELAYES {
        if let Some(v) = reponse.headers().get(nom) {
            sortie.headers_mut().insert(nom.clone(), v.clone());
        }
    }
    if modele == FICHIERS {
        durcir_fichier(&mut sortie);
    }
    *sortie.body_mut() = Body::from_stream(stream::unfold(reponse, |mut r| async move {
        match r.chunk().await {
            Ok(Some(morceau)) => Some((Ok::<_, reqwest::Error>(morceau), r)),
            Ok(None) => None,
            Err(e) => Some((Err(e), r)),
        }
    }));
    sortie.into_response()
}

fn durcir_fichier(sortie: &mut Response) {
    if sortie.status().as_u16() < 300 {
        let h = sortie.headers_mut();
        let ct = h
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let ct = ct
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let disposition = if MIMES_EN_LIGNE.contains(&ct.as_str()) {
            "inline"
        } else {
            h.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            "attachment"
        };
        h.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static(disposition),
        );
    }
    let h = sortie.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        http::{HeaderMap, Request},
    };
    use estompe_core::{
        db, middleware, testutil,
        testutil::{Faux, Requete, faux, faux_avec_entetes},
        users,
    };
    use serde_json::json;
    use std::any::Any;
    use tower::ServiceExt;

    const ORIGINE: &str = "https://app.test";

    struct Env {
        app: Router,
        _services: (Faux, Faux),
        _pg: Box<dyn Any + Send>,
    }

    fn gateway(r: &Requete) -> (u16, Vec<u8>) {
        let cookie = r.entete("cookie");
        let nom = r
            .entete("authorization")
            .strip_prefix("Bearer ")
            .map(String::from)
            .or_else(|| {
                cookie
                    .strip_prefix(&format!("{COOKIE_NAME}="))
                    .map(String::from)
            })
            .unwrap_or_default();
        if nom.is_empty() || nom == "bad" {
            return (401, vec![]);
        }
        (
            200,
            json!({ "id": format!("sub-{nom}"), "name": nom })
                .to_string()
                .into_bytes(),
        )
    }

    fn permission(r: &Requete) -> (u16, Vec<u8>) {
        let corps: serde_json::Value = serde_json::from_slice(&r.corps).unwrap();
        let ok = corps["permission"] != "estompe:cairn.admin" || corps["user_id"] != "sub-refuse";
        (200, json!({ "allowed": ok }).to_string().into_bytes())
    }

    async fn env(cairn_url: &str) -> Option<Env> {
        let (g, p) = (faux(gateway).await, faux(permission).await);
        let config = testutil::config_avec(&[
            ("GATEWAY_URL", &g.url),
            ("PERMISSION_URL", &p.url),
            ("CAIRN_URL", cairn_url),
            ("CAIRN_ORIGIN", ORIGINE),
            ("ALLOWED_ORIGINS", ORIGINE),
        ]);
        let etat = AppState::new(config, vec!["cairn".into()]);
        let (conteneur, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        db::migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "refuse"] {
            users::activate(&pool, &format!("sub-{nom}"), nom)
                .await
                .unwrap();
        }
        etat.set_pool(pool);
        Some(Env {
            app: middleware::envelopper(Router::from(mount(OpenApiRouter::new())), etat),
            _services: (g, p),
            _pg: Box::new(conteneur),
        })
    }

    struct Rep {
        statut: StatusCode,
        entetes: HeaderMap,
        corps: String,
    }

    async fn appel(
        app: &Router,
        methode: &str,
        chemin: &str,
        qui: Option<&str>,
        corps: &str,
    ) -> Rep {
        let mut r = Request::builder()
            .method(methode)
            .uri(chemin)
            .header(header::ORIGIN, ORIGINE);
        if let Some(q) = qui {
            r = r.header(header::COOKIE, format!("{COOKIE_NAME}={q}"));
        }
        if !corps.is_empty() {
            r = r.header(header::CONTENT_TYPE, "application/json");
        }
        let rep = app
            .clone()
            .oneshot(r.body(Body::from(corps.to_string())).unwrap())
            .await
            .unwrap();
        let (statut, entetes) = (rep.status(), rep.headers().clone());
        let corps = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        Rep {
            statut,
            entetes,
            corps: String::from_utf8_lossy(&corps).into_owned(),
        }
    }

    #[test]
    fn chemin_reconstruit_depuis_les_segments_valides() {
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        assert_eq!(
            chemin_amont(
                "/tickets/{key}/comments/{id}",
                &[p("key", "EST-1"), p("id", "c_2")]
            ),
            Some("/tickets/EST-1/comments/c_2".into())
        );
        assert_eq!(chemin_amont("/spaces", &[]), Some("/spaces".into()));
        for mauvais in ["", "../tokens", "a/b", "a b", "é", &"x".repeat(65)] {
            assert_eq!(
                chemin_amont("/tickets/{key}", &[p("key", mauvais)]),
                None,
                "{mauvais}"
            );
        }
        assert!(segment_valide(&"x".repeat(64)));
    }

    #[tokio::test]
    async fn relaie_lectures_et_ecritures() {
        let amont = faux_avec_entetes(|_| {
            (
                200,
                vec![("content-type", "application/json".into())],
                br#"{"actions":["space.read"]}"#.to_vec(),
            )
        })
        .await;
        let Some(e) = env(&amont.url).await else {
            return;
        };
        let r = appel(&e.app, "GET", "/cairn/me/permissions", Some("alice"), "").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.corps, r#"{"actions":["space.read"]}"#);
        assert_eq!(r.entetes[header::CONTENT_TYPE], "application/json");
        let lu = &amont.requetes()[0];
        assert_eq!(
            (lu.methode.as_str(), lu.chemin.as_str()),
            ("GET", "/me/permissions")
        );
        assert_eq!(lu.entete("cookie"), format!("{COOKIE_NAME}=alice"));
        assert_eq!(lu.entete("origin"), "");

        let corps = r#"{"version":2,"title":"t"}"#;
        let r = appel(
            &e.app,
            "PATCH",
            "/cairn/tickets/EST-1?x=1",
            Some("alice"),
            corps,
        )
        .await;
        assert_eq!(r.statut, StatusCode::OK);
        let ecrit = &amont.requetes()[1];
        assert_eq!(ecrit.methode, "PATCH");
        assert_eq!(ecrit.entete("origin"), ORIGINE);
        assert_eq!(ecrit.entete("content-type"), "application/json");
        assert_eq!(String::from_utf8_lossy(&ecrit.corps), corps);

        let mut req = Request::get("/cairn/spaces")
            .header(header::AUTHORIZATION, "Bearer alice")
            .body(Body::empty())
            .unwrap();
        req.headers_mut().remove(header::COOKIE);
        assert_eq!(
            e.app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::OK
        );
        let bearer = &amont.requetes()[2];
        assert_eq!(bearer.entete("authorization"), "Bearer alice");
        assert_eq!(bearer.entete("cookie"), "");
    }

    #[tokio::test]
    async fn requete_relayee_avec_sa_query() {
        let amont = faux(|_| (200, b"{}".to_vec())).await;
        let Some(e) = env(&amont.url).await else {
            return;
        };
        appel(
            &e.app,
            "GET",
            "/cairn/search?q=a%20b&x=1",
            Some("alice"),
            "",
        )
        .await;
        let r = amont.requetes();
        assert_eq!(r[0].chemin, "/search");
        assert_eq!(r[0].query, "q=a%20b&x=1");
    }

    #[tokio::test]
    async fn fichiers_en_tetes_imposes() {
        let ct = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let ct2 = ct.clone();
        let amont = faux_avec_entetes(move |_| {
            (
                200,
                vec![
                    ("content-type", ct2.lock().unwrap().clone()),
                    ("content-security-policy", "default-src *".into()),
                ],
                b"x".to_vec(),
            )
        })
        .await;
        let Some(e) = env(&amont.url).await else {
            return;
        };
        for (recu, attendu, disposition) in [
            (
                "text/html; charset=utf-8",
                "application/octet-stream",
                "attachment",
            ),
            ("image/svg+xml", "application/octet-stream", "attachment"),
            ("image/png", "image/png", "inline"),
            ("IMAGE/PNG; x=y", "IMAGE/PNG; x=y", "inline"),
        ] {
            *ct.lock().unwrap() = recu.into();
            let r = appel(&e.app, "GET", "/cairn/files/f1", Some("alice"), "").await;
            let h = &r.entetes;
            assert_eq!(r.statut, StatusCode::OK, "{recu}");
            assert_eq!(h[header::CONTENT_TYPE], attendu, "{recu}");
            assert_eq!(h[header::CONTENT_DISPOSITION], disposition, "{recu}");
            assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
            assert_eq!(
                h[header::CONTENT_SECURITY_POLICY],
                "default-src 'none'; sandbox"
            );
        }
    }

    #[tokio::test]
    async fn erreurs_relayees_telles_quelles() {
        let statut = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(0));
        let s2 = statut.clone();
        let amont = faux(move |_| {
            (
                s2.load(std::sync::atomic::Ordering::SeqCst),
                r#"{"error":"modifié entre-temps (version 3)"}"#.as_bytes().to_vec(),
            )
        })
        .await;
        let Some(e) = env(&amont.url).await else {
            return;
        };
        for code in [403, 404, 409, 503] {
            statut.store(code, std::sync::atomic::Ordering::SeqCst);
            let r = appel(
                &e.app,
                "PATCH",
                "/cairn/pages/p1",
                Some("alice"),
                r#"{"version":1}"#,
            )
            .await;
            assert_eq!(r.statut.as_u16(), code);
            assert_eq!(r.corps, r#"{"error":"modifié entre-temps (version 3)"}"#);
        }
    }

    #[tokio::test]
    async fn cairn_injoignable() {
        let amont = faux(|_| (200, b"{}".to_vec())).await;
        let url = amont.url.clone();
        drop(amont);
        let Some(e) = env(&url).await else {
            return;
        };
        let r = appel(&e.app, "GET", "/cairn/spaces", Some("alice"), "").await;
        assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
        assert!(r.corps.contains("cairn_unavailable"), "{}", r.corps);
        assert_eq!(r.entetes[header::CONTENT_TYPE], "application/json");
    }

    #[tokio::test]
    async fn garde_fous() {
        let amont = faux(|_| (200, b"{}".to_vec())).await;
        let Some(e) = env(&amont.url).await else {
            return;
        };
        for (m, p) in [
            ("GET", "/cairn/tokens"),
            ("POST", "/cairn/tokens"),
            ("POST", "/cairn/mcp"),
            ("POST", "/cairn/spaces"),
            ("PATCH", "/cairn/spaces/EST"),
            ("DELETE", "/cairn/spaces/EST/tickets"),
        ] {
            let r = appel(&e.app, m, p, Some("alice"), "{}").await;
            assert!(
                r.statut == StatusCode::NOT_FOUND || r.statut == StatusCode::METHOD_NOT_ALLOWED,
                "{m} {p} : {}",
                r.statut
            );
        }
        let r = appel(
            &e.app,
            "GET",
            "/cairn/tickets/%2e%2e%2ftokens",
            Some("alice"),
            "",
        )
        .await;
        assert_eq!(r.statut, StatusCode::TEMPORARY_REDIRECT);
        assert!(r.corps.contains("/cairn/tokens"));
        let r = appel(&e.app, "GET", "/cairn/spaces", Some("refuse"), "").await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN);
        assert!(r.corps.contains("accès à Cairn non accordé"));
        let r = appel(&e.app, "GET", "/cairn/spaces", Some("inconnu"), "").await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN);
        assert!(r.corps.contains("not_activated"));
        let r = appel(&e.app, "GET", "/cairn/spaces", None, "").await;
        assert_eq!(r.statut, StatusCode::UNAUTHORIZED);
        let req = Request::post("/cairn/spaces/EST/tickets")
            .header(header::ORIGIN, "https://evil.example")
            .header(header::COOKIE, format!("{COOKIE_NAME}=alice"))
            .body(Body::from(r#"{"title":"x"}"#))
            .unwrap();
        let r = e.app.clone().oneshot(req).await.unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        assert_eq!(amont.appels(), 0);
    }

    #[tokio::test]
    async fn corps_trop_volumineux() {
        let amont = faux(|_| (200, b"{}".to_vec())).await;
        let Some(e) = env(&amont.url).await else {
            return;
        };
        let gros = "a".repeat(CORPS_MAX + 1);
        let r = appel(
            &e.app,
            "POST",
            "/cairn/pages/p1/files",
            Some("alice"),
            &gros,
        )
        .await;
        assert_eq!(r.statut, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(r.corps.contains("too_large"));
        assert_eq!(amont.appels(), 0);
    }

    #[tokio::test]
    async fn cairn_desactive() {
        let Some(e) = env("/").await else {
            return;
        };
        let r = appel(&e.app, "GET", "/cairn/spaces", Some("alice"), "").await;
        assert_eq!(r.statut, StatusCode::NOT_FOUND);
        assert_eq!(r.corps, "404 page not found\n");
    }
}
