use super::*;

pub(super) async fn modifier(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = id_message(param(&p.0, "id"))?;
    let (conv, auteur) = conv_du_message(&state, id, &u.id).await?;
    if auteur != u.id {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez modifier que vos messages",
        ));
    }
    if !conv.peut(roles::SEND_MESSAGES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas écrire dans ce salon",
        ));
    }
    let e = decode(body).await?;
    let contenu_brut = texte_strict(&e, "content").ok_or_else(mauvais)?;
    let fichiers = liste_stricte(&e, "file_ids").map_err(|()| mauvais())?;
    let avec_fichiers = match &fichiers {
        Some(f) => !f.is_empty(),
        None => {
            sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM files WHERE message_id=$1)")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap_or(false)
        }
    };
    let nombre = fichiers.as_ref().map_or(0, Vec::len);
    let Some(contenu) = contenu_valide(&contenu_brut, avec_fichiers) else {
        return Ok(message_invalide());
    };
    if nombre > PIECES_JOINTES_MAX {
        return Ok(message_invalide());
    }
    if nombre > 0 && !conv.peut(roles::ATTACH_FILES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante dans ce salon",
        ));
    }
    if fichiers.iter().flatten().any(|f| !uuid_valide(f)) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_files",
            "fichier invalide",
        ));
    }
    let contenu = convertir_emojis(pool, &u.id, &contenu).await;
    let ajoutes = match remplacer(
        pool,
        state.hub(),
        &conv,
        id,
        &u.id,
        &contenu,
        fichiers.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(x) => return Ok(echec(x, FICHIERS_INTROUVABLES)),
    };
    let message = message_complet(pool, id, &u.id).await.map_err(bd)?;
    publier(&state, pool, &conv, "message.updated", &message).await;
    publier_mentions(&state, &ajoutes, &message);
    if !ajoutes.is_empty() {
        state
            .notifier(&EvenementMessage {
                conv: conv.clone(),
                message: message.clone(),
                mentionnes: ajoutes.iter().map(|(c, _)| c.clone()).collect(),
                groupe: ids_groupe(&ajoutes),
                a: ajoutes.into_iter().map(|(c, _)| c).collect(),
                ..Default::default()
            })
            .await;
    }
    Ok(write_json(StatusCode::OK, &message))
}

pub(super) async fn remplacer(
    pool: &PgPool,
    hub: &crate::hub::Hub,
    conv: &Conv,
    id: i64,
    auteur: &str,
    contenu: &str,
    fichiers: Option<&[String]>,
) -> Result<Vec<(String, bool)>, Echec> {
    let voyeurs = voyeurs_des_mentions(pool, conv, contenu).await?;
    if debit_groupe_depasse(pool, conv, auteur, contenu, id).await? {
        return Err(Echec::Debit);
    }
    let mut tx = pool.begin().await?;
    let modifies = sqlx::query(
        "UPDATE messages SET content=$3, edited_at=now() WHERE id=$1 AND author_id=$2::uuid AND NOT system",
    )
    .bind(id)
    .bind(auteur)
    .bind(contenu)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if modifies == 0 {
        return Err(Echec::Interdit);
    }
    if let Some(f) = fichiers {
        sqlx::query(
            "UPDATE files SET message_id=NULL WHERE message_id=$1 AND NOT (id::text = ANY($2))",
        )
        .bind(id)
        .bind(f)
        .execute(&mut *tx)
        .await?;
        attacher_fichiers(&mut tx, id, auteur, contenu, f).await?;
    }
    let cibles =
        cibles_des_mentions(&mut tx, hub, voyeurs.as_deref(), conv, auteur, contenu).await?;
    let ids_cibles: Vec<&String> = cibles.iter().map(|(c, _)| c).collect();
    sqlx::query(
        "DELETE FROM message_mentions WHERE message_id=$1 AND NOT (user_id::text = ANY(COALESCE($2::text[], '{}')))",
    )
    .bind(id)
    .bind(&ids_cibles)
    .execute(&mut *tx)
    .await?;
    let mut ajoutes = Vec::new();
    for (c, groupe) in &cibles {
        let n = sqlx::query(
            "INSERT INTO message_mentions (message_id, user_id, groupe) VALUES ($1,$2::uuid,$3) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(c)
        .bind(groupe)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 1 {
            ajoutes.push((c.clone(), *groupe));
        }
    }
    tx.commit().await?;
    Ok(ajoutes)
}

pub(super) async fn supprimer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = id_message(param(&p.0, "id"))?;
    let (conv, auteur) = conv_du_message(&state, id, &u.id).await?;
    if auteur != u.id
        && !conv.peut(roles::MANAGE_MESSAGES)
        && !sondages::peut_gerer(&state, pool, id, &u)
            .await
            .map_err(bd)?
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez supprimer que vos messages",
        ));
    }
    let id_brut = param(&p.0, "id").to_string();
    let mut resultats = effacer(pool, id, &auteur).await.map_err(bd)?;
    if auteur != u.id
        && let (Some(serveur), Some(salon)) = (&conv.server_id, &conv.channel_id)
    {
        let pseudo = pseudo_de(pool, &auteur).await;
        let nom = nom_du_salon(pool, salon).await;
        audit::audit_membre(
            pool,
            serveur,
            &u.id,
            "message.delete",
            &auteur,
            &format!("{pseudo} dans {nom}"),
            "",
        )
        .await;
    }
    resultats.push(id_brut);
    for rid in resultats {
        let charge = json!({ "id": rid, "channel_id": conv.channel_id, "dm_id": conv.dm_id });
        publier(&state, pool, &conv, "message.deleted", &charge).await;
    }
    Ok(sans_contenu())
}

pub(super) async fn effacer(
    pool: &PgPool,
    id: i64,
    auteur: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let resultats: Vec<String> = sqlx::query_scalar(
        "DELETE FROM messages r USING messages p
         WHERE r.system AND r.reply_to = p.id AND p.id=$1 AND p.author_id=$2::uuid RETURNING r.id::text",
    )
    .bind(id)
    .bind(auteur)
    .fetch_all(&mut *tx)
    .await?;
    let supprimes = sqlx::query("DELETE FROM messages WHERE id=$1 AND author_id=$2::uuid")
        .bind(id)
        .bind(auteur)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if supprimes == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    tx.commit().await?;
    Ok(resultats)
}

pub(crate) async fn pseudo_de(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT pseudo FROM users WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) async fn nom_du_salon(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT name FROM channels WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}
