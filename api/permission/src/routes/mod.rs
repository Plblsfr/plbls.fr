pub mod admin;
pub mod check;
pub mod health;
pub mod openapi;
pub mod users;

use axum::{
    Router,
    http::{HeaderName, HeaderValue},
};
use tower_http::set_header::SetResponseHeaderLayer;
use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

/// Description générale de l'API ; les chemins viennent des handlers (`#[utoipa::path]`) enregistrés dans [`api`].
#[derive(OpenApi)]
#[openapi(
    info(
        title = "permission.plbls.fr",
        description = "Permissions RBAC par application des comptes plbls.fr. Authentification : en-têtes d'identité signés \
du gateway (utilisateur) ou Bearer de service. Les routes `admin` exigent le rôle Keycloak configuré (`server.admin_role`, \
défaut `permission-admin`) ou la permission `permission:admin` (application réservée `permission`)."
    ),
    servers((url = "https://security.plbls.fr/api/permission", description = "Via le gateway (utilisateurs, frontend)")),
    tags(
        (name = "lecture", description = "Consultation des permissions (services et frontends)"),
        (name = "admin", description = "Administration (rôle permission-admin)"),
        (name = "système")
    ),
    modifiers(&Security),
    security(("gatewayIdentity" = []), ("serviceToken" = []))
)]
struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let c = openapi.components.get_or_insert_with(Default::default);
        let mut key = ApiKey::Header(ApiKeyValue::new("X-Identity-Sig"));
        if let ApiKey::Header(v) = &mut key {
            v.description = Some("Avec X-User-Id, X-User-Roles, X-Client-Id, X-Identity-Ts, X-User-Name et X-User-Email : injectés par auth-gateway".into());
        }
        c.add_security_scheme("gatewayIdentity", SecurityScheme::ApiKey(key));
        c.add_security_scheme(
            "serviceToken",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Token d'un micro-service (PERMISSION__SERVICES__TOKENS), réseau interne uniquement"))
                    .build(),
            ),
        );
    }
}

/// Toutes les routes documentées : la spec OpenAPI en est déduite, elle ne peut pas diverger du code.
pub fn api() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::router())
        .merge(check::router())
        .merge(admin::router())
        .merge(users::router())
}

pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self'; connect-src 'self' https://security.plbls.fr; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

const SECURITE: [(&str, &str); 4] = [
    ("content-security-policy", CSP),
    ("referrer-policy", "no-referrer"),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
];

pub fn avec_securite<S: Clone + Send + Sync + 'static>(mut app: Router<S>) -> Router<S> {
    for (nom, valeur) in SECURITE {
        app = app.layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static(nom),
            HeaderValue::from_static(valeur),
        ));
    }
    app
}

#[cfg(test)]
pub fn spec_json() -> String {
    api()
        .split_for_parts()
        .1
        .to_pretty_json()
        .expect("spec OpenAPI sérialisable")
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use serde_json::Value;
    use tower::ServiceExt;
    use tower_http::services::{ServeDir, ServeFile};

    #[tokio::test]
    async fn spa_and_api_carry_security_headers() {
        let dir = std::env::temp_dir().join(format!("permission-spa-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html>").unwrap();
        let app = super::avec_securite(
            axum::Router::new()
                .route("/health", axum::routing::get(|| async { "ok" }))
                .fallback_service(
                    ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html"))),
                ),
        );
        for chemin in ["/", "/index.html", "/apps/route-du-client", "/health"] {
            let rep = app
                .clone()
                .oneshot(Request::get(chemin).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert!(rep.status().is_success(), "{chemin} : {}", rep.status());
            let h = rep.headers();
            assert_eq!(h["content-security-policy"], super::CSP, "{chemin}");
            assert!(super::CSP.contains("frame-ancestors 'none'"));
            assert_eq!(h["x-frame-options"], "DENY", "{chemin}");
            assert_eq!(h["x-content-type-options"], "nosniff", "{chemin}");
            assert_eq!(h["referrer-policy"], "no-referrer", "{chemin}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn spec_covers_every_route() {
        let spec: Value = serde_json::from_str(&super::spec_json()).unwrap();
        let ops: Vec<String> = spec["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(p, item)| {
                item.as_object()
                    .unwrap()
                    .keys()
                    .map(move |m| format!("{} {p}", m.to_uppercase()))
            })
            .collect();
        for expected in [
            "GET /health",
            "GET /v1/me/permissions",
            "GET /v1/users/{user_id}/permissions",
            "POST /v1/check",
            "GET /v1/apps",
            "POST /v1/apps",
            "PUT /v1/apps/{app}",
            "DELETE /v1/apps/{app}",
            "GET /v1/apps/{app}/permissions",
            "POST /v1/apps/{app}/permissions",
            "DELETE /v1/apps/{app}/permissions/{key}",
            "GET /v1/apps/{app}/roles",
            "POST /v1/apps/{app}/roles",
            "PUT /v1/apps/{app}/roles/{role}",
            "DELETE /v1/apps/{app}/roles/{role}",
            "GET /v1/assignments",
            "POST /v1/assignments",
            "DELETE /v1/assignments",
            "GET /v1/audit",
            "GET /v1/users",
        ] {
            assert!(
                ops.contains(&expected.to_string()),
                "{expected} absent de la spec : {ops:?}"
            );
        }
        assert_eq!(ops.len(), 20, "route non attendue : {ops:?}");
        assert!(spec["components"]["securitySchemes"]["serviceToken"].is_object());
    }
}
