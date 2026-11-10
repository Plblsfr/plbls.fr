use std::{sync::LazyLock, time::Duration};

use axum::{
    body::to_bytes,
    extract::{FromRequestParts, Request, State},
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::any,
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

const PREFIXE: &str = "/admin/acces";
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
                "réservé aux administrateurs des accès",
            )),
            Err(_) => Err(indisponible()),
        }
    }
}

pub const ROUTES: &[(&str, &[HttpMethod], &str)] = &[
    (
        "/catalog",
        &[HttpMethod::Get],
        "Catalogue : applications, types de ressource et permissions déclarés",
    ),
    (
        "/resources/{app}/{type}",
        &[HttpMethod::Get],
        "Instances d'un type de ressource (serveurs…)",
    ),
    (
        "/permissions/{name}/holders",
        &[HttpMethod::Get],
        "Rôles, groupes et comptes qui mentionnent une permission",
    ),
    (
        "/permissions/purge-obsolete",
        &[HttpMethod::Post],
        "Supprimer les permissions obsolètes",
    ),
    (
        "/accounts",
        &[HttpMethod::Get],
        "Annuaire des comptes (recherche par e-mail, nom ou sub)",
    ),
    (
        "/accounts/{sub}",
        &[HttpMethod::Get],
        "Fiche d'un compte : rôles, groupes, règles directes",
    ),
    (
        "/accounts/{sub}/effective",
        &[HttpMethod::Get],
        "Droits effectifs expliqués d'un compte",
    ),
    (
        "/accounts/{sub}/copy",
        &[HttpMethod::Post],
        "Copier les droits d'un autre compte",
    ),
    (
        "/accounts/{sub}/grants",
        &[HttpMethod::Delete],
        "Retirer tous les droits d'un compte",
    ),
    (
        "/simulate",
        &[HttpMethod::Post],
        "Simuler une décision (compte, permission, ressource)",
    ),
    ("/roles", &[HttpMethod::Get, HttpMethod::Post], "Rôles"),
    (
        "/roles/{id}",
        &[HttpMethod::Get, HttpMethod::Put, HttpMethod::Delete],
        "Un rôle et ses règles",
    ),
    (
        "/groups",
        &[HttpMethod::Get, HttpMethod::Post],
        "Groupes de comptes",
    ),
    (
        "/groups/{id}",
        &[HttpMethod::Get, HttpMethod::Put, HttpMethod::Delete],
        "Un groupe",
    ),
    (
        "/groups/{id}/members/{sub}",
        &[HttpMethod::Put, HttpMethod::Delete],
        "Membre d'un groupe",
    ),
    (
        "/bindings",
        &[HttpMethod::Get, HttpMethod::Post],
        "Liaisons rôle → compte ou groupe",
    ),
    (
        "/bindings/{id}",
        &[HttpMethod::Delete],
        "Retirer une liaison",
    ),
    (
        "/rules",
        &[HttpMethod::Get, HttpMethod::Post],
        "Règles directes (accorder ou refuser)",
    ),
    (
        "/rules/{id}",
        &[HttpMethod::Put, HttpMethod::Delete],
        "Une règle directe",
    ),
    ("/audit", &[HttpMethod::Get], "Journal des modifications"),
];

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new();
    for (chemin, methodes, resume) in ROUTES {
        let complet = format!("{PREFIXE}{chemin}");
        routeur = routeur.route(&complet, any(relayer));
        for m in *methodes {
            let op = openapi::operation(
                "administration",
                &format!(
                    "{resume}. Relayé vers `/v1/admin{chemin}` d'aegis avec le justificatif de l'appelant"
                ),
                REPONSES,
                &complet,
            );
            routeur = openapi::documenter(routeur, &complet, m.clone(), op);
        }
    }
    routeur
}

const REPONSES: &[(&str, &str)] = &[
    ("200", "Réponse d'aegis"),
    ("400", "Requête refusée par aegis"),
    ("401", "Non authentifié"),
    ("403", "Réservé aux administrateurs des accès"),
    ("404", "Introuvable"),
    ("409", "Conflit"),
    (
        "502",
        "`permission_unavailable` : aegis injoignable depuis le gateway",
    ),
    ("503", "`permission_unavailable`"),
];

fn segment_valide(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= 200
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'@' | b'%')
        })
}

fn indisponible() -> Response {
    write_err(
        StatusCode::SERVICE_UNAVAILABLE,
        "permission_unavailable",
        "aegis injoignable",
    )
}

