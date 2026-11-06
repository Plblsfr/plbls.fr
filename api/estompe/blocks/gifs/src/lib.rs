use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    Extension,
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use estompe_core::{
    api::ActiveUser,
    fichiers::importer_fichier,
    http::{champ_texte, decode, internal, write_err},
    openapi,
    state::AppState,
};
use regex::Regex;
use reqwest::{Client, Url, redirect::Policy};
use serde::{Deserialize, Serialize};

use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const PAR_MINUTE: usize = 30;
const PAGE: &str = "24";
const DELAI: Duration = Duration::from_secs(10);
const LISTE_MAX: usize = 4 << 20;
const GIF_MAX: usize = 8 << 20;
const HOTE_GIPHY: &str = r"^(media[0-9]*|i)\.giphy\.com$";

pub struct Reglages {
    base: String,
    hote: Regex,
    https: bool,
    client: Client,
    client_sans_redirection: Client,
}

impl Reglages {
    pub fn new(base: &str, hote: Regex, https: bool) -> Self {
        let construire = |politique| {
            Client::builder()
                .timeout(DELAI)
                .redirect(politique)
                .build()
                .expect("client HTTP")
        };
        Self {
            base: base.trim_end_matches('/').to_string(),
            hote,
            https,
            client: construire(Policy::default()),
            client_sans_redirection: construire(Policy::none()),
        }
    }

    pub fn production() -> Self {
        Self::new(
            "https://api.giphy.com",
            Regex::new(HOTE_GIPHY).expect("expression d'hôte"),
            true,
        )
    }
}

enum Erreur {
    UrlInterdite,
    Indisponible,
}

#[derive(Deserialize)]
struct Image {
    url: Option<String>,
    width: Option<String>,
    height: Option<String>,
}

#[derive(Deserialize)]
struct Entree {
    id: Option<String>,
    title: Option<String>,
    images: Option<HashMap<String, Option<Image>>>,
}

#[derive(Deserialize)]
struct Liste {
    data: Option<Vec<Option<Entree>>>,
}

#[derive(Serialize)]
struct Resultat {
    id: String,
    title: String,
    preview: String,
    url: String,
    width: i64,
    height: i64,
}

fn entier(s: Option<&str>) -> i64 {
    let s = s.unwrap_or_default().trim_start();
    let (signe, chiffres) = match s.strip_prefix('-') {
        Some(reste) => (-1, reste),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let fin = chiffres
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(chiffres.len());
    chiffres[..fin].parse::<i64>().map_or(0, |n| signe * n)
}

async fn lire_borne(mut reponse: reqwest::Response, max: usize) -> Option<Vec<u8>> {
    let mut corps = Vec::new();
    while let Some(morceau) = reponse.chunk().await.ok()? {
        corps.extend_from_slice(&morceau);
        if corps.len() > max {
            return None;
        }
    }
    Some(corps)
}

fn resultats(octets: &[u8]) -> Option<Vec<Resultat>> {
    let liste = serde_json::Deserializer::from_slice(octets)
        .into_iter::<Liste>()
        .next()?
        .ok()?;
    let mut sortie = Vec::new();
    for e in liste.data.unwrap_or_default().into_iter().flatten() {
        let images = e.images.unwrap_or_default();
        let image = |nom: &str| images.get(nom).and_then(Option::as_ref);
        let url = |i: Option<&Image>| i.and_then(|i| i.url.clone()).unwrap_or_default();
        let apercu = url(image("fixed_width_small"));
        let mut complete = image("downsized");
        if url(complete).is_empty() {
            complete = image("fixed_width");
        }
        let adresse = url(complete);
        if apercu.is_empty() || adresse.is_empty() {
            continue;
        }
        let dimension = |f: fn(&Image) -> Option<&str>| entier(complete.and_then(f));
        sortie.push(Resultat {
            id: e.id.unwrap_or_default(),
            title: e.title.unwrap_or_default(),
            preview: apercu,
            url: adresse,
            width: dimension(|i| i.width.as_deref()),
            height: dimension(|i| i.height.as_deref()),
        });
    }
    Some(sortie)
}

impl Reglages {
    async fn interroger(
        &self,
        cle: &str,
        endpoint: &str,
        q: Option<&str>,
        offset: i64,
    ) -> Option<Vec<Resultat>> {
        let mut parametres = vec![
            ("api_key", cle.to_string()),
            ("limit", PAGE.to_string()),
            ("offset", offset.to_string()),
            ("rating", "pg-13".to_string()),
            ("lang", "fr".to_string()),
        ];
        if let Some(q) = q {
            parametres.push(("q", q.to_string()));
        }
        let reponse = self
            .client
            .get(format!("{}/v1/gifs/{endpoint}", self.base))
            .query(&parametres)
            .send()
            .await
            .ok()?;
        if reponse.status() != reqwest::StatusCode::OK {
            return None;
        }
        resultats(&lire_borne(reponse, LISTE_MAX).await?)
    }

    async fn recuperer(&self, brut: &str) -> Result<Vec<u8>, Erreur> {
        let url = Url::parse(brut).map_err(|_| Erreur::UrlInterdite)?;
        let identifiants = !url.username().is_empty()
            || url.password().is_some()
            || brut
                .split(['/', '?', '#'])
                .nth(2)
                .is_some_and(|autorite| autorite.contains('@'));
        let hote = url.host_str().unwrap_or_default().to_lowercase();
        if (self.https && url.scheme() != "https") || identifiants || !self.hote.is_match(&hote) {
            return Err(Erreur::UrlInterdite);
        }
        let reponse = self
            .client_sans_redirection
            .get(url)
            .send()
            .await
            .map_err(|_| Erreur::Indisponible)?;
        let redirection = matches!(reponse.status().as_u16(), 301 | 302 | 303 | 307 | 308);
        if redirection && reponse.headers().contains_key(header::LOCATION) {
            return Err(Erreur::UrlInterdite);
        }
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(Erreur::Indisponible);
        }
        lire_borne(reponse, GIF_MAX)
            .await
            .ok_or(Erreur::Indisponible)
    }
}

