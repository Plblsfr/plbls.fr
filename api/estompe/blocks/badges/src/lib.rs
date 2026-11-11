#![allow(clippy::result_large_err)]

mod image;
mod svg;

use axum::{
    body::{Body, to_bytes},
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use estompe_core::{
    administration::{Admin, journaliser},
    api::ActiveUser,
    http::{champ, champ_texte, decode, write_err, write_json},
    openapi, servers,
    state::AppState,
};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row, postgres::PgRow};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const ICONE_MAX: usize = 256 << 10;
const SVG_MAX: usize = 64 << 10;
const COTE_MAX: usize = 512;
const COLONNES: &str =
    "b.id::text, b.name, b.icon, b.color, b.description, b.icon_version, b.sort_order";

type Reponse = Result<Response, Response>;

fn introuvable() -> Response {
    servers::non_trouve("introuvable")
}

fn bd(e: sqlx::Error) -> Response {
    servers::bd(e)
}

fn couleur_valide(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

struct Badge {
    name: String,
    icon: String,
    color: String,
    description: String,
    sort_order: i64,
}

fn mauvais() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

async fn lire_badge(corps: Body) -> Result<Badge, Response> {
    let objet: Map<String, Value> = decode(corps).await?;
    let texte = |nom: &str| champ_texte(&objet, nom).ok_or_else(mauvais);
    let (name, mut icon, color, description) = (
        texte("name")?,
        texte("icon")?,
        texte("color")?,
        texte("description")?,
    );
    let sort_order = match champ(&objet, "sort_order") {
        None | Some(Value::Null) => 0,
        Some(Value::Number(n)) => n.as_i64().ok_or_else(mauvais)?,
        Some(_) => return Err(mauvais()),
    };
    let id_ok = matches!(
        champ(&objet, "id"),
        None | Some(Value::Null | Value::String(_))
    );
    let version_ok = match champ(&objet, "icon_version") {
        None | Some(Value::Null) => true,
        Some(Value::Number(n)) => n.as_i64().is_some(),
        Some(_) => false,
    };
    let date_ok = match champ(&objet, "created_at") {
        None | Some(Value::Null) => true,
        Some(Value::String(d)) => estompe_core::sondages::rfc3339_valide(d),
        Some(_) => false,
    };
    if !(id_ok && version_ok && date_ok) {
        return Err(mauvais());
    }
    let (name, description) = (name.trim().to_string(), description.trim().to_string());
    icon = icon.trim().to_string();
    if icon.is_empty() {
        icon = "🏅".into();
    }
    let refus = |code: &str, message: &str| Err(write_err(StatusCode::BAD_REQUEST, code, message));
    let n = name.chars().count();
    if !(2..=32).contains(&n) {
        return refus("invalid_name", "nom : 2 à 32 caractères");
    }
    if icon.chars().count() > 8 {
        return refus("invalid_icon", "icône : 8 caractères au plus (un emoji)");
    }
    if !couleur_valide(&color) {
        return refus("invalid_color", "couleur : #rrggbb");
    }
    if description.chars().count() > 200 {
        return refus(
            "invalid_description",
            "description : 200 caractères au plus",
        );
    }
    if !(0..=9999).contains(&sort_order) {
        return refus("invalid_order", "ordre : de 0 à 9999");
    }
    Ok(Badge {
        name,
        icon,
        color,
        description,
        sort_order,
    })
}

fn ligne(r: &PgRow) -> Result<Map<String, Value>, sqlx::Error> {
    let mut m = Map::new();
    m.insert("id".into(), json!(r.try_get::<String, _>(0)?));
    m.insert("name".into(), json!(r.try_get::<String, _>(1)?));
    m.insert("icon".into(), json!(r.try_get::<String, _>(2)?));
    m.insert("color".into(), json!(r.try_get::<String, _>(3)?));
    m.insert("description".into(), json!(r.try_get::<String, _>(4)?));
    m.insert("icon_version".into(), json!(r.try_get::<i64, _>(5)?));
    m.insert("sort_order".into(), json!(r.try_get::<i32, _>(6)?));
    m.insert(
        "created_at".into(),
        json!(servers::rfc3339(&r.try_get::<String, _>(7)?)),
    );
    Ok(m)
}

fn colonnes() -> String {
    format!("{COLONNES}, {}", servers::horodatage("b.created_at"))
}

async fn infos(pool: &PgPool, id: &str) -> Result<String, Response> {
    sqlx::query_scalar("SELECT name FROM badges WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)
}

fn existe(e: sqlx::Error) -> Response {
    if servers::unique(&e) {
        write_err(
            StatusCode::CONFLICT,
            "badge_exists",
            "un badge porte déjà ce nom",
        )
    } else {
        bd(e)
    }
}

async fn badges_utilisateur(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(id): Path<String>,
) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let pool = servers::pool(&state)?;
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM user_badges ub JOIN badges b ON b.id=ub.badge_id
         WHERE ub.user_id=$1::uuid AND (
            $1::uuid=$2::uuid
            OR EXISTS (SELECT 1 FROM server_members a JOIN server_members m ON a.server_id=m.server_id WHERE a.user_id=$1::uuid AND m.user_id=$2::uuid)
            OR EXISTS (SELECT 1 FROM dm_conversations d WHERE (d.user_a=$1::uuid AND d.user_b=$2::uuid) OR (d.user_a=$2::uuid AND d.user_b=$1::uuid)))
         ORDER BY b.sort_order, ub.granted_at",
        colonnes()
    )))
    .bind(&id)
    .bind(&u.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let l = lignes
        .iter()
        .map(|r| ligne(r).map(Value::Object))
        .collect::<Result<Vec<_>, _>>()
        .map_err(bd)?;
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn lister(State(state): State<AppState>, _: Admin) -> Reponse {
    let pool = servers::pool(&state)?;
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {},
           coalesce(array_agg(u.id::text ORDER BY u.pseudo) FILTER (WHERE u.id IS NOT NULL), '{{}}'),
           coalesce(array_agg(u.pseudo ORDER BY u.pseudo) FILTER (WHERE u.id IS NOT NULL), '{{}}')
         FROM badges b LEFT JOIN user_badges ub ON ub.badge_id=b.id LEFT JOIN users u ON u.id=ub.user_id
         GROUP BY b.id ORDER BY b.sort_order, b.created_at, b.name",
        colonnes()
    )))
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let mut l = Vec::new();
    for r in &lignes {
        let mut m = ligne(r).map_err(bd)?;
        let ids: Vec<String> = r.try_get(8).map_err(bd)?;
        let pseudos: Vec<String> = r.try_get(9).map_err(bd)?;
        let titulaires: Vec<Value> = ids
            .iter()
            .zip(&pseudos)
            .map(|(id, pseudo)| json!({ "id": id, "pseudo": pseudo }))
            .collect();
        m.insert("holders".into(), Value::Array(titulaires));
        l.push(Value::Object(m));
    }
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn creer(State(state): State<AppState>, Admin(idn): Admin, corps: Body) -> Reponse {
    let b = lire_badge(corps).await?;
    let pool = servers::pool(&state)?;
    let ligne: Result<(String, String), sqlx::Error> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO badges (name, icon, color, description, sort_order) VALUES ($1,$2,$3,$4,$5)
         RETURNING id::text, {}",
        servers::horodatage("created_at")
    )))
        .bind(&b.name)
        .bind(&b.icon)
        .bind(&b.color)
        .bind(&b.description)
        .bind(b.sort_order as i32)
        .fetch_one(pool)
        .await;
    let (id, cree) = ligne.map_err(existe)?;
    journaliser(pool, &idn.sub, "badge créé", &b.name).await;
    Ok(write_json(
        StatusCode::CREATED,
        &json!({
            "id": id, "name": b.name, "icon": b.icon, "color": b.color,
            "description": b.description, "icon_version": 0,
            "sort_order": b.sort_order, "created_at": servers::rfc3339(&cree),
        }),
    ))
}

