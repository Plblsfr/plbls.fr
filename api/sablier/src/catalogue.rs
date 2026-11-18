use std::collections::HashSet;

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;

use crate::{
    agregats::{self, UTILISATION},
    domaines::{entracte, messagerie, sillon},
    droits::Droits,
    periode::{Periode, debut_des_temps},
    series::{self, Serie},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Domaine {
    #[serde(rename = "usage")]
    Utilisation,
    #[serde(rename = "entracte")]
    Entracte,
    #[serde(rename = "sillon")]
    Sillon,
    #[serde(rename = "messaging")]
    Messagerie,
}

impl Domaine {
    pub fn accessible(self, droits: &Droits) -> bool {
        match self {
            Domaine::Entracte => droits.entracte,
            Domaine::Sillon => droits.sillon,
            Domaine::Utilisation | Domaine::Messagerie => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Unite {
    #[serde(rename = "count")]
    Nombre,
    #[serde(rename = "seconds")]
    Secondes,
    #[serde(rename = "days")]
    Jours,
    #[serde(rename = "date")]
    Date,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Calcul {
    Somme(&'static str),
    MaxJour(&'static str),
    PlusGrande(&'static str),
    ClesDistinctes(&'static str),
    JoursActifs(&'static str),
    Serie(Serie),
    PremierJour,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Stat {
    #[serde(rename = "key")]
    pub cle: &'static str,
    #[serde(rename = "label")]
    pub libelle: &'static str,
    #[serde(rename = "domain")]
    pub domaine: Domaine,
    #[serde(rename = "unit")]
    pub unite: Unite,
    #[serde(skip)]
    pub calcul: Calcul,
    #[serde(rename = "shareable")]
    pub partageable: bool,
}

impl Stat {
    pub fn pour_paliers(&self) -> bool {
        self.calcul != Calcul::PremierJour
    }
}

const fn stat(
    cle: &'static str,
    libelle: &'static str,
    domaine: Domaine,
    unite: Unite,
    calcul: Calcul,
    partageable: bool,
) -> Stat {
    Stat {
        cle,
        libelle,
        domaine,
        unite,
        calcul,
        partageable,
    }
}

pub const PREMIER_JOUR: &str = "usage.first_day";

pub const STATS: [Stat; 30] = {
    use Calcul::{ClesDistinctes, JoursActifs, MaxJour, PlusGrande, PremierJour, Somme};
    use Domaine::{Entracte, Messagerie, Sillon, Utilisation};
    use Unite::{Date, Jours, Nombre, Secondes};
    [
        stat(
            "usage.active_seconds",
            "Temps actif",
            Utilisation,
            Secondes,
            Somme(UTILISATION),
            true,
        ),
        stat(
            "usage.longest_day",
            "Journée la plus longue",
            Utilisation,
            Secondes,
            MaxJour(UTILISATION),
            false,
        ),
        stat(
            "usage.active_days",
            "Jours actifs",
            Utilisation,
            Jours,
            JoursActifs(UTILISATION),
            true,
        ),
        stat(
            PREMIER_JOUR,
            "Premier jour d'activité",
            Utilisation,
            Date,
            PremierJour,
            true,
        ),
        stat(
            "entracte.seconds",
            "Temps regardé",
            Entracte,
            Secondes,
            Somme(entracte::SECONDES),
            true,
        ),
        stat(
            "entracte.movies_watched",
            "Films vus",
            Entracte,
            Nombre,
            Somme(entracte::FILMS_VUS),
            true,
        ),
        stat(
            "entracte.episodes_watched",
            "Épisodes vus",
            Entracte,
            Nombre,
            Somme(entracte::EPISODES_VUS),
            true,
        ),
        stat(
            "entracte.series_completed",
            "Séries terminées",
            Entracte,
            Nombre,
            Somme(entracte::SERIES_TERMINEES),
            true,
        ),
        stat(
            "entracte.longest_session",
            "Plus longue session",
            Entracte,
            Secondes,
            PlusGrande(entracte::SESSION_MAX),
            false,
        ),
        stat(
            "entracte.biggest_day",
            "Plus grosse journée",
            Entracte,
            Secondes,
            MaxJour(entracte::SECONDES),
            false,
        ),
        stat(
            "entracte.episodes_in_a_row",
            "Épisodes enchaînés",
            Entracte,
            Nombre,
            PlusGrande(entracte::ENCHAINES_MAX),
            true,
        ),
        stat(
            "entracte.abandoned",
            "Titres abandonnés avant la fin",
            Entracte,
            Nombre,
            Somme(entracte::ABANDONS),
            false,
        ),
        stat(
            "entracte.intros_skipped",
            "Intros passées",
            Entracte,
            Nombre,
            Somme(entracte::INTROS_PASSEES),
            false,
        ),
        stat(
            "entracte.cinema_seconds",
            "Temps en salon cinéma",
            Entracte,
            Secondes,
            Somme(entracte::CINEMA_SECONDES),
            true,
        ),
        stat(
            "entracte.cinema_sessions",
            "Séances à plusieurs",
            Entracte,
            Nombre,
            Somme(entracte::CINEMA_SEANCES),
            true,
        ),
        stat(
            "sillon.seconds",
            "Temps d'écoute",
            Sillon,
            Secondes,
            Somme(sillon::SECONDES),
            true,
        ),
        stat(
            "sillon.tracks_played",
            "Titres écoutés",
            Sillon,
            Nombre,
            Somme(sillon::ECOUTES),
            true,
        ),
        stat(
            "sillon.distinct_tracks",
            "Titres différents",
            Sillon,
            Nombre,
            ClesDistinctes(sillon::PISTES),
            true,
        ),
        stat(
            "messaging.messages",
            "Messages envoyés",
            Messagerie,
            Nombre,
            Somme(messagerie::MESSAGES),
            true,
        ),
        stat(
            "messaging.busiest_day",
            "Journée la plus bavarde",
            Messagerie,
            Nombre,
            MaxJour(messagerie::MESSAGES),
            false,
        ),
        stat(
            "messaging.reactions_given",
            "Réactions données",
            Messagerie,
            Nombre,
            Somme(messagerie::REACTIONS_DONNEES),
            true,
        ),
        stat(
            "messaging.reactions_received",
            "Réactions reçues",
            Messagerie,
            Nombre,
            Somme(messagerie::REACTIONS_RECUES),
            true,
        ),
        stat(
            "messaging.files",
            "Fichiers et images envoyés",
            Messagerie,
            Nombre,
            Somme(messagerie::FICHIERS),
            true,
        ),
        stat(
            "messaging.voice_seconds",
            "Temps en vocal",
            Messagerie,
            Secondes,
            Somme(messagerie::VOCAL_SECONDES),
            true,
        ),
        stat(
            "messaging.longest_call",
            "Plus long appel",
            Messagerie,
            Secondes,
            PlusGrande(messagerie::APPEL_MAX),
            false,
        ),
        stat(
            "messaging.screen_share_seconds",
            "Temps en partage d'écran",
            Messagerie,
            Secondes,
            Somme(messagerie::PARTAGE_SECONDES),
            true,
        ),
        stat(
            "streak.global",
            "Jours d'affilée",
            Utilisation,
            Jours,
            Calcul::Serie(Serie::Globale),
            true,
        ),
        stat(
            "streak.entracte",
            "Jours d'affilée avec une lecture",
            Entracte,
            Jours,
            Calcul::Serie(Serie::Entracte),
            true,
        ),
        stat(
            "streak.sillon",
            "Jours d'affilée avec une écoute",
            Sillon,
            Jours,
            Calcul::Serie(Serie::Sillon),
            true,
        ),
        stat(
            "streak.messaging",
            "Jours d'affilée avec un message ou du vocal",
            Messagerie,
            Jours,
            Calcul::Serie(Serie::Messagerie),
            true,
        ),
    ]
};

pub fn trouver(cle: &str) -> Option<&'static Stat> {
    STATS.iter().find(|s| s.cle == cle)
}

pub async fn valeur(
    pool: &PgPool,
    sub: &str,
    stat: &Stat,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<i64, sqlx::Error> {
    match stat.calcul {
        Calcul::Somme(s) => agregats::somme(pool, sub, s, du, au).await,
        Calcul::MaxJour(s) => Ok(agregats::max_jour(pool, sub, s, du, au)
            .await?
            .map_or(0, |(_, n)| n)),
        Calcul::PlusGrande(s) => agregats::plus_grande(pool, sub, s, du, au).await,
        Calcul::ClesDistinctes(s) => agregats::cles_distinctes(pool, sub, s, du, au).await,
        Calcul::JoursActifs(s) => agregats::jours_actifs(pool, sub, s, du, au).await,
        Calcul::Serie(s) => Ok(series::calculer(&series::jours(pool, sub, s, du, au).await?, au).best),
        Calcul::PremierJour => Ok(0),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Forme {
    Cumul,
    Maximum,
    Suite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progression {
    pub valeur: i64,
    pub atteints: Vec<Option<NaiveDate>>,
}

fn nouvelles_cles(lignes: Vec<(NaiveDate, String)>, grain: Periode) -> Vec<(NaiveDate, i64)> {
    let mut vues = HashSet::new();
    let mut periode = None;
    let mut points = Vec::new();
    for (jour, cle) in lignes {
        let bornes = grain.bornes(jour);
        if periode != Some(bornes) {
            vues.clear();
            periode = Some(bornes);
        }
        if vues.insert(cle) {
            points.push((jour, 1));
        }
    }
    points
}

fn parcourir(
    points: &[(NaiveDate, i64)],
    forme: Forme,
    grain: Periode,
    seuils: &[i64],
    aujourd_hui: NaiveDate,
) -> Progression {
    let mut atteints = vec![None; seuils.len()];
    let mut valeur = 0;
    let mut precedent: Option<NaiveDate> = None;
    for &(jour, x) in points {
        let meme_periode = precedent.is_some_and(|p| grain.bornes(p) == grain.bornes(jour));
        if !meme_periode {
            valeur = 0;
        }
        valeur = match forme {
            Forme::Cumul => valeur + x,
            Forme::Maximum => valeur.max(x),
            Forme::Suite if precedent.and_then(|p| p.succ_opt()) == Some(jour) => valeur + 1,
            Forme::Suite => 1,
        };
        for (atteint, seuil) in atteints.iter_mut().zip(seuils) {
            if atteint.is_none() && valeur >= *seuil {
                *atteint = Some(jour);
            }
        }
        precedent = Some(jour);
    }
    let en_cours = precedent.is_some_and(|p| {
        grain.bornes(p) == grain.bornes(aujourd_hui)
            && (forme != Forme::Suite || p.succ_opt().is_some_and(|l| l >= aujourd_hui))
    });
    Progression {
        valeur: if en_cours { valeur } else { 0 },
        atteints,
    }
}

pub async fn progression(
    pool: &PgPool,
    sub: &str,
    stat: &Stat,
    grain: Periode,
    seuils: &[i64],
    aujourd_hui: NaiveDate,
) -> Result<Progression, sqlx::Error> {
    let du = debut_des_temps();
    let par_jour = |s: &'static str| agregats::par_jour(pool, sub, s, du, aujourd_hui);
    let (points, forme) = match stat.calcul {
        Calcul::Somme(s) => (par_jour(s).await?, Forme::Cumul),
        Calcul::MaxJour(s) | Calcul::PlusGrande(s) => (par_jour(s).await?, Forme::Maximum),
        Calcul::JoursActifs(s) => (
            par_jour(s)
                .await?
                .into_iter()
                .filter(|(_, n)| *n > 0)
                .map(|(jour, _)| (jour, 1))
                .collect(),
            Forme::Cumul,
        ),
        Calcul::ClesDistinctes(s) => (
            nouvelles_cles(
                agregats::cles_par_jour(pool, sub, s, du, aujourd_hui).await?,
                grain,
            ),
            Forme::Cumul,
        ),
        Calcul::Serie(s) => (
            series::jours(pool, sub, s, du, aujourd_hui)
                .await?
                .into_iter()
                .map(|jour| (jour, 1))
                .collect(),
            Forme::Suite,
        ),
        Calcul::PremierJour => (Vec::new(), Forme::Cumul),
    };
    Ok(parcourir(&points, forme, grain, seuils, aujourd_hui))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{env, jour};

    fn p(points: &[(&str, i64)]) -> Vec<(NaiveDate, i64)> {
        points.iter().map(|(j, n)| (jour(j), *n)).collect()
    }

    fn dates(textes: &[Option<&str>]) -> Vec<Option<NaiveDate>> {
        textes.iter().map(|t| t.map(jour)).collect()
    }

    #[test]
    fn catalogue_ferme_aux_cles_uniques() {
        let cles: HashSet<&str> = STATS.iter().map(|s| s.cle).collect();
        assert_eq!(cles.len(), STATS.len());
        for s in &STATS {
            assert_eq!(trouver(s.cle), Some(s));
            assert!(!s.libelle.is_empty());
            let prefixe = s.cle.split('.').next().unwrap();
            assert!(
                ["usage", "entracte", "sillon", "messaging", "streak"].contains(&prefixe),
                "{}",
                s.cle
            );
        }
        assert_eq!(trouver("entracte.genres"), None);
        let prives: Vec<&str> = STATS
            .iter()
            .filter(|s| !s.partageable)
            .map(|s| s.cle)
            .collect();
        assert_eq!(
            prives,
            [
                "usage.longest_day",
                "entracte.longest_session",
                "entracte.biggest_day",
                "entracte.abandoned",
                "entracte.intros_skipped",
                "messaging.busiest_day",
                "messaging.longest_call",
            ]
        );
        assert!(!trouver(PREMIER_JOUR).unwrap().pour_paliers());
        assert_eq!(STATS.iter().filter(|s| s.pour_paliers()).count(), 29);
    }

    #[test]
    fn cumul_date_du_jour_ou_le_seuil_est_atteint() {
        let points = p(&[("2026-01-10", 4), ("2026-02-01", 5), ("2026-02-03", 20)]);
        let r = parcourir(
            &points,
            Forme::Cumul,
            Periode::Tout,
            &[5, 9, 10, 100],
            jour("2026-06-01"),
        );
        assert_eq!(r.valeur, 29);
        assert_eq!(
            r.atteints,
            dates(&[
                Some("2026-02-01"),
                Some("2026-02-01"),
                Some("2026-02-03"),
                None
            ])
        );
        let r = parcourir(
            &points,
            Forme::Cumul,
            Periode::Mois,
            &[5, 9, 25],
            jour("2026-02-10"),
        );
        assert_eq!(r.valeur, 25);
        assert_eq!(
            r.atteints,
            dates(&[Some("2026-02-01"), Some("2026-02-03"), Some("2026-02-03")])
        );
        let r = parcourir(
            &points,
            Forme::Cumul,
            Periode::Mois,
            &[26],
            jour("2026-03-01"),
        );
        assert_eq!((r.valeur, r.atteints), (0, vec![None]));
        let r = parcourir(&[], Forme::Cumul, Periode::Tout, &[1], jour("2026-03-01"));
        assert_eq!((r.valeur, r.atteints), (0, vec![None]));
    }

    #[test]
    fn maximum_et_suite_par_periode() {
        let points = p(&[("2026-01-10", 4), ("2026-02-01", 9), ("2026-02-03", 2)]);
        let r = parcourir(
            &points,
            Forme::Maximum,
            Periode::Tout,
            &[3, 9, 10],
            jour("2026-06-01"),
        );
        assert_eq!(r.valeur, 9);
        assert_eq!(
            r.atteints,
            dates(&[Some("2026-01-10"), Some("2026-02-01"), None])
        );
        let jours = p(&[
            ("2026-01-30", 1),
            ("2026-01-31", 1),
            ("2026-02-01", 1),
            ("2026-02-02", 1),
            ("2026-02-05", 1),
        ]);
        let r = parcourir(
            &jours,
            Forme::Suite,
            Periode::Tout,
            &[2, 4, 5],
            jour("2026-02-06"),
        );
        assert_eq!(r.valeur, 1);
        assert_eq!(
            r.atteints,
            dates(&[Some("2026-01-31"), Some("2026-02-02"), None])
        );
        let r = parcourir(
            &jours,
            Forme::Suite,
            Periode::Tout,
            &[2],
            jour("2026-02-07"),
        );
        assert_eq!(r.valeur, 0);
        let r = parcourir(
            &jours,
            Forme::Suite,
            Periode::Mois,
            &[2, 3],
            jour("2026-02-02"),
        );
        assert_eq!(r.valeur, 2);
        assert_eq!(r.atteints, dates(&[Some("2026-01-31"), None]));
    }

    #[test]
    fn cles_nouvelles_dans_la_periode() {
        let l = |lignes: &[(&str, &str)]| {
            lignes
                .iter()
                .map(|(j, c)| (jour(j), c.to_string()))
                .collect::<Vec<_>>()
        };
        let lignes = l(&[
            ("2026-01-10", "a"),
            ("2026-01-10", "b"),
            ("2026-01-11", "a"),
            ("2026-02-01", "a"),
            ("2026-02-01", "c"),
        ]);
        assert_eq!(
            nouvelles_cles(lignes.clone(), Periode::Tout),
            p(&[("2026-01-10", 1), ("2026-01-10", 1), ("2026-02-01", 1)])
        );
        assert_eq!(
            nouvelles_cles(lignes, Periode::Mois),
            p(&[
                ("2026-01-10", 1),
                ("2026-01-10", 1),
                ("2026-02-01", 1),
                ("2026-02-01", 1)
            ])
        );
    }

    #[tokio::test]
    async fn valeur_et_progression_de_chaque_calcul() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let sub = "sub-alice";
        for (j, stat, cle, n) in [
            ("2026-03-01", UTILISATION, "web|sillon", 100),
            ("2026-03-01", UTILISATION, "pwa|sillon", 50),
            ("2026-03-02", UTILISATION, "web|sillon", 120),
            ("2026-03-01", sillon::PISTES, "p1", 2),
            ("2026-03-02", sillon::PISTES, "p1", 1),
            ("2026-03-02", sillon::PISTES, "p2", 1),
            ("2026-03-01", sillon::ECOUTES, "", 2),
            ("2026-03-02", sillon::ECOUTES, "", 2),
            ("2026-03-02", entracte::SESSION_MAX, "", 700),
        ] {
            agregats::ajouter(pool, sub, jour(j), stat, cle, n)
                .await
                .unwrap();
        }
        let (du, au) = (jour("2026-03-01"), jour("2026-03-31"));
        let mut lues = Vec::new();
        for cle in [
            "usage.active_seconds",
            "usage.longest_day",
            "usage.active_days",
            "sillon.distinct_tracks",
            "entracte.longest_session",
            "streak.sillon",
            "streak.entracte",
            PREMIER_JOUR,
        ] {
            let stat = trouver(cle).unwrap();
            let v = valeur(pool, sub, stat, du, au).await.unwrap();
            let pr = progression(pool, sub, stat, Periode::Tout, &[2], jour("2026-03-02"))
                .await
                .unwrap();
            lues.push((cle, v, pr.valeur, pr.atteints[0].map(|d| d.to_string())));
        }
        let le = |j: &str| Some(j.to_string());
        assert_eq!(
            lues,
            [
                ("usage.active_seconds", 270, 270, le("2026-03-01")),
                ("usage.longest_day", 150, 150, le("2026-03-01")),
                ("usage.active_days", 2, 2, le("2026-03-02")),
                ("sillon.distinct_tracks", 2, 2, le("2026-03-02")),
                ("entracte.longest_session", 700, 700, le("2026-03-02")),
                ("streak.sillon", 2, 2, le("2026-03-02")),
                ("streak.entracte", 0, 0, None),
                (PREMIER_JOUR, 0, 0, None),
            ]
        );
    }
}
