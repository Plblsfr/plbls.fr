use super::*;

pub(crate) enum Echec {
    Reponse,
    Fichiers,
    Vocal,
    Interdit,
    Debit,
    Base(sqlx::Error),
}

impl From<sqlx::Error> for Echec {
    fn from(e: sqlx::Error) -> Self {
        Echec::Base(e)
    }
}

pub(crate) async fn attacher_fichiers(
    tx: &mut PgConnection,
    message: i64,
    auteur: &str,
    contenu: &str,
    fichiers: &[String],
) -> Result<(), Echec> {
    if fichiers.is_empty() {
        return Ok(());
    }
    let uniques: HashSet<&String> = fichiers.iter().collect();
    if uniques.len() != fichiers.len() {
        return Err(Echec::Fichiers);
    }
    let kinds: Vec<String> = sqlx::query_scalar(
        "UPDATE files SET message_id=$1, position=array_position($2::uuid[], id) WHERE id = ANY($2::uuid[])
         AND (message_id=$1 OR (uploader_id=$3::uuid AND message_id IS NULL AND report_id IS NULL AND purged_at IS NULL)) RETURNING kind",
    )
    .bind(message)
    .bind(fichiers)
    .bind(auteur)
    .fetch_all(&mut *tx)
    .await?;
    if kinds.len() != fichiers.len() {
        return Err(Echec::Fichiers);
    }
    let voix = kinds.iter().filter(|k| *k == "voice").count();
    if voix > 0 && (voix != 1 || kinds.len() != 1 || !contenu.is_empty()) {
        return Err(Echec::Vocal);
    }
    Ok(())
}

pub(super) fn echec(e: Echec, fichiers_msg: &str) -> Response {
    match e {
        Echec::Reponse => write_err(
            StatusCode::BAD_REQUEST,
            "invalid_reply",
            "on ne peut répondre qu'à un message de la même conversation",
        ),
        Echec::Fichiers => write_err(StatusCode::BAD_REQUEST, "invalid_files", fichiers_msg),
        Echec::Vocal => write_err(
            StatusCode::BAD_REQUEST,
            "invalid_voice",
            "un message vocal s'envoie seul, sans texte ni autre fichier",
        ),
        Echec::Interdit => write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce message n'est pas modifiable",
        ),
        Echec::Debit => write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "une mention @everyone ou @here par salon toutes les 2 minutes",
        ),
        Echec::Base(e) => bd(e),
    }
}

pub(super) const FICHIERS_INTROUVABLES: &str =
    "fichier introuvable, déjà utilisé ou non envoyé par vous";

pub(crate) fn contenu_valide(s: &str, avec_fichiers: bool) -> Option<String> {
    let s = s.trim();
    (s.chars().count() <= LONGUEUR_MAX && (!s.is_empty() || avec_fichiers)).then(|| s.to_string())
}

pub(super) fn texte_strict(e: &Map<String, Value>, nom: &str) -> Option<String> {
    match e.get(nom) {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

pub(crate) fn liste_stricte(e: &Map<String, Value>, nom: &str) -> Result<Option<Vec<String>>, ()> {
    match e.get(nom) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .map(Some)
            .ok_or(()),
        Some(_) => Err(()),
    }
}

pub(super) async fn smileys_perso(pool: &PgPool, user: &str) -> HashSet<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT e.name FROM custom_emojis e JOIN server_members m ON m.server_id = e.server_id WHERE m.user_id=$1::uuid",
    )
    .bind(user)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect()
}

pub(super) async fn convertir_emojis(pool: &PgPool, user: &str, contenu: &str) -> String {
    if !contenu.contains(':') {
        return contenu.to_string();
    }
    emoji::convertir(contenu, &smileys_perso(pool, user).await)
}

pub(crate) fn trop_de_requetes(secondes: u64) -> Response {
    avec_reessai(
        write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "trop de requêtes, patientez avant de réessayer",
        ),
        secondes,
    )
}