async fn modifier(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
    corps: Body,
) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let b = lire_badge(corps).await?;
    let pool = servers::pool(&state)?;
    let res = sqlx::query(
        "UPDATE badges SET name=$2, color=$3, description=$4, sort_order=$5 WHERE id=$1::uuid",
    )
    .bind(&id)
    .bind(&b.name)
    .bind(&b.color)
    .bind(&b.description)
    .bind(b.sort_order as i32)
    .execute(pool)
    .await
    .map_err(existe)?;
    if res.rows_affected() == 0 {
        return Err(introuvable());
    }
    journaliser(pool, &idn.sub, "badge modifié", &b.name).await;
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "id": id, "name": b.name, "icon": b.icon, "color": b.color,
            "description": b.description, "icon_version": 0,
            "sort_order": b.sort_order, "created_at": "0001-01-01T00:00:00Z",
        }),
    ))
}

async fn supprimer(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let pool = servers::pool(&state)?;
    let nom: Option<String> =
        sqlx::query_scalar("DELETE FROM badges WHERE id=$1::uuid RETURNING name")
            .bind(&id)
            .fetch_optional(pool)
            .await
            .map_err(bd)?;
    let nom = nom.ok_or_else(introuvable)?;
    journaliser(pool, &idn.sub, "badge supprimé", &nom).await;
    Ok(servers::sans_contenu())
}

fn trop_lourd(message: &str) -> Response {
    write_err(StatusCode::PAYLOAD_TOO_LARGE, "too_large", message)
}

fn image_invalide(message: &str) -> Response {
    write_err(StatusCode::BAD_REQUEST, "invalid_image", message)
}

