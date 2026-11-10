mod common;

use common::{BasePostgres, Faux, faux, postgres};
use pli::{AppState, Config, app::CSP, db};
use reqwest::{Client, Method, StatusCode};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::net::TcpListener;

struct Serveur {
    base: String,
    pool: PgPool,
    http: Client,
    _faux: Faux,
    _arret: tokio::sync::oneshot::Sender<()>,
    _base: BasePostgres,
}

fn blob(n: usize) -> Vec<u8> {
    let mut b = vec![7u8; n];
    b[0] = 1;
    b
}

async fn faux_gateway() -> Faux {
    faux(|r| {
        let json_rep = |v: Value| (200, vec![("content-type", "application/json".to_string())], serde_json::to_vec(&v).unwrap());
        match r.chemin.as_str() {
            "/auth/me" => {
                let cookie = r.entete("cookie");
                match cookie.strip_prefix("sid=") {
                    Some(qui) if !qui.is_empty() => json_rep(json!({ "id": qui, "name": qui })),
                    _ => (401, vec![], b"{}".to_vec()),
                }
            }
            "/v1/check" => {
                assert_eq!(r.entete("authorization"), "Bearer tok");
                let d: Value = serde_json::from_slice(&r.corps).unwrap();
                json_rep(json!({ "allowed": d["user_id"] == "alice" && d["permission"] == "pli:deposit" }))
            }
            _ => (404, vec![], b"{}".to_vec()),
        }
    })
    .await
}

async fn demarre_avec(extra: &[(&str, &str)]) -> Option<Serveur> {
    let (base_pg, url) = postgres().await?;
    let gw = faux_gateway().await;
    let mut env: Vec<(String, String)> = vec![
        ("PLI_DATABASE_URL".into(), url.clone()),
        ("GATEWAY_INTERNAL_URL".into(), gw.url.clone()),
        ("PERMISSION_URL".into(), gw.url.clone()),
        ("PERMISSION_SERVICE_TOKEN".into(), "tok".into()),
        ("MAX_BYTES".into(), "1000".into()),
        ("MAX_PENDING_PER_USER".into(), "3".into()),
    ];
    for (k, v) in extra {
        env.retain(|(c, _)| c != k);
        if !v.is_empty() {
            env.push((k.to_string(), v.to_string()));
        }
    }
    let cfg =
        Config::from_env(|k| env.iter().find(|(c, _)| c == k).map(|(_, v)| v.clone())).unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    db::migrate(&pool).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let state = AppState::new(cfg, pool.clone());
    tokio::spawn(pli::serve(listener, state, async {
        rx.await.ok();
    }));
    Some(Serveur {
        base,
        pool,
        http: Client::new(),
        _faux: gw,
        _arret: tx,
        _base: base_pg,
    })
}

async fn demarre() -> Option<Serveur> {
    demarre_avec(&[]).await
}

impl Serveur {
    fn req(&self, m: Method, chemin: &str, qui: Option<&str>) -> reqwest::RequestBuilder {
        let r = self.http.request(m, format!("{}{chemin}", self.base));
        match qui {
            Some(q) => r
                .header("cookie", format!("sid={q}"))
                .header("origin", &self.base),
            None => r,
        }
    }

    async fn post(&self, chemin: &str, corps: Vec<u8>, qui: Option<&str>) -> reqwest::Response {
        self.req(Method::POST, chemin, qui)
            .header("content-type", "application/octet-stream")
            .body(corps)
            .send()
            .await
            .unwrap()
    }

