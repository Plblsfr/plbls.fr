use axum::{
    extract::{Query, State},
    response::Response,
};
use chrono::NaiveDate;
use commun::evenements::{Donnees, Evenement};
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

pub const SECONDES: &str = "sillon.secondes";
pub const ECOUTES: &str = "sillon.ecoutes";
pub const PISTES: &str = "sillon.pistes";

pub async fn appliquer(
    tx: &mut PgConnection,
    e: &Evenement,
    jour: NaiveDate,
) -> Result<(), sqlx::Error> {
    if let Donnees::SillonEcoute {
        piste,
        secondes,
        comptee,
    } = &e.donnees
    {
        agregats::ajouter(&mut *tx, &e.sub, jour, SECONDES, "", i64::from(*secondes)).await?;
        if *comptee {
            agregats::ajouter(&mut *tx, &e.sub, jour, ECOUTES, "", 1).await?;
            agregats::ajouter(tx, &e.sub, jour, PISTES, piste, 1).await?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub access: bool,
    #[serde(flatten)]
    pub intervalle: Intervalle,
    pub seconds: i64,
    pub tracks_played: i64,
    pub distinct_tracks: i64,
}

pub async fn lire(pool: &PgPool, sub: &str, plage: &Plage) -> Result<Stats, sqlx::Error> {
    let intervalle = plage.intervalle_de(pool, sub).await?;
    let (du, au) = (intervalle.from, intervalle.to);
    Ok(Stats {
        access: true,
        intervalle,
        seconds: agregats::somme(pool, sub, SECONDES, du, au).await?,
        tracks_played: agregats::somme(pool, sub, ECOUTES, du, au).await?,
        distinct_tracks: agregats::cles_distinctes(pool, sub, PISTES, du, au).await?,
    })
}

pub async fn voir(
    State(state): State<AppState>,
    p: Personne,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    if !p.droits.sillon {
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
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        file::Issue,
        testutil::{Env, env},
    };

    const SUB: &str = "sub-bruno";

    fn ecoute(piste: &str, secondes: u32, comptee: bool) -> Donnees {
        Donnees::SillonEcoute {
            piste: piste.into(),
            secondes,
            comptee,
        }
    }

    async fn lue(e: &Env, requete: &str) -> Value {
        let r = e.get("bruno", &format!("/sillon?{requete}")).await;
        assert_eq!(r.statut, StatusCode::OK);
        r.json()
    }

    fn totaux(r: &Value) -> (i64, i64, i64) {
        (
            r["seconds"].as_i64().unwrap(),
            r["tracks_played"].as_i64().unwrap(),
            r["distinct_tracks"].as_i64().unwrap(),
        )
    }

    #[tokio::test]
    async fn ecoute_courte_non_comptee_et_ecoute_comptee() {
        let Some(e) = env().await else { return };
        let mars = "period=month&date=2026-03-10";
        e.recevoir(
            "a",
            SUB,
            "2026-03-10T10:00:00Z",
            false,
            ecoute("p1", 20, false),
        )
        .await;
        assert_eq!(totaux(&lue(&e, mars).await), (20, 0, 0));
        e.recevoir(
            "b",
            SUB,
            "2026-03-10T10:05:00Z",
            false,
            ecoute("p2", 120, true),
        )
        .await;
        assert_eq!(
            lue(&e, mars).await,
            json!({
                "access": true,
                "period": "month",
                "from": "2026-03-01",
                "to": "2026-03-31",
                "seconds": 140,
                "tracks_played": 1,
                "distinct_tracks": 1,
            })
        );
        assert_eq!(
            totaux(&lue(&e, "period=day&date=2026-03-09").await),
            (0, 0, 0)
        );
    }

    #[tokio::test]
    async fn meme_piste_trois_fois_un_seul_titre_different() {
        let Some(e) = env().await else { return };
        for (i, date) in [
            "2026-03-10T10:00:00Z",
            "2026-03-10T11:00:00Z",
            "2026-03-12T10:00:00Z",
        ]
        .into_iter()
        .enumerate()
        {
            e.recevoir(&format!("e{i}"), SUB, date, false, ecoute("p1", 200, true))
                .await;
        }
        let mars = "period=month&date=2026-03-10";
        assert_eq!(totaux(&lue(&e, mars).await), (600, 3, 1));
        e.recevoir(
            "autre",
            SUB,
            "2026-03-12T10:10:00Z",
            false,
            ecoute("p2", 100, true),
        )
        .await;
        e.recevoir(
            "courte",
            SUB,
            "2026-03-12T10:20:00Z",
            false,
            ecoute("p3", 5, false),
        )
        .await;
        e.recevoir(
            "ailleurs",
            "sub-alice",
            "2026-03-12T10:20:00Z",
            false,
            ecoute("p4", 100, true),
        )
        .await;
        assert_eq!(totaux(&lue(&e, mars).await), (705, 4, 2));
        assert_eq!(
            totaux(&lue(&e, "period=day&date=2026-03-10").await),
            (400, 2, 1)
        );
    }

    #[tokio::test]
    async fn import_rejoue_deux_fois_memes_totaux() {
        let Some(e) = env().await else { return };
        let historique = [
            (
                "import:sillon:ecoute:1",
                "2025-05-01T08:00:00Z",
                ecoute("p1", 180, true),
            ),
            (
                "import:sillon:ecoute:2",
                "2025-05-01T08:03:00Z",
                ecoute("p1", 180, true),
            ),
            (
                "import:sillon:ecoute:3",
                "2025-05-02T08:00:00Z",
                ecoute("p2", 12, false),
            ),
        ];
        let mut lues = Vec::new();
        for attendue in [Issue::Applique, Issue::Doublon] {
            for (id, date, donnees) in historique.clone() {
                assert_eq!(e.recevoir(id, SUB, date, true, donnees).await, attendue);
            }
            lues.push(lue(&e, "period=year&date=2025-01-01").await);
        }
        assert_eq!(lues[0], lues[1]);
        assert_eq!(totaux(&lues[0]), (372, 2, 1));
    }
}
