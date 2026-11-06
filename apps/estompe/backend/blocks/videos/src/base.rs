use estompe_core::servers::{horodatage, rfc3339};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row, postgres::PgRow};

pub const POSTS_MAX: i64 = 300;
pub const LISTES_MAX: i64 = 30;
pub const SEANCES_MAX: i64 = 20;

pub enum Echec {
    Introuvable,
    PasUneVideo,
    Plein,
    Sql(sqlx::Error),
}

impl From<sqlx::Error> for Echec {
    fn from(e: sqlx::Error) -> Self {
        Echec::Sql(e)
    }
}

type Resultat<T> = Result<T, Echec>;

fn date(colonne: &str) -> String {
    horodatage(colonne)
}

fn rfc(s: String) -> String {
    rfc3339(&s)
}

pub struct Liste {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub collaborative: bool,
    pub items: Vec<String>,
}

impl Liste {
    pub fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "created_by": self.created_by,
            "collaborative": self.collaborative,
            "items": self.items,
        })
    }
}

pub struct Publication {
    pub provider: String,
    pub reference: String,
}

pub async fn creer_publication(
    pool: &PgPool,
    salon: &str,
    message: &str,
    provider: &str,
    reference: &str,
    url: &str,
    titre: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO video_posts (message_id, channel_id, provider, ref, url, title, site, thumb)
         VALUES ($1::bigint, $2::uuid, $3, $4, $5, $6, '', '')",
    )
    .bind(message)
    .bind(salon)
    .bind(provider)
    .bind(reference)
    .bind(url)
    .bind(titre)
    .execute(pool)
    .await
    .map(|_| ())
}

#[allow(clippy::too_many_arguments)]
pub async fn maj_metadonnees(
    pool: &PgPool,
    message: &str,
    provider: &str,
    reference: &str,
    titre: &str,
    site: &str,
    miniature: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE video_posts SET provider=$2, ref=$3, title=$4, site=$5, thumb=$6 WHERE message_id=$1::bigint",
    )
    .bind(message)
    .bind(provider)
    .bind(reference)
    .bind(titre)
    .bind(site)
    .bind(miniature)
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn miniature(pool: &PgPool, salon: &str, message: &str) -> Resultat<String> {
    sqlx::query_scalar(
        "SELECT thumb FROM video_posts WHERE message_id=$1::bigint AND channel_id=$2::uuid AND thumb <> ''",
    )
    .bind(message)
    .bind(salon)
    .fetch_optional(pool)
    .await?
    .ok_or(Echec::Introuvable)
}

pub async fn marquer_vue(pool: &PgPool, salon: &str, message: &str, vue: bool) -> Resultat<()> {
    let r = sqlx::query(
        "UPDATE video_posts SET watched_at = CASE WHEN $3 THEN now() END WHERE message_id=$1::bigint AND channel_id=$2::uuid",
    )
    .bind(message)
    .bind(salon)
    .bind(vue)
    .execute(pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(Echec::PasUneVideo);
    }
    Ok(())
}

