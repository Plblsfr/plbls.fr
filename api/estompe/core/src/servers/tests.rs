use super::*;
use crate::{
    identity::Cred,
    testutil::{self, BasePostgres, Faux, faux},
};
use axum::{
    Router,
    http::{Request, header},
};
use serde_json::json;
use tower::ServiceExt;

pub(crate) struct Env {
    pub(crate) app: Router,
    pub(crate) state: AppState,
    _g: Faux,
    _p: Faux,
    _pg: BasePostgres,
}

fn gateway(r: &testutil::Requete) -> (u16, Vec<u8>) {
    match r.entete("authorization").strip_prefix("Bearer ") {
        Some(nom) => (
            200,
            json!({ "id": format!("sub-{nom}"), "name": nom })
                .to_string()
                .into_bytes(),
        ),
        None => (401, vec![]),
    }
}

fn permission(r: &testutil::Requete) -> (u16, Vec<u8>) {
    testutil::reponse_droits(r, |_, _| true)
}

pub(crate) async fn env() -> Option<Env> {
    env_avec(permission).await
}

pub(crate) async fn env_avec(
    autorisation: impl Fn(&testutil::Requete) -> (u16, Vec<u8>) + Send + Sync + 'static,
) -> Option<Env> {
    env_avec_blocs(autorisation, vec![], &[]).await
}

pub(crate) async fn env_avec_blocs(
    autorisation: impl Fn(&testutil::Requete) -> (u16, Vec<u8>) + Send + Sync + 'static,
    blocs: Vec<String>,
    extra: &[(&str, &str)],
) -> Option<Env> {
    let g = faux(gateway).await;
    let p = faux(autorisation).await;
    let mut variables = vec![
        ("GATEWAY_URL", g.url.as_str()),
        ("PERMISSION_URL", p.url.as_str()),
    ];
    variables.extend_from_slice(extra);
    let state = AppState::new(testutil::config_avec(&variables), blocs);
    let (pg, url) = testutil::postgres().await?;
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    crate::db::migrate_embarquees(&pool).await.unwrap();
    state.set_pool(pool);
    let app = Router::from(crate::api::router()).with_state(state.clone());
    Some(Env {
        app,
        state,
        _g: g,
        _p: p,
        _pg: pg,
    })
}

pub(crate) async fn appel(
    e: &Env,
    qui: &str,
    methode: &str,
    chemin: &str,
    corps: Option<&str>,
) -> (StatusCode, Value) {
    let r = Request::builder()
        .method(methode)
        .uri(chemin)
        .header(header::AUTHORIZATION, format!("Bearer {qui}"))
        .body(corps.map_or_else(Body::empty, |c| Body::from(c.to_string())))
        .unwrap();
    let rep = e.app.clone().oneshot(r).await.unwrap();
    let statut = rep.status();
    let octets = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
    let v = if octets.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&octets).unwrap()
    };
    (statut, v)
}