pub(crate) fn avec_reessai(mut r: Response, secondes: u64) -> Response {
    if let Ok(v) = HeaderValue::from_str(&secondes.to_string()) {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

pub(super) async fn envoyer(
    state: &AppState,
    u: &User,
    conv: Conv,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(state)?;
    if !conv.peut(roles::SEND_MESSAGES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez pas écrire dans ce salon",
        ));
    }
    if conv.slowmode > 0
        && !conv.peut(roles::MANAGE_MESSAGES)
        && let Some(salon) = &conv.channel_id
    {
        let attente: Option<f64> = sqlx::query_scalar(
            "SELECT $3::float8 - extract(epoch FROM now() - max(created_at))::float8 FROM messages WHERE channel_id=$1::uuid AND author_id=$2::uuid HAVING max(created_at) IS NOT NULL",
        )
        .bind(salon)
        .bind(&u.id)
        .bind(f64::from(conv.slowmode))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some(reste) = attente
            && reste > 0.0
        {
            return Ok(avec_reessai(
                write_err(
                    StatusCode::TOO_MANY_REQUESTS,
                    "slowmode",
                    "mode lent : patientez avant d'écrire à nouveau",
                ),
                reste as u64 + 1,
            ));
        }
    }
    if !state.limiteurs().envoi.allow(&u.id, ENVOIS, FENETRE_ENVOI) {
        return Ok(avec_reessai(
            write_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_messages",
                "trop de messages envoyés, patientez quelques secondes",
            ),
            10,
        ));
    }
    let e = decode(body).await?;
    let contenu_brut = texte_strict(&e, "content").ok_or_else(mauvais)?;
    let fichiers = liste_stricte(&e, "file_ids")
        .map_err(|()| mauvais())?
        .unwrap_or_default();
    let reponse = texte_strict(&e, "reply_to").ok_or_else(mauvais)?;
    let sondage = match e.get("poll") {
        None | Some(Value::Null) => None,
        Some(Value::Object(o)) => Some(sondages::lire_entree(o).ok_or_else(mauvais)?),
        Some(_) => return Err(mauvais()),
    };
    let reponse_id = if reponse.is_empty() {
        None
    } else {
        match reponse.parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => {
                return Ok(write_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_reply",
                    "réponse invalide",
                ));
            }
        }
    };
    let Some(contenu) = contenu_valide(&contenu_brut, !fichiers.is_empty()) else {
        return Ok(message_invalide());
    };
    if fichiers.len() > PIECES_JOINTES_MAX {
        return Ok(message_invalide());
    }
    if (!fichiers.is_empty() && !conv.peut(roles::ATTACH_FILES))
        || (sondage.is_some() && !conv.peut(roles::CREATE_POLLS))
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante dans ce salon",
        ));
    }
    let contenu = convertir_emojis(pool, &u.id, &contenu).await;
    if fichiers.iter().any(|f| !uuid_valide(f)) {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_files",
            "fichier invalide",
        ));
    }
    let mut auteur = u.id.clone();
    let mut nouveau_sondage = None;
    if let Some(s) = sondage {
        let Some(options) = sondages::options_valides(&s.options) else {
            return Ok(sondages::sondage_invalide());
        };
        if contenu.is_empty() || !fichiers.is_empty() {
            return Ok(sondages::sondage_invalide());
        }
        if let Some(c) = &s.fin
            && !sondages::dans_le_futur(pool, c).await.map_err(bd)?
        {
            return Ok(sondages::sondage_invalide());
        }
        auteur = SYSTEME.to_string();
        nouveau_sondage = Some(sondages::NouveauSondage {
            options,
            multiple: s.multiple,
            fin: s.fin,
            lanceur: u.id.clone(),
        });
    }
    let (id, cites) = match creer(
        pool,
        state.hub(),
        &conv,
        &auteur,
        &contenu,
        &fichiers,
        reponse_id,
        nouveau_sondage,
    )
    .await
    {
        Ok(r) => r,
        Err(x) => return Ok(echec(x, FICHIERS_INTROUVABLES)),
    };
    let message = message_complet(pool, id, &auteur).await.map_err(bd)?;
    publier(state, pool, &conv, "message.created", &message).await;
    if conv.kind == "videos" || conv.kind == "music" {
        state.apres_message_video(&conv, &message).await;
    }
    publier_mentions(state, &cites, &message);
    let groupe = ids_groupe(&cites);
    let mut a_notifier: Vec<String> = cites.into_iter().map(|(c, _)| c).collect();
    if let Some(r) = message["reply_to"]["author"]["id"].as_str()
        && r != u.id
    {
        a_notifier.push(r.to_string());
    }
    state
        .notifier(&EvenementMessage {
            conv: conv.clone(),
            message: message.clone(),
            mentionnes: a_notifier,
            groupe,
            a: Vec::new(),
            ..Default::default()
        })
        .await;
    Ok(write_json(StatusCode::CREATED, &message))
}

