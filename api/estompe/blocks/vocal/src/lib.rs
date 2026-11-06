#![allow(clippy::result_large_err)]

mod livekit;

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use axum::{
    body::{Body, to_bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use estompe_core::{
    api::ActiveUser,
    audit,
    http::{champ, champ_texte, decode, internal, write_err, write_json},
    messages::{Conv, EvenementMessage, SYSTEME},
    openapi,
    roles::{self, Access, Perm},
    servers,
    state::{AppState, ParticipantsVocal, Vocal, VocalInjoignable},
};
use livekit::{Erreur, Lk, Mod, Participant, capacite, salle, salon_de};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::{HttpMethod, path::Operation};
use utoipa_axum::router::OpenApiRouter;

const CORPS_WEBHOOK_MAX: usize = 1 << 16;
const SESSION_MAX: i64 = 12 * 3600;
const HORODATAGE: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.US";

type Params = Path<HashMap<String, String>>;

fn uuid_valide(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    internal()
}

fn pool(state: &AppState) -> Result<&PgPool, Response> {
    state.pool().map_err(IntoResponse::into_response)
}

fn introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

fn serveur_introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "serveur introuvable")
}

fn membre_introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "membre introuvable")
}

fn interdit() -> Response {
    write_err(StatusCode::FORBIDDEN, "forbidden", "permission manquante")
}

fn injoignable() -> Response {
    write_err(
        StatusCode::BAD_GATEWAY,
        "voice_unavailable",
        "serveur vocal injoignable",
    )
}

fn absent_du_vocal() -> Response {
    write_err(
        StatusCode::CONFLICT,
        "not_in_voice",
        "ce participant n'est plus dans le salon",
    )
}

fn param<'a>(p: &'a HashMap<String, String>, nom: &str) -> &'a str {
    p.get(nom).map_or("", String::as_str)
}

fn booleen(o: &Map<String, Value>, nom: &str) -> Option<bool> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn rfc3339(s: &str) -> String {
    match s.split_once('.') {
        Some((debut, fraction)) => match fraction.trim_end_matches('0') {
            "" => format!("{debut}Z"),
            f => format!("{debut}.{f}Z"),
        },
        None => format!("{s}Z"),
    }
}

fn mod_de(mic_off: bool, sound_off: bool, p: Perm) -> Mod {
    Mod {
        mic_off,
        sound_off,
        no_speak: !roles::contient(p, roles::SPEAK),
        no_video: !roles::contient(p, roles::VIDEO),
        no_screen: !roles::contient(p, roles::SHARE_SCREEN),
    }
}

struct Salon {
    id: String,
    serveur: String,
    limite: i32,
    perms: Perm,
}

async fn salon_vocal(pool: &PgPool, cid: &str, user: &str) -> Result<Salon, Response> {
    if !uuid_valide(cid) {
        return Err(introuvable());
    }
    let (serveur, kind, limite) = sqlx::query_as::<_, (String, String, i32)>(
        "SELECT c.server_id::text, c.kind, c.user_limit FROM channels c
         JOIN server_members m ON m.server_id = c.server_id WHERE c.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(cid)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let acces = roles::access_for(pool, &serveur, user)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let perms = acces.channel(cid);
    if !roles::contient(perms, roles::VIEW_CHANNEL) {
        return Err(introuvable());
    }
    if !servers::est_vocal(&kind) {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "not_voice_channel",
            "ce salon n'est pas un salon vocal",
        ));
    }
    Ok(Salon {
        id: cid.to_string(),
        serveur,
        limite,
        perms,
    })
}

