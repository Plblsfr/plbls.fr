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

pub struct AdminAcces(pub Identity);

impl FromRequestParts<AppState> for AdminAcces {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        let Auth(idn) = Auth::from_request_parts(parts, state).await?;
        match state.gere_acces(&idn).await {
            Ok(true) => Ok(AdminAcces(idn)),
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

fn indisponible() -> Response {
    write_err(
        StatusCode::SERVICE_UNAVAILABLE,
        "permission_unavailable",
        "aegis injoignable",
    )
}

fn segment_valide(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= 200
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'@' | b'%')
        })
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
    AdminAcces(_): AdminAcces,
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
    use super::segment_valide;

    #[test]
    fn segments() {
        for ok in ["accounts", "estompe:gifs.use", "a1b2-c3", "simulate"] {
            assert!(segment_valide(ok), "{ok}");
        }
        for ko in ["", ".", "..", "a b", "a?b", "a#b"] {
            assert!(!segment_valide(ko), "{ko}");
        }
    }
}
