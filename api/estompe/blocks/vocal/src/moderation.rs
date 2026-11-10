use super::*;

pub(super) async fn moderer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let salon = salon_vocal(&state, param(&p, "cid"), &u.id).await?;
    let (proprietaire, acces) = serveur_acces(&state, &salon.serveur, &u.id, 0).await?;
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
    let Some(acces_cible) = state
        .acces_serveur(&salon.serveur, cible)
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

pub(super) async fn journal(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    serveur_acces(&state, param(&p, "id"), &u.id, roles::VIEW_AUDIT_LOG).await?;
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

pub(super) async fn statistiques(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    serveur_acces(&state, param(&p, "id"), &u.id, 0).await?;
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
