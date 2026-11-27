use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    response::Response,
};
use chrono::{DateTime, Utc};
use commun::evenements::{Donnees, Evenement, GenreBadge};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    bibliotheque::uuid,
    etat::AppState,
    garde::Personne,
    http::{bd, horodatage, introuvable, ok, sans_contenu},
    progression,
    sablier::Lectures,
};

pub const SEUIL_REGARDE: f64 = 0.9;
pub const MARGE_INTRO: f64 = 20.0;
pub const FIN_SAUTABLE: f64 = 180.0;
pub const PERIODE: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    pub sub: String,
    pub genre: GenreBadge,
    pub titre: Uuid,
    pub saison: Option<i32>,
    pub obtenu_le: DateTime<Utc>,
}

impl Badge {
    pub fn identifiant(&self) -> String {
        format!(
            "badge:entracte:{}:{}:{}:{}",
            self.sub,
            self.genre.nom(),
            self.titre,
            self.saison
                .map_or_else(|| "-".to_string(), |s| s.to_string())
        )
    }

    pub fn evenement(&self, importe: bool) -> Evenement {
        let mut e = Evenement::importe(
            self.identifiant(),
            &self.sub,
            self.obtenu_le,
            Donnees::EntracteBadge {
                genre: self.genre,
                titre: self.titre.to_string(),
                saison: self.saison.and_then(|s| u32::try_from(s).ok()),
            },
        );
        e.importe = importe;
        e
    }

    pub fn lu(l: &sqlx::postgres::PgRow) -> Option<Self> {
        Some(Badge {
            sub: l.get("sub"),
            genre: GenreBadge::lire(l.get::<String, _>("genre").as_str())?,
            titre: l.get("titre_id"),
            saison: l.get("saison"),
            obtenu_le: l.get("obtenu_le"),
        })
    }
}

pub fn credit_intro(position: Option<f64>, debut: f64, fin: f64) -> f64 {
    match position {
        None => fin - debut,
        Some(p) if p >= debut - MARGE_INTRO && p < fin => fin - p.max(debut),
        Some(_) => 0.0,
    }
}

pub fn credit_fin(position: f64, duree: f64) -> f64 {
    let reste = duree - position;
    if reste > 0.0 && reste <= FIN_SAUTABLE {
        reste
    } else {
        0.0
    }
}

const ATTRIBUTION: &str = "WITH attendus AS (
        SELECT serie_id, saison, max(n) AS n FROM (
            SELECT serie_id, saison, count(*) AS n FROM titres
            WHERE genre = 'episode' AND saison > 0 GROUP BY serie_id, saison
            UNION ALL
            SELECT serie_id, numero, episodes FROM saisons WHERE numero > 0 AND episodes > 0
        ) x GROUP BY serie_id, saison
    ),
    vues AS (
        SELECT r.sub, e.serie_id, e.saison, count(*) AS n, max(r.regarde_le) AS le
        FROM regardes r JOIN titres e ON e.id = r.titre_id
        WHERE e.genre = 'episode' AND e.saison > 0 AND ($1::text IS NULL OR r.sub = $1)
        GROUP BY r.sub, e.serie_id, e.saison
    ),
    finies AS (
        SELECT v.sub, v.serie_id, v.saison, v.le FROM vues v JOIN attendus a USING (serie_id, saison)
        WHERE v.n >= a.n
    )
    INSERT INTO badges (sub, genre, titre_id, saison, obtenu_le)
    SELECT r.sub, 'film', t.id, NULL::integer, r.regarde_le
    FROM regardes r JOIN titres t ON t.id = r.titre_id
    WHERE t.genre = 'film' AND ($1::text IS NULL OR r.sub = $1)
    UNION ALL
    SELECT sub, 'saison', serie_id, saison, le FROM finies
    UNION ALL
    SELECT f.sub, 'serie', f.serie_id, NULL::integer, max(f.le) FROM finies f
    GROUP BY f.sub, f.serie_id
    HAVING count(*) = (SELECT count(*) FROM attendus a WHERE a.serie_id = f.serie_id)
    ON CONFLICT DO NOTHING
    RETURNING sub, genre, titre_id, saison, obtenu_le";

pub async fn attribuer(pool: &PgPool, sub: Option<&str>) -> Result<Vec<Badge>, sqlx::Error> {
    let lignes = sqlx::query(ATTRIBUTION).bind(sub).fetch_all(pool).await?;
    let mut badges: Vec<Badge> = lignes.iter().filter_map(Badge::lu).collect();
    badges.sort_by_key(|b| (b.obtenu_le, b.genre.nom() == "serie"));
    Ok(badges)
}