async fn serveur_acces(
    pool: &PgPool,
    id: &str,
    user: &str,
    besoin: Perm,
) -> Result<(String, Access), Response> {
    if !uuid_valide(id) {
        return Err(serveur_introuvable());
    }
    let proprietaire: String = sqlx::query_scalar(
        "SELECT s.owner_id::text FROM servers s JOIN server_members m ON m.server_id = s.id
         WHERE s.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(serveur_introuvable)?;
    let acces = roles::access_for(pool, id, user)
        .await
        .map_err(bd)?
        .ok_or_else(internal)?;
    if !acces.can(besoin) {
        return Err(interdit());
    }
    Ok((proprietaire, acces))
}

async fn salons_vocaux(pool: &PgPool, serveur: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind = ANY($2) ORDER BY position",
    )
    .bind(serveur)
    .bind(&servers::TYPES_VOCAUX[..])
    .fetch_all(pool)
    .await
}

async fn mods(
    pool: &PgPool,
    salons: &[String],
) -> Result<HashMap<(String, String), (bool, bool)>, sqlx::Error> {
    if salons.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(sqlx::query_as::<_, (String, String, bool, bool)>(
        "SELECT channel_id::text, user_id::text, mic_off, sound_off FROM voice_mod WHERE channel_id = ANY($1::uuid[])",
    )
    .bind(salons)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(c, u, m, s)| ((c, u), (m, s)))
    .collect())
}

async fn enregistrer_mod(
    pool: &PgPool,
    serveur: &str,
    salon: &str,
    acteur: &str,
    cible: &str,
    mic_off: bool,
    sound_off: bool,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    if !mic_off && !sound_off {
        sqlx::query("DELETE FROM voice_mod WHERE channel_id=$1::uuid AND user_id=$2::uuid")
            .bind(salon)
            .bind(cible)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO voice_mod (channel_id, user_id, mic_off, sound_off) VALUES ($1::uuid,$2::uuid,$3,$4)
             ON CONFLICT (channel_id, user_id) DO UPDATE SET mic_off = EXCLUDED.mic_off, sound_off = EXCLUDED.sound_off",
        )
        .bind(salon)
        .bind(cible)
        .bind(mic_off)
        .bind(sound_off)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO voice_mod_log (server_id, channel_id, actor, target, mic_off, sound_off) VALUES ($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5,$6)",
    )
    .bind(serveur)
    .bind(salon)
    .bind(acteur)
    .bind(cible)
    .bind(mic_off)
    .bind(sound_off)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

async fn fermer_session(pool: &PgPool, user: &str, salon: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "WITH done AS (
            DELETE FROM voice_sessions WHERE user_id=$1::uuid AND ($2::text = '' OR channel_id::text=$2)
            RETURNING channel_id, LEAST(EXTRACT(EPOCH FROM now()-since)::bigint, $3::bigint) AS secs)
         INSERT INTO voice_stats (server_id, user_id, seconds)
         SELECT c.server_id, $1::uuid, d.secs FROM done d JOIN channels c ON c.id=d.channel_id
         ON CONFLICT (server_id, user_id) DO UPDATE SET seconds = voice_stats.seconds + EXCLUDED.seconds",
    )
    .bind(user)
    .bind(salon)
    .bind(SESSION_MAX)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn arrivee(pool: &PgPool, user: &str, salon: &str) -> Result<(), sqlx::Error> {
    fermer_session(pool, user, "").await?;
    sqlx::query("INSERT INTO voice_sessions (user_id, channel_id) VALUES ($1::uuid,$2::uuid)")
        .bind(user)
        .bind(salon)
        .execute(pool)
        .await
        .map(|_| ())
}

async fn depart(pool: &PgPool, user: &str, salon: &str) -> Result<(), sqlx::Error> {
    fermer_session(pool, user, salon).await
}

async fn depuis(
    pool: &PgPool,
    salons: &[String],
) -> Result<HashMap<(String, String), String>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (String, String, String)>(sqlx::AssertSqlSafe(format!(
        "SELECT channel_id::text, user_id::text, to_char(since AT TIME ZONE 'UTC', '{HORODATAGE}\"Z\"') FROM voice_sessions WHERE channel_id = ANY($1::uuid[])"
    )))
    .bind(salons)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(c, u, t)| ((c, u), t))
    .collect())
}

