use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    catalogue, db,
    http::{champ_texte, decode, internal, text_error, write_err, write_json},
    identity::{Erreur, Identity, cred_of},
    metrics::{Instantane, Stockage, Totaux},
    openapi::{self, Base},
    state::{AppState, DbIndisponible},
    users::{self, ActivateError, User},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(Base::openapi())
        .routes(routes!(healthz))
        .routes(routes!(readyz))
        .routes(routes!(metrics))
        .routes(routes!(me))
        .routes(routes!(activate))
        .merge(crate::ws::router())
        .merge(crate::fichiers::router())
        .merge(crate::servers::router())
        .merge(crate::administration::router())
        .merge(crate::relais_aegis::router())
        .merge(crate::aegis::router())
}

pub fn routes() -> Router<AppState> {
    router().into()
}

#[utoipa::path(get, path = "/healthz", summary = "Liveness", security(),
    responses((status = 200, description = "ok", content_type = "text/plain", body = String)))]
async fn healthz() -> &'static str {
    "ok"
}

pub struct Auth(pub Identity);

impl FromRequestParts<AppState> for Auth {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        match state
            .identity()
            .resolve(cred_of(&parts.headers).as_ref())
            .await
        {
            Ok(id) => Ok(Auth(id)),
            Err(Erreur::Unavailable) => Err(text_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication unavailable",
            )),
            Err(Erreur::Invalid) => Err(text_error(StatusCode::UNAUTHORIZED, "unauthorized")),
        }
    }
}

pub struct ActiveUser(pub User);

impl FromRequestParts<AppState> for ActiveUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        let Auth(idn) = Auth::from_request_parts(parts, state).await?;
        let pool = state.pool().map_err(DbIndisponible::into_response)?;
        let user = match users::user_by_sub(pool, &idn.sub).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                return Err(write_err(
                    StatusCode::FORBIDDEN,
                    "not_activated",
                    "compte non activé",
                ));
            }
            Err(_) => return Err(internal()),
        };
        if user.suspended {
            return Err(write_err(
                StatusCode::FORBIDDEN,
                "suspended",
                "compte suspendu",
            ));
        }
        verifier(state, &idn.sub).await?;
        Ok(ActiveUser(user))
    }
}

async fn verifier(state: &AppState, sub: &str) -> Result<(), Response> {
    match state.acces(sub).await {
        Err(_) => Err(permission_indisponible()),
        Ok(false) => Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "accès à Estompe non accordé",
        )),
        Ok(true) => Ok(()),
    }
}

fn permission_indisponible() -> Response {
    write_err(
        StatusCode::SERVICE_UNAVAILABLE,
        "permission_unavailable",
        "service de permission injoignable",
    )
}

#[utoipa::path(get, path = "/readyz", summary = "Readiness (base de données)", security(),
    responses(
        (status = 200, description = "ok", content_type = "text/plain", body = String),
        (status = 503, description = "Base injoignable"),
    ))]
async fn readyz(State(state): State<AppState>) -> Response {
    let pool = match state.pool() {
        Ok(pool) => pool,
        Err(e) => return e.into_response(),
    };
    match db::ping(pool).await {
        Ok(()) => "ok".into_response(),
        Err(_) => DbIndisponible.into_response(),
    }
}

pub(crate) fn jetons_egaux(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a), Sha256::digest(b));
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[utoipa::path(get, path = "/metrics", tag = "supervision",
    summary = "Métriques Prometheus (Bearer METRICS_TOKEN ; 404 si le jeton n'est pas configuré)",
    responses(
        (status = 200, description = "texte Prometheus", content_type = "text/plain", body = String),
        (status = 401, description = "jeton absent ou incorrect"),
    ))]