pub async fn publications(
    pool: &PgPool,
    salon: &str,
    spectateur: &str,
) -> Result<Vec<Value>, sqlx::Error> {
    let requete = format!(
        "SELECT v.message_id::text, u.id::text, u.pseudo, {cree}, v.provider, v.ref, v.url, v.title, v.site,
                v.thumb <> '', {vu},
                (SELECT count(*) FROM reactions r WHERE r.message_id = v.message_id),
                EXISTS (SELECT 1 FROM reactions r WHERE r.message_id = v.message_id AND r.user_id = $3::uuid AND r.emoji = '👍'),
                (SELECT count(*) FROM messages a WHERE a.reply_to = v.message_id)
         FROM video_posts v JOIN messages m ON m.id = v.message_id JOIN users u ON u.id = m.author_id
         WHERE v.channel_id=$1::uuid ORDER BY v.message_id DESC LIMIT $2",
        cree = date("m.created_at"),
        vu = date("v.watched_at"),
    );
    let lignes = sqlx::query(sqlx::AssertSqlSafe(requete))
        .bind(salon)
        .bind(POSTS_MAX)
        .bind(spectateur)
        .fetch_all(pool)
        .await?;
    lignes
        .iter()
        .map(|r| {
            Ok(json!({
                "message_id": r.try_get::<String, _>(0)?,
                "author": {"id": r.try_get::<String, _>(1)?, "pseudo": r.try_get::<String, _>(2)?},
                "created_at": rfc(r.try_get(3)?),
                "provider": r.try_get::<String, _>(4)?,
                "ref": r.try_get::<String, _>(5)?,
                "url": r.try_get::<String, _>(6)?,
                "title": r.try_get::<String, _>(7)?,
                "site": r.try_get::<String, _>(8)?,
                "has_thumb": r.try_get::<bool, _>(9)?,
                "watched_at": r.try_get::<Option<String>, _>(10)?.map(rfc),
                "reactions": r.try_get::<i64, _>(11)?,
                "liked": r.try_get::<bool, _>(12)?,
                "replies": r.try_get::<i64, _>(13)?,
            }))
        })
        .collect()
}

pub async fn publications_par_id(
    pool: &PgPool,
    salon: &str,
    ids: &[String],
) -> Result<Vec<Publication>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let lignes: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT message_id::text, provider, ref FROM video_posts WHERE channel_id=$1::uuid AND message_id = ANY($2::bigint[])",
    )
    .bind(salon)
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            lignes
                .iter()
                .find(|(m, _, _)| m == id)
                .map(|(_, p, r)| Publication {
                    provider: p.clone(),
                    reference: r.clone(),
                })
        })
        .collect())
}

fn liste_depuis(r: &PgRow) -> Result<Liste, sqlx::Error> {
    Ok(Liste {
        id: r.try_get(0)?,
        name: r.try_get(1)?,
        created_by: r.try_get(2)?,
        collaborative: r.try_get(3)?,
        items: r.try_get(4)?,
    })
}

pub async fn listes(pool: &PgPool, salon: &str) -> Result<Vec<Value>, sqlx::Error> {
    let lignes = sqlx::query(
        "SELECT l.id::text, l.name, l.created_by::text, l.collaborative,
                COALESCE(array_agg(i.message_id::text ORDER BY i.position) FILTER (WHERE i.message_id IS NOT NULL), '{}')
         FROM video_lists l LEFT JOIN video_list_items i ON i.list_id = l.id
         WHERE l.channel_id=$1::uuid GROUP BY l.id ORDER BY l.created_at",
    )
    .bind(salon)
    .fetch_all(pool)
    .await?;
    lignes
        .iter()
        .map(|r| liste_depuis(r).map(|l| l.json()))
        .collect()
}

pub async fn creer_liste(
    pool: &PgPool,
    salon: &str,
    nom: &str,
    utilisateur: &str,
    collaborative: bool,
) -> Resultat<Liste> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM channels WHERE id=$1::uuid FOR UPDATE")
        .bind(salon)
        .execute(&mut *tx)
        .await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM video_lists WHERE channel_id=$1::uuid")
        .bind(salon)
        .fetch_one(&mut *tx)
        .await?;
    if n >= LISTES_MAX {
        return Err(Echec::Plein);
    }
    let id: String = sqlx::query_scalar(
        "INSERT INTO video_lists (channel_id, name, created_by, collaborative) VALUES ($1::uuid,$2,$3::uuid,$4) RETURNING id::text",
    )
    .bind(salon)
    .bind(nom)
    .bind(utilisateur)
    .bind(collaborative)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Liste {
        id,
        name: nom.to_string(),
        created_by: utilisateur.to_string(),
        collaborative,
        items: Vec::new(),
    })
}

