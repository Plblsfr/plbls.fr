use serde_json::{Value, json};
use sqlx::PgPool;

use crate::servers::{horodatage, rfc3339};

pub async fn audit(pool: &PgPool, serveur: &str, acteur: &str, action: &str, cible: &str) {
    audit_membre(pool, serveur, acteur, action, "", cible, "").await;
}

pub async fn audit_membre(
    pool: &PgPool,
    serveur: &str,
    acteur: &str,
    action: &str,
    cible_id: &str,
    cible: &str,
    raison: &str,
) {
    let r = sqlx::query(
        "INSERT INTO audit_log (server_id, actor, action, target, target_id, reason) VALUES ($1::uuid,$2::uuid,$3,$4,NULLIF($5,'')::uuid,$6)",
    )
    .bind(serveur)
    .bind(acteur)
    .bind(action)
    .bind(cible)
    .bind(cible_id)
    .bind(raison)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::warn!("journal d'audit : {e}");
    }
}

#[derive(Default)]
pub struct Filtre {
    pub acteur: String,
    pub cible: String,
    pub action: String,
}

type Ligne = (String, String, String, String, String, String, String);

fn entree((at, actor, actor_id, action, target, target_id, reason): Ligne) -> Value {
    json!({
        "at": rfc3339(&at),
        "actor": actor,
        "actor_id": actor_id,
        "action": action,
        "target": target,
        "target_id": target_id,
        "reason": reason,
    })
}

pub async fn journal(
    pool: &PgPool,
    serveur: &str,
    filtre: &Filtre,
) -> Result<Vec<Value>, sqlx::Error> {
    let at = horodatage("l.at");
    Ok(sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
        "SELECT {at}, u.pseudo, l.actor::text, l.action, l.target, COALESCE(l.target_id::text,''), l.reason FROM audit_log l JOIN users u ON u.id = l.actor
         WHERE l.server_id=$1::uuid AND ($2='' OR l.actor::text=$2) AND ($3='' OR l.target_id::text=$3) AND ($4='' OR l.action=$4 OR l.action LIKE $4 || '.%')
         ORDER BY l.id DESC LIMIT 200"
    )))
    .bind(serveur)
    .bind(&filtre.acteur)
    .bind(&filtre.cible)
    .bind(&filtre.action)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(entree)
    .collect())
}

pub async fn historique(
    pool: &PgPool,
    serveur: &str,
    membre: &str,
) -> Result<Vec<Value>, sqlx::Error> {
    let at = horodatage("l.at");
    Ok(sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
        "SELECT {at}, u.pseudo, l.actor::text, l.action, l.target, l.target_id::text, l.reason FROM audit_log l JOIN users u ON u.id = l.actor
         WHERE l.server_id=$1::uuid AND l.target_id=$2::uuid AND l.action LIKE 'member.%' ORDER BY l.id DESC LIMIT 200"
    )))
    .bind(serveur)
    .bind(membre)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(entree)
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    async fn base() -> Option<(crate::testutil::BasePostgres, PgPool, String, String)> {
        let (pg, url) = testutil::postgres().await?;
        let pool = PgPool::connect(&url).await.unwrap();
        crate::db::migrate_embarquees(&pool).await.unwrap();
        let acteur: String = sqlx::query_scalar(
            "INSERT INTO users (keycloak_sub, pseudo) VALUES ('sub-a','alice') RETURNING id::text",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let serveur: String = sqlx::query_scalar(
            "INSERT INTO servers (name, owner_id) VALUES ('S',$1::uuid) RETURNING id::text",
        )
        .bind(&acteur)
        .fetch_one(&pool)
        .await
        .unwrap();
        Some((pg, pool, acteur, serveur))
    }

    #[tokio::test]
    async fn ecriture_lecture_et_filtres() {
        let Some((_pg, pool, acteur, serveur)) = base().await else {
            return;
        };
        audit(&pool, &serveur, &acteur, "role.create", "Admin").await;
        audit_membre(
            &pool,
            &serveur,
            &acteur,
            "member.ban",
            &acteur,
            "alice",
            "spam",
        )
        .await;
        audit(&pool, &serveur, &acteur, "channel.override", "général").await;
        let tout = journal(&pool, &serveur, &Filtre::default()).await.unwrap();
        let actions: Vec<_> = tout.iter().map(|l| l["action"].as_str().unwrap()).collect();
        assert_eq!(actions, ["channel.override", "member.ban", "role.create"]);
        assert_eq!(tout[2]["target_id"], "");
        assert_eq!(tout[1]["target_id"], acteur.as_str());
        assert_eq!(tout[1]["reason"], "spam");
        assert_eq!(tout[2]["actor"], "alice");
        assert_eq!(tout[2]["actor_id"], acteur.as_str());
        let at = tout[0]["at"].as_str().unwrap();
        assert!(
            at.ends_with('Z') && at.len() >= 20 && !at.contains("+"),
            "{at}"
        );
        assert!(
            !at.split_once('.')
                .is_some_and(|(_, f)| f.trim_end_matches('Z').ends_with('0')),
            "{at}"
        );

        let filtre = |actor: &str, target: &str, action: &str| Filtre {
            acteur: actor.into(),
            cible: target.into(),
            action: action.into(),
        };
        let n = |f: Filtre| {
            let (pool, serveur) = (pool.clone(), serveur.clone());
            async move { journal(&pool, &serveur, &f).await.unwrap().len() }
        };
        assert_eq!(n(filtre("", "", "role")).await, 1);
        assert_eq!(n(filtre("", "", "role.create")).await, 1);
        assert_eq!(n(filtre("", "", "rol")).await, 0);
        assert_eq!(n(filtre("", &acteur, "")).await, 1);
        assert_eq!(n(filtre(&acteur, "", "")).await, 3);
        assert_eq!(n(filtre("pas-un-uuid", "", "")).await, 0);

        for i in 0..205 {
            audit(&pool, &serveur, &acteur, "role.update", &format!("r{i}")).await;
        }
        let l = journal(&pool, &serveur, &Filtre::default()).await.unwrap();
        assert_eq!(l.len(), 200);
        assert_eq!(l[0]["target"], "r204");
    }

    #[tokio::test]
    async fn une_erreur_d_ecriture_ne_fait_pas_echouer() {
        let Some((_pg, pool, acteur, _)) = base().await else {
            return;
        };
        audit(
            &pool,
            "0123abcd-0123-4567-89ab-0123456789ab",
            &acteur,
            "role.create",
            "x",
        )
        .await;
        audit_membre(
            &pool,
            "0123abcd-0123-4567-89ab-0123456789ab",
            &acteur,
            "member.ban",
            "pas-un-uuid",
            "x",
            "",
        )
        .await;
    }
}
