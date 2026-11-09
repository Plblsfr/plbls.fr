use std::{
    io::SeekFrom,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Multipart, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use hmac::{Hmac, KeyInit, Mac};
use http_body_util::Limited;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    api::ActiveUser,
    blob::{Disque, ErreurBlob, nouvelle_cle},
    config::Config,
    db,
    http::{internal, write_err, write_json},
    mp4, salons, sniff,
    state::AppState,
    users::User,
};

pub const TAILLE_MAX: i64 = 10 << 20;
pub const TAILLE_VOCAL_MAX: usize = 5 << 20;
pub const MINIATURE_MAX: usize = 64 << 10;
pub const ENVOIS_VIDEO_MAX: i64 = 4;
pub const FICHIERS_EN_ATTENTE_MAX: i64 = 20;
pub const LIMITE_HEURE: i64 = 200 << 20;
pub const LIMITE_JOUR: i64 = 1 << 30;
pub const AGE_ORPHELINS: Duration = Duration::from_secs(3600);
pub const INTERVALLE_PURGE: Duration = Duration::from_secs(10 * 60);
const DUREE_VOCAL_MAX_MS: i64 = 5 * 60 * 1000;
const ONDE_MAX: usize = 100;
const DUREE_LIEN: u64 = 3600;
const MARGE_CORPS: usize = 64 << 10;
const AFFICHABLES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "video/mp4",
    "video/webm",
    "audio/mp4",
    "application/ogg",
];
const VOCAUX: &[&str] = &["video/webm", "application/ogg", "video/mp4", "audio/mp4"];
const EXTENSIONS_IMAGE: &[(&str, &str)] = &[
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(envoyer))
        .layer(DefaultBodyLimit::disable())
        .routes(routes!(telecharger))
        .routes(routes!(miniature))
        .routes(routes!(lien))
}

#[derive(Serialize, ToSchema)]
#[allow(dead_code)]
struct LienReponse {
    token: String,
}

pub struct Fichier {
    id: String,
    name: String,
    mime: String,
    key: String,
    expire: bool,
}

enum ErreurAcces {
    NonTrouve,
    Interne,
}

impl From<sqlx::Error> for ErreurAcces {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => ErreurAcces::NonTrouve,
            _ => ErreurAcces::Interne,
        }
    }
}

impl IntoResponse for ErreurAcces {
    fn into_response(self) -> Response {
        match self {
            ErreurAcces::NonTrouve => non_trouve(),
            ErreurAcces::Interne => internal(),
        }
    }
}

fn non_trouve() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

pub fn est_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn maintenant() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn signer(cle: &[u8; 32], id: &str, exp: u64) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(cle).expect("clé HMAC");
    m.update(format!("{id}.{exp}").as_bytes());
    let hex: String = m
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{exp}.{hex}")
}

fn egaux(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a), Sha256::digest(b));
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn lien_valide(cle: &[u8; 32], id: &str, jeton: &str, maintenant: u64) -> bool {
    let Some((exp, _)) = jeton.split_once('.') else {
        return false;
    };
    let Ok(exp) = exp.parse::<u64>() else {
        return false;
    };
    maintenant < exp && egaux(jeton, &signer(cle, id, exp))
}

pub fn nom_propre(s: &str) -> String {
    let s = s.replace('\\', "/");
    let base = if s.is_empty() {
        "."
    } else {
        let coupe = s.trim_end_matches('/');
        if coupe.is_empty() {
            "/"
        } else {
            coupe.rsplit('/').next().unwrap_or(coupe)
        }
    };
    let sans_controles: Vec<char> = base.chars().filter(|c| !c.is_control()).collect();
    let debut = sans_controles.len().saturating_sub(100);
    let s: String = sans_controles[debut..].iter().collect();
    if matches!(s.as_str(), "." | "/" | "") {
        "fichier".into()
    } else {
        s
    }
}

fn nom_ascii(s: &str) -> String {
    s.chars()
        .map(|c| {
            if (c as u32) < 0x20 || (c as u32) > 0x7e || c == '"' || c == '\\' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn echappe_chemin(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~$&+=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn extension(nom: &str) -> String {
    nom.rfind('.')
        .map(|i| nom[i..].to_lowercase())
        .unwrap_or_default()
}

fn parametre(requete: Option<&str>, nom: &str) -> String {
    let decode = |s: &str| {
        percent_encoding::percent_decode_str(&s.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned()
    };
    requete
        .unwrap_or_default()
        .split('&')
        .filter_map(|p| p.split_once('=').or(Some((p, ""))))
        .find(|(k, _)| decode(k) == nom)
        .map(|(_, v)| decode(v))
        .unwrap_or_default()
}

fn analyser_vocal(duree: &str, onde: &str) -> Option<(i32, Vec<i32>)> {
    let d: i64 = duree.parse().ok()?;
    if !(1..=DUREE_VOCAL_MAX_MS).contains(&d) {
        return None;
    }
    let parts: Vec<&str> = onde.split(',').collect();
    if parts.len() > ONDE_MAX {
        return None;
    }
    let mut v = Vec::with_capacity(parts.len());
    for p in parts {
        let n: i64 = p.parse().ok()?;
        if !(0..=100).contains(&n) {
            return None;
        }
        v.push(n as i32);
    }
    Some((d as i32, v))
}

struct GardeVideo(AppState);

impl Drop for GardeVideo {
    fn drop(&mut self) {
        self.0.liberer_envoi_video();
    }
}

async fn lire_borne(
    champ: &mut axum::extract::multipart::Field<'_>,
    max: usize,
) -> Result<Vec<u8>, axum::extract::multipart::MultipartError> {
    let mut v = Vec::new();
    while v.len() <= max {
        match champ.chunk().await? {
            Some(c) => v.extend_from_slice(&c),
            None => break,
        }
    }
    v.truncate(max + 1);
    Ok(v)
}

fn limite_du_type(kind: &str, video_max: i64) -> (i64, String) {
    if kind == "video" {
        (
            video_max,
            format!("{} Mo maximum pour une vidéo", video_max >> 20),
        )
    } else {
        (TAILLE_MAX, "10 Mo maximum".to_string())
    }
}

fn fichier_illisible() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "bad_request",
        "fichier vide ou illisible",
    )
}

#[utoipa::path(post, path = "/files", tag = "fichiers",
    summary = "Envoyer un fichier (10 Mo max ; une vidéo, la limite vidéo), à joindre ensuite à un message via `file_ids`",
    request_body(content_type = "multipart/form-data", content = serde_json::Value),
    responses(
        (status = 201, description = "ok", body = serde_json::Value),
        (status = 400, description = "bad_request ou invalid_voice (durée ou forme d'onde hors bornes)"),
        (status = 413, description = "file_too_large ou quota_exceeded"),
        (status = 415, description = "type_mismatch, unsupported_voice ou unsupported_video"),
        (status = 429, description = "too_many_pending_files, ou upload_rate_limited"),
        (status = 507, description = "storage_full : le stockage est plein"),
    ))]
async fn envoyer(State(state): State<AppState>, req: Request) -> Response {
    let (mut parts, corps) = req.into_parts();
    let ActiveUser(u) = match ActiveUser::from_request_parts(&mut parts, &state).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let video_max = state.config().max_video_bytes;
    let plafond = (video_max as usize)
        .saturating_add(MINIATURE_MAX)
        .saturating_add(MARGE_CORPS);
    let req = Request::from_parts(parts, Body::new(Limited::new(corps, plafond)));
    let Ok(mut multipart) = Multipart::from_request(req, &state).await else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "multipart attendu");
    };
    let mut champs = std::collections::HashMap::<String, String>::new();
    let mut miniature: Vec<u8> = Vec::new();
    let manquant = || {
        write_err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "champ « file » manquant",
        )
    };
    loop {
        let mut champ = match multipart.next_field().await {
            Ok(Some(c)) => c,
            Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                return write_err(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "file_too_large",
                    &limite_du_type(champs.get("kind").map_or("", String::as_str), video_max).1,
                );
            }
            _ => return manquant(),
        };
        let nom_champ = champ.name().unwrap_or_default().to_string();
        if nom_champ == "file"
            && let Some(fichier) = champ.file_name().filter(|f| !f.is_empty())
        {
            let nom = nom_propre(fichier);
            let type_envoi = champs.get("kind").cloned().unwrap_or_default();
            let (limite, message) = limite_du_type(&type_envoi, video_max);
            let _garde = if type_envoi == "video" {
                if !state.reserver_envoi_video(ENVOIS_VIDEO_MAX) {
                    return write_err(
                        StatusCode::TOO_MANY_REQUESTS,
                        "too_many_video_uploads",
                        "trop de vidéos en cours d'envoi, réessayez dans un instant",
                    );
                }
                Some(GardeVideo(state.clone()))
            } else {
                None
            };
            let donnees = lire_borne(&mut champ, limite as usize).await;
            let donnees = match donnees {
                Ok(d) if d.len() as i64 > limite => {
                    return write_err(StatusCode::PAYLOAD_TOO_LARGE, "file_too_large", &message);
                }
                Ok(d) if d.is_empty() => return fichier_illisible(),
                Ok(d) => d,
                Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                    return write_err(StatusCode::PAYLOAD_TOO_LARGE, "file_too_large", &message);
                }
                Err(_) => return fichier_illisible(),
            };
            return enregistrer_envoi(&state, &u, nom, &type_envoi, &champs, miniature, donnees)
                .await;
        }
        if nom_champ == "thumb" {
            miniature = lire_borne(&mut champ, MINIATURE_MAX)
                .await
                .unwrap_or_default();
            continue;
        }
        let v = lire_borne(&mut champ, (1 << 10) - 1)
            .await
            .unwrap_or_default();
        champs.insert(nom_champ, String::from_utf8_lossy(&v).into_owned());
    }
}

