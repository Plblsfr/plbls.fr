use super::*;

pub(super) async fn supprimer_ligne(pool: &PgPool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM files WHERE id=$1::uuid AND message_id IS NULL AND report_id IS NULL")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
}

pub(super) fn lire_fichier(l: &sqlx::postgres::PgRow) -> Fichier {
    Fichier {
        id: l.get(0),
        name: l.get(1),
        mime: l.get(2),
        key: l.get(3),
        expire: l.get(4),
    }
}

pub(super) const COLONNES: &str = "id::text, name, mime, key, purged_at IS NOT NULL";

pub(super) async fn fichier_signalement(pool: &PgPool, id: &str) -> Result<Fichier, ErreurAcces> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES} FROM files WHERE id=$1::uuid AND report_id IS NOT NULL"
    )))
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(lire_fichier(&l))
}

pub(super) async fn fichier_pour(
    pool: &PgPool,
    id: &str,
    user: &str,
) -> Result<Fichier, ErreurAcces> {
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

pub(super) async fn acces(
    state: &AppState,
    id: &str,
    parts: &mut Parts,
) -> Result<Fichier, Response> {
    let pool = state.pool().map_err(IntoResponse::into_response)?;
    let jeton = parametre(parts.uri.query(), "t");
    if !jeton.is_empty() {
        let user = uuid_valide(id)
            .then(|| lien_valide(state.cle_liens(), id, &jeton, maintenant()))
            .flatten();
        let Some(user) = user else {
            return Err(write_err(
                StatusCode::FORBIDDEN,
                "invalid_link",
                "lien expiré ou invalide",
            ));
        };
        return fichier_pour(pool, id, &user)
            .await
            .map_err(IntoResponse::into_response);
    }
    let ActiveUser(u) = ActiveUser::from_request_parts(parts, state).await?;
    if !uuid_valide(id) {
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

pub(super) fn entetes_securite(h: &mut HeaderMap, parts: &Parts) {
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
