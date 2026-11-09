use serde_json::Value;
use sqlx::PgConnection;

pub struct Entree<'a> {
    pub action: &'a str,
    pub target_kind: &'a str,
    pub target_id: &'a str,
    pub subject: Option<&'a str>,
    pub detail: Value,
}

pub async fn ecrire(c: &mut PgConnection, actor: &str, e: Entree<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (actor, action, target_kind, target_id, subject, detail) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor)
    .bind(e.action)
    .bind(e.target_kind)
    .bind(e.target_id)
    .bind(e.subject)
    .bind(e.detail)
    .execute(c)
    .await?;
    Ok(())
}
