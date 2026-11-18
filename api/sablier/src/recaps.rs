use axum::{
    extract::{Path, State},
    response::Response,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    domaines::{entracte, messagerie, sillon},
    droits::Droits,
    etat::AppState,
    garde::Personne,
    http::{bd, introuvable, ok},
    periode::{Periode, Plage},
    series::{self, Serie},
    trophees, utilisation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genre {
    Mois,
    Annee,
}

impl Genre {
    pub fn base(self) -> &'static str {
        match self {
            Genre::Mois => "mois",
            Genre::Annee => "annee",
        }
    }

    pub fn periode(self) -> Periode {
        match self {
            Genre::Mois => Periode::Mois,
            Genre::Annee => Periode::Annee,
        }
    }

    pub fn etiquette(self, du: NaiveDate) -> String {
        match self {
            Genre::Mois => du.format("%Y-%m").to_string(),
            Genre::Annee => du.format("%Y").to_string(),
        }
    }
}

pub fn periodes_closes(aujourd_hui: NaiveDate) -> [(Genre, NaiveDate, NaiveDate); 2] {
    [Genre::Mois, Genre::Annee].map(|genre| {
        let (debut, _) = genre.periode().bornes(aujourd_hui);
        let (du, au) = genre.periode().bornes(debut.pred_opt().unwrap_or(debut));
        (genre, du, au)
    })
}

fn faits_marquants(
    usage: &Value,
    entracte: Option<&entracte::Stats>,
    messagerie: &messagerie::Stats,
) -> Vec<Value> {
    let mut faits = Vec::new();
    let mut fait = |genre: &str, valeur: i64, mut detail: Value| {
        if valeur > 0 {
            detail["kind"] = json!(genre);
            detail["value"] = json!(valeur);
            faits.push(detail);
        }
    };
    let plus_longue = &usage["longest_day"];
    fait(
        "longest_day",
        plus_longue["seconds"].as_i64().unwrap_or(0),
        json!({ "day": plus_longue["day"] }),
    );
    if let Some(e) = entracte {
        if let Some(j) = &e.biggest_day {
            fait("biggest_day", j.seconds, json!({ "day": j.day }));
        }
        fait("longest_session", e.longest_session, json!({}));
        fait("episodes_in_a_row", e.most_episodes_in_a_row, json!({}));
    }
    if let Some(j) = &messagerie.busiest_day {
        fait("busiest_day", j.messages, json!({ "day": j.day }));
    }
    if let Some(s) = &messagerie.top_channel {
        fait(
            "top_channel",
            s.messages,
            json!({ "server": s.server, "channel": s.channel }),
        );
    }
    if let Some(e) = &messagerie.top_emoji {
        fait("top_emoji", e.count, json!({ "emoji": e.emoji }));
    }
    fait("longest_call", messagerie.longest_call, json!({}));
    if let Some(s) = &messagerie.top_voice_channel {
        fait(
            "top_voice_channel",
            s.seconds,
            json!({ "server": s.server, "channel": s.channel }),
        );
    }
    faits
}

