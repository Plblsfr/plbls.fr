use super::{EvenementMessage, brancher, routes, salons_vocaux};
use crate::livekit::{Lk, signer};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use estompe_core::{
    db::migrate_embarquees,
    identity::Cred,
    roles,
    state::AppState,
    testutil::{self, BasePostgres, Faux, faux, faux_permission},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

#[derive(Default)]
struct Sim {
    presents: HashMap<String, Vec<Value>>,
    permissions: HashMap<String, Value>,
    retires: Vec<String>,
    panne: bool,
}

#[derive(Default)]
struct Notes(Mutex<Vec<String>>);

impl estompe_core::state::Notifieur for Notes {
    fn publier<'a>(
        &'a self,
        e: &'a EvenementMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.0.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                e.vocal,
                e.message["author"]["id"].as_str().unwrap_or_default(),
                e.message["author"]["pseudo"].as_str().unwrap_or_default(),
                e.conv.channel_id.clone().unwrap_or_default()
            ));
        })
    }
}

struct Env {
    app: Router,
    state: AppState,
    sim: Arc<Mutex<Sim>>,
    ids: HashMap<&'static str, String>,
    serveur: String,
    vocal: String,
    vocal2: String,
    texte: String,
    _faux: (Faux, Faux, Faux),
    _pg: BasePostgres,
}

impl Env {
    fn pool(&self) -> &sqlx::PgPool {
        self.state.pool().ok().unwrap()
    }

    fn id(&self, nom: &str) -> String {
        self.ids[nom].clone()
    }

    fn present(&self, salon: &str, identite: &str, nom: &str, pistes: Value) {
        self.sim
            .lock()
            .unwrap()
            .presents
            .entry(format!("channel:{salon}"))
            .or_default()
            .push(json!({ "identity": identite, "name": nom, "tracks": pistes }));
    }

    fn ecouter(&self, nom: &str) -> tokio::sync::mpsc::Receiver<String> {
        let (_, rx, _) = self.state.hub().register(
            &self.id(nom),
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        rx
    }

    async fn sql(&self, requete: &str, args: &[&str]) {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(requete));
        for a in args {
            q = q.bind(*a);
        }
        q.execute(self.pool()).await.unwrap();
    }

    async fn appel(
        &self,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: &str,
    ) -> (StatusCode, Value) {
        let rep = self
            .app
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
        let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (s, serde_json::from_slice(&c).unwrap_or(Value::Null))
    }

