use super::*;

pub(super) fn salon_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_channel",
        "nom : 1 à 30 caractères ; type : text, voice, videos, music ou cinema",
    )
}

pub(super) async fn creer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), 0).await?;
    let e = decode(body).await?;
    let nom_brut = champ_texte(&e, "name").ok_or_else(mauvais)?;
    let kind = champ_texte(&e, "kind").ok_or_else(mauvais)?;
    let sujet_brut = champ_texte(&e, "topic").ok_or_else(mauvais)?;
    let after = champ_texte(&e, "after").ok_or_else(mauvais)?;
    let slowmode = entier(&e, "slowmode").ok_or_else(mauvais)?;
    let user_limit = entier(&e, "user_limit").ok_or_else(mauvais)?;
    let prive = booleen(&e, "private").ok_or_else(mauvais)?;
    let roles_ids = liste(&e, "roles").ok_or_else(mauvais)?;
    let membres_ids = liste(&e, "members").ok_or_else(mauvais)?;
    if !acc.can(roles::MANAGE_CHANNELS) {
        return Ok(interdit());
    }
    let nom = nom_propre(&nom_brut, 30);
    let sujet = sujet_brut.trim().to_string();
    let invalide =
        |code: &str, message: &str| Ok(write_err(StatusCode::BAD_REQUEST, code, message));
    if nom.is_none() || !TYPES.contains(&kind.as_str()) {
        return Ok(salon_invalide());
    }
    if kind == "cinema" && !cinema_disponible(&state) {
        return invalide(
            "invalid_channel",
            "salon cinéma : ni Entracte ni la bibliothèque de films ne sont branchés sur ce serveur",
        );
    }
    if sujet.chars().count() > 200 {
        return invalide("invalid_topic", "sujet : 200 caractères maximum");
    }
    if slowmode != 0 && (kind != "text" || !(5..=21600).contains(&slowmode)) {
        return invalide(
            "invalid_slowmode",
            "mode lent : salon texte, désactivé (0) ou de 5 s à 6 h",
        );
    }
    if user_limit != 0 && (!est_vocal(&kind) || !(2..=MAX_PARTICIPANTS_VOCAL).contains(&user_limit))
    {
        return invalide(
            "invalid_user_limit",
            &format!("limite : salon vocal, de 2 à {MAX_PARTICIPANTS_VOCAL} participants"),
        );
    }
    if !prive && (!roles_ids.is_empty() || !membres_ids.is_empty()) {
        return invalide(
            "invalid_channel",
            "des rôles ou membres n'ont de sens que pour un salon privé",
        );
    }
    if prive
        && (roles_ids.len() > MAX_ROLES
            || membres_ids.len() > 200
            || roles_ids
                .iter()
                .chain(&membres_ids)
                .any(|id| !uuid_valide(id)))
    {
        return invalide("invalid_channel", "rôles ou membres invalides");
    }
    if prive && roles::VIEW_CHANNEL & !acc.base() != 0 {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas réserver un salon que vous ne voyez pas",
        ));
    }
    let mut roles_tries = roles_ids;
    roles_tries.sort();
    roles_tries.dedup();
    let nouveau = NouveauSalon {
        name: nom.unwrap_or_default(),
        kind,
        topic: sujet,
        slowmode: slowmode as i32,
        user_limit: user_limit as i32,
        after,
        prive,
        roles: roles_tries,
        membres: membres_ids,
        createur: u.id.clone(),
    };
    match creer_salon_base(pool, &sv.id, &nouveau).await {
        Ok(c) => {
            let ev = json!({ "server_id": sv.id, "channel": c.json(), "after": nouveau.after });
            if prive {
                evenement_salon(&state, pool, &sv.id, &c.id, "channel.created", &ev).await;
            } else {
                evenement_serveur(&state, pool, &sv.id, "channel.created", &ev).await;
            }
            Ok(write_json(StatusCode::CREATED, &c.json()))
        }
        Err(Echec::NomPris) => Ok(nom_pris()),
        Err(Echec::TropDeSalons) => Ok(write_err(
            StatusCode::CONFLICT,
            "too_many_channels",
            "50 salons maximum par serveur",
        )),
        Err(Echec::NonTrouve) => invalide(
            "invalid_channel",
            "rôle ou membre introuvable sur ce serveur",
        ),
        Err(e) => Ok(echec_interne(e)),
    }
}