async fn lire_icone(entetes: &HeaderMap, corps: Body) -> Result<(Vec<u8>, String), Response> {
    let octets = to_bytes(corps, ICONE_MAX)
        .await
        .map_err(|_| trop_lourd("image trop lourde (256 Ko maximum, 64 Ko pour un SVG)"))?;
    let type_envoye = entetes
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if type_envoye.starts_with("image/svg+xml") {
        if octets.len() > SVG_MAX {
            return Err(trop_lourd("SVG trop lourd (64 Ko maximum)"));
        }
        let propre = svg::nettoyer(&octets).map_err(image_invalide)?;
        return Ok((propre, "image/svg+xml".into()));
    }
    let detecte = estompe_core::sniff::detecter(&octets);
    let dimensions = match detecte {
        "image/png" => image::dimensions_png(&octets),
        "image/jpeg" => image::dimensions_jpeg(&octets),
        "image/webp" => image::dimensions_webp(&octets),
        _ => None,
    };
    match dimensions {
        Some((l, h)) if (1..=COTE_MAX).contains(&l) && (1..=COTE_MAX).contains(&h) => {
            Ok((octets.to_vec(), detecte.to_string()))
        }
        _ => Err(image_invalide(
            "image SVG, WebP, PNG ou JPEG de 512 px au plus attendue",
        )),
    }
}

async fn definir_icone(
    state: &AppState,
    sub: &str,
    id: &str,
    image: Option<(Vec<u8>, String)>,
) -> Reponse {
    if !servers::uuid_valide(id) {
        return Err(introuvable());
    }
    let pool = servers::pool(state)?;
    let nom = infos(pool, id).await?;
    let (donnees, type_) = match image {
        Some((d, t)) => (Some(d), Some(t)),
        None => (None, None),
    };
    sqlx::query(
        "UPDATE badges SET icon_data=$2, icon_type=$3,
         icon_version = CASE WHEN $2::bytea IS NULL THEN 0 ELSE icon_version + 1 END WHERE id=$1::uuid",
    )
    .bind(id)
    .bind(donnees)
    .bind(type_)
    .execute(pool)
    .await
    .map_err(bd)?;
    journaliser(pool, sub, "badge : icône modifiée", &nom).await;
    Ok(servers::sans_contenu())
}

async fn remplacer_icone(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
    entetes: HeaderMap,
    corps: Body,
) -> Reponse {
    let image = lire_icone(&entetes, corps).await?;
    definir_icone(&state, &idn.sub, &id, Some(image)).await
}

async fn retirer_icone(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
) -> Reponse {
    definir_icone(&state, &idn.sub, &id, None).await
}

async fn icone(State(state): State<AppState>, _: ActiveUser, Path(id): Path<String>) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let pool = servers::pool(&state)?;
    let ligne: Option<(Vec<u8>, Option<String>)> = sqlx::query_as(
        "SELECT icon_data, icon_type FROM badges WHERE id=$1::uuid AND icon_data IS NOT NULL",
    )
    .bind(&id)
    .fetch_optional(pool)
    .await
    .map_err(bd)?;
    let Some((donnees, Some(type_))) = ligne else {
        return Err(introuvable());
    };
    let Ok(type_) = HeaderValue::from_str(&type_) else {
        return Err(introuvable());
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, type_),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=86400"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
            ),
        ],
        donnees,
    )
        .into_response())
}

async fn attribuer_a(pool: &PgPool, id: &str, pseudo: &str) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id) SELECT u.id, b.id FROM users u, badges b
         WHERE u.pseudo=$2 AND b.id=$1::uuid ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(pseudo)
    .execute(pool)
    .await?;
    if res.rows_affected() > 0 {
        return Ok(true);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_badges ub JOIN users u ON u.id=ub.user_id WHERE ub.badge_id=$1::uuid AND u.pseudo=$2)",
    )
    .bind(id)
    .bind(pseudo)
    .fetch_one(pool)
    .await
}

async fn attribuer(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path((id, pseudo)): Path<(String, String)>,
) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let pool = servers::pool(&state)?;
    let nom = infos(pool, &id).await?;
    if !attribuer_a(pool, &id, &pseudo).await.map_err(bd)? {
        return Err(introuvable());
    }
    journaliser(
        pool,
        &idn.sub,
        "badge attribué",
        &format!("{pseudo} · {nom}"),
    )
    .await;
    Ok(servers::sans_contenu())
}

async fn retirer(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path((id, pseudo)): Path<(String, String)>,
) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let pool = servers::pool(&state)?;
    let nom = infos(pool, &id).await?;
    let res = sqlx::query(
        "DELETE FROM user_badges WHERE badge_id=$1::uuid AND user_id=(SELECT id FROM users WHERE pseudo=$2)",
    )
    .bind(&id)
    .bind(&pseudo)
    .execute(pool)
    .await
    .map_err(bd)?;
    if res.rows_affected() == 0 {
        return Err(introuvable());
    }
    journaliser(pool, &idn.sub, "badge retiré", &format!("{pseudo} · {nom}")).await;
    Ok(servers::sans_contenu())
}

