//! Serveur MCP (HTTP streamable, JSON-RPC sans état) : donne à un assistant la documentation et la spec OpenAPI.
//! Protégé par un token statique (`PERMISSION__MCP__TOKEN`).

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

const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Docs embarquées dans le binaire : (nom, description, contenu).
const DOCS: [(&str, &str, &str); 4] = [
    (
        "README",
        "Index de la documentation",
        include_str!("../../../../docs/permission/README.md"),
    ),
    (
        "model",
        "Modèle RBAC : applications, permissions, rôles, attributions",
        include_str!("../../../../docs/permission/model.md"),
    ),
    (
        "integration",
        "Brancher un micro-service ou un frontend",
        include_str!("../../../../docs/permission/integration.md"),
    ),
    (
        "architecture",
        "Rôle du projet, stack, sécurité, décisions",
        include_str!("../../../../docs/permission/architecture.md"),
    ),
];

const INSTRUCTIONS: &str = "Documentation de permission (permission.plbls.fr) : gestion des permissions des comptes plbls.fr \
(les comptes eux-mêmes sont dans security.plbls.fr / Keycloak). Commence par get_doc(\"README\"), puis model et integration. \
get_openapi donne la spec OpenAPI complète de l'API REST, list_operations un résumé des routes.";

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(mcp))
}

async fn mcp(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    let token = state.settings.mcp.token.as_bytes();
    if token.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(s, _)| s.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim().as_bytes());
    if !given.is_some_and(|g| bool::from(g.ct_eq(token))) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    let Ok(msg) = serde_json::from_str::<Value>(&body) else {
        return Json(error(Value::Null, -32700, "JSON invalide")).into_response();
    };
    match handle(&state.spec, &msg) {
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
fn handle(spec: &str, msg: &Value) -> Option<Value> {
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
                "serverInfo": {"name": "permission", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            })
        }
        Some("ping") => json!({}),
        Some("tools/list") => tools_list(),
        Some("tools/call") => tools_call(spec, &params),
        Some("resources/list") => {
            let mut res: Vec<Value> = DOCS
                .iter()
                .map(|(n, d, _)| json!({"uri": format!("plbls://permission/docs/{n}"), "name": n, "description": d, "mimeType": "text/markdown"}))
                .collect();
            res.push(json!({"uri": "plbls://permission/openapi", "name": "openapi", "description": "Spec OpenAPI 3", "mimeType": "application/json"}));
            json!({"resources": res})
        }
        Some("resources/read") => {
            let uri = params["uri"].as_str().unwrap_or_default();
            let found = if uri == "plbls://permission/openapi" {
                Some(("application/json", spec))
            } else {
                uri.strip_prefix("plbls://permission/docs/")
                    .and_then(find_doc)
                    .map(|t| ("text/markdown", t))
            };
            match found {
                Some((mime, text)) => {
                    json!({"contents": [{"uri": uri, "mimeType": mime, "text": text}]})
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
        {"name": "get_openapi", "description": "Spec OpenAPI 3 complète de l'API REST (JSON).",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "list_operations", "description": "Résumé des routes de l'API : méthode, chemin, description.",
         "inputSchema": {"type": "object", "properties": {}}},
    ]})
}

fn list_operations(spec: &str) -> String {
    let spec: Value = serde_json::from_str(spec).unwrap_or_default();
    let mut lines = Vec::new();
    for (path, item) in spec["paths"].as_object().into_iter().flatten() {
        for (method, op) in item.as_object().into_iter().flatten() {
            lines.push(format!(
                "{} {} : {}",
                method.to_uppercase(),
                path,
                op["summary"].as_str().unwrap_or("")
            ));
        }
    }
    lines.join("\n")
}

fn tools_call(spec: &str, params: &Value) -> Value {
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
        Some("get_openapi") => text_result(spec.to_string(), false),
        Some("list_operations") => text_result(list_operations(spec), false),
        _ => text_result("outil inconnu".into(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(m: Value) -> Value {
        handle(&crate::routes::spec_json(), &m).expect("réponse")
    }

    #[test]
    fn protocol() {
        assert!(
            handle(
                "{}",
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .is_none()
        );
        let r = call(
            json!({"id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(
            call(json!({"id": 3, "method": "nope"}))["error"]["code"],
            -32601
        );
        for bad in [json!("texte"), json!(null), json!([1])] {
            assert_eq!(call(bad)["error"]["code"], -32600);
        }
    }

    #[test]
    fn docs_and_openapi() {
        let r = call(
            json!({"id": 1, "method": "tools/call", "params": {"name": "get_doc", "arguments": {"name": "model.md"}}}),
        );
        assert_eq!(r["result"]["isError"], false);
        let r = call(
            json!({"id": 2, "method": "tools/call", "params": {"name": "get_doc", "arguments": {"name": "../etc/passwd"}}}),
        );
        assert_eq!(r["result"]["isError"], true);
        let r =
            call(json!({"id": 3, "method": "tools/call", "params": {"name": "list_operations"}}));
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("POST /v1/check")
        );
        let r = call(
            json!({"id": 4, "method": "resources/read", "params": {"uri": "plbls://permission/openapi"}}),
        );
        assert!(
            serde_json::from_str::<Value>(r["result"]["contents"][0]["text"].as_str().unwrap())
                .is_ok()
        );
    }
}
