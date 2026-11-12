#![allow(clippy::result_large_err)]

use std::{sync::LazyLock, time::Duration};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use estompe_core::{
    administration::{Admin, ecrire_bot, journaliser},
    api::ActiveUser,
    http::{champ, champ_texte, decode, internal, write_err, write_json},
    openapi,
    servers::{horodatage, pool, rfc3339, uuid_valide},
    state::AppState,
};
use regex::Regex;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row, postgres::PgRow};
use std::collections::HashMap;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const PAR_HEURE: usize = 5;
const LOGS_MAX: usize = 40000;
const CAPTURES_MAX: usize = 5;
const CATEGORIES: &[&str] = &[
    "",
    "messages",
    "vocal",
    "ecran",
    "notifications",
    "autre",
];
const PLATEFORMES: &[&str] = &["", "web", "pwa", "desktop", "android"];
const STATUTS: &[(&str, &str)] = &[
    ("new", "nouveau"),
    ("in_progress", "en cours"),
    ("resolved", "résolu"),
    ("rejected", "rejeté"),
    ("duplicate", "doublon"),
];

static SECRETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)eyJ[\w-]+\.[\w-]+\.[\w-]*|\bBearer\s+[\w.~+/=-]+|(?:token|password|passwd|mot_de_passe|secret|authorization|apikey|api_key)["']?\s*[:=]\s*(?:"[^"]*"|'[^']*'|[^\s&,;}]+)"#,
    )
    .expect("expression des secrets")
});

fn trop_long(s: &str, n: usize) -> bool {
    s.chars().count() > n
}

fn mauvais() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

fn introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

fn base(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    internal()
}

fn ajouter_non_vide(m: &mut Map<String, Value>, nom: &str, v: String) {
    if !v.is_empty() {
        m.insert(nom.into(), v.into());
    }
}

const COLONNES: &str = "r.id::text, r.author_id::text, u.pseudo, {date}, r.title, r.description, r.category, r.platform, r.tech::text, r.logs, r.status, r.duplicate_of::text, r.notes, r.seen";

fn detail(l: &PgRow, pieces: Vec<Value>) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), l.get::<String, _>(0).into());
    m.insert("author_id".into(), l.get::<String, _>(1).into());
    m.insert("author".into(), l.get::<String, _>(2).into());
    m.insert("created_at".into(), rfc3339(&l.get::<String, _>(3)).into());
    m.insert("title".into(), l.get::<String, _>(4).into());
    m.insert("description".into(), l.get::<String, _>(5).into());
    m.insert("category".into(), l.get::<String, _>(6).into());
    m.insert("platform".into(), l.get::<String, _>(7).into());
    let tech: Value = serde_json::from_str(&l.get::<String, _>(8)).unwrap_or(Value::Null);
    if tech.as_object().is_some_and(|o| !o.is_empty()) {
        m.insert("tech".into(), tech);
    }
    ajouter_non_vide(&mut m, "logs", l.get(9));
    m.insert("status".into(), l.get::<String, _>(10).into());
    m.insert("duplicate_of".into(), l.get::<Option<String>, _>(11).into());
    ajouter_non_vide(&mut m, "notes", l.get(12));
    m.insert("seen".into(), l.get::<bool, _>(13).into());
    if !pieces.is_empty() {
        m.insert("attachments".into(), pieces.into());
    }
    Value::Object(m)
}

async fn signalement_admin(pool: &PgPool, id: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query("UPDATE reports SET seen=true WHERE id=$1::uuid")
        .bind(id)
        .execute(pool)
        .await?;
    let sql = format!(
        "SELECT {} FROM reports r JOIN users u ON u.id = r.author_id WHERE r.id=$1::uuid",
        COLONNES.replace("{date}", &horodatage("r.created_at"))
    );
    let Some(l) = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let pieces = sqlx::query(
        "SELECT id::text, name, mime, size, purged_at IS NOT NULL, kind FROM files WHERE report_id=$1::uuid ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|p| {
        json!({
            "id": p.get::<String, _>(0),
            "name": p.get::<String, _>(1),
            "mime": p.get::<String, _>(2),
            "size": p.get::<i64, _>(3),
            "expired": p.get::<bool, _>(4),
            "kind": p.get::<String, _>(5),
        })
    })
    .collect();
    Ok(Some(detail(&l, pieces)))
}