struct Meta {
    nom: String,
    kind: &'static str,
    duree: i32,
    onde: Vec<i32>,
    miniature: Vec<u8>,
}

async fn enregistrer_envoi(
    state: &AppState,
    u: &User,
    nom: String,
    type_envoi: &str,
    champs: &std::collections::HashMap<String, String>,
    miniature: Vec<u8>,
    donnees: Vec<u8>,
) -> Response {
    let mut meta = Meta {
        nom,
        kind: "file",
        duree: 0,
        onde: vec![],
        miniature: vec![],
    };
    if type_envoi == "voice" {
        let champ = |k: &str| champs.get(k).map_or("", String::as_str);
        let Some((duree, onde)) = analyser_vocal(champ("duration_ms"), champ("waveform")) else {
            return write_err(
                StatusCode::BAD_REQUEST,
                "invalid_voice",
                "message vocal : durée de 1 ms à 5 min et forme d'onde de 1 à 100 valeurs entre 0 et 100",
            );
        };
        (meta.kind, meta.duree, meta.onde) = ("voice", duree, onde);
    }
    if type_envoi == "video" {
        let Some(ms) = mp4::analyser(&donnees) else {
            return write_err(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_video",
                "vidéo MP4 attendue : H.264, HEVC ou AV1 avec AAC ou Opus",
            );
        };
        if miniature.len() > MINIATURE_MAX || (!miniature.is_empty() && !mp4::est_jpeg(&miniature))
        {
            return write_err(
                StatusCode::BAD_REQUEST,
                "invalid_thumb",
                "miniature : JPEG de 64 Ko maximum",
            );
        }
        (meta.kind, meta.duree, meta.miniature) = ("video", ms as i32, miniature);
    }
    sauvegarder(state, u, meta, donnees).await
}

pub async fn importer_fichier(
    state: &AppState,
    u: &User,
    nom: String,
    donnees: Vec<u8>,
) -> Response {
    let meta = Meta {
        nom,
        kind: "file",
        duree: 0,
        onde: vec![],
        miniature: vec![],
    };
    sauvegarder(state, u, meta, donnees).await
}

async fn sauvegarder(state: &AppState, u: &User, meta: Meta, donnees: Vec<u8>) -> Response {
    let detecte = sniff::detecter(&donnees);
    let mime = detecte.split(';').next().unwrap_or(detecte);
    if let Some((_, voulu)) = EXTENSIONS_IMAGE
        .iter()
        .find(|(e, _)| *e == extension(&meta.nom))
        && *voulu != mime
    {
        return write_err(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "type_mismatch",
            "le contenu ne correspond pas au type annoncé par l'extension",
        );
    }
    if meta.kind == "voice" {
        if donnees.len() > TAILLE_VOCAL_MAX {
            return write_err(
                StatusCode::PAYLOAD_TOO_LARGE,
                "file_too_large",
                "5 Mo maximum pour un message vocal",
            );
        }
        if !VOCAUX.contains(&mime) {
            return write_err(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_voice",
                "format de message vocal non pris en charge",
            );
        }
    }
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let cle = nouvelle_cle();
    let _verrou = state.verrou_stockage().await;
    if let Err(r) = controle_quota(
        pool,
        state.config(),
        &u.id,
        donnees.len() as i64,
        LIMITE_HEURE,
        LIMITE_JOUR,
    )
    .await
    {
        return r;
    }
    let id = match creer_fichier(pool, &u.id, &meta, mime, donnees.len() as i64, &cle).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return write_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_pending_files",
                "20 fichiers en attente maximum, envoyez d'abord un message",
            );
        }
        Err(_) => return internal(),
    };
    let disque = Disque::new(&state.config().data_dir);
    if disque.put(&cle, &donnees).await.is_err() {
        let _ = supprimer_ligne(pool, &id).await;
        return internal();
    }
    let mut piece = json!({
        "id": id,
        "name": meta.nom,
        "mime": mime,
        "size": donnees.len(),
        "expired": false,
        "kind": meta.kind,
    });
    if meta.duree != 0 {
        piece["duration_ms"] = json!(meta.duree);
    }
    if !meta.onde.is_empty() {
        piece["waveform"] = json!(meta.onde);
    }
    if !meta.miniature.is_empty() {
        piece["thumb"] = json!(true);
    }
    write_json(StatusCode::CREATED, &piece)
}

pub async fn controle_quota(
    pool: &PgPool,
    config: &Config,
    envoyeur: &str,
    entrant: i64,
    limite_heure: i64,
    limite_jour: i64,
) -> Result<(), Response> {
    let total = db::octets_stockes(pool).await.map_err(|_| internal())?;
    if total + entrant > config.max_storage_bytes {
        return Err(write_err(
            StatusCode::INSUFFICIENT_STORAGE,
            "storage_full",
            "stockage plein, réessayez plus tard ou prévenez un administrateur",
        ));
    }
    let (stocke, heure, jour): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(sum(size) FILTER (WHERE purged_at IS NULL), 0)::bigint,
            COALESCE(sum(size) FILTER (WHERE created_at > now() - interval '1 hour'), 0)::bigint,
            COALESCE(sum(size) FILTER (WHERE created_at > now() - interval '1 day'), 0)::bigint
         FROM files WHERE uploader_id=$1::uuid",
    )
    .bind(envoyeur)
    .fetch_one(pool)
    .await
    .map_err(|_| internal())?;
    if stocke + entrant > config.max_user_storage_bytes {
        Err(write_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "quota_exceeded",
            "quota de stockage atteint : supprimez des messages avec fichiers pour libérer de la place",
        ))
    } else if heure + entrant > limite_heure || jour + entrant > limite_jour {
        Err(write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "upload_rate_limited",
            "trop de fichiers envoyés récemment, réessayez plus tard",
        ))
    } else {
        Ok(())
    }
}

