use axum::{
    Router,
    http::Method,
    middleware::{from_fn_with_state, map_response},
    response::Response,
    routing::{MethodRouter, delete, get, patch, post, put},
};

use crate::{
    admin, battement,
    domaines::{entracte, messagerie, sillon},
    droits::Droit,
    effacement,
    etat::AppState,
    export,
    garde::{self, Garde},
    interne, journal, partage, recaps, series, trophees, utilisation,
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
                "annonce à l'app l'identité, les permissions, les sections et l'intervalle de battement, y compris sans sablier:access",
            ),
            get(interne::moi),
        ),
        entree(
            Method::POST,
            "/heartbeat",
            Garde::Session(
                "le temps d'utilisation est collecté pour toute personne connectée, avec ou sans sablier:access",
            ),
            post(battement::battre),
        ),
        entree(
            Method::GET,
            "/usage",
            Garde::Droit(Droit::Acces),
            get(utilisation::voir),
        ),
        entree(
            Method::GET,
            "/entracte",
            Garde::Droit(Droit::Acces),
            get(entracte::voir),
        ),
        entree(
            Method::GET,
            "/sillon",
            Garde::Droit(Droit::Acces),
            get(sillon::voir),
        ),
        entree(
            Method::GET,
            "/messaging",
            Garde::Droit(Droit::Acces),
            get(messagerie::voir),
        ),
        entree(
            Method::GET,
            "/trophies",
            Garde::Droit(Droit::Acces),
            get(trophees::voir),
        ),
        entree(
            Method::GET,
            "/streaks",
            Garde::Droit(Droit::Acces),
            get(series::voir),
        ),
        entree(
            Method::GET,
            "/unseen",
            Garde::Droit(Droit::Acces),
            get(trophees::voir_non_vus),
        ),
        entree(
            Method::POST,
            "/seen",
            Garde::Droit(Droit::Acces),
            post(trophees::marquer_vus),
        ),
        entree(
            Method::GET,
            "/recaps",
            Garde::Droit(Droit::Acces),
            get(recaps::voir_liste),
        ),
        entree(
            Method::GET,
            "/recaps/{id}",
            Garde::Droit(Droit::Acces),
            get(recaps::voir),
        ),
        entree(
            Method::GET,
            "/sharing",
            Garde::Droit(Droit::Acces),
            get(partage::voir),
        ),
        entree(
            Method::PUT,
            "/sharing",
            Garde::Droit(Droit::Acces),
            put(partage::modifier),
        ),
        entree(
            Method::GET,
            "/profiles/{sub}",
            Garde::Droit(Droit::Acces),
            get(partage::voir_profil),
        ),
        entree(
            Method::GET,
            "/views",
            Garde::Droit(Droit::Acces),
            get(journal::voir_vues),
        ),
        entree(
            Method::GET,
            "/export",
            Garde::Droit(Droit::Acces),
            get(export::exporter),
        ),
        entree(
            Method::DELETE,
            "/data",
            Garde::Droit(Droit::Acces),
            delete(effacement::effacer_les_siennes),
        ),
        entree(
            Method::GET,
            "/admin/trophies",
            Garde::Droit(Droit::Admin),
            get(trophees::admin_lister),
        ),
        entree(
            Method::POST,
            "/admin/trophies",
            Garde::Droit(Droit::Admin),
            post(trophees::admin_creer),
        ),
        entree(
            Method::PATCH,
            "/admin/trophies/{id}",
            Garde::Droit(Droit::Admin),
            patch(trophees::admin_modifier),
        ),
        entree(
            Method::GET,
            "/admin/catalogue",
            Garde::Droit(Droit::Admin),
            get(trophees::admin_catalogue),
        ),
        entree(
            Method::GET,
            "/admin/people",
            Garde::Droit(Droit::Admin),
            get(admin::voir_personnes),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/usage",
            Garde::Droit(Droit::Admin),
            get(admin::voir_utilisation),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/entracte",
            Garde::Droit(Droit::Admin),
            get(admin::voir_entracte),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/sillon",
            Garde::Droit(Droit::Admin),
            get(admin::voir_sillon),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/messaging",
            Garde::Droit(Droit::Admin),
            get(admin::voir_messagerie),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/trophies",
            Garde::Droit(Droit::Admin),
            get(admin::voir_trophees),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/streaks",
            Garde::Droit(Droit::Admin),
            get(admin::voir_series),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/recaps",
            Garde::Droit(Droit::Admin),
            get(admin::voir_recaps),
        ),
        entree(
            Method::GET,
            "/admin/people/{sub}/recaps/{id}",
            Garde::Droit(Droit::Admin),
            get(admin::voir_recap),
        ),
        entree(
            Method::DELETE,
            "/admin/people/{sub}/data",
            Garde::Droit(Droit::Admin),
            delete(admin::effacer),
        ),
        entree(
            Method::GET,
            "/admin/totals",
            Garde::Droit(Droit::Admin),
            get(admin::voir_totaux),
        ),
        entree(
            Method::GET,
            "/admin/audit",
            Garde::Droit(Droit::Admin),
            get(journal::voir_journal),
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

    const OPENAPI: &str = include_str!("../../../docs/sablier/openapi.yaml");

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
            "routes absentes de docs/sablier/openapi.yaml : {manquantes:?}"
        );
        assert!(
            en_trop.is_empty(),
            "docs/sablier/openapi.yaml documente des routes qui n'existent pas : {en_trop:?}"
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

    #[test]
    fn c1_la_personne_visee_n_est_un_parametre_que_sur_les_routes_d_administration() {
        const PROFIL_PUBLIC: &str = "/profiles/{sub}";
        let mut visant_une_personne = 0;
        let mut profil_public = false;
        for (route, _) in table() {
            let admin = route.garde == Garde::Droit(Droit::Admin);
            assert_eq!(
                route.chemin.starts_with("/admin/"),
                admin,
                "{} {}",
                route.methode,
                route.chemin
            );
            if !route.chemin.contains("{sub}") {
                continue;
            }
            if route.chemin == PROFIL_PUBLIC {
                assert_eq!(
                    (route.methode.clone(), route.garde),
                    (Method::GET, Garde::Droit(Droit::Acces))
                );
                profil_public = true;
                continue;
            }
            assert!(
                admin && route.chemin.starts_with("/admin/people/{sub}/"),
                "{} {} vise une personne sans sablier:admin",
                route.methode,
                route.chemin
            );
            visant_une_personne += 1;
        }
        assert!(profil_public);
        assert_eq!(visant_une_personne, 9);
    }
}
