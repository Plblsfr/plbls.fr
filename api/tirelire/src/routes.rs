use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::Method,
    middleware::{from_fn_with_state, map_response},
    response::Response,
    routing::{MethodRouter, delete, get, patch, post, put},
};

use crate::{
    categories, commercants,
    droits::Droit,
    etat::AppState,
    export,
    garde::{self, Garde},
    import, interne, mois, regles, releves,
};

pub struct Route {
    pub methode: Method,
    pub chemin: &'static str,
    pub garde: Garde,
}

pub type Entree = (Route, MethodRouter<AppState>);

pub fn entree(
    methode: Method,
    chemin: &'static str,
    garde: Garde,
    mr: MethodRouter<AppState>,
) -> Entree {
    (
        Route {
            methode,
            chemin,
            garde,
        },
        mr,
    )
}

pub fn table() -> Vec<Entree> {
    vec![
        entree(
            Method::GET,
            "/healthz",
            Garde::Public("santé du processus, sans donnée"),
            get(interne::sante),
        ),
        entree(
            Method::GET,
            "/readyz",
            Garde::Public("disponibilité de la base, sans donnée"),
            get(interne::pret),
        ),
        entree(
            Method::GET,
            "/me",
            Garde::Session(
                "annonce à l'app l'identité et les permissions, y compris sans tirelire:access",
            ),
            get(interne::moi),
        ),
        entree(
            Method::GET,
            "/accounts",
            Garde::Droit(Droit::Acces),
            get(releves::comptes),
        ),
        entree(
            Method::GET,
            "/statements",
            Garde::Droit(Droit::Acces),
            get(releves::lister),
        ),
        entree(
            Method::POST,
            "/statements/import",
            Garde::Droit(Droit::Acces),
            post(import::importer).layer(DefaultBodyLimit::max(import::REQUETE_MAX)),
        ),
        entree(
            Method::DELETE,
            "/statements/{id}",
            Garde::Droit(Droit::Acces),
            delete(releves::supprimer),
        ),
        entree(
            Method::GET,
            "/operations",
            Garde::Droit(Droit::Acces),
            get(releves::operations),
        ),
        entree(
            Method::GET,
            "/operations/export.csv",
            Garde::Droit(Droit::Acces),
            get(export::exporter),
        ),
        entree(
            Method::GET,
            "/operations/{id}",
            Garde::Droit(Droit::Acces),
            get(releves::operation),
        ),
        entree(
            Method::PUT,
            "/operations/{id}/category",
            Garde::Droit(Droit::Acces),
            put(releves::classer),
        ),
        entree(
            Method::GET,
            "/unclassified",
            Garde::Droit(Droit::Acces),
            get(commercants::a_classer),
        ),
        entree(
            Method::GET,
            "/unclassified/count",
            Garde::Droit(Droit::Acces),
            get(commercants::compter),
        ),
        entree(
            Method::GET,
            "/categories",
            Garde::Droit(Droit::Acces),
            get(categories::lister),
        ),
        entree(
            Method::POST,
            "/categories",
            Garde::Droit(Droit::Acces),
            post(categories::creer),
        ),
        entree(
            Method::PATCH,
            "/categories/{id}",
            Garde::Droit(Droit::Acces),
            patch(categories::modifier),
        ),
        entree(
            Method::GET,
            "/rules",
            Garde::Droit(Droit::Acces),
            get(regles::lister),
        ),
        entree(
            Method::POST,
            "/rules",
            Garde::Droit(Droit::Acces),
            post(regles::creer),
        ),
        entree(
            Method::PUT,
            "/rules/order",
            Garde::Droit(Droit::Acces),
            put(regles::ordonner),
        ),
        entree(
            Method::PATCH,
            "/rules/{id}",
            Garde::Droit(Droit::Acces),
            patch(regles::modifier),
        ),
        entree(
            Method::DELETE,
            "/rules/{id}",
            Garde::Droit(Droit::Acces),
            delete(regles::supprimer),
        ),
        entree(
            Method::GET,
            "/merchants",
            Garde::Droit(Droit::Acces),
            get(commercants::lister),
        ),
        entree(
            Method::GET,
            "/merchants/{id}",
            Garde::Droit(Droit::Acces),
            get(commercants::lire),
        ),
        entree(
            Method::PATCH,
            "/merchants/{id}",
            Garde::Droit(Droit::Acces),
            patch(commercants::renommer),
        ),
        entree(
            Method::PUT,
            "/merchants/{id}/category",
            Garde::Droit(Droit::Acces),
            put(commercants::classer),
        ),
        entree(
            Method::POST,
            "/merchants/{id}/merge",
            Garde::Droit(Droit::Acces),
            post(commercants::fusionner),
        ),
        entree(
            Method::POST,
            "/merchants/{id}/split",
            Garde::Droit(Droit::Acces),
            post(commercants::separer),
        ),
        entree(
            Method::GET,
            "/stats/months",
            Garde::Droit(Droit::Acces),
            get(mois::lister),
        ),
        entree(
            Method::GET,
            "/stats/month",
            Garde::Droit(Droit::Acces),
            get(mois::lire),
        ),
        entree(
            Method::GET,
            "/stats/month/categories/{id}",
            Garde::Droit(Droit::Acces),
            get(mois::categorie),
        ),
        entree(
            Method::GET,
            "/stats/history",
            Garde::Droit(Droit::Acces),
            get(mois::historique),
        ),
        entree(
            Method::GET,
            "/stats/balance",
            Garde::Droit(Droit::Acces),
            get(mois::soldes),
        ),
        entree(
            Method::GET,
            "/stats/years",
            Garde::Droit(Droit::Acces),
            get(mois::annees),
        ),
    ]
}

