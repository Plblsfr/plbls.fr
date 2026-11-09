use axum::{
    extract::{FromRequestParts, MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

use crate::{api::Auth, catalogue, http::write_err, state::AppState};

pub async fn garde(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(entree) = req
        .extensions()
        .get::<MatchedPath>()
        .and_then(|m| catalogue::garde(req.method(), m.as_str()))
    else {
        return next.run(req).await;
    };
    let (mut parts, corps) = req.into_parts();
    let Ok(Auth(idn)) = Auth::from_request_parts(&mut parts, &state).await else {
        return next.run(Request::from_parts(parts, corps)).await;
    };
    match state.droits_de(&idn).await {
        Err(_) => write_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "permission_unavailable",
            "service de permission injoignable",
        ),
        Ok(d)
            if if entree.serveur {
                d.quelque_part(&entree.nom())
            } else {
                d.a(&entree.nom())
            } =>
        {
            next.run(Request::from_parts(parts, corps)).await
        }
        Ok(_) => write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            &format!("permission {} requise : {}", entree.nom(), entree.libelle),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{roles, testutil};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::Request as Req,
        middleware::from_fn_with_state,
        routing::get,
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    fn passerelle(r: &testutil::Requete) -> (u16, Vec<u8>) {
        match r.entete("authorization").strip_prefix("Bearer ") {
            Some(nom) => (
                200,
                json!({"id": format!("sub-{nom}"), "name": nom, "email": format!("{nom}@plbls.fr"), "roles": []})
                    .to_string()
                    .into_bytes(),
            ),
            None => (401, vec![]),
        }
    }

    fn aegis(r: &testutil::Requete) -> (u16, Vec<u8>) {
        let corps: Value = serde_json::from_slice(&r.corps).unwrap_or_default();
        let permissions = match corps["sub"].as_str() {
            Some("sub-alice") => json!({
                "estompe:gifs.use": {"allow": ["*"], "deny": []},
                "estompe:vocal.screen": {"allow": ["*"], "deny": ["srv-1"]},
                "estompe:messages.send": {"allow": ["srv-2"], "deny": []}
            }),
            _ => json!({}),
        };
        (
            200,
            json!({"sub": corps["sub"], "permissions": permissions})
                .to_string()
                .into_bytes(),
        )
    }

    struct Banc {
        app: Router,
        state: AppState,
        aegis: testutil::Faux,
        _passerelle: testutil::Faux,
    }

    async fn banc() -> Banc {
        let g = testutil::faux(passerelle).await;
        let a = testutil::faux(aegis).await;
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", g.url.as_str()),
                ("AEGIS_URL", a.url.as_str()),
                ("AEGIS_TOKEN", "jeton"),
            ]),
            vec!["gifs".into()],
        );
        let app = Router::new()
            .route("/gifs/search", get(|| async { "ok" }))
            .route("/libre", get(|| async { "ok" }))
            .layer(from_fn_with_state(state.clone(), garde))
            .with_state(state.clone());
        Banc {
            app,
            state,
            aegis: a,
            _passerelle: g,
        }
    }

    async fn get_(app: &Router, chemin: &str, qui: Option<&str>) -> (StatusCode, String) {
        let mut r = Req::get(chemin);
        if let Some(q) = qui {
            r = r.header("authorization", format!("Bearer {q}"));
        }
        let res = app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let s = res.status();
        (
            s,
            String::from_utf8(to_bytes(res.into_body(), 1 << 16).await.unwrap().to_vec()).unwrap(),
        )
    }

    #[tokio::test]
    async fn la_garde_applique_le_catalogue() {
        let b = banc().await;
        assert_eq!(
            get_(&b.app, "/gifs/search", Some("alice")).await.0,
            StatusCode::OK
        );
        let (s, corps) = get_(&b.app, "/gifs/search", Some("bob")).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert!(corps.contains("estompe:gifs.use"), "{corps}");
        assert_eq!(get_(&b.app, "/libre", Some("bob")).await.0, StatusCode::OK);
        assert_eq!(
            get_(&b.app, "/gifs/search", None).await.0,
            StatusCode::OK,
            "le handler juge l'absence d'identité"
        );
        let premiere: Value = serde_json::from_slice(&b.aegis.requetes()[0].corps).unwrap();
        assert_eq!(
            premiere["email"], "alice@plbls.fr",
            "l'annuaire d'aegis reçoit l'e-mail"
        );
    }

    #[tokio::test]
    async fn aegis_injoignable_ferme_la_garde() {
        let g = testutil::faux(passerelle).await;
        let a = testutil::faux(aegis).await;
        let url = a.url.clone();
        drop(a);
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", g.url.as_str()),
                ("AEGIS_URL", url.as_str()),
                ("AEGIS_TOKEN", "x"),
            ]),
            vec![],
        );
        let app = Router::new()
            .route("/gifs/search", get(|| async { "ok" }))
            .layer(from_fn_with_state(state.clone(), garde))
            .with_state(state.clone());
        assert_eq!(
            get_(&app, "/gifs/search", Some("alice")).await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        let tout = catalogue::plafonnees().fold(0, |p, e| p | e.plafond);
        assert_eq!(state.plafond("sub-alice", "srv-1").await, tout);
    }

    #[tokio::test]
    async fn plafond_par_serveur_et_invalidation() {
        let b = banc().await;
        let p1 = b.state.plafond("sub-alice", "srv-1").await;
        let p2 = b.state.plafond("sub-alice", "srv-2").await;
        assert!(
            roles::contient(p1, roles::SHARE_SCREEN) && !roles::contient(p2, roles::SHARE_SCREEN)
        );
        assert!(
            roles::contient(p1, roles::SEND_MESSAGES) && !roles::contient(p2, roles::SEND_MESSAGES)
        );
        assert!(
            roles::contient(p2, roles::CONNECT),
            "vocal.join non accordé"
        );
        let mut a = roles::Access::default();
        a.owner = true;
        a.retire = p1;
        assert!(
            !a.can(roles::SHARE_SCREEN),
            "même le propriétaire est plafonné"
        );
        let n = b.aegis.appels();
        b.state.droits("sub-alice").await.unwrap();
        assert_eq!(b.aegis.appels(), n, "en cache");
        crate::aegis::appliquer(&b.state, r#"{"type":"compte","sub":"sub-alice"}"#).await;
        b.state.droits("sub-alice").await.unwrap();
        assert_eq!(b.aegis.appels(), n + 1, "invalidé");
        crate::aegis::appliquer(&b.state, r#"{"type":"tout","generation":3}"#).await;
        b.state.droits("sub-alice").await.unwrap();
        assert_eq!(b.aegis.appels(), n + 2);
    }
}
