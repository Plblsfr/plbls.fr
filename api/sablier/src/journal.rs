use axum::{
    extract::{Query, State},
    response::Response,
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};

use crate::{
    etat::AppState,
    garde::Personne,
    http::{bd, invalide, ok},
    identite::Identite,
};

pub const REGROUPEMENT: TimeDelta = TimeDelta::minutes(10);
pub const VUES_MAX: i64 = 500;
pub const PAGE: i64 = 50;
pub const PAGE_MAX: i64 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Consultation,
    Effacement,
}

impl Action {
    pub fn nom(self) -> &'static str {
        match self {
            Action::Consultation => "consultation",
            Action::Effacement => "effacement",
        }
    }
}

pub async fn inscrire(
    tx: &mut PgConnection,
    admin: &Identite,
    cible: &str,
    action: Action,
    maintenant: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    if admin.sub == cible {
        return Ok(false);
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("{}\n{cible}", admin.sub))
        .execute(&mut *tx)
        .await?;
    Ok(sqlx::query(
        "INSERT INTO consultations (admin_sub, admin_nom, cible_sub, action, date)
        SELECT $1, $2, $3, $4, $5
        WHERE $6 OR NOT EXISTS (
            SELECT 1 FROM consultations
            WHERE admin_sub = $1 AND cible_sub = $3 AND action = $4 AND date > $7
        )",
    )
    .bind(&admin.sub)
    .bind(admin.nom.trim())
    .bind(cible)
    .bind(action.nom())
    .bind(maintenant)
    .bind(action == Action::Effacement)
    .bind(maintenant - REGROUPEMENT)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1)
}

