use axum::{
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    droits::{Droit, Droits},
    etat::AppState,
    http::{erreur, jetons_egaux},
    identite::{Identite, Refus, justificatif},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Garde {
    Droit(Droit),
    Session(&'static str),
    Jeton(&'static str),
    Service(&'static str),
    Webhook(&'static str),
    Public(&'static str),
}

impl Garde {
    pub fn motif(self) -> Option<&'static str> {
        match self {
            Garde::Droit(_) => None,
            Garde::Session(m)
            | Garde::Jeton(m)
            | Garde::Service(m)
            | Garde::Webhook(m)
            | Garde::Public(m) => Some(m),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Personne {
    pub identite: Identite,
    pub droits: Droits,
}

impl Personne {
    pub fn sub(&self) -> &str {
        &self.identite.sub
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Personne {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Response> {
        parts
            .extensions
            .get::<Personne>()
            .cloned()
            .ok_or_else(crate::http::interne)
    }
}

pub fn non_connecte() -> Response {
    erreur(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "connexion requise",
    )
}

pub fn interdit(d: Droit) -> Response {
    erreur(
        StatusCode::FORBIDDEN,
        "forbidden",
        &format!("permission {} requise", d.nom()),
    )
}

pub async fn identifier(state: &AppState, entetes: &HeaderMap) -> Result<Identite, Response> {
    let j = justificatif(entetes);
    state
        .identites()
        .resoudre(j.as_ref())
        .await
        .map_err(|r| match r {
            Refus::Absente | Refus::Invalide => non_connecte(),
            Refus::Indisponible => erreur(
                StatusCode::BAD_GATEWAY,
                "gateway_unavailable",
                "service de connexion injoignable",
            ),
        })
}

pub async fn personne(
    state: &AppState,
    entetes: &HeaderMap,
    strict: bool,
) -> Result<Personne, Response> {
    let identite = identifier(state, entetes).await?;
    let droits = match state.droits_de(&identite).await {
        Ok(d) => d,
        Err(_) if !strict => Droits::default(),
        Err(_) => {
            return Err(erreur(
                StatusCode::SERVICE_UNAVAILABLE,
                "aegis_unavailable",
                "service de droits injoignable",
            ));
        }
    };
    Ok(Personne { identite, droits })
}

fn bearer(entetes: &HeaderMap) -> &str {
    entetes
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
}

fn parametre(req: &Request, nom: &str) -> String {
    req.uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| *k == nom)
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}

fn jetons_webhook(req: &Request) -> Vec<String> {
    let mut jetons = vec![parametre(req, "token")];
    let autorisation = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if let Some((schema, valeur)) = autorisation.split_once(' ') {
        if schema.eq_ignore_ascii_case("bearer") {
            jetons.push(valeur.trim().to_string());
        } else if schema.eq_ignore_ascii_case("basic")
            && let Some(mdp) = STANDARD
                .decode(valeur.trim())
                .ok()
                .and_then(|o| String::from_utf8(o).ok())
                .and_then(|t| t.split_once(':').map(|(_, m)| m.to_string()))
        {
            jetons.push(mdp);
        }
    }
    jetons
}

pub async fn garder(
    State((state, garde)): State<(AppState, Garde)>,
    mut req: Request,
    next: Next,
) -> Response {
    match garde {
        Garde::Droit(d) => match personne(&state, req.headers(), true).await {
            Ok(p) if p.droits.a(d) => {
                req.extensions_mut().insert(p);
            }
            Ok(_) => return interdit(d),
            Err(r) => return r,
        },
        Garde::Session(_) => match personne(&state, req.headers(), false).await {
            Ok(p) => {
                req.extensions_mut().insert(p);
            }
            Err(r) => return r,
        },
        Garde::Service(_) => {
            let attendu = &state.config().jeton_service;
            if attendu.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
            }
            if !jetons_egaux(attendu, bearer(req.headers())) {
                return erreur(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "jeton de service attendu",
                );
            }
        }
        Garde::Webhook(_) => {
            let attendu = &state.config().jeton_webhook;
            if attendu.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
            }
            if !jetons_webhook(&req)
                .iter()
                .any(|j| jetons_egaux(attendu, j))
            {
                return erreur(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "jeton de webhook attendu",
                );
            }
        }
        Garde::Jeton(_) | Garde::Public(_) => {}
    }
    next.run(req).await
}

fn avec_cors(mut r: Response, origine: &HeaderValue) -> Response {
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origine.clone());
    h.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    h.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Content-Range, Content-Length, Accept-Ranges"),
    );
    h.append(header::VARY, HeaderValue::from_static("Origin"));
    r
}

fn avec_cors_de_flux(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Content-Range, Content-Length, Accept-Ranges"),
    );
    r
}

fn a_un_cookie(h: &HeaderMap) -> bool {
    h.get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.split(';').any(|c| c.trim().contains('=')))
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
    let autorisee = !texte.is_empty() && state.config().origines.iter().any(|o| o == texte);
    let flux_ouvert = !autorisee
        && origine.is_some()
        && matches!(*req.method(), Method::GET | Method::OPTIONS)
        && req.uri().path().starts_with("/stream/");
    if flux_ouvert && req.method() == Method::OPTIONS {
        let mut r = avec_cors_de_flux(StatusCode::NO_CONTENT.into_response());
        let h = r.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Range"),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        return r;
    }
    let cors = origine.filter(|_| autorisee);
    if let Some(o) = &cors
        && req.method() == Method::OPTIONS
    {
        let mut r = avec_cors(StatusCode::NO_CONTENT.into_response(), o);
        let h = r.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, PUT, DELETE"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Range, X-Estompe-Version, X-Estompe-Platform",
            ),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        return r;
    }
    let modifiante = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let r = if modifiante && a_un_cookie(req.headers()) && !autorisee {
        erreur(StatusCode::FORBIDDEN, "bad_origin", "origine non autorisée")
    } else {
        next.run(req).await
    };
    match &cors {
        Some(o) => avec_cors(r, o),
        None if flux_ouvert => avec_cors_de_flux(r),
        None => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requete(uri: &str, autorisation: Option<&str>) -> Request {
        let mut b = Request::builder().uri(uri);
        if let Some(a) = autorisation {
            b = b.header(header::AUTHORIZATION, a);
        }
        b.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn jeton_de_webhook_par_requete_bearer_ou_basic() {
        let ok = |r: Request| jetons_webhook(&r).iter().any(|j| jetons_egaux("secret", j));
        assert!(ok(requete("/webhooks/radarr?token=secret", None)));
        assert!(ok(requete("/webhooks/radarr", Some("Bearer secret"))));
        let basic = format!("Basic {}", STANDARD.encode("radarr:secret"));
        assert!(ok(requete("/webhooks/radarr", Some(&basic))));
        let faux = format!("Basic {}", STANDARD.encode("secret:faux"));
        assert!(!ok(requete("/webhooks/radarr", Some(&faux))));
        assert!(!ok(requete("/webhooks/radarr", Some("Basic %%%"))));
        assert!(!ok(requete(
            "/webhooks/radarr?token=faux",
            Some("Bearer faux")
        )));
        assert!(!ok(requete("/webhooks/radarr", None)));
    }
}