pub async fn contenu(
    pool: &PgPool,
    sub: &str,
    droits: &Droits,
    genre: Genre,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Value, sqlx::Error> {
    let plage = Plage {
        periode: genre.periode(),
        date: du,
        du,
        au,
    };
    let usage = utilisation::lire(pool, sub, &plage).await?;
    let messagerie = messagerie::lire(pool, sub, &plage).await?;
    let entracte = if droits.entracte {
        Some(entracte::lire(pool, sub, &plage).await?)
    } else {
        None
    };
    let mut trophees: Vec<trophees::Public> = trophees::etats(pool, sub, droits)
        .await?
        .iter()
        .filter_map(|e| e.public(du, au))
        .collect();
    trophees.sort_by_key(|t| t.unlocked_at);
    let mut c = json!({
        "kind": genre.periode().nom(),
        "period": genre.etiquette(du),
        "from": du,
        "to": au,
        "highlights": faits_marquants(&usage, entracte.as_ref(), &messagerie),
        "usage": usage,
        "messaging": messagerie,
        "trophies": trophees,
        "streak": {},
    });
    for (nom, serie, inclus) in [
        ("global", Serie::Globale, true),
        ("entracte", Serie::Entracte, droits.entracte),
        ("sillon", Serie::Sillon, droits.sillon),
        ("messaging", Serie::Messagerie, true),
    ] {
        if inclus {
            let jours = series::jours(pool, sub, serie, du, au).await?;
            c["streak"][nom] = json!(series::calculer(&jours, au).best);
        }
    }
    if let Some(e) = entracte {
        c["entracte"] = json!(e);
    }
    if droits.sillon {
        c["sillon"] = json!(sillon::lire(pool, sub, &plage).await?);
    }
    Ok(c)
}

pub async fn creer_les_recaps(
    state: &AppState,
    aujourd_hui: NaiveDate,
) -> Result<u64, sqlx::Error> {
    let Some(aegis) = state.aegis() else {
        return Ok(0);
    };
    let (pool, cfg) = (state.pool(), state.config());
    let mut crees = 0;
    for (genre, du, au) in periodes_closes(aujourd_hui) {
        let minimum = match genre {
            Genre::Mois => cfg.recap_jours_min,
            Genre::Annee => cfg.recap_jours_min_annee,
        };
        let periode = genre.etiquette(du);
        let subs: Vec<String> = sqlx::query_scalar(
            "SELECT a.sub FROM agregats_jour a
            WHERE a.stat = ANY($1) AND a.valeur > 0 AND a.jour BETWEEN $2 AND $3
                AND NOT EXISTS (
                    SELECT 1 FROM recaps r WHERE r.sub = a.sub AND r.genre = $4 AND r.periode = $5
                )
            GROUP BY a.sub HAVING COUNT(DISTINCT a.jour) >= $6 ORDER BY a.sub",
        )
        .bind(Serie::Globale.stats())
        .bind(du)
        .bind(au)
        .bind(genre.base())
        .bind(&periode)
        .bind(i64::from(minimum))
        .fetch_all(pool)
        .await?;
        for sub in subs {
            let Ok(droits) = aegis.acces_services(&sub).await else {
                tracing::warn!("récaps : aegis injoignable, création reportée à la passe suivante");
                return Ok(crees);
            };
            trophees::evaluer(pool, &sub, aujourd_hui).await?;
            let contenu = contenu(pool, &sub, &droits, genre, du, au).await?;
            crees += sqlx::query(
                "INSERT INTO recaps (sub, genre, periode, contenu) VALUES ($1, $2, $3, $4)
                ON CONFLICT (sub, genre, periode) DO NOTHING",
            )
            .bind(&sub)
            .bind(genre.base())
            .bind(&periode)
            .bind(sqlx::types::Json(contenu))
            .execute(pool)
            .await?
            .rows_affected();
        }
    }
    Ok(crees)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Resume {
    pub id: i64,
    pub kind: &'static str,
    pub period: String,
    pub created_at: DateTime<Utc>,
    pub seen: bool,
}

pub async fn lister(pool: &PgPool, sub: &str) -> Result<Vec<Resume>, sqlx::Error> {
    let lignes: Vec<(i64, String, String, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT id, genre, periode, cree_le, vu_le IS NOT NULL FROM recaps
        WHERE sub = $1 ORDER BY cree_le DESC, id DESC",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|(id, genre, period, created_at, seen)| Resume {
            id,
            kind: if genre == Genre::Annee.base() {
                Periode::Annee.nom()
            } else {
                Periode::Mois.nom()
            },
            period,
            created_at,
            seen,
        })
        .collect())
}

pub async fn lire(pool: &PgPool, sub: &str, id: i64) -> Result<Option<Value>, sqlx::Error> {
    let contenu: Option<sqlx::types::Json<Value>> =
        sqlx::query_scalar("SELECT contenu FROM recaps WHERE sub = $1 AND id = $2")
            .bind(sub)
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(contenu.map(|c| {
        let mut v = c.0;
        v["id"] = json!(id);
        v
    }))
}

pub async fn non_vus(pool: &PgPool, sub: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM recaps WHERE sub = $1 AND vu_le IS NULL")
        .bind(sub)
        .fetch_one(pool)
        .await
}