pub(super) fn nom_pris() -> Response {
    write_err(
        StatusCode::CONFLICT,
        "name_taken",
        "un salon de ce type porte déjà ce nom",
    )
}

pub(super) async fn creer_salon_base(
    pool: &PgPool,
    server_id: &str,
    n: &NouveauSalon,
) -> Result<Salon, Echec> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM servers WHERE id=$1::uuid FOR UPDATE")
        .bind(server_id)
        .execute(&mut *tx)
        .await?;
    let existants: i64 =
        sqlx::query_scalar("SELECT count(*) FROM channels WHERE server_id=$1::uuid")
            .bind(server_id)
            .fetch_one(&mut *tx)
            .await?;
    if existants >= MAX_SALONS {
        return Err(Echec::TropDeSalons);
    }
    let (id, position) = sqlx::query_as::<_, (String, i32)>(
        "INSERT INTO channels (server_id, name, kind, topic, slowmode, user_limit) VALUES ($1::uuid,$2,$3,$4,$5,$6) RETURNING id::text, position",
    )
    .bind(server_id)
    .bind(&n.name)
    .bind(&n.kind)
    .bind(&n.topic)
    .bind(n.slowmode)
    .bind(n.user_limit)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| if unique(&e) { Echec::NomPris } else { Echec::Base(e) })?;
    let mut c = Salon {
        id,
        name: n.name.clone(),
        kind: n.kind.clone(),
        position,
        slowmode: n.slowmode,
        locked: false,
        topic: n.topic.clone(),
        user_limit: n.user_limit,
    };
    if !n.after.is_empty() {
        c.position = placer_apres(&mut tx, server_id, &c, &n.after).await?;
    }
    if n.prive {
        rendre_prive(&mut tx, server_id, &c.id, n).await?;
    }
    tx.commit().await?;
    Ok(c)
}

pub(super) async fn placer_apres(
    tx: &mut sqlx::PgConnection,
    server_id: &str,
    c: &Salon,
    after: &str,
) -> Result<i32, sqlx::Error> {
    let mut ids: Vec<String> = sqlx::query_scalar(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind=$2 AND id<>$3::uuid ORDER BY position",
    )
    .bind(server_id)
    .bind(&c.kind)
    .bind(&c.id)
    .fetch_all(&mut *tx)
    .await?;
    let Some(i) = ids.iter().position(|id| id == after) else {
        return Ok(c.position);
    };
    ids.insert(i + 1, c.id.clone());
    for (j, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE channels SET position=$2 WHERE id=$1::uuid")
            .bind(id)
            .bind(j as i32 + 1)
            .execute(&mut *tx)
            .await?;
    }
    Ok(i as i32 + 2)
}

