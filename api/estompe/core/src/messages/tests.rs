use std::sync::{Arc, Mutex};

use axum::{
    body::to_bytes,
    http::{HeaderMap, Request},
};
use tokio::sync::mpsc::Receiver;
use tower::ServiceExt;

use super::*;
use crate::{
    identity::Cred,
    servers::tests::{Env, appel, env_avec, invitation_de, membre, serveur},
    state::tests::Enregistreur,
    testutil::Requete,
};

fn autorisation(r: &Requete) -> (u16, Vec<u8>) {
    crate::testutil::reponse_droits(r, |sub, p| p != "estompe:admin" || sub == "sub-admin")
}

pub(crate) struct Cadre {
    pub(crate) e: Env,
    pub(crate) sid: String,
    pub(crate) salon: String,
    pub(crate) vocal: String,
    pub(crate) alice: String,
    pub(crate) bob: String,
}

pub(crate) async fn cadre() -> Option<Cadre> {
    let e = env_avec(autorisation).await?;
    let alice = membre(&e, "alice").await;
    let bob = membre(&e, "bob").await;
    membre(&e, "admin").await;
    let sid = serveur(&e, "Chat", "alice").await;
    let code = invitation_de(&e, "alice", &sid, "").await;
    let chemin = format!("/server-invites/{code}/join");
    assert_eq!(
        appel(&e, "bob", "POST", &chemin, None).await.0,
        StatusCode::OK
    );
    let (_, v) = appel(&e, "alice", "GET", &format!("/servers/{sid}"), None).await;
    let canal = |kind: &str| {
        v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == kind)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (salon, vocal) = (canal("text"), canal("voice"));
    Some(Cadre {
        e,
        sid,
        salon,
        vocal,
        alice,
        bob,
    })
}

pub(crate) fn recus(rx: &mut Receiver<String>) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    while let Ok(m) = rx.try_recv() {
        let v: Value = serde_json::from_str(&m).unwrap();
        out.push((v["type"].as_str().unwrap().to_string(), v["data"].clone()));
    }
    out
}

impl Cadre {
    pub(crate) fn pool(&self) -> &PgPool {
        self.e.state.pool().ok().unwrap()
    }