async fn evenement_salon(
    state: &AppState,
    pool: &PgPool,
    serveur: &str,
    salon: &str,
    typ: &str,
    data: &Value,
) {
    if let Ok(to) = roles::voyeurs_du_salon(pool, serveur, salon).await
        && !to.is_empty()
    {
        state.publier(&to, typ, data);
    }
}

async fn serveur_du_salon(pool: &PgPool, salon: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT server_id::text FROM channels WHERE id=$1::uuid")
        .bind(salon)
        .fetch_optional(pool)
        .await
}

async fn jeton(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let salon = salon_vocal(pool, param(&p, "cid"), &u.id).await?;
    if !roles::contient(salon.perms, roles::CONNECT) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas rejoindre ce salon vocal",
        ));
    }
    let lk = Lk::new(&state.config().livekit);
    if q.get("screen").is_some_and(|v| v == "1") {
        if !roles::contient(salon.perms, roles::SHARE_SCREEN) {
            return Ok(write_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "vous ne pouvez pas partager votre écran dans ce salon",
            ));
        }
        return Ok(write_json(
            StatusCode::OK,
            &json!({
                "url": lk.url_publique(),
                "token": lk.jeton(&salon.id, &format!("{}#screen", u.id), &u.pseudo, Mod::default()),
                "room": salle(&salon.id),
            }),
        ));
    }
    let max = capacite(salon.limite);
    match lk.admettre(&salon.id, &u.id, salon.limite).await {
        Ok(()) => {}
        Err(Erreur::Complet) => {
            return Ok(write_err(
                StatusCode::CONFLICT,
                "voice_full",
                &format!("salon vocal complet ({max} participants maximum)"),
            ));
        }
        Err(_) => return Ok(injoignable()),
    }
    let modere = mods(pool, std::slice::from_ref(&salon.id))
        .await
        .map_err(bd)?
        .get(&(salon.id.clone(), u.id.clone()))
        .copied()
        .unwrap_or_default();
    let (fps, hauteur) = lk.limites();
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "screen_max_fps": fps, "screen_max_height": hauteur,
            "url": lk.url_publique(),
            "token": lk.jeton(&salon.id, &u.id, &u.pseudo, mod_de(modere.0, modere.1, salon.perms)),
            "room": salle(&salon.id), "max_participants": max,
        }),
    ))
}

async fn presents(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (_, acces) = serveur_acces(pool, param(&p, "id"), &u.id, 0).await?;
    let lk = Lk::new(&state.config().livekit);
    let mut sortie: Vec<(String, Vec<Participant>)> = Vec::new();
    for cid in salons_vocaux(pool, param(&p, "id")).await.map_err(bd)? {
        if !roles::contient(acces.channel(&cid), roles::VIEW_CHANNEL) {
            continue;
        }
        let ps = lk.participants(&cid).await.map_err(|_| injoignable())?;
        sortie.push((cid, ps));
    }
    let ids: Vec<String> = sortie.iter().map(|(c, _)| c.clone()).collect();
    let depuis = depuis(pool, &ids).await;
    let modere = mods(pool, &ids).await.map_err(bd)?;
    let mut corps = Map::new();
    for (cid, mut ps) in sortie {
        for q in &mut ps {
            if let Ok(d) = &depuis
                && let Some(t) = d.get(&(cid.clone(), q.user_id.clone()))
            {
                q.since.clone_from(t);
            }
            let (m, s) = modere
                .get(&(cid.clone(), q.user_id.clone()))
                .copied()
                .unwrap_or_default();
            (q.mic_off, q.sound_off) = (m, s);
        }
        corps.insert(cid, json!(ps));
    }
    Ok(write_json(StatusCode::OK, &Value::Object(corps)))
}

