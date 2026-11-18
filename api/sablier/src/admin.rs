use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    response::Response,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    agregats::UTILISATION,
    domaines::{entracte, messagerie, sillon},
    droits::Droits,
    effacement,
    etat::AppState,
    garde::Personne,
    http::{bd, introuvable, invalide, ok, sans_contenu},
    journal::{self, Action},
    periode::{Demande, Plage},
    recaps, series, trophees, utilisation,
};

pub const TOUT: Droits = Droits {
    access: true,
    admin: false,
    entracte: true,
    sillon: true,
};

async fn consulter(state: &AppState, admin: &Personne, cible: &str) -> Result<(), Response> {
    journal::consulter(state.pool(), &admin.identite, cible, Utc::now())
        .await
        .map_err(bd)?;
    Ok(())
}

pub async fn voir_utilisation(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    consulter(&state, &admin, &sub).await?;
    Ok(ok(&utilisation::lire(state.pool(), &sub, &plage)
        .await
        .map_err(bd)?))
}

pub async fn voir_entracte(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    consulter(&state, &admin, &sub).await?;
    Ok(ok(&entracte::lire(state.pool(), &sub, &plage)
        .await
        .map_err(bd)?))
}

pub async fn voir_sillon(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    consulter(&state, &admin, &sub).await?;
    Ok(ok(&sillon::lire(state.pool(), &sub, &plage)
        .await
        .map_err(bd)?))
}

pub async fn voir_messagerie(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    consulter(&state, &admin, &sub).await?;
    Ok(ok(&messagerie::lire(state.pool(), &sub, &plage)
        .await
        .map_err(bd)?))
}

pub async fn voir_trophees(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
) -> Result<Response, Response> {
    consulter(&state, &admin, &sub).await?;
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    Ok(ok(&trophees::lire(state.pool(), &sub, &TOUT, aujourd_hui)
        .await
        .map_err(bd)?))
}

pub async fn voir_series(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
) -> Result<Response, Response> {
    consulter(&state, &admin, &sub).await?;
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    Ok(ok(&series::lire(state.pool(), &sub, &TOUT, aujourd_hui)
        .await
        .map_err(bd)?))
}

pub async fn voir_recaps(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
) -> Result<Response, Response> {
    consulter(&state, &admin, &sub).await?;
    Ok(ok(&recaps::lister(state.pool(), &sub).await.map_err(bd)?))
}

pub async fn voir_recap(
    State(state): State<AppState>,
    admin: Personne,
    Path((sub, id)): Path<(String, String)>,
) -> Result<Response, Response> {
    consulter(&state, &admin, &sub).await?;
    let id: i64 = id.parse().map_err(|_| introuvable())?;
    recaps::lire(state.pool(), &sub, id)
        .await
        .map_err(bd)?
        .map(|c| ok(&c))
        .ok_or_else(introuvable)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Fiche {
    pub sub: String,
    pub name: Option<String>,
    pub known: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub first_day: Option<NaiveDate>,
    pub last_day: Option<NaiveDate>,
    pub active_seconds_total: i64,
}

pub async fn personnes(pool: &PgPool, sans_compte: bool) -> Result<Vec<Fiche>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.sub, NULLIF(p.nom, '') AS name, p.sub IS NOT NULL AS known, p.vu_le AS last_seen,
            MIN(a.jour) FILTER (WHERE a.valeur > 0) AS first_day,
            MAX(a.jour) FILTER (WHERE a.valeur > 0) AS last_day,
            COALESCE(SUM(a.valeur) FILTER (WHERE a.stat = $1), 0)::bigint AS active_seconds_total
        FROM agregats_jour a LEFT JOIN personnes p ON p.sub = a.sub
        GROUP BY a.sub, p.sub, p.nom, p.vu_le
        HAVING NOT $2 OR p.sub IS NULL
        ORDER BY lower(NULLIF(p.nom, '')) NULLS LAST, a.sub",
    )
    .bind(UTILISATION)
    .bind(sans_compte)
    .fetch_all(pool)
    .await
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Filtre {
    pub orphans: Option<String>,
}

