use axum::{
    extract::{Query, State},
    response::Response,
};
use chrono::NaiveDate;
use commun::evenements::{Donnees, Evenement, Genre};
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

use crate::{
    agregats,
    domaines::sans_acces,
    etat::AppState,
    garde::Personne,
    http::{bd, ok},
    periode::{Demande, Intervalle, Plage},
};

pub const SECONDES: &str = "entracte.secondes";
pub const FILMS_VUS: &str = "entracte.films_vus";
pub const EPISODES_VUS: &str = "entracte.episodes_vus";
pub const SERIES_TERMINEES: &str = "entracte.series_terminees";
pub const SESSION_MAX: &str = "entracte.session_max";
pub const ENCHAINES_MAX: &str = "entracte.enchaines_max";
pub const ABANDONS: &str = "entracte.abandons";
pub const INTROS_PASSEES: &str = "entracte.intros_passees";
pub const CINEMA_SECONDES: &str = "entracte.cinema_secondes";
pub const CINEMA_SEANCES: &str = "entracte.cinema_seances";

pub const PAUSE_MINUTES: i32 = 30;

async fn session(
    tx: &mut PgConnection,
    e: &Evenement,
    jour: NaiveDate,
    secondes: i64,
) -> Result<(), sqlx::Error> {
    let en_cours: Option<(NaiveDate, i64)> = sqlx::query_as(
        "INSERT INTO sessions_entracte AS s (sub, jour, debut, dernier, secondes)
        VALUES ($1, $2, $3, $3, $4)
        ON CONFLICT (sub) DO UPDATE SET
            jour = CASE WHEN EXCLUDED.debut > s.dernier + make_interval(mins => $5)
                THEN EXCLUDED.jour ELSE s.jour END,
            debut = CASE WHEN EXCLUDED.debut > s.dernier + make_interval(mins => $5)
                THEN EXCLUDED.debut ELSE s.debut END,
            dernier = GREATEST(s.dernier, EXCLUDED.dernier),
            secondes = CASE WHEN EXCLUDED.debut > s.dernier + make_interval(mins => $5)
                THEN EXCLUDED.secondes ELSE s.secondes + EXCLUDED.secondes END,
            episodes = CASE WHEN EXCLUDED.debut > s.dernier + make_interval(mins => $5)
                THEN 0 ELSE s.episodes END
        WHERE EXCLUDED.debut >= s.debut - make_interval(mins => $5)
        RETURNING jour, secondes",
    )
    .bind(&e.sub)
    .bind(jour)
    .bind(e.date)
    .bind(secondes)
    .bind(PAUSE_MINUTES)
    .fetch_optional(&mut *tx)
    .await?;
    let (jour, secondes) = en_cours.unwrap_or((jour, secondes));
    agregats::maximum(tx, &e.sub, jour, SESSION_MAX, "", secondes).await
}

async fn enchainer(tx: &mut PgConnection, e: &Evenement) -> Result<(), sqlx::Error> {
    let en_cours: Option<(NaiveDate, i64)> = sqlx::query_as(
        "UPDATE sessions_entracte SET episodes = episodes + 1
        WHERE sub = $1 AND $2::timestamptz
            BETWEEN debut - make_interval(mins => $3) AND dernier + make_interval(mins => $3)
        RETURNING jour, episodes",
    )
    .bind(&e.sub)
    .bind(e.date)
    .bind(PAUSE_MINUTES)
    .fetch_optional(&mut *tx)
    .await?;
    match en_cours {
        Some((jour, episodes)) => {
            agregats::maximum(tx, &e.sub, jour, ENCHAINES_MAX, "", episodes).await
        }
        None => Ok(()),
    }
}