    pub(crate) fn ecouter(&self, id: &str) -> Receiver<String> {
        let (_, rx, _) = self.e.state.hub().register(
            id,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        rx
    }

    pub(crate) async fn api(
        &self,
        qui: &str,
        methode: &str,
        chemin: &str,
        corps: Value,
    ) -> (u16, Value) {
        let corps = if corps.is_null() {
            None
        } else {
            Some(corps.to_string())
        };
        let (s, v) = appel(&self.e, qui, methode, chemin, corps.as_deref()).await;
        (s.as_u16(), v)
    }

    async fn avec_entetes(&self, qui: &str, chemin: &str, corps: Value) -> (u16, HeaderMap, Value) {
        let r = Request::builder()
            .method("POST")
            .uri(chemin)
            .header(header::AUTHORIZATION, format!("Bearer {qui}"))
            .body(Body::from(corps.to_string()))
            .unwrap();
        let rep = self.e.app.clone().oneshot(r).await.unwrap();
        let (statut, entetes) = (rep.status().as_u16(), rep.headers().clone());
        let octets = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (
            statut,
            entetes,
            serde_json::from_slice(&octets).unwrap_or(Value::Null),
        )
    }

    pub(crate) fn chemin(&self) -> String {
        format!("/channels/{}/messages", self.salon)
    }

    pub(crate) async fn poster(&self, qui: &str, texte: &str) -> Value {
        let (s, v) = self
            .api(qui, "POST", &self.chemin(), json!({ "content": texte }))
            .await;
        assert_eq!(s, 201, "{v}");
        v
    }

    pub(crate) async fn sql(&self, requete: &str) {
        sqlx::query(sqlx::AssertSqlSafe(requete))
            .execute(self.pool())
            .await
            .unwrap();
    }

    pub(crate) async fn fichier(&self, qui: &str, kind: &str) -> String {
        sqlx::query_scalar(
                "INSERT INTO files (uploader_id, name, mime, size, key, kind) VALUES ($1::uuid,'a.txt','text/plain',1,'k',$2) RETURNING id::text",
            )
            .bind(if qui == "alice" { &self.alice } else { &self.bob })
            .bind(kind)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    pub(crate) async fn role(&self, nom: &str, permissions: &[&str], a: &[(&str, &str)]) -> String {
        let corps = json!({ "name": nom, "color": "", "icon": "", "hoist": false, "permissions": permissions });
        let (s, v) = self
            .api(
                "alice",
                "POST",
                &format!("/servers/{}/roles", self.sid),
                corps,
            )
            .await;
        assert_eq!(s, 201, "{v}");
        let id = v["id"].as_str().unwrap().to_string();
        for (_, membre) in a {
            let chemin = format!("/servers/{}/members/{membre}/roles/{id}", self.sid);
            assert_eq!(self.api("alice", "PUT", &chemin, Value::Null).await.0, 204);
        }
        id
    }

    pub(crate) async fn ouvrir_dm(&self, qui: &str, avec: &str) -> String {
        let (s, v) = self
            .api(qui, "POST", "/dms", json!({ "pseudo": avec }))
            .await;
        assert_eq!(s, 200, "{v}");
        v["id"].as_str().unwrap().to_string()
    }
}

#[tokio::test]
async fn salon_et_pagination() {
    let Some(c) = cadre().await else { return };
    let chemin = c.chemin();
    let mut ids = Vec::new();
    for i in 0..5 {
        ids.push(
            c.poster("alice", &format!("msg {i}")).await["id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    let (s, p) = c
        .api("bob", "GET", &format!("{chemin}?limit=3"), Value::Null)
        .await;
    assert_eq!(s, 200);
    let contenus = |p: &Value| -> Vec<String> {
        p["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["content"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(contenus(&p), ["msg 2", "msg 3", "msg 4"]);
    assert_eq!(p["has_more"], true);
    let premier = p["messages"][0]["id"].as_str().unwrap();
    let (_, p) = c
        .api(
            "bob",
            "GET",
            &format!("{chemin}?limit=3&before={premier}"),
            Value::Null,
        )
        .await;
    assert_eq!(contenus(&p), ["msg 0", "msg 1"]);
    assert_eq!(p["has_more"], false);
    assert_eq!(p["messages"][0]["author"]["pseudo"], "alice");
    let (_, p) = c
        .api(
            "bob",
            "GET",
            &format!("{chemin}?after={}&limit=2", ids[1]),
            Value::Null,
        )
        .await;
    assert_eq!(contenus(&p), ["msg 2", "msg 3"]);
    assert_eq!(p["has_more"], true);
    let (s, p) = c
        .api(
            "bob",
            "GET",
            &format!("{chemin}?before=%2B{}", ids[2]),
            Value::Null,
        )
        .await;
    assert_eq!(
        (s, contenus(&p)),
        (200, vec!["msg 0".to_string(), "msg 1".to_string()])
    );
    for (requete, message) in [
        ("limit=0", "limit : 1 à 100"),
        ("limit=101", "limit : 1 à 100"),
        ("limit=x", "limit : 1 à 100"),
        ("before=abc", "before invalide"),
        ("before=-1", "before invalide"),
        ("before=%205", "before invalide"),
        ("after=abc", "after invalide"),
        ("before=abc&after=1", "before invalide"),
    ] {
        let (s, v) = c
            .api("alice", "GET", &format!("{chemin}?{requete}"), Value::Null)
            .await;
        assert_eq!(
            (s, v["error"].as_str(), v["message"].as_str()),
            (400, Some("bad_request"), Some(message)),
            "{requete}"
        );
    }
    for (corps, code) in [
        (json!({ "content": "   " }), "invalid_message"),
        (json!({ "content": "é".repeat(4001) }), "invalid_message"),
    ] {
        let (s, v) = c.api("alice", "POST", &chemin, corps).await;
        assert_eq!((s, v["error"].as_str()), (400, Some(code)));
    }
    let (s, _) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "é".repeat(4000) }),
        )
        .await;
    assert_eq!(s, 201);
    let (s, v) = c
        .api(
            "alice",
            "GET",
            &format!("/channels/{}/messages", c.vocal),
            Value::Null,
        )
        .await;
    assert_eq!((s, v["error"].as_str()), (400, Some("not_text_channel")));
    membre(&c.e, "intrus").await;
    assert_eq!(c.api("intrus", "GET", &chemin, Value::Null).await.0, 404);
    assert_eq!(
        c.api("intrus", "POST", &chemin, json!({ "content": "hello" }))
            .await
            .0,
        404
    );
    assert_eq!(
        c.api("alice", "GET", "/channels/nope/messages", Value::Null)
            .await
            .0,
        404
    );
    c.sql(&format!(
        "UPDATE messages SET pinned_at = now() WHERE id = {}",
        ids[0]
    ))
    .await;
    let (_, p) = c
        .api("bob", "GET", &format!("{chemin}?limit=100"), Value::Null)
        .await;
    assert!(p["messages"][0]["pinned_at"].is_string());
    assert!(p["messages"][1]["pinned_at"].is_null());
    assert_eq!(p["messages"][0]["attachments"], json!([]));
    assert_eq!(p["messages"][0]["reply_to"], Value::Null);
    assert_eq!(p["messages"][0]["system"], false);
}

#[tokio::test]
async fn destinataires_des_evenements() {
    let Some(c) = cadre().await else { return };
    let intrus = membre(&c.e, "intrus").await;
    let mut rx_alice = c.ecouter(&c.alice);
    let mut rx_bob = c.ecouter(&c.bob);
    let mut rx_intrus = c.ecouter(&intrus);
    c.poster("alice", "salut").await;
    for rx in [&mut rx_alice, &mut rx_bob] {
        let evenements = recus(rx);
        assert_eq!(evenements.len(), 1);
        assert_eq!(evenements[0].0, "message.created");
        assert_eq!(evenements[0].1["content"], "salut");
    }
    assert!(recus(&mut rx_intrus).is_empty());
    c.sql(&format!(
            "UPDATE roles SET permissions = permissions & ~(1<<8) WHERE server_id='{}'::uuid AND is_default",
            c.sid
        ))
        .await;
    c.poster("alice", "privé").await;
    assert_eq!(recus(&mut rx_alice).len(), 1);
    assert!(recus(&mut rx_bob).is_empty());
    let (s, _) = c.api("bob", "GET", &c.chemin(), Value::Null).await;
    assert_eq!(s, 404);
}

#[tokio::test]
async fn edition_et_suppression() {
    let Some(c) = cadre().await else { return };
    let mut rx_bob = c.ecouter(&c.bob);
    let m = c.poster("alice", "premier").await;
    let id = m["id"].as_str().unwrap();
    let chemin = format!("/messages/{id}");
    let (s, v) = c
        .api("bob", "PATCH", &chemin, json!({ "content": "x" }))
        .await;
    assert_eq!((s, v["error"].as_str()), (403, Some("forbidden")));
    let (s, v) = c
        .api("alice", "PATCH", &chemin, json!({ "content": " modifié " }))
        .await;
    assert_eq!(s, 200);
    assert_eq!(v["content"], "modifié");
    assert!(v["edited_at"].is_string());
    assert!(
        recus(&mut rx_bob)
            .iter()
            .any(|(t, d)| t == "message.updated" && d["content"] == "modifié")
    );
    assert_eq!(
        c.api("alice", "PATCH", &chemin, json!({ "content": "" }))
            .await
            .1["error"],
        "invalid_message"
    );
    assert_eq!(
        c.api("alice", "PATCH", "/messages/abc", json!({ "content": "x" }))
            .await
            .0,
        404
    );

    let (f1, f2) = (
        c.fichier("alice", "file").await,
        c.fichier("alice", "file").await,
    );
    let (s, v) = c
        .api(
            "alice",
            "PATCH",
            &chemin,
            json!({ "content": "avec", "file_ids": [f1] }),
        )
        .await;
    assert_eq!((s, v["attachments"].as_array().unwrap().len()), (200, 1));
    let (s, v) = c
        .api(
            "alice",
            "PATCH",
            &chemin,
            json!({ "content": "avec", "file_ids": [f2, f1] }),
        )
        .await;
    assert_eq!(s, 200);
    let ordre: Vec<&str> = v["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(ordre, [f2.as_str(), f1.as_str()]);
    let (_, v) = c
        .api("alice", "PATCH", &chemin, json!({ "content": "inchangé" }))
        .await;
    assert_eq!(v["attachments"].as_array().unwrap().len(), 2);
    let (s, v) = c
        .api(
            "alice",
            "PATCH",
            &chemin,
            json!({ "content": "seul", "file_ids": [] }),
        )
        .await;
    assert_eq!((s, v["attachments"].as_array().unwrap().len()), (200, 0));
    let libres: i64 = sqlx::query_scalar("SELECT count(*) FROM files WHERE message_id IS NULL")
        .fetch_one(c.pool())
        .await
        .unwrap();
    assert_eq!(libres, 2);

    let (s, v) = c.api("bob", "DELETE", &chemin, Value::Null).await;
    assert_eq!((s, v["error"].as_str()), (403, Some("forbidden")));
    c.role("Modo", &["manage_messages"], &[("bob", &c.bob)])
        .await;
    recus(&mut rx_bob);
    assert_eq!(c.api("bob", "DELETE", &chemin, Value::Null).await.0, 204);
    let evenements = recus(&mut rx_bob);
    assert_eq!(evenements.len(), 1);
    assert_eq!(evenements[0].0, "message.deleted");
    assert_eq!(
        evenements[0].1,
        json!({ "id": id, "channel_id": c.salon, "dm_id": null })
    );
    let audit: (String, String, String) = sqlx::query_as(
            "SELECT action, target, target_id::text FROM audit_log WHERE server_id=$1::uuid ORDER BY id DESC LIMIT 1",
        )
        .bind(&c.sid)
        .fetch_one(c.pool())
        .await
        .unwrap();
    let nom = nom_du_salon(c.pool(), &c.salon).await;
    assert_eq!(
        audit,
        (
            "message.delete".into(),
            format!("alice dans {nom}"),
            c.alice.clone()
        )
    );
    assert_eq!(c.api("bob", "DELETE", &chemin, Value::Null).await.0, 404);
    let propre = c.poster("bob", "à moi").await;
    let avant: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log")
        .fetch_one(c.pool())
        .await
        .unwrap();
    assert_eq!(
        c.api(
            "bob",
            "DELETE",
            &format!("/messages/{}", propre["id"].as_str().unwrap()),
            Value::Null
        )
        .await
        .0,
        204
    );
    let apres: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log")
        .fetch_one(c.pool())
        .await
        .unwrap();
    assert_eq!(avant, apres);
}

#[tokio::test]
async fn mention_sans_second_acces_au_pool() {
    let Some(c) = cadre().await else { return };
    let grand = pool(&c.e.state).unwrap();
    let petit = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(1))
        .connect_with((*grand.connect_options()).clone())
        .await
        .unwrap();
    let conv = conv_du_salon(&c.e.state, &c.salon, &c.bob).await.unwrap();
    let (_, cibles) = creer(
        &petit,
        c.e.state.hub(),
        &conv,
        &c.bob,
        "salut @alice",
        &[],
        None,
        None,
    )
    .await
    .ok()
    .expect("création");
    assert_eq!(cibles, [(c.alice.clone(), false)]);
    let (id, _) = creer(
        &petit,
        c.e.state.hub(),
        &conv,
        &c.alice,
        "sans",
        &[],
        None,
        None,
    )
    .await
    .ok()
    .expect("création");
    let ajoutes = remplacer(
        &petit,
        c.e.state.hub(),
        &conv,
        id,
        &c.alice,
        "avec @bob",
        None,
    )
    .await
    .ok()
    .expect("édition");
    assert_eq!(ajoutes, [(c.bob.clone(), false)]);
}

#[tokio::test]
async fn mentions_et_notifications() {
    let Some(c) = cadre().await else { return };
    let faux = Arc::new(Enregistreur(Mutex::default()));
    c.e.state.definir_notifieur(faux.clone());
    let mut rx_alice = c.ecouter(&c.alice);
    let mut rx_bob = c.ecouter(&c.bob);
    let m = c.poster("bob", "salut @ALICE et @inconnu").await;
    assert_eq!(m["mentions"], json!([c.alice]));
    let (types, _): (Vec<_>, Vec<_>) = recus(&mut rx_alice).into_iter().unzip();
    assert_eq!(types, ["message.created", "mention"]);
    recus(&mut rx_bob);
    let (s, r) = c
        .api(
            "alice",
            "POST",
            &c.chemin(),
            json!({ "content": "réponse", "reply_to": m["id"] }),
        )
        .await;
    assert_eq!(s, 201);
    assert_eq!(r["reply_to"]["content"], "salut @ALICE et @inconnu");
    assert_eq!(r["reply_to"]["author"]["pseudo"], "bob");
    assert_eq!(
        *faux.0.lock().unwrap(),
        ["salut @ALICE et @inconnu|1|0", "réponse|1|0"]
    );
    recus(&mut rx_bob);
    let (_, v) = c
        .api("alice", "POST", &c.chemin(), json!({ "content": "sans" }))
        .await;
    let id = v["id"].as_str().unwrap().to_string();
    recus(&mut rx_bob);
    let (_, v) = c
        .api(
            "alice",
            "PATCH",
            &format!("/messages/{id}"),
            json!({ "content": "avec @bob" }),
        )
        .await;
    assert_eq!(v["mentions"], json!([c.bob]));
    let types: Vec<String> = recus(&mut rx_bob).into_iter().map(|(t, _)| t).collect();
    assert_eq!(types, ["message.updated", "mention"]);
    assert_eq!(faux.0.lock().unwrap().last().unwrap(), "avec @bob|1|1");
    let avant = faux.0.lock().unwrap().len();
    c.api(
        "alice",
        "PATCH",
        &format!("/messages/{id}"),
        json!({ "content": "encore @bob" }),
    )
    .await;
    let types: Vec<String> = recus(&mut rx_bob).into_iter().map(|(t, _)| t).collect();
    assert_eq!(types, ["message.updated"]);
    assert_eq!(faux.0.lock().unwrap().len(), avant);
    let (_, v) = c
        .api(
            "alice",
            "PATCH",
            &format!("/messages/{id}"),
            json!({ "content": "plus personne" }),
        )
        .await;
    assert_eq!(v["mentions"], json!([]));

    let (_, v) = c
        .api(
            "alice",
            "POST",
            &c.chemin(),
            json!({ "content": "@everyone réveillez-vous" }),
        )
        .await;
    assert_eq!(v["mentions"], json!([c.bob]));
    let (_, v) = c
        .api(
            "bob",
            "POST",
            &c.chemin(),
            json!({ "content": "@everyone" }),
        )
        .await;
    assert_eq!(v["mentions"], json!([]));
    c.role("Équipe", &[], &[("bob", &c.bob)]).await;
    let (_, v) = c
        .api(
            "alice",
            "POST",
            &c.chemin(),
            json!({ "content": "go @équipe !" }),
        )
        .await;
    assert_eq!(v["mentions"], json!([c.bob]));
    let (_, v) = c
        .api(
            "alice",
            "POST",
            &c.chemin(),
            json!({ "content": "go @équipes" }),
        )
        .await;
    assert_eq!(v["mentions"], json!([]));
}

#[tokio::test]
async fn mentions_ici_code_et_debit() {
    let Some(c) = cadre().await else { return };
    let v = c.poster("alice", "@here ?").await;
    assert_eq!(v["mentions"], json!([]));
    let _rx = c.ecouter(&c.bob);
    let v = c.poster("alice", "@here ?").await;
    assert_eq!(v["mentions"], json!([c.bob]));
    let v = c
        .poster("alice", "`@everyone` @bob\n```\n@here @bob\n```")
        .await;
    assert_eq!(v["mentions"], json!([c.bob]));
    let v = c.poster("alice", "`@everyone` ```@bob```").await;
    assert_eq!(v["mentions"], json!([]));
    let v = c.poster("alice", "@bob @everyone").await;
    assert_eq!(v["mentions"], json!([c.bob]));
    let groupe: bool =
        sqlx::query_scalar("SELECT groupe FROM message_mentions WHERE message_id=$1::bigint")
            .bind(v["id"].as_str().unwrap())
            .fetch_one(c.pool())
            .await
            .unwrap();
    assert!(!groupe);
    let v = c.poster("alice", "@everyone").await;
    let groupe: bool =
        sqlx::query_scalar("SELECT groupe FROM message_mentions WHERE message_id=$1::bigint")
            .bind(v["id"].as_str().unwrap())
            .fetch_one(c.pool())
            .await
            .unwrap();
    assert!(groupe);

    c.role("Ping", &["mention_everyone"], &[("bob", &c.bob)])
        .await;
    let v = c.poster("bob", "@everyone").await;
    assert_eq!(v["mentions"], json!([c.alice]));
    let (s, v) = c
        .api("bob", "POST", &c.chemin(), json!({ "content": "@here" }))
        .await;
    assert_eq!((s, v["error"].as_str()), (429, Some("rate_limited")));
    let v = c.poster("bob", "sans groupe").await;
    assert_eq!(v["mentions"], json!([]));
}

#[tokio::test]
async fn reactions() {
    let Some(c) = cadre().await else { return };
    let m = c.poster("alice", "réagissez").await;
    let id = m["id"].as_str().unwrap();
    let pouce = "%F0%9F%91%8D";
    let chemin = format!("/messages/{id}/reactions/{pouce}");
    let mut rx_bob = c.ecouter(&c.bob);
    assert_eq!(c.api("alice", "PUT", &chemin, Value::Null).await.0, 204);
    assert_eq!(c.api("bob", "PUT", &chemin, Value::Null).await.0, 204);
    assert_eq!(c.api("bob", "PUT", &chemin, Value::Null).await.0, 204);
    let evenements = recus(&mut rx_bob);
    assert_eq!(evenements.len(), 3);
    assert_eq!(
        evenements[0],
        (
            "reaction.updated".to_string(),
            json!({ "message_id": id, "emoji": "👍", "user_id": c.alice, "pseudo": "alice", "added": true })
        )
    );
    let (_, p) = c.api("bob", "GET", &c.chemin(), Value::Null).await;
    let r = &p["messages"][0]["reactions"][0];
    assert_eq!(
        (r["emoji"].as_str(), r["count"].as_i64(), r["me"].as_bool()),
        (Some("👍"), Some(2), Some(true))
    );
    assert_eq!(
        r["users"],
        json!([{ "id": c.alice, "pseudo": "alice" }, { "id": c.bob, "pseudo": "bob" }])
    );
    let (_, p) = c.api("alice", "GET", &c.chemin(), Value::Null).await;
    assert_eq!(p["messages"][0]["reactions"][0]["me"], true);
    assert_eq!(c.api("bob", "DELETE", &chemin, Value::Null).await.0, 204);
    assert_eq!(c.api("bob", "DELETE", &chemin, Value::Null).await.0, 204);
    let (_, p) = c.api("bob", "GET", &c.chemin(), Value::Null).await;
    assert_eq!(p["messages"][0]["reactions"][0]["count"], 1);
    assert_eq!(p["messages"][0]["reactions"][0]["me"], false);

    for (emoji, statut) in [
        ("abc", 400),
        ("", 404),
        ("%F0%9F%91%8D%F0%9F%91%8Dab", 400),
        ("1", 400),
    ] {
        let (s, v) = c
            .api(
                "alice",
                "PUT",
                &format!("/messages/{id}/reactions/{emoji}"),
                Value::Null,
            )
            .await;
        assert_eq!(s, statut, "{emoji} {v}");
    }
    assert_eq!(
        c.api(
            "alice",
            "PUT",
            &format!("/messages/{id}/reactions/1%EF%B8%8F%E2%83%A3"),
            Value::Null
        )
        .await
        .0,
        204
    );
    assert_eq!(
        c.api(
            "alice",
            "PUT",
            "/messages/999999/reactions/%F0%9F%91%8D",
            Value::Null
        )
        .await
        .0,
        404
    );
    assert_eq!(
        c.api(
            "alice",
            "PUT",
            "/messages/abc/reactions/%F0%9F%91%8D",
            Value::Null
        )
        .await
        .0,
        404
    );

    c.sql(&format!("UPDATE roles SET permissions = permissions & ~(1<<11) WHERE server_id='{}'::uuid AND is_default", c.sid)).await;
    let (s, v) = c
        .api(
            "bob",
            "PUT",
            &format!("/messages/{id}/reactions/%F0%9F%98%82"),
            Value::Null,
        )
        .await;
    assert_eq!((s, v["error"].as_str()), (403, Some("forbidden")));
    assert_eq!(c.api("bob", "DELETE", &chemin, Value::Null).await.0, 204);
}

#[tokio::test]
async fn plafond_de_reactions() {
    let Some(c) = cadre().await else { return };
    let m = c.poster("alice", "plafond").await;
    let id = m["id"].as_str().unwrap();
    c.sql(&format!(
            "INSERT INTO reactions (message_id, user_id, emoji) SELECT {id}, '{}'::uuid, '😀' || repeat('\u{FE0F}', g) FROM generate_series(1, 20) g",
            c.alice
        ))
        .await;
    let (s, v) = c
        .api(
            "bob",
            "PUT",
            &format!("/messages/{id}/reactions/%F0%9F%91%8D"),
            Value::Null,
        )
        .await;
    assert_eq!((s, v["error"].as_str()), (409, Some("too_many_reactions")));
    let (s, _) = c
        .api(
            "bob",
            "PUT",
            &format!("/messages/{id}/reactions/%F0%9F%98%80%EF%B8%8F"),
            Value::Null,
        )
        .await;
    assert_eq!(s, 204);
}

#[tokio::test]
async fn messages_prives() {
    let Some(c) = cadre().await else { return };
    membre(&c.e, "carol").await;
    let dm = c.ouvrir_dm("alice", "BOB").await;
    let (s, v) = c
        .api("alice", "POST", "/dms", json!({ "pseudo": " bob " }))
        .await;
    assert_eq!((s, v["id"].as_str().unwrap()), (200, dm.as_str()));
    assert_eq!(v["other"]["pseudo"], "bob");
    assert_eq!(v["online"], false);
    assert_eq!(c.ouvrir_dm("bob", "alice").await, dm);
    let (s, v) = c
        .api("alice", "POST", "/dms", json!({ "pseudo": "alice" }))
        .await;
    assert_eq!((s, v["error"].as_str()), (400, Some("self_dm")));
    let (s, v) = c
        .api("alice", "POST", "/dms", json!({ "pseudo": "personne" }))
        .await;
    assert_eq!((s, v["error"].as_str()), (404, Some("user_not_found")));
    let avec_carol = c.ouvrir_dm("alice", "carol").await;
    let (_, l) = c.api("alice", "GET", "/dms", Value::Null).await;
    assert_eq!(l[0]["id"], avec_carol);
    let mut rx_bob = c.ecouter(&c.bob);
    let (s, m) = c
        .api(
            "alice",
            "POST",
            &format!("/dms/{dm}/messages"),
            json!({ "content": "coucou" }),
        )
        .await;
    assert_eq!(s, 201);
    assert_eq!(m["dm_id"], dm);
    assert!(m.get("channel_id").is_none());
    assert_eq!(
        recus(&mut rx_bob)
            .iter()
            .filter(|(t, _)| t == "message.created")
            .count(),
        1
    );
    let (_, l) = c.api("alice", "GET", "/dms", Value::Null).await;
    assert_eq!(l.as_array().unwrap().len(), 2);
    let (_, l) = c.api("bob", "GET", "/dms", Value::Null).await;
    assert_eq!(l[0]["online"], false);
    let (_, l) = c.api("alice", "GET", "/dms", Value::Null).await;
    assert_eq!(l[0]["other"]["pseudo"], "bob");
    assert_eq!(l[0]["online"], true);
    let (s, p) = c
        .api("bob", "GET", &format!("/dms/{dm}/messages"), Value::Null)
        .await;
    assert_eq!((s, p["messages"].as_array().unwrap().len()), (200, 1));
    assert_eq!(
        c.api("carol", "GET", &format!("/dms/{dm}/messages"), Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        c.api(
            "carol",
            "POST",
            &format!("/dms/{dm}/messages"),
            json!({ "content": "x" })
        )
        .await
        .0,
        404
    );
    assert_eq!(
        c.api("alice", "GET", "/dms/nope/messages", Value::Null)
            .await
            .0,
        404
    );

    c.sql(&format!(
            "INSERT INTO dm_conversations (user_a, user_b) SELECT LEAST(id, '{SYSTEME}'::uuid), GREATEST(id, '{SYSTEME}'::uuid) FROM users WHERE id = '{}'::uuid",
            c.alice
        ))
        .await;
    let estompe: String = sqlx::query_scalar(
            "SELECT id::text FROM dm_conversations WHERE user_a = $1::uuid OR user_b = $1::uuid ORDER BY created_at DESC LIMIT 1",
        )
        .bind(SYSTEME)
        .fetch_one(c.pool())
        .await
        .unwrap();
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &format!("/dms/{estompe}/messages"),
            json!({ "content": "x" }),
        )
        .await;
    assert_eq!((s, v["error"].as_str()), (403, Some("read_only")));
    let id = m["id"].as_str().unwrap();
    assert_eq!(
        c.api("bob", "DELETE", &format!("/messages/{id}"), Value::Null)
            .await
            .0,
        204
    );
    let (s, q) = c
        .api(
            "alice",
            "POST",
            &format!("/dms/{dm}/messages"),
            json!({ "content": "Q ?", "poll": { "options": ["a", "b"] } }),
        )
        .await;
    assert_eq!(s, 201, "{q}");
    let clore = format!("/messages/{}/poll/close", q["id"].as_str().unwrap());
    assert_eq!(c.api("bob", "POST", &clore, Value::Null).await.0, 204);
    assert_eq!(c.api("alice", "POST", &clore, Value::Null).await.0, 204);
    assert_eq!(
        c.api(
            "alice",
            "GET",
            &format!("/dms/{estompe}/messages"),
            Value::Null
        )
        .await
        .0,
        200
    );
}

#[tokio::test]
async fn limiteur_d_envoi() {
    let Some(c) = cadre().await else { return };
    for i in 0..ENVOIS {
        c.poster("bob", &format!("{i}")).await;
    }
    let (s, h, v) = c
        .avec_entetes("bob", &c.chemin(), json!({ "content": "de trop" }))
        .await;
    assert_eq!((s, v["error"].as_str()), (429, Some("too_many_messages")));
    assert_eq!(h[header::RETRY_AFTER], "10");
    c.poster("alice", "autre auteur").await;
}

#[tokio::test]
async fn mode_lent() {
    let Some(c) = cadre().await else { return };
    c.sql(&format!(
        "UPDATE channels SET slowmode = 60 WHERE id = '{}'::uuid",
        c.salon
    ))
    .await;
    c.poster("bob", "un").await;
    let (s, h, v) = c
        .avec_entetes("bob", &c.chemin(), json!({ "content": "deux" }))
        .await;
    assert_eq!((s, v["error"].as_str()), (429, Some("slowmode")));
    let attente: u64 = h[header::RETRY_AFTER].to_str().unwrap().parse().unwrap();
    assert!((1..=61).contains(&attente), "{attente}");
    c.poster("alice", "un").await;
    c.poster("alice", "deux").await;
    c.sql(&format!("UPDATE messages SET created_at = now() - interval '2 minutes' WHERE author_id = '{}'::uuid", c.bob)).await;
    c.poster("bob", "trois").await;
}

#[tokio::test]
async fn envoi_valide() {
    let Some(c) = cadre().await else { return };
    let chemin = c.chemin();
    let erreur = |s: u16, v: &Value| format!("{s} {}", v["error"].as_str().unwrap_or(""));
    let onze: Vec<String> = (0..11)
        .map(|_| "0123abcd-0123-4567-89ab-0123456789ab".to_string())
        .collect();
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": onze }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_message");
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": ["pas-un-uuid"] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_files");
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": ["0123abcd-0123-4567-89ab-0123456789ab"] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_files");
    let a = c.fichier("alice", "file").await;
    let (s, v) = c
        .api(
            "bob",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": [a] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_files");
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": [a, a] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_files");
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "", "file_ids": [a] }),
        )
        .await;
    assert_eq!(
        (s, v["attachments"][0]["id"].as_str()),
        (201, Some(a.as_str()))
    );
    assert_eq!(v["attachments"][0]["kind"], "file");
    assert_eq!(v["attachments"][0]["expired"], false);
    assert!(v["attachments"][0].get("duration_ms").is_none());
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": [a] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_files");
    let voix = c.fichier("alice", "voice").await;
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": [voix] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_voice");
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "", "file_ids": [voix] }),
        )
        .await;
    assert_eq!(
        (s, v["attachments"][0]["kind"].as_str()),
        (201, Some("voice"))
    );
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "reply_to": "abc" }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_reply");
    let dm = c.ouvrir_dm("alice", "bob").await;
    let (_, ailleurs) = c
        .api(
            "alice",
            "POST",
            &format!("/dms/{dm}/messages"),
            json!({ "content": "dm" }),
        )
        .await;
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "reply_to": ailleurs["id"] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_reply");
    let (s, v) = c
        .api(
            "alice",
            "POST",
            &chemin,
            json!({ "content": "x", "reply_to": "999999" }),
        )
        .await;
    assert_eq!(erreur(s, &v), "400 invalid_reply");
    for corps in [
        json!({ "content": 3 }),
        json!({ "content": "x", "file_ids": "a" }),
        json!({ "content": "x", "reply_to": 4 }),
    ] {
        assert_eq!(c.api("alice", "POST", &chemin, corps).await.0, 400);
    }
    c.sql(&format!("UPDATE roles SET permissions = permissions & ~(1<<10) WHERE server_id='{}'::uuid AND is_default", c.sid)).await;
    let b = c.fichier("bob", "file").await;
    let (s, v) = c
        .api(
            "bob",
            "POST",
            &chemin,
            json!({ "content": "x", "file_ids": [b] }),
        )
        .await;
    assert_eq!(erreur(s, &v), "403 forbidden");
    c.sql(&format!("UPDATE roles SET permissions = permissions & ~(1<<9) WHERE server_id='{}'::uuid AND is_default", c.sid)).await;
    let (s, v) = c
        .api("bob", "POST", &chemin, json!({ "content": "x" }))
        .await;
    assert_eq!(
        (s, v["message"].as_str()),
        (403, Some("vous ne pouvez pas écrire dans ce salon"))
    );
}

