//! Serveur MCP (HTTP streamable, JSON-RPC sans état) : donne à un assistant la documentation du gateway
//! et la liste des routes configurées. Protégé par un token statique (`GATEWAY__MCP__TOKEN`).

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;

use crate::AppState;
use crate::settings::Settings;

const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Docs embarquées dans le binaire : (nom, description, contenu).
const DOCS: [(&str, &str, &str); 9] = [
    (
        "README",
        "Index de la documentation",
        include_str!("../../docs/README.md"),
    ),
    (
        "architecture",
        "Contexte, objectifs, vue d'ensemble",
        include_str!("../../docs/architecture.md"),
    ),
    (
        "flows",
        "Flows d'authentification : web BFF, mobile/Windows, extension",
        include_str!("../../docs/flows.md"),
    ),
    (
        "gateway",
        "Routage, en-têtes d'identité, erreurs",
        include_str!("../../docs/gateway.md"),
    ),
    (
        "keycloak",
        "Clients, scope d'audience, réglages du realm",
        include_str!("../../docs/keycloak.md"),
    ),
    (
        "endpoints",
        "Endpoints du service",
        include_str!("../../docs/endpoints.md"),
    ),
    (
        "security",
        "Cookie, CSRF, CORS, JWT, refresh, logout",
        include_str!("../../docs/security.md"),
    ),
    (
        "stack",
        "Stack technique et modules",
        include_str!("../../docs/stack.md"),
    ),
    (
        "decisions",
        "Décisions d'architecture et d'hébergement",
        include_str!("../../docs/decisions.md"),
    ),
];

const INSTRUCTIONS: &str = "Documentation de auth-gateway (security.plbls.fr), point d'entrée d'authentification de plbls.fr. \
Commence par get_doc(\"README\"), puis flows (quel flow selon le type de client), gateway (en-têtes d'identité à lire côté \
micro-service) et endpoints. list_routes donne les routes /api réellement configurées avec leurs règles d'accès.";

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(mcp))
}

/// Compare le Bearer reçu au token statique, à temps constant.
pub fn bearer_matches(headers: &HeaderMap, token: &[u8]) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(s, _)| s.eq_ignore_ascii_case("bearer"))
        .is_some_and(|(_, t)| bool::from(t.trim().as_bytes().ct_eq(token)))
}

