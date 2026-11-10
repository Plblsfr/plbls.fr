use super::*;

pub(super) async fn lister_invitations(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::CREATE_INVITES).await?;
    let l: Vec<Value> = sqlx::query_as::<_, (String, String, String, i32, Option<i32>, Option<String>)>(
        sqlx::AssertSqlSafe(format!(
            "SELECT i.code, i.created_at::text, u.pseudo, i.uses, i.max_uses, {} FROM server_invites i JOIN users u ON u.id = i.created_by
             WHERE i.server_id=$1::uuid AND i.revoked_at IS NULL AND (i.expires_at IS NULL OR i.expires_at > now()) AND (i.max_uses IS NULL OR i.uses < i.max_uses)
             ORDER BY i.created_at DESC",
            horodatage("i.expires_at")
        )),
    )
    .bind(&sv.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?
    .into_iter()
    .map(|(code, created_at, created_by, uses, max_uses, expires_at)| {
        invitation(&code, &created_at, &created_by, uses, max_uses, expires_at)
    })
    .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

pub(super) fn invitation(
    code: &str,
    created_at: &str,
    created_by: &str,
    uses: i32,
    max_uses: Option<i32>,
    expires_at: Option<String>,
) -> Value {
    json!({
        "code": code,
        "created_at": created_at,
        "created_by": created_by,
        "uses": uses,
        "max_uses": max_uses,
        "expires_at": expires_at.map(|t| rfc3339(&t)),
    })
}

pub(super) async fn creer_invitation(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::CREATE_INVITES).await?;
    let octets = to_bytes(body, CORPS_MAX).await.map_err(|_| mauvais())?;
    let e = if octets.is_empty() {
        Map::new()
    } else {
        premier_objet(&octets).ok_or_else(mauvais)?
    };
    let max_uses = entier(&e, "max_uses").ok_or_else(mauvais)?;
    let expires_in = entier(&e, "expires_in").ok_or_else(mauvais)?;
    if !(0..=1000).contains(&max_uses) || !(0..=365 * 86400).contains(&expires_in) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_invite",
            "utilisations : 1 à 1000 ; durée : un an au plus",
        ));
    }
    let code = nouveau_code();
    let max = (max_uses > 0).then_some(max_uses as i32);
    let (created_at, expires_at) = sqlx::query_as::<_, (String, Option<String>)>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO server_invites (code, server_id, created_by, max_uses, expires_at)
         VALUES ($1,$2::uuid,$3::uuid,$4, CASE WHEN $5::bigint > 0 THEN now() + $5::bigint * interval '1 second' END)
         RETURNING created_at::text, {}",
        horodatage("expires_at")
    )))
    .bind(&code)
    .bind(&sv.id)
    .bind(&u.id)
    .bind(max)
    .bind(expires_in)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    Ok(write_json(
        StatusCode::CREATED,
        &invitation(&code, &created_at, &u.pseudo, 0, max, expires_at),
    ))
}

pub(super) async fn revoquer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::CREATE_INVITES).await?;
    let r = sqlx::query(
        "UPDATE server_invites SET revoked_at=now() WHERE server_id=$1::uuid AND code=$2 AND revoked_at IS NULL",
    )
    .bind(&sv.id)
    .bind(param(&p, "code").to_uppercase())
    .execute(pool)
    .await
    .map_err(bd)?;
    if r.rows_affected() == 0 {
        return Ok(non_trouve("invitation introuvable"));
    }
    Ok(sans_contenu())
}

pub(super) async fn rejoindre(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let code = param(&p, "code").trim().to_uppercase();
    match rejoindre_base(pool, &u.id, &code).await {
        Err(Echec::NonTrouve) => Ok(write_err(
            StatusCode::NOT_FOUND,
            "invalid_code",
            "invitation invalide, révoquée, expirée ou épuisée",
        )),
        Err(Echec::Banni) => Ok(write_err(
            StatusCode::FORBIDDEN,
            "banned",
            "vous êtes banni de ce serveur",
        )),
        Err(e) => Err(echec_interne(e)),
        Ok((sv, nouveau)) => {
            if nouveau {
                evenement_serveur(
                    &state,
                    pool,
                    &sv.id,
                    "member.joined",
                    &json!({
                        "server_id": sv.id,
                        "member": {
                            "id": u.id,
                            "pseudo": u.pseudo,
                            "online": state.hub().online(&u.id),
                        },
                    }),
                )
                .await;
            }
            Ok(write_json(StatusCode::OK, &sv.json()))
        }
    }
}

pub(super) async fn rejoindre_base(
    pool: &PgPool,
    user: &str,
    code: &str,
) -> Result<(Serveur, bool), Echec> {
    let mut tx = pool.begin().await?;
    let sv: Serveur = sqlx::query_as::<_, LigneServeur>(
        "SELECT s.id::text, s.name, s.owner_id::text, s.icon_version FROM server_invites i JOIN servers s ON s.id = i.server_id
         WHERE i.code=$1 AND i.revoked_at IS NULL AND (i.expires_at IS NULL OR i.expires_at > now()) FOR UPDATE OF i",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Echec::NonTrouve)?
    .into();
    let banni: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM server_bans WHERE server_id=$1::uuid AND user_id=$2::uuid AND (expires_at IS NULL OR expires_at > now()))",
    )
    .bind(&sv.id)
    .bind(user)
    .fetch_one(&mut *tx)
    .await?;
    if banni {
        return Err(Echec::Banni);
    }
    let membre: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid)",
    )
    .bind(&sv.id)
    .bind(user)
    .fetch_one(&mut *tx)
    .await?;
    if membre {
        tx.commit().await?;
        return Ok((sv, false));
    }
    let utilise = sqlx::query(
        "UPDATE server_invites SET uses = uses + 1 WHERE code=$1 AND (max_uses IS NULL OR uses < max_uses)",
    )
    .bind(code)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if utilise == 0 {
        return Err(Echec::NonTrouve);
    }
    sqlx::query("DELETE FROM server_bans WHERE server_id=$1::uuid AND user_id=$2::uuid")
        .bind(&sv.id)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)")
        .bind(&sv.id)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((sv, true))
}
