use std::{sync::LazyLock, time::Duration};

use axum::{
    body::to_bytes,
    extract::{FromRequestParts, Request, State},
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde_json::Value;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::Auth,
    http::{CORPS_MAX, champ, premier_objet, write_err},
    identity::{COOKIE_NAME, Identity, cred_of},
    openapi,
    state::AppState,
};

const PREFIXE: &str = "/admin/permissions";
const REPONSE_MAX: usize = 4 << 20;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client HTTP")
});

pub struct AdminPermissions(pub Identity);

impl FromRequestParts<AppState> for AdminPermissions {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        let Auth(idn) = Auth::from_request_parts(parts, state).await?;
        match state.gere_permissions(&idn).await {
            Ok(true) => Ok(AdminPermissions(idn)),
            Ok(false) => Err(write_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "réservé aux administrateurs des permissions",
            )),
            Err(_) => Err(indisponible()),
        }
    }
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/admin/permissions/apps", get(relayer).post(relayer))
        .route(
            "/admin/permissions/apps/{app}",
            axum::routing::put(relayer).delete(relayer),
        )
        .route(
            "/admin/permissions/apps/{app}/permissions",
            get(relayer).post(relayer),
        )
        .route(
            "/admin/permissions/apps/{app}/permissions/{key}",
            delete(relayer),
        )
        .route(
            "/admin/permissions/apps/{app}/roles",
            get(relayer).post(relayer),
        )
        .route(
            "/admin/permissions/apps/{app}/roles/{role}",
            axum::routing::put(relayer).delete(relayer),
        )
        .route(
            "/admin/permissions/assignments",
            get(relayer).post(relayer).delete(relayer),
        )
        .route("/admin/permissions/audit", get(relayer))
        .route("/admin/permissions/users", get(relayer));
    for (methode, chemin, resume) in DOC {
        let op = openapi::operation("administration", resume, REPONSES, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

const REPONSES: &[(&str, &str)] = &[
    ("200", "Réponse de permission"),
    ("400", "Requête refusée par permission"),
    ("401", "Non authentifié"),
    ("403", "Réservé aux administrateurs des permissions"),
    ("404", "Introuvable"),
    ("409", "Conflit (clé existante)"),
    (
        "502",
        "`permission_unavailable` : permission injoignable depuis le gateway",
    ),
    ("503", "`permission_unavailable`"),
];

const DOC: &[(HttpMethod, &str, &str)] = &[
    (
        HttpMethod::Get,
        "/admin/permissions/apps",
        "Applications de permission",
    ),
    (
        HttpMethod::Post,
        "/admin/permissions/apps",
        "Créer une application",
    ),
    (
        HttpMethod::Put,
        "/admin/permissions/apps/{app}",
        "Modifier une application",
    ),
    (
        HttpMethod::Delete,
        "/admin/permissions/apps/{app}",
        "Supprimer une application",
    ),
    (
        HttpMethod::Get,
        "/admin/permissions/apps/{app}/permissions",
        "Permissions d'une application",
    ),
    (
        HttpMethod::Post,
        "/admin/permissions/apps/{app}/permissions",
        "Déclarer une permission",
    ),
    (
        HttpMethod::Delete,
        "/admin/permissions/apps/{app}/permissions/{key}",
        "Supprimer une permission",
    ),
    (
        HttpMethod::Get,
        "/admin/permissions/apps/{app}/roles",
        "Rôles d'une application",
    ),
    (
        HttpMethod::Post,
        "/admin/permissions/apps/{app}/roles",
        "Créer un rôle",
    ),
    (
        HttpMethod::Put,
        "/admin/permissions/apps/{app}/roles/{role}",
        "Modifier un rôle",
    ),
    (
        HttpMethod::Delete,
        "/admin/permissions/apps/{app}/roles/{role}",
        "Supprimer un rôle",
    ),
    (
        HttpMethod::Get,
        "/admin/permissions/assignments",
        "Attributions (`user_id` pour filtrer)",
    ),
    (
        HttpMethod::Post,
        "/admin/permissions/assignments",
        "Attribuer un rôle",
    ),
    (
        HttpMethod::Delete,
        "/admin/permissions/assignments",
        "Retirer un rôle (`user_id`, `app`, `role`)",
    ),
    (
        HttpMethod::Get,
        "/admin/permissions/audit",
        "Journal des modifications de permission",
    ),
    (
        HttpMethod::Get,
        "/admin/permissions/users",
        "Recherche de comptes par début d'e-mail (permission `permission:users.read`)",
    ),
];

fn indisponible() -> Response {
    write_err(
        StatusCode::SERVICE_UNAVAILABLE,
        "permission_unavailable",
        "service de permission injoignable",
    )
}

fn segment_valide(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn decrire(statut: StatusCode) -> &'static str {
    match statut.as_u16() {
        400 => "requête refusée par le gateway",
        401 => "session absente ou expirée",
        403 => "accès refusé par permission",
        404 => "route /api/permission introuvable sur le gateway",
        502 => "service permission injoignable depuis le gateway",
        503 => "authentification indisponible",
        504 => "le service permission ne répond pas",
        _ => "erreur du service permission",
    }
}

fn code(statut: StatusCode) -> &'static str {
    match statut.as_u16() {
        400 => "bad_request",
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        409 => "conflict",
        502..=504 => "permission_unavailable",
        _ => "permission_error",
    }
}

async fn relayer(
    State(state): State<AppState>,
    AdminPermissions(_): AdminPermissions,
    req: Request,
) -> Response {
    let (parts, corps) = req.into_parts();
    let reste = parts
        .uri
        .path()
        .strip_prefix(PREFIXE)
        .unwrap_or_default()
        .to_string();
    if !reste.split('/').skip(1).all(segment_valide) {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "chemin invalide");
    }
    let Some(cred) = cred_of(&parts.headers) else {
        return write_err(StatusCode::UNAUTHORIZED, "unauthorized", "non authentifié");
    };
    let Ok(corps) = to_bytes(corps, CORPS_MAX).await else {
        return write_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "corps trop volumineux",
        );
    };
    let mut url = format!(
        "{}/api/permission/v1{reste}",
        state.config().gateway_url.trim_end_matches('/')
    );
    if let Some(q) = parts.uri.query() {
        url.push('?');
        url.push_str(q);
    }
    let mut requete = CLIENT.request(parts.method.clone(), &url);
    requete = if cred.cookie {
        requete.header(header::COOKIE, format!("{COOKIE_NAME}={}", cred.value))
    } else {
        requete.header(header::AUTHORIZATION, cred.value)
    };
    if let Some(o) = parts.headers.get(header::ORIGIN) {
        requete = requete.header(header::ORIGIN, o.clone());
    }
    if !corps.is_empty() {
        requete = requete
            .header(header::CONTENT_TYPE, "application/json")
            .body(corps);
    }
    let Ok(mut reponse) = requete.send().await else {
        return indisponible();
    };
    let statut = StatusCode::from_u16(reponse.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut octets = Vec::new();
    loop {
        match reponse.chunk().await {
            Ok(Some(m)) => {
                octets.extend_from_slice(&m);
                if octets.len() > REPONSE_MAX {
                    return indisponible();
                }
            }
            Ok(None) => break,
            Err(_) => return indisponible(),
        }
    }
    if !statut.is_success() {
        let message = premier_objet(&octets)
            .and_then(|o| match champ(&o, "error") {
                Some(Value::String(m)) if !m.is_empty() => Some(m.clone()),
                _ => None,
            })
            .unwrap_or_else(|| decrire(statut).to_string());
        return write_err(statut, code(statut), &message);
    }
    if statut == StatusCode::NO_CONTENT || octets.is_empty() {
        return statut.into_response();
    }
    (
        statut,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        octets,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, Faux, Requete, faux, faux_permission};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    struct Env {
        app: Router,
        gateway: Faux,
        _permission: Faux,
    }

    fn gateway(r: &Requete) -> (u16, Vec<u8>) {
        let nom = r
            .entete("authorization")
            .strip_prefix("Bearer ")
            .map(str::to_string)
            .or_else(|| {
                r.entete("cookie")
                    .strip_prefix(&format!("{COOKIE_NAME}="))
                    .map(str::to_string)
            });
        let Some(nom) = nom else {
            return (401, vec![]);
        };
        if r.chemin == "/auth/me" {
            let roles: Vec<&str> = if nom == "kc" {
                vec!["permission-admin"]
            } else {
                vec![]
            };
            return (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom, "roles": roles })
                    .to_string()
                    .into_bytes(),
            );
        }
        match r.chemin.as_str() {
            "/api/permission/v1/apps" if r.methode == "GET" => {
                (200, br#"[{"key":"estompe","name":"Estompe"}]"#.to_vec())
            }
            "/api/permission/v1/apps" => (409, r#"{"error":"existe déjà"}"#.as_bytes().to_vec()),
            "/api/permission/v1/assignments" if r.methode == "DELETE" => (204, vec![]),
            _ => (502, vec![]),
        }
    }

    async fn env() -> Env {
        let g = faux(gateway).await;
        let p = faux_permission(|sub, perm| perm == "permission:admin" && sub == "sub-admin").await;
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", g.url.as_str()),
                ("PERMISSION_URL", p.url.as_str()),
            ]),
            vec![],
        );
        Env {
            app: crate::api::routes().with_state(state),
            gateway: g,
            _permission: p,
        }
    }

    async fn appel(
        e: &Env,
        methode: &str,
        chemin: &str,
        entetes: &[(&str, &str)],
        corps: &str,
    ) -> (StatusCode, HeaderMap, String) {
        let mut r = Request::builder().method(methode).uri(chemin);
        for (k, v) in entetes {
            r = r.header(*k, *v);
        }
        let rep = e
            .app
            .clone()
            .oneshot(r.body(Body::from(corps.to_string())).unwrap())
            .await
            .unwrap();
        let (s, h) = (rep.status(), rep.headers().clone());
        let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (s, h, String::from_utf8(c.to_vec()).unwrap())
    }

    fn requetes_relayees(e: &Env) -> Vec<Requete> {
        e.gateway
            .requetes()
            .into_iter()
            .filter(|r| r.chemin.starts_with("/api/permission/"))
            .collect()
    }

    #[tokio::test]
    async fn relaie_vers_le_gateway_avec_le_justificatif() {
        let e = env().await;
        let (s, h, c) = appel(
            &e,
            "GET",
            "/admin/permissions/apps",
            &[("authorization", "Bearer admin")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{c}");
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
        assert_eq!(
            serde_json::from_str::<Value>(&c).unwrap()[0]["key"],
            "estompe"
        );
        let r = &requetes_relayees(&e)[0];
        assert_eq!(r.entete("authorization"), "Bearer admin");

        let (s, _, _) = appel(
            &e,
            "DELETE",
            "/admin/permissions/assignments?user_id=u&app=a&role=r",
            &[
                ("cookie", &format!("{COOKIE_NAME}=kc")),
                ("origin", "https://estompe.test"),
            ],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let r = requetes_relayees(&e).pop().unwrap();
        assert_eq!(r.methode, "DELETE");
        assert_eq!(r.query, "user_id=u&app=a&role=r");
        assert_eq!(r.entete("cookie"), format!("{COOKIE_NAME}=kc"));
        assert_eq!(r.entete("origin"), "https://estompe.test");
    }

    #[tokio::test]
    async fn erreurs_au_format_d_estompe() {
        let e = env().await;
        let (s, _, c) = appel(
            &e,
            "POST",
            "/admin/permissions/apps",
            &[("authorization", "Bearer admin")],
            r#"{"key":"x","name":"X"}"#,
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(
            serde_json::from_str::<Value>(&c).unwrap(),
            json!({ "error": "conflict", "message": "existe déjà" })
        );
        assert_eq!(requetes_relayees(&e)[0].corps, br#"{"key":"x","name":"X"}"#);
        let (s, _, c) = appel(
            &e,
            "GET",
            "/admin/permissions/audit",
            &[("authorization", "Bearer admin")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::BAD_GATEWAY);
        assert_eq!(
            serde_json::from_str::<Value>(&c).unwrap()["error"],
            "permission_unavailable"
        );
    }

    #[tokio::test]
    async fn reserve_aux_administrateurs_des_permissions() {
        let e = env().await;
        let (s, _, c) = appel(
            &e,
            "GET",
            "/admin/permissions/apps",
            &[("authorization", "Bearer alice")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{c}");
        let (s, _, _) = appel(&e, "GET", "/admin/permissions/apps", &[], "").await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert!(requetes_relayees(&e).is_empty());
    }

    #[tokio::test]
    async fn chemin_echappe_refuse() {
        let e = env().await;
        let (s, _, _) = appel(
            &e,
            "DELETE",
            "/admin/permissions/apps/a%2F..%2Fb",
            &[("authorization", "Bearer admin")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(requetes_relayees(&e).is_empty());
        assert!(!segment_valide(".."));
        assert!(segment_valide("message.delete"));
    }
}