async fn creer_fichier(
    pool: &PgPool,
    envoyeur: &str,
    meta: &Meta,
    mime: &str,
    taille: i64,
    cle: &str,
) -> Result<Option<String>, sqlx::Error> {
    let en_attente: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM files WHERE uploader_id=$1::uuid AND message_id IS NULL AND report_id IS NULL",
    )
    .bind(envoyeur)
    .fetch_one(pool)
    .await?;
    if en_attente >= FICHIERS_EN_ATTENTE_MAX {
        return Ok(None);
    }
    let miniature = (!meta.miniature.is_empty()).then_some(meta.miniature.as_slice());
    sqlx::query_scalar(
        "INSERT INTO files (uploader_id, name, mime, size, key, kind, duration_ms, waveform, thumb) VALUES ($1::uuid,$2,$3,$4,$5,$6,$7,$8::integer[],$9) RETURNING id::text",
    )
    .bind(envoyeur)
    .bind(&meta.nom)
    .bind(mime)
    .bind(taille)
    .bind(cle)
    .bind(meta.kind)
    .bind(meta.duree)
    .bind(&meta.onde)
    .bind(miniature)
    .fetch_one(pool)
    .await
    .map(Some)
}

async fn supprimer_ligne(pool: &PgPool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM files WHERE id=$1::uuid AND message_id IS NULL AND report_id IS NULL")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
}

fn lire_fichier(l: &sqlx::postgres::PgRow) -> Fichier {
    Fichier {
        id: l.get(0),
        name: l.get(1),
        mime: l.get(2),
        key: l.get(3),
        expire: l.get(4),
    }
}

const COLONNES: &str = "id::text, name, mime, key, purged_at IS NOT NULL";

async fn fichier_par_id(pool: &PgPool, id: &str) -> Result<Fichier, ErreurAcces> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES} FROM files WHERE id=$1::uuid"
    )))
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(lire_fichier(&l))
}

async fn fichier_signalement(pool: &PgPool, id: &str) -> Result<Fichier, ErreurAcces> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES} FROM files WHERE id=$1::uuid AND report_id IS NOT NULL"
    )))
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(lire_fichier(&l))
}

async fn fichier_pour(pool: &PgPool, id: &str, user: &str) -> Result<Fichier, ErreurAcces> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES}, message_id, uploader_id::text FROM files WHERE id=$1::uuid"
    )))
    .bind(id)
    .fetch_one(pool)
    .await?;
    let (message, envoyeur): (Option<i64>, String) = (l.get(5), l.get(6));
    let visible = match message {
        None => envoyeur == user,
        Some(m) => salons::voit_message(pool, m, user).await?,
    };
    if visible {
        Ok(lire_fichier(&l))
    } else {
        Err(ErreurAcces::NonTrouve)
    }
}

pub async fn est_admin(state: &AppState, sub: &str) -> bool {
    state.droits(sub).await.is_ok_and(|d| {
        crate::catalogue::CATALOGUE
            .iter()
            .any(|e| e.admin() && d.a(&e.nom()))
    })
}

pub async fn a_droit(state: &AppState, sub: &str, nom: &str) -> bool {
    state.droit(sub, nom).await.unwrap_or(false)
}

async fn acces(state: &AppState, id: &str, parts: &mut Parts) -> Result<Fichier, Response> {
    let pool = state.pool().map_err(IntoResponse::into_response)?;
    let jeton = parametre(parts.uri.query(), "t");
    if !jeton.is_empty() {
        if !est_uuid(id) || !lien_valide(state.cle_liens(), id, &jeton, maintenant()) {
            return Err(write_err(
                StatusCode::FORBIDDEN,
                "invalid_link",
                "lien expiré ou invalide",
            ));
        }
        return fichier_par_id(pool, id)
            .await
            .map_err(IntoResponse::into_response);
    }
    let ActiveUser(u) = ActiveUser::from_request_parts(parts, state).await?;
    if !est_uuid(id) {
        return Err(non_trouve());
    }
    match fichier_pour(pool, id, &u.id).await {
        Err(ErreurAcces::NonTrouve)
            if a_droit(state, &u.sub, crate::catalogue::ADMIN_FICHIERS).await =>
        {
            fichier_signalement(pool, id).await
        }
        r => r,
    }
    .map_err(IntoResponse::into_response)
}

fn entetes_securite(h: &mut HeaderMap, parts: &Parts) {
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    let corp = if parametre(parts.uri.query(), "t").is_empty() {
        "same-site"
    } else {
        "cross-origin"
    };
    h.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static(corp),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
}

pub enum Plage {
    Aucune,
    Une(u64, u64),
    NonSatisfaisante,
    Invalide,
}

pub fn analyser_plage(entete: &str, taille: u64) -> Plage {
    if entete.is_empty() {
        return Plage::Aucune;
    }
    let Some(liste) = entete.strip_prefix("bytes=") else {
        return Plage::Invalide;
    };
    let blanc = |c: char| matches!(c, ' ' | '\t');
    let mut plages: Vec<(u64, u64)> = vec![];
    let mut hors = false;
    for ra in liste.split(',') {
        let ra = ra.trim_matches(blanc);
        if ra.is_empty() {
            continue;
        }
        let Some((debut, fin)) = ra.split_once('-') else {
            return Plage::Invalide;
        };
        let (debut, fin) = (debut.trim_matches(blanc), fin.trim_matches(blanc));
        if debut.is_empty() {
            if fin.is_empty() || fin.starts_with('-') {
                return Plage::Invalide;
            }
            let Ok(n) = fin.parse::<i64>() else {
                return Plage::Invalide;
            };
            if n < 0 {
                return Plage::Invalide;
            }
            let n = (n as u64).min(taille);
            plages.push((taille - n, n));
        } else {
            let Ok(d) = debut.parse::<i64>() else {
                return Plage::Invalide;
            };
            if d < 0 {
                return Plage::Invalide;
            }
            let d = d as u64;
            if d >= taille {
                hors = true;
                continue;
            }
            if fin.is_empty() {
                plages.push((d, taille - d));
            } else {
                let Ok(f) = fin.parse::<i64>() else {
                    return Plage::Invalide;
                };
                if f < 0 || d > f as u64 {
                    return Plage::Invalide;
                }
                plages.push((d, (f as u64).min(taille - 1) - d + 1));
            }
        }
    }
    if hors && plages.is_empty() {
        return Plage::NonSatisfaisante;
    }
    match plages.as_slice() {
        [(d, l)] if *l <= taille => Plage::Une(*d, *l),
        _ => Plage::Aucune,
    }
}

#[utoipa::path(get, path = "/files/{id}", tag = "fichiers",
    summary = "Télécharger un fichier (participants de la conversation, ou expéditeur tant qu'il n'est pas joint)",
    params(
        ("id" = String, Path, description = "Identifiant du fichier", format = Uuid),
        ("t" = Option<String>, Query, description = "Jeton de `POST /files/{id}/link`"),
        ("download" = Option<String>, Query, description = "`1` force le téléchargement"),
    ),
    responses(
        (status = 200, description = "Contenu", content_type = "*/*", body = Vec<u8>),
        (status = 206, description = "Contenu partiel (en-tête Range)"),
        (status = 403, description = "invalid_link : lien signé expiré ou altéré"),
        (status = 404, description = "Introuvable, ou non visible par l'utilisateur", body = crate::openapi::Error),
        (status = 410, description = "file_expired : contenu supprimé du serveur"),
    ))]