    async fn webhook(&self, autorisation: &str, corps: &str) -> StatusCode {
        self.app
            .clone()
            .oneshot(
                Request::post("/voice/webhook")
                    .header(header::AUTHORIZATION, autorisation)
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    fn signe(&self, corps: &str) -> String {
        Lk::new(&self.state.config().livekit).signer_webhook(corps.as_bytes())
    }
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
    let sim: Arc<Mutex<Sim>> = Arc::default();
    let s = sim.clone();
    let livekit = faux(move |r| {
        let mut s = s.lock().unwrap();
        if s.panne {
            return (503, vec![]);
        }
        let entree: Value = serde_json::from_slice(&r.corps).unwrap_or(Value::Null);
        let salle = entree["room"].as_str().unwrap_or_default().to_string();
        let identite = entree["identity"].as_str().unwrap_or_default().to_string();
        let ok = |v: Value| (200, v.to_string().into_bytes());
        match r.chemin.rsplit('/').next().unwrap_or_default() {
            "CreateRoom" => ok(json!({})),
            "ListParticipants" => {
                ok(json!({ "participants": s.presents.get(&salle).cloned().unwrap_or_default() }))
            }
            "RemoveParticipant" => {
                s.retires.push(identite);
                ok(json!({}))
            }
            "UpdateParticipant" => {
                let la = s
                    .presents
                    .get(&salle)
                    .is_some_and(|ps| ps.iter().any(|p| p["identity"] == identite));
                if la {
                    s.permissions.insert(identite, entree["permission"].clone());
                    ok(json!({}))
                } else {
                    (404, vec![])
                }
            }
            _ => (404, vec![]),
        }
    })
    .await;
    let (pg, url) = testutil::postgres().await?;
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    migrate_embarquees(&pool).await.unwrap();
    let mut ids = HashMap::new();
    for nom in ["alice", "bob", "carol", "dave"] {
        let id: String = sqlx::query_scalar(
            "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
        )
        .bind(format!("sub-{nom}"))
        .bind(nom)
        .fetch_one(&pool)
        .await
        .unwrap();
        ids.insert(nom, id);
    }
    let serveur: String = sqlx::query_scalar(
        "INSERT INTO servers (name, owner_id) VALUES ('s', $1::uuid) RETURNING id::text",
    )
    .bind(&ids["alice"])
    .fetch_one(&pool)
    .await
    .unwrap();
    for nom in ["alice", "bob", "carol"] {
        sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)")
            .bind(&serveur)
            .bind(&ids[nom])
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query(
            "INSERT INTO roles (server_id, name, position, permissions, is_default) VALUES ($1::uuid,'@tout le monde',0,$2,true)",
        )
        .bind(&serveur)
        .bind(roles::EVERYONE as i64)
        .execute(&pool)
        .await
        .unwrap();
    let role: String = sqlx::query_scalar(
            "INSERT INTO roles (server_id, name, position, permissions) VALUES ($1::uuid,'Modo',1,$2) RETURNING id::text",
        )
        .bind(&serveur)
        .bind((roles::MUTE_MEMBERS | roles::MOVE_MEMBERS | roles::DISCONNECT_MEMBERS) as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO member_roles (server_id, user_id, role_id) VALUES ($1::uuid,$2::uuid,$3::uuid)")
            .bind(&serveur)
            .bind(&ids["carol"])
            .bind(&role)
            .execute(&pool)
            .await
            .unwrap();
    let mut salons = vec![];
    for (nom, kind) in [("vocal", "voice"), ("vocal2", "voice"), ("texte", "text")] {
        salons.push(
                sqlx::query_scalar::<_, String>(
                    "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid,$2,$3) RETURNING id::text",
                )
                .bind(&serveur)
                .bind(nom)
                .bind(kind)
                .fetch_one(&pool)
                .await
                .unwrap(),
            );
    }
    let state = AppState::new(
        testutil::config_avec(&[
            ("GATEWAY_URL", gateway.url.as_str()),
            ("PERMISSION_URL", permission.url.as_str()),
            ("LIVEKIT_API_URL", livekit.url.as_str()),
            ("LIVEKIT_ALLOW_VIDEO", "true"),
            ("VOICE_MAX_SCREEN_FPS", "30"),
        ]),
        vec![],
    );
    state.set_pool(pool);
    let app = Router::from(routes()).with_state(state.clone());
    Some(Env {
        app,
        state,
        sim,
        ids,
        serveur,
        vocal: salons[0].clone(),
        vocal2: salons[1].clone(),
        texte: salons[2].clone(),
        _faux: (gateway, permission, livekit),
        _pg: pg,
    })
}

fn charge(jeton: &str) -> Value {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(jeton.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap()
}

fn types(rx: &mut tokio::sync::mpsc::Receiver<String>) -> Vec<String> {
    let mut sortie = vec![];
    while let Ok(m) = rx.try_recv() {
        let v: Value = serde_json::from_str(&m).unwrap();
        sortie.push(v["type"].as_str().unwrap().to_string());
    }
    sortie
}

#[tokio::test]
async fn jeton_dans_un_salon_cinema() {
    let Some(e) = env().await else { return };
    let pool = e.state.pool().ok().unwrap();
    let cinema: String = sqlx::query_scalar(
            "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid,'ciné','cinema') RETURNING id::text",
        )
        .bind(&e.serveur)
        .fetch_one(pool)
        .await
        .unwrap();
    let (s, v) = e
        .appel("bob", "POST", &format!("/voice/{cinema}/token"), "")
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["room"], format!("channel:{cinema}"));
    assert_eq!(salons_vocaux(pool, &e.serveur).await.unwrap().len(), 3);
}

#[tokio::test]
async fn jeton_de_salon() {
    let Some(e) = env().await else { return };
    let chemin = format!("/voice/{}/token", e.vocal);
    let (s, v) = e.appel("bob", "POST", &chemin, "").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["url"], "wss://livekit.test");
    assert_eq!(v["room"], format!("channel:{}", e.vocal));
    assert_eq!(v["max_participants"], 10);
    assert_eq!(
        (
            v["screen_max_fps"].as_i64(),
            v["screen_max_height"].as_i64()
        ),
        (Some(30), Some(2160))
    );
    let c = charge(v["token"].as_str().unwrap());
    assert_eq!(
        (c["sub"].as_str(), c["name"].as_str(), c["iss"].as_str()),
        (Some(e.id("bob").as_str()), Some("bob"), Some("cle"))
    );
    assert_eq!(c["video"]["canPublishSources"].as_array().unwrap().len(), 4);

    let (s, v) = e
        .appel("bob", "POST", &format!("{chemin}?screen=1"), "")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        charge(v["token"].as_str().unwrap())["sub"],
        format!("{}#screen", e.id("bob"))
    );
    assert!(v.get("max_participants").is_none());

    for (nom, chemin, attendu) in [
        ("bob", format!("/voice/{}/token", e.texte), 400),
        ("dave", format!("/voice/{}/token", e.vocal), 404),
        ("bob", "/voice/pas-un-uuid/token".to_string(), 404),
    ] {
        assert_eq!(
            e.appel(nom, "POST", &chemin, "").await.0.as_u16(),
            attendu,
            "{chemin}"
        );
    }

    for i in 0..9 {
        e.present(&e.vocal, &format!("autre-{i}"), "x", json!([]));
    }
    e.present(&e.vocal, &e.id("bob"), "bob", json!([]));
    assert_eq!(e.appel("bob", "POST", &chemin, "").await.0, StatusCode::OK);
    let (s, v) = e.appel("carol", "POST", &chemin, "").await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::CONFLICT, Some("voice_full"))
    );
    assert_eq!(
        v["message"],
        "salon vocal complet (10 participants maximum)"
    );

    e.sim.lock().unwrap().panne = true;
    let (s, v) = e.appel("bob", "POST", &chemin, "").await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_GATEWAY, Some("voice_unavailable"))
    );
}