pub async fn regarder(pool: &PgPool, sub: &str, titre: Uuid) -> Result<Vec<Badge>, sqlx::Error> {
    let nouveau = sqlx::query(
        "INSERT INTO regardes (sub, titre_id)
         SELECT sub, titre_id FROM progression
         WHERE sub = $1 AND titre_id = $2 AND secondes_vues >= duree * $3
         ON CONFLICT DO NOTHING RETURNING titre_id",
    )
    .bind(sub)
    .bind(titre)
    .bind(SEUIL_REGARDE)
    .fetch_optional(pool)
    .await?
    .is_some();
    if nouveau {
        attribuer(pool, Some(sub)).await
    } else {
        Ok(Vec::new())
    }
}

pub async fn sauter_intro(
    pool: &PgPool,
    sub: &str,
    titre: Uuid,
) -> Result<Option<Vec<Badge>>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let Some(intro) = sqlx::query(
        "SELECT i.debut, i.fin, t.duree FROM intros i JOIN titres t ON t.id = i.titre_id WHERE i.titre_id = $1",
    )
    .bind(titre)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    let (debut, fin, duree): (f64, f64, Option<f64>) = (intro.get(0), intro.get(1), intro.get(2));
    let ligne: Option<(f64, bool)> = sqlx::query_as(
        "SELECT position, intro_creditee FROM progression WHERE sub = $1 AND titre_id = $2 FOR UPDATE",
    )
    .bind(sub)
    .bind(titre)
    .fetch_optional(&mut *tx)
    .await?;
    if ligne.is_some_and(|l| l.1) {
        return Ok(Some(Vec::new()));
    }
    let position = ligne.map(|l| l.0);
    let credit = credit_intro(position, debut, fin);
    if credit <= 0.0 || (position.is_none() && duree.is_none()) {
        return Ok(Some(Vec::new()));
    }
    sqlx::query(
        "INSERT INTO progression (sub, titre_id, position, duree, secondes_vues, intro_creditee)
         VALUES ($1, $2, $3, $4, $5, true)
         ON CONFLICT (sub, titre_id) DO UPDATE SET position = EXCLUDED.position, intro_creditee = true,
           secondes_vues = LEAST(progression.duree, progression.secondes_vues + EXCLUDED.secondes_vues), maj = now()",
    )
    .bind(sub)
    .bind(titre)
    .bind(fin)
    .bind(duree.unwrap_or(fin))
    .bind(credit)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    regarder(pool, sub, titre).await.map(Some)
}

pub async fn sauter_fin(
    pool: &PgPool,
    lectures: &Lectures,
    sub: &str,
    titre: Uuid,
    maintenant: Instant,
) -> Result<Option<Vec<Evenement>>, sqlx::Error> {
    let Some((position, duree, creditee)): Option<(f64, f64, bool)> = sqlx::query_as(
        "SELECT position, duree, fin_creditee FROM progression WHERE sub = $1 AND titre_id = $2",
    )
    .bind(sub)
    .bind(titre)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let saute = credit_fin(position, duree);
    if creditee || saute <= 0.0 {
        return Ok(Some(Vec::new()));
    }
    let reserve = sqlx::query(
        "UPDATE progression SET fin_creditee = true WHERE sub = $1 AND titre_id = $2 AND NOT fin_creditee",
    )
    .bind(sub)
    .bind(titre)
    .execute(pool)
    .await?
    .rows_affected();
    if reserve == 0 {
        return Ok(Some(Vec::new()));
    }
    progression::rapporter(pool, lectures, sub, titre, duree, duree, maintenant, saute).await
}

pub async fn intro_sautee(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let badges = sauter_intro(state.pool(), p.sub(), id)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    state.emetteur().emettre(Evenement::nouveau(
        p.sub(),
        Donnees::EntracteIntroPassee {
            titre: id.to_string(),
        },
    ));
    badges
        .iter()
        .for_each(|b| state.emetteur().emettre(b.evenement(false)));
    Ok(sans_contenu())
}

pub async fn fin_sautee(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    sauter_fin(state.pool(), state.lectures(), p.sub(), id, Instant::now())
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?
        .into_iter()
        .for_each(|e| state.emetteur().emettre(e));
    Ok(sans_contenu())
}