pub(crate) async fn membre(e: &Env, nom: &str) -> String {
    let pool = e.state.pool().ok().unwrap();
    sqlx::query_scalar(
        "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
    )
    .bind(format!("sub-{nom}"))
    .bind(nom)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub(crate) async fn serveur(e: &Env, nom: &str, qui: &str) -> String {
    let (s, v) = appel(
        e,
        qui,
        "POST",
        "/servers",
        Some(&json!({ "name": nom }).to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    v["id"].as_str().unwrap().to_string()
}

pub(crate) async fn invitation_de(e: &Env, qui: &str, sid: &str, corps: &str) -> String {
    let (s, v) = appel(
        e,
        qui,
        "POST",
        &format!("/servers/{sid}/invites"),
        Some(corps),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v["code"].as_str().unwrap().to_string()
}

#[test]
fn nettoyage_des_noms() {
    assert_eq!(nom_propre("  a b  ", 5).as_deref(), Some("a b"));
    assert_eq!(nom_propre("   ", 5), None);
    assert_eq!(nom_propre("abcdef", 5), None);
    assert_eq!(nom_propre("éééé", 4).as_deref(), Some("éééé"));
    assert_eq!(nom_propre("a\nb", 5), None);
}

#[test]
fn codes_et_horodatages() {
    let c = nouveau_code();
    assert_eq!(c.len(), 11);
    assert_eq!(c.as_bytes()[5], b'-');
    assert!(
        c.bytes()
            .filter(|b| *b != b'-')
            .all(|b| ALPHABET_CODE.contains(&b))
    );
    assert_eq!(
        rfc3339("2026-01-02T03:04:05.120000"),
        "2026-01-02T03:04:05.12Z"
    );
    assert_eq!(
        rfc3339("2026-01-02T03:04:05.000000"),
        "2026-01-02T03:04:05Z"
    );
    assert!(uuid_valide("0123abcd-0123-4567-89AB-0123456789ab"));
    assert!(!uuid_valide("0123abcd-0123-4567-89ab-0123456789a"));
    assert!(!uuid_valide("pas-un-uuid"));
}

#[tokio::test]
async fn evenements_d_un_salon_prive_reserves_a_ceux_qui_le_voient() {
    use crate::messages::tests::{cadre, recus};
    let Some(c) = cadre().await else { return };
    let base = format!("/servers/{}/channels", c.sid);
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &base,
            json!({ "name": "cache", "kind": "text", "private": true }),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let cache = v["id"].as_str().unwrap().to_string();
    let (mut a, mut b) = (c.ecouter(&c.alice), c.ecouter(&c.bob));
    let types = |l: Vec<(String, Value)>| l.into_iter().map(|(t, _)| t).collect::<Vec<_>>();
    let chemin = format!("{base}/{cache}");
    assert_eq!(
        c.api("alice", "PATCH", &chemin, json!({ "name": "renomme" }))
            .await
            .0,
        204
    );
    assert_eq!(types(recus(&mut a)), ["channel.updated"]);
    assert!(recus(&mut b).is_empty());
    let public = format!("{base}/{}", c.salon);
    assert_eq!(
        c.api("alice", "PATCH", &public, json!({ "name": "accueil" }))
            .await
            .0,
        204
    );
    assert_eq!(types(recus(&mut a)), ["channel.updated"]);
    assert_eq!(types(recus(&mut b)), ["channel.updated"]);
    assert_eq!(c.api("alice", "DELETE", &chemin, Value::Null).await.0, 204);
    assert_eq!(types(recus(&mut a)), ["channel.deleted"]);
    assert!(recus(&mut b).is_empty());
    assert_eq!(c.api("alice", "DELETE", &public, Value::Null).await.0, 204);
    assert_eq!(types(recus(&mut a)), ["channel.deleted"]);
    assert_eq!(types(recus(&mut b)), ["channel.deleted"]);
}

#[tokio::test]
async fn creations_de_serveurs_limitees_par_compte() {
    let Some(e) = env().await else { return };
    membre(&e, "alice").await;
    membre(&e, "bob").await;
    for i in 0..CREATIONS_PAR_FENETRE {
        serveur(&e, &format!("s{i}"), "alice").await;
    }
    let corps = json!({ "name": "de trop" }).to_string();
    let (s, v) = appel(&e, "alice", "POST", "/servers", Some(&corps)).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::TOO_MANY_REQUESTS, Some("rate_limited"))
    );
    serveur(&e, "autre", "bob").await;
}

#[tokio::test]
async fn adhesion_concurrente_sans_erreur() {
    let Some(e) = env().await else { return };
    membre(&e, "alice").await;
    let bob = membre(&e, "bob").await;
    let sid = serveur(&e, "Amis", "alice").await;
    let code = invitation_de(&e, "alice", &sid, "").await;
    let pool = e.state.pool().ok().unwrap().clone();
    let mut autre = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)")
        .bind(&sid)
        .bind(&bob)
        .execute(&mut *autre)
        .await
        .unwrap();
    let (p, b, c) = (pool.clone(), bob.clone(), code.clone());
    let tache = tokio::spawn(async move { rejoindre_base(&p, &b, &c).await });
    loop {
        let bloque: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE 'INSERT INTO server_members%')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if bloque {
            break;
        }
        tokio::task::yield_now().await;
    }
    autre.commit().await.unwrap();
    let (sv, nouveau) = tache.await.unwrap().map_err(|_| "échec").unwrap();
    assert_eq!((sv.id.as_str(), nouveau), (sid.as_str(), false));
    let utilisations: i32 = sqlx::query_scalar("SELECT uses FROM server_invites WHERE code=$1")
        .bind(&code)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(utilisations, 0);
}