pub async fn liste(pool: &PgPool, salon: &str, id: &str) -> Resultat<Liste> {
    let ligne = sqlx::query(
        "SELECT l.id::text, l.name, l.created_by::text, l.collaborative,
                COALESCE(array_agg(i.message_id::text ORDER BY i.position) FILTER (WHERE i.message_id IS NOT NULL), '{}')
         FROM video_lists l LEFT JOIN video_list_items i ON i.list_id = l.id
         WHERE l.id=$1::uuid AND l.channel_id=$2::uuid GROUP BY l.id",
    )
    .bind(id)
    .bind(salon)
    .fetch_optional(pool)
    .await?
    .ok_or(Echec::Introuvable)?;
    Ok(liste_depuis(&ligne)?)
}

pub async fn renommer_liste(
    pool: &PgPool,
    salon: &str,
    id: &str,
    nom: &str,
    collaborative: Option<bool>,
) -> Resultat<()> {
    let mut tx = pool.begin().await?;
    if let Some(c) = collaborative {
        let r = sqlx::query(
            "UPDATE video_lists SET collaborative=$3 WHERE id=$1::uuid AND channel_id=$2::uuid",
        )
        .bind(id)
        .bind(salon)
        .bind(c)
        .execute(&mut *tx)
        .await?;
        if r.rows_affected() == 0 {
            return Err(Echec::Introuvable);
        }
    }
    let r = sqlx::query("UPDATE video_lists SET name=$3 WHERE id=$1::uuid AND channel_id=$2::uuid")
        .bind(id)
        .bind(salon)
        .bind(nom)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Echec::Introuvable);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn supprimer_liste(pool: &PgPool, salon: &str, id: &str) -> Resultat<()> {
    let r = sqlx::query("DELETE FROM video_lists WHERE id=$1::uuid AND channel_id=$2::uuid")
        .bind(id)
        .bind(salon)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Echec::Introuvable);
    }
    Ok(())
}

pub async fn remplacer_elements(
    pool: &PgPool,
    salon: &str,
    id: &str,
    ids: &[String],
) -> Resultat<()> {
    let mut tx = pool.begin().await?;
    let existe: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM video_lists WHERE id=$1::uuid AND channel_id=$2::uuid)",
    )
    .bind(id)
    .bind(salon)
    .fetch_one(&mut *tx)
    .await?;
    if !existe {
        return Err(Echec::Introuvable);
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT m) FROM unnest($2::bigint[]) m JOIN video_posts v ON v.message_id = m AND v.channel_id=$1::uuid",
    )
    .bind(salon)
    .bind(ids)
    .fetch_one(&mut *tx)
    .await?;
    if n != ids.len() as i64 {
        return Err(Echec::PasUneVideo);
    }
    sqlx::query("DELETE FROM video_list_items WHERE list_id=$1::uuid")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO video_list_items (list_id, message_id, position)
         SELECT $1::uuid, m, p - 1 FROM unnest($2::bigint[]) WITH ORDINALITY AS t(m, p)",
    )
    .bind(id)
    .bind(ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub struct Seance {
    pub id: String,
    pub title: String,
    pub starts_at: String,
    pub voice_channel_id: String,
    pub remind_minutes: i32,
    pub videos: Vec<String>,
    pub created_by: String,
    pub started_at: Option<String>,
    pub rsvps: Map<String, Value>,
}

impl Seance {
    pub fn json(&self) -> Value {
        json!({
            "id": self.id,
            "title": self.title,
            "starts_at": self.starts_at,
            "voice_channel_id": self.voice_channel_id,
            "remind_minutes": self.remind_minutes,
            "videos": self.videos,
            "created_by": self.created_by,
            "started_at": self.started_at,
            "rsvps": self.rsvps,
        })
    }
}

fn selection_seance() -> String {
    format!(
        "SELECT id::text, title, {debut}, voice_channel_id::text, remind_minutes, videos::text[], created_by::text, {lance} FROM watch_sessions",
        debut = date("starts_at"),
        lance = date("started_at"),
    )
}

async fn seances_depuis(pool: &PgPool, lignes: Vec<PgRow>) -> Result<Vec<Seance>, sqlx::Error> {
    let mut sortie = Vec::new();
    for r in &lignes {
        sortie.push(Seance {
            id: r.try_get(0)?,
            title: r.try_get(1)?,
            starts_at: rfc(r.try_get(2)?),
            voice_channel_id: r.try_get(3)?,
            remind_minutes: r.try_get(4)?,
            videos: r.try_get(5)?,
            created_by: r.try_get(6)?,
            started_at: r.try_get::<Option<String>, _>(7)?.map(rfc),
            rsvps: Map::new(),
        });
    }
    let ids: Vec<String> = sortie.iter().map(|s| s.id.clone()).collect();
    let reponses: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT session_id::text, user_id::text, answer FROM watch_rsvps WHERE session_id = ANY($1::uuid[])",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    for (session, utilisateur, reponse) in reponses {
        if let Some(s) = sortie.iter_mut().find(|s| s.id == session) {
            s.rsvps.insert(utilisateur, Value::String(reponse));
        }
    }
    Ok(sortie)
}

