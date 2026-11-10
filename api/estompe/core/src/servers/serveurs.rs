use super::*;

pub(super) async fn creer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    if !state
        .limiteurs()
        .serveurs
        .allow(&u.id, CREATIONS_PAR_FENETRE, FENETRE_CREATIONS)
    {
        return Ok(crate::messages::trop_de_requetes(60));
    }
    let entree = decode(body).await?;
    let brut = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    let Some(nom) = nom_propre(&brut, 50) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "nom : 1 à 50 caractères",
        ));
    };
    let sv = creer_serveur(pool, &u.id, &nom).await.map_err(bd)?;
    Ok(write_json(StatusCode::CREATED, &sv.json()))
}

pub(super) async fn creer_serveur(
    pool: &PgPool,
    owner: &str,
    name: &str,
) -> Result<Serveur, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id: String = sqlx::query_scalar(
        "INSERT INTO servers (name, owner_id) VALUES ($1,$2::uuid) RETURNING id::text",
    )
    .bind(name)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)")
        .bind(&id)
        .bind(owner)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO roles (server_id, name, position, permissions, is_default) VALUES ($1::uuid,'@tout le monde',0,$2,true)",
    )
    .bind(&id)
    .bind(roles::EVERYONE as i64)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid,'général','text'), ($1::uuid,'vocal','voice')",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Serveur {
        id,
        name: name.to_string(),
        owner_id: owner.to_string(),
        icon_version: 0,
    })
}

pub(super) async fn lister(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let l: Vec<Value> = sqlx::query_as::<_, LigneServeur>(
        "SELECT s.id::text, s.name, s.owner_id::text, s.icon_version FROM servers s
         JOIN server_members m ON m.server_id = s.id WHERE m.user_id=$1::uuid ORDER BY s.created_at",
    )
    .bind(&u.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?
    .into_iter()
    .map(|l| Serveur::from(l).json())
    .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

pub(super) async fn salons(pool: &PgPool, server_id: &str) -> Result<Vec<Salon>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (String, String, String, i32, i32, bool, String, i32)>(
        "SELECT id::text, name, kind, position, slowmode, locked, topic, user_limit FROM channels WHERE server_id=$1::uuid ORDER BY kind DESC, position",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(
        |(id, name, kind, position, slowmode, locked, topic, user_limit)| Salon {
            id,
            name,
            kind,
            position,
            slowmode,
            locked,
            topic,
            user_limit,
        },
    )
    .collect())
}

pub(super) type LigneMembre = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);

pub(super) async fn membres(
    pool: &PgPool,
    server_id: &str,
) -> Result<Vec<LigneMembre>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT u.id::text, u.pseudo, m.nickname, {}, {}, {}, u.banner_color FROM server_members m JOIN users u ON u.id = m.user_id
         WHERE m.server_id=$1::uuid ORDER BY lower(u.pseudo)",
        horodatage("m.timeout_until"),
        horodatage("m.joined_at"),
        horodatage("u.created_at"),
    )))
    .bind(server_id)
    .fetch_all(pool)
    .await
}

pub(super) async fn voir(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), 0).await?;
    let (salons, membres, roles_liste, roles_membres) = tokio::try_join!(
        salons(pool, &sv.id),
        membres(pool, &sv.id),
        roles::roles_du_serveur(pool, &sv.id),
        roles::roles_des_membres(pool, &sv.id),
    )
    .map_err(bd)?;
    let salons: Vec<Value> = salons
        .iter()
        .filter_map(|c| {
            let perms = acc.channel(&c.id);
            roles::contient(perms, roles::VIEW_CHANNEL).then(|| {
                let mut v = c.json();
                v["permissions"] = json!(roles::cles(perms));
                v
            })
        })
        .collect();
    let membres: Vec<Value> = membres
        .into_iter()
        .map(|(id, pseudo, nickname, timeout, joined, created, banner)| {
            json!({
                "id": id,
                "pseudo": pseudo,
                "nickname": nickname,
                "timeout_until": timeout.map(|t| rfc3339(&t)),
                "joined_at": rfc3339(&joined),
                "created_at": rfc3339(&created),
                "banner_color": banner,
                "online": state.hub().online(&id),
                "roles": roles_membres.get(&id).cloned().unwrap_or_default(),
            })
        })
        .collect();
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "server": sv.json(),
            "channels": salons,
            "members": membres,
            "roles": roles_liste,
            "permissions": roles::cles(acc.base()),
        }),
    ))
}

pub(super) async fn renommer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_SERVER).await?;
    let entree = decode(body).await?;
    let brut = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    let Some(nom) = nom_propre(&brut, 50) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "nom : 1 à 50 caractères",
        ));
    };
    sqlx::query("UPDATE servers SET name=$2 WHERE id=$1::uuid")
        .bind(&sv.id)
        .bind(&nom)
        .execute(pool)
        .await
        .map_err(bd)?;
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "server.updated",
        &json!({ "id": sv.id, "name": nom }),
    )
    .await;
    Ok(sans_contenu())
}

pub(super) async fn supprimer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    if sv.owner_id != u.id {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "réservé au créateur du serveur",
        ));
    }
    let to = membres_ids(pool, &sv.id).await.unwrap_or_default();
    sqlx::query("DELETE FROM servers WHERE id=$1::uuid")
        .bind(&sv.id)
        .execute(pool)
        .await
        .map_err(bd)?;
    diffuser(&state, &to, "server.deleted", &json!({ "id": sv.id }));
    Ok(sans_contenu())
}

pub(super) async fn quitter(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    if sv.owner_id == u.id {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "owner_cannot_leave",
            "le créateur ne peut pas quitter son serveur, il peut le supprimer",
        ));
    }
    retirer_membre(pool, &sv.id, &u.id).await.map_err(bd)?;
    let mut to = membres_ids(pool, &sv.id).await.unwrap_or_default();
    to.push(u.id.clone());
    diffuser(
        &state,
        &to,
        "member.left",
        &json!({ "server_id": sv.id, "user_id": u.id }),
    );
    Ok(sans_contenu())
}
