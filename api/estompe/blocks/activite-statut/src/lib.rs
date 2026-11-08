use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use estompe_core::{
    api::ActiveUser,
    http::{champ_texte, decode, internal, write_err, write_json},
    openapi,
    state::AppState,
    users,
};
use serde_json::{Value, json};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const ACTIVITE_MAX: usize = 64;

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    let r = OpenApiRouter::new()
        .route("/statuses", get(lister_statuts))
        .route("/me/status", put(definir_statut))
        .route("/me/banner", put(definir_banniere))
        .route("/me/activity", put(definir_activite));
    let r = openapi::documenter(
        r,
        "/statuses",
        HttpMethod::Get,
        openapi::operation(
            "comptes",
            "Statuts (idle, dnd) des personnes côtoyées, soi-même compris ; absent = en ligne",
            &[("200", "ok")],
            "",
        ),
    );
    let r = openapi::documenter(
        r,
        "/me/status",
        HttpMethod::Put,
        openapi::operation(
            "comptes",
            "Choisir son statut (online, idle, dnd) ; « dnd » coupe aussi les notifications push",
            &[("204", "ok"), ("400", "invalid_status")],
            "",
        ),
    );
    let r = openapi::documenter(
        r,
        "/me/banner",
        HttpMethod::Put,
        openapi::operation(
            "comptes",
            "Couleur de la bannière du profil (`#rrggbb`, vide pour reprendre la couleur du rôle le plus élevé)",
            &[("204", "ok"), ("400", "invalid_color")],
            "",
        ),
    );
    let r = openapi::documenter(
        r,
        "/me/activity",
        HttpMethod::Put,
        openapi::operation(
            "comptes",
            "Annoncer le jeu en cours (vide pour l'effacer) ; éphémère, effacé à la déconnexion",
            &[
                ("204", "ok"),
                ("400", "invalid_activity"),
                ("409", "offline"),
            ],
            "",
        ),
    );
    router.merge(r)
}

async fn voisins(state: &AppState, id: &str) -> Option<Vec<String>> {
    let pool = state.pool().ok()?;
    let mut ids = users::co_members(pool, id).await.ok()?;
    ids.push(id.to_string());
    Some(ids)
}

fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

async fn lister_statuts(State(state): State<AppState>, ActiveUser(user): ActiveUser) -> Response {
    let Some(ids) = voisins(&state, &user.id).await else {
        return internal();
    };
    let Ok(pool) = state.pool() else {
        return internal();
    };
    let lignes: Result<Vec<(String, String)>, _> = sqlx::query_as(
        "SELECT id::text, status FROM users WHERE id::text = ANY($1) AND status <> 'online'",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await;
    match lignes {
        Ok(l) => write_json(
            StatusCode::OK,
            &Value::Object(l.into_iter().map(|(i, s)| (i, Value::String(s))).collect()),
        ),
        Err(_) => internal(),
    }
}

async fn definir_statut(
    State(state): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(statut) = champ_texte(&objet, "status") else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    };
    if !matches!(statut.as_str(), "online" | "idle" | "dnd") {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_status",
            "statut : online, idle ou dnd",
        );
    }
    let Ok(pool) = state.pool() else {
        return internal();
    };
    if sqlx::query("UPDATE users SET status=$2 WHERE id=$1::uuid")
        .bind(&user.id)
        .bind(&statut)
        .execute(pool)
        .await
        .is_err()
    {
        return internal();
    }
    if let Some(cibles) = voisins(&state, &user.id).await {
        state.publier(
            &cibles,
            "status.updated",
            &json!({ "user_id": user.id, "status": statut }),
        );
    }
    sans_contenu()
}

fn couleur_valide(c: &str) -> bool {
    c.is_empty()
        || (c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit()))
}

async fn definir_banniere(
    State(state): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(couleur) = champ_texte(&objet, "color") else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    };
    if !couleur_valide(&couleur) {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_color",
            "couleur : #rrggbb ou vide",
        );
    }
    let Ok(pool) = state.pool() else {
        return internal();
    };
    if sqlx::query("UPDATE users SET banner_color=$2 WHERE id=$1::uuid")
        .bind(&user.id)
        .bind(&couleur)
        .execute(pool)
        .await
        .is_err()
    {
        return internal();
    }
    if let Some(mut cibles) = voisins(&state, &user.id).await {
        cibles.push(user.id.clone());
        state.publier(
            &cibles,
            "banner.updated",
            &json!({ "user_id": user.id, "color": couleur }),
        );
    }
    sans_contenu()
}