#[tokio::test]
async fn jeton_selon_les_droits_et_la_moderation() {
    let Some(e) = env().await else { return };
    let chemin = format!("/voice/{}/token", e.vocal);
    e.sql(
            "INSERT INTO voice_mod (channel_id, user_id, mic_off, sound_off) VALUES ($1::uuid,$2::uuid,true,true)",
            &[&e.vocal, &e.id("bob")],
        )
        .await;
    let (_, v) = e.appel("bob", "POST", &chemin, "").await;
    let c = charge(v["token"].as_str().unwrap());
    assert_eq!(c["video"]["canSubscribe"], false);
    assert_eq!(
        c["video"]["canPublishSources"],
        json!(["camera", "screen_share", "screen_share_audio"])
    );

    e.sql(
        "UPDATE roles SET permissions=$1::bigint WHERE is_default",
        &[&(roles::VIEW_CHANNEL | roles::CONNECT).to_string()],
    )
    .await;
    let (_, v) = e.appel("carol", "POST", &chemin, "").await;
    assert_eq!(
        charge(v["token"].as_str().unwrap())["video"]["canPublishSources"],
        json!([])
    );
    let (s, _) = e
        .appel("carol", "POST", &format!("{chemin}?screen=1"), "")
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    e.sql(
        "UPDATE roles SET permissions=$1::bigint WHERE is_default",
        &[&roles::VIEW_CHANNEL.to_string()],
    )
    .await;
    let (s, v) = e.appel("bob", "POST", &chemin, "").await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
}

