use super::*;

pub(super) fn re_mention() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:^|[^A-Za-z0-9_.-])@([A-Za-z0-9_.-]{3,20})").expect("motif invalide")
    })
}

pub fn pseudos_cites(contenu: &str) -> Vec<String> {
    let mut vus = HashSet::new();
    let mut sortie = Vec::new();
    for c in re_mention().captures_iter(contenu) {
        let p = c[1].trim_end_matches(['.', '-']).to_ascii_lowercase();
        if p.len() >= 3 && vus.insert(p.clone()) {
            sortie.push(p);
        }
    }
    sortie
}

pub(super) fn citees(contenu: &str, noms: &[&str]) -> Vec<usize> {
    let motifs = noms.iter().map(|nom| {
        format!(
            r"(?i)(?:^|[^A-Za-z0-9_.-])@{}(?:$|[^\p{{L}}\p{{N}}_])",
            regex::escape(nom)
        )
    });
    RegexSet::new(motifs).map_or_else(
        |e| {
            tracing::warn!(erreur = %e, "motifs de mention invalides");
            Vec::new()
        },
        |set| set.matches(contenu).into_iter().collect(),
    )
}

pub(super) async fn voyeurs_des_mentions(
    pool: &PgPool,
    conv: &Conv,
    contenu: &str,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    match (&conv.channel_id, &conv.server_id) {
        (Some(salon), Some(serveur)) if contenu.contains('@') => {
            roles::voyeurs_du_salon(pool, serveur, salon)
                .await
                .map(Some)
        }
        _ => Ok(None),
    }
}

pub(super) fn re_code() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)```.*?(?:```|$)|`[^`\n]*`").expect("motif invalide"))
}

pub(super) fn sans_code(contenu: &str) -> std::borrow::Cow<'_, str> {
    re_code().replace_all(contenu, " ")
}

pub(super) fn groupes_cites(contenu: &str) -> (bool, bool) {
    if !contenu.contains('@') {
        return (false, false);
    }
    let c = citees(contenu, &["everyone", "here"]);
    (c.contains(&0), c.contains(&1))
}

pub(super) const DELAI_GROUPE_SECONDES: f64 = 120.0;

pub(super) async fn debit_groupe_depasse(
    pool: &PgPool,
    conv: &Conv,
    auteur: &str,
    contenu: &str,
    exclu: i64,
) -> Result<bool, sqlx::Error> {
    let Some(salon) = &conv.channel_id else {
        return Ok(false);
    };
    let (tous, ici) = groupes_cites(&sans_code(contenu));
    if !(tous || ici)
        || !roles::contient(conv.perms, roles::MENTION_EVERYONE)
        || roles::contient(conv.perms, roles::MANAGE_MESSAGES)
    {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM messages WHERE channel_id=$1::uuid AND author_id=$2::uuid AND id<>$3
         AND created_at > now() - make_interval(secs => $4) AND content ~* '@(everyone|here)')",
    )
    .bind(salon)
    .bind(auteur)
    .bind(exclu)
    .bind(DELAI_GROUPE_SECONDES)
    .fetch_one(pool)
    .await
}

pub(super) async fn cibles_des_mentions(
    tx: &mut PgConnection,
    hub: &crate::hub::Hub,
    voyeurs: Option<&[String]>,
    conv: &Conv,
    auteur: &str,
    contenu: &str,
) -> Result<Vec<(String, bool)>, sqlx::Error> {
    let contenu = &*sans_code(contenu);
    let mut ids: Vec<String> = Vec::new();
    let mut groupe: Vec<String> = Vec::new();
    let noms = pseudos_cites(contenu);
    if !noms.is_empty() {
        let trouves: Vec<String> = if let Some(serveur) = &conv.server_id {
            sqlx::query_scalar(
                "SELECT u.id::text FROM users u JOIN server_members m ON m.user_id = u.id
                 WHERE m.server_id=$1::uuid AND lower(u.pseudo) = ANY($2) AND u.id <> $3::uuid",
            )
            .bind(serveur)
            .bind(&noms)
            .bind(auteur)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT u.id::text FROM users u JOIN dm_conversations d ON u.id IN (d.user_a, d.user_b)
                 WHERE d.id=$1::uuid AND lower(u.pseudo) = ANY($2) AND u.id <> $3::uuid",
            )
            .bind(conv.dm_id.clone().unwrap_or_default())
            .bind(&noms)
            .bind(auteur)
            .fetch_all(&mut *tx)
            .await?
        };
        ids.extend(trouves);
    }
    if let (Some(_), Some(serveur)) = (&conv.channel_id, &conv.server_id)
        && roles::contient(conv.perms, roles::MENTION_EVERYONE)
        && contenu.contains('@')
    {
        let (tous, ici) = groupes_cites(contenu);
        if tous || ici {
            let membres: Vec<String> = sqlx::query_scalar(
                "SELECT user_id::text FROM server_members WHERE server_id=$1::uuid AND user_id <> $2::uuid",
            )
            .bind(serveur)
            .bind(auteur)
            .fetch_all(&mut *tx)
            .await?;
            groupe = if tous {
                membres
            } else {
                let vus: HashSet<&String> = voyeurs.iter().flat_map(|v| v.iter()).collect();
                let candidats: Vec<String> = membres
                    .into_iter()
                    .filter(|m| voyeurs.is_none() || vus.contains(m))
                    .collect();
                hub.online_among(&candidats)
            };
        }
        if !tous {
            let roles_du_serveur: Vec<(String, String)> = sqlx::query_as(
                "SELECT id::text, name FROM roles WHERE server_id=$1::uuid AND NOT is_default",
            )
            .bind(serveur)
            .fetch_all(&mut *tx)
            .await?;
            let noms: Vec<&str> = roles_du_serveur.iter().map(|(_, n)| n.as_str()).collect();
            let cites: Vec<String> = citees(contenu, &noms)
                .into_iter()
                .map(|i| roles_du_serveur[i].0.clone())
                .collect();
            if !cites.is_empty() {
                ids.extend(
                    sqlx::query_scalar::<_, String>(
                        "SELECT user_id::text FROM member_roles WHERE server_id=$1::uuid AND role_id::text = ANY($2) AND user_id <> $3::uuid",
                    )
                    .bind(serveur)
                    .bind(&cites)
                    .bind(auteur)
                    .fetch_all(&mut *tx)
                    .await?,
                );
            }
        }
    }
    let nominatifs: HashSet<String> = ids.iter().cloned().collect();
    let mut cibles: Vec<(String, bool)> = ids
        .into_iter()
        .map(|i| (i, false))
        .chain(groupe.into_iter().map(|i| {
            let seul = !nominatifs.contains(&i);
            (i, seul)
        }))
        .collect();
    if let Some(voyeurs) = voyeurs {
        let vus: HashSet<&String> = voyeurs.iter().collect();
        cibles.retain(|(id, _)| vus.contains(id));
    }
    cibles.sort();
    cibles.dedup_by(|b, a| a.0 == b.0);
    Ok(cibles)
}
