use super::*;

pub(super) async fn cible_presente(
    state: &AppState,
    _pool: &PgPool,
    user: &str,
    p: &HashMap<String, String>,
    besoin: Perm,
) -> Result<Cible, Response> {
    let salon = salon_vocal(state, param(p, "cid"), user).await?;
    let (proprietaire, acces) = serveur_acces(state, &salon.serveur, user, 0).await?;
    if !acces.can(besoin) {
        return Err(interdit());
    }
    let uid = param(p, "uid");
    if !uuid_valide(uid) {
        return Err(membre_introuvable());
    }
    if uid == user || uid == proprietaire {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce membre ne peut pas être visé",
        ));
    }
    let acces_cible = state
        .acces_serveur(&salon.serveur, uid)
        .await
        .map_err(bd)?
        .ok_or_else(membre_introuvable)?;
    if acces.top() <= acces_cible.top() {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce membre a un rang égal ou supérieur au vôtre",
        ));
    }
    let ps = Lk::new(&state.config().livekit)
        .participants(&salon.id)
        .await
        .map_err(|_| injoignable())?;
    if !ps.iter().any(|q| q.user_id == uid) {
        return Err(absent_du_vocal());
    }
    Ok(Cible {
        salon,
        acces,
        acces_cible,
        uid: uid.to_string(),
    })
}

pub(super) async fn expulser_du_salon(lk: &Lk, salon: &str, uid: &str) -> Result<(), Response> {
    match lk.retirer(salon, uid).await {
        Ok(()) | Err(Erreur::Absent) => {}
        Err(_) => return Err(injoignable()),
    }
    let _ = lk.retirer(salon, &format!("{uid}#screen")).await;
    Ok(())
}

pub(super) async fn pseudo_de(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT pseudo FROM users WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) async fn nom_du_salon(pool: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT name FROM channels WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) async fn deplacer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let c = cible_presente(&state, pool, &u.id, &p, roles::MOVE_MEMBERS).await?;
    let entree = decode(body).await?;
    let destination = champ_texte(&entree, "channel_id")
        .ok_or_else(|| write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide"))?;
    let invalide = || {
        write_err(
            StatusCode::BAD_REQUEST,
            "invalid_channel",
            "salon vocal de destination invalide",
        )
    };
    if !uuid_valide(&destination) || destination == c.salon.id {
        return Ok(invalide());
    }
    let valide = sqlx::query_scalar::<_, bool>(
        "SELECT server_id::text = $2 AND kind = ANY($3) FROM channels WHERE id=$1::uuid",
    )
    .bind(&destination)
    .bind(&c.salon.serveur)
    .bind(&servers::TYPES_VOCAUX[..])
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .unwrap_or(false);
    if !valide || !roles::contient(c.acces.channel(&destination), roles::VIEW_CHANNEL) {
        return Ok(invalide());
    }
    if !roles::contient(c.acces_cible.channel(&destination), roles::CONNECT) {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "cannot_join",
            "ce membre ne peut pas rejoindre ce salon",
        ));
    }
    let nom = nom_du_salon(pool, &destination).await;
    state.publier(
        std::slice::from_ref(&c.uid),
        "voice.moved",
        &json!({
            "server_id": c.salon.serveur, "from": c.salon.id, "to": destination,
            "by": u.pseudo, "to_name": nom,
        }),
    );
    expulser_du_salon(&Lk::new(&state.config().livekit), &c.salon.id, &c.uid).await?;
    let pseudo = pseudo_de(pool, &c.uid).await;
    audit::audit_membre(
        pool,
        &c.salon.serveur,
        &u.id,
        "voice.move",
        &c.uid,
        &format!("{pseudo} → {nom}"),
        "",
    )
    .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub(super) async fn deconnecter(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let c = cible_presente(&state, pool, &u.id, &p, roles::DISCONNECT_MEMBERS).await?;
    state.publier(
        std::slice::from_ref(&c.uid),
        "voice.disconnected",
        &json!({ "server_id": c.salon.serveur, "channel_id": c.salon.id }),
    );
    expulser_du_salon(&Lk::new(&state.config().livekit), &c.salon.id, &c.uid).await?;
    let pseudo = pseudo_de(pool, &c.uid).await;
    audit::audit_membre(
        pool,
        &c.salon.serveur,
        &u.id,
        "voice.disconnect",
        &c.uid,
        &pseudo,
        "",
    )
    .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub(super) async fn evincer(lk: &Lk, salons: &[String], user: &str) {
    for s in salons {
        if let Ok(ps) = lk.participants(s).await
            && ps.iter().any(|p| p.user_id == user)
        {
            let _ = lk.retirer(s, user).await;
            let _ = lk.retirer(s, &format!("{user}#screen")).await;
        }
    }
}

pub(super) async fn synchroniser(state: &AppState, serveur: &str, user: &str) {
    let Ok(pool) = state.pool() else { return };
    let Ok(salons) = salons_vocaux(pool, serveur).await else {
        return;
    };
    let lk = Lk::new(&state.config().livekit);
    for cid in salons {
        let Ok(ps) = lk.participants(&cid).await else {
            continue;
        };
        if ps.is_empty() {
            continue;
        }
        let modere = mods(pool, std::slice::from_ref(&cid))
            .await
            .unwrap_or_default();
        for q in ps {
            if !user.is_empty() && q.user_id != user {
                continue;
            }
            let Ok(Some(acces)) = state.acces_serveur(serveur, &q.user_id).await else {
                continue;
            };
            let perms = acces.channel(&cid);
            if !roles::contient(perms, roles::CONNECT) {
                let _ = lk.retirer(&cid, &q.user_id).await;
                continue;
            }
            let (m, s) = modere
                .get(&(cid.clone(), q.user_id.clone()))
                .copied()
                .unwrap_or_default();
            let _ = lk.definir_mod(&cid, &q.user_id, mod_de(m, s, perms)).await;
        }
    }
}