fn texte_ou_vide(l: &PgRow, i: usize) -> Value {
    l.get::<Option<String>, _>(i).into()
}

async fn creer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    corps: Body,
) -> Result<Response, Response> {
    let e = decode(corps).await?;
    let texte = |nom: &str| champ_texte(&e, nom).ok_or_else(mauvais);
    let titre = texte("title")?.trim().to_string();
    let description = texte("description")?.trim().to_string();
    let categorie = texte("category")?;
    let plateforme = texte("platform")?;
    let logs = texte("logs")?;
    let mut tech = Map::new();
    match champ(&e, "tech") {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            for (k, v) in o {
                match v {
                    Value::String(_) => tech.insert(k.clone(), v.clone()),
                    Value::Null => tech.insert(k.clone(), "".into()),
                    _ => return Err(mauvais()),
                };
            }
        }
        Some(_) => return Err(mauvais()),
    }
    let ids: Vec<String> = match champ(&e, "file_ids") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<_>>()
            .ok_or_else(mauvais)?,
        Some(_) => return Err(mauvais()),
    };
    let invalide = titre.is_empty()
        || trop_long(&titre, 120)
        || description.is_empty()
        || trop_long(&description, 4000)
        || !CATEGORIES.contains(&categorie.as_str())
        || !PLATEFORMES.contains(&plateforme.as_str())
        || logs.len() > LOGS_MAX
        || tech.len() > 20
        || ids.len() > CAPTURES_MAX
        || tech.iter().any(|(k, v)| {
            k.is_empty() || trop_long(k, 40) || trop_long(v.as_str().unwrap_or(""), 300)
        })
        || ids.iter().any(|i| !uuid_valide(i));
    if invalide {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_report",
            "signalement : titre (120 car.) et description (4000 car.) obligatoires, catégorie et plateforme connues, 5 captures et 40 000 caractères de logs au plus",
        ));
    }
    if !state
        .limiteurs()
        .signalement
        .allow(&u.id, PAR_HEURE, Duration::from_secs(3600))
    {
        return Ok(write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_reports",
            "5 signalements par heure au maximum",
        ));
    }
    let pool = pool(&state)?;
    let logs = SECRETS.replace_all(&logs, "[masqué]").into_owned();
    let mut tx = pool.begin().await.map_err(base)?;
    let (id, statut): (String, String) = sqlx::query_as(
        "INSERT INTO reports (author_id, title, description, category, platform, tech, logs) VALUES ($1::uuid,$2,$3,$4,$5,$6::text::jsonb,$7) RETURNING id::text, status",
    )
    .bind(&u.id)
    .bind(&titre)
    .bind(&description)
    .bind(&categorie)
    .bind(&plateforme)
    .bind(Value::Object(tech).to_string())
    .bind(&logs)
    .fetch_one(&mut *tx)
    .await
    .map_err(base)?;
    if !ids.is_empty() {
        let n = sqlx::query(
            "UPDATE files SET report_id=$1::uuid WHERE id = ANY($2::uuid[]) AND uploader_id=$3::uuid AND message_id IS NULL AND report_id IS NULL AND purged_at IS NULL AND mime LIKE 'image/%'",
        )
        .bind(&id)
        .bind(&ids)
        .bind(&u.id)
        .execute(&mut *tx)
        .await
        .map_err(base)?
        .rows_affected();
        if n as usize != ids.len() {
            return Ok(write_err(
                StatusCode::BAD_REQUEST,
                "invalid_files",
                "captures introuvables (images envoyées par vous, non déjà jointes)",
            ));
        }
    }
    tx.commit().await.map_err(base)?;
    Ok(write_json(
        StatusCode::CREATED,
        &json!({ "id": id, "status": statut }),
    ))
}