pub(super) async fn rendre_prive(
    tx: &mut sqlx::PgConnection,
    server_id: &str,
    channel_id: &str,
    n: &NouveauSalon,
) -> Result<(), Echec> {
    let voir = roles::VIEW_CHANNEL as i64;
    sqlx::query(
        "INSERT INTO channel_overrides (channel_id, role_id, deny) SELECT $1::uuid, id, $3 FROM roles WHERE server_id=$2::uuid AND is_default",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(voir)
    .execute(&mut *tx)
    .await?;
    let ajoutes = sqlx::query(
        "INSERT INTO channel_overrides (channel_id, role_id, allow) SELECT $1::uuid, id, $3 FROM roles WHERE server_id=$2::uuid AND NOT is_default AND id::text = ANY($4)",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(voir)
    .bind(&n.roles)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if ajoutes as usize != n.roles.len() {
        return Err(Echec::NonTrouve);
    }
    let mut membres: Vec<String> = n.membres.clone();
    membres.push(n.createur.clone());
    membres.sort();
    membres.dedup();
    let ajoutes = sqlx::query(
        "INSERT INTO channel_member_overrides (channel_id, user_id, allow) SELECT $1::uuid, user_id, $3 FROM server_members WHERE server_id=$2::uuid AND user_id::text = ANY($4)",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(voir)
    .bind(&membres)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if ajoutes as usize != membres.len() {
        return Err(Echec::NonTrouve);
    }
    Ok(())
}

pub(super) async fn renommer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p)?;
    let entree = decode(body).await?;
    let brut = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    let Some(nom) = nom_propre(&brut, 30) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "nom : 1 à 30 caractères",
        ));
    };
    let r = sqlx::query("UPDATE channels SET name=$3 WHERE id=$2::uuid AND server_id=$1::uuid")
        .bind(&sv.id)
        .bind(&cid)
        .bind(&nom)
        .execute(pool)
        .await;
    match r {
        Err(e) if unique(&e) => Ok(nom_pris()),
        Err(e) => Err(bd(e)),
        Ok(r) if r.rows_affected() == 0 => Ok(non_trouve("salon introuvable")),
        Ok(_) => {
            evenement_salon(
                &state,
                pool,
                &sv.id,
                &cid,
                "channel.updated",
                &json!({ "server_id": sv.id, "id": cid, "name": nom }),
            )
            .await;
            Ok(sans_contenu())
        }
    }
}

pub(super) async fn ordonner(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let e = decode(body).await?;
    let kind = champ_texte(&e, "kind").ok_or_else(mauvais)?;
    let ids_nul = matches!(champ(&e, "ids"), None | Some(Value::Null));
    let ids = liste(&e, "ids").ok_or_else(mauvais)?;
    let invalide = |message: &str| write_err(StatusCode::BAD_REQUEST, "invalid_order", message);
    if !TYPES.contains(&kind.as_str()) || ids.len() as i64 > MAX_SALONS {
        return Ok(invalide(
            "type : text ou voice ; liste des salons de ce type",
        ));
    }
    if ids.iter().any(|id| !uuid_valide(id)) {
        return Ok(invalide("identifiant de salon invalide"));
    }
    match reordonner(pool, &sv.id, &kind, &ids).await {
        Err(Echec::NonTrouve) => Ok(invalide(
            "la liste doit contenir exactement les salons de ce type",
        )),
        Err(e) => Err(echec_interne(e)),
        Ok(()) => {
            let ids = if ids_nul { Value::Null } else { json!(ids) };
            evenement_serveur(
                &state,
                pool,
                &sv.id,
                "channel.reordered",
                &json!({ "server_id": sv.id, "kind": kind, "ids": ids }),
            )
            .await;
            Ok(sans_contenu())
        }
    }
}

pub(super) async fn reordonner(
    pool: &PgPool,
    server_id: &str,
    kind: &str,
    ids: &[String],
) -> Result<(), Echec> {
    let mut tx = pool.begin().await?;
    let mut presents: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind=$2 FOR UPDATE",
    )
    .bind(server_id)
    .bind(kind)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    if ids.len() != presents.len() {
        return Err(Echec::NonTrouve);
    }
    for (i, id) in ids.iter().enumerate() {
        if !presents.remove(id) {
            return Err(Echec::NonTrouve);
        }
        sqlx::query("UPDATE channels SET position=$2 WHERE id=$1::uuid")
            .bind(id)
            .bind(i as i32 + 1)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(super) async fn supprimer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p)?;
    let to = roles::voyeurs_du_salon(pool, &sv.id, &cid)
        .await
        .map_err(bd)?;
    let r = sqlx::query("DELETE FROM channels WHERE id=$2::uuid AND server_id=$1::uuid")
        .bind(&sv.id)
        .bind(&cid)
        .execute(pool)
        .await
        .map_err(bd)?;
    if r.rows_affected() == 0 {
        return Ok(non_trouve("salon introuvable"));
    }
    diffuser(
        &state,
        &to,
        "channel.deleted",
        &json!({ "server_id": sv.id, "id": cid }),
    );
    Ok(sans_contenu())
}