#[tokio::test]
async fn emojis_dans_les_messages() {
    let Some(c) = cadre().await else { return };
    let m = c.poster("alice", "ok :joy: et :perso:").await;
    assert_eq!(m["content"], "ok 😂 et :perso:");
    c.sql(&format!(
            "INSERT INTO custom_emojis (server_id, name, mime, data, created_by) VALUES ('{}'::uuid,'joy','image/png','x','{}'::uuid)",
            c.sid, c.alice
        ))
        .await;
    let m = c.poster("alice", ":joy:").await;
    assert_eq!(m["content"], ":joy:");
    let (_, v) = c
        .api(
            "alice",
            "PATCH",
            &format!("/messages/{}", m["id"].as_str().unwrap()),
            json!({ "content": ":thumbsup:" }),
        )
        .await;
    assert_eq!(v["content"], "👍");
}

#[tokio::test]
async fn hooks_videos() {
    let Some(c) = cadre().await else { return };
    let faux = Arc::new(Enregistreur(Mutex::default()));
    c.e.state.definir_post_video(faux.clone());
    for (nom, kind) in [("clips", "videos"), ("zik", "music")] {
        c.sql(&format!(
            "INSERT INTO channels (server_id, name, kind) VALUES ('{}'::uuid,'{nom}','{kind}')",
            c.sid
        ))
        .await;
        let salon: String = sqlx::query_scalar("SELECT id::text FROM channels WHERE name=$1")
            .bind(nom)
            .fetch_one(c.pool())
            .await
            .unwrap();
        let (s, _) = c
            .api(
                "alice",
                "POST",
                &format!("/channels/{salon}/messages"),
                json!({ "content": format!("dans {kind}") }),
            )
            .await;
        assert_eq!(s, 201);
    }
    c.poster("alice", "dans text").await;
    assert_eq!(
        *faux.0.lock().unwrap(),
        ["videos:\"dans videos\"", "music:\"dans music\""]
    );
}