pub async fn consulter(
    pool: &PgPool,
    admin: &Identite,
    cible: &str,
    maintenant: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let inscrite = inscrire(&mut tx, admin, cible, Action::Consultation, maintenant).await?;
    tx.commit().await?;
    Ok(inscrite)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Vue {
    pub date: DateTime<Utc>,
    pub admin_name: String,
    pub action: String,
}

pub async fn vues(pool: &PgPool, cible: &str, limite: i64) -> Result<Vec<Vue>, sqlx::Error> {
    sqlx::query_as(
        "SELECT date, admin_nom AS admin_name, action FROM consultations
        WHERE cible_sub = $1 ORDER BY id DESC LIMIT $2",
    )
    .bind(cible)
    .bind(limite)
    .fetch_all(pool)
    .await
}

pub async fn voir_vues(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    Ok(ok(&vues(state.pool(), p.sub(), VUES_MAX)
        .await
        .map_err(bd)?))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Ligne {
    pub id: i64,
    pub date: DateTime<Utc>,
    pub action: String,
    pub admin_sub: String,
    pub admin_name: String,
    pub target_sub: String,
    pub target_name: Option<String>,
}

pub async fn lire(
    pool: &PgPool,
    avant: Option<i64>,
    limite: i64,
) -> Result<Vec<Ligne>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.id, c.date, c.action, c.admin_sub, c.admin_nom AS admin_name,
            c.cible_sub AS target_sub, NULLIF(p.nom, '') AS target_name
        FROM consultations c LEFT JOIN personnes p ON p.sub = c.cible_sub
        WHERE $1::bigint IS NULL OR c.id < $1
        ORDER BY c.id DESC LIMIT $2",
    )
    .bind(avant)
    .bind(limite)
    .fetch_all(pool)
    .await
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Page {
    pub before: Option<String>,
    pub limit: Option<String>,
}

pub async fn voir_journal(
    State(state): State<AppState>,
    Query(page): Query<Page>,
) -> Result<Response, Response> {
    let entier = |texte: &Option<String>, message: &str| match texte.as_deref() {
        None | Some("") => Ok(None),
        Some(t) => t
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .map(Some)
            .ok_or_else(|| invalide(message)),
    };
    let avant = entier(&page.before, "before : identifiant d'une ligne du journal")?;
    let limite = entier(&page.limit, "limit : entier de 1 à 200")?
        .unwrap_or(PAGE)
        .min(PAGE_MAX);
    let mut lignes = lire(state.pool(), avant, limite + 1).await.map_err(bd)?;
    let suite = lignes.len() > usize::try_from(limite).unwrap_or(usize::MAX);
    if suite {
        lignes.pop();
    }
    Ok(ok(&json!({
        "next_before": lignes.last().filter(|_| suite).map(|l| l.id),
        "entries": lignes,
    })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::Value;

    use super::*;
    use crate::{
        routes,
        testutil::{Env, env, instant},
    };

    fn admin() -> Identite {
        Identite {
            sub: "sub-admin".into(),
            nom: "admin".into(),
            email: "admin@x.fr".into(),
        }
    }

    async fn lignes(e: &Env) -> Vec<(String, String, String, String)> {
        sqlx::query_as(
            "SELECT admin_sub, admin_nom, cible_sub, action FROM consultations ORDER BY id",
        )
        .fetch_all(e.state.pool())
        .await
        .unwrap()
    }

    fn ligne(cible: &str, action: &str) -> (String, String, String, String) {
        (
            "sub-admin".into(),
            "admin".into(),
            cible.into(),
            action.into(),
        )
    }

    #[tokio::test]
    async fn c2_chaque_consultation_d_une_autre_personne_est_journalisee_et_visible_par_elle() {
        let Some(e) = env().await else { return };
        let consultations: Vec<&'static str> = routes::table()
            .iter()
            .filter(|(r, _)| r.chemin.starts_with("/admin/people/{sub}/") && r.methode == "GET")
            .map(|(r, _)| r.chemin)
            .collect();
        assert_eq!(consultations.len(), 8);
        for (i, chemin) in consultations.iter().enumerate() {
            let cible = format!("sub-cible-{i}");
            let chemin = chemin.replace("{sub}", &cible).replace("{id}", "1");
            let r = e.get("admin", &chemin).await;
            assert_ne!(r.statut, StatusCode::FORBIDDEN, "{chemin}");
            assert_eq!(
                lignes(&e).await.last(),
                Some(&ligne(&cible, "consultation")),
                "{chemin}"
            );
            assert_eq!(lignes(&e).await.len(), i + 1, "{chemin}");
        }
        sqlx::query("DELETE FROM consultations")
            .execute(e.state.pool())
            .await
            .unwrap();
        assert_eq!(e.get("alice", "/views").await.json(), json!([]));
        let avant = Utc::now();
        let r = e.get("admin", "/admin/people/sub-alice/usage").await;
        assert_eq!(r.statut, StatusCode::OK);
        let r = e.get("alice", "/views").await;
        assert_eq!(r.statut, StatusCode::OK);
        let vues = r.json();
        assert_eq!(vues.as_array().unwrap().len(), 1, "{vues}");
        assert_eq!(
            (vues[0]["admin_name"].clone(), vues[0]["action"].clone()),
            (json!("admin"), json!("consultation"))
        );
        let date = instant(vues[0]["date"].as_str().unwrap());
        assert!(avant - TimeDelta::seconds(1) <= date && date <= Utc::now());
        assert!(!r.texte().contains("sub-admin"), "{}", r.texte());
        assert_eq!(vues[0].as_object().unwrap().len(), 3);
        assert_eq!(e.get("bruno", "/views").await.json(), json!([]));
        assert_eq!(e.get("admin", "/views").await.json(), json!([]));
    }

    #[tokio::test]
    async fn admin_qui_consulte_ses_propres_stats_non_journalise() {
        let Some(e) = env().await else { return };
        for chemin in ["usage", "trophies", "streaks", "recaps", "messaging"] {
            let r = e
                .get("admin", &format!("/admin/people/sub-admin/{chemin}"))
                .await;
            assert_eq!(r.statut, StatusCode::OK, "{chemin}");
        }
        e.get("admin", "/usage").await;
        e.get("admin", "/admin/totals").await;
        e.get("admin", "/admin/people").await;
        e.get("admin", "/admin/audit").await;
        assert!(lignes(&e).await.is_empty());
    }

    #[tokio::test]
    async fn consultations_regroupees_par_dix_minutes() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let t = |texte: &str| instant(texte);
        let consulte = |cible: &'static str, quand: &'static str| async move {
            consulter(pool, &admin(), cible, t(quand)).await.unwrap()
        };
        assert!(consulte("sub-alice", "2026-03-10T10:00:00Z").await);
        assert!(!consulte("sub-alice", "2026-03-10T10:00:01Z").await);
        assert!(!consulte("sub-alice", "2026-03-10T10:09:59Z").await);
        assert!(consulte("sub-bruno", "2026-03-10T10:05:00Z").await);
        assert!(consulte("sub-alice", "2026-03-10T10:10:00Z").await);
        assert!(!consulte("sub-alice", "2026-03-10T10:19:59Z").await);
        let autre = Identite {
            sub: "sub-autre-admin".into(),
            nom: "autre".into(),
            email: String::new(),
        };
        assert!(
            consulter(pool, &autre, "sub-alice", t("2026-03-10T10:11:00Z"))
                .await
                .unwrap()
        );
        assert!(!consulte("sub-admin", "2026-03-10T10:30:00Z").await);
        let dates: Vec<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT date FROM consultations
            WHERE admin_sub = 'sub-admin' AND cible_sub = 'sub-alice' ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(
            dates,
            [t("2026-03-10T10:00:00Z"), t("2026-03-10T10:10:00Z")]
        );
        assert_eq!(lignes(&e).await.len(), 4);
    }

    #[tokio::test]
    async fn ecran_qui_charge_toutes_les_routes_d_un_coup_une_seule_ligne() {
        let Some(e) = env().await else { return };
        let appel = |chemin: &'static str| e.get("admin", chemin);
        let (a, b, c, d, f, g) = tokio::join!(
            appel("/admin/people/sub-alice/usage"),
            appel("/admin/people/sub-alice/entracte"),
            appel("/admin/people/sub-alice/sillon"),
            appel("/admin/people/sub-alice/messaging"),
            appel("/admin/people/sub-alice/trophies"),
            appel("/admin/people/sub-alice/streaks"),
        );
        for r in [a, b, c, d, f, g] {
            assert_eq!(r.statut, StatusCode::OK);
        }
        assert_eq!(lignes(&e).await, [ligne("sub-alice", "consultation")]);
    }

    #[tokio::test]
    async fn journal_complet_pagine_pour_les_admins() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        e.get("alice", "/me").await;
        for (cible, quand) in [
            ("sub-alice", "2026-03-10T10:00:00Z"),
            ("sub-bruno", "2026-03-10T11:00:00Z"),
            ("sub-alice", "2026-03-10T12:00:00Z"),
        ] {
            consulter(pool, &admin(), cible, instant(quand))
                .await
                .unwrap();
        }
        let r = e.get("admin", "/admin/audit").await;
        assert_eq!(r.statut, StatusCode::OK);
        let entree = |id: i64, date: &str, cible: &str, nom: Value| {
            json!({
                "id": id,
                "date": date,
                "action": "consultation",
                "admin_sub": "sub-admin",
                "admin_name": "admin",
                "target_sub": cible,
                "target_name": nom,
            })
        };
        assert_eq!(
            r.json(),
            json!({
                "next_before": null,
                "entries": [
                    entree(3, "2026-03-10T12:00:00Z", "sub-alice", json!("alice")),
                    entree(2, "2026-03-10T11:00:00Z", "sub-bruno", Value::Null),
                    entree(1, "2026-03-10T10:00:00Z", "sub-alice", json!("alice")),
                ],
            })
        );
        let r = e.get("admin", "/admin/audit?limit=2").await.json();
        assert_eq!(r["next_before"], 2);
        assert_eq!(r["entries"].as_array().unwrap().len(), 2);
        let r = e.get("admin", "/admin/audit?limit=2&before=2").await.json();
        assert_eq!(r["next_before"], Value::Null);
        assert_eq!(
            r["entries"],
            json!([entree(
                1,
                "2026-03-10T10:00:00Z",
                "sub-alice",
                json!("alice")
            )])
        );
        for faux in ["?limit=0", "?limit=x", "?before=-1", "?before=abc"] {
            let r = e.get("admin", &format!("/admin/audit{faux}")).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
            assert_eq!(r.json()["error"], "invalid");
        }
        assert_eq!(
            e.get("alice", "/admin/audit").await.statut,
            StatusCode::FORBIDDEN
        );
    }
}