async fn definir_activite(
    State(state): State<AppState>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(brut) = champ_texte(&objet, "name") else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    };
    let nom = brut.trim();
    if nom.chars().count() > ACTIVITE_MAX || nom.chars().any(char::is_control) {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_activity",
            "nom du jeu : 64 caractères maximum, sans caractère de contrôle",
        );
    }
    if !state.hub().set_activity(&user.id, nom) {
        return write_err(StatusCode::CONFLICT, "offline", "temps réel non connecté");
    }
    if let Some(cibles) = voisins(&state, &user.id).await {
        let valeur = if nom.is_empty() {
            Value::Null
        } else {
            json!(nom)
        };
        state.publier(
            &cibles,
            "activity.updated",
            &json!({ "user_id": user.id, "name": valeur }),
        );
    }
    sans_contenu()
}

#[cfg(test)]
mod tests {
    use super::{couleur_valide, mount};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use estompe_core::{
        db::migrate_embarquees,
        identity::Cred,
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux, faux_permission},
    };
    use serde_json::{Value, json};
    use tokio::sync::mpsc::Receiver;
    use tower::ServiceExt;
    use utoipa_axum::router::OpenApiRouter;

    struct Env {
        app: Router,
        state: AppState,
        ids: Vec<String>,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env() -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux_permission(|_, _| true).await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        let mut ids = Vec::new();
        for nom in ["alice", "bob", "carol"] {
            let id: String = sqlx::query_scalar(
                "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
            )
            .bind(format!("sub-{nom}"))
            .bind(nom)
            .fetch_one(&pool)
            .await
            .unwrap();
            ids.push(id);
        }
        let serveur: String = sqlx::query_scalar(
            "INSERT INTO servers (name, owner_id) VALUES ('s', $1::uuid) RETURNING id::text",
        )
        .bind(&ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
        for id in &ids[..2] {
            sqlx::query(
                "INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid, $2::uuid)",
            )
            .bind(&serveur)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("PERMISSION_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let app = Router::from(mount(OpenApiRouter::new())).with_state(state.clone());
        Some(Env {
            app,
            state,
            ids,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn appel(
        app: &Router,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: &str,
    ) -> (StatusCode, Value) {
        let rep = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(methode)
                    .uri(chemin)
                    .header(header::AUTHORIZATION, format!("Bearer {nom}"))
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let s = rep.status();
        let c = to_bytes(rep.into_body(), 1 << 16).await.unwrap();
        (s, serde_json::from_slice(&c).unwrap_or(Value::Null))
    }

    fn connecter(e: &Env, i: usize) -> Receiver<String> {
        let cred = Cred {
            value: "x".into(),
            cookie: false,
        };
        e.state.hub().register(&e.ids[i], cred).1
    }

    async fn recevoir(rx: &mut Receiver<String>, typ: &str) -> Value {
        for _ in 0..20 {
            let m = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .expect("délai")
                .expect("canal fermé");
            let v: Value = serde_json::from_str(&m).unwrap();
            if v["type"] == typ {
                return v["data"].clone();
            }
        }
        panic!("évènement {typ} jamais reçu");
    }

    #[test]
    fn couleurs() {
        assert!(couleur_valide(""));
        assert!(couleur_valide("#aa00FF"));
        for c in ["rouge", "#abc", "aa00FF", "#aa00FG", "#aa00FF0", "#ééééé"] {
            assert!(!couleur_valide(c), "{c}");
        }
    }

    #[tokio::test]
    async fn statuts_visibles_des_voisins_seulement() {
        let Some(e) = env().await else { return };
        assert_eq!(
            appel(&e.app, "alice", "PUT", "/me/status", r#"{"status":"nope"}"#)
                .await
                .1["error"],
            "invalid_status"
        );
        assert_eq!(
            appel(&e.app, "alice", "PUT", "/me/status", "{}").await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            appel(&e.app, "alice", "PUT", "/me/status", r#"{"status":1}"#)
                .await
                .1["error"],
            "bad_request"
        );
        let mut rx = connecter(&e, 0);
        assert_eq!(
            appel(&e.app, "bob", "PUT", "/me/status", r#"{"status":"dnd"}"#)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            recevoir(&mut rx, "status.updated").await,
            json!({ "user_id": e.ids[1], "status": "dnd" })
        );
        assert_eq!(
            appel(&e.app, "carol", "PUT", "/me/status", r#"{"Status":"idle"}"#)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        let (s, v) = appel(&e.app, "alice", "GET", "/statuses", "").await;
        assert_eq!((s, v), (StatusCode::OK, json!({ e.ids[1].clone(): "dnd" })));
        appel(&e.app, "bob", "PUT", "/me/status", r#"{"status":"online"}"#).await;
        assert_eq!(
            appel(&e.app, "alice", "GET", "/statuses", "").await.1,
            json!({})
        );
        assert_eq!(
            appel(&e.app, "carol", "GET", "/statuses", "").await.1,
            json!({ e.ids[2].clone(): "idle" })
        );
    }

    #[tokio::test]
    async fn banniere() {
        let Some(e) = env().await else { return };
        assert_eq!(
            appel(
                &e.app,
                "alice",
                "PUT",
                "/me/banner",
                r##"{"color":"rouge"}"##
            )
            .await
            .1["error"],
            "invalid_color"
        );
        let mut rx = connecter(&e, 1);
        assert_eq!(
            appel(
                &e.app,
                "alice",
                "PUT",
                "/me/banner",
                r##"{"color":"#aa00FF"}"##
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            recevoir(&mut rx, "banner.updated").await,
            json!({ "user_id": e.ids[0], "color": "#aa00FF" })
        );
        let Ok(pool) = e.state.pool() else { panic!() };
        let c: String = sqlx::query_scalar("SELECT banner_color FROM users WHERE id=$1::uuid")
            .bind(&e.ids[0])
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(c, "#aa00FF");
        assert_eq!(
            appel(&e.app, "alice", "PUT", "/me/banner", r#"{"color":""}"#)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(recevoir(&mut rx, "banner.updated").await["color"], "");
    }

    #[tokio::test]
    async fn activite() {
        let Some(e) = env().await else { return };
        let (s, v) = appel(
            &e.app,
            "alice",
            "PUT",
            "/me/activity",
            r#"{"name":"PokeMMO"}"#,
        )
        .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::CONFLICT, Some("offline"))
        );
        let _alice = connecter(&e, 0);
        let mut rx = connecter(&e, 1);
        let long = format!(r#"{{"name":"{}"}}"#, "x".repeat(65));
        for corps in [long.as_str(), r#"{"name":"a\u0000b"}"#] {
            assert_eq!(
                appel(&e.app, "alice", "PUT", "/me/activity", corps).await.1["error"],
                "invalid_activity"
            );
        }
        let accents = format!(r#"{{"name":"{}"}}"#, "é".repeat(64));
        assert_eq!(
            appel(&e.app, "alice", "PUT", "/me/activity", &accents)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        recevoir(&mut rx, "activity.updated").await;
        assert_eq!(
            appel(
                &e.app,
                "alice",
                "PUT",
                "/me/activity",
                r#"{"name":" PokeMMO "}"#
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            recevoir(&mut rx, "activity.updated").await,
            json!({ "user_id": e.ids[0], "name": "PokeMMO" })
        );
        assert_eq!(
            e.state.hub().activities_among(&e.ids),
            [(e.ids[0].clone(), "PokeMMO".to_string())].into()
        );
        assert_eq!(
            appel(&e.app, "alice", "PUT", "/me/activity", r#"{"name":""}"#)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            recevoir(&mut rx, "activity.updated").await,
            json!({ "user_id": e.ids[0], "name": null })
        );
        assert!(e.state.hub().activities_among(&e.ids).is_empty());
    }

    #[tokio::test]
    async fn sans_authentification() {
        let Some(e) = env().await else { return };
        for (m, c) in [
            ("GET", "/statuses"),
            ("PUT", "/me/status"),
            ("PUT", "/me/banner"),
            ("PUT", "/me/activity"),
        ] {
            let rep = e
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(m)
                        .uri(c)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(rep.status(), StatusCode::UNAUTHORIZED, "{c}");
        }
    }
}
