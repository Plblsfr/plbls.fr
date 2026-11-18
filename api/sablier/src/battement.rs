use std::time::Duration;

use axum::{body::Bytes, extract::State, response::Response};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    agregats,
    etat::AppState,
    garde::Personne,
    http::{bd, invalide, sans_contenu},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plateforme {
    Web,
    Pwa,
    Windows,
    Android,
}

impl Plateforme {
    pub const TOUTES: [Plateforme; 4] = [
        Plateforme::Web,
        Plateforme::Pwa,
        Plateforme::Windows,
        Plateforme::Android,
    ];

    pub fn nom(self) -> &'static str {
        match self {
            Plateforme::Web => "web",
            Plateforme::Pwa => "pwa",
            Plateforme::Windows => "windows",
            Plateforme::Android => "android",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    Messagerie,
    Entracte,
    Sillon,
    Regie,
    Sablier,
    Admin,
    Autre,
}

impl Section {
    pub const TOUTES: [Section; 7] = [
        Section::Messagerie,
        Section::Entracte,
        Section::Sillon,
        Section::Regie,
        Section::Sablier,
        Section::Admin,
        Section::Autre,
    ];

    pub fn nom(self) -> &'static str {
        match self {
            Section::Messagerie => "messagerie",
            Section::Entracte => "entracte",
            Section::Sillon => "sillon",
            Section::Regie => "regie",
            Section::Sablier => "sablier",
            Section::Admin => "admin",
            Section::Autre => "autre",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Battement {
    pub platform: Plateforme,
    pub section: Section,
}

impl Battement {
    pub fn cle(&self) -> String {
        format!("{}|{}", self.platform.nom(), self.section.nom())
    }
}

pub fn decouper(cle: &str) -> (&str, &str) {
    cle.split_once('|').unwrap_or((cle, ""))
}

pub async fn enregistrer(
    pool: &PgPool,
    fuseau: &str,
    intervalle: Duration,
    sub: &str,
    battement: Battement,
    maintenant: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let premier = sqlx::query(
        "INSERT INTO battements (sub, dernier) VALUES ($1, $2) ON CONFLICT (sub) DO NOTHING",
    )
    .bind(sub)
    .bind(maintenant)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if premier {
        tx.commit().await?;
        return Ok(0);
    }
    let ancien: DateTime<Utc> =
        sqlx::query_scalar("SELECT dernier FROM battements WHERE sub = $1 FOR UPDATE")
            .bind(sub)
            .fetch_one(&mut *tx)
            .await?;
    let plafond = i64::try_from(intervalle.as_secs()).unwrap_or(i64::MAX);
    let ecoule = (maintenant - ancien).num_milliseconds();
    if ecoule < 1000 {
        tx.commit().await?;
        return Ok(0);
    }
    let (secondes, dernier) = if ecoule > plafond.saturating_mul(1000) {
        (plafond, maintenant)
    } else {
        let entieres = ecoule / 1000;
        (entieres, ancien + TimeDelta::seconds(entieres))
    };
    sqlx::query("UPDATE battements SET dernier = $2 WHERE sub = $1")
        .bind(sub)
        .bind(dernier)
        .execute(&mut *tx)
        .await?;
    let jour = agregats::jour_de(&mut *tx, fuseau, maintenant).await?;
    agregats::ajouter(
        &mut *tx,
        sub,
        jour,
        agregats::UTILISATION,
        &battement.cle(),
        secondes,
    )
    .await?;
    tx.commit().await?;
    Ok(secondes)
}

pub async fn battre(
    State(state): State<AppState>,
    p: Personne,
    corps: Bytes,
) -> Result<Response, Response> {
    let battement: Battement = serde_json::from_slice(&corps).map_err(|_| {
        invalide(
            "platform : web, pwa, windows ou android ; section : messagerie, entracte, sillon, regie, sablier, admin ou autre",
        )
    })?;
    let c = state.config();
    enregistrer(
        state.pool(),
        &c.fuseau,
        c.battement,
        p.sub(),
        battement,
        Utc::now(),
    )
    .await
    .map_err(bd)?;
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;
    use crate::testutil::{Env, env, instant, jour};

    const INTERVALLE: Duration = Duration::from_secs(30);
    const WEB: Battement = Battement {
        platform: Plateforme::Web,
        section: Section::Messagerie,
    };
    const ANDROID: Battement = Battement {
        platform: Plateforme::Android,
        section: Section::Sillon,
    };

    async fn bat(e: &Env, sub: &str, b: Battement, quand: DateTime<Utc>) -> i64 {
        enregistrer(e.state.pool(), "Europe/Paris", INTERVALLE, sub, b, quand)
            .await
            .unwrap()
    }

    async fn total(e: &Env, sub: &str, j: NaiveDate) -> i64 {
        agregats::somme(e.state.pool(), sub, agregats::UTILISATION, j, j)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn dix_minutes_de_battements_reguliers_donnent_dix_minutes() {
        let Some(e) = env().await else { return };
        let debut = instant("2026-06-10T10:00:00Z");
        for (pas, sub) in [
            (30_000, "regulier"),
            (29_600, "en-avance"),
            (30_250, "en-retard"),
        ] {
            let n = 600_000 / pas;
            for i in 0..=n {
                bat(&e, sub, WEB, debut + TimeDelta::milliseconds(i * pas)).await;
            }
            let t = total(&e, sub, jour("2026-06-10")).await;
            assert!((570..=600).contains(&t), "{sub} : {t} secondes");
        }
        assert_eq!(total(&e, "regulier", jour("2026-06-10")).await, 600);
    }

    #[tokio::test]
    async fn premier_battement_et_silence_plafonnes_a_l_intervalle() {
        let Some(e) = env().await else { return };
        let debut = instant("2026-06-10T10:00:00Z");
        assert_eq!(bat(&e, "a", WEB, debut).await, 0);
        assert_eq!(bat(&e, "a", WEB, debut + TimeDelta::seconds(12)).await, 12);
        assert_eq!(bat(&e, "a", WEB, debut + TimeDelta::hours(1)).await, 30);
        assert_eq!(bat(&e, "a", WEB, debut + TimeDelta::days(40)).await, 30);
        assert_eq!(bat(&e, "a", WEB, debut + TimeDelta::days(39)).await, 0);
        assert_eq!(total(&e, "a", jour("2026-06-10")).await, 42);
        assert_eq!(total(&e, "a", jour("2026-07-20")).await, 30);
        assert_eq!(total(&e, "b", jour("2026-06-10")).await, 0);
    }

    #[tokio::test]
    async fn deux_clients_en_meme_temps_ne_doublent_pas() {
        let Some(e) = env().await else { return };
        let debut = instant("2026-06-10T10:00:00Z");
        for i in 0..=20 {
            let t = debut + TimeDelta::seconds(i * 30);
            bat(&e, "a", WEB, t).await;
            bat(&e, "a", ANDROID, t + TimeDelta::milliseconds(40)).await;
        }
        assert_eq!(total(&e, "a", jour("2026-06-10")).await, 600);
        for i in 0..=20 {
            let t = debut + TimeDelta::hours(2) + TimeDelta::seconds(i * 30);
            bat(&e, "b", WEB, t).await;
            bat(&e, "b", ANDROID, t + TimeDelta::seconds(15)).await;
        }
        assert_eq!(total(&e, "b", jour("2026-06-10")).await, 615);
        let pool = e.state.pool().clone();
        let fin = debut + TimeDelta::seconds(630);
        let taches: Vec<_> = (0..16)
            .map(|i| {
                let pool = pool.clone();
                let b = if i % 2 == 0 { WEB } else { ANDROID };
                tokio::spawn(async move {
                    enregistrer(&pool, "Europe/Paris", INTERVALLE, "a", b, fin)
                        .await
                        .unwrap()
                })
            })
            .collect();
        let mut credit = 0;
        for t in taches {
            credit += t.await.unwrap();
        }
        assert_eq!(credit, 30);
        assert_eq!(total(&e, "a", jour("2026-06-10")).await, 630);
        let nouveaux: Vec<_> = (0..16)
            .map(|_| {
                let pool = pool.clone();
                tokio::spawn(async move {
                    enregistrer(&pool, "Europe/Paris", INTERVALLE, "c", WEB, fin)
                        .await
                        .unwrap()
                })
            })
            .collect();
        for t in nouveaux {
            assert_eq!(t.await.unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn temps_range_par_plateforme_et_section() {
        let Some(e) = env().await else { return };
        let debut = instant("2026-06-10T10:00:00Z");
        bat(&e, "a", WEB, debut).await;
        bat(&e, "a", WEB, debut + TimeDelta::seconds(30)).await;
        bat(&e, "a", ANDROID, debut + TimeDelta::seconds(50)).await;
        let j = jour("2026-06-10");
        assert_eq!(
            agregats::par_cle(e.state.pool(), "a", agregats::UTILISATION, j, j)
                .await
                .unwrap(),
            [
                ("web|messagerie".to_string(), 30),
                ("android|sillon".to_string(), 20)
            ]
        );
        assert_eq!(decouper("android|sillon"), ("android", "sillon"));
        assert_eq!(decouper("seul"), ("seul", ""));
    }

    #[tokio::test]
    async fn journee_decoupee_a_minuit_de_paris_et_aux_changements_d_heure() {
        let Some(e) = env().await else { return };
        for (sub, debut, avant, apres) in [
            ("hiver", "2026-01-15T22:55:00Z", "2026-01-15", "2026-01-16"),
            (
                "veille-ete",
                "2026-03-28T22:55:00Z",
                "2026-03-28",
                "2026-03-29",
            ),
            ("ete", "2026-03-29T21:55:00Z", "2026-03-29", "2026-03-30"),
            (
                "veille-hiver",
                "2026-10-24T21:55:00Z",
                "2026-10-24",
                "2026-10-25",
            ),
            (
                "retour-hiver",
                "2026-10-25T22:55:00Z",
                "2026-10-25",
                "2026-10-26",
            ),
        ] {
            let debut = instant(debut);
            for i in 0..=20 {
                bat(&e, sub, WEB, debut + TimeDelta::seconds(i * 30)).await;
            }
            assert_eq!(total(&e, sub, jour(avant)).await, 270, "{sub} avant minuit");
            assert_eq!(total(&e, sub, jour(apres)).await, 330, "{sub} après minuit");
        }
        let debut = instant("2026-03-29T00:55:00Z");
        for i in 0..=20 {
            bat(&e, "saut", WEB, debut + TimeDelta::seconds(i * 30)).await;
        }
        assert_eq!(total(&e, "saut", jour("2026-03-29")).await, 600);
        let debut = instant("2026-10-25T00:30:00Z");
        for i in 0..=240 {
            bat(&e, "heure-double", WEB, debut + TimeDelta::seconds(i * 30)).await;
        }
        assert_eq!(total(&e, "heure-double", jour("2026-10-25")).await, 7200);
    }

    #[tokio::test]
    async fn route_de_battement_sous_session_valeurs_fermees() {
        let Some(e) = env().await else { return };
        for (plateforme, section) in [
            ("web", "messagerie"),
            ("pwa", "entracte"),
            ("windows", "sillon"),
            ("android", "regie"),
            ("web", "sablier"),
            ("web", "admin"),
            ("web", "autre"),
        ] {
            let r = e
                .appel(
                    Some("personne"),
                    "POST",
                    "/heartbeat",
                    Some(json!({"platform": plateforme, "section": section})),
                )
                .await;
            assert_eq!(r.statut, StatusCode::NO_CONTENT, "{plateforme} {section}");
            assert!(r.corps.is_empty());
        }
        for corps in [
            json!({}),
            json!({"platform": "web"}),
            json!({"section": "sillon"}),
            json!({"platform": "ios", "section": "sillon"}),
            json!({"platform": "Web", "section": "sillon"}),
            json!({"platform": "web", "section": "musique"}),
            json!({"platform": "web", "section": 3}),
            json!([]),
        ] {
            let r = e
                .appel(Some("personne"), "POST", "/heartbeat", Some(corps.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
            assert_eq!(r.json()["error"], "invalid");
        }
        let r = e
            .appel(
                None,
                "POST",
                "/heartbeat",
                Some(json!({"platform": "web", "section": "sillon"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::UNAUTHORIZED);
        let dernier: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT dernier FROM battements WHERE sub = 'sub-personne'")
                .fetch_optional(e.state.pool())
                .await
                .unwrap();
        assert!(dernier.is_some());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM battements")
            .fetch_one(e.state.pool())
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
}