pub async fn marquer_vus(
    pool: &PgPool,
    sub: &str,
    tous: bool,
    ids: &[i64],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE recaps SET vu_le = now() WHERE sub = $1 AND vu_le IS NULL AND ($2 OR id = ANY($3))",
    )
    .bind(sub)
    .bind(tous)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn voir_liste(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    Ok(ok(&lister(state.pool(), p.sub()).await.map_err(bd)?))
}

pub async fn voir(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id: i64 = id.parse().map_err(|_| introuvable())?;
    lire(state.pool(), p.sub(), id)
        .await
        .map_err(bd)?
        .map(|c| ok(&c))
        .ok_or_else(introuvable)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::Days;
    use commun::evenements::{Donnees, Genre as GenreDeTitre};

    use super::*;
    use crate::{
        agregats,
        testutil::{Env, env, env_avec, jour},
    };

    const ALICE: &str = "sub-alice";
    const DIANE: &str = "sub-diane";
    const NOVEMBRE: &str = "2026-11-01";

    async fn ajouter(e: &Env, sub: &str, j: &str, stat: &str, cle: &str, n: i64) {
        agregats::ajouter(e.state.pool(), sub, jour(j), stat, cle, n)
            .await
            .unwrap();
    }

    async fn octobre(e: &Env, sub: &str) {
        for (j, stat, cle, n) in [
            ("2026-09-30", messagerie::MESSAGES, "srv/a", 7),
            ("2026-10-05", messagerie::MESSAGES, "srv/a", 120),
            ("2026-10-05", agregats::UTILISATION, "web|messagerie", 3600),
            ("2026-10-06", messagerie::MESSAGES, "srv/b", 30),
            ("2026-10-06", entracte::SECONDES, "", 5000),
            ("2026-10-06", entracte::SESSION_MAX, "", 4000),
            ("2026-10-07", sillon::ECOUTES, "", 3),
            ("2026-10-07", sillon::SECONDES, "", 600),
            ("2026-10-07", messagerie::REACTIONS_DONNEES, "👍", 4),
            ("2026-10-07", messagerie::APPEL_MAX, "", 900),
            ("2026-10-07", messagerie::VOCAL_SECONDES, "srv/v", 1200),
        ] {
            ajouter(e, sub, j, stat, cle, n).await;
        }
    }

    async fn passe(e: &Env, aujourd_hui: &str) -> u64 {
        creer_les_recaps(&e.state, jour(aujourd_hui)).await.unwrap()
    }

    async fn nombre(e: &Env) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM recaps")
            .fetch_one(e.state.pool())
            .await
            .unwrap()
    }

    #[test]
    fn periodes_closes_le_mois_et_l_annee_precedents() {
        let closes = |j: &str| {
            periodes_closes(jour(j))
                .map(|(g, du, au)| (g.etiquette(du), du.to_string(), au.to_string()))
        };
        let p = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        assert_eq!(
            closes("2026-11-01"),
            [
                p("2026-10", "2026-10-01", "2026-10-31"),
                p("2025", "2025-01-01", "2025-12-31")
            ]
        );
        assert_eq!(
            closes("2027-01-01"),
            [
                p("2026-12", "2026-12-01", "2026-12-31"),
                p("2026", "2026-01-01", "2026-12-31")
            ]
        );
        assert_eq!(
            closes("2028-03-31")[0],
            p("2028-02", "2028-02-01", "2028-02-29")
        );
    }

    #[tokio::test]
    async fn c6_recap_cree_inchange_par_les_evenements_en_retard_et_les_imports() {
        let Some(e) = env().await else { return };
        octobre(&e, DIANE).await;
        assert_eq!(passe(&e, NOVEMBRE).await, 1);
        let liste = e.get("diane", "/recaps").await;
        assert_eq!(liste.statut, StatusCode::OK);
        let liste = liste.json();
        let id = liste[0]["id"].as_i64().unwrap();
        assert_eq!(
            liste,
            json!([{
                "id": id,
                "kind": "month",
                "period": "2026-10",
                "created_at": liste[0]["created_at"],
                "seen": false,
            }])
        );
        let chemin = format!("/recaps/{id}");
        let r = e.get("diane", &chemin).await;
        assert_eq!(r.statut, StatusCode::OK);
        let fige = r.json();
        assert_eq!(
            (
                fige["id"].clone(),
                fige["kind"].clone(),
                fige["period"].clone(),
                fige["from"].clone(),
                fige["to"].clone()
            ),
            (
                json!(id),
                json!("month"),
                json!("2026-10"),
                json!("2026-10-01"),
                json!("2026-10-31")
            )
        );
        assert_eq!(
            (
                fige["usage"]["seconds"].clone(),
                fige["usage"]["first_day"].clone(),
                fige["entracte"]["seconds"].clone(),
                fige["messaging"]["total"].clone(),
                fige["messaging"]["voice_seconds"].clone()
            ),
            (
                json!(3600),
                json!("2026-09-30"),
                json!(5000),
                json!(150),
                json!(1200)
            )
        );
        assert!(fige.get("sillon").is_none());
        assert_eq!(
            fige["highlights"],
            json!([
                {"kind": "longest_day", "value": 3600, "day": "2026-10-05"},
                {"kind": "biggest_day", "value": 5000, "day": "2026-10-06"},
                {"kind": "longest_session", "value": 4000},
                {"kind": "busiest_day", "value": 120, "day": "2026-10-05"},
                {"kind": "top_channel", "value": 120, "server": "srv", "channel": "a"},
                {"kind": "top_emoji", "value": 4, "emoji": "👍"},
                {"kind": "longest_call", "value": 900},
                {"kind": "top_voice_channel", "value": 1200, "server": "srv", "channel": "v"},
            ])
        );
        assert_eq!(
            fige["trophies"],
            json!([{
                "name": "Sur la lancée",
                "description": "Des jours d'affilée, toutes activités confondues.",
                "icon": "etincelle",
                "tier": 1,
                "unlocked_at": "2026-10-07",
            }])
        );
        assert_eq!(
            fige["streak"],
            json!({"global": 3, "entracte": 1, "messaging": 3})
        );
        e.recevoir(
            "retard",
            DIANE,
            "2026-10-08T10:00:00Z",
            false,
            Donnees::EstompeMessage {
                serveur: Some("srv".into()),
                salon: Some("a".into()),
                prive: false,
            },
        )
        .await;
        e.recevoir(
            "import:entracte:lecture:1",
            DIANE,
            "2026-10-09T20:00:00Z",
            true,
            Donnees::EntracteLecture {
                titre: "f1".into(),
                genre: GenreDeTitre::Film,
                serie: None,
                secondes: 30000,
            },
        )
        .await;
        ajouter(
            &e,
            DIANE,
            "2026-10-10",
            agregats::UTILISATION,
            "web|entracte",
            30000,
        )
        .await;
        let id_trophee: i64 =
            sqlx::query_scalar("SELECT id FROM trophees WHERE cle = 'sur_la_lancee'")
                .fetch_one(e.state.pool())
                .await
                .unwrap();
        let r = e
            .appel(
                Some("admin"),
                "PATCH",
                &format!("/admin/trophies/{id_trophee}"),
                Some(json!({"name": "Renommé", "thresholds": [2, 4]})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::OK);
        let vivant = e
            .get("diane", "/entracte?period=month&date=2026-10-15")
            .await
            .json();
        assert_eq!(vivant["seconds"], 35000);
        assert_eq!(passe(&e, NOVEMBRE).await, 0);
        assert_eq!(passe(&e, "2026-11-02").await, 0);
        assert_eq!(nombre(&e).await, 1);
        assert_eq!(e.get("diane", &chemin).await.json(), fige);
        for (qui, autre) in [
            ("alice", chemin.as_str()),
            ("diane", "/recaps/0"),
            ("diane", "/recaps/x"),
        ] {
            let r = e.get(qui, autre).await;
            assert_eq!(r.statut, StatusCode::NOT_FOUND, "{qui} {autre}");
        }
        assert_eq!(e.get("alice", "/recaps").await.json(), json!([]));
        assert_eq!(e.get("diane", "/unseen").await.json()["recaps"], 1);
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/seen",
                Some(json!({"recaps": "all"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert_eq!(e.get("diane", "/unseen").await.json()["recaps"], 1);
        let r = e
            .appel(
                Some("diane"),
                "POST",
                "/seen",
                Some(json!({"recaps": [id]})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert_eq!(e.get("diane", "/unseen").await.json()["recaps"], 0);
        assert_eq!(e.get("diane", "/recaps").await.json()[0]["seen"], true);
        assert_eq!(e.get("diane", &chemin).await.json(), fige);
    }

    #[tokio::test]
    async fn c7_aucun_recap_sous_le_seuil_d_activite() {
        let Some(e) = env().await else { return };
        for i in 0..25 {
            let j = (jour("2026-10-01") + Days::new(i)).to_string();
            ajouter(&e, ALICE, &j, agregats::UTILISATION, "web|autre", 3600).await;
        }
        ajouter(&e, ALICE, "2026-10-03", messagerie::MESSAGES, "srv/a", 500).await;
        ajouter(&e, ALICE, "2026-10-04", sillon::ECOUTES, "", 50).await;
        ajouter(&e, ALICE, "2026-10-04", messagerie::MESSAGES, "prive", 50).await;
        ajouter(&e, ALICE, "2026-10-05", sillon::SECONDES, "", 9000).await;
        octobre(&e, "sub-bruno").await;
        assert_eq!(passe(&e, NOVEMBRE).await, 1);
        assert_eq!(e.get("alice", "/recaps").await.json(), json!([]));
        let r = e.get("bruno", "/recaps").await.json();
        assert_eq!(r.as_array().unwrap().len(), 1);
        let detail = e
            .get("bruno", &format!("/recaps/{}", r[0]["id"]))
            .await
            .json();
        assert!(detail.get("entracte").is_none());
        assert_eq!(detail["sillon"]["tracks_played"], 3);
        assert_eq!(
            detail["streak"],
            json!({"global": 3, "sillon": 1, "messaging": 3})
        );
        assert_eq!(passe(&e, NOVEMBRE).await, 0);
        ajouter(
            &e,
            ALICE,
            "2026-10-20",
            messagerie::VOCAL_SECONDES,
            "prive",
            60,
        )
        .await;
        assert_eq!(passe(&e, "2026-11-03").await, 1);
        let r = e.get("alice", "/recaps").await.json();
        assert_eq!(
            (r[0]["kind"].clone(), r[0]["period"].clone()),
            (json!("month"), json!("2026-10"))
        );
        assert_eq!(passe(&e, "2027-01-01").await, 0);
        for j in [
            "2026-02-01",
            "2026-03-01",
            "2026-04-01",
            "2026-05-01",
            "2026-06-01",
            "2026-07-01",
        ] {
            ajouter(&e, ALICE, j, messagerie::MESSAGES, "prive", 1).await;
        }
        assert_eq!(passe(&e, "2027-01-01").await, 0);
        ajouter(&e, ALICE, "2026-08-01", messagerie::MESSAGES, "prive", 1).await;
        assert_eq!(passe(&e, "2027-01-01").await, 1);
        let r = e.get("alice", "/recaps").await.json();
        assert_eq!(
            (
                r[0]["kind"].clone(),
                r[0]["period"].clone(),
                r.as_array().unwrap().len()
            ),
            (json!("year"), json!("2026"), 2)
        );
        let annee = e
            .get("alice", &format!("/recaps/{}", r[0]["id"]))
            .await
            .json();
        assert_eq!(
            (
                annee["from"].clone(),
                annee["to"].clone(),
                annee["messaging"]["total"].clone()
            ),
            (json!("2026-01-01"), json!("2026-12-31"), json!(557))
        );
    }

    #[tokio::test]
    async fn seuil_reglable_et_creation_reportee_sans_les_droits() {
        let Some(e) = env_avec(|c| c.recap_jours_min = 4).await else {
            return;
        };
        octobre(&e, DIANE).await;
        assert_eq!(passe(&e, NOVEMBRE).await, 0);
        let injoignable = |c: &mut crate::Config| {
            c.aegis = Some(("http://127.0.0.1:9".into(), "jeton".into()));
        };
        for regler in [injoignable, |c: &mut crate::Config| c.aegis = None] {
            let Some(e) = env_avec(regler).await else {
                return;
            };
            octobre(&e, DIANE).await;
            assert_eq!(passe(&e, NOVEMBRE).await, 0);
            assert_eq!(nombre(&e).await, 0);
        }
    }
}