#[tokio::test]
async fn presents_du_serveur() {
    let Some(e) = env().await else { return };
    let chemin = format!("/servers/{}/voice", e.serveur);
    e.present(
        &e.vocal,
        &e.id("bob"),
        "bob",
        json!([{ "source": "SCREEN_SHARE" }, { "source": 1 }]),
    );
    e.present(
        &e.vocal,
        &format!("{}#screen", e.id("carol")),
        "carol",
        json!([]),
    );
    e.present(
        &e.vocal,
        &e.id("carol"),
        "carol",
        json!([{ "source": "MICROPHONE" }]),
    );
    e.sql(
        "INSERT INTO voice_sessions (user_id, channel_id) VALUES ($1::uuid,$2::uuid)",
        &[&e.id("bob"), &e.vocal],
    )
    .await;
    e.sql("INSERT INTO voice_mod (channel_id, user_id, mic_off, sound_off) VALUES ($1::uuid,$2::uuid,true,false)", &[&e.vocal, &e.id("bob")]).await;
    let (s, v) = e.appel("alice", "GET", &chemin, "").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let bob = &v[&e.vocal][0];
    assert_eq!(
        (
            bob["user_id"].as_str(),
            bob["screen"].as_bool(),
            bob["camera"].as_bool(),
            bob["mic_off"].as_bool()
        ),
        (
            Some(e.id("bob").as_str()),
            Some(true),
            Some(true),
            Some(true)
        )
    );
    assert!(bob.get("sound_off").is_none());
    assert!(bob["since"].as_str().unwrap().ends_with('Z'));
    let carol = &v[&e.vocal][1];
    assert_eq!(
        (carol["screen"].as_bool(), carol.get("camera")),
        (Some(true), None)
    );
    assert_eq!(v[&e.vocal].as_array().unwrap().len(), 2);
    assert_eq!(v[&e.vocal2], json!([]));
    assert!(v.get(&e.texte).is_none());

    assert_eq!(
        e.appel("dave", "GET", &chemin, "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.appel("alice", "GET", "/servers/x/voice", "").await.0,
        StatusCode::NOT_FOUND
    );
    e.sim.lock().unwrap().panne = true;
    let (s, v) = e.appel("alice", "GET", &chemin, "").await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_GATEWAY, Some("voice_unavailable"))
    );
}

