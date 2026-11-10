use std::collections::{HashMap, HashSet};

use axum::{
    body::{Body, to_bytes},
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use rand::Rng;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    http::{CORPS_MAX, champ, champ_texte, decode, internal, premier_objet, write_err, write_json},
    openapi,
    roles::{self, Access, Perm},
    state::{AppState, DbIndisponible},
    users::User,
};

pub const MAX_SALONS: i64 = 50;
pub const MAX_ROLES: usize = 50;
pub const MAX_PARTICIPANTS_VOCAL: i64 = 10;

const ALPHABET_CODE: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
const TYPES: [&str; 5] = ["text", "voice", "videos", "music", "cinema"];
pub const TYPES_VOCAUX: [&str; 2] = ["voice", "cinema"];

pub fn cinema_disponible(state: &AppState) -> bool {
    let actif = |nom: &str| state.blocs_actifs().iter().any(|b| b == nom);
    actif("films") || (actif("regarder-ensemble") && state.config().entracte_actif())
}

pub fn est_vocal(kind: &str) -> bool {
    TYPES_VOCAUX.contains(&kind)
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/servers", post(creer).get(lister))
        .route("/servers/{id}", get(voir).patch(renommer).delete(supprimer))
        .route("/servers/{id}/leave", post(quitter))
        .route("/servers/{id}/channels", post(creer_salon))
        .route(
            "/servers/{id}/channels/order",
            put(ordonner).patch(renommer_salon).delete(supprimer_salon),
        )
        .route(
            "/servers/{id}/channels/{cid}",
            patch(renommer_salon).delete(supprimer_salon),
        )
        .route(
            "/servers/{id}/invites",
            get(lister_invitations).post(creer_invitation),
        )
        .route("/servers/{id}/invites/{code}", delete(revoquer))
        .route("/server-invites/{code}/join", post(rejoindre))
        .merge(crate::gestion_roles::router())
        .merge(crate::moderation::router())
        .merge(crate::messages::router())
        .merge(crate::sondages::router())
        .merge(crate::mentions::router())
        .merge(crate::recherche::router())
        .merge(crate::etat_lecture::router());
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("serveurs", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Post,
        "/servers",
        "Créer un serveur (avec un salon texte « général » et un salon vocal « vocal »)",
        &[("201", "Créé"), ("400", "Requête invalide")],
    ),
    (
        HttpMethod::Get,
        "/servers",
        "Serveurs de l'utilisateur",
        &[("200", "ok")],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}",
        "Détail d'un serveur (membre uniquement)",
        &[
            ("200", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/servers/{id}",
        "Renommer (permission `manage_server`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}",
        "Supprimer (créateur)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/leave",
        "Quitter un serveur",
        &[
            ("204", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "Conflit"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/channels",
        "Créer un salon (permission `manage_channels` ; 50 max par serveur)",
        &[
            ("201", "Créé"),
            ("400", "Requête invalide"),
            ("403", "Refusé"),
            ("409", "Conflit"),
        ],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/channels/order",
        "Réordonner les salons d'un type (permission `manage_channels`)",
        &[
            ("204", "ok"),
            ("400", "Requête invalide"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/servers/{id}/channels/{cid}",
        "Renommer un salon (permission `manage_channels`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "Conflit"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/channels/{cid}",
        "Supprimer un salon et ses messages (permission `manage_channels`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/invites",
        "Liens d'invitation actifs (permission `create_invites`)",
        &[("200", "ok")],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/invites",
        "Créer un lien d'invitation, réutilisable jusqu'à révocation, épuisement ou expiration (permission `create_invites`)",
        &[("201", "Créé"), ("400", "Requête invalide")],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/invites/{code}",
        "Révoquer un lien (permission `create_invites`)",
        &[
            ("204", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/server-invites/{code}/join",
        "Rejoindre un serveur avec un code (sans effet si déjà membre)",
        &[
            ("200", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
];

pub type Params = Path<HashMap<String, String>>;

enum Echec {
    NonTrouve,
    NomPris,
    TropDeSalons,
    Banni,
    Base(sqlx::Error),
}

impl From<sqlx::Error> for Echec {
    fn from(e: sqlx::Error) -> Self {
        Echec::Base(e)
    }
}

pub fn unique(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.code().as_deref() == Some("23505"))
}

pub fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    internal()
}

fn echec_interne(e: Echec) -> Response {
    match e {
        Echec::Base(e) => bd(e),
        _ => internal(),
    }
}

pub fn mauvais() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

pub fn non_trouve(message: &str) -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", message)
}

pub fn interdit() -> Response {
    write_err(StatusCode::FORBIDDEN, "forbidden", "permission manquante")
}

pub fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub fn pool(state: &AppState) -> Result<&PgPool, Response> {
    state.pool().map_err(DbIndisponible::into_response)
}

pub fn uuid_valide(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

pub fn nom_propre(s: &str, max: usize) -> Option<String> {
    let s = s.trim();
    let n = s.chars().count();
    (1..=max)
        .contains(&n)
        .then_some(s)
        .filter(|s| !s.chars().any(char::is_control))
        .map(str::to_string)
}

pub fn entier(o: &Map<String, Value>, nom: &str) -> Option<i64> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(0),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

pub fn booleen(o: &Map<String, Value>, nom: &str) -> Option<bool> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

pub fn liste(o: &Map<String, Value>, nom: &str) -> Option<Vec<String>> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>(),
        _ => None,
    }
}

pub fn rfc3339(s: &str) -> String {
    match s.split_once('.') {
        Some((debut, fraction)) => match fraction.trim_end_matches('0') {
            "" => format!("{debut}Z"),
            f => format!("{debut}.{f}Z"),
        },
        None => format!("{s}Z"),
    }
}

pub fn horodatage(colonne: &str) -> String {
    format!("to_char({colonne} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US')")
}

fn nouveau_code() -> String {
    let mut b = [0u8; 10];
    rand::rng().fill_bytes(&mut b);
    let c = |x: u8| ALPHABET_CODE[x as usize % ALPHABET_CODE.len()] as char;
    let (a, z) = b.split_at(5);
    format!(
        "{}-{}",
        a.iter().copied().map(c).collect::<String>(),
        z.iter().copied().map(c).collect::<String>()
    )
}

pub struct Serveur {
    pub id: String,
    name: String,
    pub(crate) owner_id: String,
    icon_version: i64,
}

impl Serveur {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "owner_id": self.owner_id,
            "icon_version": self.icon_version,
        })
    }
}

type LigneServeur = (String, String, String, i64);

impl From<LigneServeur> for Serveur {
    fn from((id, name, owner_id, icon_version): LigneServeur) -> Self {
        Serveur {
            id,
            name,
            owner_id,
            icon_version,
        }
    }
}

struct Salon {
    id: String,
    name: String,
    kind: String,
    position: i32,
    slowmode: i32,
    locked: bool,
    topic: String,
    user_limit: i32,
}

impl Salon {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "kind": self.kind,
            "position": self.position,
            "slowmode": self.slowmode,
            "locked": self.locked,
            "topic": self.topic,
            "user_limit": self.user_limit,
        })
    }
}

