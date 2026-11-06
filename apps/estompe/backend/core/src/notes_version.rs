use serde::Deserialize;
use sqlx::PgPool;

use crate::messages::SYSTEME;

const BRUT: &str = include_str!("notes_version/changelog.json");

#[derive(Debug, Deserialize)]
pub struct Release {
    pub version: String,
    pub date: String,
    #[serde(default, rename = "new")]
    pub nouveautes: Vec<String>,
    #[serde(default, rename = "fixes")]
    pub correctifs: Vec<String>,
}

pub fn toutes() -> Vec<Release> {
    let mut l: Vec<Release> = serde_json::from_str(BRUT).unwrap_or_default();
    l.reverse();
    l
}

impl Release {
    pub fn markdown(&self) -> String {
        let mut b = format!("**Estompe {}** ({})\n", self.version, self.date);
        for t in &self.nouveautes {
            b.push_str("\n- ");
            b.push_str(t);
        }
        if !self.correctifs.is_empty() {
            b.push_str("\n\nCorrectifs :");
            for t in &self.correctifs {
                b.push_str("\n- ");
                b.push_str(t);
            }
        }
        b
    }
}

pub async fn annoncer(pool: &PgPool, version: &str, contenu: &str) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let inseree =
        sqlx::query("INSERT INTO release_announced (version) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(version)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if inseree == 0 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO dm_conversations (user_a, user_b)
         SELECT LEAST(id, $1::uuid), GREATEST(id, $1::uuid) FROM users WHERE id <> $1::uuid ON CONFLICT DO NOTHING",
    )
    .bind(SYSTEME)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO messages (dm_id, author_id, content)
         SELECT id, $1::uuid, $2 FROM dm_conversations WHERE user_a = $1::uuid OR user_b = $1::uuid",
    )
    .bind(SYSTEME)
    .bind(contenu)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn annoncer_toutes(pool: &PgPool) {
    for r in toutes() {
        match annoncer(pool, &r.version, &r.markdown()).await {
            Ok(true) => tracing::info!("notes de version {} envoyées", r.version),
            Ok(false) => {}
            Err(e) => tracing::warn!("notes de version {} : {e}", r.version),
        }
    }
}

pub async fn envoyees(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT version FROM release_announced ORDER BY sent_at DESC, version DESC LIMIT 30",
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::tests::{Env, env, membre};

    fn release(fixes: &[&str]) -> Release {
        Release {
            version: "1.2.3".into(),
            date: "2026-01-02".into(),
            nouveautes: vec!["Un".into(), "Deux".into()],
            correctifs: fixes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn markdown_identique_au_go() {
        assert_eq!(
            release(&[]).markdown(),
            "**Estompe 1.2.3** (2026-01-02)\n\n- Un\n- Deux"
        );
        assert_eq!(
            release(&["A", "B"]).markdown(),
            "**Estompe 1.2.3** (2026-01-02)\n\n- Un\n- Deux\n\nCorrectifs :\n- A\n- B"
        );
    }

    #[test]
    fn de_la_plus_ancienne_a_la_plus_recente() {
        let l = toutes();
        assert!(l.len() > 1);
        let brut: Vec<Release> = serde_json::from_str(BRUT).unwrap();
        assert_eq!(l.first().unwrap().version, brut.last().unwrap().version);
        assert_eq!(l.last().unwrap().version, brut.first().unwrap().version);
    }

    async fn messages_du_bot(e: &Env, qui: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT m.content FROM messages m JOIN dm_conversations d ON d.id = m.dm_id
             JOIN users u ON u.id IN (d.user_a, d.user_b) WHERE u.pseudo = $1 AND m.author_id = $2::uuid ORDER BY m.id",
        )
        .bind(qui)
        .bind(SYSTEME)
        .fetch_all(e.state.pool().ok().unwrap())
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn une_seule_annonce_par_version() {
        let Some(e) = env().await else { return };
        membre(&e, "alice").await;
        let pool = e.state.pool().ok().unwrap();
        annoncer_toutes(pool).await;
        annoncer_toutes(pool).await;
        let recus = messages_du_bot(&e, "alice").await;
        assert_eq!(recus.len(), toutes().len());
        assert_eq!(recus[0], toutes()[0].markdown());
        let envoyees = envoyees(pool).await.unwrap();
        assert_eq!(envoyees.len(), toutes().len().min(30));
    }

    #[tokio::test]
    async fn version_deja_annoncee_par_le_go() {
        let Some(e) = env().await else { return };
        membre(&e, "alice").await;
        let pool = e.state.pool().ok().unwrap();
        for r in toutes() {
            sqlx::query("INSERT INTO release_announced (version) VALUES ($1)")
                .bind(&r.version)
                .execute(pool)
                .await
                .unwrap();
        }
        annoncer_toutes(pool).await;
        assert!(messages_du_bot(&e, "alice").await.is_empty());
    }

    #[tokio::test]
    async fn pas_d_annonce_retroactive() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool().ok().unwrap();
        membre(&e, "ancienne").await;
        assert!(annoncer(pool, "9.9.9", "Note").await.unwrap());
        membre(&e, "nouvelle").await;
        assert!(!annoncer(pool, "9.9.9", "Note").await.unwrap());
        assert_eq!(messages_du_bot(&e, "ancienne").await, ["Note"]);
        assert!(messages_du_bot(&e, "nouvelle").await.is_empty());
    }
}
