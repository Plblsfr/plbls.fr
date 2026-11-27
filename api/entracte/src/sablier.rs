use std::{
    collections::HashMap,
    fmt,
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use commun::{
    emission::{Emetteur, VARIABLE_URL},
    evenements::{Donnees, Evenement, Genre},
};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::progression::DUREE_MAX;

pub const COMMANDE: &str = "importer-sablier";
pub const TRANCHE_MAX: Duration = Duration::from_secs(45);
pub const ABANDON_JOURS: i32 = 30;
pub const PERIODE_ABANDONS: Duration = Duration::from_secs(3600);

struct Repere {
    instant: Instant,
    position: f64,
    reste: f64,
}

#[derive(Default)]
pub struct Lectures(Mutex<HashMap<(String, Uuid), Repere>>);

pub fn tranche(ecoule: Duration, avancee: f64) -> f64 {
    if avancee <= 0.0 || ecoule > TRANCHE_MAX {
        return 0.0;
    }
    ecoule.as_secs_f64().min(avancee)
}

impl Lectures {
    pub fn crediter(&self, sub: &str, titre: Uuid, position: f64, maintenant: Instant) -> u32 {
        let mut reperes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let cle = (sub.to_string(), titre);
        let total = reperes.get(&cle).map_or(0.0, |r| {
            r.reste
                + tranche(
                    maintenant.saturating_duration_since(r.instant),
                    position - r.position,
                )
        });
        let secondes = total.floor();
        reperes.insert(
            cle,
            Repere {
                instant: maintenant,
                position,
                reste: total - secondes,
            },
        );
        secondes as u32
    }

    pub fn purger(&self, maintenant: Instant) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, r| maintenant.saturating_duration_since(r.instant) <= TRANCHE_MAX);
    }
}

pub struct Titre {
    pub id: Uuid,
    pub genre: Genre,
    pub serie: Option<Uuid>,
}

impl Titre {
    fn lu(id: Uuid, genre: &str, serie: Option<Uuid>) -> Option<Self> {
        let genre = match genre {
            "film" => Genre::Film,
            "episode" => Genre::Episode,
            _ => return None,
        };
        Some(Titre { id, genre, serie })
    }

    pub fn lecture(&self, secondes: u32) -> Donnees {
        Donnees::EntracteLecture {
            titre: self.id.to_string(),
            genre: self.genre,
            serie: self.serie.map(|s| s.to_string()),
            secondes,
        }
    }

    pub fn vu(&self) -> Donnees {
        Donnees::EntracteVu {
            titre: self.id.to_string(),
            genre: self.genre,
            serie: self.serie.map(|s| s.to_string()),
        }
    }
}

pub async fn titre(pool: &PgPool, id: Uuid) -> Result<Option<Titre>, sqlx::Error> {
    let l = sqlx::query("SELECT genre, serie_id FROM titres WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(l.and_then(|l| Titre::lu(id, l.get(0), l.get(1))))
}

pub async fn serie_terminee(pool: &PgPool, sub: &str, serie: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM titres e WHERE e.serie_id = $2 AND e.genre = 'episode'
           AND NOT EXISTS (SELECT 1 FROM vus v WHERE v.sub = $1 AND v.titre_id = e.id))",
    )
    .bind(sub)
    .bind(serie)
    .fetch_one(pool)
    .await
}

pub async fn abandons(pool: &PgPool, jours: i32) -> Result<Vec<Evenement>, sqlx::Error> {
    let lignes = sqlx::query(
        "UPDATE progression p SET abandon_le = now() FROM titres t
         WHERE t.id = p.titre_id AND p.abandon_le IS NULL AND NOT p.termine AND p.position > 0
           AND p.maj < now() - make_interval(days => $1)
           AND NOT EXISTS (SELECT 1 FROM vus v WHERE v.sub = p.sub AND v.titre_id = p.titre_id)
         RETURNING p.sub, p.titre_id, t.genre, t.serie_id, p.maj + make_interval(days => $1)",
    )
    .bind(jours)
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .iter()
        .filter_map(|l| {
            let titre = Titre::lu(l.get(1), l.get(2), l.get(3))?;
            let mut e = Evenement::nouveau(
                l.get::<String, _>(0),
                Donnees::EntracteAbandon {
                    titre: titre.id.to_string(),
                    genre: titre.genre,
                },
            );
            e.date = l.get(4);
            Some(e)
        })
        .collect())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rapport {
    pub lectures: usize,
    pub vus: usize,
    pub series: usize,
    pub badges: usize,
}