struct NouveauSalon {
    name: String,
    kind: String,
    topic: String,
    slowmode: i32,
    user_limit: i32,
    after: String,
    prive: bool,
    roles: Vec<String>,
    membres: Vec<String>,
    createur: String,
}

pub(crate) async fn serveur_pour(
    pool: &PgPool,
    server_id: &str,
    user: &str,
) -> Result<Option<Serveur>, sqlx::Error> {
    Ok(sqlx::query_as::<_, LigneServeur>(
        "SELECT s.id::text, s.name, s.owner_id::text, s.icon_version FROM servers s
         JOIN server_members m ON m.server_id = s.id WHERE s.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(server_id)
    .bind(user)
    .fetch_optional(pool)
    .await?
    .map(Serveur::from))
}

pub async fn acces(
    state: &AppState,
    u: &User,
    id: &str,
    besoin: Perm,
) -> Result<(Serveur, Access), Response> {
    if !uuid_valide(id) {
        return Err(non_trouve("serveur introuvable"));
    }
    let pool = state.pool().map_err(IntoResponse::into_response)?;
    let sv = serveur_pour(pool, id, &u.id)
        .await
        .map_err(bd)?
        .ok_or_else(|| non_trouve("serveur introuvable"))?;
    let acc = state
        .acces_serveur(&sv.id, &u.id)
        .await
        .map_err(bd)?
        .ok_or_else(internal)?;
    if !acc.can(besoin) {
        return Err(interdit());
    }
    Ok((sv, acc))
}

pub fn param<'a>(p: &'a HashMap<String, String>, nom: &str) -> &'a str {
    p.get(nom).map_or("", String::as_str)
}

