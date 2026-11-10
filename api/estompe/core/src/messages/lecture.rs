use super::*;

pub(crate) struct Ligne {
    id: String,
    channel_id: Option<String>,
    dm_id: Option<String>,
    auteur_id: String,
    pseudo: String,
    contenu: String,
    cree: String,
    edite: Option<String>,
    epingle: Option<String>,
    systeme: bool,
    reponse_id: Option<String>,
    reponse_auteur: Option<String>,
    reponse_pseudo: Option<String>,
    reponse_contenu: Option<String>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for Ligne {
    fn from_row(r: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            id: r.try_get("id")?,
            channel_id: r.try_get("channel_id")?,
            dm_id: r.try_get("dm_id")?,
            auteur_id: r.try_get("auteur_id")?,
            pseudo: r.try_get("pseudo")?,
            contenu: r.try_get("contenu")?,
            cree: r.try_get("cree")?,
            edite: r.try_get("edite")?,
            epingle: r.try_get("epingle")?,
            systeme: r.try_get("systeme")?,
            reponse_id: r.try_get("reponse_id")?,
            reponse_auteur: r.try_get("reponse_auteur")?,
            reponse_pseudo: r.try_get("reponse_pseudo")?,
            reponse_contenu: r.try_get("reponse_contenu")?,
        })
    }
}

pub(crate) fn selection() -> String {
    format!(
        "SELECT m.id::text AS id, m.channel_id::text AS channel_id, m.dm_id::text AS dm_id, u.id::text AS auteur_id, u.pseudo, m.content AS contenu, {} AS cree, {} AS edite, {} AS epingle, m.system AS systeme,
         r.id::text AS reponse_id, ru.id::text AS reponse_auteur, ru.pseudo AS reponse_pseudo, left(r.content, 120) AS reponse_contenu
         FROM messages m JOIN users u ON u.id = m.author_id
         LEFT JOIN messages r ON r.id = m.reply_to LEFT JOIN users ru ON ru.id = r.author_id",
        horodatage("m.created_at"),
        horodatage("m.edited_at"),
        horodatage("m.pinned_at"),
    )
}

pub(super) fn date(v: Option<String>) -> Value {
    v.map_or(Value::Null, |s| Value::String(rfc3339(&s)))
}

pub(crate) fn construire(lignes: Vec<Ligne>) -> Vec<Map<String, Value>> {
    lignes
        .into_iter()
        .map(|l| {
            let mut m = Map::new();
            m.insert("id".into(), l.id.into());
            if let Some(c) = l.channel_id {
                m.insert("channel_id".into(), c.into());
            }
            if let Some(d) = l.dm_id {
                m.insert("dm_id".into(), d.into());
            }
            m.insert("author".into(), json!({ "id": l.auteur_id, "pseudo": l.pseudo }));
            m.insert("content".into(), l.contenu.into());
            m.insert("created_at".into(), Value::String(rfc3339(&l.cree)));
            m.insert("edited_at".into(), date(l.edite));
            m.insert("attachments".into(), json!([]));
            m.insert("reactions".into(), json!([]));
            m.insert("mentions".into(), json!([]));
            m.insert(
                "reply_to".into(),
                match (l.reponse_id, l.reponse_auteur, l.reponse_pseudo, l.reponse_contenu) {
                    (Some(id), Some(auteur), Some(pseudo), Some(contenu)) => {
                        json!({ "id": id, "author": { "id": auteur, "pseudo": pseudo }, "content": contenu })
                    }
                    _ => Value::Null,
                },
            );
            m.insert("pinned_at".into(), date(l.epingle));
            m.insert("poll".into(), Value::Null);
            m.insert("system".into(), l.systeme.into());
            m
        })
        .collect()
}

pub(super) fn liste_mut<'a>(
    index: &HashMap<String, usize>,
    messages: &'a mut [Map<String, Value>],
    id: &str,
    cle: &str,
) -> Option<&'a mut Vec<Value>> {
    let i = *index.get(id)?;
    messages[i].get_mut(cle)?.as_array_mut()
}