async fn telecharger(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Response {
    let (mut parts, _) = req.into_parts();
    let f = match acces(&state, &id, &mut parts).await {
        Ok(f) => f,
        Err(r) => return r,
    };
    if f.expire {
        return write_err(
            StatusCode::GONE,
            "file_expired",
            "fichier supprimé du serveur",
        );
    }
    let mut fichier = match Disque::new(&state.config().data_dir).ouvrir(&f.key).await {
        Ok(fichier) => fichier,
        Err(ErreurBlob::Introuvable) => {
            return write_err(
                StatusCode::GONE,
                "file_expired",
                "fichier introuvable sur le serveur",
            );
        }
        Err(_) => return internal(),
    };
    let Ok(meta) = fichier.metadata().await else {
        return internal();
    };
    let taille = meta.len();
    let (ct, disposition) = if AFFICHABLES.contains(&f.mime.as_str())
        && parametre(parts.uri.query(), "download") != "1"
    {
        (f.mime.as_str(), "inline")
    } else {
        ("application/octet-stream", "attachment")
    };
    let mut h = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(ct) {
        h.insert(header::CONTENT_TYPE, v);
    }
    let cd = format!(
        "{disposition}; filename*=UTF-8''{}; filename=\"{}\"",
        echappe_chemin(&f.name),
        nom_ascii(&f.name)
    );
    if let Ok(v) = HeaderValue::from_str(&cd) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    entetes_securite(&mut h, &parts);
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let demande = parts
        .headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let plage = if parts.headers.contains_key(header::IF_RANGE) {
        Plage::Aucune
    } else {
        analyser_plage(demande, taille)
    };
    let (statut, debut, longueur) = match plage {
        Plage::Invalide | Plage::NonSatisfaisante => {
            let invalide = matches!(plage, Plage::Invalide);
            let mut r = crate::http::text_error(
                StatusCode::RANGE_NOT_SATISFIABLE,
                if invalide {
                    "invalid range"
                } else {
                    "invalid range: failed to overlap"
                },
            );
            if !invalide && let Ok(v) = HeaderValue::from_str(&format!("bytes */{taille}")) {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            for (k, v) in h {
                if let Some(k) = k
                    && k != header::CONTENT_TYPE
                    && k != header::ACCEPT_RANGES
                    && k != header::CACHE_CONTROL
                {
                    r.headers_mut().insert(k, v);
                }
            }
            return r;
        }
        Plage::Une(d, l) => {
            if let Ok(v) = HeaderValue::from_str(&format!("bytes {d}-{}/{taille}", d + l - 1)) {
                h.insert(header::CONTENT_RANGE, v);
            }
            (StatusCode::PARTIAL_CONTENT, d, l)
        }
        Plage::Aucune => (StatusCode::OK, 0, taille),
    };
    if debut > 0 && fichier.seek(SeekFrom::Start(debut)).await.is_err() {
        return internal();
    }
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(longueur));
    let corps = Body::from_stream(ReaderStream::new(fichier.take(longueur)));
    (statut, h, corps).into_response()
}

#[utoipa::path(get, path = "/files/{id}/thumb", tag = "fichiers",
    summary = "Miniature JPEG d'une vidéo (mêmes droits que le fichier ; `?t=` accepté)",
    params(
        ("id" = String, Path, description = "Identifiant du fichier", format = Uuid),
        ("t" = Option<String>, Query, description = "Jeton de `POST /files/{id}/link`"),
    ),
    responses(
        (status = 200, description = "Image JPEG", content_type = "image/jpeg", body = Vec<u8>),
        (status = 404, description = "Introuvable, ou non visible par l'utilisateur", body = crate::openapi::Error),
    ))]
async fn miniature(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Response {
    let (mut parts, _) = req.into_parts();
    let f = match acces(&state, &id, &mut parts).await {
        Ok(f) => f,
        Err(r) => return r,
    };
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let octets: Result<Option<Vec<u8>>, _> =
        sqlx::query_scalar("SELECT thumb FROM files WHERE id=$1::uuid")
            .bind(&f.id)
            .fetch_one(pool)
            .await;
    match octets {
        Ok(Some(b)) if !b.is_empty() => {
            let mut h = HeaderMap::new();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
            entetes_securite(&mut h, &parts);
            (StatusCode::OK, h, b).into_response()
        }
        Ok(_) | Err(sqlx::Error::RowNotFound) => non_trouve(),
        Err(_) => internal(),
    }
}

#[utoipa::path(post, path = "/files/{id}/link", tag = "fichiers",
    summary = "Jeton de lien signé (1 h) pour lire ou télécharger ce fichier sans en-tête d'authentification",
    params(("id" = String, Path, description = "Identifiant du fichier", format = Uuid)),
    responses(
        (status = 200, description = "ok", body = LienReponse),
        (status = 404, description = "Introuvable, ou non visible par l'utilisateur", body = crate::openapi::Error),
    ))]
async fn lien(State(state): State<AppState>, Path(id): Path<String>, req: Request) -> Response {
    let (mut parts, _) = req.into_parts();
    let ActiveUser(u) = match ActiveUser::from_request_parts(&mut parts, &state).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !est_uuid(&id) {
        return non_trouve();
    }
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = fichier_pour(pool, &id, &u.id).await {
        return e.into_response();
    }
    let jeton = signer(state.cle_liens(), &id, maintenant() + DUREE_LIEN);
    write_json(StatusCode::OK, &Value::from_iter([("token", jeton)]))
}

pub async fn purger(
    pool: &PgPool,
    disque: &Disque,
    age: Duration,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut n = 0;
    loop {
        let orphelins: Vec<(String, String)> = sqlx::query_as(
            "SELECT id::text, key FROM files WHERE message_id IS NULL AND report_id IS NULL AND created_at < now() - make_interval(secs => $1) ORDER BY created_at LIMIT 100",
        )
        .bind(age.as_secs_f64())
        .fetch_all(pool)
        .await?;
        if orphelins.is_empty() {
            return Ok(n);
        }
        for (id, cle) in orphelins {
            disque
                .supprimer(&cle)
                .await
                .map_err(|e| format!("{cle} : {e:?}"))?;
            supprimer_ligne(pool, &id).await?;
            n += 1;
        }
    }
}