async fn moderer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let salon = salon_vocal(pool, param(&p, "cid"), &u.id).await?;
    let (proprietaire, acces) = serveur_acces(pool, &salon.serveur, &u.id, 0).await?;
    let cible = param(&p, "uid");
    if !uuid_valide(cible) || cible == u.id || cible == proprietaire {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce participant ne peut pas être modéré",
        ));
    }
    let entree = decode(body).await?;
    let (Some(mic_off), Some(sound_off)) =
        (booleen(&entree, "mic_off"), booleen(&entree, "sound_off"))
    else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "JSON invalide",
        ));
    };
    let Some(acces_cible) = roles::access_for(pool, &salon.serveur, cible)
        .await
        .map_err(bd)?
    else {
        return Ok(absent_du_vocal());
    };
    let actuel = mods(pool, std::slice::from_ref(&salon.id))
        .await
        .map_err(bd)?
        .get(&(salon.id.clone(), cible.to_string()))
        .copied()
        .unwrap_or_default();
    if (mic_off != actuel.0 && !acces.can(roles::MUTE_MEMBERS))
        || (sound_off != actuel.1 && !acces.can(roles::DEAFEN_MEMBERS))
        || acces.top() <= acces_cible.top()
    {
        return Ok(interdit());
    }
    let m = mod_de(mic_off, sound_off, acces_cible.channel(&salon.id));
    match Lk::new(&state.config().livekit)
        .definir_mod(&salon.id, cible, m)
        .await
    {
        Ok(()) => {}
        Err(Erreur::Absent) => return Ok(absent_du_vocal()),
        Err(_) => return Ok(injoignable()),
    }
    enregistrer_mod(
        pool,
        &salon.serveur,
        &salon.id,
        &u.id,
        cible,
        m.mic_off,
        m.sound_off,
    )
    .await
    .map_err(bd)?;
    evenement_salon(
        &state,
        pool,
        &salon.serveur,
        &salon.id,
        "voice.mod",
        &json!({
            "server_id": salon.serveur, "channel_id": salon.id, "user_id": cible,
            "mic_off": m.mic_off, "sound_off": m.sound_off,
        }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn journal(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    serveur_acces(pool, param(&p, "id"), &u.id, roles::VIEW_AUDIT_LOG).await?;
    let lignes = sqlx::query_as::<_, (String, String, String, String, bool, bool)>(sqlx::AssertSqlSafe(format!(
        "SELECT to_char(at AT TIME ZONE 'UTC', '{HORODATAGE}'), channel_id::text, actor::text, target::text, mic_off, sound_off
         FROM voice_mod_log WHERE server_id=$1::uuid ORDER BY id DESC LIMIT 100"
    )))
    .bind(param(&p, "id"))
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let sortie: Vec<Value> = lignes
        .into_iter()
        .map(|(at, channel_id, actor, target, mic_off, sound_off)| {
            json!({
                "at": rfc3339(&at), "channel_id": channel_id, "actor": actor,
                "target": target, "mic_off": mic_off, "sound_off": sound_off,
            })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(sortie)))
}

async fn statistiques(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    serveur_acces(pool, param(&p, "id"), &u.id, 0).await?;
    let lignes = sqlx::query_as::<_, (String, i64)>(
        "SELECT user_id::text, SUM(secs)::bigint FROM (
            SELECT user_id, seconds AS secs FROM voice_stats WHERE server_id=$1::uuid
            UNION ALL
            SELECT v.user_id, LEAST(EXTRACT(EPOCH FROM now()-v.since)::bigint, $2::bigint)
            FROM voice_sessions v JOIN channels c ON c.id=v.channel_id WHERE c.server_id=$1::uuid
         ) t GROUP BY user_id",
    )
    .bind(param(&p, "id"))
    .bind(SESSION_MAX)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let sortie: Map<String, Value> = lignes.into_iter().map(|(u, n)| (u, json!(n))).collect();
    Ok(write_json(StatusCode::OK, &Value::Object(sortie)))
}

async fn webhook(State(state): State<AppState>, entetes: HeaderMap, body: Body) -> Response {
    let Ok(corps) = to_bytes(body, CORPS_WEBHOOK_MAX).await else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "corps illisible");
    };
    let autorisation = entetes
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let Ok(mut ev) = Lk::new(&state.config().livekit).webhook(autorisation, &corps) else {
        return write_err(
            StatusCode::UNAUTHORIZED,
            "bad_signature",
            "signature invalide",
        );
    };
    let Ok(pool) = pool(&state) else {
        return internal();
    };
    let arrive = ev.event == "participant_joined";
    let ecran = livekit::proprietaire_ecran(&ev.identite).map(str::to_string);
    if let Some(proprio) = &ecran {
        if ev.event == "participant_left" {
            ev.event = "track_unpublished".into();
            ev.source = json!("SCREEN_SHARE");
        }
        ev.identite.clone_from(proprio);
    }
    let salon = salon_de(&ev.salle).filter(|s| uuid_valide(s));
    if let Some(cid) = salon
        && ecran.is_none()
        && (arrive || ev.event == "participant_left")
        && let Ok(Some(sid)) = serveur_du_salon(pool, cid).await
    {
        if uuid_valide(&ev.identite) && ev.identite != SYSTEME {
            let r = if arrive {
                arrivee(pool, &ev.identite, cid).await
            } else {
                depart(pool, &ev.identite, cid).await
            };
            if let Err(e) = r {
                return bd(e);
            }
            if arrive {
                prevenir(&state, &sid, cid, &ev.identite, &ev.nom, "a rejoint").await;
            }
            if !arrive {
                state.sortie_salon_vocal(&sid, cid, &ev.identite).await;
            }
        }
        let a: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT to_char(now() AT TIME ZONE 'UTC', '{HORODATAGE}')"
        )))
        .fetch_one(pool)
        .await
        .map(|t: String| rfc3339(&t))
        .unwrap_or_default();
        evenement_salon(
            &state,
            pool,
            &sid,
            cid,
            "voice.state",
            &json!({
                "server_id": sid, "channel_id": cid, "user_id": ev.identite, "pseudo": ev.nom,
                "joined": arrive, "at": a,
            }),
        )
        .await;
    }
    let src = livekit::source(&ev.source);
    if !src.is_empty()
        && (ev.event == "track_published" || ev.event == "track_unpublished")
        && let Some(cid) = salon
        && let Ok(Some(sid)) = serveur_du_salon(pool, cid).await
    {
        evenement_salon(
            &state,
            pool,
            &sid,
            cid,
            "voice.share",
            &json!({
                "server_id": sid, "channel_id": cid, "user_id": ev.identite, "pseudo": ev.nom,
                "source": src, "on": ev.event == "track_published",
            }),
        )
        .await;
        if ev.event == "track_published" && uuid_valide(&ev.identite) {
            let action = if src == "camera" {
                "a allumé sa caméra"
            } else {
                "partage son écran"
            };
            prevenir(&state, &sid, cid, &ev.identite, &ev.nom, action).await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn prevenir(
    state: &AppState,
    serveur: &str,
    salon: &str,
    user: &str,
    pseudo: &str,
    action: &str,
) {
    state
        .notifier(&EvenementMessage {
            conv: Conv {
                server_id: Some(serveur.to_string()),
                channel_id: Some(salon.to_string()),
                ..Default::default()
            },
            message: json!({ "author": { "id": user, "pseudo": pseudo } }),
            vocal: action.to_string(),
            ..Default::default()
        })
        .await;
}

struct Cible {
    salon: Salon,
    acces: Access,
    acces_cible: Access,
    uid: String,
}

async fn cible_presente(
    state: &AppState,
    pool: &PgPool,
    user: &str,
    p: &HashMap<String, String>,
    besoin: Perm,
) -> Result<Cible, Response> {
    let salon = salon_vocal(pool, param(p, "cid"), user).await?;
    let (proprietaire, acces) = serveur_acces(pool, &salon.serveur, user, 0).await?;
    if !acces.can(besoin) {
        return Err(interdit());
    }
    let uid = param(p, "uid");
    if !uuid_valide(uid) {
        return Err(membre_introuvable());
    }
    if uid == user || uid == proprietaire {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce membre ne peut pas être visé",
        ));
    }
    let acces_cible = roles::access_for(pool, &salon.serveur, uid)
        .await
        .map_err(bd)?
        .ok_or_else(membre_introuvable)?;
    if acces.top() <= acces_cible.top() {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce membre a un rang égal ou supérieur au vôtre",
        ));
    }
    let ps = Lk::new(&state.config().livekit)
        .participants(&salon.id)
        .await
        .map_err(|_| injoignable())?;
    if !ps.iter().any(|q| q.user_id == uid) {
        return Err(absent_du_vocal());
    }
    Ok(Cible {
        salon,
        acces,
        acces_cible,
        uid: uid.to_string(),
    })
}

