use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Serialize, Serializer};
use sqlx::PgPool;

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

fn iso<S: Serializer>(d: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&d.to_rfc3339_opts(SecondsFormat::Millis, true))
}

#[derive(Serialize)]
pub struct Meta {
    pub size: i32,
    #[serde(serialize_with = "iso")]
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Partage {
    pub id: String,
    pub size: i32,
    #[serde(serialize_with = "iso")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "iso")]
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Depot {
    pub id: String,
    #[serde(serialize_with = "iso")]
    pub expires_at: DateTime<Utc>,
    pub size: usize,
}

pub async fn migrate(db: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATIONS.run(db).await
}

pub async fn count_pending(db: &PgPool, owner: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM shares WHERE owner=$1 AND expires_at > now()")
        .bind(owner)
        .fetch_one(db)
        .await
}

pub async fn insert_share(
    db: &PgPool,
    id: &str,
    owner: &str,
    data: &[u8],
    ttl_seconds: u32,
    max_pending: usize,
) -> sqlx::Result<Option<DateTime<Utc>>> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(owner)
        .execute(&mut *tx)
        .await?;
    let expires_at = sqlx::query_scalar(
        "INSERT INTO shares (id, owner, data, size, expires_at)
         SELECT $1, $2, $3, $4, now() + make_interval(secs => $5)
         WHERE (SELECT count(*) FROM shares WHERE owner=$2 AND expires_at > now()) < $6
         RETURNING expires_at",
    )
    .bind(id)
    .bind(owner)
    .bind(data)
    .bind(i32::try_from(data.len()).unwrap_or(i32::MAX))
    .bind(f64::from(ttl_seconds))
    .bind(i64::try_from(max_pending).unwrap_or(i64::MAX))
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(expires_at)
}

pub async fn share_meta(db: &PgPool, id: &str) -> sqlx::Result<Option<Meta>> {
    let ligne: Option<(i32, DateTime<Utc>)> =
        sqlx::query_as("SELECT size, expires_at FROM shares WHERE id=$1 AND expires_at > now()")
            .bind(id)
            .fetch_optional(db)
            .await?;
    Ok(ligne.map(|(size, expires_at)| Meta { size, expires_at }))
}

pub async fn consume_share(db: &PgPool, id: &str) -> sqlx::Result<Option<Vec<u8>>> {
    sqlx::query_scalar("DELETE FROM shares WHERE id=$1 AND expires_at > now() RETURNING data")
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn list_shares(db: &PgPool, owner: &str) -> sqlx::Result<Vec<Partage>> {
    let lignes: Vec<(String, i32, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, size, created_at, expires_at FROM shares WHERE owner=$1 AND expires_at > now() ORDER BY created_at DESC",
    )
    .bind(owner)
    .fetch_all(db)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|(id, size, created_at, expires_at)| Partage {
            id,
            size,
            created_at,
            expires_at,
        })
        .collect())
}

pub async fn delete_share(db: &PgPool, id: &str, owner: &str) -> sqlx::Result<bool> {
    let r = sqlx::query("DELETE FROM shares WHERE id=$1 AND owner=$2")
        .bind(id)
        .bind(owner)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn purge_expired(db: &PgPool) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM shares WHERE expires_at <= now()")
        .execute(db)
        .await?;
    Ok(r.rows_affected())
}
