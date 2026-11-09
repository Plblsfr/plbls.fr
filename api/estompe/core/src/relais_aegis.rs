use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::Response,
    routing::any,
};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    http::write_err,
    openapi,
    relais_permission::{AdminPermissions, transmettre},
    state::AppState,
};

const PREFIXE: &str = "/admin/acces";

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

async fn relayer(
    State(state): State<AppState>,
    AdminPermissions(_): AdminPermissions,
    req: Request,
) -> Response {
    if !state.permissions().aegis_actif() {
        return write_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "aegis n'est pas configuré",
        );
    }
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
