use super::*;

pub(crate) async fn serveur_pour(
    pool: &PgPool,
    server_id: &str,
    user: &str,
) -> Result<Option<Serveur>, sqlx::Error> {
    Ok(sqlx::query_as::<_, LigneServeur>(
        "SELECT s.id::text, s.name, s.owner_id::text, s.icon_version FROM servers s
         JOIN server_members m ON m.server_id = s.id WHERE s.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(server_id)
    .bind(user)
    .fetch_optional(pool)
    .await?
    .map(Serveur::from))
}

pub async fn acces(
    state: &AppState,
    u: &User,
    id: &str,
    besoin: Perm,
) -> Result<(Serveur, Access), Response> {
    if !uuid_valide(id) {
        return Err(non_trouve("serveur introuvable"));
    }
    let pool = state.pool().map_err(IntoResponse::into_response)?;
    let sv = serveur_pour(pool, id, &u.id)
        .await
        .map_err(bd)?
        .ok_or_else(|| non_trouve("serveur introuvable"))?;
    let acc = state
        .acces_serveur(&sv.id, &u.id)
        .await
        .map_err(bd)?
        .ok_or_else(internal)?;
    if !acc.can(besoin) {
        return Err(interdit());
    }
    Ok((sv, acc))
}

pub fn param<'a>(p: &'a HashMap<String, String>, nom: &str) -> &'a str {
    p.get(nom).map_or("", String::as_str)
}

pub(crate) fn id_salon(p: &HashMap<String, String>) -> Result<String, Response> {
    let cid = p.get("cid").map_or("order", String::as_str);
    if uuid_valide(cid) {
        Ok(cid.to_string())
    } else {
        Err(non_trouve("salon introuvable"))
    }
}

pub async fn membres_ids(pool: &PgPool, server_id: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT user_id::text FROM server_members WHERE server_id=$1::uuid")
        .bind(server_id)
        .fetch_all(pool)
        .await
}

pub(super) fn diffuser(state: &AppState, to: &[String], typ: &str, data: &Value) {
    if !to.is_empty() {
        state.publier(to, typ, data);
    }
}

pub async fn evenement_serveur(
    state: &AppState,
    pool: &PgPool,
    server_id: &str,
    typ: &str,
    data: &Value,
) {
    if let Ok(to) = membres_ids(pool, server_id).await {
        diffuser(state, &to, typ, data);
    }
}

pub(crate) async fn evenement_salon(
    state: &AppState,
    pool: &PgPool,
    server_id: &str,
    channel_id: &str,
    typ: &str,
    data: &Value,
) {
    if let Ok(to) = roles::voyeurs_du_salon(pool, server_id, channel_id).await {
        diffuser(state, &to, typ, data);
    }
}