pub fn routes(reglages: Arc<Reglages>) -> OpenApiRouter<AppState> {
    let recherche = |resume: &str| {
        openapi::operation(
            "messages",
            resume,
            &[
                ("200", "ok"),
                ("400", "invalid_query"),
                ("429", "too_many_requests"),
                ("501", "no_gifs"),
                ("502", "gif_unavailable"),
            ],
            "",
        )
    };
    let import = openapi::operation(
        "messages",
        "Copie un GIF Giphy comme fichier en attente, à joindre à un message",
        &[
            ("201", "fichier créé"),
            ("400", "`bad_request` ou `invalid_url`"),
            ("429", "`too_many_pending_files` ou quota"),
            ("501", "no_gifs"),
            ("502", "gif_unavailable"),
        ],
        "/gifs/import",
    );
    let r = OpenApiRouter::new()
        .route("/gifs/search", get(rechercher))
        .route("/gifs/trending", get(tendances))
        .route("/gifs/import", post(importer));
    let r = openapi::documenter(
        r,
        "/gifs/search",
        HttpMethod::Get,
        recherche("Recherche de GIF (Giphy)"),
    );
    let r = openapi::documenter(
        r,
        "/gifs/trending",
        HttpMethod::Get,
        recherche("GIF du moment (Giphy)"),
    );
    openapi::documenter(r, "/gifs/import", HttpMethod::Post, import).layer(Extension(reglages))
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes(Arc::new(Reglages::production())))
}

fn indisponible() -> Response {
    write_err(
        StatusCode::NOT_IMPLEMENTED,
        "no_gifs",
        "recherche de GIF indisponible",
    )
}