    async fn get(&self, chemin: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{chemin}", self.base))
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn depot_401_sans_session_403_sans_permission_refus_autre_origine() {
    let Some(s) = demarre().await else { return };
    let r = s
        .req(Method::POST, "/api/shares?ttl=3600", None)
        .header("origin", &s.base)
        .body(blob(40))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let corps: Value = r.json().await.unwrap();
    assert_eq!(corps["error"], "non authentifié");
    assert_eq!(corps, json!({ "error": "non authentifié" }));
    assert_eq!(
        s.post("/api/shares?ttl=3600", blob(40), Some("bob"))
            .await
            .status(),
        403
    );
    let r = s
        .req(Method::POST, "/api/shares?ttl=3600", None)
        .header("cookie", "sid=alice")
        .header("origin", "https://evil.example")
        .body(blob(40))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({ "error": "origine refusée" })
    );
    let r = s
        .req(Method::POST, "/api/shares?ttl=3600", None)
        .header("cookie", "sid=alice")
        .body(blob(40))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    let r = s
        .req(Method::GET, "/api/me", Some("bob"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({ "error": "permission pli:deposit requise", "user_id": "bob" })
    );
    let r = s
        .req(Method::GET, "/api/me", None)
        .header("cookie", "sid=alice")
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({ "id": "alice", "name": "alice" })
    );
}

#[tokio::test]
async fn depot_non_configure_503() {
    let Some(s) = demarre_avec(&[("PERMISSION_URL", "")]).await else {
        return;
    };
    let r = s
        .post("/api/shares?ttl=3600", blob(40), Some("alice"))
        .await;
    assert_eq!(r.status(), 503);
}

#[tokio::test]
async fn depot_validation_duree_format_taille() {
    let Some(s) = demarre().await else { return };
    assert_eq!(
        s.post("/api/shares?ttl=5", blob(40), Some("alice"))
            .await
            .status(),
        400
    );
    assert_eq!(
        s.post("/api/shares", blob(40), Some("alice"))
            .await
            .status(),
        400
    );
    assert_eq!(
        s.post(
            "/api/shares?ttl=3600",
            b"en clair, trop court".to_vec(),
            Some("alice")
        )
        .await
        .status(),
        400
    );
    let r = s
        .post("/api/shares?ttl=3600", blob(1001), Some("alice"))
        .await;
    assert_eq!(r.status(), 413);
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({ "error": "fichier trop gros" })
    );
}

#[tokio::test]
async fn lecture_unique_meta_sans_consommer_un_seul_post_reussit() {
    let Some(s) = demarre().await else { return };
    let data = blob(60);
    let r = s
        .post("/api/shares?ttl=3600", data.clone(), Some("alice"))
        .await;
    assert_eq!(r.status(), 201);
    let depot: Value = r.json().await.unwrap();
    let id = depot["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 22);
    assert!(
        id.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    );
    assert_eq!(depot["size"], 60);
    let expire = depot["expires_at"].as_str().unwrap();
    assert!(expire.ends_with('Z') && expire.len() == 24, "{expire}");

    assert_eq!(s.get(&format!("/s/{id}")).await.status(), 200);
    for _ in 0..2 {
        let meta: Value = s.get(&format!("/api/s/{id}")).await.json().await.unwrap();
        assert_eq!(meta["size"], 60);
        assert_eq!(meta["expires_at"], depot["expires_at"]);
    }
    assert_eq!(s.get(&format!("/api/s/{id}/consume")).await.status(), 404);
    assert_eq!(s.get(&format!("/api/s/{id}")).await.status(), 200);

    let chemin = format!("/api/s/{id}/consume");
    let (a, b) = tokio::join!(s.post(&chemin, vec![], None), s.post(&chemin, vec![], None));
    let mut statuts = [a.status().as_u16(), b.status().as_u16()];
    statuts.sort();
    assert_eq!(statuts, [200, 404]);
    let gagnant = if a.status() == 200 { a } else { b };
    assert_eq!(
        gagnant.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(gagnant.headers()["cache-control"], "no-store");
    assert_eq!(gagnant.headers()["content-length"], "60");
    assert_eq!(gagnant.headers()["referrer-policy"], "no-referrer");
    assert_eq!(gagnant.bytes().await.unwrap().to_vec(), data);
    assert_eq!(s.get(&format!("/api/s/{id}")).await.status(), 404);
    assert_eq!(s.post(&chemin, vec![], None).await.status(), 404);
}

#[tokio::test]
async fn identifiants_invalides_liste_revocation_quota() {
    let Some(s) = demarre().await else { return };
    let r = s.get("/api/s/nimportequoi").await;
    assert_eq!(r.status(), 404);
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({ "error": "introuvable" })
    );
    let mut ids = Vec::new();
    for _ in 0..3 {
        let r: Value = s
            .post("/api/shares?ttl=86400", blob(40), Some("alice"))
            .await
            .json()
            .await
            .unwrap();
        ids.push(r["id"].as_str().unwrap().to_string());
    }
    let r = s
        .post("/api/shares?ttl=86400", blob(40), Some("alice"))
        .await;
    assert_eq!(r.status(), 429);
    assert_eq!(
        r.json::<Value>().await.unwrap()["error"],
        "3 partages en attente au maximum : supprimez-en"
    );
    let liste: Vec<Value> = s
        .req(Method::GET, "/api/shares", Some("alice"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut vus: Vec<String> = liste
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    vus.sort();
    let mut attendus = ids.clone();
    attendus.sort();
    assert_eq!(vus, attendus);
    let cles: Vec<&String> = liste[0].as_object().unwrap().keys().collect();
    assert_eq!(cles, ["created_at", "expires_at", "id", "size"]);
    let suppr = |qui: &'static str, id: String| {
        s.req(Method::DELETE, &format!("/api/shares/{id}"), Some(qui))
            .send()
    };
    assert_eq!(suppr("bob", ids[0].clone()).await.unwrap().status(), 403);
    let r = suppr("alice", ids[0].clone()).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.json::<Value>().await.unwrap(), json!({ "ok": true }));
    assert_eq!(suppr("alice", ids[0].clone()).await.unwrap().status(), 404);
    assert_eq!(s.get(&format!("/api/s/{}", ids[0])).await.status(), 404);
}

#[tokio::test]
async fn quota_tenu_sous_depots_simultanes_et_au_dela_de_200() {
    let Some(s) = demarre_avec(&[("MAX_PENDING_PER_USER", "250")]).await else {
        return;
    };
    sqlx::query(
        "INSERT INTO shares (id, owner, data, size, expires_at)
         SELECT 'x' || g, 'alice', '\\x01'::bytea, 1, now() + interval '1 hour' FROM generate_series(1, 247) g",
    )
    .execute(&s.pool)
    .await
    .unwrap();
    let s = std::sync::Arc::new(s);
    let mut taches = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let s = s.clone();
        taches.spawn(async move {
            s.post("/api/shares?ttl=3600", blob(40), Some("alice"))
                .await
                .status()
        });
    }
    let mut statuts = Vec::new();
    while let Some(r) = taches.join_next().await {
        statuts.push(r.unwrap());
    }
    assert_eq!(
        statuts
            .iter()
            .filter(|c| **c == StatusCode::CREATED)
            .count(),
        3,
        "{statuts:?}"
    );
    assert_eq!(
        statuts
            .iter()
            .filter(|c| **c == StatusCode::TOO_MANY_REQUESTS)
            .count(),
        5
    );
    assert_eq!(db::count_pending(&s.pool, "alice").await.unwrap(), 250);
    assert_eq!(db::list_shares(&s.pool, "alice").await.unwrap().len(), 250);
}

