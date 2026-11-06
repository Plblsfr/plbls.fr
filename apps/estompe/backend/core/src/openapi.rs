use serde::Serialize;
use utoipa::{
    Modify, OpenApi, ToSchema,
    openapi::{
        HttpMethod, Required, ResponseBuilder, Type,
        path::{Operation, OperationBuilder, ParameterBuilder, ParameterIn},
        schema::ObjectBuilder,
        security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
    },
};
use utoipa_axum::router::OpenApiRouter;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Estompe API",
        version = "0.1.0",
        description = "API du service de chat Estompe. Voir `docs/realtime.md` pour les évènements WebSocket.\n\nErreurs : `{\"error\": \"<code>\", \"message\": \"<texte>\"}`."
    ),
    servers((url = "https://chat.plbls.fr")),
    security(("bearer" = []), ("session" = [])),
    tags((name = "compte"), (name = "temps réel"), (name = "supervision")),
    components(schemas(Error, Person)),
    modifiers(&Securite)
)]
pub struct Base;

struct Securite;

impl Modify for Securite {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let composants = openapi.components.get_or_insert_with(Default::default);
        composants.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Access token Keycloak (client plbls-desktop)"))
                    .build(),
            ),
        );
        let mut cookie = ApiKeyValue::new("__Secure-plbls_sid");
        cookie.description = Some("Cookie de session posé par auth-gateway".to_string());
        composants.add_security_scheme("session", SecurityScheme::ApiKey(ApiKey::Cookie(cookie)));
    }
}

#[derive(Serialize, ToSchema)]
pub struct Error {
    pub error: String,
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct Person {
    #[schema(value_type = Uuid)]
    pub id: String,
    pub pseudo: String,
}

pub fn operation(tag: &str, summary: &str, reponses: &[(&str, &str)], chemin: &str) -> Operation {
    let mut op = OperationBuilder::new().tag(tag).summary(Some(summary));
    for (statut, description) in reponses {
        op = op.response(
            *statut,
            ResponseBuilder::new().description(*description).build(),
        );
    }
    for segment in chemin.split('/') {
        if let Some(nom) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            op = op.parameter(
                ParameterBuilder::new()
                    .name(nom)
                    .parameter_in(ParameterIn::Path)
                    .required(Required::True)
                    .schema(Some(ObjectBuilder::new().schema_type(Type::String))),
            );
        }
    }
    op.build()
}

pub fn documenter<S: Clone + Send + Sync + 'static>(
    mut routeur: OpenApiRouter<S>,
    chemin: &str,
    methode: HttpMethod,
    op: Operation,
) -> OpenApiRouter<S> {
    routeur
        .get_openapi_mut()
        .paths
        .add_path_operation(chemin, vec![methode], op);
    routeur
}