pub async fn voir_personnes(
    State(state): State<AppState>,
    Query(filtre): Query<Filtre>,
) -> Result<Response, Response> {
    let sans_compte = match filtre.orphans.as_deref() {
        None | Some("" | "false") => false,
        Some("true") => true,
        Some(_) => return Err(invalide("orphans : true ou false")),
    };
    Ok(ok(&personnes(state.pool(), sans_compte)
        .await
        .map_err(bd)?))
}

type Totaux = (Option<NaiveDate>, i64, i64, i64, i64, i64);

fn totaux_en_json((_, personnes, utilisation, regarde, ecoute, messages): &Totaux) -> Value {
    json!({
        "active_people": personnes,
        "usage_seconds": utilisation,
        "watched_seconds": regarde,
        "listened_seconds": ecoute,
        "messages": messages,
    })
}

pub async fn totaux(pool: &PgPool, plage: &Plage) -> Result<Value, sqlx::Error> {
    let premier: Option<NaiveDate> =
        sqlx::query_scalar("SELECT MIN(jour) FROM agregats_jour WHERE valeur > 0")
            .fetch_one(pool)
            .await?;
    let intervalle = plage.intervalle(premier);
    let lignes: Vec<Totaux> = sqlx::query_as(
        "SELECT jour, COUNT(DISTINCT sub) FILTER (WHERE valeur > 0 AND stat <> $7),
            COALESCE(SUM(valeur) FILTER (WHERE stat = $3), 0)::bigint,
            COALESCE(SUM(valeur) FILTER (WHERE stat = $4), 0)::bigint,
            COALESCE(SUM(valeur) FILTER (WHERE stat = $5), 0)::bigint,
            COALESCE(SUM(valeur) FILTER (WHERE stat = $6), 0)::bigint
        FROM agregats_jour WHERE jour BETWEEN $1 AND $2
        GROUP BY GROUPING SETS ((jour), ()) ORDER BY jour NULLS FIRST",
    )
    .bind(intervalle.from)
    .bind(intervalle.to)
    .bind(UTILISATION)
    .bind(entracte::SECONDES)
    .bind(sillon::SECONDES)
    .bind(messagerie::MESSAGES)
    .bind(messagerie::REACTIONS_RECUES)
    .fetch_all(pool)
    .await?;
    let total = lignes
        .iter()
        .find(|l| l.0.is_none())
        .copied()
        .unwrap_or_default();
    let mut reponse = totaux_en_json(&total);
    reponse["period"] = json!(intervalle.period);
    reponse["from"] = json!(intervalle.from);
    reponse["to"] = json!(intervalle.to);
    reponse["by_day"] = lignes
        .iter()
        .filter(|l| l.0.is_some())
        .map(|l| {
            let mut jour = totaux_en_json(l);
            jour["day"] = json!(l.0);
            jour
        })
        .collect();
    Ok(reponse)
}

pub async fn voir_totaux(
    State(state): State<AppState>,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    Ok(ok(&totaux(state.pool(), &plage).await.map_err(bd)?))
}

