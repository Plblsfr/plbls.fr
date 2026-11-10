use super::*;

pub(super) async fn jeton(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let salon = salon_vocal(&state, param(&p, "cid"), &u.id).await?;
    if !roles::contient(salon.perms, roles::CONNECT) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas rejoindre ce salon vocal",
        ));
    }
    let lk = Lk::new(&state.config().livekit);
    if q.get("screen").is_some_and(|v| v == "1") {
        if !roles::contient(salon.perms, roles::SHARE_SCREEN) {
            return Ok(write_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "vous ne pouvez pas partager votre écran dans ce salon",
            ));
        }
        match lk.participants(&salon.id).await {
            Ok(ps) if ps.iter().any(|p| p.user_id == u.id) => {}
            Ok(_) => {
                return Ok(write_err(
                    StatusCode::CONFLICT,
                    "voice_not_joined",
                    "rejoignez d'abord le salon vocal pour partager votre écran",
                ));
            }
            Err(_) => return Ok(injoignable()),
        }
        let modere = mods(pool, std::slice::from_ref(&salon.id))
            .await
            .map_err(bd)?
            .get(&(salon.id.clone(), u.id.clone()))
            .copied()
            .unwrap_or_default();
        return Ok(write_json(
            StatusCode::OK,
            &json!({
                "url": lk.url_publique(),
                "token": lk.jeton_ecran(&salon.id, &u.id, &u.pseudo, mod_de(modere.0, modere.1, salon.perms)),
                "room": salle(&salon.id),
            }),
        ));
    }
    let max = capacite(salon.limite);
    match lk.admettre(&salon.id, &u.id, salon.limite).await {
        Ok(()) => {}
        Err(Erreur::Complet) => {
            return Ok(write_err(
                StatusCode::CONFLICT,
                "voice_full",
                &format!("salon vocal complet ({max} participants maximum)"),
            ));
        }
        Err(_) => return Ok(injoignable()),
    }
    let modere = mods(pool, std::slice::from_ref(&salon.id))
        .await
        .map_err(bd)?
        .get(&(salon.id.clone(), u.id.clone()))
        .copied()
        .unwrap_or_default();
    let (fps, hauteur) = lk.limites();
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "screen_max_fps": fps, "screen_max_height": hauteur,
            "url": lk.url_publique(),
            "token": lk.jeton(&salon.id, &u.id, &u.pseudo, mod_de(modere.0, modere.1, salon.perms)),
            "room": salle(&salon.id), "max_participants": max,
        }),
    ))
}

pub(super) async fn presents(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (_, acces) = serveur_acces(&state, param(&p, "id"), &u.id, 0).await?;
    let lk = Lk::new(&state.config().livekit);
    let mut sortie: Vec<(String, Vec<Participant>)> = Vec::new();
    for cid in salons_vocaux(pool, param(&p, "id")).await.map_err(bd)? {
        if !roles::contient(acces.channel(&cid), roles::VIEW_CHANNEL) {
            continue;
        }
        let ps = lk.participants(&cid).await.map_err(|_| injoignable())?;
        sortie.push((cid, ps));
    }
    let ids: Vec<String> = sortie.iter().map(|(c, _)| c.clone()).collect();
    let depuis = depuis(pool, &ids).await;
    let modere = mods(pool, &ids).await.map_err(bd)?;
    let mut corps = Map::new();
    for (cid, mut ps) in sortie {
        for q in &mut ps {
            if let Ok(d) = &depuis
                && let Some(t) = d.get(&(cid.clone(), q.user_id.clone()))
            {
                q.since.clone_from(t);
            }
            let (m, s) = modere
                .get(&(cid.clone(), q.user_id.clone()))
                .copied()
                .unwrap_or_default();
            (q.mic_off, q.sound_off) = (m, s);
        }
        corps.insert(cid, json!(ps));
    }
    Ok(write_json(StatusCode::OK, &Value::Object(corps)))
}