async fn rechercher(
    State(state): State<AppState>,
    Extension(reglages): Extension<Arc<Reglages>>,
    ActiveUser(user): ActiveUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Response {
    lister(&state, &reglages, &user.id, &params, false).await
}

async fn tendances(
    State(state): State<AppState>,
    Extension(reglages): Extension<Arc<Reglages>>,
    ActiveUser(user): ActiveUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Response {
    lister(&state, &reglages, &user.id, &params, true).await
}

async fn lister(
    state: &AppState,
    reglages: &Reglages,
    utilisateur: &str,
    params: &[(String, String)],
    tendances: bool,
) -> Response {
    let cle = &state.config().giphy_key;
    if cle.is_empty() {
        return indisponible();
    }
    let param = |nom: &str| {
        params
            .iter()
            .find(|(k, _)| k == nom)
            .map_or("", |(_, v)| v.as_str())
    };
    let q = param("q");
    let n = q.chars().count();
    if !tendances && !(1..=100).contains(&n) {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "recherche : 1 à 100 caractères",
        );
    }
    let offset = param("offset")
        .parse::<i64>()
        .ok()
        .filter(|o| (0..=480).contains(o))
        .unwrap_or(0);
    if !state
        .limiteurs()
        .gif
        .allow(utilisateur, PAR_MINUTE, Duration::from_secs(60))
    {
        let mut r = write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_requests",
            "trop de recherches",
        );
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        return r;
    }
    let liste = if tendances {
        reglages.interroger(cle, "trending", None, offset).await
    } else {
        reglages.interroger(cle, "search", Some(q), offset).await
    };
    let Some(liste) = liste else {
        return write_err(
            StatusCode::BAD_GATEWAY,
            "gif_unavailable",
            "service de GIF injoignable",
        );
    };
    let Ok(corps) = serde_json::to_string(&liste) else {
        return internal();
    };
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=300"),
            ),
        ],
        format!("{corps}\n"),
    )
        .into_response()
}

