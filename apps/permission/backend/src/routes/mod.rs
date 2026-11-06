pub mod admin;
pub mod check;
pub mod health;
pub mod mcp;
pub mod openapi;
pub mod users;

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
            v.description = Some("Avec X-User-Id, X-User-Roles, X-Client-Id, X-Identity-Ts : injectés par auth-gateway".into());
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
    use serde_json::Value;

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