pub(crate) async fn hydrater(
    pool: &PgPool,
    messages: &mut [Map<String, Value>],
    viewer: &str,
) -> Result<(), sqlx::Error> {
    if messages.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = messages
        .iter()
        .filter_map(|m| m["id"].as_str().and_then(|s| s.parse().ok()))
        .collect();
    let index: HashMap<String, usize> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| (m["id"].as_str().unwrap_or_default().to_string(), i))
        .collect();
    let reactions = sqlx::query_as::<_, (String, String, i64, bool, String)>(
        "SELECT message_id::text, emoji, count(*), bool_or(user_id=$2::uuid),
                json_agg(json_build_object('id', user_id::text, 'pseudo', u.pseudo) ORDER BY rx.created_at)::text
         FROM reactions rx JOIN users u ON u.id = rx.user_id WHERE message_id = ANY($1::bigint[])
         GROUP BY message_id, emoji ORDER BY message_id, min(rx.created_at)",
    )
    .bind(&ids)
    .bind(viewer)
    .fetch_all(pool)
    .await?;
    for (mid, emoji, nombre, moi, users) in reactions {
        let users: Value = serde_json::from_str(&users).unwrap_or(Value::Array(Vec::new()));
        if let Some(l) = liste_mut(&index, messages, &mid, "reactions") {
            l.push(json!({ "emoji": emoji, "count": nombre, "me": moi, "users": users }));
        }
    }
    let mentions = sqlx::query_as::<_, (String, String)>(
        "SELECT message_id::text, user_id::text FROM message_mentions WHERE message_id = ANY($1::bigint[])",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    for (mid, uid) in mentions {
        if let Some(l) = liste_mut(&index, messages, &mid, "mentions") {
            l.push(uid.into());
        }
    }
    sondages::hydrater(pool, messages, &index, &ids, viewer).await?;
    let fichiers = sqlx::query_as::<_, (String, String, String, String, i64, bool, String, i32, Vec<i32>, bool, String, bool)>(
        "SELECT message_id::text, id::text, name, mime, size, purged_at IS NOT NULL, kind, duration_ms, waveform, thumb IS NOT NULL, transcript, transcript_auto
         FROM files WHERE message_id = ANY($1::bigint[]) ORDER BY position, created_at, id",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    for (mid, id, nom, mime, taille, expire, kind, duree, onde, vignette, transcript, auto) in
        fichiers
    {
        let mut a = Map::new();
        a.insert("id".into(), id.into());
        a.insert("name".into(), nom.into());
        a.insert("mime".into(), mime.into());
        a.insert("size".into(), taille.into());
        a.insert("expired".into(), expire.into());
        a.insert("kind".into(), kind.into());
        if duree != 0 {
            a.insert("duration_ms".into(), duree.into());
        }
        if !onde.is_empty() {
            a.insert("waveform".into(), onde.into());
        }
        if vignette {
            a.insert("thumb".into(), true.into());
        }
        if !transcript.is_empty() {
            a.insert("transcript".into(), transcript.into());
        }
        if auto {
            a.insert("transcript_auto".into(), true.into());
        }
        if let Some(l) = liste_mut(&index, messages, &mid, "attachments") {
            l.push(Value::Object(a));
        }
    }
    Ok(())
}

pub async fn message_complet(pool: &PgPool, id: i64, viewer: &str) -> Result<Value, sqlx::Error> {
    let lignes = sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
        "{} WHERE m.id=$1",
        selection()
    )))
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut messages = construire(lignes);
    if messages.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    hydrater(pool, &mut messages, viewer).await?;
    Ok(Value::Object(messages.remove(0)))
}

pub(super) async fn page(
    pool: &PgPool,
    conv: &Conv,
    viewer: &str,
    avant: i64,
    apres: Option<i64>,
    limite: i64,
) -> Result<(Vec<Value>, bool), sqlx::Error> {
    let (colonne, cible) = match &conv.dm_id {
        Some(dm) => ("m.dm_id", dm.clone()),
        None => ("m.channel_id", conv.channel_id.clone().unwrap_or_default()),
    };
    let lignes = match apres {
        Some(apres) => {
            sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
                "{} WHERE {colonne}=$1::uuid AND m.id > $2 ORDER BY m.id LIMIT $3",
                selection()
            )))
            .bind(&cible)
            .bind(apres)
            .bind(limite + 1)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
                "{} WHERE {colonne}=$1::uuid AND ($2::bigint = 0 OR m.id < $2) ORDER BY m.id DESC LIMIT $3",
                selection()
            )))
            .bind(&cible)
            .bind(avant)
            .bind(limite + 1)
            .fetch_all(pool)
            .await?
        }
    };
    let mut messages = construire(lignes);
    let plus = messages.len() as i64 > limite;
    if plus {
        messages.truncate(limite as usize);
    }
    if apres.is_none() {
        messages.reverse();
    }
    hydrater(pool, &mut messages, viewer).await?;
    Ok((messages.into_iter().map(Value::Object).collect(), plus))
}

pub(crate) fn entier_requete(
    requete: &str,
    nom: &str,
    erreur: &str,
    valide: impl Fn(i64) -> bool,
) -> Result<Option<i64>, Response> {
    let brut = premier_parametre(requete, nom);
    if brut.is_empty() {
        return Ok(None);
    }
    match brut.parse::<i64>() {
        Ok(n) if valide(n) => Ok(Some(n)),
        _ => Err(write_err(StatusCode::BAD_REQUEST, "bad_request", erreur)),
    }
}

pub(super) async fn lister(
    state: &AppState,
    u: &User,
    conv: Conv,
    requete: Option<String>,
) -> Result<Response, Response> {
    let pool = pool(state)?;
    let requete = requete.unwrap_or_default();
    let avant = entier_requete(&requete, "before", "before invalide", |n| n >= 0)?.unwrap_or(0);
    let limite = entier_requete(&requete, "limit", "limit : 1 à 100", |n| {
        (1..=100).contains(&n)
    })?
    .unwrap_or(50);
    let apres = entier_requete(&requete, "after", "after invalide", |n| n >= 0)?;
    let (messages, plus) = page(pool, &conv, &u.id, avant, apres, limite)
        .await
        .map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &json!({ "messages": messages, "has_more": plus }),
    ))
}

pub(super) async fn lister_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    lister(&state, &u, conv, requete).await
}

pub(super) async fn lister_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    lister(&state, &u, conv, requete).await
}