pub(crate) fn id_salon(p: &HashMap<String, String>) -> Result<String, Response> {
    let cid = p.get("cid").map_or("order", String::as_str);
    if uuid_valide(cid) {
        Ok(cid.to_string())
    } else {
        Err(non_trouve("salon introuvable"))
    }
}

pub async fn membres_ids(pool: &PgPool, server_id: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT user_id::text FROM server_members WHERE server_id=$1::uuid")
        .bind(server_id)
        .fetch_all(pool)
        .await
}

fn diffuser(state: &AppState, to: &[String], typ: &str, data: &Value) {
    if !to.is_empty() {
        state.publier(to, typ, data);
    }
}

pub async fn evenement_serveur(
    state: &AppState,
    pool: &PgPool,
    server_id: &str,
    typ: &str,
    data: &Value,
) {
    if let Ok(to) = membres_ids(pool, server_id).await {
        diffuser(state, &to, typ, data);
    }
}

pub(crate) async fn evenement_salon(
    state: &AppState,
    pool: &PgPool,
    server_id: &str,
    channel_id: &str,
    typ: &str,
    data: &Value,
) {
    if let Ok(to) = roles::voyeurs_du_salon(pool, server_id, channel_id).await {
        diffuser(state, &to, typ, data);
    }
}

async fn creer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let entree = decode(body).await?;
    let brut = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    let Some(nom) = nom_propre(&brut, 50) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "nom : 1 à 50 caractères",
        ));
    };
    let sv = creer_serveur(pool, &u.id, &nom).await.map_err(bd)?;
    Ok(write_json(StatusCode::CREATED, &sv.json()))
}

async fn creer_serveur(pool: &PgPool, owner: &str, name: &str) -> Result<Serveur, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id: String = sqlx::query_scalar(
        "INSERT INTO servers (name, owner_id) VALUES ($1,$2::uuid) RETURNING id::text",
    )
    .bind(name)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)")
        .bind(&id)
        .bind(owner)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO roles (server_id, name, position, permissions, is_default) VALUES ($1::uuid,'@tout le monde',0,$2,true)",
    )
    .bind(&id)
    .bind(roles::EVERYONE as i64)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid,'général','text'), ($1::uuid,'vocal','voice')",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Serveur {
        id,
        name: name.to_string(),
        owner_id: owner.to_string(),
        icon_version: 0,
    })
}

async fn lister(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let l: Vec<Value> = sqlx::query_as::<_, LigneServeur>(
        "SELECT s.id::text, s.name, s.owner_id::text, s.icon_version FROM servers s
         JOIN server_members m ON m.server_id = s.id WHERE m.user_id=$1::uuid ORDER BY s.created_at",
    )
    .bind(&u.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?
    .into_iter()
    .map(|l| Serveur::from(l).json())
    .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn salons(pool: &PgPool, server_id: &str) -> Result<Vec<Salon>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (String, String, String, i32, i32, bool, String, i32)>(
        "SELECT id::text, name, kind, position, slowmode, locked, topic, user_limit FROM channels WHERE server_id=$1::uuid ORDER BY kind DESC, position",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(
        |(id, name, kind, position, slowmode, locked, topic, user_limit)| Salon {
            id,
            name,
            kind,
            position,
            slowmode,
            locked,
            topic,
            user_limit,
        },
    )
    .collect())
}

type LigneMembre = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);

async fn membres(pool: &PgPool, server_id: &str) -> Result<Vec<LigneMembre>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT u.id::text, u.pseudo, m.nickname, {}, {}, {}, u.banner_color FROM server_members m JOIN users u ON u.id = m.user_id
         WHERE m.server_id=$1::uuid ORDER BY lower(u.pseudo)",
        horodatage("m.timeout_until"),
        horodatage("m.joined_at"),
        horodatage("u.created_at"),
    )))
    .bind(server_id)
    .fetch_all(pool)
    .await
}

