use axum::response::Response;
use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{
    agregats,
    etat::AppState,
    http::{bd, invalide},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Periode {
    Jour,
    Semaine,
    Mois,
    Annee,
    Tout,
}

pub fn debut_des_temps() -> NaiveDate {
    NaiveDate::from_ymd_opt(1, 1, 1).unwrap_or_default()
}

pub fn fin_des_temps() -> NaiveDate {
    NaiveDate::from_ymd_opt(9999, 12, 31).unwrap_or_default()
}

impl Periode {
    pub const TOUTES: [Periode; 5] = [
        Periode::Jour,
        Periode::Semaine,
        Periode::Mois,
        Periode::Annee,
        Periode::Tout,
    ];

    pub fn nom(self) -> &'static str {
        match self {
            Periode::Jour => "day",
            Periode::Semaine => "week",
            Periode::Mois => "month",
            Periode::Annee => "year",
            Periode::Tout => "all",
        }
    }

    pub fn lire(texte: &str) -> Option<Self> {
        Self::TOUTES.into_iter().find(|p| p.nom() == texte)
    }

    pub fn bornes(self, date: NaiveDate) -> (NaiveDate, NaiveDate) {
        match self {
            Periode::Jour => (date, date),
            Periode::Semaine => {
                let lundi = date - Days::new(u64::from(date.weekday().num_days_from_monday()));
                (lundi, lundi + Days::new(6))
            }
            Periode::Mois => {
                let premier = date.with_day(1).unwrap_or(date);
                let suivant = if date.month() == 12 {
                    NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
                };
                (premier, suivant.and_then(|s| s.pred_opt()).unwrap_or(date))
            }
            Periode::Annee => (
                NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap_or(date),
                NaiveDate::from_ymd_opt(date.year(), 12, 31).unwrap_or(date),
            ),
            Periode::Tout => (debut_des_temps(), fin_des_temps()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plage {
    pub periode: Periode,
    pub date: NaiveDate,
    pub du: NaiveDate,
    pub au: NaiveDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Intervalle {
    pub period: &'static str,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Plage {
    pub fn intervalle(&self, premier: Option<NaiveDate>) -> Intervalle {
        let (from, to) = match self.periode {
            Periode::Tout => (premier.unwrap_or(self.date), self.date),
            _ => (self.du, self.au),
        };
        Intervalle {
            period: self.periode.nom(),
            from,
            to,
        }
    }

    pub async fn intervalle_de(&self, pool: &PgPool, sub: &str) -> Result<Intervalle, sqlx::Error> {
        Ok(self.intervalle(agregats::premier_jour(pool, sub).await?))
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Demande {
    pub period: Option<String>,
    pub date: Option<String>,
}

impl Demande {
    pub fn lire(&self, aujourd_hui: NaiveDate) -> Result<Plage, &'static str> {
        let periode = match self.period.as_deref() {
            None | Some("") => Periode::Mois,
            Some(p) => Periode::lire(p).ok_or("period : day, week, month, year ou all")?,
        };
        let date = match self.date.as_deref() {
            None | Some("") => aujourd_hui,
            Some(d) => d
                .parse::<NaiveDate>()
                .ok()
                .filter(|d| (debut_des_temps()..=fin_des_temps()).contains(d))
                .ok_or("date : AAAA-MM-JJ")?,
        };
        let (du, au) = periode.bornes(date);
        Ok(Plage {
            periode,
            date,
            du,
            au,
        })
    }

    pub async fn plage(&self, state: &AppState) -> Result<Plage, Response> {
        self.lire(state.aujourd_hui().await.map_err(bd)?)
            .map_err(invalide)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn j(texte: &str) -> NaiveDate {
        texte.parse().unwrap()
    }

    fn bornes(p: Periode, date: &str) -> (String, String) {
        let (du, au) = p.bornes(j(date));
        (du.to_string(), au.to_string())
    }

    fn paire(du: &str, au: &str) -> (String, String) {
        (du.to_string(), au.to_string())
    }

    #[test]
    fn bornes_de_chaque_periode() {
        assert_eq!(
            bornes(Periode::Jour, "2026-03-29"),
            paire("2026-03-29", "2026-03-29")
        );
        assert_eq!(
            bornes(Periode::Semaine, "2026-03-29"),
            paire("2026-03-23", "2026-03-29")
        );
        assert_eq!(
            bornes(Periode::Semaine, "2026-03-30"),
            paire("2026-03-30", "2026-04-05")
        );
        assert_eq!(
            bornes(Periode::Semaine, "2026-01-01"),
            paire("2025-12-29", "2026-01-04")
        );
        assert_eq!(
            bornes(Periode::Mois, "2026-02-14"),
            paire("2026-02-01", "2026-02-28")
        );
        assert_eq!(
            bornes(Periode::Mois, "2028-02-29"),
            paire("2028-02-01", "2028-02-29")
        );
        assert_eq!(
            bornes(Periode::Mois, "2026-12-31"),
            paire("2026-12-01", "2026-12-31")
        );
        assert_eq!(
            bornes(Periode::Annee, "2026-06-15"),
            paire("2026-01-01", "2026-12-31")
        );
        assert_eq!(
            bornes(Periode::Tout, "2026-06-15"),
            paire("0001-01-01", "9999-12-31")
        );
    }

    #[test]
    fn noms_de_requete() {
        let noms: Vec<&str> = Periode::TOUTES.iter().map(|p| p.nom()).collect();
        assert_eq!(noms, ["day", "week", "month", "year", "all"]);
        for p in Periode::TOUTES {
            assert_eq!(Periode::lire(p.nom()), Some(p));
        }
        assert_eq!(Periode::lire("decade"), None);
        assert_eq!(Periode::lire("Day"), None);
    }

    #[test]
    fn demande_avec_defauts_et_refus() {
        let aujourd_hui = j("2026-11-18");
        let d = |period: Option<&str>, date: Option<&str>| {
            Demande {
                period: period.map(str::to_string),
                date: date.map(str::to_string),
            }
            .lire(aujourd_hui)
        };
        let p = d(None, None).unwrap();
        assert_eq!(
            (p.periode, p.date, p.du, p.au),
            (Periode::Mois, aujourd_hui, j("2026-11-01"), j("2026-11-30"))
        );
        let p = d(Some("week"), Some("2026-03-29")).unwrap();
        assert_eq!((p.du, p.au), (j("2026-03-23"), j("2026-03-29")));
        assert_eq!(d(Some(""), Some("")).unwrap().periode, Periode::Mois);
        assert!(d(Some("semaine"), None).unwrap_err().contains("period"));
        for faux in ["29/03/2026", "2026-02-30", "demain", "0000-01-01"] {
            assert!(d(None, Some(faux)).unwrap_err().contains("date"), "{faux}");
        }
        let i = d(Some("week"), Some("2026-03-29"))
            .unwrap()
            .intervalle(Some(j("2020-01-01")));
        assert_eq!(
            (i.period, i.from, i.to),
            ("week", j("2026-03-23"), j("2026-03-29"))
        );
        let tout = d(Some("all"), None).unwrap();
        let i = tout.intervalle(Some(j("2020-01-01")));
        assert_eq!(
            (i.period, i.from, i.to),
            ("all", j("2020-01-01"), aujourd_hui)
        );
        let i = tout.intervalle(None);
        assert_eq!((i.from, i.to), (aujourd_hui, aujourd_hui));
    }
}