impl Rapport {
    pub fn de(evenements: &[Evenement]) -> Self {
        let compte = |nom: &str| evenements.iter().filter(|e| e.donnees.nom() == nom).count();
        Rapport {
            lectures: compte("entracte.lecture"),
            vus: compte("entracte.vu"),
            series: compte("entracte.serie_terminee"),
            badges: compte("entracte.badge"),
        }
    }
}

impl fmt::Display for Rapport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Import vers Sablier : {} lectures, {} titres vus, {} séries terminées, {} badges.",
            self.lectures, self.vus, self.series, self.badges
        )
    }
}

pub async fn historique(pool: &PgPool) -> Result<Vec<Evenement>, sqlx::Error> {
    let mut evenements = Vec::new();
    let lignes = sqlx::query(
        "SELECT sub, titre_id, t.genre, t.serie_id, v.vu_le, p.position, p.maj, coalesce(p.duree, t.duree)
         FROM progression p FULL JOIN vus v USING (sub, titre_id) JOIN titres t ON t.id = titre_id
         ORDER BY sub, titre_id",
    )
    .fetch_all(pool)
    .await?;
    for l in &lignes {
        let sub: String = l.get(0);
        let Some(titre) = Titre::lu(l.get(1), l.get(2), l.get(3)) else {
            continue;
        };
        let vu_le: Option<DateTime<Utc>> = l.get(4);
        let regarde = match vu_le {
            Some(_) => l.get::<Option<f64>, _>(7),
            None => l.get::<Option<f64>, _>(5),
        };
        let secondes = regarde
            .filter(|s| s.is_finite())
            .map_or(0, |s| s.clamp(0.0, DUREE_MAX).round() as u32);
        if let Some(date) = vu_le.or(l.get(6))
            && secondes > 0
        {
            evenements.push(Evenement::importe(
                format!("import:entracte:lecture:{sub}:{}", titre.id),
                &sub,
                date,
                titre.lecture(secondes),
            ));
        }
        if let Some(date) = vu_le {
            evenements.push(Evenement::importe(
                format!("import:entracte:vu:{sub}:{}", titre.id),
                &sub,
                date,
                titre.vu(),
            ));
        }
    }
    let series = sqlx::query(
        "SELECT v.sub, e.serie_id, max(v.vu_le) FROM vus v JOIN titres e ON e.id = v.titre_id
         WHERE e.genre = 'episode' GROUP BY v.sub, e.serie_id
         HAVING count(*) = (SELECT count(*) FROM titres x WHERE x.serie_id = e.serie_id AND x.genre = 'episode')
         ORDER BY v.sub, e.serie_id",
    )
    .fetch_all(pool)
    .await?;
    for l in &series {
        let sub: String = l.get(0);
        let serie: Uuid = l.get(1);
        evenements.push(Evenement::importe(
            format!("import:entracte:serie_terminee:{sub}:{serie}"),
            &sub,
            l.get(2),
            Donnees::EntracteSerieTerminee {
                serie: serie.to_string(),
            },
        ));
    }
    crate::badges::attribuer(pool, None).await?;
    let badges = sqlx::query(
        "SELECT sub, genre, titre_id, saison, obtenu_le FROM badges ORDER BY sub, obtenu_le, genre, titre_id, saison",
    )
    .fetch_all(pool)
    .await?;
    evenements.extend(
        badges
            .iter()
            .filter_map(crate::badges::Badge::lu)
            .map(|b| b.evenement(true)),
    );
    Ok(evenements)
}

pub async fn importer(pool: &PgPool, emetteur: &Emetteur) -> Result<Rapport, String> {
    if !emetteur.actif() {
        return Err(format!("{VARIABLE_URL} manquant ou invalide"));
    }
    let evenements = historique(pool).await.map_err(|e| format!("base : {e}"))?;
    for e in &evenements {
        e.verifier()
            .map_err(|r| format!("évènement {} refusé : {r}", e.id))?;
        emetteur
            .publier(e)
            .await
            .map_err(|r| format!("file d'évènements : {r}"))?;
    }
    Ok(Rapport::de(&evenements))
}

