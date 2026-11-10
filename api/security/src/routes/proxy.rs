use axum::{
    Router,
    body::Body,
    extract::{
        FromRequest, Request, State,
        ws::{self, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use crate::AppState;
use crate::auth::{self, AuthError, Authn};
use crate::identity::{self, Identity};
use crate::session;
use crate::settings::Route;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/{*rest}", any(proxy))
}

/// En-têtes propres à une connexion, jamais transmis tels quels.
const HOP_BY_HOP: [&str; 9] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// En-têtes d'identité : toujours retirés de la requête entrante avant injection.
const IDENTITY_HEADERS: [&str; 8] = [
    "x-user-id",
    "x-user-name",
    "x-user-email",
    "x-user-roles",
    "x-client-id",
    "x-identity-ts",
    "x-identity-sig",
    "x-request-id",
];

/// Chemin qui sortirait de la route une fois normalisé par l'upstream : segment `.` ou `..` (même encodé),
/// séparateur encodé (`%2f`, `%5c`) ou antislash.
fn unsafe_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("%2f")
        || lower.contains("%5c")
        || lower.contains('\\')
        || lower
            .split('/')
            .any(|s| matches!(s.replace("%2e", ".").as_str(), "." | ".."))
}

fn find_route<'a>(routes: &'a [Route], path: &str) -> Option<&'a Route> {
    routes
        .iter()
        .filter(|r| {
            let p = r.prefix.trim_end_matches('/');
            path == p
                || path
                    .strip_prefix(p)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|r| r.prefix.len())
}

fn upstream_url(route: &Route, path: &str, query: Option<&str>) -> String {
    let rest = if route.strip_prefix {
        path.strip_prefix(route.prefix.trim_end_matches('/'))
            .unwrap_or(path)
    } else {
        path
    };
    let rest = if rest.is_empty() { "/" } else { rest };
    let mut url = format!("{}{}", route.upstream.trim_end_matches('/'), rest);
    if let Some(q) = query {
        url.push('?');
        url.push_str(q);
    }
    url
}