async fn expulser_du_salon(lk: &Lk, salon: &str, uid: &str) -> Result<(), Response> {
    match lk.retirer(salon, uid).await {
        Ok(()) | Err(Erreur::Absent) => {}
        Err(_) => return Err(injoignable()),
    }
    let _ = lk.retirer(salon, &format!("{uid}#screen")).await;
    Ok(())
}

async fn pseudo_de(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT pseudo FROM users WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

async fn nom_du_salon(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT name FROM channels WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

async fn deplacer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let c = cible_presente(&state, pool, &u.id, &p, roles::MOVE_MEMBERS).await?;
    let entree = decode(body).await?;
    let destination = champ_texte(&entree, "channel_id")
        .ok_or_else(|| write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide"))?;
    let invalide = || {
        write_err(
            StatusCode::BAD_REQUEST,
            "invalid_channel",
            "salon vocal de destination invalide",
        )
    };
    if !uuid_valide(&destination) || destination == c.salon.id {
        return Ok(invalide());
    }
    let valide = sqlx::query_scalar::<_, bool>(
        "SELECT server_id::text = $2 AND kind = ANY($3) FROM channels WHERE id=$1::uuid",
    )
    .bind(&destination)
    .bind(&c.salon.serveur)
    .bind(&servers::TYPES_VOCAUX[..])
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .unwrap_or(false);
    if !valide || !roles::contient(c.acces.channel(&destination), roles::VIEW_CHANNEL) {
        return Ok(invalide());
    }
    if !roles::contient(c.acces_cible.channel(&destination), roles::CONNECT) {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "cannot_join",
            "ce membre ne peut pas rejoindre ce salon",
        ));
    }
    let nom = nom_du_salon(pool, &destination).await;
    state.publier(
        std::slice::from_ref(&c.uid),
        "voice.moved",
        &json!({
            "server_id": c.salon.serveur, "from": c.salon.id, "to": destination,
            "by": u.pseudo, "to_name": nom,
        }),
    );
    expulser_du_salon(&Lk::new(&state.config().livekit), &c.salon.id, &c.uid).await?;
    let pseudo = pseudo_de(pool, &c.uid).await;
    audit::audit_membre(
        pool,
        &c.salon.serveur,
        &u.id,
        "voice.move",
        &c.uid,
        &format!("{pseudo} → {nom}"),
        "",
    )
    .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn deconnecter(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let c = cible_presente(&state, pool, &u.id, &p, roles::DISCONNECT_MEMBERS).await?;
    state.publier(
        std::slice::from_ref(&c.uid),
        "voice.disconnected",
        &json!({ "server_id": c.salon.serveur, "channel_id": c.salon.id }),
    );
    expulser_du_salon(&Lk::new(&state.config().livekit), &c.salon.id, &c.uid).await?;
    let pseudo = pseudo_de(pool, &c.uid).await;
    audit::audit_membre(
        pool,
        &c.salon.serveur,
        &u.id,
        "voice.disconnect",
        &c.uid,
        &pseudo,
        "",
    )
    .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn evincer(lk: &Lk, salons: &[String], user: &str) {
    for s in salons {
        if let Ok(ps) = lk.participants(s).await
            && ps.iter().any(|p| p.user_id == user)
        {
            let _ = lk.retirer(s, user).await;
            let _ = lk.retirer(s, &format!("{user}#screen")).await;
        }
    }
}

async fn synchroniser(state: &AppState, serveur: &str, user: &str) {
    let Ok(pool) = state.pool() else { return };
    let Ok(salons) = salons_vocaux(pool, serveur).await else {
        return;
    };
    let lk = Lk::new(&state.config().livekit);
    for cid in salons {
        let Ok(ps) = lk.participants(&cid).await else {
            continue;
        };
        if ps.is_empty() {
            continue;
        }
        let modere = mods(pool, std::slice::from_ref(&cid))
            .await
            .unwrap_or_default();
        for q in ps {
            if !user.is_empty() && q.user_id != user {
                continue;
            }
            let Ok(Some(acces)) = roles::access_for(pool, serveur, &q.user_id).await else {
                continue;
            };
            let perms = acces.channel(&cid);
            if !roles::contient(perms, roles::CONNECT) {
                let _ = lk.retirer(&cid, &q.user_id).await;
                continue;
            }
            let (m, s) = modere
                .get(&(cid.clone(), q.user_id.clone()))
                .copied()
                .unwrap_or_default();
            let _ = lk.definir_mod(&cid, &q.user_id, mod_de(m, s, perms)).await;
        }
    }
}

struct Accroche(AppState);

impl Vocal for Accroche {
    fn sortir_du_serveur<'a>(
        &'a self,
        serveur: &'a str,
        membre: &'a str,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let Ok(pool) = self.0.pool() else { return };
            if let Ok(salons) = salons_vocaux(pool, serveur).await {
                evincer(&Lk::new(&self.0.config().livekit), &salons, membre).await;
            }
        })
    }

    fn participants<'a>(
        &'a self,
        salon: &'a str,
    ) -> Pin<Box<dyn Future<Output = ParticipantsVocal> + Send + 'a>> {
        Box::pin(async move {
            match Lk::new(&self.0.config().livekit).participants(salon).await {
                Ok(ps) => Ok(ps.iter().map(|p| json!(p)).collect()),
                Err(Erreur::Absent) => Ok(Vec::new()),
                Err(_) => Err(VocalInjoignable),
            }
        })
    }
}

