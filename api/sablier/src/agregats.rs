use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgExecutor, PgPool};

pub const UTILISATION: &str = "utilisation.secondes";

pub async fn verifier_fuseau(pool: &PgPool, fuseau: &str) -> Result<(), String> {
    let connu: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
            .bind(fuseau)
            .fetch_one(pool)
            .await
            .map_err(|e| format!("base injoignable : {e}"))?;
    if connu {
        Ok(())
    } else {
        Err(format!("SABLIER_FUSEAU inconnu de PostgreSQL : {fuseau}"))
    }
}

pub async fn jour_de(
    executeur: impl PgExecutor<'_>,
    fuseau: &str,
    instant: DateTime<Utc>,
) -> Result<NaiveDate, sqlx::Error> {
    sqlx::query_scalar("SELECT ($1::timestamptz AT TIME ZONE $2)::date")
        .bind(instant)
        .bind(fuseau)
        .fetch_one(executeur)
        .await
}

pub async fn ajouter(
    executeur: impl PgExecutor<'_>,
    sub: &str,
    jour: NaiveDate,
    stat: &str,
    cle: &str,
    n: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agregats_jour (sub, jour, stat, cle, valeur) VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (sub, jour, stat, cle) DO UPDATE SET valeur = agregats_jour.valeur + EXCLUDED.valeur",
    )
    .bind(sub)
    .bind(jour)
    .bind(stat)
    .bind(cle)
    .bind(n)
    .execute(executeur)
    .await?;
    Ok(())
}

pub async fn maximum(
    executeur: impl PgExecutor<'_>,
    sub: &str,
    jour: NaiveDate,
    stat: &str,
    cle: &str,
    n: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agregats_jour (sub, jour, stat, cle, valeur) VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (sub, jour, stat, cle) DO UPDATE SET valeur = GREATEST(agregats_jour.valeur, EXCLUDED.valeur)",
    )
    .bind(sub)
    .bind(jour)
    .bind(stat)
    .bind(cle)
    .bind(n)
    .execute(executeur)
    .await?;
    Ok(())
}

pub async fn somme(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(valeur), 0)::bigint FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_one(pool)
    .await
}

pub async fn par_cle(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT cle, SUM(valeur)::bigint AS total FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4
        GROUP BY cle ORDER BY total DESC, cle",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_all(pool)
    .await
}

pub async fn par_jour(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT jour, SUM(valeur)::bigint FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4
        GROUP BY jour ORDER BY jour",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_all(pool)
    .await
}

pub async fn max_jour(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Option<(NaiveDate, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT jour, SUM(valeur)::bigint AS total FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4
        GROUP BY jour HAVING SUM(valeur) > 0 ORDER BY total DESC, jour LIMIT 1",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_optional(pool)
    .await
}

pub async fn plus_grande(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(valeur), 0)::bigint FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_one(pool)
    .await
}

pub async fn cles_distinctes(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(DISTINCT cle) FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4 AND valeur > 0",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_one(pool)
    .await
}

pub async fn jours_actifs(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(DISTINCT jour) FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4 AND valeur > 0",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_one(pool)
    .await
}

pub async fn cles_par_jour(
    pool: &PgPool,
    sub: &str,
    stat: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Vec<(NaiveDate, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT jour, cle FROM agregats_jour
        WHERE sub = $1 AND stat = $2 AND jour BETWEEN $3 AND $4 AND valeur > 0
        ORDER BY jour, cle",
    )
    .bind(sub)
    .bind(stat)
    .bind(du)
    .bind(au)
    .fetch_all(pool)
    .await
}