async fn importer(
    State(state): State<AppState>,
    Extension(reglages): Extension<Arc<Reglages>>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    if state.config().giphy_key.is_empty() {
        return indisponible();
    }
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(url) = champ_texte(&objet, "url") else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    };
    match reglages.recuperer(&url).await {
        Ok(donnees) => importer_fichier(&state, &user, "gif.gif".into(), donnees).await,
        Err(Erreur::UrlInterdite) => write_err(
            StatusCode::BAD_REQUEST,
            "invalid_url",
            "adresse de GIF non autorisée",
        ),
        Err(Erreur::Indisponible) => write_err(
            StatusCode::BAD_GATEWAY,
            "gif_unavailable",
            "GIF indisponible",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{PAR_MINUTE, Reglages, entier, routes};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
    };
    use estompe_core::{
        db::migrate_embarquees,
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux, faux_avec_entetes},
    };
    use regex::Regex;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tower::ServiceExt;

    const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";

    struct Env {
        app: Router,
        state: AppState,
        _dossier: tempfile::TempDir,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env(base: &str, cle: &str, reglages: Option<Reglages>) -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux(|_| (200, json!({ "allowed": true }).to_string().into_bytes())).await;
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
        let dossier = tempfile::tempdir().unwrap();
        let data = dossier.path().join("data");
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("PERMISSION_URL", permission.url.as_str()),
                ("DATA_DIR", data.to_str().unwrap()),
                ("GIPHY_API_KEY", cle),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let reglages = reglages
            .unwrap_or_else(|| Reglages::new(base, Regex::new(r"^127\.0\.0\.1$").unwrap(), false));
        let app = Router::from(routes(Arc::new(reglages))).with_state(state.clone());
        Some(Env {
            app,
            state,
            _dossier: dossier,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn appel(
        app: &Router,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: &str,
    ) -> (StatusCode, HeaderMap, Value) {
        let rep = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(methode)
                    .uri(chemin)
                    .header(header::AUTHORIZATION, format!("Bearer {nom}"))
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (s, h) = (rep.status(), rep.headers().clone());
        let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (s, h, serde_json::from_slice(&c).unwrap_or(Value::Null))
    }

    fn liste(chemin: &str) -> (u16, Vec<u8>) {
        match chemin {
            "/v1/gifs/search" | "/v1/gifs/trending" => (
                200,
                json!({ "data": [
                    { "id": "1", "title": "un chat", "images": {
                        "fixed_width_small": { "url": "https://media1.giphy.com/p.gif" },
                        "downsized": { "url": "https://media1.giphy.com/g.gif", "width": "200", "height": "100" } } },
                    { "id": "2", "images": {} },
                    { "id": "3", "images": { "fixed_width_small": { "url": "p" } } },
                    { "id": "4", "title": null, "images": {
                        "fixed_width_small": { "url": "p4" },
                        "fixed_width": { "url": "f4", "width": "12px", "height": "x" } } },
                    null
                ] })
                .to_string()
                .into_bytes(),
            ),
            _ => (404, vec![]),
        }
    }

    #[test]
    fn entiers_comme_sscan() {
        assert_eq!(entier(Some("200")), 200);
        assert_eq!(entier(Some(" 12px")), 12);
        assert_eq!(entier(Some("-3")), -3);
        assert_eq!(entier(Some("x")), 0);
        assert_eq!(entier(Some("")), 0);
        assert_eq!(entier(None), 0);
        assert_eq!(entier(Some("99999999999999999999")), 0);
    }

    #[tokio::test]
    async fn recherche_et_tendances() {
        let giphy = faux(|r| liste(&r.chemin)).await;
        let Some(e) = env(&giphy.url, "k", None).await else {
            return;
        };
        let (s, h, v) = appel(&e.app, "alice", "GET", "/gifs/search?q=chat&offset=24", "").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=300");
        assert_eq!(
            v,
            json!([
                { "id": "1", "title": "un chat", "preview": "https://media1.giphy.com/p.gif",
                  "url": "https://media1.giphy.com/g.gif", "width": 200, "height": 100 },
                { "id": "4", "title": "", "preview": "p4", "url": "f4", "width": 12, "height": 0 }
            ])
        );
        let brut = e
            .app
            .clone()
            .oneshot(
                Request::get("/gifs/search?q=chat")
                    .header(header::AUTHORIZATION, "Bearer alice")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let brut = to_bytes(brut.into_body(), 1 << 20).await.unwrap();
        assert!(
            brut.starts_with(
                br#"[{"id":"1","title":"un chat","preview":"https://media1.giphy.com/p.gif","url":"https://media1.giphy.com/g.gif","width":200,"height":100}"#
            ),
            "{}",
            String::from_utf8_lossy(&brut)
        );
        let r = giphy.requetes().remove(0);
        let mut recus: Vec<_> = r.query.split('&').collect();
        recus.sort_unstable();
        assert_eq!(
            recus,
            [
                "api_key=k",
                "lang=fr",
                "limit=24",
                "offset=24",
                "q=chat",
                "rating=pg-13"
            ]
        );
        let (s, _, v) = appel(
            &e.app,
            "alice",
            "GET",
            "/gifs/trending?offset=24&q=ignore",
            "",
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v.as_array().unwrap().len(), 2);
        let r = giphy.requetes().pop().unwrap();
        assert_eq!(r.chemin, "/v1/gifs/trending");
        assert!(r.query.contains("offset=24") && !r.query.contains("q="));
    }

    #[test]
    fn listes_vides() {
        for corps in [r#"{"data":[]}"#, r#"{"data":null}"#, "{}"] {
            let r = crate::resultats(corps.as_bytes()).unwrap();
            assert_eq!(serde_json::to_value(&r).unwrap(), json!([]), "{corps}");
        }
    }

    #[tokio::test]
    async fn requete_et_decalage() {
        let giphy = faux(|r| liste(&r.chemin)).await;
        let Some(e) = env(&giphy.url, "k", None).await else {
            return;
        };
        for q in ["", "?q=", &format!("?q={}", "a".repeat(101))] {
            let (s, _, v) = appel(&e.app, "alice", "GET", &format!("/gifs/search{q}"), "").await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_query")),
                "{q}"
            );
        }
        let accents = format!("?q={}", "é".repeat(100));
        assert_eq!(
            appel(
                &e.app,
                "alice",
                "GET",
                &format!("/gifs/search{accents}"),
                ""
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(giphy.appels(), 1);
        for (offset, attendu) in [
            ("abc", "0"),
            ("-1", "0"),
            ("481", "0"),
            ("480", "480"),
            ("", "0"),
        ] {
            let (s, _, _) = appel(
                &e.app,
                "bob",
                "GET",
                &format!("/gifs/trending?offset={offset}"),
                "",
            )
            .await;
            assert_eq!(s, StatusCode::OK);
            let r = giphy.requetes().pop().unwrap();
            assert!(
                r.query.split('&').any(|p| p == format!("offset={attendu}")),
                "{offset} : {}",
                r.query
            );
        }
    }

    #[tokio::test]
    async fn giphy_en_erreur() {
        let gros = vec![b' '; (4 << 20) + 10];
        let cas: Vec<(u16, Vec<u8>)> = vec![
            (500, vec![]),
            (200, b"pas du json".to_vec()),
            (200, vec![]),
            (200, {
                let mut v = b"{\"data\":[],\"x\":\"".to_vec();
                v.extend_from_slice(&gros);
                v.extend_from_slice(b"\"}");
                v
            }),
        ];
        for (statut, corps) in cas {
            let giphy = faux(move |_| (statut, corps.clone())).await;
            let Some(e) = env(&giphy.url, "k", None).await else {
                return;
            };
            let (s, _, v) = appel(&e.app, "alice", "GET", "/gifs/search?q=x", "").await;
            assert_eq!(
                (s, v["error"].as_str(), v["message"].as_str()),
                (
                    StatusCode::BAD_GATEWAY,
                    Some("gif_unavailable"),
                    Some("service de GIF injoignable")
                )
            );
        }
        let Some(e) = env("http://127.0.0.1:1", "k", None).await else {
            return;
        };
        let (s, _, v) = appel(&e.app, "alice", "GET", "/gifs/trending", "").await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_GATEWAY, Some("gif_unavailable"))
        );
    }

    #[tokio::test]
    async fn limiteur_partage_par_utilisateur() {
        let giphy = faux(|r| liste(&r.chemin)).await;
        let Some(e) = env(&giphy.url, "k", None).await else {
            return;
        };
        for i in 0..PAR_MINUTE {
            let chemin = if i % 2 == 0 {
                "/gifs/search?q=x"
            } else {
                "/gifs/trending"
            };
            assert_eq!(
                appel(&e.app, "alice", "GET", chemin, "").await.0,
                StatusCode::OK
            );
        }
        let (s, h, v) = appel(&e.app, "alice", "GET", "/gifs/trending", "").await;
        assert_eq!(h[header::RETRY_AFTER], "60");
        assert_eq!(
            (s, v["error"].as_str(), v["message"].as_str()),
            (
                StatusCode::TOO_MANY_REQUESTS,
                Some("too_many_requests"),
                Some("trop de recherches")
            )
        );
        assert_eq!(
            appel(&e.app, "bob", "GET", "/gifs/search?q=x", "").await.0,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn sans_cle_501_avant_toute_validation() {
        let Some(e) = env("http://127.0.0.1:1", "", None).await else {
            return;
        };
        for (m, c, corps) in [
            ("GET", "/gifs/search?q=chat", ""),
            ("GET", "/gifs/search?q=", ""),
            ("GET", "/gifs/trending", ""),
            ("POST", "/gifs/import", "{}"),
            ("POST", "/gifs/import", "pas du json"),
        ] {
            let (s, _, v) = appel(&e.app, "alice", m, c, corps).await;
            assert_eq!(
                (s, v["error"].as_str(), v["message"].as_str()),
                (
                    StatusCode::NOT_IMPLEMENTED,
                    Some("no_gifs"),
                    Some("recherche de GIF indisponible")
                ),
                "{c}"
            );
        }
    }

    #[tokio::test]
    async fn import_d_un_gif() {
        let giphy = faux(|r| match r.chemin.as_str() {
            "/g.gif" => (200, GIF.to_vec()),
            "/texte.gif" => (200, b"ceci n'est pas un gif".to_vec()),
            "/lourd.gif" => (200, vec![0; (8 << 20) + 1]),
            _ => (404, vec![]),
        })
        .await;
        let Some(e) = env(&giphy.url, "k", None).await else {
            return;
        };
        let (s, _, v) = appel(
            &e.app,
            "alice",
            "POST",
            "/gifs/import",
            &json!({ "url": format!("{}/g.gif", giphy.url) }).to_string(),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        assert_eq!(
            (v["mime"].as_str(), v["name"].as_str()),
            (Some("image/gif"), Some("gif.gif"))
        );
        assert_eq!(v["kind"], "file");
        let id = v["id"].as_str().unwrap();
        let ligne: (String, String) =
            sqlx::query_as("SELECT mime, name FROM files WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(e.state.pool().ok().unwrap())
                .await
                .unwrap();
        assert_eq!(ligne, ("image/gif".into(), "gif.gif".into()));

        let importer = |chemin: &str| {
            let corps = json!({ "url": format!("{}{chemin}", giphy.url) }).to_string();
            let app = e.app.clone();
            async move {
                let (s, _, v) = appel(&app, "alice", "POST", "/gifs/import", &corps).await;
                (
                    s,
                    v["error"].as_str().map(String::from),
                    v["message"].as_str().map(String::from),
                )
            }
        };
        assert_eq!(
            importer("/absent.gif").await,
            (
                StatusCode::BAD_GATEWAY,
                Some("gif_unavailable".into()),
                Some("GIF indisponible".into())
            )
        );
        assert_eq!(importer("/lourd.gif").await.0, StatusCode::BAD_GATEWAY);
        assert_eq!(
            importer("/texte.gif").await,
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Some("type_mismatch".into()),
                Some("le contenu ne correspond pas au type annoncé par l'extension".into())
            )
        );
        for corps in ["pas du json", r#"{"url":5}"#] {
            let (s, _, v) = appel(&e.app, "alice", "POST", "/gifs/import", corps).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("bad_request")),
                "{corps}"
            );
        }
    }

    #[tokio::test]
    async fn import_limite_les_fichiers_en_attente() {
        let giphy = faux(|_| (200, GIF.to_vec())).await;
        let Some(e) = env(&giphy.url, "k", None).await else {
            return;
        };
        let corps = json!({ "url": format!("{}/g.gif", giphy.url) }).to_string();
        for _ in 0..20 {
            let (s, _, v) = appel(&e.app, "alice", "POST", "/gifs/import", &corps).await;
            assert_eq!(s, StatusCode::CREATED, "{v}");
        }
        let (s, _, v) = appel(&e.app, "alice", "POST", "/gifs/import", &corps).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (
                StatusCode::TOO_MANY_REQUESTS,
                Some("too_many_pending_files")
            )
        );
    }

    #[tokio::test]
    async fn import_refuse_les_adresses_non_autorisees() {
        let Some(e) = env("http://127.0.0.1:1", "k", Some(Reglages::production())).await else {
            return;
        };
        for url in [
            "http://media1.giphy.com/g.gif",
            "http://evil.example/g.gif",
            "file:///etc/passwd",
            "https://evil.example/g.gif",
            "https://user:mdp@media1.giphy.com/g.gif",
            "https://user@media1.giphy.com/g.gif",
            "https://@media1.giphy.com/g.gif",
            "https://media1.giphy.com.evil.example/g.gif",
            "https://evilmedia1.giphy.com/g.gif",
            "https://giphy.com/g.gif",
            "pas une url",
            "",
        ] {
            let (s, _, v) = appel(
                &e.app,
                "alice",
                "POST",
                "/gifs/import",
                &json!({ "url": url }).to_string(),
            )
            .await;
            assert_eq!(
                (s, v["error"].as_str(), v["message"].as_str()),
                (
                    StatusCode::BAD_REQUEST,
                    Some("invalid_url"),
                    Some("adresse de GIF non autorisée")
                ),
                "{url}"
            );
        }
    }

    #[tokio::test]
    async fn import_ne_suit_aucune_redirection() {
        let cible = faux(|_| (200, GIF.to_vec())).await;
        let destination = format!("{}/g.gif", cible.url);
        let giphy = faux_avec_entetes(move |r| match r.chemin.as_str() {
            "/vers.gif" => (302, vec![("location", destination.clone())], vec![]),
            "/sans-location.gif" => (302, vec![], vec![]),
            "/300.gif" => (300, vec![("location", destination.clone())], vec![]),
            _ => (404, vec![], vec![]),
        })
        .await;
        let Some(e) = env(&giphy.url, "k", None).await else {
            return;
        };
        let mut resultats = vec![];
        for chemin in ["/vers.gif", "/sans-location.gif", "/300.gif"] {
            let (s, _, v) = appel(
                &e.app,
                "alice",
                "POST",
                "/gifs/import",
                &json!({ "url": format!("{}{chemin}", giphy.url) }).to_string(),
            )
            .await;
            resultats.push((s, v["error"].as_str().map(String::from)));
        }
        assert_eq!(
            resultats,
            [
                (StatusCode::BAD_REQUEST, Some("invalid_url".to_string())),
                (StatusCode::BAD_GATEWAY, Some("gif_unavailable".to_string())),
                (StatusCode::BAD_GATEWAY, Some("gif_unavailable".to_string()))
            ]
        );
        assert_eq!(cible.appels(), 0);
    }
}
