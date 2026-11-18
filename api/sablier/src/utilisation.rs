use std::collections::BTreeMap;

use axum::{
    extract::{Query, State},
    response::Response,
};
use serde_json::{Value, json};

use crate::{
    agregats::{self, UTILISATION},
    battement::decouper,
    etat::AppState,
    garde::Personne,
    http::{bd, ok},
    periode::Demande,
};

fn repartition(par_cle: &[(String, i64)], champ: &str, partie: fn(&str) -> &str) -> Vec<Value> {
    let mut totaux: BTreeMap<&str, i64> = BTreeMap::new();
    for (cle, secondes) in par_cle {
        *totaux.entry(partie(cle)).or_default() += secondes;
    }
    let mut tries: Vec<(&str, i64)> = totaux.into_iter().collect();
    tries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    tries
        .into_iter()
        .map(|(nom, secondes)| json!({ champ: nom, "seconds": secondes }))
        .collect()
}

pub async fn lire(
    State(state): State<AppState>,
    p: Personne,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    let (pool, sub) = (state.pool(), p.sub());
    let premier = agregats::premier_jour(pool, sub).await.map_err(bd)?;
    let intervalle = plage.intervalle(premier);
    let (du, au) = (intervalle.from, intervalle.to);
    let secondes = agregats::somme(pool, sub, UTILISATION, du, au)
        .await
        .map_err(bd)?;
    let par_jour = agregats::par_jour(pool, sub, UTILISATION, du, au)
        .await
        .map_err(bd)?;
    let par_cle = agregats::par_cle(pool, sub, UTILISATION, du, au)
        .await
        .map_err(bd)?;
    let plus_longue = agregats::max_jour(pool, sub, UTILISATION, du, au)
        .await
        .map_err(bd)?;
    let actifs = agregats::jours_actifs(pool, sub, UTILISATION, du, au)
        .await
        .map_err(bd)?;
    Ok(ok(&json!({
        "period": intervalle.period,
        "from": du,
        "to": au,
        "seconds": secondes,
        "by_day": par_jour
            .iter()
            .map(|(jour, s)| json!({ "day": jour, "seconds": s }))
            .collect::<Vec<_>>(),
        "by_section": repartition(&par_cle, "section", |c| decouper(c).1),
        "by_platform": repartition(&par_cle, "platform", |c| decouper(c).0),
        "longest_day": plus_longue.map(|(jour, s)| json!({ "day": jour, "seconds": s })),
        "active_days": actifs,
        "first_day": premier,
    })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    use crate::testutil::{env, jour};

    #[tokio::test]
    async fn utilisation_par_periode_section_et_plateforme() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        for (sub, j, cle, n) in [
            ("sub-alice", "2026-03-02", "web|messagerie", 600),
            ("sub-alice", "2026-03-02", "android|sillon", 300),
            ("sub-alice", "2026-03-04", "web|sillon", 1200),
            ("sub-alice", "2026-03-09", "windows|entracte", 60),
            ("sub-alice", "2025-12-31", "web|messagerie", 30),
            ("sub-admin", "2026-03-02", "web|messagerie", 9999),
        ] {
            agregats::ajouter(pool, sub, jour(j), UTILISATION, cle, n)
                .await
                .unwrap();
        }
        agregats::ajouter(pool, "sub-alice", jour("2024-01-01"), "autre.stat", "", 5)
            .await
            .unwrap();
        let r = e.get("alice", "/usage?period=week&date=2026-03-04").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(
            r.json(),
            json!({
                "period": "week",
                "from": "2026-03-02",
                "to": "2026-03-08",
                "seconds": 2100,
                "by_day": [
                    {"day": "2026-03-02", "seconds": 900},
                    {"day": "2026-03-04", "seconds": 1200},
                ],
                "by_section": [
                    {"section": "sillon", "seconds": 1500},
                    {"section": "messagerie", "seconds": 600},
                ],
                "by_platform": [
                    {"platform": "web", "seconds": 1800},
                    {"platform": "android", "seconds": 300},
                ],
                "longest_day": {"day": "2026-03-04", "seconds": 1200},
                "active_days": 2,
                "first_day": "2024-01-01",
            })
        );
        let r = e
            .get("alice", "/usage?period=day&date=2026-03-03")
            .await
            .json();
        assert_eq!(
            (
                r["seconds"].clone(),
                r["longest_day"].clone(),
                r["active_days"].clone()
            ),
            (json!(0), Value::Null, json!(0))
        );
        assert_eq!(r["by_day"], json!([]));
        let r = e
            .get("alice", "/usage?period=month&date=2026-03-31")
            .await
            .json();
        assert_eq!(
            (r["from"].clone(), r["to"].clone()),
            (json!("2026-03-01"), json!("2026-03-31"))
        );
        assert_eq!(
            (r["seconds"].clone(), r["active_days"].clone()),
            (json!(2160), json!(3))
        );
        let r = e
            .get("alice", "/usage?period=year&date=2026-07-01")
            .await
            .json();
        assert_eq!(
            (r["from"].clone(), r["to"].clone()),
            (json!("2026-01-01"), json!("2026-12-31"))
        );
        assert_eq!(r["seconds"], 2160);
        let r = e
            .get("alice", "/usage?period=all&date=2026-12-31")
            .await
            .json();
        assert_eq!(
            (r["from"].clone(), r["to"].clone()),
            (json!("2024-01-01"), json!("2026-12-31"))
        );
        assert_eq!(
            (r["seconds"].clone(), r["active_days"].clone()),
            (json!(2190), json!(4))
        );
        let r = e.get("alice", "/usage").await.json();
        assert_eq!(r["period"], "month");
        let r = e.get("bruno", "/usage?period=all").await.json();
        assert_eq!(
            (r["seconds"].clone(), r["first_day"].clone()),
            (json!(0), Value::Null)
        );
        assert_eq!(r["from"], r["to"]);
        for faux in [
            "/usage?period=decade",
            "/usage?date=hier",
            "/usage?period=week&date=2026-13-01",
        ] {
            let r = e.get("alice", faux).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
            assert_eq!(r.json()["error"], "invalid");
        }
        assert_eq!(
            e.get("personne", "/usage").await.statut,
            StatusCode::FORBIDDEN
        );
    }
}
