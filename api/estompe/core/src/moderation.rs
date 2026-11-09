use std::collections::BTreeMap;

use axum::{
    body::{Body, to_bytes},
    extract::State,
    http::StatusCode,
    response::Response,
    routing::{delete, get, post, put},
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    audit,
    gestion_roles::signaler,
    http::{CORPS_MAX, champ_texte, decode, premier_objet, write_err, write_json},
    messages::{self, nom_du_salon, pseudo_de},
    openapi,
    roles::{self, Access},
    servers::{
        Params, Serveur, acces, bd, booleen, entier, evenement_salon, evenement_serveur,
        horodatage, id_salon, liste, mauvais, membres_ids, nom_propre, non_trouve, param, pool,
        rfc3339, sans_contenu, uuid_valide,
    },
    state::AppState,
    users::User,
};

const MAX_RAISON: usize = 200;
const MAX_EXCLUSION: i64 = 28 * 24 * 3600;
const MAX_BAN: i64 = 10 * 365 * 24 * 3600;
const SUPPRESSIONS: [i64; 4] = [0, 3600, 86400, 604800];
fn fin(col: &str) -> String {
    format!("to_char({col} AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI')")
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/servers/{id}/members/{uid}/kick", post(expulser))
        .route("/servers/{id}/members/{uid}/ban", post(bannir))
        .route("/servers/{id}/members/{uid}/warn", post(avertir))
        .route("/servers/{id}/bans", get(lister_bans))
        .route("/servers/{id}/bans/{uid}", delete(lever_ban))
        .route(
            "/servers/{id}/members/{uid}/timeout",
            put(exclure).delete(lever_exclusion),
        )
        .route("/servers/{id}/members/{uid}/nickname", put(pseudo))
        .route("/servers/{id}/members/{uid}/history", get(historique))
        .route(
            "/servers/{id}/channels/{cid}/moderation",
            put(moderer_salon),
        )
        .route(
            "/channels/{cid}/messages/bulk-delete",
            post(supprimer_en_masse),
        )
        .route("/servers/{id}/transfer", post(transferer));
    for (methode, chemin, resume, reponses) in DOC {
        let tag = if chemin.starts_with("/channels/") {
            "messages"
        } else {
            "serveurs"
        };
        let op = openapi::operation(tag, resume, reponses, chemin);
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
        "/servers/{id}/members/{uid}/kick",
        "Expulser un membre (permission `kick_members`) ; il peut revenir avec une invitation. Ni soi-même, ni le créateur, ni un rang égal ou supérieur (403)",
        &[
            ("204", "ok"),
            ("400", "invalid_reason"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/members/{uid}/ban",
        "Bannir un membre (permission `ban_members`, même hiérarchie que l'expulsion) ; il est retiré du serveur et ne peut plus rejoindre jusqu'à la levée ou l'échéance",
        &[
            ("204", "ok"),
            ("400", "`invalid_ban` ou `invalid_reason`"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/bans",
        "Bannissements en cours (permission `ban_members`)",
        &[("200", "ok"), ("403", "Refusé")],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/bans/{uid}",
        "Lever un bannissement (permission `ban_members`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/members/{uid}/timeout",
        "Exclure temporairement un membre (permission `timeout_members`, même hiérarchie) — il reste membre et lit, mais ne peut plus écrire, modifier, réagir, joindre des fichiers, voter ni utiliser le vocal et la musique ; déconnecté du vocal",
        &[
            ("204", "ok"),
            ("400", "`invalid_timeout` ou `invalid_reason`"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/members/{uid}/timeout",
        "Lever l'exclusion avant l'échéance (permission `timeout_members`)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/members/{uid}/warn",
        "Avertir un membre (permission `timeout_members`, même hiérarchie) ; consigné dans l'historique et notifié au membre",
        &[
            ("204", "ok"),
            ("400", "invalid_reason"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/members/{uid}/nickname",
        "Changer le pseudo d'un membre sur ce serveur (permission `manage_nicknames`, même hiérarchie) ; chaîne vide : retire le pseudo de serveur",
        &[
            ("204", "ok"),
            ("400", "invalid_name"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/members/{uid}/history",
        "Historique de modération d'un membre — avertissements, exclusions, expulsions, bannissements (permission `view_audit_log`) ; mêmes entrées que le journal",
        &[("200", "ok"), ("403", "Refusé")],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/channels/{cid}/moderation",
        "Mode lent et verrouillage d'un salon texte (permission `manage_channels`). Mode lent ignoré pour ceux qui ont `manage_messages` ; salon verrouillé = lecture seule sauf pour eux",
        &[
            ("204", "ok"),
            ("400", "invalid_slowmode"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/messages/bulk-delete",
        "Supprimer plusieurs messages d'un salon (permission `manage_messages`) — `ids` (100 au plus) ou les `last` derniers (100 au plus), éventuellement d'un seul `author`",
        &[
            ("200", "ok — {deleted: [ids]}"),
            ("400", "invalid_bulk, invalid_ids"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/servers/{id}/transfer",
        "Donner le serveur à un autre membre (créateur seulement) ; l'ancien créateur devient un membre ordinaire",
        &[("204", "ok"), ("400", "invalid_user"), ("403", "Refusé")],
    ),
];

fn raison_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_reason",
        "raison : 200 caractères au plus",
    )
}

fn raison_valide(s: &str) -> bool {
    s.chars().count() <= MAX_RAISON && !s.chars().any(char::is_control)
}

fn texte(e: &Map<String, Value>, nom: &str) -> Result<String, Response> {
    champ_texte(e, nom).ok_or_else(mauvais)
}

fn nombre(e: &Map<String, Value>, nom: &str) -> Result<i64, Response> {
    entier(e, nom).ok_or_else(mauvais)
}

pub(crate) async fn corps_facultatif(body: Body) -> Result<Map<String, Value>, Response> {
    let octets = to_bytes(body, CORPS_MAX).await.map_err(|_| mauvais())?;
    if octets.is_empty() {
        return Ok(Map::new());
    }
    premier_objet(&octets).ok_or_else(mauvais)
}

async fn cible(
    pool: &PgPool,
    sv: &Serveur,
    acc: &Access,
    u: &User,
    uid: &str,
) -> Result<(), Response> {
    let introuvable = || non_trouve("membre introuvable");
    if !uuid_valide(uid) {
        return Err(introuvable());
    }
    if uid == u.id || uid == sv.owner_id {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce membre ne peut pas être visé",
        ));
    }
    let tacc = roles::access_for(pool, &sv.id, uid)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    if acc.top() <= tacc.top() {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce membre a un rang égal ou supérieur au vôtre",
        ));
    }
    Ok(())
}

async fn membre_vise<'a>(
    state: &AppState,
    u: &User,
    p: &'a Params,
    besoin: roles::Perm,
) -> Result<(Serveur, &'a str), Response> {
    let (sv, acc) = acces(state, u, param(&p.0, "id"), besoin).await?;
    let pool = state
        .pool()
        .map_err(axum::response::IntoResponse::into_response)?;
    let uid = param(&p.0, "uid");
    cible(pool, &sv, &acc, u, uid).await?;
    Ok((sv, uid))
}

fn jusqu_au(fin: Option<&str>) -> String {
    match fin {
        None => " définitivement".to_string(),
        Some(f) => format!(" jusqu'au {f} UTC"),
    }
}

async fn depart(
    state: &AppState,
    pool: &PgPool,
    serveur: &str,
    uid: &str,
    kind: &str,
    raison: &str,
    until: Option<&str>,
) {
    let mut to = membres_ids(pool, serveur).await.unwrap_or_default();
    to.push(uid.to_string());
    state.publier(
        &to,
        "member.left",
        &json!({
            "server_id": serveur,
            "user_id": uid,
            "kind": kind,
            "reason": raison,
            "until": until.map(rfc3339),
        }),
    );
}

async fn expulser(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, uid) = membre_vise(&state, &u, &p, roles::KICK_MEMBERS).await?;
    let e = corps_facultatif(body).await?;
    let raison = texte(&e, "reason")?;
    if !raison_valide(&raison) {
        return Ok(raison_invalide());
    }
    let pseudo = pseudo_de(pool, uid).await;
    sqlx::query("DELETE FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid")
        .bind(&sv.id)
        .bind(uid)
        .execute(pool)
        .await
        .map_err(bd)?;
    state.sortir_du_vocal(&sv.id, uid).await;
    audit::audit_membre(pool, &sv.id, &u.id, "member.kick", uid, &pseudo, &raison).await;
    depart(&state, pool, &sv.id, uid, "kick", &raison, None).await;
    Ok(sans_contenu())
}

type Supprimes = BTreeMap<String, Vec<String>>;

async fn bannir_en_base(
    pool: &PgPool,
    serveur: &str,
    uid: &str,
    par: &str,
    raison: &str,
    duree: i64,
    purge: i64,
) -> Result<(Option<String>, Option<String>, Supprimes), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let (fin, libelle) = sqlx::query_as::<_, (Option<String>, Option<String>)>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO server_bans (server_id, user_id, banned_by, reason, expires_at)
         VALUES ($1::uuid,$2::uuid,$3::uuid,$4, CASE WHEN $5::bigint > 0 THEN now() + $5::bigint * interval '1 second' END)
         ON CONFLICT (server_id, user_id) DO UPDATE SET banned_by=EXCLUDED.banned_by, reason=EXCLUDED.reason, expires_at=EXCLUDED.expires_at, created_at=now()
         RETURNING {}, {}",
        horodatage("expires_at"),
        fin("expires_at")
    )))
    .bind(serveur)
    .bind(uid)
    .bind(par)
    .bind(raison)
    .bind(duree)
    .fetch_one(&mut *tx)
    .await?;
    let mut supprimes = Supprimes::new();
    if purge > 0 {
        let lignes = sqlx::query_as::<_, (String, String)>(
            "DELETE FROM messages m USING channels c WHERE m.channel_id = c.id AND c.server_id=$1::uuid AND m.author_id=$2::uuid
             AND m.created_at > now() - $3::bigint * interval '1 second' RETURNING m.channel_id::text, m.id::text",
        )
        .bind(serveur)
        .bind(uid)
        .bind(purge)
        .fetch_all(&mut *tx)
        .await?;
        for (salon, id) in lignes {
            supprimes.entry(salon).or_default().push(id);
        }
    }
    sqlx::query("DELETE FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid")
        .bind(serveur)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((fin, libelle, supprimes))
}

async fn bannir(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, uid) = membre_vise(&state, &u, &p, roles::BAN_MEMBERS).await?;
    let e = decode(body).await?;
    let raison = texte(&e, "reason")?;
    let duree = nombre(&e, "duration")?;
    let purge = nombre(&e, "delete_messages")?;
    if !raison_valide(&raison) {
        return Ok(raison_invalide());
    }
    if !(0..=MAX_BAN).contains(&duree) || !SUPPRESSIONS.contains(&purge) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_ban",
            "durée : définitive (0) ou jusqu'à 10 ans ; messages supprimés : 0, 1 h, 24 h ou 7 jours",
        ));
    }
    let pseudo = pseudo_de(pool, uid).await;
    let (fin, libelle, supprimes) = bannir_en_base(pool, &sv.id, uid, &u.id, &raison, duree, purge)
        .await
        .map_err(bd)?;
    for (salon, ids) in supprimes {
        let charge = json!({ "channel_id": salon, "ids": ids });
        evenement_salon(&state, pool, &sv.id, &salon, "messages.deleted", &charge).await;
    }
    state.sortir_du_vocal(&sv.id, uid).await;
    let cible = format!("{pseudo}{}", jusqu_au(libelle.as_deref()));
    audit::audit_membre(pool, &sv.id, &u.id, "member.ban", uid, &cible, &raison).await;
    depart(&state, pool, &sv.id, uid, "ban", &raison, fin.as_deref()).await;
    Ok(sans_contenu())
}