async fn mes_signalements(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let sql = format!(
        "SELECT r.id::text, {}, r.title, r.category, r.platform, r.status, r.duplicate_of::text FROM reports r WHERE r.author_id=$1::uuid ORDER BY r.created_at DESC LIMIT 100",
        horodatage("r.created_at")
    );
    let l: Vec<Value> = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(&u.id)
        .fetch_all(pool(&state)?)
        .await
        .map_err(base)?
        .iter()
        .map(|l| {
            json!({
                "id": l.get::<String, _>(0),
                "created_at": rfc3339(&l.get::<String, _>(1)),
                "title": l.get::<String, _>(2),
                "category": l.get::<String, _>(3),
                "platform": l.get::<String, _>(4),
                "status": l.get::<String, _>(5),
                "duplicate_of": texte_ou_vide(l, 6),
                "seen": false,
            })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn liste_admin(
    State(state): State<AppState>,
    _: Admin,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let filtre = |nom: &str| q.get(nom).cloned().unwrap_or_default();
    let sql = format!(
        "SELECT r.id::text, u.pseudo, {}, r.title, r.category, r.platform, r.status, r.duplicate_of::text, r.seen
FROM reports r JOIN users u ON u.id = r.author_id
WHERE ($1::text = '' OR r.status = $1) AND ($2::text = '' OR r.category = $2) AND ($3::text = '' OR r.platform = $3)
  AND ($4::text = '' OR position(lower($4) in lower(u.pseudo)) > 0)
  AND ($5::text = '' OR position(lower($5) in lower(r.title || ' ' || r.description)) > 0)
ORDER BY r.created_at DESC LIMIT 200",
        horodatage("r.created_at")
    );
    let l: Vec<Value> = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(filtre("status"))
        .bind(filtre("category"))
        .bind(filtre("platform"))
        .bind(filtre("author"))
        .bind(filtre("q"))
        .fetch_all(pool(&state)?)
        .await
        .map_err(base)?
        .iter()
        .map(|l| {
            json!({
                "id": l.get::<String, _>(0),
                "author": l.get::<String, _>(1),
                "created_at": rfc3339(&l.get::<String, _>(2)),
                "title": l.get::<String, _>(3),
                "category": l.get::<String, _>(4),
                "platform": l.get::<String, _>(5),
                "status": l.get::<String, _>(6),
                "duplicate_of": texte_ou_vide(l, 7),
                "seen": l.get::<bool, _>(8),
            })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn detail_admin(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    if !uuid_valide(&id) {
        return Ok(introuvable());
    }
    match signalement_admin(pool(&state)?, &id).await.map_err(base)? {
        Some(r) => Ok(write_json(StatusCode::OK, &r)),
        None => Ok(introuvable()),
    }
}

async fn modifier(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    if !uuid_valide(&id) {
        return Ok(introuvable());
    }
    let e = decode(corps).await?;
    let statut = champ_texte(&e, "status").ok_or_else(mauvais)?;
    let notes = champ_texte(&e, "notes").ok_or_else(mauvais)?;
    let origine = match champ(&e, "duplicate_of") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(mauvais()),
    };
    let libelle = STATUTS.iter().find(|(c, _)| *c == statut).map(|(_, l)| *l);
    if libelle.is_none()
        || trop_long(&notes, 4000)
        || origine.as_deref().is_some_and(|o| !uuid_valide(o))
    {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_report",
            "statut inconnu, notes de 4000 caractères au plus",
        ));
    }
    let pool = pool(&state)?;
    let Some(avant) = signalement_admin(pool, &id).await.map_err(base)? else {
        return Ok(introuvable());
    };
    let origine = origine.filter(|_| statut == "duplicate");
    if statut == "duplicate" && origine.as_deref().is_none_or(|o| o == id) {
        return Ok(doublon_invalide());
    }
    let maj = sqlx::query(
        "UPDATE reports SET status=$2, duplicate_of=$3::uuid, notes=$4 WHERE id=$1::uuid",
    )
    .bind(&id)
    .bind(&statut)
    .bind(&origine)
    .bind(notes.trim())
    .execute(pool)
    .await;
    match maj {
        Ok(r) if r.rows_affected() == 0 => return Ok(introuvable()),
        Ok(_) => {}
        Err(sqlx::Error::Database(d)) if d.code().as_deref() == Some("23503") => {
            return Ok(doublon_invalide());
        }
        Err(e) => return Err(base(e)),
    }
    let Some(apres) = signalement_admin(pool, &id).await.map_err(base)? else {
        return Ok(introuvable());
    };
    if avant["status"] != apres["status"] {
        let titre = apres["title"].as_str().unwrap_or_default();
        journaliser(
            pool,
            &idn.sub,
            &format!("signalement : statut « {} »", libelle.unwrap_or_default()),
            titre,
        )
        .await;
        if statut == "resolved" {
            let auteur = apres["author_id"].as_str().unwrap_or_default();
            let message =
                format!("Votre signalement « {titre} » est résolu. Merci de votre aide !");
            if let Err(e) = ecrire_bot(&state, pool, auteur, &message).await {
                tracing::warn!("message de signalement résolu : {e}");
            }
        }
    }
    Ok(write_json(StatusCode::OK, &apres))
}

fn doublon_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_duplicate",
        "un doublon renvoie vers un autre signalement existant",
    )
}

async fn supprimer(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    if !uuid_valide(&id) {
        return Ok(introuvable());
    }
    let pool = pool(&state)?;
    let Some(r) = signalement_admin(pool, &id).await.map_err(base)? else {
        return Ok(introuvable());
    };
    let n = sqlx::query("DELETE FROM reports WHERE id=$1::uuid")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(base)?
        .rows_affected();
    if n == 0 {
        return Ok(introuvable());
    }
    journaliser(
        pool,
        &idn.sub,
        "signalement supprimé",
        r["title"].as_str().unwrap_or_default(),
    )
    .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    let admin = |resume: &str, reponses: &[(&str, &str)], chemin: &str| {
        openapi::operation("administration", resume, reponses, chemin)
    };
    let r = router
        .route("/reports", post(creer).get(mes_signalements))
        .route("/admin/reports", get(liste_admin))
        .route(
            "/admin/reports/{id}",
            get(detail_admin).patch(modifier).delete(supprimer),
        );
    let acces = [
        ("401", "Non authentifié"),
        ("403", "Réservé aux administrateurs"),
    ];
    let avec = |extra: &[(&'static str, &'static str)]| {
        let mut v = vec![];
        v.extend_from_slice(extra);
        v.extend_from_slice(&acces);
        v
    };
    let r = openapi::documenter(
        r,
        "/reports",
        HttpMethod::Post,
        openapi::operation(
            "comptes",
            "Signaler un bug (5 par heure) ; captures jointes par `file_ids`",
            &[
                ("201", "créé — {id, status}"),
                ("400", "`invalid_report` ou `invalid_files`"),
                ("429", "`too_many_reports`"),
            ],
            "/reports",
        ),
    );
    let r = openapi::documenter(
        r,
        "/reports",
        HttpMethod::Get,
        openapi::operation(
            "comptes",
            "Mes signalements et leur statut (sans logs ni notes internes)",
            &[("200", "ok")],
            "/reports",
        ),
    );
    let r = openapi::documenter(
        r,
        "/admin/reports",
        HttpMethod::Get,
        admin(
            "Signalements (200 au plus) filtrés par `status`, `category`, `platform`, `author` et `q`",
            &avec(&[("200", "ok")]),
            "/admin/reports",
        ),
    );
    let chemin = "/admin/reports/{id}";
    let r = openapi::documenter(
        r,
        chemin,
        HttpMethod::Get,
        admin(
            "Détail d'un signalement ; le marque comme consulté",
            &avec(&[("200", "ok"), ("404", "introuvable")]),
            chemin,
        ),
    );
    let r = openapi::documenter(
        r,
        chemin,
        HttpMethod::Patch,
        admin(
            "Change le statut, le doublon d'origine et les notes internes",
            &avec(&[
                ("200", "ok"),
                ("400", "`invalid_report` ou `invalid_duplicate`"),
                ("404", "introuvable"),
            ]),
            chemin,
        ),
    );
    openapi::documenter(
        r,
        chemin,
        HttpMethod::Delete,
        admin(
            "Supprime un signalement",
            &avec(&[("204", "ok"), ("404", "introuvable")]),
            chemin,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::mount;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use estompe_core::{
        db::migrate_embarquees,
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use utoipa_axum::router::OpenApiRouter;

    struct Env {
        app: Router,
        state: AppState,
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
        let permission =
            testutil::faux_aegis(|sub, p| !testutil::est_admin(p) || sub == "sub-root").await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "bob", "carol", "root"] {
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(&pool)
                .await
                .unwrap();
        }
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("AEGIS_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let app = Router::from(mount(OpenApiRouter::new())).with_state(state.clone());
        Some(Env {
            app,
            state,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn appel(
        e: &Env,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: &str,
    ) -> (StatusCode, Value) {
        let rep = e
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

    async fn capture(e: &Env, proprietaire: &str, mime: &str) -> String {
        sqlx::query_scalar(
            "INSERT INTO files (uploader_id, name, mime, size, key) VALUES ((SELECT id FROM users WHERE pseudo=$1), 'c.png', $2, 3, 'k') RETURNING id::text",
        )
        .bind(proprietaire)
        .bind(mime)
        .fetch_one(e.state.pool().ok().unwrap())
        .await
        .unwrap()
    }

    fn corps(fichiers: &[&str]) -> String {
        json!({
            "title": "Le vocal coupe", "description": "après 5 min", "category": "vocal",
            "platform": "web", "tech": { "version": "1.0" },
            "logs": "GET Bearer abc.def x\npassword=hunter2", "file_ids": fichiers
        })
        .to_string()
    }

    #[tokio::test]
    async fn creation_et_validation() {
        let Some(e) = env().await else { return };
        let image = capture(&e, "alice", "image/png").await;
        let texte = capture(&e, "alice", "text/plain").await;
        for (nom, c) in [("alice", corps(&[&texte])), ("bob", corps(&[&image]))] {
            let (s, v) = appel(&e, nom, "POST", "/reports", &c).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_files"))
            );
        }
        for c in [
            r#"{"title":"","description":"x"}"#.to_string(),
            r#"{"title":"t","description":"x","category":"inconnue"}"#.to_string(),
            json!({ "title": "t", "description": "x", "platform": "toaster" }).to_string(),
            json!({ "title": "t", "description": "x", "file_ids": ["pas-un-uuid"] }).to_string(),
            json!({ "title": "t".repeat(121), "description": "x" }).to_string(),
            json!({ "title": "t", "description": "x", "logs": "a".repeat(40001) }).to_string(),
        ] {
            let (s, v) = appel(&e, "alice", "POST", "/reports", &c).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_report")),
                "{c}"
            );
        }
        assert_eq!(
            appel(
                &e,
                "alice",
                "POST",
                "/reports",
                r#"{"title":3,"description":"x"}"#
            )
            .await
            .1["error"],
            "bad_request"
        );
        let (s, v) = appel(&e, "alice", "POST", "/reports", &corps(&[&image])).await;
        assert_eq!((s, &v["status"]), (StatusCode::CREATED, &json!("new")));
        let (s, v) = appel(&e, "alice", "POST", "/reports", &corps(&[&image])).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_files"))
        );
    }

    #[tokio::test]
    async fn acces_des_utilisateurs_et_limite() {
        let Some(e) = env().await else { return };
        let (_, rep) = appel(&e, "alice", "POST", "/reports", &corps(&[])).await;
        let id = rep["id"].as_str().unwrap().to_string();
        let (s, l) = appel(&e, "alice", "GET", "/reports", "").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(l[0]["id"], id.as_str());
        assert_eq!(l[0]["duplicate_of"], Value::Null);
        assert!(l[0].get("logs").is_none() && l[0].get("notes").is_none());
        assert_eq!(appel(&e, "bob", "GET", "/reports", "").await.1, json!([]));
        for (m, p) in [
            ("GET", "/admin/reports".to_string()),
            ("GET", format!("/admin/reports/{id}")),
            ("PATCH", format!("/admin/reports/{id}")),
            ("DELETE", format!("/admin/reports/{id}")),
        ] {
            assert_eq!(
                appel(&e, "alice", m, &p, r#"{"status":"resolved"}"#)
                    .await
                    .0,
                StatusCode::FORBIDDEN
            );
        }
        for _ in 0..5 {
            assert_eq!(
                appel(
                    &e,
                    "carol",
                    "POST",
                    "/reports",
                    r#"{"title":"t","description":"x"}"#
                )
                .await
                .0,
                StatusCode::CREATED
            );
        }
        let (s, v) = appel(
            &e,
            "carol",
            "POST",
            "/reports",
            r#"{"title":"t","description":"x"}"#,
        )
        .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::TOO_MANY_REQUESTS, Some("too_many_reports"))
        );
    }

    #[tokio::test]
    async fn traitement_par_l_administration() {
        let Some(e) = env().await else { return };
        let image = capture(&e, "alice", "image/png").await;
        let (_, rep) = appel(&e, "alice", "POST", "/reports", &corps(&[&image])).await;
        let id = rep["id"].as_str().unwrap().to_string();
        let (_, autre) = appel(
            &e,
            "bob",
            "POST",
            "/reports",
            r#"{"title":"Autre","description":"x"}"#,
        )
        .await;
        let autre = autre["id"].as_str().unwrap().to_string();

        let (_, l) = appel(
            &e,
            "root",
            "GET",
            "/admin/reports?category=vocal&platform=web&q=5+min&author=ali",
            "",
        )
        .await;
        assert_eq!(l.as_array().unwrap().len(), 1);
        assert_eq!(
            (&l[0]["id"], &l[0]["seen"], &l[0]["author"]),
            (&json!(id), &json!(false), &json!("alice"))
        );
        let (_, l) = appel(&e, "root", "GET", "/admin/reports?status=resolved", "").await;
        assert_eq!(l, json!([]));

        let (s, d) = appel(&e, "root", "GET", &format!("/admin/reports/{id}"), "").await;
        assert_eq!(s, StatusCode::OK);
        let logs = d["logs"].as_str().unwrap();
        assert!(
            logs.contains("[masqué]") && !logs.contains("hunter2") && !logs.contains("abc.def")
        );
        assert_eq!(d["attachments"][0]["id"], image.as_str());
        assert_eq!(d["attachments"][0]["expired"], false);
        assert_eq!(
            (&d["tech"], &d["seen"]),
            (&json!({ "version": "1.0" }), &json!(true))
        );
        assert!(d.get("notes").is_none());
        let (_, l) = appel(&e, "root", "GET", "/admin/reports?status=new&q=vocal", "").await;
        assert_eq!(l[0]["seen"], true);

        let patch = |p: &str, c: String| (format!("/admin/reports/{p}"), c);
        for c in [
            r#"{"status":"inconnu"}"#.to_string(),
            json!({ "status": "new", "notes": "n".repeat(4001) }).to_string(),
            r#"{"status":"new","duplicate_of":"x"}"#.to_string(),
        ] {
            let (p, c) = patch(&id, c);
            let (s, v) = appel(&e, "root", "PATCH", &p, &c).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_report"))
            );
        }
        for c in [
            r#"{"status":"duplicate"}"#.to_string(),
            json!({ "status": "duplicate", "duplicate_of": autre }).to_string(),
            r#"{"status":"duplicate","duplicate_of":"00000000-0000-0000-0000-000000000000"}"#
                .to_string(),
        ] {
            let (p, c) = patch(&autre, c);
            let (s, v) = appel(&e, "root", "PATCH", &p, &c).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_duplicate"))
            );
        }
        let (p, c) = patch(
            &autre,
            json!({ "status": "duplicate", "duplicate_of": id, "notes": " vu aussi " }).to_string(),
        );
        let (s, v) = appel(&e, "root", "PATCH", &p, &c).await;
        assert_eq!(
            (s, &v["duplicate_of"], &v["notes"]),
            (StatusCode::OK, &json!(id), &json!("vu aussi"))
        );
        let (_, l) = appel(&e, "bob", "GET", "/reports", "").await;
        assert!(l[0].get("notes").is_none());
        assert_eq!(
            appel(
                &e,
                "root",
                "PATCH",
                "/admin/reports/00000000-0000-0000-0000-000000000000",
                r#"{"status":"new"}"#
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            appel(&e, "root", "GET", "/admin/reports/pas-un-uuid", "")
                .await
                .0,
            StatusCode::NOT_FOUND
        );

        let (p, c) = patch(&id, r#"{"status":"resolved"}"#.into());
        assert_eq!(appel(&e, "root", "PATCH", &p, &c).await.0, StatusCode::OK);
        let (_, l) = appel(&e, "alice", "GET", "/reports", "").await;
        assert_eq!(l[0]["status"], "resolved");
        let pool = e.state.pool().ok().unwrap();
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM messages m JOIN dm_conversations c ON c.id = m.dm_id WHERE m.content LIKE '%Le vocal coupe%est résolu%'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(n, 1);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_log WHERE action = 'signalement : statut « résolu »' AND target = 'Le vocal coupe'")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        appel(&e, "root", "PATCH", &p, &c).await;
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM messages WHERE content LIKE '%Le vocal coupe%est résolu%'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(n, 1);

        let chemin = format!("/admin/reports/{id}");
        assert_eq!(
            appel(&e, "root", "DELETE", &chemin, "").await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            appel(&e, "root", "GET", &chemin, "").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            appel(&e, "root", "DELETE", &chemin, "").await.0,
            StatusCode::NOT_FOUND
        );
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM admin_log WHERE action = 'signalement supprimé'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(n, 1);
        let rattachee: Option<String> =
            sqlx::query_scalar("SELECT report_id::text FROM files WHERE id=$1::uuid")
                .bind(&image)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(rattachee, None);
    }
}