pub async fn premier_jour(pool: &PgPool, sub: &str) -> Result<Option<NaiveDate>, sqlx::Error> {
    sqlx::query_scalar("SELECT MIN(jour) FROM agregats_jour WHERE sub = $1 AND valeur > 0")
        .bind(sub)
        .fetch_one(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{env, instant, jour};

    #[tokio::test]
    async fn sommes_maximums_et_lectures() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let (j1, j2, j3) = (jour("2026-03-01"), jour("2026-03-02"), jour("2026-03-09"));
        assert_eq!(premier_jour(pool, "a").await.unwrap(), None);
        assert_eq!(max_jour(pool, "a", "s", j1, j3).await.unwrap(), None);
        let mut tx = pool.begin().await.unwrap();
        ajouter(&mut *tx, "a", j2, "s", "x", 10).await.unwrap();
        ajouter(&mut *tx, "a", j2, "s", "x", 5).await.unwrap();
        ajouter(&mut *tx, "a", j2, "s", "y", 20).await.unwrap();
        ajouter(&mut *tx, "a", j3, "s", "x", 30).await.unwrap();
        ajouter(&mut *tx, "a", j3, "autre", "", 1000).await.unwrap();
        ajouter(&mut *tx, "b", j1, "s", "x", 7).await.unwrap();
        maximum(&mut *tx, "a", j2, "m", "", 4).await.unwrap();
        maximum(&mut *tx, "a", j2, "m", "", 9).await.unwrap();
        maximum(&mut *tx, "a", j2, "m", "", 6).await.unwrap();
        maximum(&mut *tx, "a", j3, "m", "", 7).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(somme(pool, "a", "s", j1, j3).await.unwrap(), 65);
        assert_eq!(somme(pool, "a", "s", j1, j2).await.unwrap(), 35);
        assert_eq!(somme(pool, "a", "m", j1, j2).await.unwrap(), 9);
        assert_eq!(plus_grande(pool, "a", "m", j1, j3).await.unwrap(), 9);
        assert_eq!(plus_grande(pool, "a", "m", j3, j3).await.unwrap(), 7);
        assert_eq!(plus_grande(pool, "a", "m", j1, j1).await.unwrap(), 0);
        assert_eq!(cles_distinctes(pool, "a", "s", j1, j3).await.unwrap(), 2);
        assert_eq!(cles_distinctes(pool, "a", "s", j3, j3).await.unwrap(), 1);
        assert_eq!(cles_distinctes(pool, "c", "s", j1, j3).await.unwrap(), 0);
        assert_eq!(somme(pool, "c", "s", j1, j3).await.unwrap(), 0);
        assert_eq!(
            par_cle(pool, "a", "s", j1, j3).await.unwrap(),
            [("x".to_string(), 45), ("y".to_string(), 20)]
        );
        assert_eq!(
            par_jour(pool, "a", "s", j1, j3).await.unwrap(),
            [(j2, 35), (j3, 30)]
        );
        assert_eq!(
            max_jour(pool, "a", "s", j1, j3).await.unwrap(),
            Some((j2, 35))
        );
        assert_eq!(
            max_jour(pool, "a", "s", j3, j3).await.unwrap(),
            Some((j3, 30))
        );
        assert_eq!(jours_actifs(pool, "a", "s", j1, j3).await.unwrap(), 2);
        assert_eq!(jours_actifs(pool, "a", "s", j1, j1).await.unwrap(), 0);
        assert_eq!(premier_jour(pool, "a").await.unwrap(), Some(j2));
        assert_eq!(premier_jour(pool, "b").await.unwrap(), Some(j1));
    }

    #[tokio::test]
    async fn jour_dans_le_fuseau_y_compris_aux_changements_d_heure() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        for (utc, attendu) in [
            ("2026-01-15T22:59:59Z", "2026-01-15"),
            ("2026-01-15T23:00:00Z", "2026-01-16"),
            ("2026-03-28T22:59:59Z", "2026-03-28"),
            ("2026-03-28T23:00:00Z", "2026-03-29"),
            ("2026-03-29T00:59:59Z", "2026-03-29"),
            ("2026-03-29T01:00:00Z", "2026-03-29"),
            ("2026-03-29T21:59:59Z", "2026-03-29"),
            ("2026-03-29T22:00:00Z", "2026-03-30"),
            ("2026-10-24T21:59:59Z", "2026-10-24"),
            ("2026-10-24T22:00:00Z", "2026-10-25"),
            ("2026-10-25T00:30:00Z", "2026-10-25"),
            ("2026-10-25T01:30:00Z", "2026-10-25"),
            ("2026-10-25T22:59:59Z", "2026-10-25"),
            ("2026-10-25T23:00:00Z", "2026-10-26"),
        ] {
            assert_eq!(
                jour_de(pool, "Europe/Paris", instant(utc)).await.unwrap(),
                jour(attendu),
                "{utc}"
            );
        }
        assert_eq!(
            jour_de(pool, "UTC", instant("2026-03-28T23:30:00Z"))
                .await
                .unwrap(),
            jour("2026-03-28")
        );
        assert_eq!(
            jour_de(pool, "America/New_York", instant("2026-03-29T03:00:00Z"))
                .await
                .unwrap(),
            jour("2026-03-28")
        );
    }

    #[tokio::test]
    async fn fuseau_verifie_au_demarrage() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        assert!(verifier_fuseau(pool, "Europe/Paris").await.is_ok());
        assert!(verifier_fuseau(pool, "UTC").await.is_ok());
        let erreur = verifier_fuseau(pool, "Europe/Lutece").await.unwrap_err();
        assert!(erreur.contains("SABLIER_FUSEAU") && erreur.contains("Europe/Lutece"));
    }
}