pub async fn liste(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let lignes = sqlx::query(
        "SELECT b.genre, b.titre_id, b.saison, b.obtenu_le, t.titre, t.annee, t.duree,
           (t.affiche IS NOT NULL) AS a_affiche, sa.titre AS saison_titre, (sa.affiche IS NOT NULL) AS saison_affiche,
           e.episodes, e.secondes
         FROM badges b JOIN titres t ON t.id = b.titre_id
         LEFT JOIN saisons sa ON sa.serie_id = b.titre_id AND sa.numero = b.saison
         LEFT JOIN LATERAL (SELECT count(*) AS episodes, sum(x.duree) AS secondes FROM titres x
           WHERE b.genre <> 'film' AND x.serie_id = b.titre_id AND x.genre = 'episode'
             AND x.saison > 0 AND (b.saison IS NULL OR x.saison = b.saison)) e ON true
         WHERE b.sub = $1 ORDER BY b.obtenu_le DESC, b.genre, b.saison DESC",
    )
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let items: Vec<_> = lignes
        .iter()
        .map(|l| {
            let genre: String = l.get("genre");
            let film = genre == "film";
            let obtenu: DateTime<Utc> = l.get("obtenu_le");
            json!({
                "kind": match genre.as_str() { "film" => "movie", "saison" => "season", _ => "series" },
                "title_id": l.get::<Uuid, _>("titre_id"),
                "season": l.get::<Option<i32>, _>("saison"),
                "title": l.get::<String, _>("titre"),
                "season_title": l.get::<Option<String>, _>("saison_titre"),
                "year": l.get::<Option<i32>, _>("annee"),
                "poster": l.get::<bool, _>("a_affiche"),
                "season_poster": l.get::<Option<bool>, _>("saison_affiche").unwrap_or(false),
                "episodes": if film { None } else { l.get::<Option<i64>, _>("episodes") },
                "duration": if film { l.get::<Option<f64>, _>("duree") } else { l.get::<Option<f64>, _>("secondes") },
                "obtained_at": horodatage(&obtenu),
            })
        })
        .collect();
    Ok(ok(&json!({ "items": items })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        progression::rapporter,
        testutil::{BasePostgres, postgres},
    };

    async fn titre(
        pool: &PgPool,
        genre: &str,
        serie: Option<(Uuid, i32, i32)>,
        duree: f64,
        tmdb: i64,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO titres (genre, tmdb_id, serie_id, saison, episode, titre, duree, present)
             VALUES ($1, $2, $3, $4, $5, 'x', $6, true) RETURNING id",
        )
        .bind(genre)
        .bind(tmdb)
        .bind(serie.map(|s| s.0))
        .bind(serie.map(|s| s.1))
        .bind(serie.map(|s| s.2))
        .bind(duree)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    struct Lecteur<'a> {
        pool: &'a PgPool,
        lectures: Lectures,
        debut: Instant,
        horloge: u64,
        evenements: Vec<Evenement>,
    }

    impl Lecteur<'_> {
        fn maintenant(&self) -> Instant {
            self.debut + Duration::from_secs(self.horloge)
        }

        async fn regarder(&mut self, sub: &str, id: Uuid, de: f64, a: f64, duree: f64) {
            let mut position = de;
            loop {
                let e = rapporter(
                    self.pool,
                    &self.lectures,
                    sub,
                    id,
                    position,
                    duree,
                    self.maintenant(),
                    0.0,
                )
                .await
                .unwrap()
                .unwrap();
                self.evenements.extend(e);
                if position >= a {
                    break;
                }
                self.horloge += 15;
                position = (position + 15.0).min(a);
            }
        }

        fn badges(&self) -> Vec<Donnees> {
            self.evenements
                .iter()
                .filter(|e| e.donnees.nom() == "entracte.badge")
                .map(|e| e.donnees.clone())
                .collect()
        }
    }

    async fn badges_de(pool: &PgPool, sub: &str) -> Vec<(String, Uuid, Option<i32>)> {
        sqlx::query_as(
            "SELECT genre, titre_id, saison FROM badges WHERE sub = $1 ORDER BY genre, saison",
        )
        .bind(sub)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn base() -> Option<(BasePostgres, PgPool)> {
        let (base, url) = postgres().await?;
        let pool = PgPool::connect(&url).await.unwrap();
        crate::MIGRATIONS.run(&pool).await.unwrap();
        Some((base, pool))
    }

    #[tokio::test]
    async fn film_regarde_en_entier_et_saut_a_la_fin() {
        let Some((_base, pool)) = base().await else {
            return;
        };
        let film = titre(&pool, "film", None, 100.0, 1).await;
        let mut l = Lecteur {
            pool: &pool,
            lectures: Lectures::default(),
            debut: Instant::now(),
            horloge: 0,
            evenements: Vec::new(),
        };
        l.regarder("bob", film, 0.0, 15.0, 100.0).await;
        l.horloge += 15;
        l.regarder("bob", film, 95.0, 95.0, 100.0).await;
        assert!(
            l.badges().is_empty(),
            "avancer jusqu'à la fin ne compte pas"
        );
        assert!(
            l.evenements
                .iter()
                .any(|e| e.donnees.nom() == "entracte.vu")
        );
        l.regarder("alice", film, 0.0, 75.0, 100.0).await;
        assert!(l.badges().is_empty());
        l.regarder("alice", film, 75.0, 90.0, 100.0).await;
        assert_eq!(
            l.badges(),
            [Donnees::EntracteBadge {
                genre: GenreBadge::Film,
                titre: film.to_string(),
                saison: None
            }]
        );
        l.regarder("alice", film, 0.0, 100.0, 100.0).await;
        assert_eq!(l.badges().len(), 1, "un seul badge par titre");
        assert_eq!(
            badges_de(&pool, "alice").await,
            [("film".to_string(), film, None)]
        );
        assert!(badges_de(&pool, "bob").await.is_empty());
        let e = l
            .evenements
            .iter()
            .find(|e| e.donnees.nom() == "entracte.badge")
            .unwrap();
        assert_eq!(e.id, format!("badge:entracte:alice:film:{film}:-"));
        assert!(!e.importe && e.verifier().is_ok());
    }

    #[tokio::test]
    async fn saison_et_serie_avec_intro_et_fin_passees() {
        let Some((_base, pool)) = base().await else {
            return;
        };
        let serie = titre(&pool, "serie", None, 0.0, 10).await;
        let s1 = [
            titre(&pool, "episode", Some((serie, 1, 1)), 1200.0, 11).await,
            titre(&pool, "episode", Some((serie, 1, 2)), 1200.0, 12).await,
        ];
        let special = titre(&pool, "episode", Some((serie, 0, 1)), 600.0, 13).await;
        for e in s1 {
            sqlx::query(
                "INSERT INTO fichiers (titre_id, chemin, taille, infos) VALUES ($1, $2, 1, '{}')",
            )
            .bind(e)
            .bind(format!("/srv/{e}.mkv"))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO intros (titre_id, fichier_id, debut, fin) SELECT titre_id, id, 30, 120 FROM fichiers WHERE titre_id = $1")
                .bind(e)
                .execute(&pool)
                .await
                .unwrap();
        }
        let mut l = Lecteur {
            pool: &pool,
            lectures: Lectures::default(),
            debut: Instant::now(),
            horloge: 0,
            evenements: Vec::new(),
        };
        for (i, e) in s1.into_iter().enumerate() {
            l.regarder("alice", e, 0.0, 30.0, 1200.0).await;
            assert!(
                sauter_intro(&pool, "alice", e)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                sauter_intro(&pool, "alice", e)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_empty()
            );
            l.horloge += 15;
            l.regarder("alice", e, 125.0, 1000.0, 1200.0).await;
            assert!(
                sauter_fin(&pool, &l.lectures, "alice", e, l.maintenant())
                    .await
                    .unwrap()
                    .unwrap()
                    .is_empty(),
                "trop tôt"
            );
            l.horloge += 15;
            l.regarder("alice", e, 1050.0, 1050.0, 1200.0).await;
            l.horloge += 5;
            let fin = sauter_fin(&pool, &l.lectures, "alice", e, l.maintenant())
                .await
                .unwrap()
                .unwrap();
            let noms: Vec<_> = fin.iter().map(|e| e.donnees.nom()).collect();
            if i == 0 {
                assert_eq!(noms, ["entracte.lecture", "entracte.vu"]);
                l.horloge += 15;
                l.regarder("alice", e, 1100.0, 1100.0, 1200.0).await;
                assert!(
                    sauter_fin(&pool, &l.lectures, "alice", e, l.maintenant())
                        .await
                        .unwrap()
                        .unwrap()
                        .is_empty(),
                    "la fin ne se crédite qu'une fois"
                );
            } else {
                assert_eq!(
                    noms,
                    [
                        "entracte.lecture",
                        "entracte.vu",
                        "entracte.badge",
                        "entracte.badge"
                    ]
                );
                assert_eq!(
                    fin[2..]
                        .iter()
                        .map(|e| e.donnees.clone())
                        .collect::<Vec<_>>(),
                    [
                        Donnees::EntracteBadge {
                            genre: GenreBadge::Saison,
                            titre: serie.to_string(),
                            saison: Some(1)
                        },
                        Donnees::EntracteBadge {
                            genre: GenreBadge::Serie,
                            titre: serie.to_string(),
                            saison: None
                        },
                    ]
                );
            }
        }
        assert_eq!(
            badges_de(&pool, "alice").await,
            [
                ("saison".to_string(), serie, Some(1)),
                ("serie".to_string(), serie, None)
            ]
        );
        let regardes: i64 =
            sqlx::query_scalar("SELECT count(*) FROM regardes WHERE titre_id <> $1")
                .bind(special)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(regardes, 2);
        assert!(
            sauter_intro(&pool, "alice", special)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            sauter_fin(&pool, &l.lectures, "alice", special, l.maintenant())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn historique_vu_repris_a_la_migration() {
        let Some((_base, url)) = postgres().await else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        crate::MIGRATIONS.run_to(3, &pool).await.unwrap();
        let film = titre(&pool, "film", None, 100.0, 1).await;
        let serie = titre(&pool, "serie", None, 0.0, 2).await;
        let e1 = titre(&pool, "episode", Some((serie, 1, 1)), 40.0, 3).await;
        let e2 = titre(&pool, "episode", Some((serie, 2, 1)), 40.0, 4).await;
        for (t, jour) in [(film, 1), (e1, 2)] {
            sqlx::query("INSERT INTO vus (sub, titre_id, vu_le) VALUES ('alice', $1, $2)")
                .bind(t)
                .bind(DateTime::<Utc>::from_timestamp(1_700_000_000 + jour * 86_400, 0).unwrap())
                .execute(&pool)
                .await
                .unwrap();
        }
        crate::MIGRATIONS.run(&pool).await.unwrap();
        let nouveaux = attribuer(&pool, None).await.unwrap();
        assert_eq!(
            nouveaux
                .iter()
                .map(|b| (b.genre, b.saison))
                .collect::<Vec<_>>(),
            [(GenreBadge::Film, None), (GenreBadge::Saison, Some(1))]
        );
        assert_eq!(nouveaux[0].obtenu_le.timestamp(), 1_700_086_400);
        assert!(attribuer(&pool, None).await.unwrap().is_empty());
        sqlx::query("DELETE FROM titres WHERE id = $1")
            .bind(e2)
            .execute(&pool)
            .await
            .unwrap();
        let serie_complete = attribuer(&pool, Some("alice")).await.unwrap();
        assert_eq!(
            serie_complete.iter().map(|b| b.genre).collect::<Vec<_>>(),
            [GenreBadge::Serie]
        );
    }

    #[test]
    fn credit_de_l_intro_passee() {
        assert_eq!(credit_intro(None, 30.0, 90.0), 60.0);
        assert_eq!(credit_intro(Some(15.0), 30.0, 90.0), 60.0);
        assert_eq!(credit_intro(Some(45.0), 30.0, 90.0), 45.0);
        assert_eq!(
            credit_intro(Some(5.0), 30.0, 90.0),
            0.0,
            "bien avant l'intro"
        );
        assert_eq!(credit_intro(Some(90.0), 30.0, 90.0), 0.0, "déjà passée");
        assert_eq!(credit_intro(Some(600.0), 30.0, 90.0), 0.0);
    }

    #[test]
    fn credit_de_la_fin_passee() {
        assert_eq!(credit_fin(1100.0, 1200.0), 100.0);
        assert_eq!(credit_fin(1020.0, 1200.0), 180.0);
        assert_eq!(credit_fin(1019.0, 1200.0), 0.0, "trop tôt");
        assert_eq!(credit_fin(1200.0, 1200.0), 0.0);
    }

    #[test]
    fn identifiant_stable_et_evenement() {
        let titre = Uuid::nil();
        let date = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let b = Badge {
            sub: "alice".into(),
            genre: GenreBadge::Saison,
            titre,
            saison: Some(2),
            obtenu_le: date,
        };
        assert_eq!(
            b.identifiant(),
            format!("badge:entracte:alice:saison:{titre}:2")
        );
        let e = b.evenement(true);
        assert!(e.importe && e.date == date && e.verifier().is_ok());
        assert_eq!(
            e.donnees,
            Donnees::EntracteBadge {
                genre: GenreBadge::Saison,
                titre: titre.to_string(),
                saison: Some(2)
            }
        );
        let f = Badge {
            genre: GenreBadge::Film,
            saison: None,
            ..b
        };
        assert_eq!(
            f.identifiant(),
            format!("badge:entracte:alice:film:{titre}:-")
        );
        assert_eq!(f.evenement(false).id, f.identifiant());
    }
}
