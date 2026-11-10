use super::*;

pub(super) async fn changer_epingle(
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

pub(super) async fn epingler(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    changer_epingle(&state, &u, &p, true).await
}

pub(super) async fn desepingler(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    changer_epingle(&state, &u, &p, false).await
}

pub(super) async fn lister_epingles(
    state: &AppState,
    u: &User,
    conv: Conv,
) -> Result<Response, Response> {
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

pub(super) async fn epingles_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    lister_epingles(&state, &u, conv).await
}

pub(super) async fn epingles_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    lister_epingles(&state, &u, conv).await
}