#[tokio::test]
async fn moderation_du_micro_et_du_son() {
    let Some(e) = env().await else { return };
    let mut rx = e.ecouter("alice");
    let (alice, bob, carol) = (e.id("alice"), e.id("bob"), e.id("carol"));
    e.present(&e.vocal, &bob, "bob", json!([]));
    let chemin = |u: &str| format!("/voice/{}/participants/{u}/mod", e.vocal);
    let mic = r#"{"mic_off":true,"sound_off":false}"#;

    for (nom, cible, corps, attendu) in [
        ("bob", bob.as_str(), mic, 403),
        ("alice", alice.as_str(), mic, 403),
        ("alice", "pas-un-uuid", mic, 403),
        ("alice", bob.as_str(), r#"{"mic_off":"#, 400),
        ("alice", bob.as_str(), r#"{"mic_off":"oui"}"#, 400),
        ("carol", alice.as_str(), mic, 403),
        (
            "carol",
            bob.as_str(),
            r#"{"mic_off":true,"sound_off":true}"#,
            403,
        ),
        ("alice", carol.as_str(), mic, 409),
    ] {
        assert_eq!(
            e.appel(nom, "PUT", &chemin(cible), corps).await.0.as_u16(),
            attendu,
            "{nom} {cible} {corps}"
        );
    }
    assert!(e.sim.lock().unwrap().permissions.is_empty());

    assert_eq!(
        e.appel("carol", "PUT", &chemin(&bob), mic).await.0,
        StatusCode::NO_CONTENT
    );
    {
        let sim = e.sim.lock().unwrap();
        let p = &sim.permissions[&bob];
        assert_eq!(p["canSubscribe"], true);
        assert_eq!(
            p["canPublishSources"],
            json!(["CAMERA", "SCREEN_SHARE", "SCREEN_SHARE_AUDIO"])
        );
    }
    let ligne: (bool, bool) =
        sqlx::query_as("SELECT mic_off, sound_off FROM voice_mod WHERE user_id=$1::uuid")
            .bind(&bob)
            .fetch_one(e.pool())
            .await
            .unwrap();
    assert_eq!(ligne, (true, false));
    assert_eq!(types(&mut rx), ["voice.mod"]);

    assert_eq!(
        e.appel(
            "alice",
            "PUT",
            &chemin(&bob),
            r#"{"mic_off":false,"sound_off":true}"#
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.sim.lock().unwrap().permissions[&bob]["canSubscribe"],
        false
    );
    assert_eq!(
        e.appel("alice", "PUT", &chemin(&bob), "{}").await.0,
        StatusCode::NO_CONTENT
    );
    let restant: i64 = sqlx::query_scalar("SELECT count(*) FROM voice_mod")
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(restant, 0);

    let journal = format!("/servers/{}/voice/moderation-log", e.serveur);
    let (s, v) = e.appel("alice", "GET", &journal, "").await;
    assert_eq!(s, StatusCode::OK);
    let lignes = v.as_array().unwrap();
    assert_eq!(lignes.len(), 3);
    assert_eq!(
        (
            lignes[0]["mic_off"].as_bool(),
            lignes[0]["sound_off"].as_bool(),
            lignes[0]["actor"].as_str(),
            lignes[0]["target"].as_str()
        ),
        (
            Some(false),
            Some(false),
            Some(alice.as_str()),
            Some(bob.as_str())
        )
    );
    assert_eq!(lignes[2]["actor"], carol);
    assert!(lignes[0]["at"].as_str().unwrap().ends_with('Z'));
    for nom in ["bob", "carol"] {
        assert_eq!(
            e.appel(nom, "GET", &journal, "").await.0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        e.appel("dave", "GET", &journal, "").await.0,
        StatusCode::NOT_FOUND
    );

    e.sim.lock().unwrap().panne = true;
    assert_eq!(
        e.appel("alice", "PUT", &chemin(&bob), mic).await.0,
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn webhook_et_temps_passe() {
    let Some(e) = env().await else { return };
    let notes = Arc::new(Notes::default());
    e.state.definir_notifieur(notes.clone());
    let mut rx = e.ecouter("alice");
    let bob = e.id("bob");
    let corps = |ev: &str, id: &str, piste: &str| {
        format!(
            r#"{{"event":"{ev}","room":{{"name":"channel:{}"}},"participant":{{"identity":"{id}","name":"bob"}}{piste}}}"#,
            e.vocal
        )
    };
    let envoyer = |c: String| {
        let auth = e.signe(&c);
        let e = &e;
        async move { e.webhook(&auth, &c).await }
    };

    let arrive = corps("participant_joined", &bob, "");
    assert_eq!(
        e.webhook("faux.jeton.ici", &arrive).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.webhook(&e.signe("autre corps"), &arrive).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(e.webhook("", &arrive).await, StatusCode::UNAUTHORIZED);
    let expire = signer(
        &e.state.config().livekit.secret,
        &json!({ "exp": 1, "sha256": "x" }),
    );
    assert_eq!(e.webhook(&expire, &arrive).await, StatusCode::UNAUTHORIZED);
    assert!(types(&mut rx).is_empty());

    assert_eq!(envoyer(arrive).await, StatusCode::NO_CONTENT);
    assert_eq!(types(&mut rx), ["voice.state"]);
    let (s, v) = e
        .appel(
            "alice",
            "GET",
            &format!("/servers/{}/voice/stats", e.serveur),
            "",
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v[&bob].as_i64().unwrap() <= 1);

    e.sql(
        "UPDATE voice_sessions SET since = now() - interval '1 hour'",
        &[],
    )
    .await;
    let (_, v) = e
        .appel(
            "alice",
            "GET",
            &format!("/servers/{}/voice/stats", e.serveur),
            "",
        )
        .await;
    assert!((3600..=3601).contains(&v[&bob].as_i64().unwrap()));
    assert_eq!(
        envoyer(corps("participant_left", &bob, "")).await,
        StatusCode::NO_CONTENT
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM voice_sessions")
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
    let (_, v) = e
        .appel(
            "alice",
            "GET",
            &format!("/servers/{}/voice/stats", e.serveur),
            "",
        )
        .await;
    assert!((3600..=3601).contains(&v[&bob].as_i64().unwrap()));
    assert_eq!(types(&mut rx), ["voice.state"]);
    e.sql("UPDATE voice_stats SET seconds = 0", &[]).await;

    for (ev, id) in [
        ("participant_joined", bob.as_str()),
        ("participant_joined", "00000000-0000-0000-0000-000000000001"),
    ] {
        assert_eq!(envoyer(corps(ev, id, "")).await, StatusCode::NO_CONTENT);
    }
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM voice_sessions")
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 1);
    types(&mut rx);

    let ecran = corps(
        "track_published",
        &bob,
        r#","track":{"source":"SCREEN_SHARE"}"#,
    );
    assert_eq!(envoyer(ecran).await, StatusCode::NO_CONTENT);
    assert_eq!(types(&mut rx), ["voice.share"]);
    let camera = corps("track_published", &bob, r#","track":{"source":"CAMERA"}"#);
    assert_eq!(envoyer(camera).await, StatusCode::NO_CONTENT);
    let fin = corps(
        "track_unpublished",
        &bob,
        r#","track":{"source":"SCREEN_SHARE"}"#,
    );
    assert_eq!(envoyer(fin).await, StatusCode::NO_CONTENT);
    assert_eq!(
        *notes.0.lock().unwrap(),
        [
            format!("a rejoint|{bob}|bob|{}", e.vocal),
            format!("a rejoint|{bob}|bob|{}", e.vocal),
            format!("partage son écran|{bob}|bob|{}", e.vocal),
            format!("a allumé sa caméra|{bob}|bob|{}", e.vocal),
        ]
    );
    types(&mut rx);
    let micro = corps(
        "track_published",
        &bob,
        r#","track":{"source":"MICROPHONE"}"#,
    );
    assert_eq!(envoyer(micro).await, StatusCode::NO_CONTENT);
    assert!(types(&mut rx).is_empty());
    let natif = format!("{bob}#screen");
    for ev in ["participant_joined", "participant_left"] {
        assert_eq!(envoyer(corps(ev, &natif, "")).await, StatusCode::NO_CONTENT);
    }
    assert_eq!(types(&mut rx), ["voice.share"]);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM voice_sessions")
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 1);
    let autre = r#"{"event":"room_started","room":{"name":"autre"}}"#.to_string();
    assert_eq!(envoyer(autre).await, StatusCode::NO_CONTENT);
    assert!(types(&mut rx).is_empty());
}

#[tokio::test]
async fn deplacer_et_deconnecter() {
    let Some(e) = env().await else { return };
    let mut rx_bob = e.ecouter("bob");
    let (alice, bob) = (e.id("alice"), e.id("bob"));
    let deplacer = |u: &str| format!("/voice/{}/participants/{u}/move", e.vocal);
    let deconnecter = |u: &str| format!("/voice/{}/participants/{u}/disconnect", e.vocal);
    let vers = format!(r#"{{"channel_id":"{}"}}"#, e.vocal2);

    assert_eq!(
        e.appel("carol", "PUT", &deplacer(&bob), &vers).await.0,
        StatusCode::CONFLICT
    );
    e.present(&e.vocal, &bob, "bob", json!([]));
    for (nom, cible, corps, attendu) in [
        ("bob", bob.as_str(), vers.as_str(), 403),
        ("carol", alice.as_str(), vers.as_str(), 403),
        ("carol", "pas-un-uuid", vers.as_str(), 404),
        ("carol", bob.as_str(), r#"{"channel_id":"#, 400),
        ("carol", bob.as_str(), r#"{"channel_id":""}"#, 400),
    ] {
        assert_eq!(
            e.appel(nom, "PUT", &deplacer(cible), corps)
                .await
                .0
                .as_u16(),
            attendu,
            "{nom} {corps}"
        );
    }
    for destination in [
        e.vocal.clone(),
        e.texte.clone(),
        "00000000-0000-0000-0000-0000000000aa".to_string(),
    ] {
        let (s, v) = e
            .appel(
                "carol",
                "PUT",
                &deplacer(&bob),
                &json!({ "channel_id": destination }).to_string(),
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_channel")),
            "{destination}"
        );
    }
    assert!(e.sim.lock().unwrap().retires.is_empty());

    e.sql(
            "INSERT INTO channel_overrides (channel_id, role_id, deny) SELECT $1::uuid, id, $2::bigint FROM roles WHERE is_default",
            &[&e.vocal2, &roles::CONNECT.to_string()],
        )
        .await;
    let (s, v) = e.appel("alice", "PUT", &deplacer(&bob), &vers).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::CONFLICT, Some("cannot_join"))
    );
    e.sql("DELETE FROM channel_overrides", &[]).await;

    assert_eq!(
        e.appel("carol", "PUT", &deplacer(&bob), &vers).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.sim.lock().unwrap().retires,
        [bob.clone(), format!("{bob}#screen")]
    );
    let m = rx_bob.try_recv().unwrap();
    let m: Value = serde_json::from_str(&m).unwrap();
    assert_eq!(m["type"], "voice.moved");
    assert_eq!(
        (
            m["data"]["by"].as_str(),
            m["data"]["to_name"].as_str(),
            m["data"]["to"].as_str()
        ),
        (Some("carol"), Some("vocal2"), Some(e.vocal2.as_str()))
    );
    let audit: (String, String) =
        sqlx::query_as("SELECT action, target FROM audit_log ORDER BY id DESC LIMIT 1")
            .fetch_one(e.pool())
            .await
            .unwrap();
    assert_eq!(
        audit,
        ("voice.move".to_string(), "bob → vocal2".to_string())
    );

    assert_eq!(
        e.appel("bob", "POST", &deconnecter(&bob), "").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.appel("carol", "POST", &deconnecter(&alice), "").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.appel("carol", "POST", &deconnecter(&bob), "").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(types(&mut rx_bob), ["voice.disconnected"]);
    let audit: String = sqlx::query_scalar("SELECT action FROM audit_log ORDER BY id DESC LIMIT 1")
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(audit, "voice.disconnect");
    e.sim.lock().unwrap().panne = true;
    assert_eq!(
        e.appel("carol", "POST", &deconnecter(&bob), "").await.0,
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn accroches_du_coeur() {
    let Some(e) = env().await else { return };
    brancher(&e.state);
    let (bob, carol) = (e.id("bob"), e.id("carol"));
    e.present(&e.vocal, &bob, "bob", json!([]));
    e.present(&e.vocal2, &carol, "carol", json!([]));

    assert_eq!(
        e.state.participants_vocal(&e.vocal).await,
        Some(Ok(vec![json!({ "user_id": bob, "pseudo": "bob" })]))
    );

    e.sql(
        "UPDATE roles SET permissions=$1::bigint WHERE is_default",
        &[&(roles::VIEW_CHANNEL | roles::CONNECT | roles::SPEAK).to_string()],
    )
    .await;
    e.state.droits_modifies(&e.serveur, &bob).await;
    {
        let sim = e.sim.lock().unwrap();
        assert_eq!(
            sim.permissions[&bob]["canPublishSources"],
            json!(["MICROPHONE"])
        );
        assert!(!sim.permissions.contains_key(&carol));
    }
    e.sql(
        "UPDATE roles SET permissions=$1::bigint WHERE is_default",
        &[&roles::VIEW_CHANNEL.to_string()],
    )
    .await;
    e.state.droits_modifies(&e.serveur, "").await;
    assert_eq!(e.sim.lock().unwrap().retires, [bob.clone(), carol.clone()]);

    e.state.sortir_du_vocal(&e.serveur, &carol).await;
    assert_eq!(
        e.sim.lock().unwrap().retires[2..],
        [carol.clone(), format!("{carol}#screen")]
    );
    e.state.acces_retire(&bob).await;
    assert_eq!(e.sim.lock().unwrap().retires.len(), 6);
}
