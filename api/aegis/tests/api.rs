use std::time::{SystemTime, UNIX_EPOCH};

use aegis::{
    AppState, MIGRATIONS, identity,
    settings::{
        Annuaire, Cache, Database, Gateway, Identity, Redis, Ressources, Server, Services, Settings,
    },
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::Connection;
use tower::ServiceExt;

fn secret() -> &'static str {
    static SECRET: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SECRET.get_or_init(|| {
        use std::hash::BuildHasher;
        std::collections::hash_map::RandomState::new()
            .hash_one(std::process::id())
            .to_string()
    })
}

struct Banc {
    app: axum::Router,
    state: AppState,
    admin: String,
    base: String,
}

impl Drop for Banc {
    fn drop(&mut self) {
        let (admin, base) = (self.admin.clone(), self.base.clone());
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Ok(mut c) = sqlx::PgConnection::connect(&admin).await {
                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                        "DROP DATABASE IF EXISTS \"{base}\" WITH (FORCE)"
                    )))
                    .execute(&mut c)
                    .await;
                }
            });
        })
        .join()
        .ok();
    }
}

fn url_de_base(url: &str, nom: &str) -> String {
    let (debut, requete) = url
        .split_once('?')
        .map_or((url, String::new()), |(d, r)| (d, format!("?{r}")));
    let racine = debut
        .rsplit_once('/')
        .expect("TEST_DATABASE_URL doit se terminer par /<base>")
        .0;
    format!("{racine}/{nom}{requete}")
}

static COMPTEUR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

async fn banc() -> Option<Banc> {
    let Ok(admin) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("ignoré : TEST_DATABASE_URL absent");
        return None;
    };
    let base = format!(
        "aegis_test_{}_{}",
        std::process::id(),
        COMPTEUR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let mut c = sqlx::PgConnection::connect(&admin)
        .await
        .expect("connexion");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{base}\"")))
        .execute(&mut c)
        .await
        .expect("création de la base");
    let url = url_de_base(&admin, &base);
    let db = sqlx::PgPool::connect(&url).await.expect("pool");
    MIGRATIONS.run(&db).await.expect("migrations");
    let settings = Settings {
        server: Server {
            bind: "127.0.0.1:0".into(),
            admin_role: "aegis-admin".into(),
        },
        database: Database { url },
        identity: Identity {
            secret: secret().into(),
        },
        redis: Redis {
            url: std::env::var("TEST_REDIS_URL").unwrap_or_default(),
            prefix: base.clone(),
        },
        cache: Cache::default(),
        services: Services {
            tokens: "estompe:jeton-estompe,pli:jeton-pli".into(),
        },
        gateway: Gateway::default(),
        annuaire: Annuaire::default(),
        ressources: Ressources::default(),
    };
    let state = AppState::new(settings, db).await.expect("état");
    state.cache.invalider_tout().await;
    Some(Banc {
        app: state.routeur(),
        state,
        admin,
        base,
    })
}

