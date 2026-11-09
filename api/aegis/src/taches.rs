use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::Row;

use crate::{AppState, annuaire, cache::Portee, journal};

pub fn lancer(state: AppState) {
    let s = state.clone();
    tokio::spawn(async move {
        let mut depuis = Utc::now() - chrono::Duration::minutes(1);
        let mut t = tokio::time::interval(Duration::from_secs(60));
        loop {
            t.tick().await;
            match expirer(&s, depuis).await {
                Ok(jusqua) => depuis = jusqua,
                Err(e) => tracing::warn!("tâche d'expiration : {e}"),
            }
        }
    });
    if state.settings.gateway.configure() && state.settings.annuaire.sync_minutes > 0 {
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(
                state.settings.annuaire.sync_minutes * 60,
            ));
            loop {
                t.tick().await;
                match annuaire::synchroniser(&state).await {
                    Ok(n) => tracing::info!("annuaire synchronisé depuis Keycloak : {n} comptes"),
                    Err(e) => tracing::warn!("synchronisation de l'annuaire : {e:?}"),
                }
            }
        });
    }
}

const SQL_EXPIRES: &str = "
SELECT 'rule' AS genre, id::text AS id, subject_kind, subject_id, permission AS quoi, expires_at FROM rules
WHERE expires_at > $1 AND expires_at <= $2
UNION ALL
SELECT 'binding', id::text, subject_kind, subject_id, role_id::text, expires_at FROM role_bindings
WHERE expires_at > $1 AND expires_at <= $2
UNION ALL
SELECT 'membership', group_id::text || ':' || sub, 'account', sub, group_id::text, expires_at FROM group_members
WHERE expires_at > $1 AND expires_at <= $2";

pub async fn expirer(state: &AppState, depuis: DateTime<Utc>) -> sqlx::Result<DateTime<Utc>> {
    let jusqua: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&state.db)
        .await?;
    let lignes = sqlx::query(SQL_EXPIRES)
        .bind(depuis)
        .bind(jusqua)
        .fetch_all(&state.db)
        .await?;
    if lignes.is_empty() {
        return Ok(jusqua);
    }
    let mut tout = false;
    let mut comptes = Vec::new();
    let mut tx = state.db.begin().await?;
    for l in &lignes {
        let genre: String = l.try_get("genre")?;
        let id: String = l.try_get("id")?;
        let kind: String = l.try_get("subject_kind")?;
        let sujet: String = l.try_get("subject_id")?;
        let quoi: String = l.try_get("quoi")?;
        let compte = kind == "account";
        if compte {
            comptes.push(sujet.clone());
        } else {
            tout = true;
        }
        journal::ecrire(
            &mut tx,
            "aegis",
            journal::Entree {
                action: "expire",
                target_kind: &genre,
                target_id: &id,
                subject: compte.then_some(sujet.as_str()),
                detail: json!({"subject_kind": kind, "subject_id": sujet, "what": quoi}),
            },
        )
        .await?;
    }
    tx.commit().await?;
    if tout {
        state.cache.invalider(Portee::Tout).await;
    } else {
        for c in comptes {
            state.cache.invalider(Portee::Compte(&c)).await;
        }
    }
    Ok(jusqua)
}