async fn lister_bans(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p.0, "id"), roles::BAN_MEMBERS).await?;
    let lignes = sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(sqlx::AssertSqlSafe(format!(
        "SELECT b.user_id::text, u.pseudo, bu.pseudo, b.reason, {}, {} FROM server_bans b
         JOIN users u ON u.id = b.user_id JOIN users bu ON bu.id = b.banned_by
         WHERE b.server_id=$1::uuid AND (b.expires_at IS NULL OR b.expires_at > now()) ORDER BY b.created_at DESC",
        horodatage("b.created_at"),
        horodatage("b.expires_at")
    )))
    .bind(&sv.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let l: Vec<Value> = lignes
        .into_iter()
        .map(|(user_id, pseudo, by, reason, at, expires_at)| {
            json!({
                "user_id": user_id,
                "pseudo": pseudo,
                "by": by,
                "reason": reason,
                "at": rfc3339(&at),
                "expires_at": expires_at.as_deref().map(rfc3339),
            })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn lever_ban(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p.0, "id"), roles::BAN_MEMBERS).await?;
    let uid = param(&p.0, "uid");
    let introuvable = || non_trouve("bannissement introuvable");
    if !uuid_valide(uid) {
        return Ok(introuvable());
    }
    let n = sqlx::query(
        "DELETE FROM server_bans WHERE server_id=$1::uuid AND user_id=$2::uuid AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(&sv.id)
    .bind(uid)
    .execute(pool)
    .await
    .map_err(bd)?
    .rows_affected();
    if n == 0 {
        return Ok(introuvable());
    }
    let pseudo = pseudo_de(pool, uid).await;
    audit::audit_membre(pool, &sv.id, &u.id, "member.unban", uid, &pseudo, "").await;
    Ok(sans_contenu())
}

async fn fixer_exclusion(
    state: &AppState,
    pool: &PgPool,
    sv: &Serveur,
    u: &User,
    uid: &str,
    secondes: i64,
    raison: &str,
) -> Result<(), Response> {
    let (fin, libelle) = sqlx::query_as::<_, (Option<String>, Option<String>)>(sqlx::AssertSqlSafe(format!(
        "UPDATE server_members SET timeout_until = CASE WHEN $3::bigint > 0 THEN now() + $3::bigint * interval '1 second' END
         WHERE server_id=$1::uuid AND user_id=$2::uuid RETURNING {}, {}",
        horodatage("timeout_until"),
        fin("timeout_until")
    )))
    .bind(&sv.id)
    .bind(uid)
    .bind(secondes)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(|| non_trouve("introuvable"))?;
    let (action, suite) = match &libelle {
        Some(l) => ("member.timeout", jusqu_au(Some(l.as_str()))),
        None => ("member.untimeout", String::new()),
    };
    let pseudo = pseudo_de(pool, uid).await;
    audit::audit_membre(
        pool,
        &sv.id,
        &u.id,
        action,
        uid,
        &format!("{pseudo}{suite}"),
        raison,
    )
    .await;
    evenement_serveur(
        state,
        pool,
        &sv.id,
        "member.timeout",
        &json!({ "server_id": sv.id, "user_id": uid, "until": fin.as_deref().map(rfc3339), "reason": raison }),
    )
    .await;
    state.droits_modifies(&sv.id, uid).await;
    Ok(())
}

async fn exclure(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, uid) = membre_vise(&state, &u, &p, roles::TIMEOUT_MEMBERS).await?;
    let e = decode(body).await?;
    let raison = texte(&e, "reason")?;
    let duree = nombre(&e, "duration")?;
    if !raison_valide(&raison) {
        return Ok(raison_invalide());
    }
    if !(1..=MAX_EXCLUSION).contains(&duree) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_timeout",
            "durée : 1 seconde à 28 jours",
        ));
    }
    fixer_exclusion(&state, pool, &sv, &u, uid, duree, &raison).await?;
    Ok(sans_contenu())
}