pub async fn seances(pool: &PgPool, salon: &str) -> Result<Vec<Value>, sqlx::Error> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{} WHERE channel_id=$1::uuid AND starts_at > now() - interval '3 hours' ORDER BY starts_at LIMIT $2",
        selection_seance()
    )))
    .bind(salon)
    .bind(SEANCES_MAX)
    .fetch_all(pool)
    .await?;
    Ok(seances_depuis(pool, lignes)
        .await?
        .iter()
        .map(Seance::json)
        .collect())
}

pub async fn seance(pool: &PgPool, salon: &str, id: &str) -> Resultat<Seance> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{} WHERE id=$1::uuid AND channel_id=$2::uuid",
        selection_seance()
    )))
    .bind(id)
    .bind(salon)
    .fetch_all(pool)
    .await?;
    seances_depuis(pool, lignes)
        .await?
        .into_iter()
        .next()
        .ok_or(Echec::Introuvable)
}

pub struct NouvelleSeance<'a> {
    pub titre: &'a str,
    pub debut: &'a str,
    pub vocal: &'a str,
    pub rappel: i32,
    pub videos: &'a [String],
    pub createur: &'a str,
}

pub async fn creer_seance(pool: &PgPool, salon: &str, n: &NouvelleSeance<'_>) -> Resultat<Seance> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM channels WHERE id=$1::uuid FOR UPDATE")
        .bind(salon)
        .execute(&mut *tx)
        .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM watch_sessions WHERE channel_id=$1::uuid AND started_at IS NULL",
    )
    .bind(salon)
    .fetch_one(&mut *tx)
    .await?;
    if total >= SEANCES_MAX {
        return Err(Echec::Plein);
    }
    let (id, debut): (String, String) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO watch_sessions (channel_id, voice_channel_id, title, starts_at, remind_minutes, videos, created_by)
         VALUES ($1::uuid,$2::uuid,$3,$4::timestamptz,$5,$6::bigint[],$7::uuid) RETURNING id::text, {}",
        date("starts_at")
    )))
    .bind(salon)
    .bind(n.vocal)
    .bind(n.titre)
    .bind(n.debut)
    .bind(n.rappel)
    .bind(n.videos)
    .bind(n.createur)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO watch_rsvps (session_id, user_id, answer) VALUES ($1::uuid,$2::uuid,'yes')",
    )
    .bind(&id)
    .bind(n.createur)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut rsvps = Map::new();
    rsvps.insert(n.createur.to_string(), Value::String("yes".into()));
    Ok(Seance {
        id,
        title: n.titre.to_string(),
        starts_at: rfc(debut),
        voice_channel_id: n.vocal.to_string(),
        remind_minutes: n.rappel,
        videos: n.videos.to_vec(),
        created_by: n.createur.to_string(),
        started_at: None,
        rsvps,
    })
}

