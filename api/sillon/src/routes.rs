use axum::{
    Router,
    http::Method,
    middleware::{from_fn_with_state, map_response},
    response::Response,
    routing::{MethodRouter, get, post, put},
};

use crate::{
    bibliotheque,
    droits::Droit,
    etat::AppState,
    garde::{self, Garde},
    interne,
};

use crate::{lecture, preferences};

const FLUX: Garde = Garde::Jeton(
    "jeton de flux signé, lié à la personne et à la piste, droits vérifiés à chaque requête",
);

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
                "annonce à l'app l'identité, les permissions et les sections, y compris sans sillon:access",
            ),
            get(interne::moi),
        ),
        entree(
            Method::GET,
            "/artists",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::artistes),
        ),
        entree(
            Method::GET,
            "/artists/{id}",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::artiste),
        ),
        entree(
            Method::GET,
            "/artists/{id}/image",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::image_artiste),
        ),
        entree(
            Method::GET,
            "/albums/recent",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::recents),
        ),
        entree(
            Method::GET,
            "/albums/{id}",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::album),
        ),
        entree(
            Method::GET,
            "/albums/{id}/cover",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::pochette),
        ),
        entree(
            Method::GET,
            "/tracks/{id}",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::piste),
        ),
        entree(
            Method::GET,
            "/genres",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::genres),
        ),
        entree(
            Method::GET,
            "/search",
            Garde::Droit(Droit::Acces),
            get(bibliotheque::recherche),
        ),
        entree(
            Method::GET,
            "/admin/status",
            Garde::Droit(Droit::Admin),
            get(bibliotheque::etat),
        ),
        entree(
            Method::GET,
            "/admin/rejected",
            Garde::Droit(Droit::Admin),
            get(bibliotheque::ecartes),
        ),
        entree(
            Method::POST,
            "/admin/sync",
            Garde::Droit(Droit::Admin),
            post(bibliotheque::lancer_synchro),
        ),
        entree(
            Method::POST,
            "/webhooks/lidarr",
            Garde::Webhook(
                "appelé par Lidarr (import, renommage, suppression), jeton SILLON_WEBHOOK_TOKEN",
            ),
            post(bibliotheque::webhook),
        ),
        entree(
            Method::POST,
            "/tracks/{id}/playback",
            Garde::Droit(Droit::Acces),
            post(lecture::ouvrir),
        ),
        entree(Method::GET, "/stream/{jeton}", FLUX, get(lecture::flux)),
        entree(
            Method::GET,
            "/preferences",
            Garde::Droit(Droit::Acces),
            get(preferences::lire),
        ),
        entree(
            Method::PUT,
            "/preferences",
            Garde::Droit(Droit::Acces),
            put(preferences::ecrire),
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

    const OPENAPI: &str = include_str!("../../../docs/sillon/openapi.yaml");

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
    fn exception_sans_motif_refusee() {
        let fautive = Garde::Public("");
        assert!(fautive.motif().unwrap().split_whitespace().count() < 4);
    }

    #[test]
    fn routes_et_openapi_identiques() {
        let (manquantes, en_trop) = ecarts(&du_code(), &documentees(OPENAPI));
        assert!(
            manquantes.is_empty(),
            "routes absentes de docs/sillon/openapi.yaml : {manquantes:?}"
        );
        assert!(
            en_trop.is_empty(),
            "docs/sillon/openapi.yaml documente des routes qui n'existent pas : {en_trop:?}"
        );
    }

    #[test]
    fn ecart_detecte_dans_les_deux_sens() {
        let sans_me = OPENAPI.replace("\n  /me:\n", "\n  /moi:\n");
        let (manquantes, en_trop) = ecarts(&du_code(), &documentees(&sans_me));
        assert_eq!(manquantes, ["GET /me"]);
        assert_eq!(en_trop, ["GET /moi"]);
        let mut code = du_code();
        code.insert(("post".into(), "/me".into()));
        let (manquantes, en_trop) = ecarts(&code, &documentees(OPENAPI));
        assert_eq!(manquantes, ["POST /me"]);
        assert!(en_trop.is_empty());
    }
}