async fn voir(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), 0).await?;
    let (salons, membres, roles_liste, roles_membres) = tokio::try_join!(
        salons(pool, &sv.id),
        membres(pool, &sv.id),
        roles::roles_du_serveur(pool, &sv.id),
        roles::roles_des_membres(pool, &sv.id),
    )
    .map_err(bd)?;
    let salons: Vec<Value> = salons
        .iter()
        .filter_map(|c| {
            let perms = acc.channel(&c.id);
            roles::contient(perms, roles::VIEW_CHANNEL).then(|| {
                let mut v = c.json();
                v["permissions"] = json!(roles::cles(perms));
                v
            })
        })
        .collect();
    let membres: Vec<Value> = membres
        .into_iter()
        .map(|(id, pseudo, nickname, timeout, joined, created, banner)| {
            json!({
                "id": id,
                "pseudo": pseudo,
                "nickname": nickname,
                "timeout_until": timeout.map(|t| rfc3339(&t)),
                "joined_at": rfc3339(&joined),
                "created_at": rfc3339(&created),
                "banner_color": banner,
                "online": state.hub().online(&id),
                "roles": roles_membres.get(&id).cloned().unwrap_or_default(),
            })
        })
        .collect();
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "server": sv.json(),
            "channels": salons,
            "members": membres,
            "roles": roles_liste,
            "permissions": roles::cles(acc.base()),
        }),
    ))
}

async fn renommer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_SERVER).await?;
    let entree = decode(body).await?;
    let brut = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    let Some(nom) = nom_propre(&brut, 50) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "nom : 1 à 50 caractères",
        ));
    };
    sqlx::query("UPDATE servers SET name=$2 WHERE id=$1::uuid")
        .bind(&sv.id)
        .bind(&nom)
        .execute(pool)
        .await
        .map_err(bd)?;
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "server.updated",
        &json!({ "id": sv.id, "name": nom }),
    )
    .await;
    Ok(sans_contenu())
}

async fn supprimer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    if sv.owner_id != u.id {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "réservé au créateur du serveur",
        ));
    }
    let to = membres_ids(pool, &sv.id).await.unwrap_or_default();
    sqlx::query("DELETE FROM servers WHERE id=$1::uuid")
        .bind(&sv.id)
        .execute(pool)
        .await
        .map_err(bd)?;
    diffuser(&state, &to, "server.deleted", &json!({ "id": sv.id }));
    Ok(sans_contenu())
}

async fn quitter(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    if sv.owner_id == u.id {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "owner_cannot_leave",
            "le créateur ne peut pas quitter son serveur, il peut le supprimer",
        ));
    }
    sqlx::query("DELETE FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid")
        .bind(&sv.id)
        .bind(&u.id)
        .execute(pool)
        .await
        .map_err(bd)?;
    let mut to = membres_ids(pool, &sv.id).await.unwrap_or_default();
    to.push(u.id.clone());
    diffuser(
        &state,
        &to,
        "member.left",
        &json!({ "server_id": sv.id, "user_id": u.id }),
    );
    Ok(sans_contenu())
}

fn salon_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_channel",
        "nom : 1 à 30 caractères ; type : text, voice, videos, music ou cinema",
    )
}