async fn metrics(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let attendu = &state.config().metrics_token;
    if attendu.is_empty() {
        return text_error(StatusCode::NOT_FOUND, "404 page not found");
    }
    let recu = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !jetons_egaux(recu.strip_prefix("Bearer ").unwrap_or(recu), attendu) {
        let mut r = text_error(StatusCode::UNAUTHORIZED, "unauthorized");
        r.headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return r;
    }
    let pool = state.pool().ok();
    let base_up = match pool {
        Some(p) => db::ping(p).await.is_ok(),
        None => false,
    };
    let (mut totaux, mut stockage) = (None, None);
    if let Some(p) = pool {
        match db::totaux(p).await {
            Ok((utilisateurs, messages, fichiers)) => {
                totaux = Some(Totaux {
                    utilisateurs,
                    messages,
                    fichiers,
                });
            }
            Err(e) => tracing::warn!("métriques : totaux : {e}"),
        }
        match db::octets_stockes(p).await {
            Ok(utilise) => {
                stockage = Some(Stockage {
                    utilise,
                    plafond: state.config().max_storage_bytes,
                });
            }
            Err(e) => tracing::warn!("métriques : stockage : {e}"),
        }
    }
    let corps = state.metrics().rendu(&Instantane {
        uptime: state.uptime(),
        base_up,
        totaux,
        stockage,
        ws: state.hub().counts(),
    });
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4"),
        )],
        corps,
    )
        .into_response()
}

#[derive(Serialize, ToSchema)]
#[allow(dead_code)]
struct MeReponse {
    allowed: bool,
    admin: bool,
    rights: Vec<String>,
    activated: bool,
    suspended: Option<bool>,
    cinema: Option<bool>,
    max_video_size: Option<u64>,
    #[schema(inline, value_type = Option<String>)]
    transcription: Option<Transcription>,
    blocks: Option<Vec<String>>,
    sub: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    pseudo: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[allow(dead_code)]
enum Transcription {
    #[serde(rename = "")]
    Aucune,
    #[serde(rename = "alpha")]
    Alpha,
    #[serde(rename = "on")]
    On,
}

#[utoipa::path(get, path = "/me", tag = "compte",
    summary = "État du compte Estompe de l'utilisateur connecté",
    responses(
        (status = 200, description = "`allowed: false` sans la permission d'utiliser Estompe ; `activated: false` tant que le pseudo n'est pas choisi ; `rights` : droits du compte (`access`, `admin`, `acces` = administrer aegis, `users` = rechercher des comptes)", body = MeReponse),
        (status = 401, description = "Identité absente ou invalide", body = openapi::Error),
        (status = 503, description = "`permission_unavailable` : service de permission injoignable", body = openapi::Error),
    ))]
async fn me(State(state): State<AppState>, Auth(idn): Auth) -> Response {
    let (Ok(droits), Ok(gere_acces)) = (state.droits_de(&idn).await, state.gere_acces(&idn).await)
    else {
        return permission_indisponible();
    };
    let config = state.config();
    let allowed = droits.a(catalogue::ACCES);
    let admin = catalogue::CATALOGUE
        .iter()
        .any(|e| e.admin() && droits.a(&e.nom()));
    let capacites = droits.json("estompe:");
    let rights: Vec<&str> = [
        ("access", allowed),
        ("admin", admin),
        ("acces", gere_acces),
        ("users", gere_acces),
    ]
    .into_iter()
    .filter_map(|(nom, ok)| ok.then_some(nom))
    .collect();
    let pool = match state.pool() {
        Ok(pool) => pool,
        Err(e) => return e.into_response(),
    };
    match users::user_by_sub(pool, &idn.sub).await {
        Ok(None) => write_json(
            StatusCode::OK,
            &json!({ "activated": false, "allowed": allowed, "admin": admin, "rights": rights, "capabilities": capacites }),
        ),
        Err(_) => internal(),
        Ok(Some(u)) => {
            let transcription = match (config.transcribe_url.is_empty(), config.transcribe_alpha) {
                (true, _) => "",
                (false, true) => "alpha",
                (false, false) => "on",
            };
            write_json(
                StatusCode::OK,
                &json!({
                    "activated": true,
                    "allowed": allowed,
                    "admin": admin,
                    "rights": rights,
                    "capabilities": capacites,
                    "suspended": u.suspended,
                    "id": u.id,
                    "pseudo": u.pseudo,
                    "sub": idn.sub,
                    "cinema": crate::servers::cinema_disponible(&state),
                    "max_video_size": config.max_video_bytes,
                    "transcription": transcription,
                    "blocks": state.blocs_actifs(),
                }),
            )
        }
    }
}

