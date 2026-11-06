use std::future::Future;
use std::time::Duration;

use include_dir::{Dir, include_dir};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub type Erreur = Box<dyn std::error::Error + Send + Sync>;

static MIGRATIONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");

pub struct Migration {
    pub nom: String,
    pub sql: String,
}

pub fn embarquees() -> Vec<Migration> {
    let mut v: Vec<Migration> = MIGRATIONS
        .files()
        .filter(|f| f.path().extension().is_some_and(|e| e == "sql"))
        .map(|f| Migration {
            nom: format!("migrations/{}", f.path().to_string_lossy()),
            sql: f.contents_utf8().expect("migration en UTF-8").to_string(),
        })
        .collect();
    v.sort_by(|a, b| a.nom.cmp(&b.nom));
    v
}

pub async fn migrate(pool: &PgPool, migrations: &[Migration]) -> Result<(), Erreur> {
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (name text PRIMARY KEY)")
        .execute(pool)
        .await?;
    let mut tries: Vec<&Migration> = migrations.iter().collect();
    tries.sort_by(|a, b| a.nom.cmp(&b.nom));
    for m in tries {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(7001)")
            .execute(&mut *tx)
            .await?;
        let inseree =
            sqlx::query("INSERT INTO schema_migrations (name) VALUES ($1) ON CONFLICT DO NOTHING")
                .bind(&m.nom)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if inseree == 1 {
            sqlx::raw_sql(sqlx::AssertSqlSafe(m.sql.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("{} : {e}", m.nom))?;
        }
        tx.commit().await?;
    }
    Ok(())
}

pub async fn migrate_embarquees(pool: &PgPool) -> Result<(), Erreur> {
    migrate(pool, &embarquees()).await
}

pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

pub async fn totaux(pool: &PgPool) -> Result<(i64, i64, i64), sqlx::Error> {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM messages), (SELECT count(*) FROM files WHERE purged_at IS NULL)",
    )
    .fetch_one(pool)
    .await
}

pub async fn octets_stockes(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COALESCE(sum(size), 0)::bigint FROM files WHERE purged_at IS NULL")
        .fetch_one(pool)
        .await
}

pub async fn journaliser_erreur(
    pool: &PgPool,
    methode: &str,
    chemin: &str,
    statut: u16,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM server_errors WHERE at < now() - interval '30 days'")
        .execute(pool)
        .await?;
    let chemin: String = chemin.chars().take(512).collect();
    sqlx::query("INSERT INTO server_errors (method, path, status) VALUES ($1, $2, $3)")
        .bind(methode)
        .bind(chemin)
        .bind(i32::from(statut))
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn attendre<T, E, F, Fut>(
    intervalle: Duration,
    delai: Duration,
    mut tentative: F,
) -> Result<T, E>
where
    E: std::fmt::Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let limite = tokio::time::Instant::now() + delai;
    loop {
        match tentative().await {
            Ok(v) => return Ok(v),
            Err(e) if tokio::time::Instant::now() >= limite => return Err(e),
            Err(e) => {
                tracing::warn!("base de données indisponible, nouvelle tentative : {e}");
                tokio::time::sleep(intervalle).await;
            }
        }
    }
}