async fn creer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), 0).await?;
    let e = decode(body).await?;
    let nom_brut = champ_texte(&e, "name").ok_or_else(mauvais)?;
    let kind = champ_texte(&e, "kind").ok_or_else(mauvais)?;
    let sujet_brut = champ_texte(&e, "topic").ok_or_else(mauvais)?;
    let after = champ_texte(&e, "after").ok_or_else(mauvais)?;
    let slowmode = entier(&e, "slowmode").ok_or_else(mauvais)?;
    let user_limit = entier(&e, "user_limit").ok_or_else(mauvais)?;
    let prive = booleen(&e, "private").ok_or_else(mauvais)?;
    let roles_ids = liste(&e, "roles").ok_or_else(mauvais)?;
    let membres_ids = liste(&e, "members").ok_or_else(mauvais)?;
    if !acc.can(roles::MANAGE_CHANNELS) {
        return Ok(interdit());
    }
    let nom = nom_propre(&nom_brut, 30);
    let sujet = sujet_brut.trim().to_string();
    let invalide =
        |code: &str, message: &str| Ok(write_err(StatusCode::BAD_REQUEST, code, message));
    if nom.is_none() || !TYPES.contains(&kind.as_str()) {
        return Ok(salon_invalide());
    }
    if kind == "cinema" && !cinema_disponible(&state) {
        return invalide(
            "invalid_channel",
            "salon cinéma : ni Entracte ni la bibliothèque de films ne sont branchés sur ce serveur",
        );
    }
    if sujet.chars().count() > 200 {
        return invalide("invalid_topic", "sujet : 200 caractères maximum");
    }
    if slowmode != 0 && (kind != "text" || !(5..=21600).contains(&slowmode)) {
        return invalide(
            "invalid_slowmode",
            "mode lent : salon texte, désactivé (0) ou de 5 s à 6 h",
        );
    }
    if user_limit != 0 && (!est_vocal(&kind) || !(2..=MAX_PARTICIPANTS_VOCAL).contains(&user_limit))
    {
        return invalide(
            "invalid_user_limit",
            &format!("limite : salon vocal, de 2 à {MAX_PARTICIPANTS_VOCAL} participants"),
        );
    }
    if !prive && (!roles_ids.is_empty() || !membres_ids.is_empty()) {
        return invalide(
            "invalid_channel",
            "des rôles ou membres n'ont de sens que pour un salon privé",
        );
    }
    if prive
        && (roles_ids.len() > MAX_ROLES
            || membres_ids.len() > 200
            || roles_ids
                .iter()
                .chain(&membres_ids)
                .any(|id| !uuid_valide(id)))
    {
        return invalide("invalid_channel", "rôles ou membres invalides");
    }
    if prive && roles::VIEW_CHANNEL & !acc.base() != 0 {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas réserver un salon que vous ne voyez pas",
        ));
    }
    let mut roles_tries = roles_ids;
    roles_tries.sort();
    roles_tries.dedup();
    let nouveau = NouveauSalon {
        name: nom.unwrap_or_default(),
        kind,
        topic: sujet,
        slowmode: slowmode as i32,
        user_limit: user_limit as i32,
        after,
        prive,
        roles: roles_tries,
        membres: membres_ids,
        createur: u.id.clone(),
    };
    match creer_salon_base(pool, &sv.id, &nouveau).await {
        Ok(c) => {
            let ev = json!({ "server_id": sv.id, "channel": c.json(), "after": nouveau.after });
            if prive {
                evenement_salon(&state, pool, &sv.id, &c.id, "channel.created", &ev).await;
            } else {
                evenement_serveur(&state, pool, &sv.id, "channel.created", &ev).await;
            }
            Ok(write_json(StatusCode::CREATED, &c.json()))
        }
        Err(Echec::NomPris) => Ok(nom_pris()),
        Err(Echec::TropDeSalons) => Ok(write_err(
            StatusCode::CONFLICT,
            "too_many_channels",
            "50 salons maximum par serveur",
        )),
        Err(Echec::NonTrouve) => invalide(
            "invalid_channel",
            "rôle ou membre introuvable sur ce serveur",
        ),
        Err(e) => Ok(echec_interne(e)),
    }
}

fn nom_pris() -> Response {
    write_err(
        StatusCode::CONFLICT,
        "name_taken",
        "un salon de ce type porte déjà ce nom",
    )
}

async fn creer_salon_base(
    pool: &PgPool,
    server_id: &str,
    n: &NouveauSalon,
) -> Result<Salon, Echec> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM servers WHERE id=$1::uuid FOR UPDATE")
        .bind(server_id)
        .execute(&mut *tx)
        .await?;
    let existants: i64 =
        sqlx::query_scalar("SELECT count(*) FROM channels WHERE server_id=$1::uuid")
            .bind(server_id)
            .fetch_one(&mut *tx)
            .await?;
    if existants >= MAX_SALONS {
        return Err(Echec::TropDeSalons);
    }
    let (id, position) = sqlx::query_as::<_, (String, i32)>(
        "INSERT INTO channels (server_id, name, kind, topic, slowmode, user_limit) VALUES ($1::uuid,$2,$3,$4,$5,$6) RETURNING id::text, position",
    )
    .bind(server_id)
    .bind(&n.name)
    .bind(&n.kind)
    .bind(&n.topic)
    .bind(n.slowmode)
    .bind(n.user_limit)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| if unique(&e) { Echec::NomPris } else { Echec::Base(e) })?;
    let mut c = Salon {
        id,
        name: n.name.clone(),
        kind: n.kind.clone(),
        position,
        slowmode: n.slowmode,
        locked: false,
        topic: n.topic.clone(),
        user_limit: n.user_limit,
    };
    if !n.after.is_empty() {
        c.position = placer_apres(&mut tx, server_id, &c, &n.after).await?;
    }
    if n.prive {
        rendre_prive(&mut tx, server_id, &c.id, n).await?;
    }
    tx.commit().await?;
    Ok(c)
}