#[tokio::test]
async fn lecture_croisee_sql() {
    let Some(c) = cadre().await else { return };
    let id: i64 = sqlx::query_scalar(
            "INSERT INTO messages (channel_id, author_id, content) VALUES ($1::uuid,$2::uuid,'écrit par le Go') RETURNING id",
        )
        .bind(&c.salon)
        .bind(&c.alice)
        .fetch_one(c.pool())
        .await
        .unwrap();
    c.sql(&format!(
        "INSERT INTO message_mentions (message_id, user_id) VALUES ({id}, '{}'::uuid)",
        c.bob
    ))
    .await;
    c.sql(&format!(
        "INSERT INTO reactions (message_id, user_id, emoji) VALUES ({id}, '{}'::uuid, '🔥')",
        c.bob
    ))
    .await;
    let f = c.fichier("alice", "voice").await;
    c.sql(&format!("UPDATE files SET message_id = {id}, duration_ms = 1500, waveform = '{{1,2,3}}', transcript = 'bonjour', transcript_auto = true, thumb = 'x' WHERE id = '{f}'::uuid")).await;
    let (_, p) = c.api("bob", "GET", &c.chemin(), Value::Null).await;
    let m = &p["messages"][0];
    assert_eq!(m["id"], id.to_string());
    assert_eq!(m["mentions"], json!([c.bob]));
    assert_eq!(
        m["reactions"][0],
        json!({ "emoji": "🔥", "count": 1, "me": true, "users": [{ "id": c.bob, "pseudo": "bob" }] })
    );
    assert_eq!(
        m["attachments"][0],
        json!({ "id": f, "name": "a.txt", "mime": "text/plain", "size": 1, "expired": false, "kind": "voice",
                    "duration_ms": 1500, "waveform": [1, 2, 3], "thumb": true, "transcript": "bonjour", "transcript_auto": true })
    );
    assert!(m["created_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn epingler_et_desepingler() {
    let Some(c) = cadre().await else { return };
    let mut rb = c.ecouter(&c.bob);
    let mut ra = c.ecouter(&c.alice);
    let m = c.poster("alice", "à garder").await;
    let id = m["id"].as_str().unwrap().to_string();
    let pin = format!("/messages/{id}/pin");
    let pins = format!("/channels/{}/pins", c.salon);
    assert_eq!(c.api("bob", "PUT", &pin, Value::Null).await.0, 403);
    recus(&mut rb);
    recus(&mut ra);
    assert_eq!(c.api("alice", "PUT", &pin, Value::Null).await.0, 204);
    let (s, l) = c.api("bob", "GET", &pins, Value::Null).await;
    assert_eq!(s, 200);
    assert_eq!(l["messages"][0]["id"], json!(id));
    let epingle_le = l["messages"][0]["pinned_at"].clone();
    assert!(epingle_le.as_str().unwrap().ends_with('Z'));
    for rx in [&mut rb, &mut ra] {
        let evenements = recus(rx);
        assert!(
            evenements
                .iter()
                .any(|(t, d)| t == "message.updated" && d["id"] == json!(id))
        );
    }
    assert_eq!(c.api("alice", "PUT", &pin, Value::Null).await.0, 204);
    let (_, l) = c.api("bob", "GET", &pins, Value::Null).await;
    assert_eq!(l["messages"][0]["pinned_at"], epingle_le);
    assert_eq!(
        recus(&mut rb)
            .iter()
            .filter(|(t, _)| t == "message.updated")
            .count(),
        1
    );
    assert_eq!(c.api("alice", "DELETE", &pin, Value::Null).await.0, 204);
    assert_eq!(
        c.api("bob", "GET", &pins, Value::Null).await.1,
        json!({ "messages": [] })
    );
    c.role("modo", &["manage_messages"], &[("", &c.bob)]).await;
    assert_eq!(c.api("bob", "PUT", &pin, Value::Null).await.0, 204);
    assert_eq!(c.api("bob", "DELETE", &pin, Value::Null).await.0, 204);
    assert_eq!(c.api("admin", "PUT", &pin, Value::Null).await.0, 404);
    assert_eq!(
        c.api("alice", "PUT", "/messages/x/pin", Value::Null)
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn epingles_ordre_plafond_et_mp() {
    let Some(c) = cadre().await else { return };
    let premier = c.poster("alice", "un").await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second = c.poster("alice", "deux").await["id"]
        .as_str()
        .unwrap()
        .to_string();
    for (i, id) in [&second, &premier].iter().enumerate() {
        let chemin = format!("/messages/{id}/pin");
        assert_eq!(c.api("alice", "PUT", &chemin, Value::Null).await.0, 204);
        c.sql(&format!(
            "UPDATE messages SET pinned_at = now() + interval '{} seconds' WHERE id={id}",
            i + 1
        ))
        .await;
    }
    let pins = format!("/channels/{}/pins", c.salon);
    let (_, l) = c.api("bob", "GET", &pins, Value::Null).await;
    assert_eq!(l["messages"][0]["id"], json!(premier));
    assert_eq!(l["messages"][1]["id"], json!(second));
    c.sql(&format!(
            "INSERT INTO messages (channel_id, author_id, content, pinned_at)
             SELECT '{}'::uuid, '{}'::uuid, 'm' || g, now() + g * interval '1 second' FROM generate_series(10, 110) g",
            c.salon, c.alice
        ))
        .await;
    let (_, l) = c.api("bob", "GET", &pins, Value::Null).await;
    assert_eq!(l["messages"].as_array().unwrap().len(), 100);
    let dm = c.ouvrir_dm("alice", "bob").await;
    let (_, m) = c
        .api(
            "alice",
            "POST",
            &format!("/dms/{dm}/messages"),
            json!({ "content": "privé" }),
        )
        .await;
    let chemin = format!("/messages/{}/pin", m["id"].as_str().unwrap());
    assert_eq!(c.api("bob", "PUT", &chemin, Value::Null).await.0, 204);
    let (s, l) = c
        .api("alice", "GET", &format!("/dms/{dm}/pins"), Value::Null)
        .await;
    assert_eq!((s, l["messages"].as_array().unwrap().len()), (200, 1));
    assert_eq!(
        c.api("admin", "GET", &format!("/dms/{dm}/pins"), Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        c.api(
            "bob",
            "GET",
            &format!("/channels/{}/pins", c.vocal),
            Value::Null
        )
        .await
        .0,
        400
    );
}

#[tokio::test]
async fn saisie_en_cours() {
    let Some(c) = cadre().await else { return };
    let mut ra = c.ecouter(&c.alice);
    let mut rb = c.ecouter(&c.bob);
    let chemin = format!("/channels/{}/typing", c.salon);
    let debut = std::time::Instant::now();
    assert_eq!(c.api("alice", "POST", &chemin, Value::Null).await.0, 204);
    let evenements = recus(&mut rb);
    let typing: Vec<_> = evenements.iter().filter(|(t, _)| t == "typing").collect();
    assert_eq!(typing.len(), 1);
    assert_eq!(
        typing[0].1,
        json!({ "channel_id": c.salon, "dm_id": null, "user_id": c.alice, "pseudo": "alice" })
    );
    assert!(recus(&mut ra).iter().all(|(t, _)| t != "typing"));
    for _ in 0..2 {
        assert_eq!(c.api("alice", "POST", &chemin, Value::Null).await.0, 204);
    }
    let suivants = recus(&mut rb).iter().filter(|(t, _)| t == "typing").count();
    if debut.elapsed() < FENETRE_SAISIE {
        assert_eq!(suivants, 1);
    } else {
        assert!(suivants <= 2);
    }
    let dm = c.ouvrir_dm("alice", "bob").await;
    let chemin_dm = format!("/dms/{dm}/typing");
    assert_eq!(c.api("alice", "POST", &chemin_dm, Value::Null).await.0, 204);
    let evenements = recus(&mut rb);
    let typing: Vec<_> = evenements.iter().filter(|(t, _)| t == "typing").collect();
    assert_eq!(typing.len(), 1);
    assert_eq!(
        typing[0].1,
        json!({ "channel_id": null, "dm_id": dm, "user_id": c.alice, "pseudo": "alice" })
    );
    assert_eq!(
        c.api(
            "alice",
            "POST",
            &format!("/channels/{}/typing", c.vocal),
            Value::Null
        )
        .await
        .0,
        400
    );
    assert_eq!(c.api("admin", "POST", &chemin, Value::Null).await.0, 404);
    assert_eq!(c.api("admin", "POST", &chemin_dm, Value::Null).await.0, 404);
}
