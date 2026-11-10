use super::*;

pub(super) async fn salon_vocal(
    state: &AppState,
    cid: &str,
    user: &str,
) -> Result<Salon, Response> {
    let servers::SalonVisible {
        id,
        serveur,
        kind,
        limite,
        perms,
    } = servers::salon_visible(state, cid, user).await?;
    if !servers::est_vocal(&kind) {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "not_voice_channel",
            "ce salon n'est pas un salon vocal",
        ));
    }
    Ok(Salon {
        id,
        serveur,
        limite,
        perms,
    })
}

pub(super) async fn serveur_acces(
    state: &AppState,
    id: &str,
    user: &str,
    besoin: Perm,
) -> Result<(String, Access), Response> {
    let pool = state.pool().map_err(IntoResponse::into_response)?;
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
    let acces = state
        .acces_serveur(id, user)
        .await
        .map_err(bd)?
        .ok_or_else(internal)?;
    if !acces.can(besoin) {
        return Err(interdit());
    }
    Ok((proprietaire, acces))
}

pub(super) async fn salons_vocaux(
    pool: &PgPool,
    serveur: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind = ANY($2) ORDER BY position",
    )
    .bind(serveur)
    .bind(&servers::TYPES_VOCAUX[..])
    .fetch_all(pool)
    .await
}

pub(super) async fn mods(
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

pub(super) async fn enregistrer_mod(
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

pub(super) async fn fermer_session(
    pool: &PgPool,
    user: &str,
    salon: &str,
) -> Result<(), sqlx::Error> {
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

pub(super) async fn arrivee(pool: &PgPool, user: &str, salon: &str) -> Result<(), sqlx::Error> {
    fermer_session(pool, user, "").await?;
    sqlx::query("INSERT INTO voice_sessions (user_id, channel_id) VALUES ($1::uuid,$2::uuid)")
        .bind(user)
        .bind(salon)
        .execute(pool)
        .await
        .map(|_| ())
}

pub(super) async fn depart(pool: &PgPool, user: &str, salon: &str) -> Result<(), sqlx::Error> {
    fermer_session(pool, user, salon).await
}

pub(super) async fn depuis(
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

pub(super) async fn evenement_salon(
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

pub(super) async fn serveur_du_salon(
    pool: &PgPool,
    salon: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT server_id::text FROM channels WHERE id=$1::uuid")
        .bind(salon)
        .fetch_optional(pool)
        .await
}