pub fn lancer_purge(
    state: AppState,
    intervalle: Duration,
    age: Duration,
    mut arret: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tic =
            tokio::time::interval_at(tokio::time::Instant::now() + intervalle, intervalle);
        loop {
            tokio::select! {
                _ = arret.changed() => return,
                _ = tic.tick() => {}
            }
            let Ok(pool) = state.pool() else {
                continue;
            };
            let disque = Disque::new(&state.config().data_dir);
            match purger(pool, &disque, age).await {
                Err(e) => tracing::error!("purge des fichiers : {e}"),
                Ok(n) if n > 0 => tracing::info!("purge : {n} fichiers supprimés"),
                Ok(_) => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mp4::tests::mp4_valide,
        testutil::{self, BasePostgres, Faux, faux},
        users,
    };
    use axum::{Router, body::to_bytes};
    use tower::ServiceExt;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    const WEBM: &[u8] = b"\x1aE\xdf\xa3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

    struct Env {
        app: Router,
        state: AppState,
        _dossier: tempfile::TempDir,
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
        testutil::reponse_droits(r, |sub, p| match p {
            "estompe:access" => true,
            "estompe:admin" => sub == "sub-admin",
            _ => false,
        })
    }

    async fn env(extra: &[(&str, &str)], regler: impl FnOnce(&mut Config)) -> Option<Env> {
        let (pg, url) = testutil::postgres().await?;
        let g = faux(gateway).await;
        let p = faux(permission).await;
        let dossier = tempfile::tempdir().unwrap();
        let data = dossier.path().join("data");
        let mut vars = vec![
            ("GATEWAY_URL", g.url.as_str()),
            ("PERMISSION_URL", p.url.as_str()),
            ("DATA_DIR", data.to_str().unwrap()),
        ];
        vars.extend_from_slice(extra);
        let mut config = testutil::config_avec(&vars);
        regler(&mut config);
        let state = AppState::new(config, vec![]);
        let pool = PgPool::connect(&url).await.unwrap();
        db::migrate_embarquees(&pool).await.unwrap();
        state.set_pool(pool);
        Some(Env {
            app: Router::from(router()).with_state(state.clone()),
            state,
            _dossier: dossier,
            _g: g,
            _p: p,
            _pg: pg,
        })
    }

    impl Env {
        fn pool(&self) -> &PgPool {
            self.state.pool().ok().unwrap()
        }

        async fn user(&self, nom: &str) -> String {
            users::activate(self.pool(), &format!("sub-{nom}"), nom)
                .await
                .ok()
                .unwrap()
                .id
        }

        async fn requete(
            &self,
            methode: &str,
            chemin: &str,
            nom: Option<&str>,
            entetes: &[(&str, &str)],
            corps: Vec<u8>,
        ) -> (StatusCode, HeaderMap, Vec<u8>) {
            let mut r = Request::builder().method(methode).uri(chemin);
            if let Some(n) = nom {
                r = r.header(header::AUTHORIZATION, format!("Bearer {n}"));
            }
            for (k, v) in entetes {
                r = r.header(*k, *v);
            }
            let rep = self
                .app
                .clone()
                .oneshot(r.body(Body::from(corps)).unwrap())
                .await
                .unwrap();
            let (s, h) = (rep.status(), rep.headers().clone());
            (
                s,
                h,
                to_bytes(rep.into_body(), 64 << 20).await.unwrap().to_vec(),
            )
        }

        async fn envoi(
            &self,
            nom: &str,
            champs: &[(&str, &str)],
            fichier: Option<(&str, &[u8])>,
            miniature: Option<&[u8]>,
        ) -> (StatusCode, Value) {
            let (ct, corps) = multipart(champs, fichier, miniature);
            let (s, h, c) = self
                .requete("POST", "/files", Some(nom), &[("content-type", &ct)], corps)
                .await;
            assert_eq!(h[header::CONTENT_TYPE], "application/json");
            (s, serde_json::from_slice(&c).unwrap())
        }

        async fn envoi_ok(&self, nom: &str, fichier: &str, donnees: &[u8]) -> String {
            let (s, v) = self.envoi(nom, &[], Some((fichier, donnees)), None).await;
            assert_eq!(s, StatusCode::CREATED, "{v}");
            v["id"].as_str().unwrap().to_string()
        }

        async fn lire(&self, nom: Option<&str>, chemin: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
            self.requete("GET", chemin, nom, &[], vec![]).await
        }

        async fn sql(&self, requete: &str, args: &[&str]) -> String {
            let mut q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(requete));
            for a in args {
                q = q.bind(*a);
            }
            q.fetch_one(self.pool()).await.unwrap()
        }

        async fn serveur(&self, proprietaire: &str, membres: &[&str]) -> (String, String) {
            let serveur = self
                .sql(
                    "INSERT INTO servers (name, owner_id) VALUES ('s', $1::uuid) RETURNING id::text",
                    &[proprietaire],
                )
                .await;
            for m in std::iter::once(&proprietaire).chain(membres) {
                self.sql(
                    "INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid, $2::uuid) RETURNING user_id::text",
                    &[&serveur, m],
                )
                .await;
            }
            self.sql(
                "INSERT INTO roles (server_id, name, position, permissions, is_default) VALUES ($1::uuid, '@tout le monde', 0, 25677568, true) RETURNING id::text",
                &[&serveur],
            )
            .await;
            let salon = self
                .sql(
                    "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid, 'general', 'text') RETURNING id::text",
                    &[&serveur],
                )
                .await;
            (serveur, salon)
        }

        async fn joindre(&self, salon: &str, auteur: &str, fichier: &str) {
            let message = self
                .sql(
                    "INSERT INTO messages (channel_id, author_id, content) VALUES ($1::uuid, $2::uuid, 'x') RETURNING id::text",
                    &[salon, auteur],
                )
                .await;
            sqlx::query("UPDATE files SET message_id=$1::bigint WHERE id=$2::uuid")
                .bind(message)
                .bind(fichier)
                .execute(self.pool())
                .await
                .unwrap();
        }
    }

    fn multipart(
        champs: &[(&str, &str)],
        fichier: Option<(&str, &[u8])>,
        miniature: Option<&[u8]>,
    ) -> (String, Vec<u8>) {
        let limite = "----limite";
        let mut c = Vec::new();
        for (k, v) in champs {
            c.extend_from_slice(
                format!(
                    "--{limite}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n"
                )
                .as_bytes(),
            );
        }
        if let Some(m) = miniature {
            c.extend_from_slice(
                format!("--{limite}\r\nContent-Disposition: form-data; name=\"thumb\"; filename=\"t.jpg\"\r\n\r\n")
                    .as_bytes(),
            );
            c.extend_from_slice(m);
            c.extend_from_slice(b"\r\n");
        }
        if let Some((nom, donnees)) = fichier {
            c.extend_from_slice(
                format!("--{limite}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{nom}\"\r\n\r\n")
                    .as_bytes(),
            );
            c.extend_from_slice(donnees);
            c.extend_from_slice(b"\r\n");
        }
        c.extend_from_slice(format!("--{limite}--\r\n").as_bytes());
        (format!("multipart/form-data; boundary={limite}"), c)
    }

    fn texte(c: &[u8]) -> String {
        String::from_utf8_lossy(c).into_owned()
    }

    #[tokio::test]
    async fn cycle_de_vie_et_visibilite() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        let (alice, bob) = (e.user("alice").await, e.user("bob").await);
        e.user("carol").await;
        let (s, v) = e.envoi("alice", &[], Some(("photo.png", PNG)), None).await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(v["name"], "photo.png");
        assert_eq!(v["mime"], "image/png");
        assert_eq!(v["size"], PNG.len());
        assert_eq!(v["kind"], "file");
        assert_eq!(v["expired"], false);
        assert!(
            v.get("duration_ms").is_none()
                && v.get("waveform").is_none()
                && v.get("thumb").is_none()
        );
        let id = v["id"].as_str().unwrap().to_string();
        let chemin = format!("/files/{id}");

        let (s, h, c) = e.lire(Some("alice"), &chemin).await;
        assert_eq!((s, c.as_slice()), (StatusCode::OK, PNG));
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            h[header::CONTENT_DISPOSITION],
            "inline; filename*=UTF-8''photo.png; filename=\"photo.png\""
        );
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(
            h[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; sandbox"
        );
        assert_eq!(h["cross-origin-resource-policy"], "same-site");
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=3600");
        assert_eq!(h[header::ACCEPT_RANGES], "bytes");
        assert_eq!(h[header::CONTENT_LENGTH], PNG.len().to_string());
        assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::NOT_FOUND);
        assert_eq!(e.lire(None, &chemin).await.0, StatusCode::UNAUTHORIZED);

        let (serveur, salon) = e.serveur(&alice, &[&bob]).await;
        e.joindre(&salon, &alice, &id).await;
        assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::OK);
        assert_eq!(
            e.lire(Some("carol"), &chemin).await.0,
            StatusCode::NOT_FOUND
        );

        let defaut = e
            .sql(
                "SELECT id::text FROM roles WHERE server_id=$1::uuid AND is_default",
                &[&serveur],
            )
            .await;
        sqlx::query("INSERT INTO channel_overrides (channel_id, role_id, allow, deny) VALUES ($1::uuid, $2::uuid, 0, 256)")
            .bind(&salon)
            .bind(&defaut)
            .execute(e.pool())
            .await
            .unwrap();
        assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::NOT_FOUND);
        assert_eq!(e.lire(Some("alice"), &chemin).await.0, StatusCode::OK);
        sqlx::query("INSERT INTO channel_member_overrides (channel_id, user_id, allow, deny) VALUES ($1::uuid, $2::uuid, 256, 0)")
            .bind(&salon)
            .bind(&bob)
            .execute(e.pool())
            .await
            .unwrap();
        assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::OK);

        for mauvais in [
            "/files/00000000-0000-0000-0000-000000000000",
            "/files/pas-un-uuid",
        ] {
            assert_eq!(
                e.lire(Some("alice"), mauvais).await.0,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn messages_prives() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        let (a, b) = (e.user("alice").await, e.user("bob").await);
        e.user("carol").await;
        let (u1, u2) = if a < b { (&a, &b) } else { (&b, &a) };
        let dm = e
            .sql(
                "INSERT INTO dm_conversations (user_a, user_b) VALUES ($1::uuid, $2::uuid) RETURNING id::text",
                &[u1, u2],
            )
            .await;
        let id = e.envoi_ok("alice", "a.txt", b"bonjour").await;
        let message = e
            .sql(
                "INSERT INTO messages (dm_id, author_id, content) VALUES ($1::uuid, $2::uuid, 'x') RETURNING id::text",
                &[&dm, &a],
            )
            .await;
        sqlx::query("UPDATE files SET message_id=$1::bigint WHERE id=$2::uuid")
            .bind(message)
            .bind(&id)
            .execute(e.pool())
            .await
            .unwrap();
        let chemin = format!("/files/{id}");
        assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::OK);
        assert_eq!(
            e.lire(Some("carol"), &chemin).await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn liens_signes() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        e.user("bob").await;
        let id = e.envoi_ok("alice", "photo.png", PNG).await;
        let lien = format!("/files/{id}/link");
        let (s, _, _) = e.requete("POST", &lien, Some("bob"), &[], vec![]).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _, _) = e.requete("POST", &lien, None, &[], vec![]).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, h, c) = e.requete("POST", &lien, Some("alice"), &[], vec![]).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
        let jeton = serde_json::from_slice::<Value>(&c).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();

        let (s, h, c) = e.lire(None, &format!("/files/{id}?t={jeton}")).await;
        assert_eq!((s, c.as_slice()), (StatusCode::OK, PNG));
        assert_eq!(h["cross-origin-resource-policy"], "cross-origin");
        let (s, _, _) = e.lire(None, &format!("/files/{id}/thumb?t={jeton}")).await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        for mauvais in [
            format!("/files/{id}?t=abc"),
            format!("/files/{id}?t=9999999999.{}", "0".repeat(64)),
            format!("/files/00000000-0000-0000-0000-000000000000?t={jeton}"),
            format!("/files/pas-un-uuid?t={jeton}"),
        ] {
            let (s, _, c) = e.lire(None, &mauvais).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "{mauvais}");
            assert_eq!(
                serde_json::from_slice::<Value>(&c).unwrap()["error"],
                "invalid_link"
            );
        }
        let perime = signer(e.state.cle_liens(), &id, maintenant() - 1);
        assert_eq!(
            e.lire(None, &format!("/files/{id}?t={perime}")).await.0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn lecture_par_plages() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        let id = e.envoi_ok("alice", "a.txt", b"0123456789").await;
        let chemin = format!("/files/{id}");
        let avec = |range: &'static str| {
            let e = &e;
            let chemin = &chemin;
            async move {
                e.requete("GET", chemin, Some("alice"), &[("range", range)], vec![])
                    .await
            }
        };
        let (s, h, c) = avec("bytes=2-5").await;
        assert_eq!(
            (s, texte(&c).as_str()),
            (StatusCode::PARTIAL_CONTENT, "2345")
        );
        assert_eq!(h[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(h[header::CONTENT_LENGTH], "4");
        assert_eq!(h[header::CONTENT_TYPE], "application/octet-stream");
        let (s, h, c) = avec("bytes=7-").await;
        assert_eq!(
            (s, texte(&c).as_str()),
            (StatusCode::PARTIAL_CONTENT, "789")
        );
        assert_eq!(h[header::CONTENT_RANGE], "bytes 7-9/10");
        let (_, h, c) = avec("bytes=-3").await;
        assert_eq!(
            (
                texte(&c).as_str(),
                h[header::CONTENT_RANGE].to_str().unwrap()
            ),
            ("789", "bytes 7-9/10")
        );
        let (_, _, c) = avec("bytes=8-100").await;
        assert_eq!(texte(&c), "89");
        let (s, h, _) = avec("bytes=50-60").await;
        assert_eq!(s, StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(h[header::CONTENT_RANGE], "bytes */10");
        assert!(h.get(header::ACCEPT_RANGES).is_none() && h.get(header::CACHE_CONTROL).is_none());
        let (s, h, c) = avec("octets=1-2").await;
        assert_eq!(
            (s, texte(&c).as_str()),
            (StatusCode::RANGE_NOT_SATISFIABLE, "invalid range\n")
        );
        assert!(h.get(header::CONTENT_RANGE).is_none());
        let (s, _, c) = e
            .requete(
                "GET",
                &chemin,
                Some("alice"),
                &[("range", "bytes=2-5"), ("if-range", "\"x\"")],
                vec![],
            )
            .await;
        assert_eq!((s, c.len()), (StatusCode::OK, 10));
        let (s, h, c) = e.requete("HEAD", &chemin, Some("alice"), &[], vec![]).await;
        assert_eq!((s, c.len()), (StatusCode::OK, 0));
        assert_eq!(h[header::CONTENT_LENGTH], "10");
    }

    #[tokio::test]
    async fn controles_de_type() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        let (s, v) = e
            .envoi(
                "alice",
                &[],
                Some(("faux.png", b"<script>alert(1)</script>")),
                None,
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, Some("type_mismatch"))
        );
        let (s, _) = e.envoi("alice", &[], Some(("vrai.jpg", PNG)), None).await;
        assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let (s, _) = e.envoi("alice", &[], Some(("MAJ.PNG", PNG)), None).await;
        assert_eq!(s, StatusCode::CREATED);

        let page = e
            .envoi_ok(
                "alice",
                "page.html",
                b"<html><script>alert(1)</script></html>",
            )
            .await;
        let (_, h, _) = e.lire(Some("alice"), &format!("/files/{page}")).await;
        assert_eq!(h[header::CONTENT_TYPE], "application/octet-stream");
        assert!(
            h[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .starts_with("attachment")
        );
        assert!(
            h[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("sandbox")
        );

        let (s, v) = e
            .envoi("alice", &[], Some(("../../etc/pass\\wd", b"x")), None)
            .await;
        assert_eq!((s, v["name"].as_str()), (StatusCode::CREATED, Some("wd")));

        let (s, v) = e.envoi("alice", &[], Some(("vide.txt", b"")), None).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("bad_request"))
        );
        let (s, v) = e.envoi("alice", &[("kind", "voice")], None, None).await;
        assert_eq!(
            (s, v["message"].as_str()),
            (StatusCode::BAD_REQUEST, Some("champ « file » manquant"))
        );
        let (s, h, _) = e
            .requete(
                "POST",
                "/files",
                Some("alice"),
                &[("content-type", "text/plain")],
                b"x".to_vec(),
            )
            .await;
        assert_eq!(
            (s, h[header::CONTENT_TYPE].to_str().unwrap()),
            (StatusCode::BAD_REQUEST, "application/json")
        );
    }

    #[tokio::test]
    async fn messages_vocaux() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        let vocal = |duree: &'static str, onde: &'static str| {
            [
                ("kind", "voice"),
                ("duration_ms", duree),
                ("waveform", onde),
            ]
        };
        let (s, v) = e
            .envoi(
                "alice",
                &vocal("12000", "0,40,100"),
                Some(("m.webm", WEBM)),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        assert_eq!(v["kind"], "voice");
        assert_eq!(v["mime"], "video/webm");
        assert_eq!(v["duration_ms"], 12000);
        assert_eq!(v["waveform"], json!([0, 40, 100]));
        for (duree, onde, code) in [
            ("0", "1", 400),
            ("300001", "1", 400),
            ("abc", "1", 400),
            ("1000", "101", 400),
            ("1000", "-1", 400),
            ("1000", "", 400),
            ("1000", "1,x", 400),
        ] {
            let (s, v) = e
                .envoi("alice", &vocal(duree, onde), Some(("m.webm", WEBM)), None)
                .await;
            assert_eq!(
                (s.as_u16(), v["error"].as_str()),
                (code, Some("invalid_voice")),
                "{duree} {onde}"
            );
        }
        let onde = vec!["1"; 101].join(",");
        let (s, _) = e
            .envoi(
                "alice",
                &[
                    ("kind", "voice"),
                    ("duration_ms", "1000"),
                    ("waveform", &onde),
                ],
                Some(("m.webm", WEBM)),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, v) = e
            .envoi(
                "alice",
                &vocal("1000", "1"),
                Some(("m.webm", b"<script>")),
                None,
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Some("unsupported_voice")
            )
        );
        let gros = [WEBM, &vec![0; TAILLE_VOCAL_MAX]].concat();
        let (s, v) = e
            .envoi("alice", &vocal("1000", "1"), Some(("m.webm", &gros)), None)
            .await;
        assert_eq!(
            (s, v["message"].as_str()),
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                Some("5 Mo maximum pour un message vocal")
            )
        );
    }

    #[tokio::test]
    async fn limites_de_taille() {
        let Some(e) = env(&[("MAX_VIDEO_SIZE_MB", "11")], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        let juste = vec![b'a'; TAILLE_MAX as usize];
        let (s, _) = e.envoi("alice", &[], Some(("a.txt", &juste)), None).await;
        assert_eq!(s, StatusCode::CREATED);
        let trop = vec![b'a'; TAILLE_MAX as usize + 1];
        let (s, v) = e.envoi("alice", &[], Some(("b.txt", &trop)), None).await;
        assert_eq!(
            (s, v["error"].as_str(), v["message"].as_str()),
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                Some("file_too_large"),
                Some("10 Mo maximum")
            )
        );
        let video = vec![0u8; (11 << 20) + 1];
        let (s, v) = e
            .envoi("alice", &[("kind", "video")], Some(("v.mp4", &video)), None)
            .await;
        assert_eq!(
            (s, v["message"].as_str()),
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                Some("11 Mo maximum pour une vidéo")
            )
        );
        assert!(e.state.reserver_envoi_video(ENVOIS_VIDEO_MAX));
        e.state.liberer_envoi_video();
    }

    #[tokio::test]
    async fn videos_et_miniatures() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        let mp4 = mp4_valide(&[b"avc1", b"mp4a"], 1000, 4500);
        let jpeg = b"\xFF\xD8\xFF\xE0miniature";
        let (s, v) = e
            .envoi(
                "alice",
                &[("kind", "video")],
                Some(("clip.mp4", &mp4)),
                Some(jpeg),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        assert_eq!(
            (
                v["kind"].as_str(),
                v["duration_ms"].as_i64(),
                v["thumb"].as_bool()
            ),
            (Some("video"), Some(4500), Some(true))
        );
        let id = v["id"].as_str().unwrap();
        let (s, h, c) = e.lire(Some("alice"), &format!("/files/{id}/thumb")).await;
        assert_eq!((s, c.as_slice()), (StatusCode::OK, &jpeg[..]));
        assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=3600");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(
            e.lire(Some("alice"), &format!("/files/{id}/thumb?t=x"))
                .await
                .0,
            StatusCode::FORBIDDEN
        );

        let (s, v) = e
            .envoi(
                "alice",
                &[("kind", "video")],
                Some(("clip.mp4", b"pas une video")),
                None,
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Some("unsupported_video")
            )
        );
        for mauvaise in [&b"GIF89a-pas-jpeg"[..], &vec![0xFF; MINIATURE_MAX + 1]] {
            let mut m = b"\xFF\xD8\xFF".to_vec();
            m.extend_from_slice(if mauvaise.len() > 100 { mauvaise } else { b"" });
            let miniature = if mauvaise.len() > 100 {
                m.as_slice()
            } else {
                mauvaise
            };
            let (s, v) = e
                .envoi(
                    "alice",
                    &[("kind", "video")],
                    Some(("clip.mp4", &mp4)),
                    Some(miniature),
                )
                .await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_thumb"))
            );
        }
        let img = e.envoi_ok("alice", "photo.png", PNG).await;
        assert_eq!(
            e.lire(Some("alice"), &format!("/files/{img}/thumb"))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn envois_de_videos_simultanes() {
        let e = AppState::new(testutil::config(), vec![]);
        assert!((0..ENVOIS_VIDEO_MAX).all(|_| e.reserver_envoi_video(ENVOIS_VIDEO_MAX)));
        assert!(!e.reserver_envoi_video(ENVOIS_VIDEO_MAX));
        e.liberer_envoi_video();
        assert!(e.reserver_envoi_video(ENVOIS_VIDEO_MAX));
    }

    #[tokio::test]
    async fn quotas_de_stockage() {
        let Some(e) = env(&[], |c| {
            c.max_storage_bytes = 40;
            c.max_user_storage_bytes = 30;
        })
        .await
        else {
            return;
        };
        e.user("alice").await;
        e.user("bob").await;
        let (s, v) = e
            .envoi("alice", &[], Some(("a.txt", &[b'a'; 20])), None)
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        let (s, v) = e
            .envoi("alice", &[], Some(("b.txt", &[b'a'; 11])), None)
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::PAYLOAD_TOO_LARGE, Some("quota_exceeded"))
        );
        let (s, _) = e
            .envoi("bob", &[], Some(("c.txt", &[b'a'; 20])), None)
            .await;
        assert_eq!(s, StatusCode::CREATED);
        let (s, v) = e.envoi("bob", &[], Some(("d.txt", &[b'a'; 1])), None).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::INSUFFICIENT_STORAGE, Some("storage_full"))
        );
        let restants = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM files")
            .fetch_one(e.pool())
            .await
            .unwrap();
        assert_eq!(restants, 2);
    }

    #[tokio::test]
    async fn debit_par_heure_et_par_jour() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        let alice = e.user("alice").await;
        e.envoi_ok("alice", "a.txt", &[b'a'; 20]).await;
        let config = e.state.config();
        let code = |r: Result<(), Response>| r.err().map(|r| r.status());
        assert_eq!(
            code(controle_quota(e.pool(), config, &alice, 5, 25, 1000).await),
            None
        );
        assert_eq!(
            code(controle_quota(e.pool(), config, &alice, 6, 25, 1000).await),
            Some(StatusCode::TOO_MANY_REQUESTS)
        );
        assert_eq!(
            code(controle_quota(e.pool(), config, &alice, 6, 1000, 25).await),
            Some(StatusCode::TOO_MANY_REQUESTS)
        );
        assert_eq!(
            code(controle_quota(e.pool(), config, &alice, 6, 1000, 26).await),
            None
        );
    }

    #[tokio::test]
    async fn fichiers_en_attente_et_purge() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        let alice = e.user("alice").await;
        let mut ids = vec![];
        for i in 0..FICHIERS_EN_ATTENTE_MAX {
            ids.push(e.envoi_ok("alice", &format!("{i}.txt"), b"x").await);
        }
        let (s, v) = e
            .envoi("alice", &[], Some(("de-trop.txt", b"x")), None)
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (
                StatusCode::TOO_MANY_REQUESTS,
                Some("too_many_pending_files")
            )
        );
        let (_, salon) = e.serveur(&alice, &[]).await;
        e.joindre(&salon, &alice, &ids[0]).await;
        let signalement = e
            .sql("INSERT INTO reports (author_id, title, description) VALUES ($1::uuid, 't', 'd') RETURNING id::text", &[&alice])
            .await;
        sqlx::query("UPDATE files SET report_id=$1::uuid WHERE id=$2::uuid")
            .bind(&signalement)
            .bind(&ids[1])
            .execute(e.pool())
            .await
            .unwrap();
        let disque = Disque::new(&e.state.config().data_dir);
        assert_eq!(purger(e.pool(), &disque, AGE_ORPHELINS).await.unwrap(), 0);
        let cle: String = e
            .sql("SELECT key FROM files WHERE id=$1::uuid", &[&ids[5]])
            .await;
        disque.ouvrir(&cle).await.unwrap();
        assert_eq!(
            purger(e.pool(), &disque, Duration::ZERO).await.unwrap(),
            FICHIERS_EN_ATTENTE_MAX as usize - 2
        );
        assert!(matches!(
            disque.ouvrir(&cle).await,
            Err(ErreurBlob::Introuvable)
        ));
        let restants: Vec<String> =
            sqlx::query_scalar("SELECT id::text FROM files ORDER BY position, id")
                .fetch_all(e.pool())
                .await
                .unwrap();
        assert_eq!(restants.len(), 2);
        assert!(restants.contains(&ids[0]) && restants.contains(&ids[1]));
        assert_eq!(purger(e.pool(), &disque, Duration::ZERO).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn purge_periodique_et_arret_propre() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        e.envoi_ok("alice", "a.txt", b"x").await;
        let (stop, recu) = tokio::sync::watch::channel(false);
        let tache = lancer_purge(
            e.state.clone(),
            Duration::from_millis(20),
            Duration::ZERO,
            recu,
        );
        let limite = tokio::time::Instant::now() + Duration::from_secs(10);
        while sqlx::query_scalar::<_, i64>("SELECT count(*) FROM files")
            .fetch_one(e.pool())
            .await
            .unwrap()
            > 0
        {
            assert!(tokio::time::Instant::now() < limite, "purge non exécutée");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        stop.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), tache)
            .await
            .expect("arrêt propre")
            .unwrap();
    }

    #[tokio::test]
    async fn fichier_supprime_ou_absent() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        e.user("alice").await;
        let id = e.envoi_ok("alice", "a.txt", b"x").await;
        let chemin = format!("/files/{id}");
        let cle = e
            .sql("SELECT key FROM files WHERE id=$1::uuid", &[&id])
            .await;
        Disque::new(&e.state.config().data_dir)
            .supprimer(&cle)
            .await
            .unwrap();
        let (s, _, c) = e.lire(Some("alice"), &chemin).await;
        assert_eq!(s, StatusCode::GONE);
        assert!(texte(&c).contains("fichier introuvable sur le serveur"));
        sqlx::query("UPDATE files SET purged_at=now() WHERE id=$1::uuid")
            .bind(&id)
            .execute(e.pool())
            .await
            .unwrap();
        let (s, _, c) = e.lire(Some("alice"), &chemin).await;
        assert_eq!(s, StatusCode::GONE);
        assert!(texte(&c).contains("fichier supprimé du serveur"));
    }

    #[tokio::test]
    async fn capture_de_signalement_pour_les_admins() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        let alice = e.user("alice").await;
        e.user("admin").await;
        let id = e.envoi_ok("alice", "a.txt", b"capture").await;
        let chemin = format!("/files/{id}");
        assert_eq!(
            e.lire(Some("admin"), &chemin).await.0,
            StatusCode::NOT_FOUND
        );
        let signalement = e
            .sql("INSERT INTO reports (author_id, title, description) VALUES ($1::uuid, 't', 'd') RETURNING id::text", &[&alice])
            .await;
        sqlx::query("UPDATE files SET report_id=$1::uuid WHERE id=$2::uuid")
            .bind(&signalement)
            .bind(&id)
            .execute(e.pool())
            .await
            .unwrap();
        e.user("bob").await;
        assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::NOT_FOUND);
        let (s, _, c) = e.lire(Some("admin"), &chemin).await;
        assert_eq!((s, texte(&c).as_str()), (StatusCode::OK, "capture"));
    }

    #[test]
    fn noms_nettoyes() {
        for (entree, attendu) in [
            ("photo.png", "photo.png"),
            ("../../etc/passwd", "passwd"),
            ("C:\\dossier\\fichier.txt", "fichier.txt"),
            ("a/b/", "b"),
            ("a\nb\u{7f}c", "abc"),
            ("", "fichier"),
            ("/", "fichier"),
            ("///", "fichier"),
            (".", "fichier"),
            ("..", ".."),
            ("été.txt", "été.txt"),
        ] {
            assert_eq!(nom_propre(entree), attendu, "{entree:?}");
        }
        let long = format!("{}fin", "a".repeat(200));
        let propre = nom_propre(&long);
        assert_eq!(propre.chars().count(), 100);
        assert!(propre.ends_with("fin"));
        assert_eq!(nom_ascii("été \"q\" \\ ok"), "_t_ _q_ _ ok");
        assert_eq!(
            echappe_chemin("été (1) a+b.txt"),
            "%C3%A9t%C3%A9%20%281%29%20a+b.txt"
        );
        assert_eq!(extension("a.PNG"), ".png");
        assert_eq!(extension("sans"), "");
        assert_eq!(extension("a.b.jpeg"), ".jpeg");
    }

    #[test]
    fn plages_comme_le_go() {
        use Plage::*;
        let lire = |h: &str| match analyser_plage(h, 10) {
            Aucune => "aucune".to_string(),
            Une(d, l) => format!("{d}+{l}"),
            NonSatisfaisante => "hors".into(),
            Invalide => "invalide".into(),
        };
        for (entete, attendu) in [
            ("", "aucune"),
            ("bytes=0-0", "0+1"),
            ("bytes=0-", "0+10"),
            ("bytes=0-99", "0+10"),
            ("bytes=-4", "6+4"),
            ("bytes=-99", "0+10"),
            ("bytes= 2 - 3 ", "2+2"),
            ("bytes=10-", "hors"),
            ("bytes=10-12", "hors"),
            ("bytes=5-2", "invalide"),
            ("bytes=a-b", "invalide"),
            ("bytes=-", "invalide"),
            ("bytes=--1", "invalide"),
            ("bytes=3", "invalide"),
            ("items=0-1", "invalide"),
            ("bytes=0-1,4-5", "aucune"),
            ("bytes=0-1,20-30", "0+2"),
            ("bytes=", "aucune"),
            ("bytes=,", "aucune"),
        ] {
            assert_eq!(lire(entete), attendu, "{entete:?}");
        }
    }

    fn cle_aleatoire() -> [u8; 32] {
        use std::hash::BuildHasher;
        let graine = std::collections::hash_map::RandomState::new();
        std::array::from_fn(|i| graine.hash_one(i) as u8)
    }

    #[test]
    fn liens_et_identifiants() {
        let cle = cle_aleatoire();
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let jeton = signer(&cle, id, 2000);
        assert!(lien_valide(&cle, id, &jeton, 1999));
        assert!(!lien_valide(&cle, id, &jeton, 2000));
        assert!(!lien_valide(&cle, "autre", &jeton, 1999));
        assert!(!lien_valide(&cle_aleatoire(), id, &jeton, 1999));
        for mauvais in ["", "x", "2000.", "+2000.ab", "-1.ab", &format!("{jeton}0")] {
            assert!(!lien_valide(&cle, id, mauvais, 1999), "{mauvais}");
        }
        assert!(est_uuid(id) && est_uuid(&id.to_uppercase()));
        for mauvais in [
            "",
            "pas-un-uuid",
            &id[1..],
            &format!("{id}0"),
            &id.replace('-', "_"),
        ] {
            assert!(!est_uuid(mauvais), "{mauvais}");
        }
        assert_eq!(parametre(Some("a=1&t=x%2By+z&t=2"), "t"), "x+y z");
        assert_eq!(parametre(Some("download=1"), "t"), "");
        assert_eq!(parametre(None, "t"), "");
        assert_eq!(parametre(Some("t"), "t"), "");
    }

    #[tokio::test]
    async fn importer_fichier_cree_un_fichier_en_attente() {
        let Some(e) = env(&[], |_| {}).await else {
            return;
        };
        let u = users::activate(e.pool(), "sub-alice", "alice")
            .await
            .ok()
            .unwrap();
        let rep = importer_fichier(
            &e.state,
            &u,
            "gif.gif".into(),
            b"GIF89a\x01\0\x01\0\0\0\0;".to_vec(),
        )
        .await;
        assert_eq!(rep.status(), StatusCode::CREATED);
        let v: Value =
            serde_json::from_slice(&to_bytes(rep.into_body(), 1 << 20).await.unwrap()).unwrap();
        assert_eq!(v["mime"], "image/gif");
        assert_eq!(v["name"], "gif.gif");
        assert_eq!(v["kind"], "file");
        let rep = importer_fichier(&e.state, &u, "gif.gif".into(), b"pas un gif".to_vec()).await;
        assert_eq!(rep.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