pub(super) fn ids_groupe(cibles: &[(String, bool)]) -> Vec<String> {
    cibles
        .iter()
        .filter(|(_, g)| *g)
        .map(|(c, _)| c.clone())
        .collect()
}

pub(super) fn publier_mentions(state: &AppState, cibles: &[(String, bool)], message: &Value) {
    let mut groupe = message.clone();
    groupe["group_mention"] = Value::Bool(true);
    for (c, g) in cibles {
        state.publier(
            std::slice::from_ref(c),
            "mention",
            if *g { &groupe } else { message },
        );
    }
}

pub(super) fn message_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_message",
        "message : 1 à 4000 caractères (vide accepté avec fichier), 10 fichiers maximum",
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn creer(
    pool: &PgPool,
    hub: &crate::hub::Hub,
    conv: &Conv,
    auteur: &str,
    contenu: &str,
    fichiers: &[String],
    reponse: Option<i64>,
    sondage: Option<sondages::NouveauSondage>,
) -> Result<(i64, Vec<(String, bool)>), Echec> {
    let voyeurs = voyeurs_des_mentions(pool, conv, contenu).await?;
    if debit_groupe_depasse(pool, conv, auteur, contenu, 0).await? {
        return Err(Echec::Debit);
    }
    let mut tx = pool.begin().await?;
    if let Some(r) = reponse {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE id=$1 AND channel_id IS NOT DISTINCT FROM $2::uuid AND dm_id IS NOT DISTINCT FROM $3::uuid)",
        )
        .bind(r)
        .bind(&conv.channel_id)
        .bind(&conv.dm_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| Echec::Reponse)?;
        if !ok {
            return Err(Echec::Reponse);
        }
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO messages (channel_id, dm_id, author_id, content, reply_to) VALUES ($1::uuid,$2::uuid,$3::uuid,$4,$5) RETURNING id",
    )
    .bind(&conv.channel_id)
    .bind(&conv.dm_id)
    .bind(auteur)
    .bind(contenu)
    .bind(reponse)
    .fetch_one(&mut *tx)
    .await?;
    attacher_fichiers(&mut tx, id, auteur, contenu, fichiers).await?;
    if let Some(s) = sondage {
        sondages::inserer(&mut tx, id, &s).await?;
    }
    let cibles =
        cibles_des_mentions(&mut tx, hub, voyeurs.as_deref(), conv, auteur, contenu).await?;
    for (c, groupe) in &cibles {
        sqlx::query(
            "INSERT INTO message_mentions (message_id, user_id, groupe) VALUES ($1,$2::uuid,$3)",
        )
        .bind(id)
        .bind(c)
        .bind(groupe)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((id, cibles))
}

pub(crate) async fn envoyer_systeme(
    state: &AppState,
    pool: &PgPool,
    dm_id: &str,
    contenu: &str,
) -> Result<(), sqlx::Error> {
    let conv = Conv {
        channel_id: None,
        server_id: None,
        dm_id: Some(dm_id.to_string()),
        kind: String::new(),
        slowmode: 0,
        user_limit: 0,
        perms: 0,
    };
    let (id, _) = match creer(pool, state.hub(), &conv, SYSTEME, contenu, &[], None, None).await {
        Ok(r) => r,
        Err(Echec::Base(e)) => return Err(e),
        Err(_) => {
            tracing::error!("message système : échec inattendu");
            return Err(sqlx::Error::RowNotFound);
        }
    };
    let message = message_complet(pool, id, SYSTEME).await?;
    publier(state, pool, &conv, "message.created", &message).await;
    Ok(())
}

pub(super) async fn envoyer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    envoyer(&state, &u, conv, body).await
}

pub(super) async fn envoyer_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let conv = conv_du_dm(pool, param(&p.0, "id"), &u.id).await?;
    if let Ok(to) = participants(pool, &conv).await
        && to.iter().any(|t| t == SYSTEME)
        && u.id != SYSTEME
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "read_only",
            "ce compte n'accepte pas de messages",
        ));
    }
    envoyer(&state, &u, conv, body).await
}
