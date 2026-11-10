use super::*;

pub(super) async fn reagir(
    state: State<AppState>,
    u: ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    reaction(state.0, u.0, p, true).await
}

pub(super) async fn retirer_reaction(
    state: State<AppState>,
    u: ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    reaction(state.0, u.0, p, false).await
}

pub(super) async fn reaction(
    state: AppState,
    u: User,
    p: Params,
    ajout: bool,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let emoji = param(&p.0, "emoji");
    if !emoji::valide(emoji) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_emoji",
            "emoji invalide",
        ));
    }
    let id = id_message(param(&p.0, "id"))?;
    let (conv, _) = conv_du_message(&state, id, &u.id).await?;
    if ajout && !conv.peut(roles::ADD_REACTIONS) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas réagir dans ce salon",
        ));
    }
    if ajout {
        match ajouter_reaction(pool, id, &u.id, emoji).await {
            Ok(true) => {}
            Ok(false) => {
                return Ok(write_err(
                    StatusCode::CONFLICT,
                    "too_many_reactions",
                    "20 emojis différents maximum par message",
                ));
            }
            Err(e) => return Err(bd(e)),
        }
    } else {
        sqlx::query("DELETE FROM reactions WHERE message_id=$1 AND user_id=$2::uuid AND emoji=$3")
            .bind(id)
            .bind(&u.id)
            .bind(emoji)
            .execute(pool)
            .await
            .map_err(bd)?;
    }
    let charge = json!({ "message_id": param(&p.0, "id"), "emoji": emoji, "user_id": u.id, "pseudo": u.pseudo, "added": ajout });
    publier(&state, pool, &conv, "reaction.updated", &charge).await;
    Ok(sans_contenu())
}

pub(super) async fn ajouter_reaction(
    pool: &PgPool,
    id: i64,
    user: &str,
    emoji: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM messages WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let deja: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM reactions WHERE message_id=$1 AND emoji=$2)",
    )
    .bind(id)
    .bind(emoji)
    .fetch_one(&mut *tx)
    .await?;
    if !deja {
        let n: i64 =
            sqlx::query_scalar("SELECT count(DISTINCT emoji) FROM reactions WHERE message_id=$1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if n >= REACTIONS_MAX {
            return Ok(false);
        }
    }
    sqlx::query(
        "INSERT INTO reactions (message_id, user_id, emoji) VALUES ($1,$2::uuid,$3) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(user)
    .bind(emoji)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}