fn pseudo_valide(pseudo: &str) -> bool {
    (3..=20).contains(&pseudo.len())
        && pseudo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

#[derive(Deserialize, ToSchema)]
#[allow(dead_code)]
struct ActivateCorps {
    #[schema(pattern = "^[A-Za-z0-9_.-]{3,20}$")]
    pseudo: String,
}

#[utoipa::path(post, path = "/activate", tag = "compte",
    summary = "Créer le compte Estompe (pseudo), réservé aux détenteurs de la permission",
    request_body = ActivateCorps,
    responses(
        (status = 201, description = "Compte créé", body = openapi::Person),
        (status = 400, description = "`invalid_pseudo`", body = openapi::Error),
        (status = 403, description = "`forbidden` : permission non accordée", body = openapi::Error),
        (status = 409, description = "`pseudo_taken` ou `already_activated`", body = openapi::Error),
        (status = 503, description = "`permission_unavailable` : service de permission injoignable", body = openapi::Error),
    ))]
async fn activate(State(state): State<AppState>, Auth(idn): Auth, body: Body) -> Response {
    let entree = match decode(body).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(pseudo) = champ_texte(&entree, "pseudo") else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    };
    if !pseudo_valide(&pseudo) {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_pseudo",
            "pseudo : 3 à 20 caractères parmi lettres, chiffres, _ . -",
        );
    }
    if let Err(r) = verifier(&state, &idn.sub).await {
        return r;
    }
    let pool = match state.pool() {
        Ok(pool) => pool,
        Err(e) => return e.into_response(),
    };
    match users::activate(pool, &idn.sub, &pseudo).await {
        Ok(u) => write_json(
            StatusCode::CREATED,
            &json!({ "id": u.id, "pseudo": u.pseudo }),
        ),
        Err(ActivateError::PseudoTaken) => {
            write_err(StatusCode::CONFLICT, "pseudo_taken", "pseudo déjà pris")
        }
        Err(ActivateError::AlreadyActivated) => write_err(
            StatusCode::CONFLICT,
            "already_activated",
            "compte déjà activé",
        ),
        Err(ActivateError::Db(_)) => internal(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveUser, router, routes};
    use crate::{
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux},
    };
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
        routing::get,
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    struct Env {
        app: Router,
        state: AppState,
        _gateway: Option<Faux>,
        _permission: Faux,
        _pg: Option<BasePostgres>,
    }

    fn gateway(r: &testutil::Requete) -> (u16, Vec<u8>) {
        match r.entete("authorization").strip_prefix("Bearer ") {
            Some("down") => (502, vec![]),
            Some(nom) if nom != "bad" => (
                200,
                json!({
                    "id": format!("sub-{nom}"),
                    "name": nom,
                    "roles": if nom.ends_with("-kc") { vec!["aegis-admin"] } else { vec![] },
                })
                .to_string()
                .into_bytes(),
            ),
            _ => (401, vec![]),
        }
    }

    fn permission(r: &testutil::Requete) -> (u16, Vec<u8>) {
        if testutil::sub_aegis(r).as_deref() == Some("sub-boom") {
            return (500, vec![]);
        }
        testutil::reponse_droits(r, |sub, perm| match perm {
            "estompe:access" => !sub.starts_with("sub-nope"),
            "aegis:admin" => sub == "sub-perm",
            p if testutil::est_admin(p) => sub == "sub-admin",
            p => testutil::ouverte(p),
        })
    }

    async fn env(extra: &[(&str, &str)], avec_base: bool) -> Option<Env> {
        let g = faux(gateway).await;
        let p = faux(permission).await;
        let mut vars = vec![
            ("GATEWAY_URL", g.url.as_str()),
            ("AEGIS_URL", p.url.as_str()),
        ];
        vars.extend_from_slice(extra);
        let state = AppState::new(
            testutil::config_avec(&vars),
            vec!["gifs".into(), "vocal".into()],
        );
        let pg = if avec_base {
            let (c, url) = testutil::postgres().await?;
            let pool = sqlx::PgPool::connect(&url).await.unwrap();
            crate::db::migrate_embarquees(&pool).await.unwrap();
            state.set_pool(pool);
            Some(c)
        } else {
            None
        };
        let app = routes()
            .route(
                "/actif",
                get(|ActiveUser(u): ActiveUser| async move { u.pseudo }),
            )
            .with_state(state.clone());
        Some(Env {
            app,
            state,
            _gateway: Some(g),
            _permission: p,
            _pg: pg,
        })
    }

    async fn appel(
        app: &Router,
        methode: &str,
        chemin: &str,
        bearer: Option<&str>,
        corps: Option<String>,
    ) -> (StatusCode, HeaderMap, String) {
        let mut r = Request::builder().method(methode).uri(chemin);
        if let Some(b) = bearer {
            r = r.header(header::AUTHORIZATION, format!("Bearer {b}"));
        }
        let r = r.body(corps.map_or_else(Body::empty, Body::from)).unwrap();
        let rep = app.clone().oneshot(r).await.unwrap();
        let (statut, entetes) = (rep.status(), rep.headers().clone());
        let corps = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (statut, entetes, String::from_utf8(corps.to_vec()).unwrap())
    }

    async fn json_de(
        app: &Router,
        methode: &str,
        chemin: &str,
        bearer: &str,
        corps: Option<&str>,
    ) -> (StatusCode, Value) {
        let (s, h, c) = appel(app, methode, chemin, Some(bearer), corps.map(String::from)).await;
        assert_eq!(h[header::CONTENT_TYPE], "application/json", "{c}");
        assert_eq!(h[header::CACHE_CONTROL], "no-store");
        assert!(c.ends_with('\n'));
        let v: Value = serde_json::from_str(&c).unwrap();
        let spec = router().split_for_parts().1;
        testutil::conforme_openapi(&spec, methode, chemin, s.as_u16(), &v).unwrap();
        (s, v)
    }

    async fn activer(app: &Router, nom: &str, pseudo: &str) -> (StatusCode, Value) {
        json_de(
            app,
            "POST",
            "/activate",
            nom,
            Some(&json!({ "pseudo": pseudo }).to_string()),
        )
        .await
    }

    #[test]
    fn le_validateur_de_schema_refuse_un_ecart() {
        let spec = router().split_for_parts().1;
        let conforme =
            |statut, v: Value| testutil::conforme_openapi(&spec, "POST", "/activate", statut, &v);
        let ok = json!({ "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7", "pseudo": "alice" });
        assert_eq!(conforme(201, ok.clone()), Ok(()));
        assert!(conforme(201, json!({ "id": 3, "pseudo": "alice" })).is_err());
        assert!(conforme(418, ok).is_err());
        assert_eq!(
            conforme(409, json!({ "error": "x", "message": "y" })),
            Ok(())
        );
        assert!(conforme(409, json!({ "error": "x" })).is_err());
    }

    #[tokio::test]
    async fn healthz_sans_base() {
        let e = env(&[], false).await.unwrap();
        let (s, _, c) = appel(&e.app, "GET", "/healthz", None, None).await;
        assert_eq!((s, c.as_str()), (StatusCode::OK, "ok"));
    }

    #[tokio::test]
    async fn readyz_sans_base() {
        let e = env(&[], false).await.unwrap();
        let (s, h, c) = appel(&e.app, "GET", "/readyz", None, None).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(c, "database unavailable\n");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(h[header::CONTENT_TYPE], "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn readyz_base_prete() {
        let Some(e) = env(&[], true).await else {
            return;
        };
        let (s, _, c) = appel(&e.app, "GET", "/readyz", None, None).await;
        assert_eq!((s, c.as_str()), (StatusCode::OK, "ok"));
    }

    #[tokio::test]
    async fn authentification_refusee() {
        let e = env(&[], false).await.unwrap();
        for (chemin, methode) in [("/me", "GET"), ("/activate", "POST")] {
            let (s, h, c) = appel(&e.app, methode, chemin, None, None).await;
            assert_eq!(
                (s, c.as_str()),
                (StatusCode::UNAUTHORIZED, "unauthorized\n")
            );
            assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
            assert_eq!(h[header::CONTENT_TYPE], "text/plain; charset=utf-8");
            let (s, _, c) = appel(&e.app, methode, chemin, Some("bad"), None).await;
            assert_eq!(
                (s, c.as_str()),
                (StatusCode::UNAUTHORIZED, "unauthorized\n")
            );
            let (s, _, c) = appel(&e.app, methode, chemin, Some("down"), None).await;
            assert_eq!(
                (s, c.as_str()),
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "authentication unavailable\n"
                )
            );
        }
    }

    #[tokio::test]
    async fn gateway_arrete() {
        let mut e = env(&[], false).await.unwrap();
        e._gateway = None;
        let (s, _, c) = appel(&e.app, "GET", "/me", Some("alice"), None).await;
        assert_eq!(
            (s, c.as_str()),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication unavailable\n"
            )
        );
    }

    #[tokio::test]
    async fn me_selon_l_etat_du_compte() {
        let Some(e) = env(&[], true).await else {
            return;
        };
        let (s, mut v) = json_de(&e.app, "GET", "/me", "alice", None).await;
        assert_eq!(s, StatusCode::OK);
        let capacites = v.as_object_mut().unwrap().remove("capabilities").unwrap();
        assert_eq!(
            capacites["estompe:access"],
            json!({"allow": ["*"], "deny": []})
        );
        assert!(capacites.get("estompe:admin.users").is_none());
        assert!(capacites.get("aegis:admin").is_none());
        assert_eq!(
            v,
            json!({ "activated": false, "allowed": true, "admin": false, "rights": ["access"] })
        );
        let (s, v) = json_de(&e.app, "GET", "/me", "nope", None).await;
        assert_eq!((s, &v["allowed"]), (StatusCode::OK, &json!(false)));
        assert_eq!(v["rights"], json!([]));
        let (_, mut v) = json_de(&e.app, "GET", "/me", "nope-kc", None).await;
        v.as_object_mut().unwrap().remove("capabilities");
        assert_eq!(
            v,
            json!({ "activated": false, "allowed": false, "admin": false, "rights": ["acces", "users"] })
        );
        let (_, v) = json_de(&e.app, "GET", "/me", "perm", None).await;
        assert_eq!(v["rights"], json!(["access", "acces", "users"]));
        let (s, v) = json_de(&e.app, "GET", "/me", "boom", None).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            v,
            json!({ "error": "permission_unavailable", "message": "service de permission injoignable" })
        );

        assert_eq!(
            activer(&e.app, "alice", "Alice_1").await.0,
            StatusCode::CREATED
        );
        let (s, v) = json_de(&e.app, "GET", "/me", "alice", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["activated"], true);
        assert_eq!(v["allowed"], true);
        assert_eq!(v["admin"], false);
        assert_eq!(v["suspended"], false);
        assert_eq!(v["pseudo"], "Alice_1");
        assert_eq!(v["sub"], "sub-alice");
        let mut cles: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        cles.sort_unstable();
        assert_eq!(
            cles,
            [
                "activated",
                "admin",
                "allowed",
                "blocks",
                "capabilities",
                "cinema",
                "id",
                "max_video_size",
                "pseudo",
                "rights",
                "sub",
                "suspended",
                "transcription"
            ]
        );
        assert_eq!(v["max_video_size"], 100 << 20);
        assert_eq!(v["transcription"], "");
        assert_eq!(v["blocks"], json!(["gifs", "vocal"]));
        assert_eq!(v["id"].as_str().unwrap().len(), 36);

        sqlx::query("UPDATE users SET suspended_at = now() WHERE keycloak_sub = 'sub-alice'")
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        let (_, v) = json_de(&e.app, "GET", "/me", "alice", None).await;
        assert_eq!(v["suspended"], true);

        assert_eq!(
            activer(&e.app, "admin", "Admin").await.0,
            StatusCode::CREATED
        );
        let (_, v) = json_de(&e.app, "GET", "/me", "admin", None).await;
        assert_eq!(v["admin"], true);
        assert_eq!(v["rights"], json!(["access", "admin"]));
    }

    #[tokio::test]
    async fn me_transcription() {
        for (extra, attendu) in [
            (vec![("TRANSCRIBE_URL", "http://t")], "alpha"),
            (
                vec![
                    ("TRANSCRIBE_URL", "http://t"),
                    ("TRANSCRIBE_ALPHA", "false"),
                ],
                "on",
            ),
            (vec![("TRANSCRIBE_ALPHA", "false")], ""),
        ] {
            let Some(e) = env(&extra, true).await else {
                return;
            };
            activer(&e.app, "alice", "alice").await;
            let (_, v) = json_de(&e.app, "GET", "/me", "alice", None).await;
            assert_eq!(v["transcription"], attendu);
        }
        let Some(e) = env(&[("MAX_VIDEO_SIZE_MB", "7")], true).await else {
            return;
        };
        activer(&e.app, "alice", "alice").await;
        let (_, v) = json_de(&e.app, "GET", "/me", "alice", None).await;
        assert_eq!(v["max_video_size"], 7 << 20);
    }

    #[tokio::test]
    async fn activate_pseudo_invalide() {
        let Some(e) = env(&[], false).await else {
            return;
        };
        for pseudo in [
            "ab",
            "a".repeat(21).as_str(),
            "a b",
            "日本語x",
            "ab\n",
            "a@bcd",
            "",
        ] {
            let (s, v) = activer(&e.app, "alice", pseudo).await;
            assert_eq!(s, StatusCode::BAD_REQUEST, "{pseudo:?}");
            assert_eq!(v["error"], "invalid_pseudo");
            assert_eq!(
                v["message"],
                "pseudo : 3 à 20 caractères parmi lettres, chiffres, _ . -"
            );
        }
        for pseudo in ["abc", &"a".repeat(20), "A_b.c-9"] {
            assert!(super::pseudo_valide(pseudo), "{pseudo}");
        }
    }

    #[tokio::test]
    async fn activate_json_invalide() {
        let Some(e) = env(&[], false).await else {
            return;
        };
        for corps in [
            "pas du json".to_string(),
            "[]".into(),
            r#"{"pseudo":12}"#.into(),
            format!(r#"{{"pseudo":"{}"}}"#, "a".repeat(70_000)),
        ] {
            let (s, v) = json_de(&e.app, "POST", "/activate", "alice", Some(&corps)).await;
            assert_eq!(s, StatusCode::BAD_REQUEST);
            assert_eq!(
                v,
                json!({ "error": "bad_request", "message": "JSON invalide" })
            );
        }
    }

    #[tokio::test]
    async fn activate_premier_objet_seulement() {
        let Some(e) = env(&[], true).await else {
            return;
        };
        let (s, v) = json_de(
            &e.app,
            "POST",
            "/activate",
            "alice",
            Some(r#"{"PSEUDO":"abc"} {"x":1}"#),
        )
        .await;
        assert_eq!(
            (s, v["pseudo"].as_str()),
            (StatusCode::CREATED, Some("abc"))
        );
    }

    #[tokio::test]
    async fn activate_permission() {
        let Some(e) = env(&[], false).await else {
            return;
        };
        let (s, v) = activer(&e.app, "nope", "nopenope").await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::FORBIDDEN, Some("forbidden"))
        );
        assert_eq!(v["message"], "accès à Estompe non accordé");
        let (s, v) = activer(&e.app, "boom", "boomboom").await;
        assert_eq!(
            (s, v["error"].as_str()),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Some("permission_unavailable")
            )
        );
    }

    #[tokio::test]
    async fn activate_succes_et_conflits() {
        let Some(e) = env(&[], true).await else {
            return;
        };
        let (s, v) = activer(&e.app, "alice", "Alice").await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(v["pseudo"], "Alice");
        assert_eq!(v["id"].as_str().unwrap().len(), 36);
        let (s, v) = activer(&e.app, "bob", "ALICE").await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::CONFLICT, Some("pseudo_taken"))
        );
        assert_eq!(v["message"], "pseudo déjà pris");
        let (s, v) = activer(&e.app, "alice", "autre").await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::CONFLICT, Some("already_activated"))
        );
        assert_eq!(v["message"], "compte déjà activé");
    }

    #[tokio::test]
    async fn utilisateur_actif() {
        let Some(e) = env(&[], true).await else {
            return;
        };
        let (s, _, c) = appel(&e.app, "GET", "/actif", Some("alice"), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert!(c.contains("\"not_activated\""), "{c}");
        for nom in ["alice", "nope", "boom"] {
            let pool = e.state.pool().ok().unwrap();
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(pool)
                .await
                .unwrap();
        }
        let (s, _, c) = appel(&e.app, "GET", "/actif", Some("alice"), None).await;
        assert_eq!((s, c.as_str()), (StatusCode::OK, "alice"));
        let (s, _, c) = appel(&e.app, "GET", "/actif", Some("nope"), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert!(c.contains("\"forbidden\""), "{c}");
        let (s, _, c) = appel(&e.app, "GET", "/actif", Some("boom"), None).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert!(c.contains("\"permission_unavailable\""), "{c}");
        sqlx::query("UPDATE users SET suspended_at = now() WHERE keycloak_sub = 'sub-alice'")
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        let (s, _, c) = appel(&e.app, "GET", "/actif", Some("alice"), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert!(c.contains("\"suspended\""), "{c}");
        let (s, _, _) = appel(&e.app, "GET", "/actif", None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn base_en_erreur_donne_internal() {
        let Some(e) = env(&[], true).await else {
            return;
        };
        e.state.pool().ok().unwrap().close().await;
        let (s, _, c) = appel(&e.app, "GET", "/actif", Some("alice"), None).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(c.contains("\"internal\""), "{c}");
        let (s, _, c) = appel(&e.app, "GET", "/me", Some("alice"), None).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(c.contains("\"internal\""), "{c}");
        let (s, _, c) = appel(
            &e.app,
            "POST",
            "/activate",
            Some("alice"),
            Some(r#"{"pseudo":"alice"}"#.into()),
        )
        .await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(c.contains("\"internal\""), "{c}");
    }

    #[tokio::test]
    async fn me_sans_base() {
        let e = env(&[], false).await.unwrap();
        let (s, _, c) = appel(&e.app, "GET", "/me", Some("alice"), None).await;
        assert_eq!(
            (s, c.as_str()),
            (StatusCode::SERVICE_UNAVAILABLE, "database unavailable\n")
        );
    }
}