pub async fn supprimer_seance(pool: &PgPool, salon: &str, id: &str) -> Resultat<()> {
    let r = sqlx::query("DELETE FROM watch_sessions WHERE id=$1::uuid AND channel_id=$2::uuid")
        .bind(id)
        .bind(salon)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Echec::Introuvable);
    }
    Ok(())
}

pub async fn repondre(
    pool: &PgPool,
    salon: &str,
    id: &str,
    utilisateur: &str,
    reponse: &str,
) -> Resultat<()> {
    let r = sqlx::query(
        "INSERT INTO watch_rsvps (session_id, user_id, answer)
         SELECT id, $3::uuid, $4 FROM watch_sessions WHERE id=$1::uuid AND channel_id=$2::uuid
         ON CONFLICT (session_id, user_id) DO UPDATE SET answer = EXCLUDED.answer",
    )
    .bind(id)
    .bind(salon)
    .bind(utilisateur)
    .bind(reponse)
    .execute(pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(Echec::Introuvable);
    }
    Ok(())
}

pub struct Due {
    pub id: String,
    pub salon: String,
    pub vocal: String,
    pub titre: String,
    pub dans: f64,
    pub videos: Vec<String>,
    pub presents: Vec<String>,
}

type LigneDue = (String, String, String, String, f64, Vec<String>);

async fn dues(pool: &PgPool, requete: &'static str) -> Result<Vec<Due>, sqlx::Error> {
    let lignes: Vec<LigneDue> = sqlx::query_as(requete).fetch_all(pool).await?;
    let mut sortie = Vec::with_capacity(lignes.len());
    for (id, salon, vocal, titre, dans, videos) in lignes {
        let presents = sqlx::query_scalar(
            "SELECT user_id::text FROM watch_rsvps WHERE session_id=$1::uuid AND answer IN ('yes','maybe')",
        )
        .bind(&id)
        .fetch_all(pool)
        .await?;
        sortie.push(Due {
            id,
            salon,
            vocal,
            titre,
            dans,
            videos,
            presents,
        });
    }
    Ok(sortie)
}

pub async fn rappels_dus(pool: &PgPool) -> Result<Vec<Due>, sqlx::Error> {
    dues(
        pool,
        "UPDATE watch_sessions SET reminded_at = now()
        WHERE started_at IS NULL AND reminded_at IS NULL AND starts_at > now() AND starts_at - make_interval(mins => remind_minutes) <= now()
        RETURNING id::text, channel_id::text, voice_channel_id::text, title, extract(epoch FROM starts_at - now())::float8, videos::text[]",
    )
    .await
}

pub async fn demarrages_dus(pool: &PgPool) -> Result<Vec<Due>, sqlx::Error> {
    dues(
        pool,
        "SELECT id::text, channel_id::text, voice_channel_id::text, title, extract(epoch FROM starts_at - now())::float8, videos::text[] FROM watch_sessions
        WHERE started_at IS NULL AND starts_at <= now() AND starts_at > now() - interval '2 hours'",
    )
    .await
}

pub async fn reclamer_demarrage(pool: &PgPool, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE watch_sessions SET started_at = now() WHERE id=$1::uuid AND started_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn marquer_ecoutees(
    pool: &PgPool,
    salon: &str,
    ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE video_posts SET watched_at = now() WHERE channel_id=$1::uuid AND message_id = ANY($2::bigint[]) AND watched_at IS NULL",
    )
    .bind(salon)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn serveur_et_genre(
    pool: &PgPool,
    salon: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT server_id::text, kind FROM channels WHERE id=$1::uuid")
        .bind(salon)
        .fetch_optional(pool)
        .await
}
