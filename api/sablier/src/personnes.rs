use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;

use std::collections::HashMap;

use crate::{droits::Compte, etat::AppState, identite::Identite};

pub const AEGIS: &str = "aegis";
pub const LOCAL: &str = "local";

pub type Annuaire = Option<HashMap<String, Compte>>;

pub async fn annuaire(state: &AppState, subs: &[String]) -> Annuaire {
    state.aegis()?.comptes(subs).await.ok()
}

pub fn source(annuaire: &Annuaire) -> &'static str {
    if annuaire.is_some() { AEGIS } else { LOCAL }
}

pub const ECART_ECRITURE: TimeDelta = TimeDelta::hours(1);

pub async fn noter(
    pool: &PgPool,
    identite: &Identite,
    maintenant: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "INSERT INTO personnes (sub, nom, vu_le) VALUES ($1, $2, $3)
        ON CONFLICT (sub) DO UPDATE SET nom = EXCLUDED.nom, vu_le = EXCLUDED.vu_le
        WHERE personnes.vu_le <= $4",
    )
    .bind(&identite.sub)
    .bind(identite.nom.trim())
    .bind(maintenant)
    .bind(maintenant - ECART_ECRITURE)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::*;
    use crate::testutil::{env, instant};

    fn identite(nom: &str) -> Identite {
        Identite {
            sub: "sub-alice".into(),
            nom: nom.into(),
            email: "alice@x.fr".into(),
        }
    }

    async fn lignes(pool: &PgPool) -> Vec<(String, String, DateTime<Utc>)> {
        sqlx::query_as("SELECT sub, nom, vu_le FROM personnes ORDER BY sub")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn nom_ecrit_au_plus_une_fois_par_heure() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let debut = instant("2026-03-10T10:00:00Z");
        assert!(noter(pool, &identite("Alice"), debut).await.unwrap());
        let plus_tard = instant("2026-03-10T10:59:59Z");
        assert!(!noter(pool, &identite("Alice B."), plus_tard).await.unwrap());
        assert_eq!(
            lignes(pool).await,
            [("sub-alice".to_string(), "Alice".to_string(), debut)]
        );
        let une_heure = instant("2026-03-10T11:00:00Z");
        assert!(
            noter(pool, &identite(" Alice B. "), une_heure)
                .await
                .unwrap()
        );
        assert_eq!(
            lignes(pool).await,
            [("sub-alice".to_string(), "Alice B.".to_string(), une_heure)]
        );
    }

    #[tokio::test]
    async fn personne_notee_par_me_et_par_le_battement() {
        let Some(e) = env().await else { return };
        assert_eq!(e.get("alice", "/me").await.statut, StatusCode::OK);
        let r = e
            .appel(
                Some("personne"),
                "POST",
                "/heartbeat",
                Some(json!({"platform": "web", "section": "sablier"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let noms: Vec<(String, String)> = lignes(e.state.pool())
            .await
            .into_iter()
            .map(|(sub, nom, _)| (sub, nom))
            .collect();
        assert_eq!(
            noms,
            [
                ("sub-alice".to_string(), "alice".to_string()),
                ("sub-personne".to_string(), "personne".to_string()),
            ]
        );
    }
}