async fn placer_apres(
    tx: &mut sqlx::PgConnection,
    server_id: &str,
    c: &Salon,
    after: &str,
) -> Result<i32, sqlx::Error> {
    let mut ids: Vec<String> = sqlx::query_scalar(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind=$2 AND id<>$3::uuid ORDER BY position",
    )
    .bind(server_id)
    .bind(&c.kind)
    .bind(&c.id)
    .fetch_all(&mut *tx)
    .await?;
    let Some(i) = ids.iter().position(|id| id == after) else {
        return Ok(c.position);
    };
    ids.insert(i + 1, c.id.clone());
    for (j, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE channels SET position=$2 WHERE id=$1::uuid")
            .bind(id)
            .bind(j as i32 + 1)
            .execute(&mut *tx)
            .await?;
    }
    Ok(i as i32 + 2)
}

async fn rendre_prive(
    tx: &mut sqlx::PgConnection,
    server_id: &str,
    channel_id: &str,
    n: &NouveauSalon,
) -> Result<(), Echec> {
    let voir = roles::VIEW_CHANNEL as i64;
    sqlx::query(
        "INSERT INTO channel_overrides (channel_id, role_id, deny) SELECT $1::uuid, id, $3 FROM roles WHERE server_id=$2::uuid AND is_default",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(voir)
    .execute(&mut *tx)
    .await?;
    let ajoutes = sqlx::query(
        "INSERT INTO channel_overrides (channel_id, role_id, allow) SELECT $1::uuid, id, $3 FROM roles WHERE server_id=$2::uuid AND NOT is_default AND id::text = ANY($4)",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(voir)
    .bind(&n.roles)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if ajoutes as usize != n.roles.len() {
        return Err(Echec::NonTrouve);
    }
    let mut membres: Vec<String> = n.membres.clone();
    membres.push(n.createur.clone());
    membres.sort();
    membres.dedup();
    let ajoutes = sqlx::query(
        "INSERT INTO channel_member_overrides (channel_id, user_id, allow) SELECT $1::uuid, user_id, $3 FROM server_members WHERE server_id=$2::uuid AND user_id::text = ANY($4)",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(voir)
    .bind(&membres)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if ajoutes as usize != membres.len() {
        return Err(Echec::NonTrouve);
    }
    Ok(())
}

async fn renommer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p)?;
    let entree = decode(body).await?;
    let brut = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    let Some(nom) = nom_propre(&brut, 30) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "nom : 1 à 30 caractères",
        ));
    };
    let r = sqlx::query("UPDATE channels SET name=$3 WHERE id=$2::uuid AND server_id=$1::uuid")
        .bind(&sv.id)
        .bind(&cid)
        .bind(&nom)
        .execute(pool)
        .await;
    match r {
        Err(e) if unique(&e) => Ok(nom_pris()),
        Err(e) => Err(bd(e)),
        Ok(r) if r.rows_affected() == 0 => Ok(non_trouve("salon introuvable")),
        Ok(_) => {
            evenement_serveur(
                &state,
                pool,
                &sv.id,
                "channel.updated",
                &json!({ "server_id": sv.id, "id": cid, "name": nom }),
            )
            .await;
            Ok(sans_contenu())
        }
    }
}

async fn ordonner(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let e = decode(body).await?;
    let kind = champ_texte(&e, "kind").ok_or_else(mauvais)?;
    let ids_nul = matches!(champ(&e, "ids"), None | Some(Value::Null));
    let ids = liste(&e, "ids").ok_or_else(mauvais)?;
    let invalide = |message: &str| write_err(StatusCode::BAD_REQUEST, "invalid_order", message);
    if !TYPES.contains(&kind.as_str()) || ids.len() as i64 > MAX_SALONS {
        return Ok(invalide(
            "type : text ou voice ; liste des salons de ce type",
        ));
    }
    if ids.iter().any(|id| !uuid_valide(id)) {
        return Ok(invalide("identifiant de salon invalide"));
    }
    match reordonner(pool, &sv.id, &kind, &ids).await {
        Err(Echec::NonTrouve) => Ok(invalide(
            "la liste doit contenir exactement les salons de ce type",
        )),
        Err(e) => Err(echec_interne(e)),
        Ok(()) => {
            let ids = if ids_nul { Value::Null } else { json!(ids) };
            evenement_serveur(
                &state,
                pool,
                &sv.id,
                "channel.reordered",
                &json!({ "server_id": sv.id, "kind": kind, "ids": ids }),
            )
            .await;
            Ok(sans_contenu())
        }
    }
}