#[tokio::test]
async fn entetes_de_securite_statiques_et_pas_de_cache_sur_api() {
    let Some(s) = demarre().await else { return };
    let r = s.get("/").await;
    assert_eq!(r.status(), 200);
    let h = r.headers();
    assert_eq!(h["content-security-policy"], CSP);
    assert!(CSP.contains("frame-ancestors 'none'"));
    assert_eq!(h["referrer-policy"], "no-referrer");
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["x-frame-options"], "DENY");
    assert_eq!(h["content-type"], "text/html; charset=utf-8");
    assert_eq!(h["cache-control"], "no-cache, max-age=0, must-revalidate");
    let index = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/pli/public/index.html"
    ))
    .unwrap();
    assert_eq!(r.bytes().await.unwrap().to_vec(), index);

    let s_html = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/pli/public/s.html"
    ))
    .unwrap();
    assert_eq!(
        s.get("/s/abc").await.bytes().await.unwrap().to_vec(),
        s_html
    );
    assert_eq!(
        s.get("/app.js").await.headers()["content-type"],
        "text/javascript"
    );
    assert_eq!(
        s.get("/pli.css").await.headers()["content-type"],
        "text/css"
    );
    assert_eq!(
        s.get("/favicon.svg").await.headers()["content-type"],
        "image/svg+xml"
    );
    assert_eq!(
        s.get("/favicon.ico").await.headers()["content-type"],
        "image/x-icon"
    );
    for (chemin, type_mime) in [
        ("/vendor/plbls-ui/plbls-ui.min.css", "text/css"),
        ("/vendor/plbls-ui/plbls-ui.esm.min.js", "text/javascript"),
    ] {
        let r = s.get(chemin).await;
        assert_eq!(r.status(), 200, "{chemin}");
        assert_eq!(r.headers()["content-type"], type_mime, "{chemin}");
    }

    let api = s.get("/api/config").await;
    assert_eq!(api.headers()["cache-control"], "no-store");
    assert_eq!(api.headers()["content-type"], "application/json");
    assert_eq!(api.headers()["content-security-policy"], CSP);
    let texte = api.text().await.unwrap();
    assert!(
        texte.starts_with(r#"{"gatewayUrl":"https://security.plbls.fr","maxBytes":1000,"ttls":["#),
        "{texte}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&texte).unwrap(),
        json!({ "gatewayUrl": "https://security.plbls.fr", "maxBytes": 1000, "ttls": [3600, 86400, 604800, 2592000], "permission": "pli:deposit" })
    );

    let r = s.get("/..%2fserver/index.js").await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.headers()["content-type"], "text/plain; charset=utf-8");
    assert_eq!(r.text().await.unwrap(), "Introuvable");
    assert_eq!(s.get("/vendor").await.status(), 404);
    assert_eq!(s.get("/api/inconnu").await.status(), 404);
    assert_eq!(
        s.req(Method::POST, "/index.html", None)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        s.req(Method::HEAD, "/", None)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn limite_de_debit_sur_la_lecture_publique() {
    let Some(s) = demarre().await else { return };
    let lire = |ip: &'static str| {
        s.http
            .get(format!("{}/api/s/nimportequoi", s.base))
            .header("x-forwarded-for", format!("9.9.9.9, {ip}"))
            .send()
    };
    for _ in 0..60 {
        assert_eq!(lire("1.2.3.4").await.unwrap().status(), 404);
    }
    let r = lire("1.2.3.4").await.unwrap();
    assert_eq!(r.status(), 429);
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({ "error": "trop de requêtes, réessayez dans une minute" })
    );
    assert_eq!(lire("5.6.7.8").await.unwrap().status(), 404);
}

