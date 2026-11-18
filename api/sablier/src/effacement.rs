use axum::{body::Bytes, extract::State, response::Response};
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};

use crate::{
    etat::AppState,
    garde::Personne,
    http::{bd, corps, invalide, sans_contenu},
};

pub const CONFIRMATION: &str = "EFFACER";

pub fn tables_personnelles() -> &'static [&'static str] {
    &[
        "evenements",
        "agregats_jour",
        "battements",
        "sessions_entracte",
        "deblocages",
        "a_evaluer",
        "recaps",
        "partages_stats",
        "partages_trophees",
        "personnes",
    ]
}

pub async fn effacer_dans(tx: &mut PgConnection, sub: &str) -> Result<u64, sqlx::Error> {
    let mut lignes = 0;
    for table in tables_personnelles() {
        lignes += sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE sub = $1"
        )))
        .bind(sub)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    Ok(lignes)
}

pub async fn effacer(pool: &PgPool, sub: &str) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let lignes = effacer_dans(&mut tx, sub).await?;
    tx.commit().await?;
    Ok(lignes)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Demande {
    confirm: String,
}

pub fn confirmer(octets: &[u8]) -> Result<(), Response> {
    let message = "confirm : écrire EFFACER pour confirmer";
    let demande: Demande = corps(octets, message)?;
    if demande.confirm == CONFIRMATION {
        Ok(())
    } else {
        Err(invalide(message))
    }
}

pub async fn effacer_les_siennes(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    confirmer(&octets)?;
    effacer(state.pool(), p.sub()).await.map_err(bd)?;
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use commun::evenements::Donnees;
    use serde_json::json;

    use super::*;
    use crate::{file::Issue, testutil::env};

    #[tokio::test]
    async fn toute_table_avec_une_colonne_sub_est_une_table_personnelle() {
        let Some(e) = env().await else { return };
        let mut en_base: Vec<String> = sqlx::query_scalar(
            "SELECT table_name::text FROM information_schema.columns
            WHERE table_schema = 'public' AND column_name = 'sub'",
        )
        .fetch_all(e.state.pool())
        .await
        .unwrap();
        en_base.sort();
        let mut declarees: Vec<&str> = tables_personnelles().to_vec();
        declarees.sort_unstable();
        assert_eq!(en_base, declarees);
    }

    #[tokio::test]
    async fn c4_apres_effacement_la_personne_est_comme_sans_activite_et_les_autres_intactes() {
        let Some(e) = env().await else { return };
        let vierge = e.lectures("alice").await;
        assert!(vierge.len() >= 12, "{vierge:?}");
        e.remplir("alice").await;
        e.remplir("bruno").await;
        e.get("admin", "/admin/people/sub-alice/usage").await;
        for (table, n) in e.lignes_de("sub-alice").await {
            assert!(n > 0, "{table} sans ligne avant l'effacement");
        }
        let remplie = e.lectures("alice").await;
        for ((chemin, _, avant), (_, _, apres)) in vierge.iter().zip(&remplie) {
            let inchangees = ["/sillon", "/entracte"];
            assert!(
                avant != apres || inchangees.contains(&chemin.as_str()),
                "{chemin} ne montre pas les données d'alice"
            );
        }
        let bruno = e.lectures("bruno").await;
        let lignes_de_bruno = e.lignes_de("sub-bruno").await;
        for corps in [
            None,
            Some(json!({})),
            Some(json!({"confirm": "effacer"})),
            Some(json!({"confirm": "EFFACER", "sub": "sub-bruno"})),
            Some(json!({"confirm": true})),
        ] {
            let r = e
                .appel(Some("alice"), "DELETE", "/data", corps.clone())
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps:?}");
            assert_eq!(r.json()["error"], "invalid");
        }
        assert_eq!(e.lectures("alice").await, remplie);
        let confirmation = json!({"confirm": CONFIRMATION});
        let r = e
            .appel(Some("alice"), "DELETE", "/data", Some(confirmation.clone()))
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert!(r.corps.is_empty());
        for (table, n) in e.lignes_de("sub-alice").await {
            assert_eq!(n, 0, "{table} garde des lignes d'alice");
        }
        let apres = e.lectures("alice").await;
        assert_eq!(apres, vierge);
        let profil = apres
            .iter()
            .find(|(chemin, _, _)| chemin == "/profiles/sub-alice")
            .unwrap();
        assert_eq!(profil.2, json!({"month": [], "total": [], "trophies": []}));
        assert_eq!(e.lignes_de("sub-bruno").await, lignes_de_bruno);
        assert_eq!(e.lectures("bruno").await, bruno);
        let journal: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM consultations WHERE cible_sub = 'sub-alice'")
                .fetch_one(e.state.pool())
                .await
                .unwrap();
        assert_eq!(journal, 1);
        assert_eq!(
            e.get("alice", "/views")
                .await
                .json()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let r = e
            .appel(Some("alice"), "DELETE", "/data", Some(confirmation.clone()))
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let r = e
            .appel(Some("personne"), "DELETE", "/data", Some(confirmation))
            .await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn c5_un_evenement_arrive_apres_l_effacement_est_compte_normalement() {
        let Some(e) = env().await else { return };
        e.remplir("alice").await;
        let messages =
            || async { e.get("alice", "/messaging?period=all").await.json()["total"].clone() };
        assert_eq!(messages().await, 1);
        assert!(effacer(e.state.pool(), "sub-alice").await.unwrap() > 0);
        assert_eq!(messages().await, 0);
        let message = Donnees::EstompeMessage {
            serveur: None,
            salon: None,
            prive: true,
        };
        let recu = e
            .recevoir(
                "nouveau",
                "sub-alice",
                "2026-04-02T09:00:00Z",
                false,
                message.clone(),
            )
            .await;
        assert_eq!(recu, Issue::Applique);
        assert_eq!(messages().await, 1);
        let r = e.get("alice", "/usage?period=all").await.json();
        assert_eq!(r["first_day"], "2026-04-02");
        let r = e.get("alice", "/trophies").await.json();
        let premier_jour = r["unlocked"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == "premier_jour")
            .unwrap();
        assert_eq!(premier_jour["unlocked_at"], "2026-04-02");
        let rejoue = e
            .recevoir(
                "nouveau",
                "sub-alice",
                "2026-04-02T09:00:00Z",
                false,
                message.clone(),
            )
            .await;
        assert_eq!(rejoue, Issue::Doublon);
        let import_rejoue = e
            .recevoir(
                "sub-alice-evt-0",
                "sub-alice",
                "2026-03-10T10:00:00Z",
                true,
                message,
            )
            .await;
        assert_eq!(import_rejoue, Issue::Applique);
        assert_eq!(messages().await, 2);
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/heartbeat",
                Some(json!({"platform": "web", "section": "sablier"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let lignes = e.lignes_de("sub-alice").await;
        assert!(lignes.contains(&("battements", 1)) && lignes.contains(&("personnes", 1)));
    }
}