async fn mcp(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    let token = state.settings.mcp.token.as_bytes();
    if token.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !bearer_matches(&headers, token) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    let Ok(msg) = serde_json::from_str::<Value>(&body) else {
        return Json(error(Value::Null, -32700, "JSON invalide")).into_response();
    };
    match handle(&state.settings, &msg) {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(), // notification : pas de réponse
    }
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn text_result(text: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

/// `None` pour une notification (message sans `id`).
fn handle(settings: &Settings, msg: &Value) -> Option<Value> {
    if !msg.is_object() {
        return Some(error(Value::Null, -32600, "requête invalide"));
    }
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match msg.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            let wanted = params["protocolVersion"].as_str().unwrap_or_default();
            let version = PROTOCOLS
                .iter()
                .find(|v| **v == wanted)
                .unwrap_or(&PROTOCOLS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "auth-gateway", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            })
        }
        Some("ping") => json!({}),
        Some("tools/list") => tools_list(),
        Some("tools/call") => tools_call(settings, &params),
        Some("resources/list") => json!({"resources": DOCS.iter().map(|(n, d, _)| json!({
            "uri": format!("plbls://docs/{n}"), "name": n, "description": d, "mimeType": "text/markdown",
        })).collect::<Vec<_>>()}),
        Some("resources/read") => {
            let uri = params["uri"].as_str().unwrap_or_default();
            match uri.strip_prefix("plbls://docs/").and_then(find_doc) {
                Some(text) => {
                    json!({"contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}]})
                }
                None => return Some(error(id, -32002, "ressource inconnue")),
            }
        }
        _ => return Some(error(id, -32601, "méthode inconnue")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn find_doc(name: &str) -> Option<&'static str> {
    DOCS.iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(name.trim_end_matches(".md")))
        .map(|(_, _, t)| *t)
}

fn tools_list() -> Value {
    json!({"tools": [
        {"name": "list_docs", "description": "Liste les documents disponibles avec leur contenu résumé.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "get_doc", "description": "Texte complet d'un document (voir list_docs).",
         "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}},
        {"name": "list_routes", "description": "Routes /api configurées : préfixe, accès (public, rôles requis), \
    transmission du token, retrait du préfixe. Ne donne pas les upstreams (réseau interne).",
         "inputSchema": {"type": "object", "properties": {}}},
    ]})
}

fn tools_call(settings: &Settings, params: &Value) -> Value {
    let args = &params["arguments"];
    match params["name"].as_str() {
        Some("list_docs") => text_result(
            DOCS.iter()
                .map(|(n, d, _)| format!("- {n} : {d}"))
                .collect::<Vec<_>>()
                .join("\n"),
            false,
        ),
        Some("get_doc") => match args["name"].as_str().and_then(find_doc) {
            Some(t) => text_result(t.to_string(), false),
            None => text_result("document inconnu, voir list_docs".into(), true),
        },
        Some("list_routes") => {
            let base = settings.server.public_url.trim_end_matches('/');
            let routes: Vec<Value> = settings
                .routes
                .iter()
                .map(|r| {
                    json!({
                        "url": format!("{base}{}", r.prefix),
                        "prefix": r.prefix,
                        "public": r.public,
                        "roles": r.roles,
                        "strip_prefix": r.strip_prefix,
                        "forward_token": r.forward_token,
                    })
                })
                .collect();
            text_result(
                serde_json::to_string_pretty(&routes).unwrap_or_default(),
                false,
            )
        }
        _ => text_result("outil inconnu".into(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        let toml = r#"
            [server]
            bind = "x"
            public_url = "https://security.plbls.fr/"
            default_redirect = "x"
            allowed_origin_suffix = "plbls.fr"
            [redis]
            url = "x"
            [oidc]
            issuer = "x"
            client_id = "x"
            client_secret = "x"
            audience = "x"
            [session]
            cookie_name = "x"
            cookie_domain = "x"
            [identity]
            secret = "x"
            [[routes]]
            prefix = "/api/chat"
            upstream = "http://interne:8080"
            roles = ["user"]
        "#;
        config::Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap()
    }

    fn call(s: &Settings, m: Value) -> Value {
        handle(s, &m).expect("réponse")
    }

    #[test]
    fn protocol() {
        let s = settings();
        assert!(
            handle(
                &s,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .is_none()
        );
        let r = call(
            &s,
            json!({"id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
        let r = call(
            &s,
            json!({"id": 2, "method": "initialize", "params": {"protocolVersion": "1999"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], PROTOCOLS[0]);
        assert_eq!(
            call(&s, json!({"id": 3, "method": "nope"}))["error"]["code"],
            -32601
        );
        for bad in [json!("texte"), json!(null), json!([1])] {
            assert_eq!(call(&s, bad)["error"]["code"], -32600);
        }
    }

    #[test]
    fn docs_and_routes() {
        let s = settings();
        let r = call(&s, json!({"id": 1, "method": "resources/list"}));
        assert_eq!(
            r["result"]["resources"].as_array().unwrap().len(),
            DOCS.len()
        );
        let r = call(
            &s,
            json!({"id": 2, "method": "resources/read", "params": {"uri": "plbls://docs/flows"}}),
        );
        assert!(
            r["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("PKCE")
        );
        assert!(
            call(
                &s,
                json!({"id": 3, "method": "resources/read", "params": {"uri": "plbls://docs/zz"}})
            )
            .get("error")
            .is_some()
        );

        let r = call(
            &s,
            json!({"id": 4, "method": "tools/call", "params": {"name": "get_doc", "arguments": {"name": "Security.md"}}}),
        );
        assert_eq!(r["result"]["isError"], false);
        let r = call(
            &s,
            json!({"id": 5, "method": "tools/call", "params": {"name": "get_doc", "arguments": {"name": "../etc/passwd"}}}),
        );
        assert_eq!(r["result"]["isError"], true);

        let r = call(
            &s,
            json!({"id": 6, "method": "tools/call", "params": {"name": "list_routes"}}),
        );
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("https://security.plbls.fr/api/chat"));
        assert!(!text.contains("interne"), "l'upstream ne doit pas fuiter");
    }
}