async fn lever_exclusion(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, uid) = membre_vise(&state, &u, &p, roles::TIMEOUT_MEMBERS).await?;
    fixer_exclusion(&state, pool, &sv, &u, uid, 0, "").await?;
    Ok(sans_contenu())
}

async fn avertir(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, uid) = membre_vise(&state, &u, &p, roles::TIMEOUT_MEMBERS).await?;
    let e = decode(body).await?;
    let raison = texte(&e, "reason")?;
    if raison.is_empty() || !raison_valide(&raison) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_reason",
            "raison : 1 à 200 caractères",
        ));
    }
    let pseudo = pseudo_de(pool, uid).await;
    audit::audit_membre(pool, &sv.id, &u.id, "member.warn", uid, &pseudo, &raison).await;
    state.publier(
        &[uid.to_string()],
        "member.warned",
        &json!({ "server_id": sv.id, "reason": raison }),
    );
    Ok(sans_contenu())
}

async fn pseudo(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, uid) = membre_vise(&state, &u, &p, roles::MANAGE_NICKNAMES).await?;
    let e = decode(body).await?;
    let saisi = texte(&e, "nickname")?;
    let surnom = if saisi.is_empty() {
        saisi
    } else {
        match nom_propre(&saisi, 32) {
            Some(n) => n,
            None => {
                return Ok(write_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_name",
                    "pseudo : 1 à 32 caractères",
                ));
            }
        }
    };
    let n = sqlx::query(
        "UPDATE server_members SET nickname=$3 WHERE server_id=$1::uuid AND user_id=$2::uuid",
    )
    .bind(&sv.id)
    .bind(uid)
    .bind(&surnom)
    .execute(pool)
    .await
    .map_err(bd)?
    .rows_affected();
    if n == 0 {
        return Ok(non_trouve("introuvable"));
    }
    let pseudo = pseudo_de(pool, uid).await;
    audit::audit_membre(
        pool,
        &sv.id,
        &u.id,
        "member.nickname",
        uid,
        &format!("{pseudo} → {surnom}"),
        "",
    )
    .await;
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "member.nickname",
        &json!({ "server_id": sv.id, "user_id": uid, "nickname": surnom }),
    )
    .await;
    Ok(sans_contenu())
}

async fn historique(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p.0, "id"), roles::VIEW_AUDIT_LOG).await?;
    let uid = param(&p.0, "uid");
    if !uuid_valide(uid) {
        return Ok(non_trouve("membre introuvable"));
    }
    let l = audit::historique(pool, &sv.id, uid).await.map_err(bd)?;
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn moderer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p.0, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p.0)?;
    let e = decode(body).await?;
    let lent = nombre(&e, "slowmode")?;
    let verrou = booleen(&e, "locked").ok_or_else(mauvais)?;
    if lent != 0 && !(5..=21600).contains(&lent) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_slowmode",
            "mode lent : désactivé (0) ou de 5 s à 6 h",
        ));
    }
    let nom = sqlx::query_scalar::<_, String>(
        "UPDATE channels SET slowmode=$3, locked=$4 WHERE id=$2::uuid AND server_id=$1::uuid AND kind='text' RETURNING name",
    )
    .bind(&sv.id)
    .bind(&cid)
    .bind(lent as i32)
    .bind(verrou)
    .fetch_optional(pool)
    .await
    .map_err(bd)?;
    let Some(nom) = nom else {
        return Ok(non_trouve("introuvable"));
    };
    audit::audit(pool, &sv.id, &u.id, "channel.moderation", &nom).await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(sans_contenu())
}

async fn salon_a_moderer(state: &AppState, u: &User, cid: &str) -> Result<String, Response> {
    let conv = messages::conv_du_salon(state, cid, &u.id).await?;
    if !conv.peut(roles::MANAGE_MESSAGES) {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante dans ce salon",
        ));
    }
    Ok(conv.server_id.unwrap_or_default())
}

