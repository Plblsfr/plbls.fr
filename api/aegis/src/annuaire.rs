use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;

use crate::{AppState, error::ApiError};

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow, PartialEq)]
pub struct Compte {
    pub sub: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub in_keycloak: bool,
}

impl Compte {
    pub fn inconnu(sub: &str) -> Self {
        Compte {
            sub: sub.to_string(),
            email: None,
            display_name: None,
            last_seen_at: None,
            in_keycloak: true,
        }
    }
}

fn nettoyer(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(200).collect())
}

pub async fn vu(
    db: &PgPool,
    sub: &str,
    email: Option<&str>,
    nom: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO accounts (sub, email, display_name, last_seen_at) VALUES ($1, $2, $3, now())
         ON CONFLICT (sub) DO UPDATE SET
             email = coalesce(EXCLUDED.email, accounts.email),
             display_name = coalesce(EXCLUDED.display_name, accounts.display_name),
             last_seen_at = now(),
             in_keycloak = true
         WHERE accounts.last_seen_at IS NULL OR accounts.last_seen_at < now() - interval '1 hour'
            OR (EXCLUDED.email IS NOT NULL AND accounts.email IS DISTINCT FROM EXCLUDED.email)
            OR (EXCLUDED.display_name IS NOT NULL AND accounts.display_name IS DISTINCT FROM EXCLUDED.display_name)",
    )
    .bind(sub)
    .bind(nettoyer(email))
    .bind(nettoyer(nom))
    .execute(db)
    .await?;
    Ok(())
}

pub async fn assurer(c: &mut PgConnection, sub: &str) -> Result<(), ApiError> {
    if sub.trim().is_empty() || sub.len() > 200 {
        return Err(ApiError::BadRequest("compte invalide".into()));
    }
    sqlx::query("INSERT INTO accounts (sub) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(sub)
        .execute(c)
        .await?;
    Ok(())
}

pub async fn par_sub(db: &PgPool, subs: &[String]) -> sqlx::Result<HashMap<String, Compte>> {
    let lignes: Vec<Compte> = sqlx::query_as(
        "SELECT sub, email, display_name, last_seen_at, in_keycloak FROM accounts WHERE sub = ANY($1)",
    )
    .bind(subs)
    .fetch_all(db)
    .await?;
    Ok(lignes.into_iter().map(|c| (c.sub.clone(), c)).collect())
}

pub async fn un(db: &PgPool, sub: &str) -> sqlx::Result<Compte> {
    Ok(par_sub(db, &[sub.to_string()])
        .await?
        .remove(sub)
        .unwrap_or_else(|| Compte::inconnu(sub)))
}

#[derive(Deserialize)]
struct PageGateway {
    users: Vec<CompteGateway>,
    next: Option<u32>,
}

#[derive(Deserialize)]
struct CompteGateway {
    id: String,
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

async fn page_gateway(state: &AppState, email: &str, first: u32) -> Result<PageGateway, ApiError> {
    let g = &state.settings.gateway;
    let res = state
        .http
        .get(format!("{}/internal/users", g.url.trim_end_matches('/')))
        .bearer_auth(&g.internal_token)
        .query(&[("email", email)])
        .query(&[("first", first), ("max", 100)])
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("passerelle injoignable : {e}")))?;
    if !res.status().is_success() {
        return Err(ApiError::Upstream(format!(
            "la passerelle a répondu {}",
            res.status()
        )));
    }
    res.json()
        .await
        .map_err(|e| ApiError::Upstream(format!("réponse de la passerelle illisible : {e}")))
}

async fn enregistrer(db: &PgPool, comptes: &[CompteGateway]) -> sqlx::Result<()> {
    for c in comptes {
        sqlx::query(
            "INSERT INTO accounts (sub, email, display_name, synced_at, in_keycloak) VALUES ($1, $2, $3, now(), true)
             ON CONFLICT (sub) DO UPDATE SET email = coalesce(EXCLUDED.email, accounts.email),
                 display_name = coalesce(EXCLUDED.display_name, accounts.display_name),
                 synced_at = now(), in_keycloak = true",
        )
        .bind(&c.id)
        .bind(nettoyer(c.email.as_deref()))
        .bind(nettoyer(c.name.as_deref()))
        .execute(db)
        .await?;
    }
    Ok(())
}

pub async fn chercher_keycloak(state: &AppState, debut_email: &str) -> Result<usize, ApiError> {
    if !state.settings.gateway.configure() || debut_email.trim().is_empty() {
        return Ok(0);
    }
    let page = page_gateway(state, debut_email.trim(), 0).await?;
    enregistrer(&state.db, &page.users).await?;
    Ok(page.users.len())
}

pub async fn synchroniser(state: &AppState) -> Result<usize, ApiError> {
    if !state.settings.gateway.configure() {
        return Ok(0);
    }
    let debut: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&state.db)
        .await?;
    let (mut first, mut total) = (0u32, 0usize);
    loop {
        let page = page_gateway(state, "", first).await?;
        enregistrer(&state.db, &page.users).await?;
        total += page.users.len();
        match page.next {
            Some(n) if n > first => first = n,
            _ => break,
        }
    }
    sqlx::query(
        "UPDATE accounts SET in_keycloak = false
         WHERE in_keycloak AND coalesce(synced_at, '-infinity') < $1 AND coalesce(last_seen_at, '-infinity') < $1",
    )
    .bind(debut)
    .execute(&state.db)
    .await?;
    Ok(total)
}
