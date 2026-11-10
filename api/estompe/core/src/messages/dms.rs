use super::*;

pub(super) fn vue_dm(id: &str, autre_id: &str, pseudo: &str, en_ligne: bool) -> Value {
    json!({ "id": id, "other": { "id": autre_id, "pseudo": pseudo }, "online": en_ligne })
}

pub(super) async fn lister_dms(
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

pub(super) async fn ouvrir_dm(
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