pub async fn lancer(brut: impl Fn(&str) -> Option<String>) -> Result<Rapport, String> {
    let url = commun::env::url_base(&brut, "entracte")?;
    let emetteur = Emetteur::depuis_env(&brut);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .map_err(|e| format!("base d'Entracte : {e}"))?;
    importer(&pool, &emetteur).await
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::{
        progression::rapporter,
        testutil::{BasePostgres, postgres},
    };

    const RADARR_ID: i64 = 987_654;
    const SONARR_ID: i64 = 876_543;
    const TMDB_ID: i64 = 765_432;
    const CHEMIN: &str = "/srv/medias/tv/La Serie/S01E01.mkv";

    fn secondes(evenements: &[Evenement]) -> u32 {
        evenements
            .iter()
            .map(|e| match e.donnees {
                Donnees::EntracteLecture { secondes, .. } => secondes,
                _ => 0,
            })
            .sum()
    }

    fn noms(evenements: &[Evenement]) -> Vec<&'static str> {
        evenements.iter().map(|e| e.donnees.nom()).collect()
    }

    #[test]
    fn dix_minutes_avec_pause_et_saut_en_avant_donnent_huit_minutes() {
        let lectures = Lectures::default();
        let titre = Uuid::new_v4();
        let debut = Instant::now();
        let mut position = 0.25;
        let mut total = 0;
        for horloge in (0..=600).step_by(15) {
            let en_pause = horloge > 180 && horloge <= 300;
            if horloge > 0 && !en_pause {
                position += 15.0;
            }
            if horloge == 405 {
                position += 900.0;
            }
            if en_pause && horloge != 300 {
                continue;
            }
            total += lectures.crediter(
                "alice",
                titre,
                position,
                debut + Duration::from_secs(horloge),
            );
        }
        assert!((465..=480).contains(&total), "{total} s regardées");
        assert_eq!(
            lectures.crediter("bob", titre, 500.0, debut + Duration::from_secs(600)),
            0
        );
    }

    #[test]
    fn tranches_creditees() {
        let s = Duration::from_secs;
        assert_eq!(tranche(s(15), 15.0), 15.0);
        assert_eq!(tranche(s(15), 14.5), 14.5);
        assert_eq!(tranche(s(15), 30.0), 15.0);
        assert_eq!(tranche(s(15), 600.0), 15.0);
        assert_eq!(tranche(s(120), 0.0), 0.0);
        assert_eq!(tranche(s(15), -300.0), 0.0);
        assert_eq!(tranche(s(45), 45.0), 45.0);
        assert_eq!(tranche(s(46), 46.0), 0.0);
        assert_eq!(tranche(s(3600), 600.0), 0.0);

        let lectures = Lectures::default();
        let (titre, debut) = (Uuid::new_v4(), Instant::now());
        let credit = |horloge: u64, position: f64| {
            lectures.crediter("alice", titre, position, debut + s(horloge))
        };
        assert_eq!(credit(0, 1000.0), 0);
        assert_eq!(credit(15, 1015.0), 15);
        assert_eq!(credit(30, 400.0), 0, "retour en arrière");
        assert_eq!(credit(45, 415.0), 15, "passage revu");
        assert_eq!(credit(60, 422.6), 7);
        assert_eq!(credit(75, 430.2), 8, "le reste d'une tranche est gardé");
        assert_eq!(credit(74, 440.0), 0, "horloge qui recule");
        lectures.purger(debut + s(100));
        assert_eq!(lectures.0.lock().unwrap().len(), 1);
        lectures.purger(debut + s(200));
        assert!(lectures.0.lock().unwrap().is_empty());
        assert_eq!(credit(210, 450.0), 0);
    }

    struct Base {
        _base: BasePostgres,
        pool: PgPool,
        film: Uuid,
        serie: Uuid,
        episodes: [Uuid; 2],
    }

    async fn inserer(pool: &PgPool, genre: &str, serie: Option<(Uuid, i32)>, duree: f64) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO titres (genre, tmdb_id, serie_id, saison, episode, titre, duree, present, radarr_id, sonarr_id)
             VALUES ($1, $2, $3, $4, $5, 'x', $6, true, $7, $8) RETURNING id",
        )
        .bind(genre)
        .bind(TMDB_ID + i64::from(serie.map_or(0, |s| s.1)))
        .bind(serie.map(|s| s.0))
        .bind(serie.map(|_| 1))
        .bind(serie.map(|s| s.1))
        .bind(duree)
        .bind(RADARR_ID)
        .bind(SONARR_ID)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn base() -> Option<Base> {
        let (base, url) = postgres().await?;
        let pool = PgPool::connect(&url).await.unwrap();
        crate::MIGRATIONS.run(&pool).await.unwrap();
        let film = inserer(&pool, "film", None, 100.0).await;
        let serie = inserer(&pool, "serie", None, 0.0).await;
        let episodes = [
            inserer(&pool, "episode", Some((serie, 1)), 40.0).await,
            inserer(&pool, "episode", Some((serie, 2)), 40.0).await,
        ];
        sqlx::query(
            "INSERT INTO fichiers (titre_id, chemin, taille, infos) VALUES ($1, $2, 1, '{}')",
        )
        .bind(episodes[0])
        .bind(CHEMIN)
        .execute(&pool)
        .await
        .unwrap();
        Some(Base {
            _base: base,
            pool,
            film,
            serie,
            episodes,
        })
    }

    fn sans_fuite(b: &Base, evenements: &[Evenement]) {
        let publics = [b.film, b.serie, b.episodes[0], b.episodes[1]].map(|u| u.to_string());
        for e in evenements {
            let json = e.json();
            for interdit in [
                "/srv",
                "medias",
                ".mkv",
                &RADARR_ID.to_string(),
                &SONARR_ID.to_string(),
                &TMDB_ID.to_string(),
            ] {
                assert!(!json.contains(interdit), "« {interdit} » dans {json}");
            }
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            for (cle, valeur) in v["donnees"].as_object().unwrap() {
                match cle.as_str() {
                    "titre" | "serie" => assert!(
                        valeur.is_null() || publics.iter().any(|p| valeur == p.as_str()),
                        "{json}"
                    ),
                    "genre" | "secondes" => {}
                    autre => panic!("champ inattendu {autre} dans {json}"),
                }
            }
            assert!(e.verifier().is_ok(), "{json}");
        }
    }

    #[tokio::test]
    async fn vu_une_seule_fois_serie_terminee_et_identifiants_publics() {
        let Some(b) = base().await else { return };
        let lectures = Lectures::default();
        let debut = Instant::now();
        let mut horloge = 0;
        let mut tous = Vec::new();
        let mut rapport = async |sub: &'static str, titre: Uuid, position: f64, duree: f64| {
            horloge += 15;
            let e = rapporter(
                &b.pool,
                &lectures,
                sub,
                titre,
                position,
                duree,
                debut + Duration::from_secs(horloge),
                0.0,
            )
            .await
            .unwrap()
            .unwrap();
            tous.extend(e.clone());
            e
        };
        assert!(rapport("alice", b.film, 60.0, 100.0).await.is_empty());
        let e = rapport("alice", b.film, 75.0, 100.0).await;
        assert_eq!(noms(&e), ["entracte.lecture"]);
        assert_eq!(
            e[0].donnees,
            Donnees::EntracteLecture {
                titre: b.film.to_string(),
                genre: Genre::Film,
                serie: None,
                secondes: 15
            }
        );
        assert!(!e[0].importe && e[0].sub == "alice");
        let e = rapport("alice", b.film, 90.0, 100.0).await;
        assert_eq!(noms(&e), ["entracte.lecture", "entracte.vu"]);
        assert_eq!(
            e[1].donnees,
            Donnees::EntracteVu {
                titre: b.film.to_string(),
                genre: Genre::Film,
                serie: None
            }
        );
        assert_eq!(
            noms(&rapport("alice", b.film, 99.0, 100.0).await),
            ["entracte.lecture"],
            "la lecture continue"
        );
        assert!(rapport("alice", b.film, 5.0, 100.0).await.is_empty());
        assert_eq!(
            noms(&rapport("alice", b.film, 20.0, 100.0).await),
            ["entracte.lecture"]
        );
        assert_eq!(
            noms(&rapport("alice", b.film, 95.0, 100.0).await),
            ["entracte.lecture"],
            "la lecture reprend"
        );
        assert_eq!(
            noms(&rapport("bob", b.film, 95.0, 100.0).await),
            ["entracte.vu"],
            "une autre personne"
        );

        assert_eq!(
            noms(&rapport("alice", b.episodes[0], 39.0, 40.0).await),
            ["entracte.vu"]
        );
        let e = rapport("alice", b.episodes[1], 39.0, 40.0).await;
        assert_eq!(noms(&e), ["entracte.vu", "entracte.serie_terminee"]);
        assert_eq!(
            e[0].donnees,
            Donnees::EntracteVu {
                titre: b.episodes[1].to_string(),
                genre: Genre::Episode,
                serie: Some(b.serie.to_string())
            }
        );
        assert_eq!(
            e[1].donnees,
            Donnees::EntracteSerieTerminee {
                serie: b.serie.to_string()
            }
        );
        assert!(rapport("alice", b.episodes[1], 39.5, 40.0).await.is_empty());
        assert!(
            rapporter(
                &b.pool,
                &lectures,
                "alice",
                b.serie,
                1.0,
                10.0,
                Instant::now(),
                0.0
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            noms(&tous)
                .iter()
                .filter(|n| **n == "entracte.vu" || **n == "entracte.serie_terminee")
                .count(),
            5
        );
        assert_eq!(secondes(&tous), 69);
        sans_fuite(&b, &tous);
    }

    async fn ancienne(pool: &PgPool, sub: &str, titre: Uuid, position: f64, jours: i32) {
        sqlx::query(
            "INSERT INTO progression (sub, titre_id, position, duree, termine, maj)
             VALUES ($1, $2, $3, 100, $3 >= 90, $4)",
        )
        .bind(sub)
        .bind(titre)
        .bind(position)
        .bind(date(jours))
        .execute(pool)
        .await
        .unwrap();
    }

    fn date(jours: i32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap() + chrono::Duration::days(jours.into())
    }

    #[tokio::test]
    async fn abandon_releve_une_seule_fois() {
        let Some(b) = base().await else { return };
        ancienne(&b.pool, "alice", b.film, 30.0, 0).await;
        ancienne(&b.pool, "alice", b.episodes[0], 95.0, 0).await;
        ancienne(&b.pool, "bob", b.film, 0.0, 0).await;
        ancienne(&b.pool, "carole", b.film, 30.0, 0).await;
        sqlx::query("INSERT INTO vus (sub, titre_id) VALUES ('carole', $1)")
            .bind(b.film)
            .execute(&b.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO progression (sub, titre_id, position, duree) VALUES ('david', $1, 30, 100)",
        )
        .bind(b.episodes[1])
        .execute(&b.pool)
        .await
        .unwrap();
        let e = abandons(&b.pool, ABANDON_JOURS).await.unwrap();
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(
            (e[0].sub.as_str(), e[0].importe, e[0].date),
            ("alice", false, date(ABANDON_JOURS))
        );
        assert_eq!(
            e[0].donnees,
            Donnees::EntracteAbandon {
                titre: b.film.to_string(),
                genre: Genre::Film
            }
        );
        sans_fuite(&b, &e);
        assert!(abandons(&b.pool, ABANDON_JOURS).await.unwrap().is_empty());
        let lectures = Lectures::default();
        rapporter(
            &b.pool,
            &lectures,
            "alice",
            b.film,
            40.0,
            100.0,
            Instant::now(),
            0.0,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE progression SET maj = $1 WHERE sub = 'alice'")
            .bind(date(1))
            .execute(&b.pool)
            .await
            .unwrap();
        assert!(abandons(&b.pool, ABANDON_JOURS).await.unwrap().is_empty());
        let e = abandons(&b.pool, 0).await.unwrap();
        assert_eq!(
            e.iter().map(|e| e.sub.as_str()).collect::<Vec<_>>(),
            ["david"]
        );
        assert_eq!(
            e[0].donnees,
            Donnees::EntracteAbandon {
                titre: b.episodes[1].to_string(),
                genre: Genre::Episode
            }
        );
    }

    #[tokio::test]
    async fn import_date_marque_et_relancable() {
        let Some(b) = base().await else { return };
        ancienne(&b.pool, "alice", b.film, 30.4, 1).await;
        ancienne(&b.pool, "alice", b.episodes[0], 39.0, 2).await;
        ancienne(&b.pool, "bob", b.film, 5.0, 3).await;
        ancienne(&b.pool, "bob", b.episodes[0], 0.0, 3).await;
        for (sub, titre, jours) in [
            ("alice", b.episodes[0], 4),
            ("alice", b.episodes[1], 5),
            ("bob", b.film, 6),
        ] {
            sqlx::query("INSERT INTO vus (sub, titre_id, vu_le) VALUES ($1, $2, $3)")
                .bind(sub)
                .bind(titre)
                .bind(date(jours))
                .execute(&b.pool)
                .await
                .unwrap();
        }
        let premier = historique(&b.pool).await.unwrap();
        assert_eq!(historique(&b.pool).await.unwrap(), premier);
        let [f, e1, e2, s] = [b.film, b.episodes[0], b.episodes[1], b.serie].map(|u| u.to_string());
        let lecture = |titre: &str, serie: bool, secondes: u32| Donnees::EntracteLecture {
            titre: titre.to_string(),
            genre: if serie { Genre::Episode } else { Genre::Film },
            serie: serie.then(|| s.clone()),
            secondes,
        };
        let vu = |titre: &str, serie: bool| Donnees::EntracteVu {
            titre: titre.to_string(),
            genre: if serie { Genre::Episode } else { Genre::Film },
            serie: serie.then(|| s.clone()),
        };
        let mut attendus = vec![
            (
                format!("import:entracte:lecture:alice:{f}"),
                1,
                lecture(&f, false, 30),
            ),
            (
                format!("import:entracte:lecture:alice:{e1}"),
                4,
                lecture(&e1, true, 100),
            ),
            (format!("import:entracte:vu:alice:{e1}"), 4, vu(&e1, true)),
            (
                format!("import:entracte:lecture:alice:{e2}"),
                5,
                lecture(&e2, true, 40),
            ),
            (format!("import:entracte:vu:alice:{e2}"), 5, vu(&e2, true)),
            (
                format!("import:entracte:lecture:bob:{f}"),
                6,
                lecture(&f, false, 100),
            ),
            (format!("import:entracte:vu:bob:{f}"), 6, vu(&f, false)),
            (
                format!("import:entracte:serie_terminee:alice:{s}"),
                5,
                Donnees::EntracteSerieTerminee { serie: s.clone() },
            ),
        ];
        let mut obtenus: Vec<_> = premier
            .iter()
            .map(|e| {
                assert!(e.importe && e.sub == e.id.split(':').nth(3).unwrap());
                (e.id.clone(), e.date, e.donnees.clone())
            })
            .collect();
        attendus.sort_by(|a, b| a.0.cmp(&b.0));
        obtenus.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            obtenus,
            attendus
                .into_iter()
                .map(|(id, jours, d)| (id, date(jours), d))
                .collect::<Vec<_>>()
        );
        sans_fuite(&b, &premier);

        let erreur = importer(&b.pool, &Emetteur::inactif()).await.unwrap_err();
        assert!(erreur.contains(VARIABLE_URL), "{erreur}");
        let arrete = Emetteur::depuis(Some("redis://127.0.0.1:1"), None);
        let erreur = importer(&b.pool, &arrete).await.unwrap_err();
        assert!(erreur.starts_with("file d'évènements"), "{erreur}");
        let rapport = Rapport::de(&premier);
        assert_eq!(
            rapport,
            Rapport {
                lectures: 4,
                vus: 3,
                series: 1,
                badges: 0
            }
        );
        assert_eq!(
            rapport.to_string(),
            "Import vers Sablier : 4 lectures, 3 titres vus, 1 séries terminées, 0 badges.\n"
        );
    }

    #[tokio::test]
    async fn variables_manquantes() {
        let erreur = lancer(|_| None).await.unwrap_err();
        assert!(erreur.contains("DATABASE_URL"), "{erreur}");
    }
}
