use std::collections::{HashMap, HashSet};

use axum::{
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::{get, post, put},
};
use regex::{Regex, RegexSet};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use std::sync::OnceLock;
use std::time::Duration;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    audit, emoji,
    gestion_roles::premier_parametre,
    http::{decode, write_err, write_json},
    openapi,
    roles::{self, Perm},
    servers::{
        Params, bd, horodatage, mauvais, non_trouve, param, pool, rfc3339, sans_contenu,
        uuid_valide,
    },
    sondages,
    state::AppState,
    users::User,
};

pub const SYSTEME: &str = "00000000-0000-0000-0000-000000000001";
const LONGUEUR_MAX: usize = 4000;
const PIECES_JOINTES_MAX: usize = 10;
const REACTIONS_MAX: i64 = 20;
pub(crate) const REACTIONS_PAR_FENETRE: usize = 30;
const FENETRE_REACTIONS: std::time::Duration = std::time::Duration::from_secs(10);
pub(crate) const DMS_PAR_FENETRE: usize = 10;
const FENETRE_DMS: std::time::Duration = std::time::Duration::from_secs(60);
const ENVOIS: usize = 20;
const FENETRE_ENVOI: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Debug, Default)]
pub struct Conv {
    pub channel_id: Option<String>,
    pub server_id: Option<String>,
    pub dm_id: Option<String>,
    pub kind: String,
    pub slowmode: i32,
    pub user_limit: i32,
    pub perms: Perm,
}

impl Conv {
    pub fn peut(&self, p: Perm) -> bool {
        self.channel_id.is_none() || roles::contient(self.perms, p)
    }
}