fn propre(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn hv(s: &str) -> HeaderValue {
    // les valeurs viennent de claims signés ; on neutralise tout caractère de contrôle
    HeaderValue::from_str(&propre(s)).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn inject(h: &mut HeaderMap, secret: &str, id: &Identity, request_id: &str) {
    let ts = session::now();
    let (uid, roles, client) = (
        propre(&id.id),
        propre(&id.roles.join(",")),
        propre(&id.client),
    );
    let (name, email) = (identity::encoder(&id.name), identity::encoder(&id.email));
    let sig = identity::sign(
        secret,
        &identity::Champs {
            id: &uid,
            roles: &roles,
            client: &client,
            ts,
            name: &name,
            email: &email,
        },
    );
    let pairs = [
        ("x-user-id", hv(&uid)),
        ("x-user-name", hv(&name)),
        ("x-user-email", hv(&email)),
        ("x-user-roles", hv(&roles)),
        ("x-client-id", hv(&client)),
        ("x-identity-ts", hv(&ts.to_string())),
        ("x-identity-sig", hv(&sig)),
    ];
    for (k, v) in pairs {
        h.insert(HeaderName::from_static(k), v);
    }
    h.insert(HeaderName::from_static("x-request-id"), hv(request_id));
}

fn identite_route(r: Result<Authn, AuthError>, public: bool) -> Result<Option<Authn>, StatusCode> {
    match r {
        Ok(a) => Ok(Some(a)),
        Err(_) if public => Ok(None),
        Err(AuthError::Unavailable) => Err(StatusCode::SERVICE_UNAVAILABLE),
        Err(AuthError::Unauthenticated) => Err(StatusCode::UNAUTHORIZED),
    }
}

fn entetes_reponse(amont: &HeaderMap, request_id: &str) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in amont {
        if !HOP_BY_HOP.contains(&k.as_str()) && k.as_str() != "x-request-id" {
            out.append(k.clone(), v.clone());
        }
    }
    out.insert(HeaderName::from_static("x-request-id"), hv(request_id));
    out
}

fn is_websocket(headers: &HeaderMap) -> bool {
    headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

async fn proxy(State(state): State<AppState>, req: Request) -> Response {
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(str::to_string);
    if unsafe_path(&path) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(route) = find_route(&state.settings.routes, &path).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let request_id = session::random_token(12);

    // authentification
    let authn = match identite_route(
        auth::authenticate(&state, req.headers()).await,
        route.public,
    ) {
        Ok(a) => a,
        Err(statut) => return statut.into_response(),
    };
    if let Some(a) = &authn
        && !auth::csrf_ok(&state, req.headers(), req.method(), a.sid.is_some())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !route.roles.is_empty() {
        match &authn {
            Some(a) if a.identity.has_any_role(&route.roles) => {}
            Some(_) => return StatusCode::FORBIDDEN.into_response(),
            None => return StatusCode::UNAUTHORIZED.into_response(),
        }
    }

    let url = upstream_url(&route, &path, query.as_deref());
    let secret = &state.settings.identity.secret;

    if is_websocket(req.headers()) {
        return proxy_ws(&state, req, &url, authn.as_ref(), secret, &request_id).await;
    }

    // en-têtes sortants
    let mut out = HeaderMap::new();
    for (k, v) in req.headers() {
        let name = k.as_str();
        if HOP_BY_HOP.contains(&name)
            || IDENTITY_HEADERS.contains(&name)
            || matches!(name, "host" | "cookie" | "authorization")
        {
            continue;
        }
        out.append(k.clone(), v.clone());
    }
    if let Some(a) = &authn {
        inject(&mut out, secret, &a.identity, &request_id);
        if route.forward_token
            && let Ok(v) = HeaderValue::from_str(&format!("Bearer {}", a.access_token))
        {
            out.insert(header::AUTHORIZATION, v);
        }
    } else {
        out.insert(HeaderName::from_static("x-request-id"), hv(&request_id));
    }

    let method = req.method().clone();
    let body = reqwest::Body::wrap_stream(req.into_body().into_data_stream());
    let upstream = state
        .proxy_http
        .request(method, &url)
        .headers(out)
        .body(body)
        .send()
        .await;
    let upstream = match upstream {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(%url, "upstream injoignable: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let (statut, entetes) = (
        upstream.status(),
        entetes_reponse(upstream.headers(), &request_id),
    );
    let mut resp = Response::new(Body::from_stream(upstream.bytes_stream()));
    *resp.status_mut() = statut;
    *resp.headers_mut() = entetes;
    resp
}

async fn proxy_ws(
    state: &AppState,
    req: Request,
    url: &str,
    authn: Option<&Authn>,
    secret: &str,
    request_id: &str,
) -> Response {
    let ws_url = url.replacen("http", "ws", 1);
    let mut up_req = match ws_url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(%ws_url, "URL WebSocket invalide: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    if let Some(p) = req.headers().get(header::SEC_WEBSOCKET_PROTOCOL) {
        up_req
            .headers_mut()
            .insert(header::SEC_WEBSOCKET_PROTOCOL, p.clone());
    }
    if let Some(a) = authn {
        inject(up_req.headers_mut(), secret, &a.identity, request_id);
    }

    let ws = match WebSocketUpgrade::from_request(req, state).await {
        Ok(ws) => ws,
        Err(e) => return e.into_response(),
    };
    let upstream = match tokio_tungstenite::connect_async(up_req).await {
        Ok((s, _)) => s,
        Err(e) => {
            tracing::warn!(%ws_url, "upstream WebSocket injoignable: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let max_life = std::time::Duration::from_secs(state.settings.server.ws_max_lifetime_secs);
    ws.on_upgrade(move |client| relay(client, upstream, max_life))
}

async fn relay(
    client: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    max_life: std::time::Duration,
) {
    let (mut c_tx, mut c_rx) = client.split();
    let (mut u_tx, mut u_rx) = upstream.split();

    let to_upstream = async {
        while let Some(Ok(msg)) = c_rx.next().await {
            let close = matches!(msg, ws::Message::Close(_));
            if u_tx.send(to_up(msg)).await.is_err() || close {
                break;
            }
        }
    };
    let to_client = async {
        while let Some(Ok(msg)) = u_rx.next().await {
            let close = matches!(msg, tungstenite::Message::Close(_));
            if let Some(m) = to_client_msg(msg)
                && c_tx.send(m).await.is_err()
            {
                break;
            }
            if close {
                break;
            }
        }
    };
    let expired = tokio::select! { _ = to_upstream => false, _ = to_client => false, _ = tokio::time::sleep(max_life) => true };
    if expired {
        let frame = ws::CloseFrame {
            code: 1001,
            reason: "reconnexion requise".into(),
        };
        let _ = c_tx.send(ws::Message::Close(Some(frame))).await;
    }
}

fn to_up(m: ws::Message) -> tungstenite::Message {
    match m {
        ws::Message::Text(t) => tungstenite::Message::Text(t.as_str().into()),
        ws::Message::Binary(b) => tungstenite::Message::Binary(b),
        ws::Message::Ping(b) => tungstenite::Message::Ping(b),
        ws::Message::Pong(b) => tungstenite::Message::Pong(b),
        ws::Message::Close(f) => {
            tungstenite::Message::Close(f.map(|f| tungstenite::protocol::CloseFrame {
                code: f.code.into(),
                reason: f.reason.as_str().into(),
            }))
        }
    }
}

fn to_client_msg(m: tungstenite::Message) -> Option<ws::Message> {
    Some(match m {
        tungstenite::Message::Text(t) => ws::Message::Text(t.as_str().into()),
        tungstenite::Message::Binary(b) => ws::Message::Binary(b),
        tungstenite::Message::Ping(b) => ws::Message::Ping(b),
        tungstenite::Message::Pong(b) => ws::Message::Pong(b),
        tungstenite::Message::Close(f) => ws::Message::Close(f.map(|f| ws::CloseFrame {
            code: u16::from(f.code),
            reason: f.reason.as_str().into(),
        })),
        tungstenite::Message::Frame(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(prefix: &str, strip: bool) -> Route {
        Route {
            prefix: prefix.into(),
            upstream: "http://svc:8080/".into(),
            strip_prefix: strip,
            public: false,
            roles: vec![],
            forward_token: false,
        }
    }

    #[test]
    fn rejects_path_traversal() {
        for bad in [
            "/api/a/../b",
            "/api/a/./b",
            "/api/a/%2e%2e/b",
            "/api/a/%2E./b",
            "/api/a/..",
            "/api/a/x%2fy",
            "/api/a/x%5Cy",
            "/api/a\\b",
        ] {
            assert!(unsafe_path(bad), "{bad}");
        }
        for ok in [
            "/api/a/b",
            "/api/a/..b",
            "/api/a/b..",
            "/api/a/v1.2/x",
            "/api/a/%e2%82%ac",
        ] {
            assert!(!unsafe_path(ok), "{ok}");
        }
    }

    #[test]
    fn longest_prefix_wins() {
        let routes = vec![route("/api/chat", true), route("/api/chat/admin", true)];
        assert_eq!(
            find_route(&routes, "/api/chat/admin/x").unwrap().prefix,
            "/api/chat/admin"
        );
        assert_eq!(
            find_route(&routes, "/api/chat/x").unwrap().prefix,
            "/api/chat"
        );
        assert!(find_route(&routes, "/api/chatty").is_none());
        assert!(find_route(&routes, "/api/other").is_none());
    }

    #[test]
    fn builds_upstream_url() {
        assert_eq!(
            upstream_url(&route("/api/chat", true), "/api/chat/rooms", Some("a=1")),
            "http://svc:8080/rooms?a=1"
        );
        assert_eq!(
            upstream_url(&route("/api/chat", true), "/api/chat", None),
            "http://svc:8080/"
        );
        assert_eq!(
            upstream_url(&route("/api/chat", false), "/api/chat/rooms", None),
            "http://svc:8080/api/chat/rooms"
        );
    }

    fn authn() -> Authn {
        Authn {
            identity: Identity {
                id: "u1".into(),
                name: "Jérôme".into(),
                email: "zoé@plbls.fr".into(),
                roles: vec!["user".into(), "chat:admin".into()],
                client: "plbls-gateway".into(),
            },
            access_token: "t".into(),
            sid: None,
        }
    }

    #[test]
    fn public_route_falls_back_to_anonymous() {
        assert!(identite_route(Ok(authn()), false).unwrap().is_some());
        for e in [AuthError::Unavailable, AuthError::Unauthenticated] {
            assert!(identite_route(Err(e), true).unwrap().is_none());
        }
        assert_eq!(
            identite_route(Err(AuthError::Unavailable), false).err(),
            Some(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(
            identite_route(Err(AuthError::Unauthenticated), false).err(),
            Some(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn single_request_id_in_response() {
        let mut amont = HeaderMap::new();
        amont.insert("x-request-id", HeaderValue::from_static("amont"));
        amont.insert("connection", HeaderValue::from_static("close"));
        amont.insert("content-type", HeaderValue::from_static("text/plain"));
        let h = entetes_reponse(&amont, "passerelle");
        let ids: Vec<_> = h.get_all("x-request-id").iter().collect();
        assert_eq!(ids, vec![HeaderValue::from_static("passerelle")]);
        assert!(h.get("connection").is_none());
        assert_eq!(h["content-type"], "text/plain");
    }

    #[test]
    fn injects_encoded_and_signed_name_and_email() {
        let mut h = HeaderMap::new();
        inject(&mut h, "secret", &authn().identity, "rid");
        let v = |k: &str| h[k].to_str().unwrap().to_string();
        assert_eq!(v("x-user-name"), "J%C3%A9r%C3%B4me");
        assert_eq!(v("x-user-email"), "zo%C3%A9@plbls.fr");
        let (id, roles, client, name, email) = (
            v("x-user-id"),
            v("x-user-roles"),
            v("x-client-id"),
            v("x-user-name"),
            v("x-user-email"),
        );
        let ts: u64 = v("x-identity-ts").parse().unwrap();
        let mut champs = identity::Champs {
            id: &id,
            roles: &roles,
            client: &client,
            ts,
            name: &name,
            email: &email,
        };
        assert!(identity::verify(
            "secret",
            &champs,
            &v("x-identity-sig"),
            ts
        ));
        champs.email = "root@plbls.fr";
        assert!(!identity::verify(
            "secret",
            &champs,
            &v("x-identity-sig"),
            ts
        ));
    }
}