async fn supprimer_en_masse(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let cid = param(&p.0, "cid");
    let serveur = salon_a_moderer(&state, &u, cid).await?;
    let e = decode(body).await?;
    let ids = liste(&e, "ids").ok_or_else(mauvais)?;
    let derniers = nombre(&e, "last")?;
    let auteur = texte(&e, "author")?;
    let numeriques: Vec<i64> = match ids.iter().map(|i| i.parse()).collect() {
        Ok(v) => v,
        Err(_) => {
            return Ok(write_err(
                StatusCode::BAD_REQUEST,
                "invalid_ids",
                "identifiant de message invalide",
            ));
        }
    };
    let par_ids = !ids.is_empty();
    if par_ids == (derniers > 0)
        || ids.len() > 100
        || !(0..=100).contains(&derniers)
        || (!auteur.is_empty() && !uuid_valide(&auteur))
    {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_bulk",
            "indiquez 1 à 100 identifiants, ou les N derniers messages (1 à 100)",
        ));
    }
    let supprimes: Vec<String> = if par_ids {
        sqlx::query_scalar(
            "DELETE FROM messages WHERE channel_id=$1::uuid AND id = ANY($2::bigint[]) RETURNING id::text",
        )
        .bind(cid)
        .bind(&numeriques)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_scalar(
            "DELETE FROM messages WHERE id IN (SELECT id FROM messages WHERE channel_id=$1::uuid AND ($2='' OR author_id::text=$2)
             ORDER BY id DESC LIMIT $3) RETURNING id::text",
        )
        .bind(cid)
        .bind(&auteur)
        .bind(derniers)
        .fetch_all(pool)
        .await
    }
    .map_err(bd)?;
    if !supprimes.is_empty() {
        let charge = json!({ "channel_id": cid, "ids": supprimes });
        evenement_salon(&state, pool, &serveur, cid, "messages.deleted", &charge).await;
        let nom = nom_du_salon(pool, cid).await;
        let libelle = format!("{} messages dans {nom}", supprimes.len());
        audit::audit_membre(
            pool,
            &serveur,
            &u.id,
            "message.bulk_delete",
            &auteur,
            &libelle,
            "",
        )
        .await;
    }
    Ok(write_json(StatusCode::OK, &json!({ "deleted": supprimes })))
}