#[tokio::test]
async fn purge_des_partages_expires() {
    let Some(s) = demarre().await else { return };
    db::insert_share(
        &s.pool,
        "AAAAAAAAAAAAAAAAAAAAAA",
        "alice",
        &blob(40),
        3600,
        50,
    )
    .await
    .unwrap()
    .unwrap();
    db::insert_share(
        &s.pool,
        "BBBBBBBBBBBBBBBBBBBBBB",
        "alice",
        &blob(40),
        3600,
        50,
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query("UPDATE shares SET expires_at = now() - interval '1 second' WHERE id = 'BBBBBBBBBBBBBBBBBBBBBB'")
        .execute(&s.pool)
        .await
        .unwrap();
    assert_eq!(s.get("/api/s/BBBBBBBBBBBBBBBBBBBBBB").await.status(), 404);
    assert_eq!(
        s.post("/api/s/BBBBBBBBBBBBBBBBBBBBBB/consume", vec![], None)
            .await
            .status(),
        404
    );
    assert_eq!(db::purge_expired(&s.pool).await.unwrap(), 1);
    assert_eq!(db::purge_expired(&s.pool).await.unwrap(), 0);
    let restants: i64 = sqlx::query_scalar("SELECT count(*) FROM shares")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert_eq!(restants, 1);
}

#[tokio::test]
async fn migration_compatible_avec_une_base_creee_par_le_serveur_node() {
    let Some((_base, url)) = postgres().await else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS shares (
           id TEXT PRIMARY KEY, owner TEXT NOT NULL, data BYTEA NOT NULL, size INTEGER NOT NULL,
           created_at TIMESTAMPTZ NOT NULL DEFAULT now(), expires_at TIMESTAMPTZ NOT NULL);
         CREATE INDEX IF NOT EXISTS shares_owner ON shares (owner);
         CREATE INDEX IF NOT EXISTS shares_expires ON shares (expires_at);
         INSERT INTO shares (id, owner, data, size, expires_at)
           VALUES ('CCCCCCCCCCCCCCCCCCCCCC', 'alice', '\\x01020304', 4, now() + interval '1 hour');",
    )
    .execute(&pool)
    .await
    .unwrap();
    db::migrate(&pool).await.unwrap();
    db::migrate(&pool).await.unwrap();
    let meta = db::share_meta(&pool, "CCCCCCCCCCCCCCCCCCCCCC")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(meta.size, 4);
    assert_eq!(db::list_shares(&pool, "alice").await.unwrap().len(), 1);
    assert_eq!(
        db::consume_share(&pool, "CCCCCCCCCCCCCCCCCCCCCC")
            .await
            .unwrap(),
        Some(vec![1, 2, 3, 4])
    );
}
