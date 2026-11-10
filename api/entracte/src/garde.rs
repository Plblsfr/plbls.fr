use axum::{
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};

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
            if !jetons_egaux(attendu, &parametre(&req, "token")) {
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
        None => r,
    }
}
