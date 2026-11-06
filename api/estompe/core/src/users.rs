use sqlx::{PgPool, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub sub: String,
    pub pseudo: String,
    pub suspended: bool,
}

#[derive(Debug)]
pub enum ActivateError {
    PseudoTaken,
    AlreadyActivated,
    Db(sqlx::Error),
}

pub async fn user_by_sub(pool: &PgPool, sub: &str) -> Result<Option<User>, sqlx::Error> {
    let ligne = sqlx::query(
        "SELECT id::text, keycloak_sub, pseudo, suspended_at IS NOT NULL FROM users WHERE keycloak_sub=$1",
    )
    .bind(sub)
    .fetch_optional(pool)
    .await?;
    Ok(ligne.map(|l| User {
        id: l.get(0),
        sub: l.get(1),
        pseudo: l.get(2),
        suspended: l.get(3),
    }))
}

pub async fn activate(pool: &PgPool, sub: &str, pseudo: &str) -> Result<User, ActivateError> {
    let ligne = sqlx::query(
        "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1,$2) RETURNING id::text, keycloak_sub, pseudo",
    )
    .bind(sub)
    .bind(pseudo)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => {
            if d.constraint().is_some_and(|c| c.contains("pseudo")) {
                ActivateError::PseudoTaken
            } else {
                ActivateError::AlreadyActivated
            }
        }
        _ => ActivateError::Db(e),
    })?;
    Ok(User {
        id: ligne.get(0),
        sub: ligne.get(1),
        pseudo: ligne.get(2),
        suspended: false,
    })
}

pub async fn co_members(pool: &PgPool, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM (
            SELECT sm2.user_id::text AS id FROM server_members sm1 JOIN server_members sm2 ON sm1.server_id = sm2.server_id WHERE sm1.user_id=$1::uuid
            UNION
            SELECT CASE WHEN user_a=$1::uuid THEN user_b ELSE user_a END::text FROM dm_conversations WHERE $1::uuid IN (user_a, user_b)
        ) t WHERE id <> $1::text",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
