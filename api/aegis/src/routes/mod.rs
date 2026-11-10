pub mod admin;
pub mod health;
pub mod openapi;
pub mod service;

use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "aegis",
        description = "Permissions des comptes plbls.fr : rôles multi-applications, groupes, droits directs, refus explicites, \
expiration et portée sur une ressource. Le refus l'emporte toujours. Les applications déclarent leurs permissions par manifeste \
(`PUT /v1/manifest`) et lisent les droits calculés (`POST /v1/effective`), mis en cache dans Redis et invalidés par pub/sub \
(`aegis:invalidation`). Les routes `admin` exigent le rôle Keycloak `server.admin_role` (défaut `aegis-admin`) ou la permission `aegis:admin`."
    ),
    servers((url = "https://security.plbls.fr/api/aegis", description = "Via la passerelle (utilisateurs, administration)")),
    tags(
        (name = "service", description = "Applications (jeton de service, réseau interne)"),
        (name = "utilisateur", description = "Compte connecté (identité signée par la passerelle)"),
        (name = "admin", description = "Administration (super-admin)"),
        (name = "système")
    ),
    modifiers(&Securite),
    security(("gatewayIdentity" = []))
)]
struct ApiDoc;

struct Securite;

impl Modify for Securite {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let c = openapi.components.get_or_insert_with(Default::default);
        let mut cle = ApiKey::Header(ApiKeyValue::new("X-Identity-Sig"));
        if let ApiKey::Header(v) = &mut cle {
            v.description = Some(
                "Avec X-User-Id, X-User-Roles, X-Client-Id, X-Identity-Ts, X-User-Name et X-User-Email (percent-encoding UTF-8) : injectés par la passerelle".into(),
            );
        }
        c.add_security_scheme("gatewayIdentity", SecurityScheme::ApiKey(cle));
        c.add_security_scheme(
            "serviceToken",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Jeton d'une application (AEGIS_TOKEN_<APPLICATION>), lié à une seule application"))
                    .build(),
            ),
        );
    }
}

pub fn api() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::router())
        .merge(service::router())
        .merge(admin::router())
}

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
    fn la_spec_couvre_chaque_route() {
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
        let attendues = [
            "GET /health",
            "PUT /v1/manifest",
            "POST /v1/effective",
            "POST /v1/check",
            "GET /v1/me/effective",
            "GET /v1/admin/catalog",
            "GET /v1/admin/resources/{app}/{type}",
            "GET /v1/admin/permissions/{name}/holders",
            "POST /v1/admin/permissions/purge-obsolete",
            "GET /v1/admin/accounts",
            "GET /v1/admin/accounts/{sub}",
            "GET /v1/admin/accounts/{sub}/effective",
            "POST /v1/admin/accounts/{sub}/copy",
            "DELETE /v1/admin/accounts/{sub}/grants",
            "POST /v1/admin/simulate",
            "GET /v1/admin/roles",
            "POST /v1/admin/roles",
            "GET /v1/admin/roles/{id}",
            "PUT /v1/admin/roles/{id}",
            "DELETE /v1/admin/roles/{id}",
            "GET /v1/admin/groups",
            "POST /v1/admin/groups",
            "GET /v1/admin/groups/{id}",
            "PUT /v1/admin/groups/{id}",
            "DELETE /v1/admin/groups/{id}",
            "PUT /v1/admin/groups/{id}/members/{sub}",
            "DELETE /v1/admin/groups/{id}/members/{sub}",
            "GET /v1/admin/bindings",
            "POST /v1/admin/bindings",
            "DELETE /v1/admin/bindings/{id}",
            "GET /v1/admin/rules",
            "POST /v1/admin/rules",
            "PUT /v1/admin/rules/{id}",
            "DELETE /v1/admin/rules/{id}",
            "GET /v1/admin/audit",
        ];
        for a in attendues {
            assert!(
                ops.contains(&a.to_string()),
                "{a} absente de la spec : {ops:?}"
            );
        }
        assert_eq!(ops.len(), attendues.len(), "route non attendue : {ops:?}");
        assert!(spec["components"]["securitySchemes"]["serviceToken"].is_object());
    }
}
