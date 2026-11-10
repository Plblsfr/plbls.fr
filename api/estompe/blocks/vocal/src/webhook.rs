use super::*;

pub(super) async fn webhook(
    State(state): State<AppState>,
    entetes: HeaderMap,
    body: Body,
) -> Response {
    let Ok(corps) = to_bytes(body, CORPS_WEBHOOK_MAX).await else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "corps illisible");
    };
    let autorisation = entetes
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let Ok(mut ev) = Lk::new(&state.config().livekit).webhook(autorisation, &corps) else {
        return write_err(
            StatusCode::UNAUTHORIZED,
            "bad_signature",
            "signature invalide",
        );
    };
    let Ok(pool) = pool(&state) else {
        return internal();
    };
    let arrive = ev.event == "participant_joined";
    let ecran = livekit::proprietaire_ecran(&ev.identite).map(str::to_string);
    if let Some(proprio) = &ecran {
        if ev.event == "participant_left" {
            ev.event = "track_unpublished".into();
            ev.source = json!("SCREEN_SHARE");
        }
        ev.identite.clone_from(proprio);
    }
    let salon = salon_de(&ev.salle).filter(|s| uuid_valide(s));
    if let Some(cid) = salon
        && ecran.is_none()
        && (arrive || ev.event == "participant_left")
        && let Ok(Some(sid)) = serveur_du_salon(pool, cid).await
    {
        if uuid_valide(&ev.identite) && ev.identite != SYSTEME {
            let r = if arrive {
                arrivee(pool, &ev.identite, cid).await
            } else {
                depart(pool, &ev.identite, cid).await
            };
            if let Err(e) = r {
                return bd(e);
            }
            if arrive {
                prevenir(&state, &sid, cid, &ev.identite, &ev.nom, "a rejoint").await;
            }
            if !arrive {
                state.sortie_salon_vocal(&sid, cid, &ev.identite).await;
            }
        }
        let a: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT to_char(now() AT TIME ZONE 'UTC', '{HORODATAGE}')"
        )))
        .fetch_one(pool)
        .await
        .map(|t: String| rfc3339(&t))
        .unwrap_or_default();
        evenement_salon(
            &state,
            pool,
            &sid,
            cid,
            "voice.state",
            &json!({
                "server_id": sid, "channel_id": cid, "user_id": ev.identite, "pseudo": ev.nom,
                "joined": arrive, "at": a,
            }),
        )
        .await;
    }
    let src = livekit::source(&ev.source);
    if !src.is_empty()
        && (ev.event == "track_published" || ev.event == "track_unpublished")
        && let Some(cid) = salon
        && let Ok(Some(sid)) = serveur_du_salon(pool, cid).await
    {
        evenement_salon(
            &state,
            pool,
            &sid,
            cid,
            "voice.share",
            &json!({
                "server_id": sid, "channel_id": cid, "user_id": ev.identite, "pseudo": ev.nom,
                "source": src, "on": ev.event == "track_published",
            }),
        )
        .await;
        if ev.event == "track_published" && uuid_valide(&ev.identite) {
            let action = if src == "camera" {
                "a allumé sa caméra"
            } else {
                "partage son écran"
            };
            prevenir(&state, &sid, cid, &ev.identite, &ev.nom, action).await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn prevenir(
    state: &AppState,
    serveur: &str,
    salon: &str,
    user: &str,
    pseudo: &str,
    action: &str,
) {
    state
        .notifier(&EvenementMessage {
            conv: Conv {
                server_id: Some(serveur.to_string()),
                channel_id: Some(salon.to_string()),
                ..Default::default()
            },
            message: json!({ "author": { "id": user, "pseudo": pseudo } }),
            vocal: action.to_string(),
            ..Default::default()
        })
        .await;
}