#[tokio::test]
async fn serveur_defaut_et_non_membre() {
    let Some(e) = env().await else { return };
    let (s, v) = appel(&e, "alice", "GET", "/servers", None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("not_activated"))
    );
    membre(&e, "alice").await;
    membre(&e, "bob").await;
    let id = serveur(&e, "Les amis", "alice").await;
    let (s, v) = appel(&e, "alice", "GET", &format!("/servers/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["server"]["name"], "Les amis");
    assert_eq!(v["server"]["icon_version"], 0);
    let salons = v["channels"].as_array().unwrap();
    assert_eq!(salons.len(), 2);
    assert_eq!(salons[0]["kind"], "voice");
    assert_eq!(salons[1]["name"], "général");
    assert_eq!(salons[1]["permissions"].as_array().unwrap().len(), 25);
    assert_eq!(v["members"][0]["pseudo"], "alice");
    assert_eq!(v["members"][0]["online"], false);
    assert_eq!(v["members"][0]["roles"], json!([]));
    assert_eq!(v["members"][0]["timeout_until"], Value::Null);
    assert_eq!(v["roles"][0]["name"], "@tout le monde");
    assert_eq!(v["roles"][0]["is_default"], true);
    assert_eq!(v["permissions"].as_array().unwrap().len(), 25);

    for (m, chemin) in [
        ("GET", format!("/servers/{id}")),
        ("PATCH", format!("/servers/{id}")),
        ("DELETE", format!("/servers/{id}")),
        ("POST", format!("/servers/{id}/channels")),
        ("GET", format!("/servers/{id}/invites")),
        ("POST", format!("/servers/{id}/leave")),
        ("GET", "/servers/pas-un-uuid".to_string()),
    ] {
        let (s, v) = appel(&e, "bob", m, &chemin, Some(r#"{"name":"x","kind":"text"}"#)).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::NOT_FOUND, Some("not_found")),
            "{m} {chemin}"
        );
    }
    let (_, v) = appel(&e, "bob", "GET", "/servers", None).await;
    assert_eq!(v, json!([]));
    for corps in [
        r#"{"name":"   "}"#,
        &format!(r#"{{"name":"{}"}}"#, "a".repeat(51)),
    ] {
        let (s, v) = appel(&e, "alice", "POST", "/servers", Some(corps)).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_name"))
        );
    }
    let (s, v) = appel(&e, "alice", "POST", "/servers", Some(r#"{"name":3}"#)).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("bad_request"))
    );
}

#[tokio::test]
async fn rejoindre_quitter_supprimer_et_evenements() {
    let Some(e) = env().await else { return };
    let alice = membre(&e, "alice").await;
    let bob = membre(&e, "bob").await;
    let id = serveur(&e, "S", "alice").await;
    let (_c, mut rx_alice, _) = e.state.hub().register(
        &alice,
        Cred {
            value: "a".into(),
            cookie: false,
        },
    );
    let code = invitation_de(&e, "alice", &id, "").await;

    let (s, v) = appel(&e, "bob", "POST", "/server-invites/inconnu/join", None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("invalid_code"))
    );
    let chemin = format!("/server-invites/{}/join", code.to_lowercase());
    let (s, v) = appel(&e, "bob", "POST", &chemin, None).await;
    assert_eq!((s, v["id"].as_str()), (StatusCode::OK, Some(id.as_str())));
    let msg: Value = serde_json::from_str(&rx_alice.recv().await.unwrap()).unwrap();
    assert_eq!(msg["type"], "member.joined");
    assert_eq!(msg["data"]["member"]["pseudo"], "bob");
    let (s, _) = appel(&e, "bob", "POST", &chemin, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(rx_alice.try_recv().is_err());

    let (s, v) = appel(&e, "bob", "DELETE", &format!("/servers/{id}"), None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
    assert_eq!(v["message"], "réservé au créateur du serveur");
    let (s, v) = appel(
        &e,
        "bob",
        "PATCH",
        &format!("/servers/{id}"),
        Some(r#"{"name":"x"}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
    let (s, v) = appel(&e, "alice", "POST", &format!("/servers/{id}/leave"), None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::CONFLICT, Some("owner_cannot_leave"))
    );

    let (s, _) = appel(
        &e,
        "alice",
        "PATCH",
        &format!("/servers/{id}"),
        Some(r#"{"name":"Nouveau"}"#),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let msg: Value = serde_json::from_str(&rx_alice.recv().await.unwrap()).unwrap();
    assert_eq!(
        (msg["type"].as_str(), msg["data"]["name"].as_str()),
        (Some("server.updated"), Some("Nouveau"))
    );

    let (s, _) = appel(&e, "bob", "POST", &format!("/servers/{id}/leave"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let msg: Value = serde_json::from_str(&rx_alice.recv().await.unwrap()).unwrap();
    assert_eq!(msg["type"], "member.left");
    assert_eq!(msg["data"]["user_id"], bob);
    let (s, _) = appel(&e, "bob", "GET", &format!("/servers/{id}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, _) = appel(&e, "alice", "DELETE", &format!("/servers/{id}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let msg: Value = serde_json::from_str(&rx_alice.recv().await.unwrap()).unwrap();
    assert_eq!(
        (msg["type"].as_str(), msg["data"]["id"].as_str()),
        (Some("server.deleted"), Some(id.as_str()))
    );
    let (_, v) = appel(&e, "alice", "GET", "/servers", None).await;
    assert_eq!(v, json!([]));
}

#[tokio::test]
async fn salons_validation_ordre_et_limite() {
    let Some(e) = env().await else { return };
    membre(&e, "alice").await;
    let id = serveur(&e, "S", "alice").await;
    let base = format!("/servers/{id}/channels");

    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"blabla","kind":"text","topic":"un sujet","slowmode":30}"#),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(
        (
            v["topic"].as_str(),
            v["slowmode"].as_i64(),
            v["locked"].as_bool()
        ),
        (Some("un sujet"), Some(30), Some(false))
    );
    let premier = v["id"].as_str().unwrap().to_string();
    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"BLABLA","kind":"text"}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::CONFLICT, Some("name_taken"))
    );
    let (s, _) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"blabla","kind":"voice","user_limit":4}"#),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    for (corps, code) in [
        (r#"{"name":"","kind":"text"}"#, "invalid_channel"),
        (r#"{"name":"x","kind":"autre"}"#, "invalid_channel"),
        (r#"{"name":"x","kind":"text","topic":"%"}"#, "invalid_topic"),
        (
            r#"{"name":"x","kind":"text","slowmode":4}"#,
            "invalid_slowmode",
        ),
        (
            r#"{"name":"x","kind":"voice","slowmode":10}"#,
            "invalid_slowmode",
        ),
        (
            r#"{"name":"x","kind":"voice","user_limit":11}"#,
            "invalid_user_limit",
        ),
        (
            r#"{"name":"x","kind":"text","user_limit":3}"#,
            "invalid_user_limit",
        ),
        (
            r#"{"name":"x","kind":"text","roles":["a"]}"#,
            "invalid_channel",
        ),
        (
            r#"{"name":"x","kind":"text","private":true,"members":["pas-uuid"]}"#,
            "invalid_channel",
        ),
        (
            r#"{"name":"x","kind":"text","private":true,"roles":["0123abcd-0123-4567-89ab-0123456789ab"]}"#,
            "invalid_channel",
        ),
    ] {
        let corps = corps.replace('%', &"a".repeat(201));
        let (s, v) = appel(&e, "alice", "POST", &base, Some(&corps)).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some(code)),
            "{corps}"
        );
    }
    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"x","kind":"text","slowmode":1.5}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("bad_request"))
    );

    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"juste-apres","kind":"text","after":""}"#),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (_, v) = appel(&e, "alice", "GET", &format!("/servers/{id}"), None).await;
    let textes: Vec<&str> = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "text")
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(textes, ["général", "blabla", "juste-apres"]);

    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(&json!({"name":"entre","kind":"text","after":premier}).to_string()),
    )
    .await;
    assert_eq!((s, v["position"].as_i64()), (StatusCode::CREATED, Some(3)));

    let (_, v) = appel(&e, "alice", "GET", &format!("/servers/{id}"), None).await;
    let mut ids: Vec<String> = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "text")
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    ids.reverse();
    let ordre = format!("{base}/order");
    let (s, _) = appel(
        &e,
        "alice",
        "PUT",
        &ordre,
        Some(&json!({"kind":"text","ids":ids}).to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    ids.pop();
    let (s, v) = appel(
        &e,
        "alice",
        "PUT",
        &ordre,
        Some(&json!({"kind":"text","ids":ids}).to_string()),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_order"))
    );
    let (s, v) = appel(&e, "alice", "PUT", &ordre, Some(r#"{"kind":"x","ids":[]}"#)).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_order"))
    );
    for methode in ["PATCH", "DELETE"] {
        let (s, _) = appel(&e, "alice", methode, &ordre, Some(r#"{"name":"x"}"#)).await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED, "{methode}");
    }

    let un = format!("{base}/{premier}");
    let (s, _) = appel(&e, "alice", "PATCH", &un, Some(r#"{"name":"renomme"}"#)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = appel(&e, "alice", "PATCH", &un, Some(r#"{"name":"général"}"#)).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::CONFLICT, Some("name_taken"))
    );
    let (s, v) = appel(
        &e,
        "alice",
        "PATCH",
        &format!("{base}/0123abcd-0123-4567-89ab-0123456789ab"),
        Some(r#"{"name":"x"}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
    let (s, _) = appel(&e, "alice", "DELETE", &un, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = appel(&e, "alice", "DELETE", &un, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = appel(&e, "alice", "DELETE", &format!("{base}/nope"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let pool = e.state.pool().ok().unwrap();
    sqlx::query("INSERT INTO channels (server_id, name, kind) SELECT $1::uuid, 'c' || g, 'text' FROM generate_series(1, 60) g ON CONFLICT DO NOTHING")
            .bind(&id)
            .execute(pool)
            .await
            .unwrap();
    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"trop","kind":"music"}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::CONFLICT, Some("too_many_channels"))
    );
}

#[test]
fn cinema_avec_entracte_seulement() {
    use crate::testutil::{config, config_avec};
    let blocs = |b: &[&str]| b.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let entracte = || {
        config_avec(&[
            ("ENTRACTE_INTERNE_URL", "http://entracte:8080/"),
            ("ENTRACTE_SERVICE_TOKEN", "t"),
        ])
    };
    assert!(!cinema_disponible(&AppState::new(
        config(),
        blocs(&["regarder-ensemble"])
    )));
    assert!(!cinema_disponible(&AppState::new(
        config(),
        blocs(&["films"])
    )));
    assert!(cinema_disponible(&AppState::new(
        entracte(),
        blocs(&["regarder-ensemble"])
    )));
    assert!(!cinema_disponible(&AppState::new(entracte(), blocs(&[]))));
    assert_eq!(entracte().entracte_url, "http://entracte:8080");
}

#[tokio::test]
async fn salon_cinema_seulement_avec_entracte() {
    let corps = r#"{"name":"ciné","kind":"cinema","user_limit":6}"#;
    let Some(sans) = env().await else { return };
    membre(&sans, "alice").await;
    let id = serveur(&sans, "S", "alice").await;
    let (s, v) = appel(
        &sans,
        "alice",
        "POST",
        &format!("/servers/{id}/channels"),
        Some(corps),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_channel"))
    );

    let Some(e) = env_avec_blocs(
        permission,
        vec!["regarder-ensemble".into()],
        &[
            ("ENTRACTE_INTERNE_URL", "http://entracte:8080"),
            ("ENTRACTE_SERVICE_TOKEN", "t"),
        ],
    )
    .await
    else {
        return;
    };
    membre(&e, "alice").await;
    let id = serveur(&e, "S", "alice").await;
    let base = format!("/servers/{id}/channels");
    let (s, v) = appel(&e, "alice", "POST", &base, Some(corps)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(
        (v["kind"].as_str(), v["user_limit"].as_i64()),
        (Some("cinema"), Some(6))
    );
    let (_, d) = appel(&e, "alice", "GET", &format!("/servers/{id}"), None).await;
    assert!(
        d["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "cinema" && c["name"] == "ciné"),
        "{d}"
    );
    let (s, v) = appel(
        &e,
        "alice",
        "POST",
        &base,
        Some(r#"{"name":"x","kind":"cinema","slowmode":10}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_slowmode"))
    );
    assert!(est_vocal("cinema") && est_vocal("voice") && !est_vocal("text"));
}

#[tokio::test]
async fn salon_prive_et_droits() {
    let Some(e) = env().await else { return };
    membre(&e, "alice").await;
    let bob = membre(&e, "bob").await;
    let id = serveur(&e, "S", "alice").await;
    let code = invitation_de(&e, "alice", &id, "").await;
    appel(
        &e,
        "bob",
        "POST",
        &format!("/server-invites/{code}/join"),
        None,
    )
    .await;
    let base = format!("/servers/{id}/channels");

    let (s, v) = appel(
        &e,
        "bob",
        "POST",
        &base,
        Some(r#"{"name":"x","kind":"text"}"#),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
    let (s, _) = appel(&e, "bob", "GET", &format!("/servers/{id}/invites"), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let corps = json!({"name":"secret","kind":"text","private":true,"members":[bob]}).to_string();
    let (s, v) = appel(&e, "alice", "POST", &base, Some(&corps)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let secret = v["id"].as_str().unwrap().to_string();
    let (_, v) = appel(&e, "bob", "GET", &format!("/servers/{id}"), None).await;
    assert!(
        v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == secret)
    );

    let corps = r#"{"name":"cache","kind":"text","private":true}"#;
    let (_, v) = appel(&e, "alice", "POST", &base, Some(corps)).await;
    let cache = v["id"].as_str().unwrap().to_string();
    let (_, v) = appel(&e, "bob", "GET", &format!("/servers/{id}"), None).await;
    assert!(
        !v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == cache)
    );
    let (_, v) = appel(&e, "alice", "GET", &format!("/servers/{id}"), None).await;
    assert!(
        v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == cache)
    );

    let corps = json!({"name":"x","kind":"text","private":true,"members":["0123abcd-0123-4567-89ab-0123456789ab"]}).to_string();
    let (s, v) = appel(&e, "alice", "POST", &base, Some(&corps)).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_channel"))
    );
    assert_eq!(v["message"], "rôle ou membre introuvable sur ce serveur");
    let (_, v) = appel(&e, "alice", "GET", &format!("/servers/{id}"), None).await;
    assert!(
        !v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "x")
    );
}

#[tokio::test]
async fn invitations_limites_expiration_et_bannissement() {
    let Some(e) = env().await else { return };
    membre(&e, "alice").await;
    let bob = membre(&e, "bob").await;
    membre(&e, "carol").await;
    let id = serveur(&e, "S", "alice").await;
    let base = format!("/servers/{id}/invites");

    for corps in [
        r#"{"max_uses":-1}"#,
        r#"{"max_uses":1001}"#,
        r#"{"expires_in":31536001}"#,
    ] {
        let (s, v) = appel(&e, "alice", "POST", &base, Some(corps)).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_invite")),
            "{corps}"
        );
    }
    let (s, v) = appel(&e, "alice", "POST", &base, Some("pas du json")).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("bad_request"))
    );

    let (s, v) = appel(&e, "alice", "POST", &base, None).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(
        (
            v["created_by"].as_str(),
            v["uses"].as_i64(),
            &v["max_uses"],
            &v["expires_at"]
        ),
        (Some("alice"), Some(0), &Value::Null, &Value::Null)
    );
    assert_eq!(v["code"].as_str().unwrap().len(), 11);

    let une = invitation_de(&e, "alice", &id, r#"{"max_uses":1,"expires_in":3600}"#).await;
    let (_, liste) = appel(&e, "alice", "GET", &base, None).await;
    let l = liste.as_array().unwrap();
    assert_eq!(l.len(), 2);
    let ligne = l.iter().find(|i| i["code"] == une).unwrap();
    assert_eq!(ligne["max_uses"], 1);
    assert!(ligne["expires_at"].as_str().unwrap().ends_with('Z'));

    let (s, _) = appel(
        &e,
        "bob",
        "POST",
        &format!("/server-invites/{une}/join"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = appel(
        &e,
        "carol",
        "POST",
        &format!("/server-invites/{une}/join"),
        None,
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("invalid_code"))
    );
    let (_, liste) = appel(&e, "alice", "GET", &base, None).await;
    assert_eq!(liste.as_array().unwrap().len(), 1);

    let pool = e.state.pool().ok().unwrap();
    let ouverte = liste[0]["code"].as_str().unwrap().to_string();
    sqlx::query("UPDATE server_invites SET expires_at = now() - interval '1 second' WHERE code=$1")
        .bind(&ouverte)
        .execute(pool)
        .await
        .unwrap();
    let (s, _) = appel(
        &e,
        "carol",
        "POST",
        &format!("/server-invites/{ouverte}/join"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE server_invites SET expires_at = NULL WHERE code=$1")
        .bind(&ouverte)
        .execute(pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO server_bans (server_id, user_id, banned_by) SELECT $1::uuid, id, $2::uuid FROM users WHERE pseudo='carol'")
            .bind(&id)
            .bind(&bob)
            .execute(pool)
            .await
            .unwrap();
    let (s, v) = appel(
        &e,
        "carol",
        "POST",
        &format!("/server-invites/{ouverte}/join"),
        None,
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("banned"))
    );
    sqlx::query("UPDATE server_bans SET expires_at = now() - interval '1 second'")
        .execute(pool)
        .await
        .unwrap();
    let (s, _) = appel(
        &e,
        "carol",
        "POST",
        &format!("/server-invites/{ouverte}/join"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, v) = appel(
        &e,
        "alice",
        "DELETE",
        &format!("{base}/{}", ouverte.to_lowercase()),
        None,
    )
    .await;
    assert_eq!((s, v), (StatusCode::NO_CONTENT, Value::Null));
    let (s, v) = appel(&e, "alice", "DELETE", &format!("{base}/{ouverte}"), None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
    assert_eq!(v["message"], "invitation introuvable");
    let (s, _) = appel(
        &e,
        "bob",
        "POST",
        &format!("/server-invites/{ouverte}/join"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