fn decrire(statut: StatusCode) -> &'static str {
    match statut.as_u16() {
        400 => "requête refusée par le gateway",
        401 => "session absente ou expirée",
        403 => "accès refusé par aegis",
        404 => "route /api/aegis introuvable sur le gateway",
        502 => "aegis injoignable depuis le gateway",
        503 => "authentification indisponible",
        504 => "aegis ne répond pas",
        _ => "erreur d'aegis",
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
    if reste.contains("%2") || reste.contains("%5") || !reste.split('/').skip(1).all(segment_valide)
    {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "chemin invalide");
    }
    let mut url = format!(
        "{}/api/aegis/v1/admin{reste}",
        state.config().gateway_url.trim_end_matches('/')
    );
    if let Some(q) = parts.uri.query() {
        url.push('?');
        url.push_str(q);
    }
    transmettre(parts, corps, url).await
}

async fn transmettre(
    parts: axum::http::request::Parts,
    corps: axum::body::Body,
    url: String,
) -> Response {
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
    use crate::testutil::{self, Faux, Requete, faux, faux_aegis};
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
        _aegis: Faux,
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
                vec!["aegis-admin"]
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
            "/api/aegis/v1/admin/roles" if r.methode == "GET" => {
                (200, br#"[{"id":"r1","name":"Moderation"}]"#.to_vec())
            }
            "/api/aegis/v1/admin/roles" => (409, r#"{"error":"existe déjà"}"#.as_bytes().to_vec()),
            "/api/aegis/v1/admin/bindings/b1" if r.methode == "DELETE" => (204, vec![]),
            _ => (502, vec![]),
        }
    }

    async fn env() -> Env {
        let g = faux(gateway).await;
        let a = faux_aegis(|sub, perm| perm == crate::catalogue::ADMIN_AEGIS && sub == "sub-admin")
            .await;
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", g.url.as_str()),
                ("AEGIS_URL", a.url.as_str()),
            ]),
            vec![],
        );
        Env {
            app: crate::api::routes().with_state(state),
            gateway: g,
            _aegis: a,
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
            .filter(|r| r.chemin.starts_with("/api/aegis/"))
            .collect()
    }

    #[test]
    fn segments() {
        for ok in ["accounts", "estompe:gifs.use", "a1b2-c3", "simulate"] {
            assert!(segment_valide(ok), "{ok}");
        }
        for ko in ["", ".", "..", "a b", "a?b", "a#b"] {
            assert!(!segment_valide(ko), "{ko}");
        }
    }

    #[tokio::test]
    async fn relaie_vers_le_gateway_avec_le_justificatif() {
        let e = env().await;
        let (s, h, c) = appel(
            &e,
            "GET",
            "/admin/acces/roles",
            &[("authorization", "Bearer admin")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{c}");
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
        assert_eq!(serde_json::from_str::<Value>(&c).unwrap()[0]["id"], "r1");
        let r = &requetes_relayees(&e)[0];
        assert_eq!(r.entete("authorization"), "Bearer admin");

        let (s, _, _) = appel(
            &e,
            "DELETE",
            "/admin/acces/bindings/b1?motif=x",
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
        assert_eq!(r.query, "motif=x");
        assert_eq!(r.entete("cookie"), format!("{COOKIE_NAME}=kc"));
        assert_eq!(r.entete("origin"), "https://estompe.test");
    }

    #[tokio::test]
    async fn erreurs_au_format_d_estompe() {
        let e = env().await;
        let (s, _, c) = appel(
            &e,
            "POST",
            "/admin/acces/roles",
            &[("authorization", "Bearer admin")],
            r#"{"name":"X"}"#,
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(
            serde_json::from_str::<Value>(&c).unwrap(),
            json!({ "error": "conflict", "message": "existe déjà" })
        );
        assert_eq!(requetes_relayees(&e)[0].corps, br#"{"name":"X"}"#);
        let (s, _, c) = appel(
            &e,
            "GET",
            "/admin/acces/audit",
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
    async fn reserve_aux_administrateurs_des_acces() {
        let e = env().await;
        let (s, _, c) = appel(
            &e,
            "GET",
            "/admin/acces/roles",
            &[("authorization", "Bearer alice")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{c}");
        let (s, _, _) = appel(&e, "GET", "/admin/acces/roles", &[], "").await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert!(requetes_relayees(&e).is_empty());
    }

    #[tokio::test]
    async fn chemin_echappe_refuse() {
        let e = env().await;
        let (s, _, _) = appel(
            &e,
            "DELETE",
            "/admin/acces/roles/a%2F..%2Fb",
            &[("authorization", "Bearer admin")],
            "",
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(requetes_relayees(&e).is_empty());
    }
}
