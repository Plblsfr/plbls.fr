use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub async fn purger(
    pool: &PgPool,
    mois: u32,
    maintenant: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let r = sqlx::query(
        "DELETE FROM evenements WHERE GREATEST(date, recu_le) < $1::timestamptz - make_interval(months => $2)",
    )
    .bind(maintenant)
    .bind(i32::try_from(mois).unwrap_or(i32::MAX))
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agregats,
        testutil::{env, instant, jour},
    };

    async fn brut(pool: &PgPool, id: &str, date: &str, recu_le: &str) {
        sqlx::query(
            "INSERT INTO evenements (id, sub, source, type, date, importe, donnees, recu_le)
            VALUES ($1, 'sub-alice', 'sillon', 'sillon.ecoute', $2, false, '{}'::jsonb, $3)",
        )
        .bind(id)
        .bind(instant(date))
        .bind(instant(recu_le))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn restants(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM evenements ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn bruts_de_plus_de_treize_mois_supprimes_agregats_intacts() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let maintenant = instant("2026-11-18T12:00:00Z");
        brut(pool, "vieux", "2025-10-18T11:59:59Z", "2025-10-18T11:59:59Z").await;
        brut(pool, "limite", "2025-10-18T12:00:00Z", "2025-10-18T12:00:00Z").await;
        brut(pool, "recent", "2026-11-17T00:00:00Z", "2026-11-17T00:00:00Z").await;
        brut(pool, "import", "2020-01-01T00:00:00Z", "2026-11-01T00:00:00Z").await;
        brut(pool, "tres-vieux", "2020-01-01T00:00:00Z", "2020-01-01T00:00:01Z").await;
        let vieux_jour = jour("2020-01-01");
        agregats::ajouter(pool, "sub-alice", vieux_jour, agregats::UTILISATION, "web|sillon", 600)
            .await
            .unwrap();
        assert_eq!(purger(pool, 13, maintenant).await.unwrap(), 2);
        assert_eq!(restants(pool).await, ["import", "limite", "recent"]);
        assert_eq!(purger(pool, 13, maintenant).await.unwrap(), 0);
        assert_eq!(
            agregats::somme(pool, "sub-alice", agregats::UTILISATION, vieux_jour, vieux_jour)
                .await
                .unwrap(),
            600
        );
        assert_eq!(purger(pool, 1, maintenant).await.unwrap(), 1);
        assert_eq!(restants(pool).await, ["import", "recent"]);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agregats_jour")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
}