async fn reordonner(
    pool: &PgPool,
    server_id: &str,
    kind: &str,
    ids: &[String],
) -> Result<(), Echec> {
    let mut tx = pool.begin().await?;
    let mut presents: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind=$2 FOR UPDATE",
    )
    .bind(server_id)
    .bind(kind)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    if ids.len() != presents.len() {
        return Err(Echec::NonTrouve);
    }
    for (i, id) in ids.iter().enumerate() {
        if !presents.remove(id) {
            return Err(Echec::NonTrouve);
        }
        sqlx::query("UPDATE channels SET position=$2 WHERE id=$1::uuid")
            .bind(id)
            .bind(i as i32 + 1)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn supprimer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p)?;
    let r = sqlx::query("DELETE FROM channels WHERE id=$2::uuid AND server_id=$1::uuid")
        .bind(&sv.id)
        .bind(&cid)
        .execute(pool)
        .await
        .map_err(bd)?;
    if r.rows_affected() == 0 {
        return Ok(non_trouve("salon introuvable"));
    }
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "channel.deleted",
        &json!({ "server_id": sv.id, "id": cid }),
    )
    .await;
    Ok(sans_contenu())
}

async fn lister_invitations(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::CREATE_INVITES).await?;
    let l: Vec<Value> = sqlx::query_as::<_, (String, String, String, i32, Option<i32>, Option<String>)>(
        sqlx::AssertSqlSafe(format!(
            "SELECT i.code, i.created_at::text, u.pseudo, i.uses, i.max_uses, {} FROM server_invites i JOIN users u ON u.id = i.created_by
             WHERE i.server_id=$1::uuid AND i.revoked_at IS NULL AND (i.expires_at IS NULL OR i.expires_at > now()) AND (i.max_uses IS NULL OR i.uses < i.max_uses)
             ORDER BY i.created_at DESC",
            horodatage("i.expires_at")
        )),
    )
    .bind(&sv.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?
    .into_iter()
    .map(|(code, created_at, created_by, uses, max_uses, expires_at)| {
        invitation(&code, &created_at, &created_by, uses, max_uses, expires_at)
    })
    .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

fn invitation(
    code: &str,
    created_at: &str,
    created_by: &str,
    uses: i32,
    max_uses: Option<i32>,
    expires_at: Option<String>,
) -> Value {
    json!({
        "code": code,
        "created_at": created_at,
        "created_by": created_by,
        "uses": uses,
        "max_uses": max_uses,
        "expires_at": expires_at.map(|t| rfc3339(&t)),
    })
}

async fn creer_invitation(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::CREATE_INVITES).await?;
    let octets = to_bytes(body, CORPS_MAX).await.map_err(|_| mauvais())?;
    let e = if octets.is_empty() {
        Map::new()
    } else {
        premier_objet(&octets).ok_or_else(mauvais)?
    };
    let max_uses = entier(&e, "max_uses").ok_or_else(mauvais)?;
    let expires_in = entier(&e, "expires_in").ok_or_else(mauvais)?;
    if !(0..=1000).contains(&max_uses) || !(0..=365 * 86400).contains(&expires_in) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_invite",
            "utilisations : 1 à 1000 ; durée : un an au plus",
        ));
    }
    let code = nouveau_code();
    let max = (max_uses > 0).then_some(max_uses as i32);
    let (created_at, expires_at) = sqlx::query_as::<_, (String, Option<String>)>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO server_invites (code, server_id, created_by, max_uses, expires_at)
         VALUES ($1,$2::uuid,$3::uuid,$4, CASE WHEN $5::bigint > 0 THEN now() + $5::bigint * interval '1 second' END)
         RETURNING created_at::text, {}",
        horodatage("expires_at")
    )))
    .bind(&code)
    .bind(&sv.id)
    .bind(&u.id)
    .bind(max)
    .bind(expires_in)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    Ok(write_json(
        StatusCode::CREATED,
        &invitation(&code, &created_at, &u.pseudo, 0, max, expires_at),
    ))
}

async fn revoquer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::CREATE_INVITES).await?;
    let r = sqlx::query(
        "UPDATE server_invites SET revoked_at=now() WHERE server_id=$1::uuid AND code=$2 AND revoked_at IS NULL",
    )
    .bind(&sv.id)
    .bind(param(&p, "code").to_uppercase())
    .execute(pool)
    .await
    .map_err(bd)?;
    if r.rows_affected() == 0 {
        return Ok(non_trouve("invitation introuvable"));
    }
    Ok(sans_contenu())
}