enum Qui<'a> {
    Service(&'a str),
    User(&'a str, &'a str, &'a str),
}

impl Banc {
    async fn appel(
        &self,
        qui: Qui<'_>,
        methode: &str,
        chemin: &str,
        corps: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut r = Request::builder().method(methode).uri(chemin);
        match qui {
            Qui::Service(j) => r = r.header("authorization", format!("Bearer {j}")),
            Qui::User(sub, roles, email) => {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                r = r
                    .header("x-user-id", sub)
                    .header("x-user-roles", roles)
                    .header("x-user-email", email)
                    .header("x-client-id", "estompe")
                    .header("x-identity-ts", ts.to_string())
                    .header(
                        "x-identity-sig",
                        identity::sign(secret(), sub, roles, "estompe", ts),
                    );
            }
        }
        let body = match corps {
            Some(v) => {
                r = r.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .app
            .clone()
            .oneshot(r.body(body).unwrap())
            .await
            .unwrap();
        let statut = res.status();
        let octets = res.into_body().collect().await.unwrap().to_bytes();
        (
            statut,
            serde_json::from_slice(&octets).unwrap_or(Value::Null),
        )
    }

    async fn admin(&self, methode: &str, chemin: &str, corps: Option<Value>) -> Value {
        let (s, v) = self
            .appel(
                Qui::User("root", "user,aegis-admin", "root@plbls.fr"),
                methode,
                chemin,
                corps,
            )
            .await;
        assert!(s.is_success(), "{methode} {chemin} → {s} {v}");
        v
    }

    async fn effectifs(&self, sub: &str) -> Value {
        let (s, v) = self
            .appel(
                Qui::Service("jeton-estompe"),
                "POST",
                "/v1/effective",
                Some(json!({"sub": sub})),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v["permissions"].clone()
    }

    async fn autorise(&self, sub: &str, perm: &str, res: Option<&str>) -> bool {
        let (s, v) = self
            .appel(
                Qui::Service("jeton-estompe"),
                "POST",
                "/v1/check",
                Some(json!({"sub": sub, "permission": perm, "resource": res})),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v["allowed"].as_bool().unwrap()
    }

    async fn manifeste(&self) {
        let (s, v) = self
            .appel(
                Qui::Service("jeton-estompe"),
                "PUT",
                "/v1/manifest",
                Some(manifeste_estompe()),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }

    async fn role(&self, nom: &str, regles: Value) -> String {
        self.admin(
            "POST",
            "/v1/admin/roles",
            Some(json!({"name": nom, "rules": regles})),
        )
        .await["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

fn manifeste_estompe() -> Value {
    json!({
        "app": "estompe", "name": "Estompe",
        "resource_types": [{"key": "server", "name": "Serveur"}],
        "permissions": [
            {"key": "access", "category": "Accès", "label": "Utiliser Estompe"},
            {"key": "gifs.use", "category": "Médias", "label": "GIF", "open_default": true},
            {"key": "vocal.screen", "category": "Vocal", "label": "Partager l'écran", "resource_type": "server", "open_default": true},
            {"key": "films.use", "category": "Médias", "label": "Films"}
        ]
    })
}

#[tokio::test]
async fn manifeste_ouvre_les_permissions_par_defaut_et_marque_les_obsoletes() {
    let Some(b) = banc().await else { return };
    let (s, v) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "PUT",
            "/v1/manifest",
            Some(manifeste_estompe()),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["added"].as_array().unwrap().len(), 4);
    assert_eq!(
        v["opened_to_everyone"],
        json!(["estompe:gifs.use", "estompe:vocal.screen"])
    );
    let (_, v) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "PUT",
            "/v1/manifest",
            Some(manifeste_estompe()),
        )
        .await;
    assert_eq!(v["changed"], false);
    let p = b.effectifs("alice").await;
    assert!(p.get("estompe:gifs.use").is_some() && p.get("estompe:access").is_none());

    let (s, _) = b
        .appel(
            Qui::Service("jeton-pli"),
            "PUT",
            "/v1/manifest",
            Some(manifeste_estompe()),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = b
        .appel(
            Qui::User("alice", "user", ""),
            "PUT",
            "/v1/manifest",
            Some(manifeste_estompe()),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let mut m = manifeste_estompe();
    m["permissions"].as_array_mut().unwrap().remove(1);
    let (_, v) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "PUT",
            "/v1/manifest",
            Some(m),
        )
        .await;
    assert_eq!(v["obsolete"], json!(["estompe:gifs.use"]));
    assert!(!b.autorise("alice", "estompe:gifs.use", None).await);
    let (_, v) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "PUT",
            "/v1/manifest",
            Some(manifeste_estompe()),
        )
        .await;
    assert_eq!(v["added"], json!([]));
    assert!(
        b.autorise("alice", "estompe:gifs.use", None).await,
        "réactivée avec ses règles"
    );
}

#[tokio::test]
async fn administration_reservee() {
    let Some(b) = banc().await else { return };
    let (s, _) = b
        .appel(
            Qui::User("alice", "user", ""),
            "GET",
            "/v1/admin/roles",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "GET",
            "/v1/admin/roles",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = b
        .appel(
            Qui::Service("faux"),
            "POST",
            "/v1/effective",
            Some(json!({"sub": "a"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let roles = b.admin("GET", "/v1/admin/roles", None).await;
    let admin = roles
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["system"] == "admin")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    b.admin(
        "POST",
        "/v1/admin/bindings",
        Some(json!({"role_id": admin, "subject_kind": "account", "subject_id": "alice"})),
    )
    .await;
    let (s, _) = b
        .appel(
            Qui::User("alice", "user", ""),
            "GET",
            "/v1/admin/roles",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "aegis:admin via le rôle système");
    let (s, v) = b
        .appel(
            Qui::User("alice", "user", ""),
            "DELETE",
            &format!("/v1/admin/roles/{admin}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
}

#[tokio::test]
async fn refus_groupes_portee_et_expiration() {
    let Some(b) = banc().await else { return };
    b.manifeste().await;
    let films = b
        .role(
            "Équipe films",
            json!([{"permission": "estompe:films.use", "effect": "allow"},
            {"permission": "estompe:access", "effect": "allow"}]),
        )
        .await;
    let groupe = b
        .admin(
            "POST",
            "/v1/admin/groups",
            Some(json!({"name": "Cinéphiles"})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    b.admin(
        "POST",
        "/v1/admin/bindings",
        Some(json!({"role_id": films, "subject_kind": "group", "subject_id": groupe})),
    )
    .await;
    assert!(!b.autorise("bob", "estompe:films.use", None).await);
    b.admin(
        "PUT",
        &format!("/v1/admin/groups/{groupe}/members/bob"),
        Some(json!({})),
    )
    .await;
    assert!(
        b.autorise("bob", "estompe:films.use", None).await,
        "via le groupe"
    );

    b.admin(
        "POST",
        "/v1/admin/rules",
        Some(json!({"subject_kind": "account", "subject_id": "bob", "permission": "estompe:films.use", "effect": "deny"})),
    )
    .await;
    assert!(
        !b.autorise("bob", "estompe:films.use", None).await,
        "le refus l'emporte"
    );

    b.admin(
        "POST",
        "/v1/admin/rules",
        Some(json!({"subject_kind": "group", "subject_id": groupe, "permission": "estompe:vocal.screen",
            "resource": "srv-1", "effect": "deny"})),
    )
    .await;
    assert!(
        b.autorise("bob", "estompe:vocal.screen", Some("srv-2"))
            .await
    );
    assert!(
        !b.autorise("bob", "estompe:vocal.screen", Some("srv-1"))
            .await
    );

    let (s, v) = b
        .appel(
            Qui::User("root", "aegis-admin", ""),
            "POST",
            "/v1/admin/rules",
            Some(json!({"subject_kind": "account", "subject_id": "bob", "permission": "estompe:gifs.use",
                "resource": "srv-1", "effect": "allow"})),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "gifs.use n'a pas de type de ressource : {v}"
    );

    let modo = b
        .role(
            "Modérateur vocal",
            json!([{"permission": "estompe:vocal.screen", "effect": "allow"},
            {"permission": "estompe:films.use", "effect": "allow"}]),
        )
        .await;
    b.admin(
        "POST",
        "/v1/admin/bindings",
        Some(
            json!({"role_id": modo, "subject_kind": "account", "subject_id": "carol",
            "scope_type": "estompe:server", "scope_id": "srv-9"}),
        ),
    )
    .await;
    let p = b.effectifs("carol").await;
    assert!(
        p["estompe:vocal.screen"]["allow"]
            .as_array()
            .unwrap()
            .contains(&json!("srv-9"))
    );
    assert!(
        p.get("estompe:films.use").is_none(),
        "permission sans ce type ignorée par la portée"
    );

    let bientot = (chrono::Utc::now() + chrono::Duration::seconds(2)).to_rfc3339();
    b.admin(
        "POST",
        "/v1/admin/rules",
        Some(
            json!({"subject_kind": "account", "subject_id": "dave", "permission": "estompe:access",
            "effect": "allow", "expires_at": bientot}),
        ),
    )
    .await;
    let (_, v) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "POST",
            "/v1/effective",
            Some(json!({"sub": "dave", "app": "estompe"})),
        )
        .await;
    assert!(v["permissions"].get("estompe:access").is_some());
    let validite =
        chrono::DateTime::parse_from_rfc3339(v["valid_until"].as_str().unwrap()).unwrap();
    assert!(validite <= chrono::DateTime::parse_from_rfc3339(&bientot).unwrap());
    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
    assert!(!b.autorise("dave", "estompe:access", None).await, "expirée");
    aegis::taches::expirer(&b.state, chrono::Utc::now() - chrono::Duration::minutes(1))
        .await
        .unwrap();
    let journal = b.admin("GET", "/v1/admin/audit?subject=dave", None).await;
    assert_eq!(journal[0]["action"], "expire");
}

#[tokio::test]
async fn fiche_compte_explication_simulation_copie() {
    let Some(b) = banc().await else { return };
    b.manifeste().await;
    let (_, _) = b
        .appel(
            Qui::Service("jeton-estompe"),
            "POST",
            "/v1/effective",
            Some(json!({"sub": "erin", "email": "erin@plbls.fr", "display_name": "Erin"})),
        )
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let page = b.admin("GET", "/v1/admin/accounts?q=ERIN@", None).await;
    assert_eq!(page["accounts"][0]["sub"], "erin");
    assert_eq!(page["accounts"][0]["email"], "erin@plbls.fr");

    let membre = b
        .role(
            "Membre",
            json!([{"permission": "estompe:access", "effect": "allow"}]),
        )
        .await;
    b.admin(
        "POST",
        "/v1/admin/bindings",
        Some(json!({"role_id": membre, "subject_kind": "account", "subject_id": "erin"})),
    )
    .await;
    b.admin(
        "POST",
        "/v1/admin/rules",
        Some(json!({"subject_kind": "account", "subject_id": "erin", "permission": "estompe:gifs.use", "effect": "deny",
            "note": "abus"})),
    )
    .await;
    let e = b
        .admin("GET", "/v1/admin/accounts/erin/effective", None)
        .await;
    let etat = |p: &str| {
        e["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["permission"] == p)
            .unwrap()
            .clone()
    };
    assert_eq!(etat("estompe:access")["state"], "autorise");
    assert_eq!(etat("estompe:access")["rules"][0]["source"]["kind"], "role");
    assert_eq!(etat("estompe:gifs.use")["state"], "refuse");
    assert_eq!(etat("estompe:films.use")["state"], "non_defini");

    let d = b
        .admin(
            "POST",
            "/v1/admin/simulate",
            Some(json!({"sub": "erin", "permission": "estompe:gifs.use"})),
        )
        .await;
    assert_eq!(d["allowed"], false);
    assert_eq!(d["rules"][0]["source"]["kind"], "compte");

    let essai = b
        .admin(
            "POST",
            "/v1/admin/accounts/frank/copy",
            Some(json!({"from": "erin", "mode": "ajouter", "dry_run": true})),
        )
        .await;
    assert_eq!(
        (
            essai["bindings"].as_i64(),
            essai["rules"].as_i64(),
            essai["applied"].as_bool()
        ),
        (Some(1), Some(1), Some(false))
    );
    assert!(!b.autorise("frank", "estompe:access", None).await);
    b.admin(
        "POST",
        "/v1/admin/accounts/frank/copy",
        Some(json!({"from": "erin", "mode": "remplacer"})),
    )
    .await;
    assert!(b.autorise("frank", "estompe:access", None).await);
    assert!(!b.autorise("frank", "estompe:gifs.use", None).await);

    let fiche = b.admin("GET", "/v1/admin/accounts/frank", None).await;
    assert_eq!(fiche["bindings"][0]["role"], "Membre");
    let purge = b
        .admin("DELETE", "/v1/admin/accounts/frank/grants", None)
        .await;
    assert_eq!(purge["removed"], 2);

    let journal = b.admin("GET", "/v1/admin/audit?subject=erin", None).await;
    assert_eq!(journal[0]["action"], "rule.deny");
    assert_eq!(journal[0]["actor_account"]["email"], "root@plbls.fr");
    assert_eq!(journal[0]["subject_account"]["email"], "erin@plbls.fr");

    let cat = b.admin("GET", "/v1/admin/catalog", None).await;
    let estompe = cat["apps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["key"] == "estompe")
        .unwrap();
    assert_eq!(estompe["permissions"].as_array().unwrap().len(), 4);
    let det = b
        .admin(
            "GET",
            "/v1/admin/permissions/estompe:gifs.use/holders",
            None,
        )
        .await;
    assert!(
        det.as_array()
            .unwrap()
            .iter()
            .any(|d| d["subject_label"] == "erin@plbls.fr")
    );
    assert!(
        det.as_array()
            .unwrap()
            .iter()
            .any(|d| d["subject_label"] == "Tout le monde")
    );
}

#[tokio::test]
async fn edition_d_un_role_et_tout_le_monde() {
    let Some(b) = banc().await else { return };
    b.manifeste().await;
    let roles = b.admin("GET", "/v1/admin/roles", None).await;
    let tlm = roles[0]["id"].as_str().unwrap().to_string();
    assert_eq!(roles[0]["system"], "tout-le-monde");
    let detail = b
        .admin("GET", &format!("/v1/admin/roles/{tlm}"), None)
        .await;
    assert_eq!(detail["rules"].as_array().unwrap().len(), 2);
    assert!(b.autorise("zoe", "estompe:gifs.use", None).await);
    b.admin(
        "PUT",
        &format!("/v1/admin/roles/{tlm}"),
        Some(json!({"name": "Tout le monde", "rules": [{"permission": "estompe:vocal.screen", "effect": "allow"}]})),
    )
    .await;
    assert!(
        !b.autorise("zoe", "estompe:gifs.use", None).await,
        "fermé à tous"
    );
    let (s, _) = b
        .appel(
            Qui::User("root", "aegis-admin", ""),
            "POST",
            "/v1/admin/bindings",
            Some(json!({"role_id": tlm, "subject_kind": "account", "subject_id": "zoe"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = b
        .appel(
            Qui::User("root", "aegis-admin", ""),
            "PUT",
            &format!("/v1/admin/roles/{tlm}"),
            Some(json!({"name": "Tout le monde", "rules": [{"permission": "estompe:inconnue", "effect": "allow"}]})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let detail = b
        .admin("GET", &format!("/v1/admin/roles/{tlm}"), None)
        .await;
    assert_eq!(
        detail["rules"].as_array().unwrap().len(),
        1,
        "transaction annulée"
    );
}

#[tokio::test]
async fn cache_redis_et_invalidation() {
    let Some(b) = banc().await else { return };
    let Ok(url) = std::env::var("TEST_REDIS_URL") else {
        eprintln!("ignoré : TEST_REDIS_URL absent");
        return;
    };
    assert!(b.state.cache.actif());
    b.manifeste().await;
    let client = redis::Client::open(url).unwrap();
    let mut pubsub = client.get_async_pubsub().await.unwrap();
    pubsub.subscribe(b.state.cache.canal()).await.unwrap();
    let mut flux = pubsub.into_on_message();
    assert!(b.autorise("yan", "estompe:gifs.use", None).await);
    b.admin(
        "POST",
        "/v1/admin/rules",
        Some(json!({"subject_kind": "account", "subject_id": "yan", "permission": "estompe:gifs.use", "effect": "deny"})),
    )
    .await;
    use futures_util::StreamExt;
    let msg: String = flux.next().await.unwrap().get_payload().unwrap();
    let v: Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(
        (v["type"].as_str(), v["sub"].as_str()),
        (Some("compte"), Some("yan"))
    );
    assert!(
        !b.autorise("yan", "estompe:gifs.use", None).await,
        "cache invalidé"
    );
}

#[tokio::test]
async fn calcul_concurrent_d_une_invalidation_non_servi() {
    let Some(b) = banc().await else { return };
    if std::env::var("TEST_REDIS_URL").is_err() {
        eprintln!("ignoré : TEST_REDIS_URL absent");
        return;
    }
    let cache = &b.state.cache;
    let avant = cache.version("yan").await.unwrap();
    let perime = aegis::moteur::Effectifs::calculer(
        "yan",
        avant.generation,
        &[],
        chrono::Utc::now(),
        cache.ttl(),
    );
    cache.invalider_compte("yan").await;
    cache.ecrire(avant, &perime).await;
    let apres = cache.version("yan").await.unwrap();
    assert_ne!(apres, avant);
    assert_eq!(
        cache.lire(apres, "yan").await,
        None,
        "résultat calculé avant l'invalidation"
    );
}