#[derive(Clone, Debug, Default)]
pub struct EvenementMessage {
    pub conv: Conv,
    pub message: Value,
    pub mentionnes: Vec<String>,
    pub groupe: Vec<String>,
    pub a: Vec<String>,
    pub vocal: String,
    pub rappel: String,
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route(
            "/channels/{cid}/messages",
            get(lister_salon).post(envoyer_salon),
        )
        .route("/dms", get(lister_dms).post(ouvrir_dm))
        .route("/dms/{id}/messages", get(lister_dm).post(envoyer_dm))
        .route(
            "/messages/{id}",
            axum::routing::patch(modifier).delete(supprimer),
        )
        .route(
            "/messages/{id}/reactions/{emoji}",
            put(reagir).delete(retirer_reaction),
        )
        .route("/messages/{id}/pin", put(epingler).delete(desepingler))
        .route("/channels/{cid}/pins", get(epingles_salon))
        .route("/dms/{id}/pins", get(epingles_dm))
        .route("/channels/{cid}/typing", post(saisie_salon))
        .route("/dms/{id}/typing", post(saisie_dm));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("messages", resume, reponses, chemin);
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
        HttpMethod::Get,
        "/channels/{cid}/messages",
        "Historique d'un salon texte, du plus ancien au plus récent, par pages",
        &[
            ("200", "ok — {messages, has_more}"),
            ("400", "`not_text_channel` ou paramètre invalide"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/messages",
        "Envoyer un message",
        &[
            ("201", "Créé"),
            ("400", "`invalid_message` ou `invalid_files`"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "`too_many_messages` : 20 messages / 10 s"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms",
        "Conversations privées, la plus récemment active d'abord",
        &[("200", "ok")],
    ),
    (
        HttpMethod::Post,
        "/dms",
        "Ouvrir (ou retrouver) la conversation privée avec un utilisateur",
        &[
            ("200", "ok"),
            ("400", "self_dm"),
            ("404", "user_not_found"),
            ("429", "`rate_limited` : 10 / min"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms/{id}/messages",
        "Historique d'une conversation privée",
        &[
            ("200", "ok — {messages, has_more}"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/dms/{id}/messages",
        "Envoyer un message privé",
        &[
            ("201", "Créé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "too_many_messages"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/messages/{id}",
        "Modifier son message",
        &[
            ("200", "ok"),
            ("400", "Requête invalide"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}",
        "Supprimer son message",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Put,
        "/messages/{id}/reactions/{emoji}",
        "Ajouter sa réaction (idempotent)",
        &[
            ("204", "ok"),
            ("400", "invalid_emoji"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            (
                "409",
                "too_many_reactions : 20 emojis différents par message",
            ),
            ("429", "`rate_limited` : 30 / 10 s"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}/reactions/{emoji}",
        "Retirer sa réaction",
        &[
            ("204", "ok"),
            ("400", "invalid_emoji"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "`rate_limited` : 30 / 10 s"),
        ],
    ),
    (
        HttpMethod::Put,
        "/messages/{id}/pin",
        "Épingler un message (l'auteur, ou « Gérer les messages »)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}/pin",
        "Désépingler un message (l'auteur, ou « Gérer les messages »)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/channels/{cid}/pins",
        "Messages épinglés d'un salon, le plus récemment épinglé d'abord (100 max)",
        &[
            ("200", "ok — {messages}"),
            ("400", "`not_text_channel`"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms/{id}/pins",
        "Messages épinglés d'une conversation privée (100 max)",
        &[
            ("200", "ok — {messages}"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/typing",
        "Signaler que l'on écrit dans un salon",
        &[
            ("204", "ok"),
            ("400", "`not_text_channel`"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/dms/{id}/typing",
        "Signaler que l'on écrit dans une conversation privée",
        &[
            ("204", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
];

fn introuvable() -> Response {
    non_trouve("introuvable")
}

async fn avec_droits(state: &AppState, conv: &mut Conv, user: &str) -> Result<(), Response> {
    let (Some(salon), Some(serveur)) = (conv.channel_id.clone(), conv.server_id.clone()) else {
        return Ok(());
    };
    let acces = state
        .acces_serveur(&serveur, user)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    conv.perms = acces.channel(&salon);
    if roles::contient(conv.perms, roles::VIEW_CHANNEL) {
        Ok(())
    } else {
        Err(introuvable())
    }
}

pub async fn conv_du_salon(state: &AppState, cid: &str, user: &str) -> Result<Conv, Response> {
    let conv = conv_de_tout_salon(state, cid, user).await?;
    if !["text", "videos", "music"].contains(&conv.kind.as_str()) {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "not_text_channel",
            "ce salon n'est pas un salon texte",
        ));
    }
    Ok(conv)
}

pub async fn conv_de_tout_salon(state: &AppState, cid: &str, user: &str) -> Result<Conv, Response> {
    let pool = state
        .pool()
        .map_err(axum::response::IntoResponse::into_response)?;
    if !uuid_valide(cid) {
        return Err(introuvable());
    }
    let (serveur, kind, slowmode, user_limit) = sqlx::query_as::<_, (String, String, i32, i32)>(
        "SELECT c.server_id::text, c.kind, c.slowmode, c.user_limit FROM channels c
         JOIN server_members m ON m.server_id = c.server_id WHERE c.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(cid)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let mut conv = Conv {
        channel_id: Some(cid.to_string()),
        server_id: Some(serveur),
        dm_id: None,
        kind,
        slowmode,
        user_limit,
        perms: 0,
    };
    avec_droits(state, &mut conv, user).await?;
    Ok(conv)
}

pub async fn conv_du_dm(pool: &PgPool, id: &str, user: &str) -> Result<Conv, Response> {
    if !uuid_valide(id) {
        return Err(introuvable());
    }
    let id: String = sqlx::query_scalar(
        "SELECT id::text FROM dm_conversations WHERE id=$1::uuid AND (user_a=$2::uuid OR user_b=$2::uuid)",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    Ok(Conv {
        channel_id: None,
        server_id: None,
        dm_id: Some(id),
        kind: String::new(),
        slowmode: 0,
        user_limit: 0,
        perms: 0,
    })
}

pub fn id_message(texte: &str) -> Result<i64, Response> {
    texte.parse::<i64>().map_err(|_| introuvable())
}

pub async fn conv_du_message(
    state: &AppState,
    id: i64,
    user: &str,
) -> Result<(Conv, String), Response> {
    let pool = state
        .pool()
        .map_err(axum::response::IntoResponse::into_response)?;
    let (salon, serveur, dm, kind, slowmode, auteur) = sqlx::query_as::<
        _,
        (Option<String>, Option<String>, Option<String>, String, i32, String),
    >(
        "SELECT m.channel_id::text, ch.server_id::text, m.dm_id::text, COALESCE(ch.kind,''), COALESCE(ch.slowmode,0), m.author_id::text
         FROM messages m
         LEFT JOIN channels ch ON ch.id = m.channel_id
         WHERE m.id=$1 AND (
             (m.channel_id IS NOT NULL AND EXISTS (SELECT 1 FROM server_members sm WHERE sm.server_id = ch.server_id AND sm.user_id=$2::uuid))
             OR (m.dm_id IS NOT NULL AND EXISTS (SELECT 1 FROM dm_conversations d WHERE d.id = m.dm_id AND (d.user_a=$2::uuid OR d.user_b=$2::uuid))))",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let mut conv = Conv {
        channel_id: salon,
        server_id: serveur,
        dm_id: dm,
        kind,
        slowmode,
        user_limit: 0,
        perms: 0,
    };
    avec_droits(state, &mut conv, user).await?;
    Ok((conv, auteur))
}

pub async fn participants(pool: &PgPool, conv: &Conv) -> Result<Vec<String>, sqlx::Error> {
    if let (Some(salon), Some(serveur)) = (&conv.channel_id, &conv.server_id) {
        return roles::voyeurs_du_salon(pool, serveur, salon).await;
    }
    let Some(dm) = &conv.dm_id else {
        return Ok(Vec::new());
    };
    sqlx::query_scalar(
        "SELECT user_a::text FROM dm_conversations WHERE id=$1::uuid UNION SELECT user_b::text FROM dm_conversations WHERE id=$1::uuid",
    )
    .bind(dm)
    .fetch_all(pool)
    .await
}

pub async fn publier(state: &AppState, pool: &PgPool, conv: &Conv, typ: &str, data: &Value) {
    if let Ok(to) = participants(pool, conv).await
        && !to.is_empty()
    {
        state.publier(&to, typ, data);
    }
}

pub(crate) struct Ligne {
    id: String,
    channel_id: Option<String>,
    dm_id: Option<String>,
    auteur_id: String,
    pseudo: String,
    contenu: String,
    cree: String,
    edite: Option<String>,
    epingle: Option<String>,
    systeme: bool,
    reponse_id: Option<String>,
    reponse_auteur: Option<String>,
    reponse_pseudo: Option<String>,
    reponse_contenu: Option<String>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for Ligne {
    fn from_row(r: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            id: r.try_get("id")?,
            channel_id: r.try_get("channel_id")?,
            dm_id: r.try_get("dm_id")?,
            auteur_id: r.try_get("auteur_id")?,
            pseudo: r.try_get("pseudo")?,
            contenu: r.try_get("contenu")?,
            cree: r.try_get("cree")?,
            edite: r.try_get("edite")?,
            epingle: r.try_get("epingle")?,
            systeme: r.try_get("systeme")?,
            reponse_id: r.try_get("reponse_id")?,
            reponse_auteur: r.try_get("reponse_auteur")?,
            reponse_pseudo: r.try_get("reponse_pseudo")?,
            reponse_contenu: r.try_get("reponse_contenu")?,
        })
    }
}

pub(crate) fn selection() -> String {
    format!(
        "SELECT m.id::text AS id, m.channel_id::text AS channel_id, m.dm_id::text AS dm_id, u.id::text AS auteur_id, u.pseudo, m.content AS contenu, {} AS cree, {} AS edite, {} AS epingle, m.system AS systeme,
         r.id::text AS reponse_id, ru.id::text AS reponse_auteur, ru.pseudo AS reponse_pseudo, left(r.content, 120) AS reponse_contenu
         FROM messages m JOIN users u ON u.id = m.author_id
         LEFT JOIN messages r ON r.id = m.reply_to LEFT JOIN users ru ON ru.id = r.author_id",
        horodatage("m.created_at"),
        horodatage("m.edited_at"),
        horodatage("m.pinned_at"),
    )
}

fn date(v: Option<String>) -> Value {
    v.map_or(Value::Null, |s| Value::String(rfc3339(&s)))
}

pub(crate) fn construire(lignes: Vec<Ligne>) -> Vec<Map<String, Value>> {
    lignes
        .into_iter()
        .map(|l| {
            let mut m = Map::new();
            m.insert("id".into(), l.id.into());
            if let Some(c) = l.channel_id {
                m.insert("channel_id".into(), c.into());
            }
            if let Some(d) = l.dm_id {
                m.insert("dm_id".into(), d.into());
            }
            m.insert("author".into(), json!({ "id": l.auteur_id, "pseudo": l.pseudo }));
            m.insert("content".into(), l.contenu.into());
            m.insert("created_at".into(), Value::String(rfc3339(&l.cree)));
            m.insert("edited_at".into(), date(l.edite));
            m.insert("attachments".into(), json!([]));
            m.insert("reactions".into(), json!([]));
            m.insert("mentions".into(), json!([]));
            m.insert(
                "reply_to".into(),
                match (l.reponse_id, l.reponse_auteur, l.reponse_pseudo, l.reponse_contenu) {
                    (Some(id), Some(auteur), Some(pseudo), Some(contenu)) => {
                        json!({ "id": id, "author": { "id": auteur, "pseudo": pseudo }, "content": contenu })
                    }
                    _ => Value::Null,
                },
            );
            m.insert("pinned_at".into(), date(l.epingle));
            m.insert("poll".into(), Value::Null);
            m.insert("system".into(), l.systeme.into());
            m
        })
        .collect()
}

fn liste_mut<'a>(
    index: &HashMap<String, usize>,
    messages: &'a mut [Map<String, Value>],
    id: &str,
    cle: &str,
) -> Option<&'a mut Vec<Value>> {
    let i = *index.get(id)?;
    messages[i].get_mut(cle)?.as_array_mut()
}

pub(crate) async fn hydrater(
    pool: &PgPool,
    messages: &mut [Map<String, Value>],
    viewer: &str,
) -> Result<(), sqlx::Error> {
    if messages.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = messages
        .iter()
        .filter_map(|m| m["id"].as_str().and_then(|s| s.parse().ok()))
        .collect();
    let index: HashMap<String, usize> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| (m["id"].as_str().unwrap_or_default().to_string(), i))
        .collect();
    let reactions = sqlx::query_as::<_, (String, String, i64, bool, String)>(
        "SELECT message_id::text, emoji, count(*), bool_or(user_id=$2::uuid),
                json_agg(json_build_object('id', user_id::text, 'pseudo', u.pseudo) ORDER BY rx.created_at)::text
         FROM reactions rx JOIN users u ON u.id = rx.user_id WHERE message_id = ANY($1::bigint[])
         GROUP BY message_id, emoji ORDER BY message_id, min(rx.created_at)",
    )
    .bind(&ids)
    .bind(viewer)
    .fetch_all(pool)
    .await?;
    for (mid, emoji, nombre, moi, users) in reactions {
        let users: Value = serde_json::from_str(&users).unwrap_or(Value::Array(Vec::new()));
        if let Some(l) = liste_mut(&index, messages, &mid, "reactions") {
            l.push(json!({ "emoji": emoji, "count": nombre, "me": moi, "users": users }));
        }
    }
    let mentions = sqlx::query_as::<_, (String, String)>(
        "SELECT message_id::text, user_id::text FROM message_mentions WHERE message_id = ANY($1::bigint[])",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    for (mid, uid) in mentions {
        if let Some(l) = liste_mut(&index, messages, &mid, "mentions") {
            l.push(uid.into());
        }
    }
    sondages::hydrater(pool, messages, &index, &ids, viewer).await?;
    let fichiers = sqlx::query_as::<_, (String, String, String, String, i64, bool, String, i32, Vec<i32>, bool, String, bool)>(
        "SELECT message_id::text, id::text, name, mime, size, purged_at IS NOT NULL, kind, duration_ms, waveform, thumb IS NOT NULL, transcript, transcript_auto
         FROM files WHERE message_id = ANY($1::bigint[]) ORDER BY position, created_at, id",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    for (mid, id, nom, mime, taille, expire, kind, duree, onde, vignette, transcript, auto) in
        fichiers
    {
        let mut a = Map::new();
        a.insert("id".into(), id.into());
        a.insert("name".into(), nom.into());
        a.insert("mime".into(), mime.into());
        a.insert("size".into(), taille.into());
        a.insert("expired".into(), expire.into());
        a.insert("kind".into(), kind.into());
        if duree != 0 {
            a.insert("duration_ms".into(), duree.into());
        }
        if !onde.is_empty() {
            a.insert("waveform".into(), onde.into());
        }
        if vignette {
            a.insert("thumb".into(), true.into());
        }
        if !transcript.is_empty() {
            a.insert("transcript".into(), transcript.into());
        }
        if auto {
            a.insert("transcript_auto".into(), true.into());
        }
        if let Some(l) = liste_mut(&index, messages, &mid, "attachments") {
            l.push(Value::Object(a));
        }
    }
    Ok(())
}

pub async fn message_complet(pool: &PgPool, id: i64, viewer: &str) -> Result<Value, sqlx::Error> {
    let lignes = sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
        "{} WHERE m.id=$1",
        selection()
    )))
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut messages = construire(lignes);
    if messages.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    hydrater(pool, &mut messages, viewer).await?;
    Ok(Value::Object(messages.remove(0)))
}

async fn page(
    pool: &PgPool,
    conv: &Conv,
    viewer: &str,
    avant: i64,
    apres: Option<i64>,
    limite: i64,
) -> Result<(Vec<Value>, bool), sqlx::Error> {
    let (colonne, cible) = match &conv.dm_id {
        Some(dm) => ("m.dm_id", dm.clone()),
        None => ("m.channel_id", conv.channel_id.clone().unwrap_or_default()),
    };
    let lignes = match apres {
        Some(apres) => {
            sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
                "{} WHERE {colonne}=$1::uuid AND m.id > $2 ORDER BY m.id LIMIT $3",
                selection()
            )))
            .bind(&cible)
            .bind(apres)
            .bind(limite + 1)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
                "{} WHERE {colonne}=$1::uuid AND ($2::bigint = 0 OR m.id < $2) ORDER BY m.id DESC LIMIT $3",
                selection()
            )))
            .bind(&cible)
            .bind(avant)
            .bind(limite + 1)
            .fetch_all(pool)
            .await?
        }
    };
    let mut messages = construire(lignes);
    let plus = messages.len() as i64 > limite;
    if plus {
        messages.truncate(limite as usize);
    }
    if apres.is_none() {
        messages.reverse();
    }
    hydrater(pool, &mut messages, viewer).await?;
    Ok((messages.into_iter().map(Value::Object).collect(), plus))
}

pub(crate) fn entier_requete(
    requete: &str,
    nom: &str,
    erreur: &str,
    valide: impl Fn(i64) -> bool,
) -> Result<Option<i64>, Response> {
    let brut = premier_parametre(requete, nom);
    if brut.is_empty() {
        return Ok(None);
    }
    match brut.parse::<i64>() {
        Ok(n) if valide(n) => Ok(Some(n)),
        _ => Err(write_err(StatusCode::BAD_REQUEST, "bad_request", erreur)),
    }
}

async fn lister(
    state: &AppState,
    u: &User,
    conv: Conv,
    requete: Option<String>,
) -> Result<Response, Response> {
    let pool = pool(state)?;
    let requete = requete.unwrap_or_default();
    let avant = entier_requete(&requete, "before", "before invalide", |n| n >= 0)?.unwrap_or(0);
    let limite = entier_requete(&requete, "limit", "limit : 1 à 100", |n| {
        (1..=100).contains(&n)
    })?
    .unwrap_or(50);
    let apres = entier_requete(&requete, "after", "after invalide", |n| n >= 0)?;
    let (messages, plus) = page(pool, &conv, &u.id, avant, apres, limite)
        .await
        .map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &json!({ "messages": messages, "has_more": plus }),
    ))
}

async fn lister_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    lister(&state, &u, conv, requete).await
}

async fn lister_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    lister(&state, &u, conv, requete).await
}

fn re_mention() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:^|[^A-Za-z0-9_.-])@([A-Za-z0-9_.-]{3,20})").expect("motif invalide")
    })
}

pub fn pseudos_cites(contenu: &str) -> Vec<String> {
    let mut vus = HashSet::new();
    let mut sortie = Vec::new();
    for c in re_mention().captures_iter(contenu) {
        let p = c[1].trim_end_matches(['.', '-']).to_ascii_lowercase();
        if p.len() >= 3 && vus.insert(p.clone()) {
            sortie.push(p);
        }
    }
    sortie
}

fn citees(contenu: &str, noms: &[&str]) -> Vec<usize> {
    let motifs = noms.iter().map(|nom| {
        format!(
            r"(?i)(?:^|[^A-Za-z0-9_.-])@{}(?:$|[^\p{{L}}\p{{N}}_])",
            regex::escape(nom)
        )
    });
    RegexSet::new(motifs).map_or_else(
        |e| {
            tracing::warn!(erreur = %e, "motifs de mention invalides");
            Vec::new()
        },
        |set| set.matches(contenu).into_iter().collect(),
    )
}

async fn voyeurs_des_mentions(
    pool: &PgPool,
    conv: &Conv,
    contenu: &str,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    match (&conv.channel_id, &conv.server_id) {
        (Some(salon), Some(serveur)) if contenu.contains('@') => {
            roles::voyeurs_du_salon(pool, serveur, salon)
                .await
                .map(Some)
        }
        _ => Ok(None),
    }
}

fn re_code() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)```.*?(?:```|$)|`[^`\n]*`").expect("motif invalide"))
}

fn sans_code(contenu: &str) -> std::borrow::Cow<'_, str> {
    re_code().replace_all(contenu, " ")
}

fn groupes_cites(contenu: &str) -> (bool, bool) {
    if !contenu.contains('@') {
        return (false, false);
    }
    let c = citees(contenu, &["everyone", "here"]);
    (c.contains(&0), c.contains(&1))
}

const DELAI_GROUPE_SECONDES: f64 = 120.0;

async fn debit_groupe_depasse(
    pool: &PgPool,
    conv: &Conv,
    auteur: &str,
    contenu: &str,
    exclu: i64,
) -> Result<bool, sqlx::Error> {
    let Some(salon) = &conv.channel_id else {
        return Ok(false);
    };
    let (tous, ici) = groupes_cites(&sans_code(contenu));
    if !(tous || ici)
        || !roles::contient(conv.perms, roles::MENTION_EVERYONE)
        || roles::contient(conv.perms, roles::MANAGE_MESSAGES)
    {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM messages WHERE channel_id=$1::uuid AND author_id=$2::uuid AND id<>$3
         AND created_at > now() - make_interval(secs => $4) AND content ~* '@(everyone|here)')",
    )
    .bind(salon)
    .bind(auteur)
    .bind(exclu)
    .bind(DELAI_GROUPE_SECONDES)
    .fetch_one(pool)
    .await
}

async fn cibles_des_mentions(
    tx: &mut PgConnection,
    hub: &crate::hub::Hub,
    voyeurs: Option<&[String]>,
    conv: &Conv,
    auteur: &str,
    contenu: &str,
) -> Result<Vec<(String, bool)>, sqlx::Error> {
    let contenu = &*sans_code(contenu);
    let mut ids: Vec<String> = Vec::new();
    let mut groupe: Vec<String> = Vec::new();
    let noms = pseudos_cites(contenu);
    if !noms.is_empty() {
        let trouves: Vec<String> = if let Some(serveur) = &conv.server_id {
            sqlx::query_scalar(
                "SELECT u.id::text FROM users u JOIN server_members m ON m.user_id = u.id
                 WHERE m.server_id=$1::uuid AND lower(u.pseudo) = ANY($2) AND u.id <> $3::uuid",
            )
            .bind(serveur)
            .bind(&noms)
            .bind(auteur)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT u.id::text FROM users u JOIN dm_conversations d ON u.id IN (d.user_a, d.user_b)
                 WHERE d.id=$1::uuid AND lower(u.pseudo) = ANY($2) AND u.id <> $3::uuid",
            )
            .bind(conv.dm_id.clone().unwrap_or_default())
            .bind(&noms)
            .bind(auteur)
            .fetch_all(&mut *tx)
            .await?
        };
        ids.extend(trouves);
    }
    if let (Some(_), Some(serveur)) = (&conv.channel_id, &conv.server_id)
        && roles::contient(conv.perms, roles::MENTION_EVERYONE)
        && contenu.contains('@')
    {
        let (tous, ici) = groupes_cites(contenu);
        if tous || ici {
            let membres: Vec<String> = sqlx::query_scalar(
                "SELECT user_id::text FROM server_members WHERE server_id=$1::uuid AND user_id <> $2::uuid",
            )
            .bind(serveur)
            .bind(auteur)
            .fetch_all(&mut *tx)
            .await?;
            groupe = if tous {
                membres
            } else {
                let vus: HashSet<&String> = voyeurs.iter().flat_map(|v| v.iter()).collect();
                let candidats: Vec<String> = membres
                    .into_iter()
                    .filter(|m| voyeurs.is_none() || vus.contains(m))
                    .collect();
                hub.online_among(&candidats)
            };
        }
        if !tous {
            let roles_du_serveur: Vec<(String, String)> = sqlx::query_as(
                "SELECT id::text, name FROM roles WHERE server_id=$1::uuid AND NOT is_default",
            )
            .bind(serveur)
            .fetch_all(&mut *tx)
            .await?;
            let noms: Vec<&str> = roles_du_serveur.iter().map(|(_, n)| n.as_str()).collect();
            let cites: Vec<String> = citees(contenu, &noms)
                .into_iter()
                .map(|i| roles_du_serveur[i].0.clone())
                .collect();
            if !cites.is_empty() {
                ids.extend(
                    sqlx::query_scalar::<_, String>(
                        "SELECT user_id::text FROM member_roles WHERE server_id=$1::uuid AND role_id::text = ANY($2) AND user_id <> $3::uuid",
                    )
                    .bind(serveur)
                    .bind(&cites)
                    .bind(auteur)
                    .fetch_all(&mut *tx)
                    .await?,
                );
            }
        }
    }
    let nominatifs: HashSet<String> = ids.iter().cloned().collect();
    let mut cibles: Vec<(String, bool)> = ids
        .into_iter()
        .map(|i| (i, false))
        .chain(groupe.into_iter().map(|i| {
            let seul = !nominatifs.contains(&i);
            (i, seul)
        }))
        .collect();
    if let Some(voyeurs) = voyeurs {
        let vus: HashSet<&String> = voyeurs.iter().collect();
        cibles.retain(|(id, _)| vus.contains(id));
    }
    cibles.sort();
    cibles.dedup_by(|b, a| a.0 == b.0);
    Ok(cibles)
}

pub(crate) enum Echec {
    Reponse,
    Fichiers,
    Vocal,
    Interdit,
    Debit,
    Base(sqlx::Error),
}

impl From<sqlx::Error> for Echec {
    fn from(e: sqlx::Error) -> Self {
        Echec::Base(e)
    }
}

pub(crate) async fn attacher_fichiers(
    tx: &mut PgConnection,
    message: i64,
    auteur: &str,
    contenu: &str,
    fichiers: &[String],
) -> Result<(), Echec> {
    if fichiers.is_empty() {
        return Ok(());
    }
    let uniques: HashSet<&String> = fichiers.iter().collect();
    if uniques.len() != fichiers.len() {
        return Err(Echec::Fichiers);
    }
    let kinds: Vec<String> = sqlx::query_scalar(
        "UPDATE files SET message_id=$1, position=array_position($2::uuid[], id) WHERE id = ANY($2::uuid[])
         AND (message_id=$1 OR (uploader_id=$3::uuid AND message_id IS NULL AND report_id IS NULL AND purged_at IS NULL)) RETURNING kind",
    )
    .bind(message)
    .bind(fichiers)
    .bind(auteur)
    .fetch_all(&mut *tx)
    .await?;
    if kinds.len() != fichiers.len() {
        return Err(Echec::Fichiers);
    }
    let voix = kinds.iter().filter(|k| *k == "voice").count();
    if voix > 0 && (voix != 1 || kinds.len() != 1 || !contenu.is_empty()) {
        return Err(Echec::Vocal);
    }
    Ok(())
}

fn echec(e: Echec, fichiers_msg: &str) -> Response {
    match e {
        Echec::Reponse => write_err(
            StatusCode::BAD_REQUEST,
            "invalid_reply",
            "on ne peut répondre qu'à un message de la même conversation",
        ),
        Echec::Fichiers => write_err(StatusCode::BAD_REQUEST, "invalid_files", fichiers_msg),
        Echec::Vocal => write_err(
            StatusCode::BAD_REQUEST,
            "invalid_voice",
            "un message vocal s'envoie seul, sans texte ni autre fichier",
        ),
        Echec::Interdit => write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce message n'est pas modifiable",
        ),
        Echec::Debit => write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "une mention @everyone ou @here par salon toutes les 2 minutes",
        ),
        Echec::Base(e) => bd(e),
    }
}

const FICHIERS_INTROUVABLES: &str = "fichier introuvable, déjà utilisé ou non envoyé par vous";

pub(crate) fn contenu_valide(s: &str, avec_fichiers: bool) -> Option<String> {
    let s = s.trim();
    (s.chars().count() <= LONGUEUR_MAX && (!s.is_empty() || avec_fichiers)).then(|| s.to_string())
}

fn texte_strict(e: &Map<String, Value>, nom: &str) -> Option<String> {
    match e.get(nom) {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

pub(crate) fn liste_stricte(e: &Map<String, Value>, nom: &str) -> Result<Option<Vec<String>>, ()> {
    match e.get(nom) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .map(Some)
            .ok_or(()),
        Some(_) => Err(()),
    }
}

async fn smileys_perso(pool: &PgPool, user: &str) -> HashSet<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT e.name FROM custom_emojis e JOIN server_members m ON m.server_id = e.server_id WHERE m.user_id=$1::uuid",
    )
    .bind(user)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect()
}

async fn convertir_emojis(pool: &PgPool, user: &str, contenu: &str) -> String {
    if !contenu.contains(':') {
        return contenu.to_string();
    }
    emoji::convertir(contenu, &smileys_perso(pool, user).await)
}

pub(crate) fn trop_de_requetes(secondes: u64) -> Response {
    avec_reessai(
        write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "trop de requêtes, patientez avant de réessayer",
        ),
        secondes,
    )
}

pub(crate) fn avec_reessai(mut r: Response, secondes: u64) -> Response {
    if let Ok(v) = HeaderValue::from_str(&secondes.to_string()) {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

async fn envoyer(state: &AppState, u: &User, conv: Conv, body: Body) -> Result<Response, Response> {
    let pool = pool(state)?;
    if !conv.peut(roles::SEND_MESSAGES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas écrire dans ce salon",
        ));
    }
    if conv.slowmode > 0
        && !conv.peut(roles::MANAGE_MESSAGES)
        && let Some(salon) = &conv.channel_id
    {
        let attente: Option<f64> = sqlx::query_scalar(
            "SELECT $3::float8 - extract(epoch FROM now() - max(created_at))::float8 FROM messages WHERE channel_id=$1::uuid AND author_id=$2::uuid HAVING max(created_at) IS NOT NULL",
        )
        .bind(salon)
        .bind(&u.id)
        .bind(f64::from(conv.slowmode))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some(reste) = attente
            && reste > 0.0
        {
            return Ok(avec_reessai(
                write_err(
                    StatusCode::TOO_MANY_REQUESTS,
                    "slowmode",
                    "mode lent : patientez avant d'écrire à nouveau",
                ),
                reste as u64 + 1,
            ));
        }
    }
    if !state.limiteurs().envoi.allow(&u.id, ENVOIS, FENETRE_ENVOI) {
        return Ok(avec_reessai(
            write_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_messages",
                "trop de messages envoyés, patientez quelques secondes",
            ),
            10,
        ));
    }
    let e = decode(body).await?;
    let contenu_brut = texte_strict(&e, "content").ok_or_else(mauvais)?;
    let fichiers = liste_stricte(&e, "file_ids")
        .map_err(|()| mauvais())?
        .unwrap_or_default();
    let reponse = texte_strict(&e, "reply_to").ok_or_else(mauvais)?;
    let sondage = match e.get("poll") {
        None | Some(Value::Null) => None,
        Some(Value::Object(o)) => Some(sondages::lire_entree(o).ok_or_else(mauvais)?),
        Some(_) => return Err(mauvais()),
    };
    let reponse_id = if reponse.is_empty() {
        None
    } else {
        match reponse.parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => {
                return Ok(write_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_reply",
                    "réponse invalide",
                ));
            }
        }
    };
    let Some(contenu) = contenu_valide(&contenu_brut, !fichiers.is_empty()) else {
        return Ok(message_invalide());
    };
    if fichiers.len() > PIECES_JOINTES_MAX {
        return Ok(message_invalide());
    }
    if (!fichiers.is_empty() && !conv.peut(roles::ATTACH_FILES))
        || (sondage.is_some() && !conv.peut(roles::CREATE_POLLS))
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante dans ce salon",
        ));
    }
    let contenu = convertir_emojis(pool, &u.id, &contenu).await;
    if fichiers.iter().any(|f| !uuid_valide(f)) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_files",
            "fichier invalide",
        ));
    }
    let mut auteur = u.id.clone();
    let mut nouveau_sondage = None;
    if let Some(s) = sondage {
        let Some(options) = sondages::options_valides(&s.options) else {
            return Ok(sondages::sondage_invalide());
        };
        if contenu.is_empty() || !fichiers.is_empty() {
            return Ok(sondages::sondage_invalide());
        }
        if let Some(c) = &s.fin
            && !sondages::dans_le_futur(pool, c).await.map_err(bd)?
        {
            return Ok(sondages::sondage_invalide());
        }
        auteur = SYSTEME.to_string();
        nouveau_sondage = Some(sondages::NouveauSondage {
            options,
            multiple: s.multiple,
            fin: s.fin,
            lanceur: u.id.clone(),
        });
    }
    let (id, cites) = match creer(
        pool,
        state.hub(),
        &conv,
        &auteur,
        &contenu,
        &fichiers,
        reponse_id,
        nouveau_sondage,
    )
    .await
    {
        Ok(r) => r,
        Err(x) => return Ok(echec(x, FICHIERS_INTROUVABLES)),
    };
    let message = message_complet(pool, id, &auteur).await.map_err(bd)?;
    publier(state, pool, &conv, "message.created", &message).await;
    if conv.kind == "videos" || conv.kind == "music" {
        state.apres_message_video(&conv, &message).await;
    }
    publier_mentions(state, &cites, &message);
    let groupe = ids_groupe(&cites);
    let mut a_notifier: Vec<String> = cites.into_iter().map(|(c, _)| c).collect();
    if let Some(r) = message["reply_to"]["author"]["id"].as_str()
        && r != u.id
    {
        a_notifier.push(r.to_string());
    }
    state
        .notifier(&EvenementMessage {
            conv: conv.clone(),
            message: message.clone(),
            mentionnes: a_notifier,
            groupe,
            a: Vec::new(),
            ..Default::default()
        })
        .await;
    Ok(write_json(StatusCode::CREATED, &message))
}

fn ids_groupe(cibles: &[(String, bool)]) -> Vec<String> {
    cibles
        .iter()
        .filter(|(_, g)| *g)
        .map(|(c, _)| c.clone())
        .collect()
}

fn publier_mentions(state: &AppState, cibles: &[(String, bool)], message: &Value) {
    let mut groupe = message.clone();
    groupe["group_mention"] = Value::Bool(true);
    for (c, g) in cibles {
        state.publier(
            std::slice::from_ref(c),
            "mention",
            if *g { &groupe } else { message },
        );
    }
}

fn message_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_message",
        "message : 1 à 4000 caractères (vide accepté avec fichier), 10 fichiers maximum",
    )
}

#[allow(clippy::too_many_arguments)]
async fn creer(
    pool: &PgPool,
    hub: &crate::hub::Hub,
    conv: &Conv,
    auteur: &str,
    contenu: &str,
    fichiers: &[String],
    reponse: Option<i64>,
    sondage: Option<sondages::NouveauSondage>,
) -> Result<(i64, Vec<(String, bool)>), Echec> {
    let voyeurs = voyeurs_des_mentions(pool, conv, contenu).await?;
    if debit_groupe_depasse(pool, conv, auteur, contenu, 0).await? {
        return Err(Echec::Debit);
    }
    let mut tx = pool.begin().await?;
    if let Some(r) = reponse {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE id=$1 AND channel_id IS NOT DISTINCT FROM $2::uuid AND dm_id IS NOT DISTINCT FROM $3::uuid)",
        )
        .bind(r)
        .bind(&conv.channel_id)
        .bind(&conv.dm_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| Echec::Reponse)?;
        if !ok {
            return Err(Echec::Reponse);
        }
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO messages (channel_id, dm_id, author_id, content, reply_to) VALUES ($1::uuid,$2::uuid,$3::uuid,$4,$5) RETURNING id",
    )
    .bind(&conv.channel_id)
    .bind(&conv.dm_id)
    .bind(auteur)
    .bind(contenu)
    .bind(reponse)
    .fetch_one(&mut *tx)
    .await?;
    attacher_fichiers(&mut tx, id, auteur, contenu, fichiers).await?;
    if let Some(s) = sondage {
        sondages::inserer(&mut tx, id, &s).await?;
    }
    let cibles =
        cibles_des_mentions(&mut tx, hub, voyeurs.as_deref(), conv, auteur, contenu).await?;
    for (c, groupe) in &cibles {
        sqlx::query(
            "INSERT INTO message_mentions (message_id, user_id, groupe) VALUES ($1,$2::uuid,$3)",
        )
        .bind(id)
        .bind(c)
        .bind(groupe)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((id, cibles))
}

pub(crate) async fn envoyer_systeme(
    state: &AppState,
    pool: &PgPool,
    dm_id: &str,
    contenu: &str,
) -> Result<(), sqlx::Error> {
    let conv = Conv {
        channel_id: None,
        server_id: None,
        dm_id: Some(dm_id.to_string()),
        kind: String::new(),
        slowmode: 0,
        user_limit: 0,
        perms: 0,
    };
    let (id, _) = match creer(pool, state.hub(), &conv, SYSTEME, contenu, &[], None, None).await {
        Ok(r) => r,
        Err(Echec::Base(e)) => return Err(e),
        Err(_) => {
            tracing::error!("message système : échec inattendu");
            return Err(sqlx::Error::RowNotFound);
        }
    };
    let message = message_complet(pool, id, SYSTEME).await?;
    publier(state, pool, &conv, "message.created", &message).await;
    Ok(())
}

async fn envoyer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    envoyer(&state, &u, conv, body).await
}

async fn envoyer_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let conv = conv_du_dm(pool, param(&p.0, "id"), &u.id).await?;
    if let Ok(to) = participants(pool, &conv).await
        && to.iter().any(|t| t == SYSTEME)
        && u.id != SYSTEME
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "read_only",
            "ce compte n'accepte pas de messages",
        ));
    }
    envoyer(&state, &u, conv, body).await
}

async fn modifier(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = id_message(param(&p.0, "id"))?;
    let (conv, auteur) = conv_du_message(&state, id, &u.id).await?;
    if auteur != u.id {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez modifier que vos messages",
        ));
    }
    if !conv.peut(roles::SEND_MESSAGES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas écrire dans ce salon",
        ));
    }
    let e = decode(body).await?;
    let contenu_brut = texte_strict(&e, "content").ok_or_else(mauvais)?;
    let fichiers = liste_stricte(&e, "file_ids").map_err(|()| mauvais())?;
    let avec_fichiers = match &fichiers {
        Some(f) => !f.is_empty(),
        None => {
            sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM files WHERE message_id=$1)")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap_or(false)
        }
    };
    let nombre = fichiers.as_ref().map_or(0, Vec::len);
    let Some(contenu) = contenu_valide(&contenu_brut, avec_fichiers) else {
        return Ok(message_invalide());
    };
    if nombre > PIECES_JOINTES_MAX {
        return Ok(message_invalide());
    }
    if nombre > 0 && !conv.peut(roles::ATTACH_FILES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante dans ce salon",
        ));
    }
    if fichiers.iter().flatten().any(|f| !uuid_valide(f)) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_files",
            "fichier invalide",
        ));
    }
    let contenu = convertir_emojis(pool, &u.id, &contenu).await;
    let ajoutes = match remplacer(
        pool,
        state.hub(),
        &conv,
        id,
        &u.id,
        &contenu,
        fichiers.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(x) => return Ok(echec(x, FICHIERS_INTROUVABLES)),
    };
    let message = message_complet(pool, id, &u.id).await.map_err(bd)?;
    publier(&state, pool, &conv, "message.updated", &message).await;
    publier_mentions(&state, &ajoutes, &message);
    if !ajoutes.is_empty() {
        state
            .notifier(&EvenementMessage {
                conv: conv.clone(),
                message: message.clone(),
                mentionnes: ajoutes.iter().map(|(c, _)| c.clone()).collect(),
                groupe: ids_groupe(&ajoutes),
                a: ajoutes.into_iter().map(|(c, _)| c).collect(),
                ..Default::default()
            })
            .await;
    }
    Ok(write_json(StatusCode::OK, &message))
}

async fn remplacer(
    pool: &PgPool,
    hub: &crate::hub::Hub,
    conv: &Conv,
    id: i64,
    auteur: &str,
    contenu: &str,
    fichiers: Option<&[String]>,
) -> Result<Vec<(String, bool)>, Echec> {
    let voyeurs = voyeurs_des_mentions(pool, conv, contenu).await?;
    if debit_groupe_depasse(pool, conv, auteur, contenu, id).await? {
        return Err(Echec::Debit);
    }
    let mut tx = pool.begin().await?;
    let modifies = sqlx::query(
        "UPDATE messages SET content=$3, edited_at=now() WHERE id=$1 AND author_id=$2::uuid AND NOT system",
    )
    .bind(id)
    .bind(auteur)
    .bind(contenu)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if modifies == 0 {
        return Err(Echec::Interdit);
    }
    if let Some(f) = fichiers {
        sqlx::query(
            "UPDATE files SET message_id=NULL WHERE message_id=$1 AND NOT (id::text = ANY($2))",
        )
        .bind(id)
        .bind(f)
        .execute(&mut *tx)
        .await?;
        attacher_fichiers(&mut tx, id, auteur, contenu, f).await?;
    }
    let cibles =
        cibles_des_mentions(&mut tx, hub, voyeurs.as_deref(), conv, auteur, contenu).await?;
    let ids_cibles: Vec<&String> = cibles.iter().map(|(c, _)| c).collect();
    sqlx::query(
        "DELETE FROM message_mentions WHERE message_id=$1 AND NOT (user_id::text = ANY(COALESCE($2::text[], '{}')))",
    )
    .bind(id)
    .bind(&ids_cibles)
    .execute(&mut *tx)
    .await?;
    let mut ajoutes = Vec::new();
    for (c, groupe) in &cibles {
        let n = sqlx::query(
            "INSERT INTO message_mentions (message_id, user_id, groupe) VALUES ($1,$2::uuid,$3) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(c)
        .bind(groupe)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 1 {
            ajoutes.push((c.clone(), *groupe));
        }
    }
    tx.commit().await?;
    Ok(ajoutes)
}

async fn supprimer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = id_message(param(&p.0, "id"))?;
    let (conv, auteur) = conv_du_message(&state, id, &u.id).await?;
    if auteur != u.id
        && !conv.peut(roles::MANAGE_MESSAGES)
        && !sondages::peut_gerer(&state, pool, id, &u)
            .await
            .map_err(bd)?
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez supprimer que vos messages",
        ));
    }
    let id_brut = param(&p.0, "id").to_string();
    let mut resultats = effacer(pool, id, &auteur).await.map_err(bd)?;
    if auteur != u.id
        && let (Some(serveur), Some(salon)) = (&conv.server_id, &conv.channel_id)
    {
        let pseudo = pseudo_de(pool, &auteur).await;
        let nom = nom_du_salon(pool, salon).await;
        audit::audit_membre(
            pool,
            serveur,
            &u.id,
            "message.delete",
            &auteur,
            &format!("{pseudo} dans {nom}"),
            "",
        )
        .await;
    }
    resultats.push(id_brut);
    for rid in resultats {
        let charge = json!({ "id": rid, "channel_id": conv.channel_id, "dm_id": conv.dm_id });
        publier(&state, pool, &conv, "message.deleted", &charge).await;
    }
    Ok(sans_contenu())
}

async fn effacer(pool: &PgPool, id: i64, auteur: &str) -> Result<Vec<String>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let resultats: Vec<String> = sqlx::query_scalar(
        "DELETE FROM messages r USING messages p
         WHERE r.system AND r.reply_to = p.id AND p.id=$1 AND p.author_id=$2::uuid RETURNING r.id::text",
    )
    .bind(id)
    .bind(auteur)
    .fetch_all(&mut *tx)
    .await?;
    let supprimes = sqlx::query("DELETE FROM messages WHERE id=$1 AND author_id=$2::uuid")
        .bind(id)
        .bind(auteur)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if supprimes == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    tx.commit().await?;
    Ok(resultats)
}

pub(crate) async fn pseudo_de(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT pseudo FROM users WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) async fn nom_du_salon(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT name FROM channels WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

async fn reagir(state: State<AppState>, u: ActiveUser, p: Params) -> Result<Response, Response> {
    reaction(state.0, u.0, p, true).await
}

async fn retirer_reaction(
    state: State<AppState>,
    u: ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    reaction(state.0, u.0, p, false).await
}

async fn reaction(state: AppState, u: User, p: Params, ajout: bool) -> Result<Response, Response> {
    let pool = pool(&state)?;
    if !state
        .limiteurs()
        .reactions
        .allow(&u.id, REACTIONS_PAR_FENETRE, FENETRE_REACTIONS)
    {
        return Ok(trop_de_requetes(FENETRE_REACTIONS.as_secs()));
    }
    let emoji = param(&p.0, "emoji");
    if !emoji::valide(emoji) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_emoji",
            "emoji invalide",
        ));
    }
    let id = id_message(param(&p.0, "id"))?;
    let (conv, _) = conv_du_message(&state, id, &u.id).await?;
    if ajout && !conv.peut(roles::ADD_REACTIONS) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas réagir dans ce salon",
        ));
    }
    if ajout {
        match ajouter_reaction(pool, id, &u.id, emoji).await {
            Ok(true) => {}
            Ok(false) => {
                return Ok(write_err(
                    StatusCode::CONFLICT,
                    "too_many_reactions",
                    "20 emojis différents maximum par message",
                ));
            }
            Err(e) => return Err(bd(e)),
        }
    } else {
        sqlx::query("DELETE FROM reactions WHERE message_id=$1 AND user_id=$2::uuid AND emoji=$3")
            .bind(id)
            .bind(&u.id)
            .bind(emoji)
            .execute(pool)
            .await
            .map_err(bd)?;
    }
    let charge = json!({ "message_id": param(&p.0, "id"), "emoji": emoji, "user_id": u.id, "pseudo": u.pseudo, "added": ajout });
    publier(&state, pool, &conv, "reaction.updated", &charge).await;
    Ok(sans_contenu())
}

async fn ajouter_reaction(
    pool: &PgPool,
    id: i64,
    user: &str,
    emoji: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM messages WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let deja: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM reactions WHERE message_id=$1 AND emoji=$2)",
    )
    .bind(id)
    .bind(emoji)
    .fetch_one(&mut *tx)
    .await?;
    if !deja {
        let n: i64 =
            sqlx::query_scalar("SELECT count(DISTINCT emoji) FROM reactions WHERE message_id=$1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if n >= REACTIONS_MAX {
            return Ok(false);
        }
    }
    sqlx::query(
        "INSERT INTO reactions (message_id, user_id, emoji) VALUES ($1,$2::uuid,$3) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(user)
    .bind(emoji)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

fn vue_dm(id: &str, autre_id: &str, pseudo: &str, en_ligne: bool) -> Value {
    json!({ "id": id, "other": { "id": autre_id, "pseudo": pseudo }, "online": en_ligne })
}

async fn lister_dms(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let lignes = sqlx::query_as::<_, (String, String, String)>(
        "SELECT d.id::text, o.id::text, o.pseudo FROM dm_conversations d
         JOIN users o ON o.id = CASE WHEN d.user_a=$1::uuid THEN d.user_b ELSE d.user_a END
         WHERE d.user_a=$1::uuid OR d.user_b=$1::uuid
         ORDER BY COALESCE((SELECT max(id) FROM messages WHERE dm_id = d.id), 0) DESC, d.created_at DESC",
    )
    .bind(&u.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let sortie: Vec<Value> = lignes
        .iter()
        .map(|(id, oid, pseudo)| vue_dm(id, oid, pseudo, state.hub().online(oid)))
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(sortie)))
}

async fn ouvrir_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    if !state
        .limiteurs()
        .dms
        .allow(&u.id, DMS_PAR_FENETRE, FENETRE_DMS)
    {
        return Ok(trop_de_requetes(FENETRE_DMS.as_secs()));
    }
    let e = decode(body).await?;
    let pseudo = texte_strict(&e, "pseudo").ok_or_else(mauvais)?;
    let autre = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text, pseudo FROM users WHERE lower(pseudo)=lower($1)",
    )
    .bind(pseudo.trim())
    .fetch_optional(pool)
    .await
    .map_err(bd)?;
    let Some((autre_id, autre_pseudo)) = autre else {
        return Ok(write_err(
            StatusCode::NOT_FOUND,
            "user_not_found",
            "utilisateur introuvable",
        ));
    };
    if autre_id == u.id {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "self_dm",
            "impossible de s'écrire à soi-même",
        ));
    }
    let (a, b) = if u.id > autre_id {
        (autre_id.clone(), u.id.clone())
    } else {
        (u.id.clone(), autre_id.clone())
    };
    let id: String = sqlx::query_scalar(
        "INSERT INTO dm_conversations (user_a, user_b) VALUES ($1::uuid,$2::uuid)
         ON CONFLICT (user_a, user_b) DO UPDATE SET user_a = EXCLUDED.user_a RETURNING id::text",
    )
    .bind(&a)
    .bind(&b)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &vue_dm(&id, &autre_id, &autre_pseudo, state.hub().online(&autre_id)),
    ))
}

async fn changer_epingle(
    state: &AppState,
    u: &User,
    p: &Params,
    etat: bool,
) -> Result<Response, Response> {
    let pool = pool(state)?;
    let id = id_message(param(&p.0, "id"))?;
    let (conv, auteur) = conv_du_message(state, id, &u.id).await?;
    if auteur != u.id && !conv.peut(roles::MANAGE_MESSAGES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante dans ce salon",
        ));
    }
    sqlx::query(
        "UPDATE messages SET pinned_at = CASE WHEN $2 THEN COALESCE(pinned_at, now()) ELSE NULL END WHERE id=$1",
    )
    .bind(id)
    .bind(etat)
    .execute(pool)
    .await
    .map_err(bd)?;
    let message = message_complet(pool, id, &u.id).await.map_err(bd)?;
    publier(state, pool, &conv, "message.updated", &message).await;
    Ok(sans_contenu())
}

async fn epingler(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    changer_epingle(&state, &u, &p, true).await
}

async fn desepingler(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    changer_epingle(&state, &u, &p, false).await
}

async fn lister_epingles(state: &AppState, u: &User, conv: Conv) -> Result<Response, Response> {
    let pool = pool(state)?;
    let (colonne, cible) = match &conv.dm_id {
        Some(dm) => ("m.dm_id", dm.clone()),
        None => ("m.channel_id", conv.channel_id.clone().unwrap_or_default()),
    };
    let lignes = sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
        "{} WHERE {colonne}=$1::uuid AND m.pinned_at IS NOT NULL ORDER BY m.pinned_at DESC LIMIT 100",
        selection()
    )))
    .bind(&cible)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let mut messages = construire(lignes);
    hydrater(pool, &mut messages, &u.id).await.map_err(bd)?;
    let messages: Vec<Value> = messages.into_iter().map(Value::Object).collect();
    Ok(write_json(StatusCode::OK, &json!({ "messages": messages })))
}

async fn epingles_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    lister_epingles(&state, &u, conv).await
}

async fn epingles_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    lister_epingles(&state, &u, conv).await
}

const SAISIES: usize = 2;
const FENETRE_SAISIE: Duration = Duration::from_secs(3);

async fn saisie(state: &AppState, u: &User, conv: Conv) -> Result<Response, Response> {
    let pool = pool(state)?;
    let cible = conv
        .channel_id
        .as_deref()
        .or(conv.dm_id.as_deref())
        .unwrap_or_default();
    let cle = format!("{}{cible}", u.id);
    if state
        .limiteurs()
        .saisie
        .allow(&cle, SAISIES, FENETRE_SAISIE)
        && let Ok(to) = participants(pool, &conv).await
    {
        let autres: Vec<String> = to.into_iter().filter(|id| *id != u.id).collect();
        if !autres.is_empty() {
            state.publier(
                &autres,
                "typing",
                &json!({
                    "channel_id": conv.channel_id,
                    "dm_id": conv.dm_id,
                    "user_id": u.id,
                    "pseudo": u.pseudo,
                }),
            );
        }
    }
    Ok(sans_contenu())
}

async fn saisie_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    saisie(&state, &u, conv).await
}

async fn saisie_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    saisie(&state, &u, conv).await
}

#[cfg(test)]
pub(crate) mod tests {
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

        async fn avec_entetes(
            &self,
            qui: &str,
            chemin: &str,
            corps: Value,
        ) -> (u16, HeaderMap, Value) {
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

        pub(crate) async fn role(
            &self,
            nom: &str,
            permissions: &[&str],
            a: &[(&str, &str)],
        ) -> String {
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
    async fn debit_des_reactions_et_des_conversations_privees() {
        let Some(c) = cadre().await else { return };
        let m = c.poster("alice", "débit").await;
        let chemin = format!(
            "/messages/{}/reactions/%F0%9F%91%8D",
            m["id"].as_str().unwrap()
        );
        for i in 0..REACTIONS_PAR_FENETRE {
            let methode = if i % 2 == 0 { "PUT" } else { "DELETE" };
            assert_eq!(
                c.api("bob", methode, &chemin, Value::Null).await.0,
                204,
                "{i}"
            );
        }
        let (s, v) = c.api("bob", "DELETE", &chemin, Value::Null).await;
        assert_eq!((s, v["error"].as_str()), (429, Some("rate_limited")));
        assert_eq!(c.api("alice", "PUT", &chemin, Value::Null).await.0, 204);

        for _ in 0..DMS_PAR_FENETRE {
            c.ouvrir_dm("alice", "bob").await;
        }
        let (s, v) = c
            .api("alice", "POST", "/dms", json!({ "pseudo": "bob" }))
            .await;
        assert_eq!((s, v["error"].as_str()), (429, Some("rate_limited")));
        c.ouvrir_dm("bob", "alice").await;
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
}
