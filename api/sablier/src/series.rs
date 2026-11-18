use axum::{extract::State, response::Response};
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;

use crate::{
    domaines::{entracte, messagerie, sillon},
    droits::Droits,
    etat::AppState,
    garde::Personne,
    http::{bd, ok},
    periode::debut_des_temps,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Serie {
    Globale,
    Entracte,
    Sillon,
    Messagerie,
}

impl Serie {
    pub fn stats(self) -> &'static [&'static str] {
        match self {
            Serie::Globale => &[
                entracte::SECONDES,
                sillon::ECOUTES,
                messagerie::MESSAGES,
                messagerie::VOCAL_SECONDES,
            ],
            Serie::Entracte => &[entracte::SECONDES],
            Serie::Sillon => &[sillon::ECOUTES],
            Serie::Messagerie => &[messagerie::MESSAGES, messagerie::VOCAL_SECONDES],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Etat {
    pub current: i64,
    pub best: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Series {
    pub global: Etat,
    pub entracte: Option<Etat>,
    pub sillon: Option<Etat>,
    pub messaging: Etat,
}

pub async fn jours(
    pool: &PgPool,
    sub: &str,
    serie: Serie,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Vec<NaiveDate>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT jour FROM agregats_jour
        WHERE sub = $1 AND stat = ANY($2) AND valeur > 0 AND jour BETWEEN $3 AND $4
        ORDER BY jour",
    )
    .bind(sub)
    .bind(serie.stats())
    .bind(du)
    .bind(au)
    .fetch_all(pool)
    .await
}

pub fn calculer(jours: &[NaiveDate], aujourd_hui: NaiveDate) -> Etat {
    let mut etat = Etat::default();
    let mut precedent: Option<NaiveDate> = None;
    for jour in jours.iter().filter(|j| **j <= aujourd_hui) {
        let suite = precedent.and_then(|p| p.succ_opt()) == Some(*jour);
        etat.current = if suite { etat.current + 1 } else { 1 };
        etat.best = etat.best.max(etat.current);
        precedent = Some(*jour);
    }
    if precedent
        .and_then(|p| p.succ_opt())
        .is_none_or(|lendemain| lendemain < aujourd_hui)
    {
        etat.current = 0;
    }
    etat
}

pub async fn etat(
    pool: &PgPool,
    sub: &str,
    serie: Serie,
    aujourd_hui: NaiveDate,
) -> Result<Etat, sqlx::Error> {
    let jours = jours(pool, sub, serie, debut_des_temps(), aujourd_hui).await?;
    Ok(calculer(&jours, aujourd_hui))
}

pub async fn lire(
    pool: &PgPool,
    sub: &str,
    droits: &Droits,
    aujourd_hui: NaiveDate,
) -> Result<Series, sqlx::Error> {
    Ok(Series {
        global: etat(pool, sub, Serie::Globale, aujourd_hui).await?,
        entracte: if droits.entracte {
            Some(etat(pool, sub, Serie::Entracte, aujourd_hui).await?)
        } else {
            None
        },
        sillon: if droits.sillon {
            Some(etat(pool, sub, Serie::Sillon, aujourd_hui).await?)
        } else {
            None
        },
        messaging: etat(pool, sub, Serie::Messagerie, aujourd_hui).await?,
    })
}

pub async fn voir(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    Ok(ok(&lire(state.pool(), p.sub(), &p.droits, aujourd_hui)
        .await
        .map_err(bd)?))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use commun::evenements::{Donnees, Genre};
    use serde_json::json;

    use super::*;
    use crate::{
        agregats,
        battement::{Battement, Plateforme, Section, enregistrer},
        testutil::{Env, env, instant, jour},
    };

    const SUB: &str = "sub-alice";

    fn message() -> Donnees {
        Donnees::EstompeMessage {
            serveur: Some("srv".into()),
            salon: Some("general".into()),
            prive: false,
        }
    }

    async fn envoyer(e: &Env, dates: &[&str]) {
        for date in dates {
            e.recevoir(&format!("m-{date}"), SUB, date, false, message())
                .await;
        }
    }

    async fn globale(e: &Env, aujourd_hui: &str) -> (i64, i64) {
        let s = etat(e.state.pool(), SUB, Serie::Globale, jour(aujourd_hui))
            .await
            .unwrap();
        (s.current, s.best)
    }

    #[test]
    fn suites_de_jours_en_cours_et_record() {
        let j = |textes: &[&str]| textes.iter().map(|t| jour(t)).collect::<Vec<_>>();
        let c = |textes: &[&str], aujourd_hui: &str| {
            let e = calculer(&j(textes), jour(aujourd_hui));
            (e.current, e.best)
        };
        assert_eq!(c(&[], "2026-03-10"), (0, 0));
        assert_eq!(c(&["2026-03-10"], "2026-03-10"), (1, 1));
        assert_eq!(c(&["2026-03-09"], "2026-03-10"), (1, 1));
        assert_eq!(c(&["2026-03-08"], "2026-03-10"), (0, 1));
        let jours = [
            "2026-02-27",
            "2026-02-28",
            "2026-03-01",
            "2026-03-02",
            "2026-03-08",
            "2026-03-09",
        ];
        assert_eq!(c(&jours, "2026-03-10"), (2, 4));
        assert_eq!(c(&jours, "2026-03-11"), (0, 4));
        assert_eq!(c(&jours, "2026-03-01"), (3, 3));
        assert_eq!(c(&["2027-12-31", "2028-01-01"], "2028-01-01"), (2, 2));
    }

    #[tokio::test]
    async fn c5_serie_cassee_par_une_journee_sans_action_reelle_prolongee_par_un_message() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        envoyer(&e, &["2026-03-10T10:00:00Z", "2026-03-11T10:00:00Z"]).await;
        assert_eq!(globale(&e, "2026-03-11").await, (2, 2));
        let battement = Battement {
            platform: Plateforme::Web,
            section: Section::Messagerie,
        };
        for quand in ["2026-03-12T10:00:00Z", "2026-03-12T10:00:20Z"] {
            enregistrer(
                pool,
                "Europe/Paris",
                std::time::Duration::from_secs(30),
                SUB,
                battement,
                instant(quand),
            )
            .await
            .unwrap();
        }
        assert_eq!(
            agregats::somme(
                pool,
                SUB,
                agregats::UTILISATION,
                jour("2026-03-12"),
                jour("2026-03-12")
            )
            .await
            .unwrap(),
            20
        );
        assert_eq!(globale(&e, "2026-03-12").await, (2, 2));
        assert_eq!(globale(&e, "2026-03-13").await, (0, 2));
        envoyer(&e, &["2026-03-13T10:00:00Z"]).await;
        assert_eq!(globale(&e, "2026-03-13").await, (1, 2));
        envoyer(&e, &["2026-03-12T21:00:00Z"]).await;
        assert_eq!(globale(&e, "2026-03-13").await, (4, 4));
    }

    #[tokio::test]
    async fn c5_serie_au_passage_de_minuit_et_aux_changements_d_heure() {
        let Some(e) = env().await else { return };
        envoyer(
            &e,
            &[
                "2026-03-27T22:59:59Z",
                "2026-03-28T22:59:59Z",
                "2026-03-28T23:00:00Z",
                "2026-03-29T22:00:00Z",
            ],
        )
        .await;
        assert_eq!(
            jours(
                e.state.pool(),
                SUB,
                Serie::Messagerie,
                jour("2026-03-01"),
                jour("2026-03-31")
            )
            .await
            .unwrap(),
            [
                jour("2026-03-27"),
                jour("2026-03-28"),
                jour("2026-03-29"),
                jour("2026-03-30")
            ]
        );
        assert_eq!(globale(&e, "2026-03-30").await, (4, 4));
        assert_eq!(globale(&e, "2026-03-31").await, (4, 4));
        assert_eq!(globale(&e, "2026-04-01").await, (0, 4));
        envoyer(
            &e,
            &[
                "2026-10-24T21:59:59Z",
                "2026-10-24T22:00:00Z",
                "2026-10-25T22:59:59Z",
            ],
        )
        .await;
        assert_eq!(globale(&e, "2026-10-25").await, (2, 4));
        assert_eq!(globale(&e, "2026-10-27").await, (0, 4));
        envoyer(&e, &["2026-10-25T23:00:00Z"]).await;
        assert_eq!(globale(&e, "2026-10-27").await, (3, 4));
    }

    #[tokio::test]
    async fn series_par_domaine_selon_les_acces() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let aujourd_hui = e.state.aujourd_hui().await.unwrap();
        let hier = aujourd_hui.pred_opt().unwrap();
        for sub in ["sub-alice", "sub-bruno", "sub-diane"] {
            agregats::ajouter(pool, sub, hier, entracte::SECONDES, "", 60)
                .await
                .unwrap();
            agregats::ajouter(pool, sub, aujourd_hui, entracte::SECONDES, "", 60)
                .await
                .unwrap();
            agregats::ajouter(pool, sub, aujourd_hui, sillon::ECOUTES, "", 1)
                .await
                .unwrap();
            agregats::ajouter(pool, sub, aujourd_hui, sillon::SECONDES, "", 500)
                .await
                .unwrap();
            agregats::ajouter(pool, sub, hier, messagerie::VOCAL_SECONDES, "prive", 30)
                .await
                .unwrap();
        }
        let r = e.get("alice", "/streaks").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(
            r.json(),
            json!({
                "global": {"current": 2, "best": 2},
                "entracte": null,
                "sillon": null,
                "messaging": {"current": 1, "best": 1},
            })
        );
        let r = e.get("bruno", "/streaks").await.json();
        assert_eq!(
            (r["entracte"].clone(), r["sillon"].clone()),
            (json!(null), json!({"current": 1, "best": 1}))
        );
        let r = e.get("diane", "/streaks").await.json();
        assert_eq!(
            (r["entracte"].clone(), r["sillon"].clone()),
            (json!({"current": 2, "best": 2}), json!(null))
        );
        e.recevoir(
            "lecture",
            "sub-admin",
            "2026-03-10T10:00:00Z",
            false,
            Donnees::EntracteLecture {
                titre: "f1".into(),
                genre: Genre::Film,
                serie: None,
                secondes: 60,
            },
        )
        .await;
        assert_eq!(
            e.get("admin", "/streaks").await.json()["global"],
            json!({"current": 0, "best": 1})
        );
    }
}
