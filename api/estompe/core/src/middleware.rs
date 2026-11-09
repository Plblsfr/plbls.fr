use std::{panic::AssertUnwindSafe, time::Instant};

use axum::{
    Router,
    body::HttpBody,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{Next, from_fn_with_state, map_response},
    response::{IntoResponse, Response},
};
use futures_util::FutureExt;
use percent_encoding::percent_decode_str;
use serde_json::json;

use crate::{
    http::{internal, text_error, write_err, write_json},
    state::AppState,
};

fn est_jeton(nom: &str) -> bool {
    !nom.is_empty()
        && nom
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn valeur_de_cookie_valide(valeur: &str) -> bool {
    let nue = valeur
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(valeur);
    nue.bytes()
        .all(|b| (0x20..0x7f).contains(&b) && !matches!(b, b'"' | b';' | b'\\'))
}

fn compte_cookies(entetes: &HeaderMap) -> usize {
    entetes
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|ligne| ligne.split(';'))
        .filter_map(|part| part.trim_matches([' ', '\t']).split_once('='))
        .filter(|(nom, valeur)| {
            est_jeton(nom.trim_matches([' ', '\t'])) && valeur_de_cookie_valide(valeur)
        })
        .count()
}

fn avec_cors(mut reponse: Response, origine: &HeaderValue) -> Response {
    let h = reponse.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origine.clone());
    h.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    h.append(header::VARY, HeaderValue::from_static("Origin"));
    reponse
}

pub async fn politique_origine(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let origine = req.headers().get(header::ORIGIN).cloned();
    let texte = origine
        .as_ref()
        .and_then(|o| o.to_str().ok())
        .unwrap_or_default();
    let autorisee = state
        .config()
        .allowed_origins
        .iter()
        .any(|o| o.as_str() == texte);
    let cors = if texte.is_empty() {
        None
    } else {
        origine.filter(|_| autorisee)
    };
    if let Some(o) = &cors
        && req.method() == Method::OPTIONS
    {
        let mut r = avec_cors(StatusCode::NO_CONTENT.into_response(), o);
        let h = r.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, PATCH, PUT, DELETE"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, X-Estompe-Version, X-Estompe-Platform",
            ),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        return r;
    }
    let modifiante = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let reponse = if modifiante && compte_cookies(req.headers()) > 0 && !autorisee {
        write_err(StatusCode::FORBIDDEN, "bad_origin", "origine non autorisée")
    } else {
        next.run(req).await
    };
    match &cors {
        Some(o) => avec_cors(reponse, o),
        None => reponse,
    }
}

fn parametre(requete: Option<&str>, nom: &str) -> String {
    requete
        .unwrap_or_default()
        .split('&')
        .filter_map(|p| p.split_once('=').or(Some((p, ""))))
        .find(|(k, _)| decoder(k) == nom)
        .map(|(_, v)| decoder(v))
        .unwrap_or_default()
}