async fn securite(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(
        "x-content-type-options",
        "nosniff".parse().expect("en-tête"),
    );
    h.insert("referrer-policy", "no-referrer".parse().expect("en-tête"));
    r
}

pub fn construire(state: AppState, entrees: Vec<Entree>) -> Router {
    let mut r: Router<AppState> = Router::new();
    for (route, mr) in entrees {
        let couche = from_fn_with_state((state.clone(), route.garde), garde::garder);
        r = r.route(route.chemin, mr.route_layer(couche));
    }
    r.fallback(|| async { crate::http::introuvable() })
        .layer(from_fn_with_state(state.clone(), garde::politique_origine))
        .layer(map_response(securite))
        .with_state(state)
}

pub fn routeur(state: AppState) -> Router {
    construire(state, table())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const OPENAPI: &str = include_str!("../../../docs/tirelire/openapi.yaml");

    type Paires = BTreeSet<(String, String)>;

    fn du_code() -> Paires {
        table()
            .iter()
            .map(|(r, _)| {
                (
                    r.methode.as_str().to_ascii_lowercase(),
                    r.chemin.to_string(),
                )
            })
            .collect()
    }

    fn documentees(texte: &str) -> Paires {
        let doc: serde_json::Value = serde_norway::from_str(texte).expect("openapi.yaml lisible");
        doc["paths"]
            .as_object()
            .expect("paths")
            .iter()
            .flat_map(|(chemin, c)| {
                c.as_object()
                    .into_iter()
                    .flat_map(|o| o.keys())
                    .filter(|k| *k != "parameters")
                    .map(move |m| (m.to_string(), chemin.clone()))
            })
            .collect()
    }

    fn ecarts(code: &Paires, doc: &Paires) -> (Vec<String>, Vec<String>) {
        let texte = |s: &mut dyn Iterator<Item = &(String, String)>| {
            s.map(|(m, c)| format!("{} {c}", m.to_ascii_uppercase()))
                .collect::<Vec<_>>()
        };
        (
            texte(&mut code.difference(doc)),
            texte(&mut doc.difference(code)),
        )
    }

    #[test]
    fn chaque_route_a_une_permission_ou_une_exception_motivee() {
        let mut vues = std::collections::HashSet::new();
        for (route, _) in table() {
            assert!(
                vues.insert((route.methode.clone(), route.chemin)),
                "{} {} en double",
                route.methode,
                route.chemin
            );
            if let Some(m) = route.garde.motif() {
                assert!(
                    m.split_whitespace().count() >= 4,
                    "{} {} : exception sans motif",
                    route.methode,
                    route.chemin
                );
            }
        }
    }

    #[test]
    fn routes_et_openapi_identiques() {
        let (manquantes, en_trop) = ecarts(&du_code(), &documentees(OPENAPI));
        assert!(
            manquantes.is_empty(),
            "routes absentes de docs/tirelire/openapi.yaml : {manquantes:?}"
        );
        assert!(
            en_trop.is_empty(),
            "docs/tirelire/openapi.yaml documente des routes qui n'existent pas : {en_trop:?}"
        );
    }

    #[test]
    fn ecart_detecte_dans_les_deux_sens() {
        let sans_me = OPENAPI.replace("\n  /me:\n", "\n  /moi:\n");
        let (manquantes, en_trop) = ecarts(&du_code(), &documentees(&sans_me));
        assert_eq!(manquantes, ["GET /me"]);
        assert_eq!(en_trop, ["GET /moi"]);
    }
}