pub async fn open(url: &str) -> Result<PgPool, Erreur> {
    attendre(Duration::from_secs(2), Duration::from_secs(60), || async {
        let pool = PgPoolOptions::new()
            .acquire_timeout(Duration::from_secs(2))
            .connect(url)
            .await?;
        if let Err(e) = migrate_embarquees(&pool).await {
            pool.close().await;
            return Err(e);
        }
        Ok(pool)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::postgres;
    use std::cell::Cell;
    use std::path::Path;

    const DOSSIER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    async fn pool(url: &str) -> PgPool {
        PgPoolOptions::new().connect(url).await.unwrap()
    }

    async fn noms(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar("SELECT name FROM schema_migrations ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    fn attendus() -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(DOSSIER)
            .unwrap()
            .map(|e| format!("migrations/{}", e.unwrap().file_name().to_string_lossy()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn embarquees_identiques_au_dossier() {
        let e = embarquees();
        assert_eq!(
            e.iter().map(|m| m.nom.clone()).collect::<Vec<_>>(),
            attendus()
        );
        for m in &e {
            let sql =
                std::fs::read(Path::new(DOSSIER).join(m.nom.trim_start_matches("migrations/")))
                    .unwrap();
            assert_eq!(m.sql.as_bytes(), sql.as_slice(), "{}", m.nom);
        }
    }

    #[tokio::test]
    async fn base_vide_inscrit_les_noms() {
        let Some((_c, url)) = postgres().await else {
            return;
        };
        let p = pool(&url).await;
        migrate_embarquees(&p).await.unwrap();
        assert_eq!(noms(&p).await, attendus());
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&p)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn migrate_deux_fois_ne_rejoue_rien() {
        let Some((_c, url)) = postgres().await else {
            return;
        };
        let p = pool(&url).await;
        migrate_embarquees(&p).await.unwrap();
        sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ('a', 'abc')")
            .execute(&p)
            .await
            .unwrap();
        migrate_embarquees(&p).await.unwrap();
        assert_eq!(noms(&p).await.len(), 42);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&p)
            .await
            .unwrap();
        assert_eq!(n, 2);
    }

    #[tokio::test]
    async fn base_remplie_par_le_go_ne_rejoue_aucun_sql() {
        let Some((_c, url)) = postgres().await else {
            return;
        };
        let p = pool(&url).await;
        sqlx::query("CREATE TABLE schema_migrations (name text PRIMARY KEY)")
            .execute(&p)
            .await
            .unwrap();
        for nom in attendus() {
            sqlx::query("INSERT INTO schema_migrations (name) VALUES ($1)")
                .bind(nom)
                .execute(&p)
                .await
                .unwrap();
        }
        migrate_embarquees(&p).await.unwrap();
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name <> 'schema_migrations'",
        )
        .fetch_one(&p)
        .await
        .unwrap();
        assert_eq!(tables, 0);
        assert_eq!(noms(&p).await.len(), 42);
    }

    #[tokio::test]
    async fn migrations_concurrentes_jouees_une_fois() {
        let Some((_c, url)) = postgres().await else {
            return;
        };
        let p = pool(&url).await;
        let mut liste = embarquees();
        liste.push(Migration {
            nom: "migrations/999_test.sql".into(),
            sql:
                "CREATE TABLE test_unique (id int PRIMARY KEY); INSERT INTO test_unique VALUES (1);"
                    .into(),
        });
        let (a, b) = tokio::join!(migrate(&p, &liste), migrate(&p, &liste));
        a.unwrap();
        b.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM test_unique")
            .fetch_one(&p)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(noms(&p).await.last().unwrap(), "migrations/999_test.sql");
    }

    #[tokio::test]
    async fn migration_en_erreur_non_inscrite() {
        let Some((_c, url)) = postgres().await else {
            return;
        };
        let p = pool(&url).await;
        let mut liste = embarquees();
        liste.push(Migration {
            nom: "migrations/998_ko.sql".into(),
            sql: "CREATE TABLE a (id int); SELECT 1/0;".into(),
        });
        let e = migrate(&p, &liste).await.unwrap_err().to_string();
        assert!(e.starts_with("migrations/998_ko.sql : "), "{e}");
        assert!(
            !noms(&p)
                .await
                .contains(&"migrations/998_ko.sql".to_string())
        );
        let existe: bool = sqlx::query_scalar("SELECT to_regclass('a') IS NOT NULL")
            .fetch_one(&p)
            .await
            .unwrap();
        assert!(!existe);
    }

    #[tokio::test]
    async fn ping_repond() {
        let Some((_c, url)) = postgres().await else {
            return;
        };
        ping(&pool(&url).await).await.unwrap();
    }

    #[tokio::test]
    async fn attendre_reussit_apres_des_echecs() {
        let essais = Cell::new(0);
        let r: Result<u32, String> =
            attendre(Duration::from_millis(1), Duration::from_secs(30), || {
                essais.set(essais.get() + 1);
                let n = essais.get();
                async move {
                    if n < 4 {
                        Err(format!("échec {n}"))
                    } else {
                        Ok(n)
                    }
                }
            })
            .await;
        assert_eq!(r, Ok(4));
    }

    #[tokio::test]
    async fn attendre_abandonne_avec_la_derniere_erreur() {
        let essais = Cell::new(0);
        let r: Result<(), String> =
            attendre(Duration::from_millis(1), Duration::from_millis(20), || {
                essais.set(essais.get() + 1);
                let n = essais.get();
                async move { Err(format!("échec {n}")) }
            })
            .await;
        assert_eq!(r, Err(format!("échec {}", essais.get())));
        assert!(essais.get() > 1);
    }
}
