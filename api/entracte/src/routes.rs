use axum::{
    Router,
    http::Method,
    middleware::{from_fn_with_state, map_response},
    response::Response,
    routing::{MethodRouter, delete, get, post, put},
};

use crate::{
    bibliotheque, demandes,
    droits::Droit::{Acces, Admin, Demande},
    etat::AppState,
    garde::{self, Garde},
    interne, lecture, progression, push,
};

pub struct Route {
    pub methode: Method,
    pub chemin: &'static str,
    pub garde: Garde,
}

const FLUX: Garde = Garde::Jeton(
    "jeton de flux signé, lié à la personne et vérifié contre ses droits à chaque requête",
);

pub fn table() -> Vec<(Route, MethodRouter<AppState>)> {
    let r = |methode: Method, chemin: &'static str, garde: Garde, mr: MethodRouter<AppState>| {
        (
            Route {
                methode,
                chemin,
                garde,
            },
            mr,
        )
    };
    vec![
        r(
            Method::GET,
            "/healthz",
            Garde::Public("santé du processus, sans donnée"),
            get(interne::sante),
        ),
        r(
            Method::GET,
            "/readyz",
            Garde::Public("disponibilité de la base, sans donnée"),
            get(interne::pret),
        ),
        r(
            Method::GET,
            "/me",
            Garde::Session(
                "annonce à l'app l'identité et les permissions, y compris sans entracte:access",
            ),
            get(interne::moi),
        ),
        r(
            Method::GET,
            "/library",
            Garde::Droit(Acces),
            get(bibliotheque::liste),
        ),
        r(
            Method::GET,
            "/genres",
            Garde::Droit(Acces),
            get(bibliotheque::genres),
        ),
        r(
            Method::GET,
            "/titles/{id}",
            Garde::Droit(Acces),
            get(bibliotheque::detail),
        ),
        r(
            Method::GET,
            "/titles/{id}/seasons",
            Garde::Droit(Acces),
            get(bibliotheque::saisons),
        ),
        r(
            Method::GET,
            "/titles/{id}/seasons/{n}/episodes",
            Garde::Droit(Acces),
            get(bibliotheque::episodes),
        ),
        r(
            Method::GET,
            "/titles/{id}/seasons/{n}/image",
            Garde::Droit(Acces),
            get(bibliotheque::image_saison),
        ),
        r(
            Method::GET,
            "/titles/{id}/image",
            Garde::Droit(Acces),
            get(bibliotheque::image),
        ),
        r(
            Method::PUT,
            "/titles/{id}/progress",
            Garde::Droit(Acces),
            put(progression::progression),
        ),
        r(
            Method::POST,
            "/titles/{id}/playback",
            Garde::Droit(Acces),
            post(lecture::ouvrir),
        ),
        r(
            Method::GET,
            "/continue",
            Garde::Droit(Acces),
            get(progression::reprise),
        ),
        r(
            Method::GET,
            "/history",
            Garde::Droit(Acces),
            get(progression::historique),
        ),
        r(
            Method::DELETE,
            "/streams/{id}",
            Garde::Droit(Acces),
            delete(lecture::fermer),
        ),
        r(
            Method::GET,
            "/stream/{jeton}/direct",
            FLUX,
            get(lecture::direct),
        ),
        r(
            Method::GET,
            "/stream/{jeton}/hls.m3u8",
            FLUX,
            get(lecture::hls),
        ),
        r(
            Method::GET,
            "/stream/{jeton}/sub/{n}",
            FLUX,
            get(lecture::sous_titre),
        ),
        r(
            Method::GET,
            "/stream/{jeton}/{segment}",
            FLUX,
            get(lecture::segment),
        ),
        r(
            Method::GET,
            "/search",
            Garde::Droit(Acces),
            get(demandes::rechercher),
        ),
        r(
            Method::GET,
            "/search/series/{tmdb_id}",
            Garde::Droit(Acces),
            get(demandes::saisons_demandables),
        ),
        r(
            Method::GET,
            "/images/tmdb/{nom}",
            Garde::Droit(Acces),
            get(demandes::image_tmdb),
        ),
        r(
            Method::POST,
            "/requests",
            Garde::Droit(Demande),
            post(demandes::creer),
        ),
        r(
            Method::GET,
            "/requests/mine",
            Garde::Droit(Demande),
            get(demandes::miennes),
        ),
        r(
            Method::GET,
            "/admin/requests",
            Garde::Droit(Admin),
            get(demandes::liste_admin),
        ),
        r(
            Method::GET,
            "/admin/requests/options",
            Garde::Droit(Admin),
            get(demandes::options_envoi),
        ),
        r(
            Method::POST,
            "/admin/requests/{id}/accept",
            Garde::Droit(Admin),
            post(demandes::accepter),
        ),
        r(
            Method::POST,
            "/admin/requests/{id}/refuse",
            Garde::Droit(Admin),
            post(demandes::refuser),
        ),
        r(
            Method::GET,
            "/admin/rejected",
            Garde::Droit(Admin),
            get(bibliotheque::ecartes),
        ),
        r(
            Method::GET,
            "/admin/status",
            Garde::Droit(Admin),
            get(bibliotheque::etat),
        ),
        r(
            Method::POST,
            "/admin/sync",
            Garde::Droit(Admin),
            post(bibliotheque::lancer_synchro),
        ),
        r(
            Method::GET,
            "/push/key",
            Garde::Droit(Acces),
            get(push::cle),
        ),
        r(
            Method::PUT,
            "/push/subscription",
            Garde::Droit(Acces),
            put(push::abonner),
        ),
        r(
            Method::DELETE,
            "/push/subscription",
            Garde::Droit(Acces),
            delete(push::desabonner),
        ),
        r(
            Method::PUT,
            "/push/fcm",
            Garde::Droit(Acces),
            put(push::abonner_fcm),
        ),
        r(
            Method::DELETE,
            "/push/fcm",
            Garde::Droit(Acces),
            delete(push::desabonner_fcm),
        ),
        r(
            Method::GET,
            "/notifications",
            Garde::Droit(Acces),
            get(push::notifications),
        ),
        r(
            Method::POST,
            "/webhooks/radarr",
            Garde::Webhook(
                "appelé par Radarr avec le jeton ENTRACTE_WEBHOOK_TOKEN, déclenche une synchronisation",
            ),
            post(bibliotheque::webhook),
        ),
        r(
            Method::POST,
            "/webhooks/sonarr",
            Garde::Webhook(
                "appelé par Sonarr avec le jeton ENTRACTE_WEBHOOK_TOKEN, déclenche une synchronisation",
            ),
            post(bibliotheque::webhook),
        ),
        r(
            Method::GET,
            "/internal/titles/{id}",
            Garde::Service(
                "appel de serveur à serveur d'Estompe (salon cinéma), jeton ENTRACTE_SERVICE_TOKEN",
            ),
            get(interne::titre),
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

pub fn routeur(state: AppState) -> Router {
    let mut r: Router<AppState> = Router::new();
    for (route, mr) in table() {
        let couche = from_fn_with_state((state.clone(), route.garde), garde::garder);
        r = r.route(route.chemin, mr.route_layer(couche));
    }
    r.fallback(|| async { crate::http::introuvable() })
        .layer(from_fn_with_state(state.clone(), garde::politique_origine))
        .layer(map_response(securite))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn routes_documentees_dans_openapi() {
        let texte = include_str!("../../../docs/entracte/openapi.yaml");
        let doc: serde_json::Value = serde_norway::from_str(texte).expect("openapi.yaml lisible");
        let chemins = doc["paths"].as_object().expect("paths");
        for (route, _) in table() {
            let methode = route.methode.as_str().to_ascii_lowercase();
            assert!(
                chemins
                    .get(route.chemin)
                    .and_then(|c| c.get(&methode))
                    .is_some(),
                "{} {} absent de docs/entracte/openapi.yaml",
                route.methode,
                route.chemin
            );
        }
        let nombre: usize = chemins
            .values()
            .map(|c| {
                c.as_object()
                    .map_or(0, |o| o.keys().filter(|k| *k != "parameters").count())
            })
            .sum();
        assert_eq!(
            nombre,
            table().len(),
            "openapi.yaml documente des routes qui n'existent pas"
        );
    }
}