fn decoder(brut: &str) -> String {
    percent_decode_str(&brut.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

fn entete(entetes: &HeaderMap, nom: &str) -> String {
    entetes
        .get(nom)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

pub async fn version_minimale(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let (mut v, mut p) = (
        entete(req.headers(), "x-estompe-version"),
        entete(req.headers(), "x-estompe-platform"),
    );
    if v.is_empty() {
        let q = req.uri().query();
        (v, p) = (parametre(q, "v"), parametre(q, "platform"));
    }
    let regle = state.politique().pour(&p);
    if !regle.min_version.is_empty() && crate::updates::en_dessous(&v, &regle.min_version) {
        return write_json(
            StatusCode::UPGRADE_REQUIRED,
            &json!({
                "error": "upgrade_required",
                "message": regle.message,
                "min_version": regle.min_version,
            }),
        );
    }
    next.run(req).await
}

fn chemin_suivi(req: &Request) -> Option<String> {
    let chemin = req.uri().path();
    (!chemin.starts_with("/ws"))
        .then(|| percent_decode_str(chemin).decode_utf8_lossy().into_owned())
}

pub async fn journal_erreurs(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(chemin) = chemin_suivi(&req) else {
        return next.run(req).await;
    };
    let methode = req.method().to_string();
    let reponse = match AssertUnwindSafe(next.run(req)).catch_unwind().await {
        Ok(r) => r,
        Err(_) => {
            tracing::error!(%methode, ?chemin, "panique du gestionnaire");
            internal()
        }
    };
    let statut = reponse.status();
    if statut.is_server_error() {
        let ecrit = match state.pool() {
            Ok(pool) => crate::db::journaliser_erreur(pool, &methode, &chemin, statut.as_u16())
                .await
                .map_err(|e| e.to_string()),
            Err(_) => Err("base indisponible".into()),
        };
        if let Err(e) = ecrit {
            tracing::warn!("journal des erreurs : {e}");
        }
    }
    reponse
}

pub async fn metriques(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if chemin_suivi(&req).is_none() {
        return next.run(req).await;
    }
    let _garde = state.metrics().commencer();
    let debut = Instant::now();
    let reponse = next.run(req).await;
    state
        .metrics()
        .observer(reponse.status().as_u16(), debut.elapsed());
    reponse
}

pub async fn normaliser_404_405(reponse: Response) -> Response {
    let vide = reponse.body().size_hint().exact() == Some(0)
        && !reponse.headers().contains_key(header::CONTENT_TYPE);
    match reponse.status() {
        StatusCode::NOT_FOUND if vide => text_error(StatusCode::NOT_FOUND, "404 page not found"),
        StatusCode::METHOD_NOT_ALLOWED if vide => {
            text_error(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        }
        _ => reponse,
    }
}

async fn allow_comme_le_go(mut reponse: Response) -> Response {
    if reponse.status() != StatusCode::METHOD_NOT_ALLOWED {
        reponse.headers_mut().remove(header::ALLOW);
        return reponse;
    }
    let mut methodes: Vec<String> = reponse
        .headers()
        .get_all(header::ALLOW)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(',').map(|m| m.trim().to_string()))
        .collect();
    if methodes.is_empty() {
        return reponse;
    }
    methodes.sort_unstable();
    methodes.dedup();
    if let Ok(v) = HeaderValue::from_str(&methodes.join(", ")) {
        reponse.headers_mut().insert(header::ALLOW, v);
    }
    reponse
}

fn echappement_valide(chemin: &str) -> bool {
    let o = chemin.as_bytes();
    o.iter().enumerate().all(|(i, b)| {
        *b != b'%'
            || (o.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && o.get(i + 2).is_some_and(u8::is_ascii_hexdigit))
    })
}

pub async fn chemin_mal_echappe(req: Request, next: Next) -> Response {
    if echappement_valide(req.uri().path()) {
        return next.run(req).await;
    }
    (
        StatusCode::BAD_REQUEST,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        "400 Bad Request",
    )
        .into_response()
}

fn nettoyer(chemin: &str) -> String {
    let mut pile: Vec<&str> = vec![];
    for morceau in chemin.split('/') {
        match morceau {
            "" | "." => {}
            ".." => {
                pile.pop();
            }
            m => pile.push(m),
        }
    }
    let mut propre = format!("/{}", pile.join("/"));
    if chemin.ends_with('/') && propre != "/" {
        propre.push('/');
    }
    propre
}

fn echapper_chemin(chemin: &str) -> String {
    let mut sortie = String::new();
    for b in chemin.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~$&+,/:;=@".contains(&b) {
            sortie.push(b as char);
        } else {
            sortie.push_str(&format!("%{b:02X}"));
        }
    }
    sortie
}

pub async fn redirection_chemin(req: Request, next: Next) -> Response {
    let decode = percent_decode_str(req.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let propre = nettoyer(&decode);
    if propre == decode || req.method() == Method::CONNECT {
        return next.run(req).await;
    }
    let mut cible = echapper_chemin(&propre);
    if let Some(q) = req.uri().query() {
        cible.push('?');
        cible.push_str(q);
    }
    let mut reponse = Response::new(axum::body::Body::empty());
    *reponse.status_mut() = StatusCode::TEMPORARY_REDIRECT;
    if let Ok(v) = HeaderValue::from_str(&cible) {
        reponse.headers_mut().insert(header::LOCATION, v);
    }
    if req.method() == Method::GET || req.method() == Method::HEAD {
        reponse.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
    }
    if req.method() == Method::GET {
        let html = cible
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&#34;")
            .replace('\'', "&#39;");
        *reponse.body_mut() =
            axum::body::Body::from(format!("<a href=\"{html}\">Temporary Redirect</a>.\n\n"));
    }
    reponse
}

pub fn envelopper(routes: Router<AppState>, state: AppState) -> Router {
    let interieur = routes
        .layer(from_fn_with_state(state.clone(), crate::garde::garde))
        .layer(map_response(normaliser_404_405))
        .layer(axum::middleware::from_fn(redirection_chemin))
        .layer(from_fn_with_state(state.clone(), version_minimale))
        .layer(from_fn_with_state(state.clone(), politique_origine))
        .layer(from_fn_with_state(state.clone(), journal_erreurs))
        .layer(from_fn_with_state(state.clone(), metriques))
        .layer(axum::middleware::from_fn(crate::journal::entrant))
        .with_state(state);
    Router::new()
        .fallback_service(interieur)
        .layer(axum::middleware::from_fn(chemin_mal_echappe))
        .layer(map_response(allow_comme_le_go))
}

#[cfg(test)]
mod chemins {
    use super::{echappement_valide, echapper_chemin, nettoyer};

    #[test]
    fn nettoyage_et_echappement_comme_le_mux_go() {
        for (entree, attendu) in [
            ("//me", "/me"),
            ("/me//", "/me/"),
            ("//", "/"),
            ("/a/./b/../c", "/a/c"),
            ("/a/b/", "/a/b/"),
            ("/..", "/"),
            ("/me", "/me"),
        ] {
            assert_eq!(nettoyer(entree), attendu, "{entree}");
        }
        assert!(echappement_valide("/a%20b"));
        assert!(!echappement_valide("/a%zz"));
        assert!(!echappement_valide("/a%2"));
        assert_eq!(echapper_chemin("/é a?"), "/%C3%A9%20a%3F");
    }
}

#[cfg(test)]
mod tests {
    use super::envelopper;
    use crate::{
        api,
        state::AppState,
        testutil::{self, BasePostgres, config_avec},
        updates,
    };
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header, request::Builder},
        routing::{any, get},
    };
    use tower::ServiceExt;

    const ORIGINE: &str = "https://app.test";

    struct Env {
        app: Router,
        state: AppState,
        _pg: Option<BasePostgres>,
    }

    async fn paniquer() -> &'static str {
        panic!("boom")
    }

    async fn env(avec_base: bool) -> Option<Env> {
        let config = config_avec(&[
            ("ALLOWED_ORIGINS", ORIGINE),
            ("METRICS_TOKEN", "sekret"),
            ("MAX_STORAGE_GB", "3"),
        ]);
        let politique = updates::parse(
            br#"{"minVersion":"0.5.0","platforms":{"android":{"minVersion":"1.0.0","message":"faille corrigee"}}}"#,
        );
        let state = AppState::new_avec_politique(config, vec![], politique);
        let pg = if avec_base {
            let (c, url) = testutil::postgres().await?;
            let pool = sqlx::PgPool::connect(&url).await.unwrap();
            crate::db::migrate_embarquees(&pool).await.unwrap();
            state.set_pool(pool);
            Some(c)
        } else {
            None
        };
        let routes = api::routes()
            .route("/ok", any(|| async { "ok" }))
            .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/panic", get(paniquer))
            .route("/ws/x", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }));
        Some(Env {
            app: envelopper(routes, state.clone()),
            state,
            _pg: pg,
        })
    }

    fn req(methode: &str, chemin: &str) -> Builder {
        Request::builder().method(methode).uri(chemin)
    }

    async fn envoyer(app: &Router, r: Builder) -> (StatusCode, HeaderMap, String) {
        let rep = app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (statut, entetes) = (rep.status(), rep.headers().clone());
        let corps = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (statut, entetes, String::from_utf8(corps.to_vec()).unwrap())
    }

    fn cors_absent(h: &HeaderMap) {
        for n in [
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            header::ACCESS_CONTROL_ALLOW_METHODS,
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            header::ACCESS_CONTROL_MAX_AGE,
            header::VARY,
        ] {
            assert!(!h.contains_key(&n), "{n}");
        }
    }

    #[tokio::test]
    async fn cors_origine_autorisee() {
        let e = env(false).await.unwrap();
        let (s, h, _) = envoyer(&e.app, req("GET", "/healthz").header("Origin", ORIGINE)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGINE);
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_CREDENTIALS], "true");
        assert_eq!(h[header::VARY], "Origin");
        assert!(!h.contains_key(header::ACCESS_CONTROL_MAX_AGE));
    }

    #[tokio::test]
    async fn cors_preflight() {
        let e = env(false).await.unwrap();
        let (s, h, c) = envoyer(
            &e.app,
            req("OPTIONS", "/activate").header("Origin", ORIGINE),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert_eq!(c, "");
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGINE);
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_CREDENTIALS], "true");
        assert_eq!(h[header::VARY], "Origin");
        assert_eq!(
            h[header::ACCESS_CONTROL_ALLOW_METHODS],
            "GET, POST, PATCH, PUT, DELETE"
        );
        assert_eq!(
            h[header::ACCESS_CONTROL_ALLOW_HEADERS],
            "Authorization, Content-Type, X-Estompe-Version, X-Estompe-Platform"
        );
        assert_eq!(h[header::ACCESS_CONTROL_MAX_AGE], "600");
        assert!(!h.contains_key(header::ALLOW));
    }

    #[tokio::test]
    async fn cors_origine_etrangere_sans_en_tete() {
        let e = env(false).await.unwrap();
        let (_, h, _) = envoyer(
            &e.app,
            req("OPTIONS", "/activate").header("Origin", "https://evil.example"),
        )
        .await;
        cors_absent(&h);
        let (_, h, _) = envoyer(
            &e.app,
            req("GET", "/healthz").header("Origin", "https://evil.example"),
        )
        .await;
        cors_absent(&h);
        let (_, h, _) = envoyer(&e.app, req("GET", "/healthz")).await;
        cors_absent(&h);
    }

    #[tokio::test]
    async fn csrf_refuse_les_requetes_modifiantes_avec_cookie() {
        let e = env(false).await.unwrap();
        for methode in ["POST", "PATCH", "PUT", "DELETE"] {
            for origine in [None, Some("https://evil.example")] {
                let mut r = req(methode, "/ok").header(header::COOKIE, "sid=abc");
                if let Some(o) = origine {
                    r = r.header("Origin", o);
                }
                let (s, h, c) = envoyer(&e.app, r).await;
                assert_eq!(s, StatusCode::FORBIDDEN, "{methode} {origine:?}");
                assert_eq!(h[header::CONTENT_TYPE], "application/json");
                assert_eq!(
                    c,
                    "{\"error\":\"bad_origin\",\"message\":\"origine non autorisée\"}\n"
                );
                cors_absent(&h);
            }
        }
    }

    #[tokio::test]
    async fn csrf_laisse_passer_les_cas_legitimes() {
        let e = env(false).await.unwrap();
        let (s, h, _) = envoyer(
            &e.app,
            req("POST", "/ok")
                .header(header::COOKIE, "sid=abc")
                .header("Origin", ORIGINE),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGINE);
        let (s, _, _) = envoyer(
            &e.app,
            req("POST", "/ok").header(header::AUTHORIZATION, "Bearer x"),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        for methode in ["GET", "HEAD"] {
            let (s, _, _) = envoyer(
                &e.app,
                req(methode, "/healthz").header(header::COOKIE, "sid=abc"),
            )
            .await;
            assert_eq!(s, StatusCode::OK, "{methode}");
        }
    }

    #[tokio::test]
    async fn csrf_ignore_les_cookies_invalides() {
        let e = env(false).await.unwrap();
        for cookie in [
            "",
            "sansegal",
            "=v",
            "n°m=v",
            "a b=v",
            "n=v\\w",
            "n=\"v\"x\"",
            " ; ; ",
        ] {
            let (s, _, _) =
                envoyer(&e.app, req("POST", "/ok").header(header::COOKIE, cookie)).await;
            assert_eq!(s, StatusCode::OK, "{cookie:?}");
        }
        for cookie in ["a=1", "x; a=1", "a=\"v\"", "a=", "x=1;y=2"] {
            let (s, _, _) =
                envoyer(&e.app, req("POST", "/ok").header(header::COOKIE, cookie)).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "{cookie:?}");
        }
    }

    #[tokio::test]
    async fn refus_426_selon_la_version() {
        let e = env(false).await.unwrap();
        for (nom, version, plateforme, requete, attendu) in [
            ("sans en-tête", "", "", "", StatusCode::OK),
            ("à jour", "0.6.0", "windows", "", StatusCode::OK),
            (
                "sous le minimum commun",
                "0.4.9",
                "windows",
                "",
                StatusCode::UPGRADE_REQUIRED,
            ),
            (
                "minimum de plateforme",
                "0.9.0",
                "android",
                "",
                StatusCode::UPGRADE_REQUIRED,
            ),
            (
                "pwa suit web",
                "0.4.0",
                "pwa",
                "",
                StatusCode::UPGRADE_REQUIRED,
            ),
            (
                "paramètres du WebSocket",
                "",
                "",
                "?v=0.4.0&platform=web",
                StatusCode::UPGRADE_REQUIRED,
            ),
            ("version illisible", "abc", "web", "", StatusCode::OK),
        ] {
            let mut r = req("GET", &format!("/healthz{requete}"));
            if !version.is_empty() {
                r = r
                    .header("X-Estompe-Version", version)
                    .header("X-Estompe-Platform", plateforme);
            }
            let (s, h, c) = envoyer(&e.app, r).await;
            assert_eq!(s, attendu, "{nom}");
            if attendu == StatusCode::UPGRADE_REQUIRED {
                assert_eq!(h[header::CONTENT_TYPE], "application/json", "{nom}");
                assert!(c.contains("\"upgrade_required\""), "{nom} : {c}");
            }
        }
    }

    #[tokio::test]
    async fn corps_du_426() {
        let e = env(false).await.unwrap();
        let (s, _, c) = envoyer(
            &e.app,
            req("GET", "/me")
                .header("X-Estompe-Version", "0.9.0")
                .header("X-Estompe-Platform", "android"),
        )
        .await;
        assert_eq!(s, StatusCode::UPGRADE_REQUIRED);
        assert_eq!(
            c,
            "{\"error\":\"upgrade_required\",\"message\":\"faille corrigee\",\"min_version\":\"1.0.0\"}\n"
        );
    }

    #[tokio::test]
    async fn les_en_tetes_priment_sur_la_requete() {
        let e = env(false).await.unwrap();
        let (s, _, _) = envoyer(
            &e.app,
            req("GET", "/healthz?v=0.1.0&platform=web")
                .header("X-Estompe-Version", "0.6.0")
                .header("X-Estompe-Platform", "web"),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }

    async fn erreurs(e: &Env) -> Vec<(String, String, i32)> {
        sqlx::query_as("SELECT method, path, status FROM server_errors ORDER BY id")
            .fetch_all(e.state.pool().ok().unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn une_reponse_5xx_est_inscrite() {
        let Some(e) = env(true).await else { return };
        let (s, _, _) = envoyer(&e.app, req("GET", "/boom")).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        envoyer(&e.app, req("GET", "/ok")).await;
        envoyer(&e.app, req("GET", "/ws/x")).await;
        assert_eq!(erreurs(&e).await, [("GET".into(), "/boom".into(), 500)]);
    }

    #[tokio::test]
    async fn le_chemin_journalise_est_decode() {
        let Some(e) = env(true).await else { return };
        envoyer(&e.app, req("GET", "/%C3%A9t%C3%A9+x")).await;
        assert!(erreurs(&e).await.is_empty());
        sqlx::query("INSERT INTO server_errors (method, path, status) VALUES ('GET', '/a', 500)")
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        envoyer(&e.app, req("POST", "/boom")).await;
        assert_eq!(erreurs(&e).await.len(), 1);
    }

    #[tokio::test]
    async fn le_chemin_journalise_est_tronque() {
        let Some(e) = env(true).await else { return };
        let long = format!("/{}", "é".repeat(600));
        crate::db::journaliser_erreur(e.state.pool().ok().unwrap(), "GET", &long, 500)
            .await
            .unwrap();
        let chemins = erreurs(&e).await;
        assert_eq!(chemins[0].1.chars().count(), 512);
    }

    #[tokio::test]
    async fn les_erreurs_de_plus_de_30_jours_sont_purgees() {
        let Some(e) = env(true).await else { return };
        let pool = e.state.pool().ok().unwrap();
        for (jours, chemin) in [(40, "/vieille"), (29, "/recente")] {
            sqlx::query(
                "INSERT INTO server_errors (at, method, path, status) VALUES (now() - make_interval(days => $1), 'GET', $2, 500)",
            )
            .bind(jours)
            .bind(chemin)
            .execute(pool)
            .await
            .unwrap();
        }
        envoyer(&e.app, req("GET", "/boom")).await;
        let chemins: Vec<String> = erreurs(&e).await.into_iter().map(|(_, p, _)| p).collect();
        assert_eq!(chemins, ["/recente", "/boom"]);
    }

    #[tokio::test]
    async fn une_panique_devient_un_500_journalise() {
        let Some(e) = env(true).await else { return };
        let (s, h, c) = envoyer(&e.app, req("GET", "/panic")).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
        assert_eq!(
            c,
            "{\"error\":\"internal\",\"message\":\"erreur interne\"}\n"
        );
        assert_eq!(erreurs(&e).await, [("GET".into(), "/panic".into(), 500)]);
    }

    #[tokio::test]
    async fn sans_base_la_reponse_5xx_reste_intacte() {
        let e = env(false).await.unwrap();
        let (s, _, _) = envoyer(&e.app, req("GET", "/boom")).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        let (s, _, c) = envoyer(&e.app, req("GET", "/panic")).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            c,
            "{\"error\":\"internal\",\"message\":\"erreur interne\"}\n"
        );
    }

    async fn metriques(e: &Env, jeton: Option<&str>) -> (StatusCode, HeaderMap, String) {
        let mut r = req("GET", "/metrics");
        if let Some(j) = jeton {
            r = r.header(header::AUTHORIZATION, j);
        }
        envoyer(&e.app, r).await
    }

    #[tokio::test]
    async fn metrics_sans_jeton_configure_404() {
        let state = AppState::new(testutil::config(), vec![]);
        let app = envelopper(api::routes(), state);
        let (s, _, c) = envoyer(
            &app,
            req("GET", "/metrics").header(header::AUTHORIZATION, "Bearer x"),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(c, "404 page not found\n");
    }

    #[tokio::test]
    async fn metrics_refuse_sans_jeton_ou_mauvais() {
        let e = env(false).await.unwrap();
        for jeton in [
            None,
            Some("Bearer mauvais"),
            Some("Bearer "),
            Some("sekre"),
            Some("Bearer sekret2"),
        ] {
            let (s, h, c) = metriques(&e, jeton).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{jeton:?}");
            assert_eq!(h[header::WWW_AUTHENTICATE], "Bearer");
            assert_eq!(c, "unauthorized\n");
        }
    }

    #[tokio::test]
    async fn metrics_accepte_le_bon_jeton_et_le_jeton_nu() {
        let e = env(false).await.unwrap();
        for jeton in ["Bearer sekret", "sekret"] {
            let (s, h, c) = metriques(&e, Some(jeton)).await;
            assert_eq!(s, StatusCode::OK, "{jeton}");
            assert_eq!(h[header::CONTENT_TYPE], "text/plain; version=0.0.4");
            for nom in [
                "estompe_up 1\n",
                "estompe_database_up 0\n",
                "estompe_http_inflight 1\n",
                "estompe_ws_users 0\n",
                "estompe_ws_connections 0\n",
                "estompe_search_duration_seconds_count 0\n",
            ] {
                assert!(c.contains(nom), "{nom}\n{c}");
            }
            for absent in ["estompe_users", "estompe_storage"] {
                assert!(!c.contains(absent), "{absent}");
            }
        }
    }

    #[tokio::test]
    async fn metrics_totaux_et_stockage_depuis_la_base() {
        let Some(e) = env(true).await else { return };
        let pool = e.state.pool().ok().unwrap();
        sqlx::query(
            "WITH u AS (INSERT INTO users(keycloak_sub, pseudo) VALUES ('s', 'alice') RETURNING id),
             s AS (INSERT INTO servers(name, owner_id) SELECT 'x', id FROM u RETURNING id),
             c AS (INSERT INTO channels(server_id, name, kind) SELECT id, 'g', 'text' FROM s RETURNING id),
             m AS (INSERT INTO messages(channel_id, author_id, content) SELECT c.id, u.id, 'salut' FROM c, u RETURNING id),
             f AS (INSERT INTO files(uploader_id, message_id, name, mime, size, key) SELECT u.id, m.id, 'a', 'text/plain', 1000, 'k1' FROM u, m RETURNING uploader_id)
             INSERT INTO files(uploader_id, name, mime, size, key, purged_at) SELECT uploader_id, 'b', 'text/plain', 500, 'k2', now() FROM f",
        )
        .execute(pool)
        .await
        .unwrap();
        let utilisateurs: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(utilisateurs >= 1);
        let (s, _, c) = metriques(&e, Some("Bearer sekret")).await;
        assert_eq!(s, StatusCode::OK);
        for ligne in [
            "estompe_database_up 1\n",
            &format!("estompe_users {utilisateurs}\n"),
            "estompe_messages 1\n",
            "estompe_files 1\n",
            "estompe_storage_bytes 1000\n",
            "estompe_storage_max_bytes 3221225472\n",
        ] {
            assert!(c.contains(ligne), "{ligne}\n{c}");
        }
    }

    #[tokio::test]
    async fn metrics_compte_les_codes_des_autres_middlewares() {
        let e = env(false).await.unwrap();
        envoyer(&e.app, req("GET", "/healthz")).await;
        envoyer(&e.app, req("POST", "/ok").header(header::COOKIE, "a=1")).await;
        envoyer(
            &e.app,
            req("GET", "/healthz")
                .header("X-Estompe-Version", "0.1.0")
                .header("X-Estompe-Platform", "web"),
        )
        .await;
        envoyer(&e.app, req("GET", "/boom")).await;
        envoyer(&e.app, req("GET", "/ws/x")).await;
        let (_, _, c) = metriques(&e, Some("Bearer sekret")).await;
        for ligne in [
            "estompe_http_requests_total{code=\"2xx\"} 1\n",
            "estompe_http_requests_total{code=\"4xx\"} 2\n",
            "estompe_http_requests_total{code=\"5xx\"} 1\n",
            "estompe_http_request_duration_seconds_count 4\n",
        ] {
            assert!(c.contains(ligne), "{ligne}\n{c}");
        }
        e.state
            .metrics()
            .searched(std::time::Duration::from_millis(500));
        let (_, _, c) = metriques(&e, Some("Bearer sekret")).await;
        assert!(c.contains("estompe_search_duration_seconds_sum 0.5\n"));
        assert!(c.contains("estompe_search_duration_seconds_count 1\n"));
    }

    #[tokio::test]
    async fn not_found_comme_le_mux_go() {
        let e = env(false).await.unwrap();
        let (s, h, c) = envoyer(&e.app, req("GET", "/inconnue")).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(c, "404 page not found\n");
        assert_eq!(h[header::CONTENT_TYPE], "text/plain; charset=utf-8");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }

    #[tokio::test]
    async fn method_not_allowed_comme_le_mux_go() {
        let e = env(false).await.unwrap();
        let (s, h, c) = envoyer(&e.app, req("POST", "/healthz")).await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(c, "Method Not Allowed\n");
        assert_eq!(h[header::CONTENT_TYPE], "text/plain; charset=utf-8");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(h[header::ALLOW], "GET, HEAD");
        let (_, h, _) = envoyer(&e.app, req("GET", "/activate")).await;
        assert_eq!(h[header::ALLOW], "POST");
    }

    #[tokio::test]
    async fn les_404_et_405_traversent_cors_426_et_metriques() {
        let e = env(false).await.unwrap();
        let (s, h, _) = envoyer(&e.app, req("GET", "/inconnue").header("Origin", ORIGINE)).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGINE);
        let (s, _, c) = envoyer(
            &e.app,
            req("POST", "/healthz")
                .header("X-Estompe-Version", "0.1.0")
                .header("X-Estompe-Platform", "web"),
        )
        .await;
        assert_eq!(s, StatusCode::UPGRADE_REQUIRED);
        assert!(c.contains("upgrade_required"));
        envoyer(&e.app, req("POST", "/healthz")).await;
        let (_, _, c) = metriques(&e, Some("Bearer sekret")).await;
        assert!(
            c.contains("estompe_http_requests_total{code=\"4xx\"} 3\n"),
            "{c}"
        );
    }

    #[tokio::test]
    async fn un_404_avec_corps_n_est_pas_reecrit() {
        let state = AppState::new(testutil::config(), vec![]);
        let app = envelopper(
            Router::new().route("/x", get(|| async { (StatusCode::NOT_FOUND, "absent") })),
            state,
        );
        let (s, _, c) = envoyer(&app, req("GET", "/x")).await;
        assert_eq!((s, c.as_str()), (StatusCode::NOT_FOUND, "absent"));
    }
}