pub async fn appliquer(
    tx: &mut PgConnection,
    e: &Evenement,
    jour: NaiveDate,
) -> Result<(), sqlx::Error> {
    let sub = e.sub.as_str();
    match &e.donnees {
        Donnees::EntracteLecture { secondes, .. } => {
            let secondes = i64::from(*secondes);
            agregats::ajouter(&mut *tx, sub, jour, SECONDES, "", secondes).await?;
            if !e.importe {
                session(tx, e, jour, secondes).await?;
            }
        }
        Donnees::EntracteVu { genre, .. } => {
            let stat = match genre {
                Genre::Film => FILMS_VUS,
                Genre::Episode => EPISODES_VUS,
            };
            agregats::ajouter(&mut *tx, sub, jour, stat, "", 1).await?;
            if !e.importe && *genre == Genre::Episode {
                enchainer(tx, e).await?;
            }
        }
        Donnees::EntracteSerieTerminee { .. } => {
            agregats::ajouter(tx, sub, jour, SERIES_TERMINEES, "", 1).await?;
        }
        Donnees::EntracteAbandon { .. } if !e.importe => {
            agregats::ajouter(tx, sub, jour, ABANDONS, "", 1).await?;
        }
        Donnees::EntracteIntroPassee { .. } if !e.importe => {
            agregats::ajouter(tx, sub, jour, INTROS_PASSEES, "", 1).await?;
        }
        Donnees::EntracteBadge {
            genre,
            titre,
            saison,
        } => {
            crate::badges::enregistrer(tx, sub, *genre, titre, *saison, e.date).await?;
        }
        Donnees::EstompeCinemaTemps { secondes } => {
            agregats::ajouter(tx, sub, jour, CINEMA_SECONDES, "", i64::from(*secondes)).await?;
        }
        Donnees::EstompeCinemaSeance {} => {
            agregats::ajouter(tx, sub, jour, CINEMA_SEANCES, "", 1).await?;
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Journee {
    pub day: NaiveDate,
    pub seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub access: bool,
    #[serde(flatten)]
    pub intervalle: Intervalle,
    pub seconds: i64,
    pub movies_watched: i64,
    pub episodes_watched: i64,
    pub series_completed: i64,
    pub longest_session: i64,
    pub biggest_day: Option<Journee>,
    pub most_episodes_in_a_row: i64,
    pub abandoned: i64,
    pub intros_skipped: i64,
    pub cinema_seconds: i64,
    pub cinema_sessions: i64,
}

pub async fn lire(pool: &PgPool, sub: &str, plage: &Plage) -> Result<Stats, sqlx::Error> {
    let intervalle = plage.intervalle_de(pool, sub).await?;
    let (du, au) = (intervalle.from, intervalle.to);
    Ok(Stats {
        access: true,
        intervalle,
        seconds: agregats::somme(pool, sub, SECONDES, du, au).await?,
        movies_watched: agregats::somme(pool, sub, FILMS_VUS, du, au).await?,
        episodes_watched: agregats::somme(pool, sub, EPISODES_VUS, du, au).await?,
        series_completed: agregats::somme(pool, sub, SERIES_TERMINEES, du, au).await?,
        longest_session: agregats::plus_grande(pool, sub, SESSION_MAX, du, au).await?,
        biggest_day: agregats::max_jour(pool, sub, SECONDES, du, au)
            .await?
            .map(|(day, seconds)| Journee { day, seconds }),
        most_episodes_in_a_row: agregats::plus_grande(pool, sub, ENCHAINES_MAX, du, au).await?,
        abandoned: agregats::somme(pool, sub, ABANDONS, du, au).await?,
        intros_skipped: agregats::somme(pool, sub, INTROS_PASSEES, du, au).await?,
        cinema_seconds: agregats::somme(pool, sub, CINEMA_SECONDES, du, au).await?,
        cinema_sessions: agregats::somme(pool, sub, CINEMA_SEANCES, du, au).await?,
    })
}

pub async fn voir(
    State(state): State<AppState>,
    p: Personne,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    if !p.droits.entracte {
        return Ok(sans_acces());
    }
    let plage = demande.plage(&state).await?;
    Ok(ok(&lire(state.pool(), p.sub(), &plage)
        .await
        .map_err(bd)?))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::*;
    use crate::{
        file::Issue,
        testutil::{Env, env},
    };

    const SUB: &str = "sub-diane";

    fn lecture(titre: &str, genre: Genre, secondes: u32) -> Donnees {
        Donnees::EntracteLecture {
            titre: titre.into(),
            genre,
            serie: (genre == Genre::Episode).then(|| "s1".into()),
            secondes,
        }
    }

    fn vu(titre: &str, genre: Genre) -> Donnees {
        Donnees::EntracteVu {
            titre: titre.into(),
            genre,
            serie: (genre == Genre::Episode).then(|| "s1".into()),
        }
    }

    async fn mois(e: &Env) -> serde_json::Value {
        let r = e
            .get("diane", "/entracte?period=month&date=2026-03-10")
            .await;
        assert_eq!(r.statut, StatusCode::OK);
        r.json()
    }

    #[tokio::test]
    async fn temps_regarde_titres_vus_et_salon_cinema() {
        let Some(e) = env().await else { return };
        let evenements = [
            ("2026-03-10T20:00:00Z", lecture("f1", Genre::Film, 600)),
            ("2026-03-10T20:10:00Z", lecture("f1", Genre::Film, 540)),
            ("2026-03-10T20:10:01Z", vu("f1", Genre::Film)),
            ("2026-03-11T20:00:00Z", lecture("e1", Genre::Episode, 60)),
            ("2026-03-11T20:00:01Z", vu("e1", Genre::Episode)),
            ("2026-03-11T20:05:00Z", vu("e2", Genre::Episode)),
            (
                "2026-03-11T20:05:01Z",
                Donnees::EntracteSerieTerminee { serie: "s1".into() },
            ),
            (
                "2026-03-12T10:00:00Z",
                Donnees::EntracteAbandon {
                    titre: "f2".into(),
                    genre: Genre::Film,
                },
            ),
            (
                "2026-03-12T10:01:00Z",
                Donnees::EntracteIntroPassee { titre: "e3".into() },
            ),
            (
                "2026-03-12T21:00:00Z",
                Donnees::EstompeCinemaTemps { secondes: 3000 },
            ),
            (
                "2026-03-12T21:30:00Z",
                Donnees::EstompeCinemaTemps { secondes: 600 },
            ),
            ("2026-03-12T21:30:01Z", Donnees::EstompeCinemaSeance {}),
            ("2026-04-01T12:00:00Z", lecture("f3", Genre::Film, 9999)),
        ];
        for (i, (date, donnees)) in evenements.into_iter().enumerate() {
            assert_eq!(
                e.recevoir(&format!("e{i}"), SUB, date, false, donnees)
                    .await,
                Issue::Applique
            );
        }
        e.recevoir(
            "autre",
            "sub-alice",
            "2026-03-10T20:00:00Z",
            false,
            lecture("f1", Genre::Film, 77),
        )
        .await;
        assert_eq!(
            mois(&e).await,
            json!({
                "access": true,
                "period": "month",
                "from": "2026-03-01",
                "to": "2026-03-31",
                "seconds": 1200,
                "movies_watched": 1,
                "episodes_watched": 2,
                "series_completed": 1,
                "longest_session": 1140,
                "biggest_day": {"day": "2026-03-10", "seconds": 1140},
                "most_episodes_in_a_row": 2,
                "abandoned": 1,
                "intros_skipped": 1,
                "cinema_seconds": 3600,
                "cinema_sessions": 1,
            })
        );
        let r = e
            .get("diane", "/entracte?period=day&date=2026-03-09")
            .await
            .json();
        assert_eq!(
            r,
            json!({
                "access": true,
                "period": "day",
                "from": "2026-03-09",
                "to": "2026-03-09",
                "seconds": 0,
                "movies_watched": 0,
                "episodes_watched": 0,
                "series_completed": 0,
                "longest_session": 0,
                "biggest_day": null,
                "most_episodes_in_a_row": 0,
                "abandoned": 0,
                "intros_skipped": 0,
                "cinema_seconds": 0,
                "cinema_sessions": 0,
            })
        );
        let r = e
            .get("diane", "/entracte?period=all&date=2026-12-31")
            .await
            .json();
        assert_eq!(
            (r["from"].clone(), r["to"].clone(), r["seconds"].clone()),
            (json!("2026-03-10"), json!("2026-12-31"), json!(11199))
        );
        let r = e.get("diane", "/entracte?period=siecle").await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn import_rejoue_deux_fois_memes_totaux() {
        let Some(e) = env().await else { return };
        let historique = [
            (
                "import:entracte:lecture:1",
                "2025-11-02T21:00:00Z",
                lecture("f1", Genre::Film, 7200),
            ),
            (
                "import:entracte:vu:1",
                "2025-11-02T21:00:00Z",
                vu("f1", Genre::Film),
            ),
            (
                "import:entracte:lecture:2",
                "2025-11-03T21:00:00Z",
                lecture("e1", Genre::Episode, 1500),
            ),
            (
                "import:entracte:vu:2",
                "2025-11-03T21:00:00Z",
                vu("e1", Genre::Episode),
            ),
            (
                "import:entracte:lecture:3",
                "2025-11-03T22:00:00Z",
                lecture("e2", Genre::Episode, 400),
            ),
            (
                "import:entracte:serie:s1",
                "2025-11-03T22:00:00Z",
                Donnees::EntracteSerieTerminee { serie: "s1".into() },
            ),
        ];
        let mut totaux = Vec::new();
        for attendue in [Issue::Applique, Issue::Doublon] {
            for (id, date, donnees) in historique.clone() {
                assert_eq!(e.recevoir(id, SUB, date, true, donnees).await, attendue);
            }
            totaux.push(
                e.get("diane", "/entracte?period=year&date=2025-06-01")
                    .await
                    .json(),
            );
        }
        assert_eq!(totaux[0], totaux[1]);
        assert_eq!(
            (
                totaux[0]["seconds"].clone(),
                totaux[0]["movies_watched"].clone(),
                totaux[0]["episodes_watched"].clone(),
                totaux[0]["series_completed"].clone(),
            ),
            (json!(9100), json!(1), json!(1), json!(1))
        );
    }

    #[tokio::test]
    async fn facons_de_regarder_sans_les_evenements_importes() {
        let Some(e) = env().await else { return };
        let historique = [
            ("2026-03-10T20:00:00Z", lecture("e1", Genre::Episode, 1500)),
            ("2026-03-10T20:00:00Z", vu("e1", Genre::Episode)),
            ("2026-03-10T20:25:00Z", lecture("e2", Genre::Episode, 1500)),
            ("2026-03-10T20:25:00Z", vu("e2", Genre::Episode)),
            (
                "2026-03-10T20:30:00Z",
                Donnees::EntracteAbandon {
                    titre: "f1".into(),
                    genre: Genre::Film,
                },
            ),
            (
                "2026-03-10T20:31:00Z",
                Donnees::EntracteIntroPassee { titre: "e2".into() },
            ),
        ];
        for (i, (date, donnees)) in historique.into_iter().enumerate() {
            e.recevoir(&format!("import:{i}"), SUB, date, true, donnees)
                .await;
        }
        let r = mois(&e).await;
        assert_eq!(
            (r["seconds"].clone(), r["episodes_watched"].clone()),
            (json!(3000), json!(2))
        );
        for stat in [
            "longest_session",
            "most_episodes_in_a_row",
            "abandoned",
            "intros_skipped",
        ] {
            assert_eq!(r[stat], 0, "{stat}");
        }
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions_entracte")
            .fetch_one(e.state.pool())
            .await
            .unwrap();
        assert_eq!(sessions, 0);
        e.recevoir(
            "reel-1",
            SUB,
            "2026-03-10T21:00:00Z",
            false,
            lecture("e3", Genre::Episode, 300),
        )
        .await;
        e.recevoir(
            "import:tardif",
            SUB,
            "2026-03-10T21:01:00Z",
            true,
            vu("e3", Genre::Episode),
        )
        .await;
        let r = mois(&e).await;
        assert_eq!(
            (
                r["seconds"].clone(),
                r["longest_session"].clone(),
                r["most_episodes_in_a_row"].clone()
            ),
            (json!(3300), json!(300), json!(0))
        );
    }

    #[tokio::test]
    async fn sessions_et_episodes_enchaines() {
        let Some(e) = env().await else { return };
        let suite = [
            ("2026-03-10T22:00:00Z", lecture("e1", Genre::Episode, 600)),
            ("2026-03-10T22:30:00Z", lecture("e1", Genre::Episode, 900)),
            ("2026-03-10T22:30:01Z", vu("e1", Genre::Episode)),
            ("2026-03-10T23:00:00Z", lecture("e2", Genre::Episode, 1500)),
            ("2026-03-10T23:00:01Z", vu("e2", Genre::Episode)),
            ("2026-03-10T23:20:00Z", lecture("e3", Genre::Episode, 1200)),
            ("2026-03-10T23:20:01Z", vu("e3", Genre::Episode)),
            ("2026-03-10T23:20:02Z", vu("f9", Genre::Film)),
            ("2026-03-10T23:50:02Z", lecture("e4", Genre::Episode, 100)),
            ("2026-03-10T23:50:03Z", vu("e4", Genre::Episode)),
            ("2026-03-11T00:10:00Z", lecture("e5", Genre::Episode, 50)),
            ("2026-03-12T09:00:00Z", vu("e6", Genre::Episode)),
        ];
        for (i, (date, donnees)) in suite.into_iter().enumerate() {
            e.recevoir(&format!("s{i}"), SUB, date, false, donnees)
                .await;
        }
        let jour = |j: &'static str| {
            let e = &e;
            async move {
                let r = e
                    .get("diane", &format!("/entracte?period=day&date={j}"))
                    .await
                    .json();
                (
                    r["longest_session"].as_i64().unwrap(),
                    r["most_episodes_in_a_row"].as_i64().unwrap(),
                    r["episodes_watched"].as_i64().unwrap(),
                )
            }
        };
        assert_eq!(jour("2026-03-10").await, (4200, 3, 1));
        assert_eq!(jour("2026-03-11").await, (150, 1, 3));
        assert_eq!(jour("2026-03-12").await, (0, 0, 1));
        let r = mois(&e).await;
        assert_eq!(
            (
                r["longest_session"].clone(),
                r["most_episodes_in_a_row"].clone()
            ),
            (json!(4200), json!(3))
        );
        e.recevoir(
            "retard-ancien",
            SUB,
            "2026-03-05T12:00:00Z",
            false,
            lecture("f1", Genre::Film, 5000),
        )
        .await;
        e.recevoir(
            "retard-vu",
            SUB,
            "2026-03-05T12:00:01Z",
            false,
            vu("e0", Genre::Episode),
        )
        .await;
        e.recevoir(
            "retard-proche",
            SUB,
            "2026-03-11T00:05:00Z",
            false,
            lecture("e5", Genre::Episode, 25),
        )
        .await;
        let etat: (NaiveDate, i64, i64, bool) = sqlx::query_as(
            "SELECT jour, secondes, episodes, debut <= dernier FROM sessions_entracte WHERE sub = $1",
        )
        .bind(SUB)
        .fetch_one(e.state.pool())
        .await
        .unwrap();
        assert_eq!(etat, (crate::testutil::jour("2026-03-11"), 175, 1, true));
        assert_eq!(jour("2026-03-05").await, (5000, 0, 1));
        assert_eq!(jour("2026-03-11").await, (175, 1, 3));
        let negatives: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agregats_jour WHERE valeur < 0")
                .fetch_one(e.state.pool())
                .await
                .unwrap();
        assert_eq!(negatives, 0);
    }
}