pub fn brancher(state: &AppState) {
    state.definir_vocal(Arc::new(Accroche(state.clone())));
    state.sur_droits_modifies(|s, serveur, membre| async move {
        synchroniser(&s, &serveur, &membre).await;
    });
    state.sur_acces_retire(|s, user| async move {
        let Ok(pool) = s.pool() else { return };
        if let Ok(salons) =
            sqlx::query_scalar::<_, String>("SELECT id::text FROM channels WHERE kind = ANY($1)")
                .bind(&servers::TYPES_VOCAUX[..])
                .fetch_all(pool)
                .await
        {
            evincer(&Lk::new(&s.config().livekit), &salons, &user).await;
        }
    });
}

fn operation(resume: &str, reponses: &[(&str, &str)], chemin: &str) -> Operation {
    openapi::operation("vocal", resume, reponses, chemin)
}

pub fn routes() -> OpenApiRouter<AppState> {
    let mut webhook_op = operation(
        "Webhook LiveKit (signé), relayé en évènement `voice.state`",
        &[("204", "ok"), ("401", "bad_signature")],
        "/voice/webhook",
    );
    webhook_op.security = Some(vec![]);
    let r = OpenApiRouter::new()
        .route("/voice/{cid}/token", post(jeton))
        .route("/servers/{id}/voice", get(presents))
        .route("/servers/{id}/voice/stats", get(statistiques))
        .route("/voice/{cid}/participants/{uid}/mod", put(moderer))
        .route("/servers/{id}/voice/moderation-log", get(journal))
        .route("/voice/webhook", post(webhook))
        .route("/voice/{cid}/participants/{uid}/move", put(deplacer))
        .route(
            "/voice/{cid}/participants/{uid}/disconnect",
            post(deconnecter),
        );
    let documentees = [
        (
            "/voice/{cid}/token",
            HttpMethod::Post,
            operation(
                "Jeton pour rejoindre un salon vocal avec le client LiveKit",
                &[
                    ("200", "ok"),
                    ("400", "not_voice_channel"),
                    ("403", "forbidden"),
                    ("404", "not_found"),
                    (
                        "409",
                        "voice_full : limite du salon atteinte (10 participants au plus)",
                    ),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/token",
            ),
        ),
        (
            "/servers/{id}/voice",
            HttpMethod::Get,
            operation(
                "Participants présents dans chaque salon vocal",
                &[
                    ("200", "ok"),
                    ("404", "not_found"),
                    ("502", "voice_unavailable"),
                ],
                "/servers/{id}/voice",
            ),
        ),
        (
            "/servers/{id}/voice/stats",
            HttpMethod::Get,
            operation(
                "Temps passé en vocal (secondes) par membre dans ce serveur, session en cours comprise",
                &[("200", "ok"), ("404", "not_found")],
                "/servers/{id}/voice/stats",
            ),
        ),
        (
            "/voice/{cid}/participants/{uid}/mod",
            HttpMethod::Put,
            operation(
                "Couper ou rétablir le micro et/ou le son d'un participant pour tout le salon (EST-102)",
                &[
                    ("204", "ok"),
                    ("400", "corps invalide"),
                    ("403", "forbidden"),
                    ("409", "not_in_voice"),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/participants/{uid}/mod",
            ),
        ),
        (
            "/servers/{id}/voice/moderation-log",
            HttpMethod::Get,
            operation(
                "Journal des coupures de micro ou de son (100 dernières, les plus récentes d'abord)",
                &[("200", "ok"), ("403", "forbidden"), ("404", "not_found")],
                "/servers/{id}/voice/moderation-log",
            ),
        ),
        ("/voice/webhook", HttpMethod::Post, webhook_op),
        (
            "/voice/{cid}/participants/{uid}/move",
            HttpMethod::Put,
            operation(
                "Déplacer un participant vers un autre salon vocal du serveur",
                &[
                    ("204", "ok"),
                    ("400", "invalid_channel"),
                    ("403", "forbidden"),
                    ("404", "not_found"),
                    ("409", "`not_in_voice` ou `cannot_join`"),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/participants/{uid}/move",
            ),
        ),
        (
            "/voice/{cid}/participants/{uid}/disconnect",
            HttpMethod::Post,
            operation(
                "Déconnecter un participant du vocal sans l'expulser du serveur",
                &[
                    ("204", "ok"),
                    ("403", "forbidden"),
                    ("404", "not_found"),
                    ("409", "not_in_voice"),
                    ("502", "voice_unavailable"),
                ],
                "/voice/{cid}/participants/{uid}/disconnect",
            ),
        ),
    ];
    documentees.into_iter().fold(r, |r, (chemin, methode, op)| {
        openapi::documenter(r, chemin, methode, op)
    })
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes())
}

#[cfg(test)]
mod tests {
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
        testutil::{self, BasePostgres, Faux, faux},
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
        let permission = faux(|_| (200, json!({ "allowed": true }).to_string().into_bytes())).await;
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
                "ListParticipants" => ok(
                    json!({ "participants": s.presents.get(&salle).cloned().unwrap_or_default() }),
                ),
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
            sqlx::query(
                "INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)",
            )
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
        let expire = signer("secret", &json!({ "exp": 1, "sha256": "x" }));
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
        let audit: String =
            sqlx::query_scalar("SELECT action FROM audit_log ORDER BY id DESC LIMIT 1")
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
}
