use super::*;

pub(super) struct GardeVideo(AppState);

impl Drop for GardeVideo {
    fn drop(&mut self) {
        self.0.liberer_envoi_video();
    }
}

pub(super) async fn lire_borne(
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

pub(super) fn limite_du_type(kind: &str, video_max: i64) -> (i64, String) {
    if kind == "video" {
        (
            video_max,
            format!("{} Mo maximum pour une vidéo", video_max >> 20),
        )
    } else {
        (TAILLE_MAX, "10 Mo maximum".to_string())
    }
}

pub(super) fn fichier_illisible() -> Response {
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
pub(super) async fn envoyer(State(state): State<AppState>, req: Request) -> Response {
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

pub(super) struct Meta {
    nom: String,
    kind: &'static str,
    duree: i32,
    onde: Vec<i32>,
    miniature: Vec<u8>,
}

pub(super) async fn enregistrer_envoi(
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

pub(super) async fn sauvegarder(
    state: &AppState,
    u: &User,
    meta: Meta,
    donnees: Vec<u8>,
) -> Response {
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

pub(super) async fn creer_fichier(
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