async fn attribuer_plusieurs(
    State(state): State<AppState>,
    Admin(idn): Admin,
    Path(id): Path<String>,
    corps: Body,
) -> Reponse {
    if !servers::uuid_valide(&id) {
        return Err(introuvable());
    }
    let objet = decode(corps).await?;
    let pseudos: Vec<String> = match champ(&objet, "pseudos") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(l)) => l
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(s.clone()),
                Value::Null => Ok(String::new()),
                _ => Err(mauvais()),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(mauvais()),
    };
    if pseudos.is_empty() || pseudos.len() > 100 {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_pseudos",
            "pseudos : de 1 à 100",
        ));
    }
    let pool = servers::pool(&state)?;
    let nom = infos(pool, &id).await?;
    let (mut attribues, mut inconnus) = (0, Vec::new());
    for p in pseudos {
        if attribuer_a(pool, &id, &p).await.map_err(bd)? {
            attribues += 1;
            journaliser(pool, &idn.sub, "badge attribué", &format!("{p} · {nom}")).await;
        } else {
            inconnus.push(p);
        }
    }
    Ok(write_json(
        StatusCode::OK,
        &json!({ "granted": attribues, "missing": inconnus }),
    ))
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const NON_AUTH: (&str, &str) = ("401", "Non authentifié");
const INTERDIT: (&str, &str) = ("403", "forbidden");
const INTROUVABLE: (&str, &str) = ("404", "not_found");