async fn rejoindre(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let code = param(&p, "code").trim().to_uppercase();
    match rejoindre_base(pool, &u.id, &code).await {
        Err(Echec::NonTrouve) => Ok(write_err(
            StatusCode::NOT_FOUND,
            "invalid_code",
            "invitation invalide, révoquée, expirée ou épuisée",
        )),
        Err(Echec::Banni) => Ok(write_err(
            StatusCode::FORBIDDEN,
            "banned",
            "vous êtes banni de ce serveur",
        )),
        Err(e) => Err(echec_interne(e)),
        Ok((sv, nouveau)) => {
            if nouveau {
                evenement_serveur(
                    &state,
                    pool,
                    &sv.id,
                    "member.joined",
                    &json!({
                        "server_id": sv.id,
                        "member": {
                            "id": u.id,
                            "pseudo": u.pseudo,
                            "online": state.hub().online(&u.id),
                        },
                    }),
                )
                .await;
            }
            Ok(write_json(StatusCode::OK, &sv.json()))
        }
    }
}

async fn rejoindre_base(pool: &PgPool, user: &str, code: &str) -> Result<(Serveur, bool), Echec> {
    let mut tx = pool.begin().await?;
    let sv: Serveur = sqlx::query_as::<_, LigneServeur>(
        "SELECT s.id::text, s.name, s.owner_id::text, s.icon_version FROM server_invites i JOIN servers s ON s.id = i.server_id
         WHERE i.code=$1 AND i.revoked_at IS NULL AND (i.expires_at IS NULL OR i.expires_at > now()) FOR UPDATE OF i",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Echec::NonTrouve)?
    .into();
    let banni: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM server_bans WHERE server_id=$1::uuid AND user_id=$2::uuid AND (expires_at IS NULL OR expires_at > now()))",
    )
    .bind(&sv.id)
    .bind(user)
    .fetch_one(&mut *tx)
    .await?;
    if banni {
        return Err(Echec::Banni);
    }
    let membre: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid)",
    )
    .bind(&sv.id)
    .bind(user)
    .fetch_one(&mut *tx)
    .await?;
    if membre {
        tx.commit().await?;
        return Ok((sv, false));
    }
    let utilise = sqlx::query(
        "UPDATE server_invites SET uses = uses + 1 WHERE code=$1 AND (max_uses IS NULL OR uses < max_uses)",
    )
    .bind(code)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if utilise == 0 {
        return Err(Echec::NonTrouve);
    }
    sqlx::query("DELETE FROM server_bans WHERE server_id=$1::uuid AND user_id=$2::uuid")
        .bind(&sv.id)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)")
        .bind(&sv.id)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((sv, true))
}

#[cfg(test)]
pub(crate) mod tests {
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
        env_avec_blocs(autorisation, vec![]).await
    }

    pub(crate) async fn env_avec_blocs(
        autorisation: impl Fn(&testutil::Requete) -> (u16, Vec<u8>) + Send + Sync + 'static,
        blocs: Vec<String>,
    ) -> Option<Env> {
        let g = faux(gateway).await;
        let p = faux(autorisation).await;
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", g.url.as_str()),
                ("PERMISSION_URL", p.url.as_str()),
            ]),
            blocs,
        );
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
        let (s, v) = appel(&e, "alice", "PATCH", &ordre, Some(r#"{"name":"x"}"#)).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::NOT_FOUND, Some("not_found"))
        );

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
    fn cinema_avec_entracte_ou_les_films() {
        use crate::testutil::{config, config_avec};
        let blocs = |b: &[&str]| b.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let entracte = || {
            config_avec(&[
                ("ESTOMPE_ENTRACTE_URL", "http://entracte:8080/"),
                ("ESTOMPE_ENTRACTE_TOKEN", "t"),
            ])
        };
        assert!(!cinema_disponible(&AppState::new(
            config(),
            blocs(&["regarder-ensemble"])
        )));
        assert!(cinema_disponible(&AppState::new(
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
    async fn salon_cinema_seulement_avec_la_bibliotheque_de_films() {
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

        let Some(e) = env_avec_blocs(permission, vec!["films".into()]).await else {
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

        let corps =
            json!({"name":"secret","kind":"text","private":true,"members":[bob]}).to_string();
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
        sqlx::query(
            "UPDATE server_invites SET expires_at = now() - interval '1 second' WHERE code=$1",
        )
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
}
