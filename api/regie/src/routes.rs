use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::Method,
    middleware::{from_fn_with_state, map_response},
    response::Response,
    routing::{MethodRouter, delete, get, post, put},
};

use crate::{
    depots,
    droits::Droit,
    etat::AppState,
    garde::{self, Garde},
    interne, sante, telechargements,
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
            "/me",
            Garde::Session(
                "annonce à l'app l'identité et les permissions, y compris sans regie:admin",
            ),
            get(interne::moi),
        ),
        entree(
            Method::GET,
            "/admin/status",
            Garde::Droit(Droit::Admin),
            get(sante::etat),
        ),
        entree(
            Method::POST,
            "/admin/indexers/{id}/test",
            Garde::Droit(Droit::Admin),
            post(sante::tester_indexeur),
        ),
        entree(
            Method::DELETE,
            "/admin/queue/{service}/{id}",
            Garde::Droit(Droit::Admin),
            delete(sante::retirer),
        ),
        entree(
            Method::GET,
            "/admin/downloads",
            Garde::Droit(Droit::Admin),
            get(telechargements::lister),
        ),
        entree(
            Method::POST,
            "/admin/downloads/pause",
            Garde::Droit(Droit::Admin),
            post(telechargements::suspendre),
        ),
        entree(
            Method::POST,
            "/admin/downloads/resume",
            Garde::Droit(Droit::Admin),
            post(telechargements::reprendre),
        ),
        entree(
            Method::POST,
            "/admin/downloads/recheck",
            Garde::Droit(Droit::Admin),
            post(telechargements::reverifier),
        ),
        entree(
            Method::POST,
            "/admin/downloads/delete",
            Garde::Droit(Droit::Admin),
            post(telechargements::supprimer),
        ),
        entree(
            Method::POST,
            "/admin/downloads/category",
            Garde::Droit(Droit::Admin),
            post(telechargements::categoriser),
        ),
        entree(
            Method::POST,
            "/admin/downloads/tags",
            Garde::Droit(Droit::Admin),
            post(telechargements::etiqueter),
        ),
        entree(
            Method::POST,
            "/admin/downloads/limits",
            Garde::Droit(Droit::Admin),
            post(telechargements::limiter),
        ),
        entree(
            Method::PUT,
            "/admin/transfer",
            Garde::Droit(Droit::Admin),
            put(telechargements::limiter_tout),
        ),
        entree(
            Method::GET,
            "/admin/downloads/{hash}",
            Garde::Droit(Droit::Admin),
            get(telechargements::detail),
        ),
        entree(
            Method::POST,
            "/admin/downloads/{hash}/files",
            Garde::Droit(Droit::Admin),
            post(telechargements::prioriser),
        ),
        entree(
            Method::GET,
            "/admin/deposits",
            Garde::Droit(Droit::Admin),
            get(depots::lister),
        ),
        entree(
            Method::POST,
            "/admin/deposits",
            Garde::Droit(Droit::Admin),
            post(depots::creer).layer(DefaultBodyLimit::max(depots::TORRENT_MAX)),
        ),
        entree(
            Method::GET,
            "/admin/deposits/{id}",
            Garde::Droit(Droit::Admin),
            get(depots::lire),
        ),
        entree(
            Method::DELETE,
            "/admin/deposits/{id}",
            Garde::Droit(Droit::Admin),
            delete(depots::supprimer),
        ),
        entree(
            Method::PUT,
            "/admin/deposits/{id}/files/{index}",
            Garde::Droit(Droit::Admin),
            put(depots::recevoir).layer(DefaultBodyLimit::max(depots::MORCEAU_MAX)),
        ),
        entree(
            Method::POST,
            "/admin/deposits/{id}/seed",
            Garde::Droit(Droit::Admin),
            post(depots::partager),
        ),
        entree(
            Method::GET,
            "/admin/deposits/{id}/import",
            Garde::Droit(Droit::Admin),
            get(depots::proposer_import),
        ),
        entree(
            Method::POST,
            "/admin/deposits/{id}/import",
            Garde::Droit(Droit::Admin),
            post(depots::importer),
        ),
        entree(
            Method::GET,
            "/admin/library/{service}",
            Garde::Droit(Droit::Admin),
            get(depots::bibliotheque),
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

    const OPENAPI: &str = include_str!("../../../docs/regie/openapi.yaml");

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
            "routes absentes de docs/regie/openapi.yaml : {manquantes:?}"
        );
        assert!(
            en_trop.is_empty(),
            "docs/regie/openapi.yaml documente des routes qui n'existent pas : {en_trop:?}"
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