async fn transferer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p.0, "id"), 0).await?;
    if sv.owner_id != u.id {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "réservé au créateur du serveur",
        ));
    }
    let e = decode(body).await?;
    let vers = texte(&e, "user_id")?;
    let invalide = || {
        write_err(
            StatusCode::BAD_REQUEST,
            "invalid_user",
            "choisissez un autre membre du serveur",
        )
    };
    if !uuid_valide(&vers) || vers == u.id {
        return Ok(invalide());
    }
    let n = sqlx::query(
        "UPDATE servers SET owner_id=$2::uuid WHERE id=$1::uuid AND EXISTS (SELECT 1 FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid)",
    )
    .bind(&sv.id)
    .bind(&vers)
    .execute(pool)
    .await
    .map_err(bd)?
    .rows_affected();
    if n == 0 {
        return Ok(invalide());
    }
    let pseudo = pseudo_de(pool, &vers).await;
    audit::audit_membre(pool, &sv.id, &u.id, "server.transfer", &vers, &pseudo, "").await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
    };

    use tokio::sync::mpsc::Receiver;

    use super::*;
    use crate::{
        identity::Cred,
        servers::tests::{Env, appel, env, invitation_de, membre, serveur},
        state::Vocal,
    };

    struct Cadre {
        e: Env,
        sid: String,
        base: String,
        salon: String,
        code: String,
        alice: String,
        modo: String,
        junior: String,
        victime: String,
    }

    async fn code(e: &Env, qui: &str, methode: &str, chemin: &str, corps: &str) -> u16 {
        appel(e, qui, methode, chemin, Some(corps)).await.0.as_u16()
    }

    async fn erreur(e: &Env, qui: &str, methode: &str, chemin: &str, corps: &str) -> String {
        let (s, v) = appel(e, qui, methode, chemin, Some(corps)).await;
        format!("{} {}", s.as_u16(), v["error"].as_str().unwrap_or(""))
    }

    async fn rejoint(e: &Env, nom: &str, sid: &str) -> String {
        let id = membre(e, nom).await;
        let code = invitation_de(e, "alice", sid, "").await;
        let chemin = format!("/server-invites/{code}/join");
        assert_eq!(appel(e, nom, "POST", &chemin, None).await.0, StatusCode::OK);
        id
    }

    async fn nouveau_role(e: &Env, sid: &str, nom: &str, permissions: &[&str]) -> String {
        let corps = json!({ "name": nom, "color": "", "icon": "", "hoist": false, "permissions": permissions });
        let (s, v) = appel(
            e,
            "alice",
            "POST",
            &format!("/servers/{sid}/roles"),
            Some(&corps.to_string()),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    async fn cadre() -> Option<Cadre> {
        let e = env().await?;
        let alice = membre(&e, "alice").await;
        let sid = serveur(&e, "Modération", "alice").await;
        let modo = rejoint(&e, "modo", &sid).await;
        let junior = rejoint(&e, "junior", &sid).await;
        let victime = rejoint(&e, "victime", &sid).await;
        let boss = nouveau_role(
            &e,
            &sid,
            "Modo",
            &[
                "kick_members",
                "ban_members",
                "timeout_members",
                "manage_nicknames",
                "view_audit_log",
                "manage_messages",
                "manage_channels",
            ],
        )
        .await;
        let petit = nouveau_role(&e, &sid, "Junior", &["kick_members"]).await;
        let base = format!("/servers/{sid}");
        let ordre = json!({ "ids": [boss, petit] }).to_string();
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{base}/roles-order"), &ordre).await,
            204
        );
        for (id, role) in [(&modo, &boss), (&junior, &petit)] {
            assert_eq!(
                code(
                    &e,
                    "alice",
                    "PUT",
                    &format!("{base}/members/{id}/roles/{role}"),
                    ""
                )
                .await,
                204
            );
        }
        let (_, v) = appel(&e, "alice", "GET", &base, None).await;
        let salon = v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "text")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let code = invitation_de(&e, "alice", &sid, "").await;
        Some(Cadre {
            e,
            sid,
            base,
            salon,
            code,
            alice,
            modo,
            junior,
            victime,
        })
    }

    impl Cadre {
        async fn rejoindre(&self, qui: &str) -> u16 {
            let chemin = format!("/server-invites/{}/join", self.code);
            appel(&self.e, qui, "POST", &chemin, None).await.0.as_u16()
        }

        fn ecouter(&self, id: &str) -> (Arc<crate::hub::Conn>, Receiver<String>) {
            let (c, rx, _) = self.e.state.hub().register(
                id,
                Cred {
                    value: "x".into(),
                    cookie: false,
                },
            );
            (c, rx)
        }

        async fn message(&self, auteur: &str, age: &str) -> String {
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "INSERT INTO messages (channel_id, author_id, content, created_at) VALUES ($1::uuid,$2::uuid,'x', now() - interval '{age}') RETURNING id::text"
            )))
            .bind(&self.salon)
            .bind(auteur)
            .fetch_one(self.e.state.pool().ok().unwrap())
            .await
            .unwrap()
        }

        async fn dernier_audit(&self) -> (String, String, String, String) {
            sqlx::query_as(
                "SELECT action, target, COALESCE(target_id::text,''), reason FROM audit_log WHERE server_id=$1::uuid ORDER BY id DESC LIMIT 1",
            )
            .bind(&self.sid)
            .fetch_one(self.e.state.pool().ok().unwrap())
            .await
            .unwrap()
        }

        async fn sql(&self, requete: &str) {
            sqlx::query(sqlx::AssertSqlSafe(requete))
                .execute(self.e.state.pool().ok().unwrap())
                .await
                .unwrap();
        }
    }

    fn recus(rx: &mut Receiver<String>) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        while let Ok(m) = rx.try_recv() {
            let v: Value = serde_json::from_str(&m).unwrap();
            out.push((v["type"].as_str().unwrap().to_string(), v["data"].clone()));
        }
        out
    }

    struct Faux(Mutex<Vec<(String, String)>>);

    impl Vocal for Faux {
        fn sortir_du_serveur<'a>(
            &'a self,
            serveur: &'a str,
            membre: &'a str,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.0
                    .lock()
                    .unwrap()
                    .push((serveur.to_string(), membre.to_string()));
            })
        }

        fn participants<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = crate::state::ParticipantsVocal> + Send + 'a>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[tokio::test]
    async fn kick_ban_et_hierarchie() {
        let Some(c) = cadre().await else { return };
        let (e, b) = (&c.e, &c.base);
        let cible = |v: &str, a: &str| format!("{b}/members/{v}/{a}");
        assert_eq!(
            code(e, "victime", "POST", &cible(&c.junior, "kick"), "{}").await,
            403
        );
        assert_eq!(
            code(e, "modo", "POST", &cible(&c.modo, "kick"), "{}").await,
            403
        );
        assert_eq!(
            code(e, "modo", "POST", &cible(&c.alice, "ban"), "{}").await,
            403
        );
        assert_eq!(
            code(e, "junior", "POST", &cible(&c.modo, "kick"), "{}").await,
            403
        );
        assert_eq!(
            code(e, "junior", "POST", &cible(&c.victime, "ban"), "{}").await,
            403
        );
        assert_eq!(
            code(
                e,
                "junior",
                "PUT",
                &cible(&c.victime, "timeout"),
                r#"{"duration":60}"#
            )
            .await,
            403
        );
        assert_eq!(
            code(e, "modo", "POST", &cible("pas-un-uuid", "kick"), "{}").await,
            404
        );
        assert_eq!(
            code(
                e,
                "modo",
                "POST",
                &cible("0123abcd-0123-4567-89ab-0123456789ab", "kick"),
                "{}"
            )
            .await,
            404
        );
        let longue = json!({ "reason": "x".repeat(201) }).to_string();
        assert_eq!(
            erreur(e, "modo", "POST", &cible(&c.victime, "kick"), &longue).await,
            "400 invalid_reason"
        );
        assert_eq!(
            erreur(
                e,
                "modo",
                "POST",
                &cible(&c.victime, "kick"),
                r#"{"reason":"a\u0001"}"#
            )
            .await,
            "400 invalid_reason"
        );
        assert_eq!(
            erreur(
                e,
                "modo",
                "POST",
                &cible(&c.victime, "kick"),
                r#"{"reason":3}"#
            )
            .await,
            "400 bad_request"
        );

        assert_eq!(
            code(
                e,
                "modo",
                "POST",
                &cible(&c.victime, "kick"),
                r#"{"reason":"spam"}"#
            )
            .await,
            204
        );
        assert_eq!(
            c.dernier_audit().await,
            (
                "member.kick".into(),
                "victime".into(),
                c.victime.clone(),
                "spam".into()
            )
        );
        assert_eq!(
            appel(e, "victime", "GET", b, None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(e, "modo", "POST", &cible(&c.victime, "kick"), "{}").await,
            404
        );
        assert_eq!(c.rejoindre("victime").await, 200);
        assert_eq!(
            code(e, "modo", "POST", &cible(&c.victime, "kick"), "").await,
            204
        );
        assert_eq!(c.rejoindre("victime").await, 200);

        c.message(&c.victime, "0 seconds").await;
        let ancien = c.message(&c.victime, "2 hours").await;
        let ban = r#"{"reason":"insultes","delete_messages":3600}"#;
        assert_eq!(
            code(e, "modo", "POST", &cible(&c.victime, "ban"), ban).await,
            204
        );
        assert_eq!(
            c.dernier_audit().await,
            (
                "member.ban".into(),
                "victime définitivement".into(),
                c.victime.clone(),
                "insultes".into()
            )
        );
        let restants: Vec<String> =
            sqlx::query_scalar("SELECT id::text FROM messages WHERE author_id=$1::uuid")
                .bind(&c.victime)
                .fetch_all(e.state.pool().ok().unwrap())
                .await
                .unwrap();
        assert_eq!(restants, [ancien]);
        assert_eq!(c.rejoindre("victime").await, 403);
        assert_eq!(
            appel(e, "victime", "GET", b, None).await.0,
            StatusCode::NOT_FOUND
        );
        let (s, v) = appel(e, "modo", "GET", &format!("{b}/bans"), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["user_id"], c.victime.as_str());
        assert_eq!(v[0]["pseudo"], "victime");
        assert_eq!(v[0]["by"], "modo");
        assert_eq!(v[0]["reason"], "insultes");
        assert!(v[0]["expires_at"].is_null());
        assert!(v[0]["at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(
            code(e, "junior", "GET", &format!("{b}/bans"), "").await,
            403
        );
        assert_eq!(
            code(e, "modo", "POST", &cible(&c.victime, "ban"), "{}").await,
            404
        );
        assert_eq!(
            code(e, "modo", "DELETE", &format!("{b}/bans/pas-un-uuid"), "").await,
            404
        );
        assert_eq!(
            code(e, "modo", "DELETE", &format!("{b}/bans/{}", c.victime), "").await,
            204
        );
        assert_eq!(c.dernier_audit().await.0, "member.unban");
        assert_eq!(
            code(e, "modo", "DELETE", &format!("{b}/bans/{}", c.victime), "").await,
            404
        );
        assert_eq!(c.rejoindre("victime").await, 200);

        for (corps, attendu) in [
            (r#"{"duration":-1}"#, "400 invalid_ban"),
            (r#"{"delete_messages":5}"#, "400 invalid_ban"),
            (r#"{"duration":315360001}"#, "400 invalid_ban"),
            (r#"{"duration":1.5}"#, "400 bad_request"),
            ("", "400 bad_request"),
        ] {
            assert_eq!(
                erreur(e, "modo", "POST", &cible(&c.victime, "ban"), corps).await,
                attendu,
                "{corps}"
            );
        }
        assert_eq!(
            code(
                e,
                "modo",
                "POST",
                &cible(&c.victime, "ban"),
                r#"{"duration":3600}"#
            )
            .await,
            204
        );
        let (a, t, _, _) = c.dernier_audit().await;
        assert_eq!(a, "member.ban");
        assert!(
            t.starts_with("victime jusqu'au ") && t.ends_with(" UTC"),
            "{t}"
        );
        assert_eq!(c.rejoindre("victime").await, 403);
        c.sql("UPDATE server_bans SET expires_at = now() - interval '1 second'")
            .await;
        assert_eq!(c.rejoindre("victime").await, 200);
        assert_eq!(
            code(e, "modo", "DELETE", &format!("{b}/bans/{}", c.victime), "").await,
            404
        );

        let (s, v) = appel(e, "modo", "GET", &cible(&c.victime, "history"), None).await;
        assert_eq!(s, StatusCode::OK);
        let actions: Vec<&str> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["action"].as_str().unwrap())
            .collect();
        assert_eq!(
            actions,
            [
                "member.ban",
                "member.unban",
                "member.ban",
                "member.kick",
                "member.kick"
            ]
        );
        assert_eq!(v[4]["reason"], "spam");
        assert_eq!(v[4]["actor"], "modo");
        assert_eq!(v[4]["target_id"], c.victime.as_str());
        assert_eq!(
            code(e, "victime", "GET", &cible(&c.victime, "history"), "").await,
            403
        );
        assert_eq!(
            code(e, "modo", "GET", &cible("pas-un-uuid", "history"), "").await,
            404
        );
        let (_, v) = appel(e, "modo", "GET", &cible(&c.modo, "history"), None).await;
        assert!(v.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn evenements_de_depart_et_sortie_du_vocal() {
        let Some(c) = cadre().await else { return };
        let faux = Arc::new(Faux(Mutex::default()));
        c.e.state.definir_vocal(faux.clone());
        let tiers = membre(&c.e, "tiers").await;
        let (_a, mut rx_alice) = c.ecouter(&c.alice);
        let (_m, mut rx_modo) = c.ecouter(&c.modo);
        let (_v, mut rx_victime) = c.ecouter(&c.victime);
        let (_t, mut rx_tiers) = c.ecouter(&tiers);
        let lien = |a: &str| format!("{}/members/{}/{a}", c.base, c.victime);

        assert_eq!(
            code(&c.e, "modo", "POST", &lien("kick"), r#"{"reason":"spam"}"#).await,
            204
        );
        let attendu = vec![(
            "member.left".to_string(),
            json!({ "server_id": c.sid, "user_id": c.victime, "kind": "kick", "reason": "spam", "until": null }),
        )];
        assert_eq!(recus(&mut rx_alice), attendu);
        assert_eq!(recus(&mut rx_modo), attendu);
        assert_eq!(recus(&mut rx_victime), attendu);
        assert!(recus(&mut rx_tiers).is_empty());
        assert_eq!(
            *faux.0.lock().unwrap(),
            [(c.sid.clone(), c.victime.clone())]
        );

        assert_eq!(c.rejoindre("victime").await, 200);
        recus(&mut rx_alice);
        recus(&mut rx_modo);
        recus(&mut rx_victime);
        let tous = sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM roles WHERE server_id=$1::uuid AND is_default",
        )
        .bind(&c.sid)
        .fetch_one(c.e.state.pool().ok().unwrap())
        .await
        .unwrap();
        let secret = format!("{}/channels/{}/overrides/{tous}", c.base, c.salon);
        assert_eq!(
            code(
                &c.e,
                "alice",
                "PUT",
                &secret,
                r#"{"deny":["view_channel"]}"#
            )
            .await,
            204
        );
        recus(&mut rx_alice);
        recus(&mut rx_modo);
        recus(&mut rx_victime);
        c.message(&c.victime, "0 seconds").await;
        let corps = r#"{"reason":"r","duration":3600,"delete_messages":86400}"#;
        assert_eq!(code(&c.e, "modo", "POST", &lien("ban"), corps).await, 204);
        let recu = recus(&mut rx_alice);
        let types: Vec<&str> = recu.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(types, ["messages.deleted", "member.left"]);
        assert_eq!(recu[0].1["channel_id"], c.salon.as_str());
        assert_eq!(recu[0].1["ids"].as_array().unwrap().len(), 1);
        assert_eq!(recu[1].1["kind"], "ban");
        assert_eq!(recu[1].1["reason"], "r");
        let fin = recu[1].1["until"].as_str().unwrap();
        assert!(fin.ends_with('Z') && fin.len() >= 20, "{fin}");
        let modo: Vec<String> = recus(&mut rx_modo).into_iter().map(|(t, _)| t).collect();
        assert_eq!(modo, ["member.left"]);
        assert_eq!(
            recus(&mut rx_victime)
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>(),
            ["member.left"]
        );
        assert!(recus(&mut rx_tiers).is_empty());
        assert_eq!(faux.0.lock().unwrap().len(), 2);

        assert_eq!(c.dernier_audit().await.0, "member.ban");
        assert_eq!(
            code(&c.e, "modo", "PUT", &lien("timeout"), r#"{"duration":60}"#).await,
            404
        );
        assert_eq!(faux.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn kick_et_ban_sans_vocal() {
        let Some(c) = cadre().await else { return };
        let lien = |a: &str| format!("{}/members/{}/{a}", c.base, c.victime);
        assert_eq!(code(&c.e, "modo", "POST", &lien("kick"), "{}").await, 204);
        assert_eq!(c.rejoindre("victime").await, 200);
        assert_eq!(code(&c.e, "modo", "POST", &lien("ban"), "{}").await, 204);
    }

    #[tokio::test]
    async fn timeout_avertissement_pseudo() {
        let Some(c) = cadre().await else { return };
        let appels = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let notes = appels.clone();
        c.e.state.sur_droits_modifies(move |_, serveur, membre| {
            let notes = notes.clone();
            async move { notes.lock().unwrap().push((serveur, membre)) }
        });
        let faux = Arc::new(Faux(Mutex::default()));
        c.e.state.definir_vocal(faux.clone());
        let (_a, mut rx_alice) = c.ecouter(&c.alice);
        let (_v, mut rx_victime) = c.ecouter(&c.victime);
        let lien = |a: &str| format!("{}/members/{}/{a}", c.base, c.victime);
        let exclu = || async {
            roles::access_for(c.e.state.pool().ok().unwrap(), &c.sid, &c.victime)
                .await
                .unwrap()
                .unwrap()
                .timed_out
        };

        for corps in [r#"{"duration":0}"#, r#"{"duration":2419201}"#, "{}"] {
            assert_eq!(
                erreur(&c.e, "modo", "PUT", &lien("timeout"), corps).await,
                "400 invalid_timeout",
                "{corps}"
            );
        }
        assert_eq!(
            erreur(
                &c.e,
                "modo",
                "PUT",
                &lien("timeout"),
                r#"{"duration":60,"reason":"a\nb"}"#
            )
            .await,
            "400 invalid_reason"
        );
        assert!(appels.lock().unwrap().is_empty());
        assert_eq!(
            code(
                &c.e,
                "modo",
                "PUT",
                &lien("timeout"),
                r#"{"duration":2419200,"reason":"calme"}"#
            )
            .await,
            204
        );
        assert!(exclu().await);
        let (a, t, _, r) = c.dernier_audit().await;
        assert_eq!((a.as_str(), r.as_str()), ("member.timeout", "calme"));
        assert!(t.starts_with("victime jusqu'au "), "{t}");
        let recu = recus(&mut rx_alice);
        assert_eq!(recu.len(), 1);
        assert_eq!(recu[0].0, "member.timeout");
        assert_eq!(recu[0].1["user_id"], c.victime.as_str());
        assert_eq!(recu[0].1["reason"], "calme");
        assert!(recu[0].1["until"].as_str().unwrap().ends_with('Z'));
        assert_eq!(recus(&mut rx_victime).len(), 1);
        assert_eq!(
            *appels.lock().unwrap(),
            [(c.sid.clone(), c.victime.clone())]
        );

        assert_eq!(
            code(&c.e, "modo", "DELETE", &lien("timeout"), "").await,
            204
        );
        assert!(!exclu().await);
        assert_eq!(
            c.dernier_audit().await,
            (
                "member.untimeout".into(),
                "victime".into(),
                c.victime.clone(),
                String::new()
            )
        );
        let recu = recus(&mut rx_alice);
        assert_eq!(recu[0].1["until"], Value::Null);
        assert_eq!(appels.lock().unwrap().len(), 2);
        assert_eq!(
            code(&c.e, "modo", "PUT", &lien("timeout"), r#"{"duration":1}"#).await,
            204
        );
        c.sql("UPDATE server_members SET timeout_until = now() - interval '1 second'")
            .await;
        assert!(!exclu().await);
        recus(&mut rx_alice);
        recus(&mut rx_victime);
        appels.lock().unwrap().clear();

        for corps in [
            "{}",
            r#"{"reason":""}"#,
            &format!(r#"{{"reason":"{}"}}"#, "x".repeat(201)),
        ] {
            assert_eq!(
                erreur(&c.e, "modo", "POST", &lien("warn"), corps).await,
                "400 invalid_reason",
                "{corps}"
            );
        }
        assert_eq!(
            code(&c.e, "modo", "POST", &lien("warn"), r#"{"reason":"stop"}"#).await,
            204
        );
        assert_eq!(
            c.dernier_audit().await,
            (
                "member.warn".into(),
                "victime".into(),
                c.victime.clone(),
                "stop".into()
            )
        );
        assert_eq!(
            recus(&mut rx_victime),
            [(
                "member.warned".to_string(),
                json!({ "server_id": c.sid, "reason": "stop" })
            )]
        );
        assert!(recus(&mut rx_alice).is_empty());

        assert_eq!(
            erreur(
                &c.e,
                "modo",
                "PUT",
                &lien("nickname"),
                &json!({ "nickname": "x".repeat(33) }).to_string()
            )
            .await,
            "400 invalid_name"
        );
        assert_eq!(
            erreur(
                &c.e,
                "modo",
                "PUT",
                &lien("nickname"),
                r#"{"nickname":"   "}"#
            )
            .await,
            "400 invalid_name"
        );
        assert_eq!(
            code(
                &c.e,
                "modo",
                "PUT",
                &lien("nickname"),
                r#"{"nickname":" Surnom "}"#
            )
            .await,
            204
        );
        assert_eq!(c.dernier_audit().await.1, "victime → Surnom");
        let attendu = [(
            "member.nickname".to_string(),
            json!({ "server_id": c.sid, "user_id": c.victime, "nickname": "Surnom" }),
        )];
        assert_eq!(recus(&mut rx_alice), attendu);
        assert_eq!(recus(&mut rx_victime), attendu);
        assert_eq!(
            code(&c.e, "modo", "PUT", &lien("nickname"), r#"{"nickname":""}"#).await,
            204
        );
        assert_eq!(c.dernier_audit().await.1, "victime → ");
        assert_eq!(recus(&mut rx_alice)[0].1["nickname"], "");
        assert!(appels.lock().unwrap().is_empty());
        assert!(faux.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn mode_lent_et_verrouillage() {
        let Some(c) = cadre().await else { return };
        let appels = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let notes = appels.clone();
        c.e.state.sur_droits_modifies(move |_, serveur, membre| {
            let notes = notes.clone();
            async move { notes.lock().unwrap().push((serveur, membre)) }
        });
        let (_a, mut rx_alice) = c.ecouter(&c.alice);
        let lien = format!("{}/channels/{}/moderation", c.base, c.salon);
        assert_eq!(code(&c.e, "junior", "PUT", &lien, "{}").await, 403);
        for corps in [
            r#"{"slowmode":4}"#,
            r#"{"slowmode":21601}"#,
            r#"{"slowmode":-1}"#,
        ] {
            assert_eq!(
                erreur(&c.e, "modo", "PUT", &lien, corps).await,
                "400 invalid_slowmode",
                "{corps}"
            );
        }
        assert_eq!(
            erreur(&c.e, "modo", "PUT", &lien, r#"{"locked":"oui"}"#).await,
            "400 bad_request"
        );
        for corps in [
            r#"{"slowmode":0}"#,
            r#"{"slowmode":5,"locked":true}"#,
            r#"{"slowmode":21600}"#,
        ] {
            assert_eq!(
                code(&c.e, "modo", "PUT", &lien, corps).await,
                204,
                "{corps}"
            );
        }
        let (lent, verrou): (i32, bool) =
            sqlx::query_as("SELECT slowmode, locked FROM channels WHERE id=$1::uuid")
                .bind(&c.salon)
                .fetch_one(c.e.state.pool().ok().unwrap())
                .await
                .unwrap();
        assert_eq!((lent, verrou), (21600, false));
        assert_eq!(c.dernier_audit().await.0, "channel.moderation");
        assert_eq!(
            recus(&mut rx_alice)
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>(),
            ["roles.updated"; 3]
        );
        assert_eq!(
            *appels.lock().unwrap(),
            vec![(c.sid.clone(), String::new()); 3]
        );
        let vocal: String = sqlx::query_scalar(
            "INSERT INTO channels (server_id, name, kind, position) VALUES ($1::uuid,'voc','voice',99) RETURNING id::text",
        )
        .bind(&c.sid)
        .fetch_one(c.e.state.pool().ok().unwrap())
        .await
        .unwrap();
        let autre = format!("{}/channels/{vocal}/moderation", c.base);
        assert_eq!(code(&c.e, "modo", "PUT", &autre, "{}").await, 404);
        assert_eq!(
            code(
                &c.e,
                "modo",
                "PUT",
                &format!("{}/channels/pas-un-uuid/moderation", c.base),
                "{}"
            )
            .await,
            404
        );
    }

    #[tokio::test]
    async fn bulk_delete() {
        let Some(c) = cadre().await else { return };
        let (_a, mut rx_alice) = c.ecouter(&c.alice);
        let lien = format!("/channels/{}/messages/bulk-delete", c.salon);
        let mut ids = Vec::new();
        for _ in 0..3 {
            ids.push(c.message(&c.victime, "0 seconds").await);
            c.message(&c.junior, "0 seconds").await;
        }
        assert_eq!(
            code(&c.e, "junior", "POST", &lien, r#"{"last":1}"#).await,
            403
        );
        assert_eq!(
            code(
                &c.e,
                "modo",
                "POST",
                "/channels/pas-un-uuid/messages/bulk-delete",
                "{}"
            )
            .await,
            404
        );
        let trop = json!({ "ids": vec!["1"; 101] }).to_string();
        for (corps, attendu) in [
            ("{}", "400 invalid_bulk"),
            (r#"{"last":2,"ids":["1"]}"#, "400 invalid_bulk"),
            (r#"{"last":101}"#, "400 invalid_bulk"),
            (r#"{"last":-1}"#, "400 invalid_bulk"),
            (r#"{"last":1,"author":"abc"}"#, "400 invalid_bulk"),
            (trop.as_str(), "400 invalid_bulk"),
            (r#"{"ids":["abc"]}"#, "400 invalid_ids"),
            (r#"{"ids":[1]}"#, "400 bad_request"),
        ] {
            assert_eq!(
                erreur(&c.e, "modo", "POST", &lien, corps).await,
                attendu,
                "{corps}"
            );
        }
        assert!(recus(&mut rx_alice).is_empty());

        let avant = c.dernier_audit().await;
        let (s, v) = appel(&c.e, "modo", "POST", &lien, Some(r#"{"ids":["999999"]}"#)).await;
        assert_eq!((s, v), (StatusCode::OK, json!({ "deleted": [] })));
        assert!(recus(&mut rx_alice).is_empty());
        assert_eq!(c.dernier_audit().await, avant);

        let corps = json!({ "ids": [ids[0]] }).to_string();
        let (s, v) = appel(&c.e, "modo", "POST", &lien, Some(&corps)).await;
        assert_eq!((s, v), (StatusCode::OK, json!({ "deleted": [ids[0]] })));
        let recu = recus(&mut rx_alice);
        assert_eq!(
            recu,
            [(
                "messages.deleted".to_string(),
                json!({ "channel_id": c.salon, "ids": [ids[0]] })
            )]
        );
        assert_eq!(
            c.dernier_audit().await,
            (
                "message.bulk_delete".into(),
                "1 messages dans général".into(),
                String::new(),
                String::new()
            )
        );

        let corps = json!({ "last": 2, "author": c.victime }).to_string();
        let (_, v) = appel(&c.e, "modo", "POST", &lien, Some(&corps)).await;
        let mut supprimes: Vec<String> = serde_json::from_value(v["deleted"].clone()).unwrap();
        supprimes.sort();
        assert_eq!(supprimes, [ids[1].clone(), ids[2].clone()]);
        let (a, t, cible, _) = c.dernier_audit().await;
        assert_eq!(
            (a.as_str(), t.as_str(), cible.as_str()),
            (
                "message.bulk_delete",
                "2 messages dans général",
                c.victime.as_str()
            )
        );
        let reste: i64 =
            sqlx::query_scalar("SELECT count(*) FROM messages WHERE channel_id=$1::uuid")
                .bind(&c.salon)
                .fetch_one(c.e.state.pool().ok().unwrap())
                .await
                .unwrap();
        assert_eq!(reste, 3);
        let (_, v) = appel(&c.e, "modo", "POST", &lien, Some(r#"{"last":100}"#)).await;
        assert_eq!(v["deleted"].as_array().unwrap().len(), 3);

        let autre = serveur(&c.e, "Autre", "alice").await;
        let (_, v) = appel(&c.e, "alice", "GET", &format!("/servers/{autre}"), None).await;
        let salon = v["channels"][0]["id"].as_str().unwrap();
        let chemin = format!("/channels/{salon}/messages/bulk-delete");
        assert_eq!(
            code(&c.e, "modo", "POST", &chemin, r#"{"last":1}"#).await,
            404
        );
    }

    #[tokio::test]
    async fn transfert() {
        let Some(c) = cadre().await else { return };
        let (_a, mut rx_alice) = c.ecouter(&c.alice);
        let lien = format!("{}/transfer", c.base);
        let vers = |id: &str| json!({ "user_id": id }).to_string();
        assert_eq!(code(&c.e, "modo", "POST", &lien, &vers(&c.modo)).await, 403);
        for corps in [
            vers(&c.alice),
            vers("pas-un-uuid"),
            vers("0123abcd-0123-4567-89ab-0123456789ab"),
            "{}".to_string(),
        ] {
            assert_eq!(
                erreur(&c.e, "alice", "POST", &lien, &corps).await,
                "400 invalid_user",
                "{corps}"
            );
        }
        let hors = membre(&c.e, "hors").await;
        assert_eq!(
            erreur(&c.e, "alice", "POST", &lien, &vers(&hors)).await,
            "400 invalid_user"
        );
        assert_eq!(code(&c.e, "alice", "POST", &lien, "").await, 400);
        assert!(recus(&mut rx_alice).is_empty());

        assert_eq!(
            code(&c.e, "alice", "POST", &lien, &vers(&c.modo)).await,
            204
        );
        assert_eq!(
            c.dernier_audit().await,
            (
                "server.transfer".into(),
                "modo".into(),
                c.modo.clone(),
                String::new()
            )
        );
        assert_eq!(
            recus(&mut rx_alice),
            [("roles.updated".to_string(), json!({ "server_id": c.sid }))]
        );
        let proprietaire: String =
            sqlx::query_scalar("SELECT owner_id::text FROM servers WHERE id=$1::uuid")
                .bind(&c.sid)
                .fetch_one(c.e.state.pool().ok().unwrap())
                .await
                .unwrap();
        assert_eq!(proprietaire, c.modo);
        assert_eq!(
            code(&c.e, "alice", "POST", &lien, &vers(&c.junior)).await,
            403
        );
        assert_eq!(code(&c.e, "alice", "DELETE", &c.base, "").await, 403);
        assert_eq!(
            appel(&c.e, "alice", "GET", &c.base, None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            code(&c.e, "modo", "POST", &lien, &vers(&c.alice)).await,
            204
        );
    }
}