const DOC: &[Doc] = &[
    (
        HttpMethod::Get,
        "/users/{id}/badges",
        "compte",
        "Badges d'un utilisateur affichés dans son profil ; liste vide si l'on ne partage ni serveur ni message privé avec lui",
        &[("200", "ok"), NON_AUTH, INTROUVABLE],
    ),
    (
        HttpMethod::Get,
        "/admin/badges",
        "administration",
        "Badges et pseudos qui les portent. Permission `estompe:admin`",
        &[("200", "ok"), NON_AUTH, INTERDIT],
    ),
    (
        HttpMethod::Post,
        "/admin/badges",
        "administration",
        "Crée un badge. Journalisé. Permission `estompe:admin`",
        &[
            ("201", "ok"),
            (
                "400",
                "invalid_name / invalid_icon / invalid_color / invalid_description / invalid_order",
            ),
            NON_AUTH,
            INTERDIT,
            ("409", "badge_exists"),
        ],
    ),
    (
        HttpMethod::Put,
        "/admin/badges/{id}",
        "administration",
        "Modifie nom, couleur, description et ordre d'affichage. Journalisé. Permission `estompe:admin`",
        &[
            ("200", "ok"),
            (
                "400",
                "invalid_name / invalid_color / invalid_description / invalid_order",
            ),
            NON_AUTH,
            INTERDIT,
            INTROUVABLE,
            ("409", "badge_exists"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/admin/badges/{id}",
        "administration",
        "Supprime le badge et le retire à tous. Permission `estompe:admin`",
        &[("204", "ok"), NON_AUTH, INTERDIT, INTROUVABLE],
    ),
    (
        HttpMethod::Put,
        "/admin/badges/{id}/icon",
        "administration",
        "Remplace l'image du badge ; corps = le fichier. SVG nettoyé (64 Ko), WebP fixe, PNG ou JPEG (256 Ko, 512 px au plus). Permission `estompe:admin`",
        &[
            ("204", "ok"),
            ("400", "invalid_image"),
            NON_AUTH,
            INTERDIT,
            INTROUVABLE,
            ("413", "too_large"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/admin/badges/{id}/icon",
        "administration",
        "Supprime l'image du badge. Permission `estompe:admin`",
        &[("204", "ok"), NON_AUTH, INTERDIT, INTROUVABLE],
    ),
    (
        HttpMethod::Get,
        "/badges/{id}/icon",
        "compte",
        "Image du badge (SVG nettoyé, WebP, PNG ou JPEG), à charger quand `icon_version` > 0",
        &[("200", "ok"), NON_AUTH, ("404", "pas d'image")],
    ),
    (
        HttpMethod::Post,
        "/admin/badges/{id}/users",
        "administration",
        "Attribue le badge à plusieurs pseudos (1 à 100). Journalisé. Permission `estompe:admin`",
        &[
            ("200", "ok"),
            ("400", "invalid_pseudos"),
            NON_AUTH,
            INTERDIT,
            ("404", "badge inconnu"),
        ],
    ),
    (
        HttpMethod::Put,
        "/admin/badges/{id}/users/{pseudo}",
        "administration",
        "Attribue le badge au pseudo. Permission `estompe:admin`",
        &[
            ("204", "ok"),
            NON_AUTH,
            INTERDIT,
            ("404", "badge ou pseudo inconnu"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/admin/badges/{id}/users/{pseudo}",
        "administration",
        "Retire le badge au pseudo. Permission `estompe:admin`",
        &[
            ("204", "ok"),
            NON_AUTH,
            INTERDIT,
            ("404", "badge non porté par ce pseudo"),
        ],
    ),
];

pub fn routes() -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::new()
        .route("/users/{id}/badges", get(badges_utilisateur))
        .route("/admin/badges", get(lister).post(creer))
        .route("/admin/badges/{id}", put(modifier).delete(supprimer))
        .route(
            "/admin/badges/{id}/icon",
            put(remplacer_icone).delete(retirer_icone),
        )
        .route("/badges/{id}/icon", get(icone))
        .route(
            "/admin/badges/{id}/users",
            axum::routing::post(attribuer_plusieurs),
        )
        .route(
            "/admin/badges/{id}/users/{pseudo}",
            put(attribuer).delete(retirer),
        );
    for (methode, chemin, tag, resume, reponses) in DOC {
        let op = openapi::operation(tag, resume, reponses, chemin);
        r = openapi::documenter(r, chemin, methode.clone(), op);
    }
    r
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::to_bytes, http::Request};
    use estompe_core::{
        db::migrate_embarquees,
        testutil::{self, BasePostgres, Faux, faux},
    };
    use tower::ServiceExt;

    struct Env {
        app: Router,
        pool: PgPool,
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
        let pool = PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["root", "alice", "bob", "stranger"] {
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
        state.set_pool(pool.clone());
        Some(Env {
            app: Router::from(routes()).with_state(state),
            pool,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn brut(
        e: &Env,
        nom: &str,
        methode: &str,
        chemin: &str,
        type_: &str,
        corps: Vec<u8>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut r = Request::builder()
            .method(methode)
            .uri(chemin)
            .header(header::AUTHORIZATION, format!("Bearer {nom}"));
        if !type_.is_empty() {
            r = r.header(header::CONTENT_TYPE, type_);
        }
        let rep = e
            .app
            .clone()
            .oneshot(r.body(Body::from(corps)).unwrap())
            .await
            .unwrap();
        let (s, h) = (rep.status(), rep.headers().clone());
        let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (s, h, c.to_vec())
    }

    async fn appel(
        e: &Env,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: &str,
    ) -> (StatusCode, Value) {
        let (s, _, c) = brut(e, nom, methode, chemin, "", corps.as_bytes().to_vec()).await;
        (s, serde_json::from_slice(&c).unwrap_or(Value::Null))
    }

    async fn code(e: &Env, nom: &str, methode: &str, chemin: &str, corps: &str) -> u16 {
        appel(e, nom, methode, chemin, corps).await.0.as_u16()
    }

    async fn icone_code(
        e: &Env,
        nom: &str,
        methode: &str,
        chemin: &str,
        type_: &str,
        corps: &[u8],
    ) -> u16 {
        brut(e, nom, methode, chemin, type_, corps.to_vec())
            .await
            .0
            .as_u16()
    }

    fn png(l: u32, h: u32) -> Vec<u8> {
        let mut ihdr = b"IHDR".to_vec();
        ihdr.extend(l.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        let mut sortie = b"\x89PNG\r\n\x1a\n".to_vec();
        sortie.extend(13u32.to_be_bytes());
        sortie.extend(&ihdr);
        sortie.extend(image::crc32(&ihdr).to_be_bytes());
        sortie
    }

    fn webp_64() -> Vec<u8> {
        let mut b = b"RIFF\x1a\x00\x00\x00WEBPVP8L\x0d\x00\x00\x00\x2f".to_vec();
        b.extend([0x3f, 0xc0, 0x0f, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    fn jpeg(l: u16, h: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xC0, 0, 11, 8];
        b.extend(h.to_be_bytes());
        b.extend(l.to_be_bytes());
        b.extend([1, 1, 0x11, 0]);
        b
    }

    async fn creer_badge(e: &Env, corps: &str) -> String {
        let (s, v) = appel(e, "root", "POST", "/admin/badges", corps).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    async fn journal(e: &Env) -> String {
        let l: Vec<(String, String)> = sqlx::query_as("SELECT action, target FROM admin_log")
            .fetch_all(&e.pool)
            .await
            .unwrap();
        format!("{l:?}")
    }

    #[tokio::test]
    async fn edition_des_badges() {
        let Some(e) = env().await else { return };
        let (s, v) = appel(
            &e,
            "root",
            "POST",
            "/admin/badges",
            r##"{"name":"Mentor","color":"#112233","sort_order":5}"##,
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(
            (
                v["icon"].as_str(),
                v["sort_order"].as_i64(),
                v["icon_version"].as_i64()
            ),
            (Some("🏅"), Some(5), Some(0))
        );
        let id = v["id"].as_str().unwrap().to_string();
        let autre = creer_badge(&e, r##"{"name":"Autre","color":"#112233"}"##).await;
        assert_ne!(id, autre);
        let base = format!("/admin/badges/{id}");
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &base,
                r##"{"name":"Guide","color":"#445566"}"##
            )
            .await,
            403
        );
        let (s, v) = appel(
            &e,
            "root",
            "PUT",
            &base,
            r##"{"name":"Guide","color":"#445566","description":"Aide","sort_order":2}"##,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["created_at"], "0001-01-01T00:00:00Z");
        assert_eq!(
            code(
                &e,
                "root",
                "PUT",
                &base,
                r##"{"name":"Autre","color":"#445566"}"##
            )
            .await,
            409
        );
        assert_eq!(
            code(
                &e,
                "root",
                "PUT",
                &base,
                r##"{"name":"Guide","color":"#445566","sort_order":10000}"##
            )
            .await,
            400
        );
        assert_eq!(
            code(
                &e,
                "root",
                "PUT",
                "/admin/badges/00000000-0000-0000-0000-000000000000",
                r##"{"name":"Guide","color":"#445566"}"##
            )
            .await,
            404
        );
        assert_eq!(code(&e, "root", "PUT", "/admin/badges/x", "{}").await, 404);
        let (_, liste) = appel(&e, "root", "GET", "/admin/badges", "").await;
        let guide = liste
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id.as_str())
            .unwrap();
        assert_eq!(
            (guide["name"].as_str(), guide["description"].as_str()),
            (Some("Guide"), Some("Aide"))
        );
        assert!(guide["created_at"].as_str().unwrap().ends_with('Z'));

        let icone = format!("{base}/icon");
        let lire = format!("/badges/{id}/icon");
        assert_eq!(
            icone_code(&e, "alice", "PUT", &icone, "image/png", &png(64, 64)).await,
            403
        );
        assert_eq!(icone_code(&e, "alice", "GET", &lire, "", b"").await, 404);
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/png", &png(64, 64)).await,
            204
        );
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/png", &png(600, 600)).await,
            400
        );
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/png", b"pas une image").await,
            400
        );
        assert_eq!(
            icone_code(
                &e,
                "root",
                "PUT",
                &icone,
                "image/png",
                &vec![0; ICONE_MAX + 1]
            )
            .await,
            413
        );
        assert_eq!(
            icone_code(
                &e,
                "root",
                "PUT",
                &icone,
                "image/svg+xml",
                &vec![b' '; SVG_MAX + 1]
            )
            .await,
            413
        );
        let (s, h, c) = brut(&e, "alice", "GET", &lire, "", vec![]).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=86400");
        assert_eq!(c, png(64, 64));
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/webp", &webp_64()).await,
            204
        );
        assert_eq!(
            brut(&e, "alice", "GET", &lire, "", vec![]).await.1[header::CONTENT_TYPE],
            "image/webp"
        );
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/jpeg", &jpeg(100, 50)).await,
            204
        );
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/jpeg", &jpeg(513, 50)).await,
            400
        );
        let sale = concat!(
            r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10" onload="alert(1)">"##,
            r##"<script>alert(1)</script><foreignObject><div>x</div></foreignObject><image href="http://evil/x.png"/>"##,
            r##"<circle cx="5" cy="5" r="4" fill="url(http://evil/#a)" onclick="x()" stroke="red"/><use xlink:href="javascript:alert(1)"/><path d="M0 0L5 5" fill="url(#g)"/></svg>"##
        );
        assert_eq!(
            icone_code(&e, "root", "PUT", &icone, "image/svg+xml", sale.as_bytes()).await,
            204
        );
        let (_, h, c) = brut(&e, "alice", "GET", &lire, "", vec![]).await;
        let propre = String::from_utf8(c).unwrap();
        for mauvais in [
            "script",
            "alert",
            "onload",
            "onclick",
            "foreignObject",
            "image",
            "evil",
            "javascript",
            "<div",
        ] {
            assert!(!propre.contains(mauvais), "{mauvais} : {propre}");
        }
        for bon in [
            r##"viewBox="0 0 10 10""##,
            r##"<circle cx="5" cy="5" r="4" stroke="red">"##,
            r##"fill="url(#g)""##,
        ] {
            assert!(propre.contains(bon), "{bon} : {propre}");
        }
        assert_eq!(h[header::CONTENT_TYPE], "image/svg+xml");
        assert!(
            h[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("sandbox")
        );
        for mauvais in [
            r##"<svg xmlns="http://www.w3.org/2000/svg"></svg>"##,
            r##"<!DOCTYPE svg [<!ENTITY x "y">]><svg viewBox="0 0 1 1"/>"##,
            "<html/>",
        ] {
            assert_eq!(
                icone_code(
                    &e,
                    "root",
                    "PUT",
                    &icone,
                    "image/svg+xml",
                    mauvais.as_bytes()
                )
                .await,
                400,
                "{mauvais}"
            );
        }
        let (_, liste) = appel(&e, "root", "GET", "/admin/badges", "").await;
        let version = liste
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id.as_str())
            .unwrap()["icon_version"]
            .clone();
        assert_eq!(version, 4);
        assert_eq!(icone_code(&e, "root", "DELETE", &icone, "", b"").await, 204);
        assert_eq!(icone_code(&e, "alice", "GET", &lire, "", b"").await, 404);
        assert_eq!(
            icone_code(
                &e,
                "root",
                "DELETE",
                "/admin/badges/00000000-0000-0000-0000-000000000000/icon",
                "",
                b""
            )
            .await,
            404
        );

        let (s, v) = appel(
            &e,
            "root",
            "POST",
            &format!("{base}/users"),
            r#"{"pseudos":["alice","bob","inconnu"]}"#,
        )
        .await;
        assert_eq!(
            (s, v),
            (
                StatusCode::OK,
                json!({"granted": 2, "missing": ["inconnu"]})
            )
        );
        assert_eq!(
            code(
                &e,
                "root",
                "POST",
                &format!("{base}/users"),
                r#"{"pseudos":[]}"#
            )
            .await,
            400
        );
        assert_eq!(
            code(
                &e,
                "root",
                "POST",
                &format!("{base}/users"),
                r#"{"pseudos":"alice"}"#
            )
            .await,
            400
        );
        let cent: Vec<String> = (0..101).map(|i| format!("\"p{i}\"")).collect();
        assert_eq!(
            code(
                &e,
                "root",
                "POST",
                &format!("{base}/users"),
                &format!("{{\"pseudos\":[{}]}}", cent.join(","))
            )
            .await,
            400
        );
        assert_eq!(
            code(
                &e,
                "root",
                "POST",
                "/admin/badges/00000000-0000-0000-0000-000000000000/users",
                r#"{"pseudos":["alice"]}"#
            )
            .await,
            404
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "POST",
                &format!("{base}/users"),
                r#"{"pseudos":["bob"]}"#
            )
            .await,
            403
        );
        assert_eq!(
            code(&e, "root", "DELETE", &format!("{base}/users/bob"), "").await,
            204
        );
        assert_eq!(code(&e, "root", "DELETE", &base, "").await, 204);

        let j = journal(&e).await;
        for voulu in [
            "badge créé",
            "badge modifié",
            "badge : icône modifiée",
            "badge attribué",
            "badge retiré",
            "badge supprimé",
            "Guide",
            "bob · Guide",
        ] {
            assert!(j.contains(voulu), "{voulu} : {j}");
        }
    }

    #[tokio::test]
    async fn creation_invalide() {
        let Some(e) = env().await else { return };
        assert_eq!(
            code(
                &e,
                "alice",
                "POST",
                "/admin/badges",
                r##"{"name":"Test","icon":"⭐","color":"#112233"}"##
            )
            .await,
            403
        );
        creer_badge(&e, r##"{"name":"Étoile","icon":"⭐","color":"#112233"}"##).await;
        for (corps, attendu) in [
            (r##"{"name":"Étoile","icon":"⭐","color":"#112233"}"##, 409),
            (r##"{"name":"X","color":"#112233"}"##, 400),
            (r##"{"name":"Rouge","color":"red"}"##, 400),
            (r##"{"name":"Rouge","color":"#12345g"}"##, 400),
            (
                r##"{"name":"Ok","icon":"123456789","color":"#112233"}"##,
                400,
            ),
            (
                r##"{"name":"Ok","color":"#112233","description":"é"}"##,
                201,
            ),
            (
                r##"{"name":"Long","color":"#112233","description":"ééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééé"}"##,
                400,
            ),
            (r##"{"name":"Neg","color":"#112233","sort_order":-1}"##, 400),
            (
                r##"{"name":"Flottant","color":"#112233","sort_order":1.5}"##,
                400,
            ),
            (r##"{"name":5,"color":"#112233"}"##, 400),
            (r##"{"name":"Id","color":"#112233","id":5}"##, 400),
            (
                r##"{"name":"Ver","color":"#112233","icon_version":"1"}"##,
                400,
            ),
            (
                r##"{"name":"Ver","color":"#112233","icon_version":1.5}"##,
                400,
            ),
            (r##"{"name":"Date","color":"#112233","created_at":5}"##, 400),
            (
                r##"{"name":"Date","color":"#112233","created_at":"hier"}"##,
                400,
            ),
            (
                r##"{"name":"Nul","color":"#112233","id":null,"icon_version":null,"created_at":null}"##,
                201,
            ),
            ("[]", 400),
            ("", 400),
        ] {
            assert_eq!(
                code(&e, "root", "POST", "/admin/badges", corps).await,
                attendu,
                "{corps}"
            );
        }
        let (_, v) = appel(
            &e,
            "root",
            "POST",
            "/admin/badges",
            r##"{"name":"  Espaces  ","color":"#AbCdEf"}"##,
        )
        .await;
        assert_eq!(v["name"], "Espaces");
    }

    #[tokio::test]
    async fn attribution_et_profil() {
        let Some(e) = env().await else { return };
        sqlx::raw_sql(
            "INSERT INTO servers (name, owner_id) SELECT 'S', id FROM users WHERE pseudo='alice';
             INSERT INTO server_members (server_id, user_id) SELECT s.id, u.id FROM servers s, users u WHERE u.pseudo IN ('alice','bob');",
        )
        .execute(&e.pool)
        .await
        .unwrap();
        let moi: String = sqlx::query_scalar("SELECT id::text FROM users WHERE pseudo='alice'")
            .fetch_one(&e.pool)
            .await
            .unwrap();
        let id = creer_badge(
            &e,
            r##"{"name":"Étoile","icon":"⭐","color":"#112233","description":"Une étoile"}"##,
        )
        .await;
        let second = creer_badge(
            &e,
            r##"{"name":"Premier","color":"#112233","sort_order":0}"##,
        )
        .await;
        let chemin = format!("/admin/badges/{id}/users/alice");
        assert_eq!(code(&e, "root", "PUT", &chemin, "").await, 204);
        assert_eq!(code(&e, "root", "PUT", &chemin, "").await, 204);
        assert_eq!(
            code(
                &e,
                "root",
                "PUT",
                &format!("/admin/badges/{id}/users/inconnu"),
                ""
            )
            .await,
            404
        );
        assert_eq!(
            code(
                &e,
                "root",
                "PUT",
                &format!("/admin/badges/{}/users/alice", "0".repeat(36)),
                ""
            )
            .await,
            404
        );
        assert_eq!(code(&e, "alice", "PUT", &chemin, "").await, 403);
        assert_eq!(
            code(
                &e,
                "root",
                "PUT",
                &format!("/admin/badges/{second}/users/alice"),
                ""
            )
            .await,
            204
        );
        let voit = |qui: &'static str| {
            let (e, moi) = (&e, moi.clone());
            async move {
                let (s, v) = appel(e, qui, "GET", &format!("/users/{moi}/badges"), "").await;
                assert_eq!(s, StatusCode::OK);
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b["name"].as_str().unwrap().to_string())
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(voit("alice").await, ["Étoile", "Premier"]);
        assert_eq!(voit("bob").await, ["Étoile", "Premier"]);
        assert!(voit("stranger").await.is_empty());
        sqlx::raw_sql(
            "INSERT INTO dm_conversations (user_a, user_b) SELECT LEAST(a.id, s.id), GREATEST(a.id, s.id) FROM users a, users s WHERE a.pseudo='alice' AND s.pseudo='stranger'",
        )
        .execute(&e.pool)
        .await
        .unwrap();
        assert_eq!(voit("stranger").await.len(), 2);
        assert_eq!(
            code(&e, "alice", "GET", "/users/pas-un-uuid/badges", "").await,
            404
        );
        let (_, liste) = appel(&e, "root", "GET", "/admin/badges", "").await;
        let premier = liste
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "Premier")
            .unwrap();
        let titulaires = &premier["holders"];
        assert_eq!(titulaires[0]["pseudo"], "alice");
        assert_eq!(titulaires[0]["id"], moi.as_str());

        assert_eq!(code(&e, "root", "DELETE", &chemin, "").await, 204);
        assert_eq!(code(&e, "root", "DELETE", &chemin, "").await, 404);
        assert_eq!(voit("alice").await, ["Premier"]);
        assert_eq!(code(&e, "root", "PUT", &chemin, "").await, 204);
        assert_eq!(
            code(&e, "root", "DELETE", &format!("/admin/badges/{id}"), "").await,
            204
        );
        assert_eq!(
            code(&e, "root", "DELETE", &format!("/admin/badges/{id}"), "").await,
            404
        );
        assert_eq!(voit("alice").await, ["Premier"]);
    }

    #[tokio::test]
    async fn acces_sans_identite() {
        let Some(e) = env().await else { return };
        for (m, c) in [
            ("GET", "/admin/badges"),
            ("GET", "/users/x/badges"),
            ("GET", "/badges/x/icon"),
        ] {
            let r = e
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
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "{c}");
        }
    }

    #[test]
    fn nettoyage_svg() {
        let sortie = |s: &str| String::from_utf8(svg::nettoyer(s.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            sortie(r##"<svg viewBox="0 0 1 1" width='a"b&amp;c'><g><path d="M0 0"/></g></svg>"##),
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1" width="a&#34;b&amp;c"><g><path d="M0 0"></path></g></svg>"##
        );
        assert_eq!(
            sortie(
                r##"<svg viewBox="0 0 1 1"><use href="#a"/><use href="http://x"/><style>a{}</style><g mask="url( '#m')" clip-path="url(x)"/></svg>"##
            ),
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><use href="#a"></use><use></use><g mask="url( &#39;#m&#39;)"></g></svg>"##
        );
        for mauvais in [
            "",
            "pas du xml",
            "<svg viewBox='0 0 1 1'>",
            "<svg viewBox='0 0 1 1'></g>",
            "<svg viewBox='0 0 1 1' d='&x;'/>",
        ] {
            assert!(svg::nettoyer(mauvais.as_bytes()).is_err(), "{mauvais}");
        }
        assert!(svg::nettoyer(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn dimensions_des_images() {
        assert_eq!(image::dimensions_png(&png(3, 4)), Some((3, 4)));
        let mut mauvais_crc = png(3, 4);
        *mauvais_crc.last_mut().unwrap() ^= 1;
        assert_eq!(image::dimensions_png(&mauvais_crc), None);
        assert_eq!(image::dimensions_png(&png(0, 4)), None);
        assert_eq!(image::dimensions_jpeg(&jpeg(30, 20)), Some((30, 20)));
        let mut sof3 = jpeg(30, 20);
        sof3[3] = 0xC3;
        assert_eq!(image::dimensions_jpeg(&sof3), None);
        assert_eq!(image::dimensions_jpeg(&[0xFF, 0xD8, 0xFF, 0xD9]), None);
        assert_eq!(image::dimensions_webp(&webp_64()), Some((64, 64)));
        let mut anime = b"RIFF\x1a\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00".to_vec();
        anime.extend([0x02, 0, 0, 0, 9, 0, 0, 9, 0, 0, 0, 0]);
        assert_eq!(image::dimensions_webp(&anime), None);
        anime[20] = 0;
        assert_eq!(image::dimensions_webp(&anime), Some((10, 10)));
        assert_eq!(image::dimensions_webp(b"RIFF"), None);
    }

    #[test]
    fn couleurs() {
        assert!(couleur_valide("#0aF9b8"));
        for mauvais in ["", "#fff", "112233", "#11223", "#1122334", "#11223é"] {
            assert!(!couleur_valide(mauvais), "{mauvais}");
        }
    }
}
