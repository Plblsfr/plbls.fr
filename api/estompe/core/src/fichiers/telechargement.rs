use super::*;

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
            if n == 0 || taille == 0 {
                hors = true;
                continue;
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
pub(super) async fn telecharger(
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
pub(super) async fn miniature(
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
pub(super) async fn lien(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Response {
    let (mut parts, _) = req.into_parts();
    let ActiveUser(u) = match ActiveUser::from_request_parts(&mut parts, &state).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !uuid_valide(&id) {
        return non_trouve();
    }
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = fichier_pour(pool, &id, &u.id).await {
        return e.into_response();
    }
    let jeton = signer(state.cle_liens(), &id, &u.id, maintenant() + DUREE_LIEN);
    write_json(StatusCode::OK, &Value::from_iter([("token", jeton)]))
}