pub async fn effacer(
    State(state): State<AppState>,
    admin: Personne,
    Path(sub): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    effacement::confirmer(&octets)?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    effacement::effacer_dans(&mut tx, &sub).await.map_err(bd)?;
    journal::inscrire(
        &mut tx,
        &admin.identite,
        &sub,
        Action::Effacement,
        Utc::now(),
    )
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    use crate::{
        agregats,
        effacement::CONFIRMATION,
        routes,
        testutil::{env, jour},
    };

    #[tokio::test]
    async fn c1_toute_route_d_administration_repond_403_sans_sablier_admin() {
        let Some(e) = env().await else { return };
        e.remplir("bruno").await;
        let avant = e.lignes_de("sub-bruno").await;
        let mut vues = 0;
        for (route, _) in routes::table() {
            if !route.chemin.starts_with("/admin/") {
                continue;
            }
            vues += 1;
            let chemin = route
                .chemin
                .replace("{sub}", "sub-bruno")
                .replace("{id}", "1");
            let m = route.methode.as_str();
            let corps = (m != "GET").then(|| json!({"confirm": CONFIRMATION}));
            for qui in ["alice", "bruno", "personne"] {
                let r = e.appel(Some(qui), m, &chemin, corps.clone()).await;
                assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin} pour {qui}");
                assert_eq!(r.json()["error"], "forbidden", "{m} {chemin}");
                assert!(r.texte().contains("sablier:admin"), "{m} {chemin}");
            }
        }
        assert_eq!(vues, 16);
        assert_eq!(e.lignes_de("sub-bruno").await, avant);
        let journal: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM consultations")
            .fetch_one(e.state.pool())
            .await
            .unwrap();
        assert_eq!(journal, 0);
        let r = e.get("alice", "/usage?period=all&sub=sub-bruno").await;
        assert_eq!(
            (r.statut, r.json()["seconds"].clone()),
            (StatusCode::OK, json!(0))
        );
        let r = e
            .get("alice", "/messaging?period=all&sub=sub-bruno")
            .await
            .json();
        assert_eq!(r["total"], 0);
    }

    #[tokio::test]
    async fn vue_d_une_personne_identique_a_la_sienne_sans_rien_modifier() {
        let Some(e) = env().await else { return };
        let e = &e;
        e.remplir("alice").await;
        let etat = || async {
            let pool = e.state.pool();
            let vus: i64 = sqlx::query_scalar(
                "SELECT (SELECT COUNT(*) FROM deblocages WHERE vu_le IS NOT NULL)
                    + (SELECT COUNT(*) FROM recaps WHERE vu_le IS NOT NULL)",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            (e.lignes_de("sub-alice").await, vus)
        };
        let avant = etat().await;
        let recap: i64 = sqlx::query_scalar("SELECT id FROM recaps WHERE sub = 'sub-alice'")
            .fetch_one(e.state.pool())
            .await
            .unwrap();
        let requete = "period=day&date=2026-03-10";
        let vue = |chemin: String| async move {
            let r = e
                .get("admin", &format!("/admin/people/sub-alice{chemin}"))
                .await;
            assert_eq!(r.statut, StatusCode::OK, "{chemin}");
            r.json()
        };
        let entracte = vue(format!("/entracte?{requete}")).await;
        assert_eq!(
            (entracte["access"].clone(), entracte["seconds"].clone()),
            (json!(true), json!(600))
        );
        let sillon = vue(format!("/sillon?{requete}")).await;
        assert_eq!(
            (sillon["access"].clone(), sillon["seconds"].clone()),
            (json!(true), json!(120))
        );
        let series = vue("/streaks".into()).await;
        assert!(series["entracte"].is_object() && series["sillon"].is_object());
        let trophees = vue("/trophies".into()).await;
        assert_eq!(trophees["unlocked"][0]["key"], "premier_jour");
        assert!(trophees["hidden_remaining"].as_i64().unwrap() > 0);
        let caches = [
            "signature",
            "droit_au_but",
            "zappeur",
            "habitant_du_salon",
            "mois_parfait",
        ];
        assert!(!caches.iter().any(|c| trophees.to_string().contains(c)));
        let contenu = vue(format!("/recaps/{recap}")).await;
        assert_eq!(contenu["marque"], "recap-sub-alice");
        assert_eq!(etat().await, avant);
        for inconnu in ["/recaps/999999", "/recaps/abc"] {
            let r = e
                .get("admin", &format!("/admin/people/sub-alice{inconnu}"))
                .await;
            assert_eq!(r.statut, StatusCode::NOT_FOUND, "{inconnu}");
        }
        let r = e
            .get("admin", "/admin/people/sub-alice/usage?period=siecle")
            .await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
        for chemin in [
            format!("/usage?{requete}"),
            format!("/messaging?{requete}"),
            "/recaps".to_string(),
            format!("/recaps/{recap}"),
        ] {
            let sienne = e.get("alice", &chemin).await.json();
            assert_eq!(vue(chemin).await, sienne);
        }
        let vide = vue_vide(e, "/usage?period=all").await;
        assert_eq!(
            (vide["seconds"].clone(), vide["first_day"].clone()),
            (json!(0), Value::Null)
        );
    }

    async fn vue_vide(e: &crate::testutil::Env, chemin: &str) -> Value {
        e.get("admin", &format!("/admin/people/sub-inconnu{chemin}"))
            .await
            .json()
    }

    #[tokio::test]
    async fn totaux_globaux_par_periode() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let vide = e
            .get("admin", "/admin/totals?period=all&date=2026-03-31")
            .await;
        assert_eq!(
            vide.json(),
            json!({
                "period": "all", "from": "2026-03-31", "to": "2026-03-31", "active_people": 0,
                "usage_seconds": 0, "watched_seconds": 0, "listened_seconds": 0, "messages": 0, "by_day": [],
            })
        );
        for (sub, j, stat, cle, n) in [
            ("sub-alice", "2026-03-02", UTILISATION, "web|sablier", 600),
            ("sub-alice", "2026-03-02", messagerie::MESSAGES, "prive", 4),
            ("sub-bruno", "2026-03-02", sillon::SECONDES, "", 300),
            ("sub-bruno", "2026-03-03", entracte::SECONDES, "", 900),
            ("sub-bruno", "2026-03-03", UTILISATION, "web|entracte", 100),
            (
                "sub-diane",
                "2026-03-03",
                messagerie::REACTIONS_RECUES,
                "",
                7,
            ),
            ("sub-diane", "2026-03-04", UTILISATION, "web|sablier", 0),
            ("sub-diane", "2026-04-01", messagerie::MESSAGES, "a/b", 1),
            ("sub-alice", "2025-12-31", UTILISATION, "web|sablier", 50),
        ] {
            agregats::ajouter(pool, sub, jour(j), stat, cle, n)
                .await
                .unwrap();
        }
        let r = e
            .get("admin", "/admin/totals?period=month&date=2026-03-15")
            .await;
        assert_eq!(r.statut, StatusCode::OK);
        let jour_de = |day: &str, personnes: i64, valeurs: [i64; 4]| {
            json!({
                "day": day, "active_people": personnes, "usage_seconds": valeurs[0],
                "watched_seconds": valeurs[1], "listened_seconds": valeurs[2], "messages": valeurs[3],
            })
        };
        assert_eq!(
            r.json(),
            json!({
                "period": "month", "from": "2026-03-01", "to": "2026-03-31", "active_people": 2,
                "usage_seconds": 700, "watched_seconds": 900, "listened_seconds": 300, "messages": 4,
                "by_day": [
                    jour_de("2026-03-02", 2, [600, 0, 300, 4]),
                    jour_de("2026-03-03", 1, [100, 900, 0, 0]),
                    jour_de("2026-03-04", 0, [0, 0, 0, 0]),
                ],
            })
        );
        let r = e
            .get("admin", "/admin/totals?period=all&date=2026-04-30")
            .await
            .json();
        assert_eq!(
            (
                r["from"].clone(),
                r["to"].clone(),
                r["active_people"].clone()
            ),
            (json!("2025-12-31"), json!("2026-04-30"), json!(3))
        );
        assert_eq!(
            (r["usage_seconds"].clone(), r["messages"].clone()),
            (json!(750), json!(5))
        );
        let r = e
            .get("admin", "/admin/totals?period=day&date=2026-03-03")
            .await
            .json();
        assert_eq!(
            (r["active_people"].clone(), r["watched_seconds"].clone()),
            (json!(1), json!(900))
        );
        assert_eq!(
            e.get("admin", "/admin/totals").await.json()["period"],
            "month"
        );
        let r = e.get("admin", "/admin/totals?period=siecle").await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn c6_effacement_par_un_admin_meme_effet_et_personnes_sans_compte_listees() {
        let Some(e) = env().await else { return };
        let e = &e;
        let vierge = e.lectures("alice").await;
        e.remplir("alice").await;
        e.remplir("bruno").await;
        let pool = e.state.pool();
        for (j, n) in [("2026-01-05", 40), ("2026-02-07", 60)] {
            agregats::ajouter(pool, "sub-fantome", jour(j), UTILISATION, "web|sablier", n)
                .await
                .unwrap();
        }
        let vue_le = "2026-03-10T10:00:00Z";
        let fiche = |qui: &str| {
            json!({
                "sub": format!("sub-{qui}"), "name": qui, "known": true, "last_seen": vue_le,
                "first_day": "2026-03-10", "last_day": "2026-03-10", "active_seconds_total": 20,
            })
        };
        let fantome = json!({
            "sub": "sub-fantome", "name": null, "known": false, "last_seen": null,
            "first_day": "2026-01-05", "last_day": "2026-02-07", "active_seconds_total": 100,
        });
        let r = e.get("admin", "/admin/people").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.json(), json!([fiche("alice"), fiche("bruno"), fantome]));
        let sans_compte = e.get("admin", "/admin/people?orphans=true").await.json();
        assert_eq!(sans_compte, json!([fantome]));
        assert_eq!(
            e.get("admin", "/admin/people?orphans=false").await.json(),
            r.json()
        );
        let r = e.get("admin", "/admin/people?orphans=oui").await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
        let bruno = e.lectures("bruno").await;
        let lignes_de_bruno = e.lignes_de("sub-bruno").await;
        let effacer = |sub: &'static str, corps: Value| async move {
            let chemin = format!("/admin/people/{sub}/data");
            e.appel(Some("admin"), "DELETE", &chemin, Some(corps)).await
        };
        let r = effacer("sub-alice", json!({"confirm": "oui"})).await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
        assert_eq!(r.json()["error"], "invalid");
        assert!(e.lignes_de("sub-alice").await.iter().all(|(_, n)| *n > 0));
        let confirmation = json!({"confirm": CONFIRMATION});
        let r = effacer("sub-alice", confirmation.clone()).await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        for (table, n) in e.lignes_de("sub-alice").await {
            assert_eq!(n, 0, "{table} garde des lignes d'alice");
        }
        assert_eq!(e.lectures("alice").await, vierge);
        assert_eq!(e.lignes_de("sub-bruno").await, lignes_de_bruno);
        assert_eq!(e.lectures("bruno").await, bruno);
        let vues = e.get("alice", "/views").await.json();
        assert_eq!(vues.as_array().unwrap().len(), 1, "{vues}");
        assert_eq!(
            (vues[0]["admin_name"].clone(), vues[0]["action"].clone()),
            (json!("admin"), json!("effacement"))
        );
        let r = effacer("sub-fantome", confirmation.clone()).await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert_eq!(
            e.get("admin", "/admin/people?orphans=true").await.json(),
            json!([])
        );
        assert_eq!(
            e.get("admin", "/admin/people").await.json(),
            json!([fiche("bruno")])
        );
        let r = effacer("sub-admin", confirmation).await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let journal = e.get("admin", "/admin/audit").await.json();
        let lignes: Vec<(&str, &str)> = journal["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["target_sub"].as_str().unwrap(),
                    l["action"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            lignes,
            [("sub-fantome", "effacement"), ("sub-alice", "effacement")]
        );
    }
}
