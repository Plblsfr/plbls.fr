use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Days, NaiveDate};
use serde::Serialize;

use crate::stats::{Calcul, Categorie, Ecart, Mode, bp, division, ecart, nature_cle};

pub const FENETRE_DEFAUT: usize = 12;
pub const FENETRES: [usize; 3] = [6, 12, 24];
pub const TENDANCE_PERIODES: usize = 6;
pub const TENDANCE_MIN: usize = 3;
pub const SEUIL_TENDANCE: i64 = 1000;
pub const SEUIL_TENDANCE_BP: i64 = 1000;
pub const CATEGORIES_DU_GRAPHIQUE: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fenetre {
    Periodes(usize),
    Tout,
}

impl Fenetre {
    pub fn lire(cle: Option<&str>) -> Option<Fenetre> {
        match cle {
            None => Some(Fenetre::Periodes(FENETRE_DEFAUT)),
            Some("all") => Some(Fenetre::Tout),
            Some(cle) => cle
                .parse()
                .ok()
                .filter(|n| FENETRES.contains(n))
                .map(Fenetre::Periodes),
        }
    }

    pub fn cle(self) -> String {
        match self {
            Fenetre::Periodes(n) => n.to_string(),
            Fenetre::Tout => "all".to_string(),
        }
    }

    pub fn plage(self, periodes: usize) -> std::ops::Range<usize> {
        match self {
            Fenetre::Periodes(n) => periodes.saturating_sub(n)..periodes,
            Fenetre::Tout => 0..periodes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Tendance {
    pub direction: &'static str,
    pub periods: usize,
    pub average: Option<i64>,
    pub delta: Option<i64>,
    pub delta_bp: Option<i64>,
    pub values: Vec<i64>,
}

pub fn tendance(valeurs: &[i64]) -> Tendance {
    let valeurs = &valeurs[valeurs.len().saturating_sub(TENDANCE_PERIODES)..];
    let n = valeurs.len();
    if n < TENDANCE_MIN {
        return Tendance {
            direction: "insufficient",
            periods: n,
            average: None,
            delta: None,
            delta_bp: None,
            values: valeurs.to_vec(),
        };
    }
    let mut pentes: Vec<i128> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            pentes.push(i128::from(valeurs[j] - valeurs[i]) * 60 / (j - i) as i128);
        }
    }
    pentes.sort_unstable();
    let milieu = pentes.len() / 2;
    let double = if pentes.len() % 2 == 1 {
        2 * pentes[milieu]
    } else {
        pentes[milieu - 1] + pentes[milieu]
    };
    let delta = division(double * (n as i128 - 1), 120);
    let moyenne = division(valeurs.iter().map(|v| i128::from(*v)).sum(), n as i128);
    let delta_bp = bp(delta, moyenne);
    let direction = match delta_bp {
        Some(b) if delta >= SEUIL_TENDANCE && b >= SEUIL_TENDANCE_BP => "up",
        Some(b) if delta <= -SEUIL_TENDANCE && b <= -SEUIL_TENDANCE_BP => "down",
        _ => "stable",
    };
    Tendance {
        direction,
        periods: n,
        average: Some(moyenne),
        delta: Some(delta),
        delta_bp,
        values: valeurs.to_vec(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PeriodeHistorique {
    pub id: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub complete: bool,
    pub open: bool,
    pub expenses: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneHistorique {
    pub id: i64,
    pub name: String,
    pub kind: &'static str,
    pub color: i32,
    pub archived: bool,
    pub monthly_cap: Option<i64>,
    pub amounts: Vec<i64>,
    pub total: i64,
    pub average: Option<i64>,
    pub over_cap: Option<usize>,
    pub trend: Tendance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneNonClassee {
    pub amounts: Vec<i64>,
    pub total: i64,
    pub average: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneTotale {
    pub amounts: Vec<i64>,
    pub total: i64,
    pub average: Option<i64>,
    pub budget: Option<i64>,
    pub over_budget: Option<usize>,
    pub trend: Tendance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Graphique {
    pub category_ids: Vec<i64>,
    pub others: Vec<i64>,
    pub others_categories: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Historique {
    pub mode: &'static str,
    pub window: String,
    pub available: usize,
    pub complete: usize,
    pub periods: Vec<PeriodeHistorique>,
    pub trend_period_ids: Vec<String>,
    pub categories: Vec<LigneHistorique>,
    pub unclassified: LigneNonClassee,
    pub total: LigneTotale,
    pub chart: Graphique,
}

fn moyenne(montants: &[i64], completes: &[bool]) -> Option<i64> {
    let retenus: Vec<i64> = montants
        .iter()
        .zip(completes)
        .filter(|(_, c)| **c)
        .map(|(m, _)| *m)
        .collect();
    (!retenus.is_empty()).then(|| {
        division(
            retenus.iter().map(|m| i128::from(*m)).sum(),
            retenus.len() as i128,
        )
    })
}

pub fn historique(calcul: &Calcul, fenetre: Fenetre) -> Historique {
    let periodes = calcul.periodes();
    let plage = fenetre.plage(periodes.len());
    let completes: Vec<bool> = plage.clone().map(|i| periodes[i].complete).collect();
    let mut reference: Vec<usize> = (0..plage.end)
        .rev()
        .filter(|i| periodes[*i].complete)
        .take(TENDANCE_PERIODES)
        .collect();
    reference.reverse();
    let pente =
        |f: &dyn Fn(usize) -> i64| tendance(&reference.iter().map(|i| f(*i)).collect::<Vec<i64>>());
    let mut categories: Vec<LigneHistorique> = calcul
        .categories_de_depense()
        .map(|c: &Categorie| {
            let montants: Vec<i64> = plage.clone().map(|i| calcul.montant(i, c.id)).collect();
            LigneHistorique {
                id: c.id,
                name: c.nom.clone(),
                kind: nature_cle(c.nature),
                color: c.couleur,
                archived: c.archivee,
                monthly_cap: c.plafond,
                total: montants.iter().sum(),
                average: moyenne(&montants, &completes),
                over_cap: c
                    .plafond
                    .map(|p| montants.iter().filter(|m| **m > p).count()),
                trend: pente(&|i| calcul.montant(i, c.id)),
                amounts: montants,
            }
        })
        .filter(|l| l.amounts.iter().any(|m| *m != 0) || (!l.archived && l.monthly_cap.is_some()))
        .collect();
    categories.sort_by(|x, y| y.total.cmp(&x.total).then_with(|| x.name.cmp(&y.name)));
    let a_classer: Vec<i64> = plage.clone().map(|i| calcul.a_classer(i)).collect();
    let depenses: Vec<i64> = plage.clone().map(|i| calcul.depenses(i)).collect();
    let budget = calcul.plafonds();
    let retenues: Vec<i64> = categories
        .iter()
        .filter(|l| l.total > 0)
        .take(CATEGORIES_DU_GRAPHIQUE)
        .map(|l| l.id)
        .collect();
    let autres: Vec<&LigneHistorique> = categories
        .iter()
        .filter(|l| !retenues.contains(&l.id))
        .collect();
    Historique {
        mode: calcul.mode().cle(),
        window: fenetre.cle(),
        available: periodes.len(),
        complete: completes.iter().filter(|c| **c).count(),
        periods: plage
            .clone()
            .map(|i| PeriodeHistorique {
                id: periodes[i].id.clone(),
                from: periodes[i].from,
                to: periodes[i].to,
                complete: periodes[i].complete,
                open: periodes[i].open,
                expenses: calcul.depenses(i),
            })
            .collect(),
        trend_period_ids: reference.iter().map(|i| periodes[*i].id.clone()).collect(),
        chart: Graphique {
            others: (0..plage.len())
                .map(|k| autres.iter().map(|l| l.amounts[k]).sum())
                .collect(),
            others_categories: autres
                .iter()
                .filter(|l| l.amounts.iter().any(|m| *m != 0))
                .count(),
            category_ids: retenues,
        },
        unclassified: LigneNonClassee {
            total: a_classer.iter().sum(),
            average: moyenne(&a_classer, &completes),
            amounts: a_classer,
        },
        total: LigneTotale {
            total: depenses.iter().sum(),
            average: moyenne(&depenses, &completes),
            budget,
            over_budget: budget.map(|b| depenses.iter().filter(|d| **d > b).count()),
            trend: pente(&|i| calcul.depenses(i)),
            amounts: depenses,
        },
        categories,
    }
}

#[derive(Clone, Debug)]
pub struct ReleveSolde {
    pub id: i64,
    pub compte: i64,
    pub debut: NaiveDate,
    pub fin: NaiveDate,
    pub ancien: i64,
    pub nouveau: i64,
}

#[derive(Clone, Debug)]
pub struct Mouvement {
    pub releve: i64,
    pub jour: NaiveDate,
    pub credit: bool,
    pub montant: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub values: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Trou {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Jour {
    pub date: NaiveDate,
    pub amount: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Solde {
    pub account_id: i64,
    pub days: usize,
    pub reconciled: bool,
    pub segments: Vec<Segment>,
    pub gaps: Vec<Trou>,
    pub first: Option<Jour>,
    pub last: Option<Jour>,
    pub min: Option<Jour>,
    pub max: Option<Jour>,
}

fn lendemain(j: NaiveDate) -> NaiveDate {
    j.checked_add_days(Days::new(1)).unwrap_or(j)
}

fn veille(j: NaiveDate) -> NaiveDate {
    j.pred_opt().unwrap_or(j)
}

pub fn soldes(
    releves: &[ReleveSolde],
    mouvements: &[Mouvement],
    du: NaiveDate,
    au: NaiveDate,
) -> Vec<Solde> {
    let mut par_releve: BTreeMap<i64, Vec<&Mouvement>> = BTreeMap::new();
    for m in mouvements {
        par_releve.entry(m.releve).or_default().push(m);
    }
    let comptes: BTreeSet<i64> = releves.iter().map(|r| r.compte).collect();
    comptes
        .into_iter()
        .map(|compte| {
            let mut du_compte: Vec<&ReleveSolde> =
                releves.iter().filter(|r| r.compte == compte).collect();
            du_compte.sort_by_key(|r| (r.fin, r.id));
            let mut jours: BTreeMap<NaiveDate, i64> = BTreeMap::new();
            let mut rapproche = true;
            for r in du_compte {
                let premier = lendemain(r.debut).min(r.fin);
                let mut variations: BTreeMap<NaiveDate, i64> = BTreeMap::new();
                for m in par_releve.get(&r.id).map_or(&[][..], Vec::as_slice) {
                    *variations.entry(m.jour.clamp(premier, r.fin)).or_default() +=
                        if m.credit { m.montant } else { -m.montant };
                }
                let mut solde = r.ancien;
                if r.debut < premier {
                    jours.insert(r.debut, solde);
                }
                let mut j = premier;
                while j <= r.fin {
                    solde += variations.get(&j).copied().unwrap_or(0);
                    jours.insert(j, solde);
                    j = lendemain(j);
                }
                rapproche &= solde == r.nouveau;
            }
            let mut segments: Vec<Segment> = Vec::new();
            for (j, v) in jours.range(du..=au) {
                match segments.last_mut() {
                    Some(s) if lendemain(s.to) == *j => {
                        s.to = *j;
                        s.values.push(*v);
                    }
                    _ => segments.push(Segment {
                        from: *j,
                        to: *j,
                        values: vec![*v],
                    }),
                }
            }
            let mut trous: Vec<Trou> = Vec::new();
            let mut attendu = du;
            for s in &segments {
                if attendu < s.from {
                    trous.push(Trou {
                        from: attendu,
                        to: veille(s.from),
                        days: (s.from - attendu).num_days(),
                    });
                }
                attendu = lendemain(s.to);
            }
            if attendu <= au {
                trous.push(Trou {
                    from: attendu,
                    to: au,
                    days: (au - attendu).num_days() + 1,
                });
            }
            let points = || {
                segments.iter().flat_map(|s| {
                    s.values.iter().enumerate().map(|(k, v)| Jour {
                        date: s.from.checked_add_days(Days::new(k as u64)).unwrap_or(s.to),
                        amount: *v,
                    })
                })
            };
            Solde {
                account_id: compte,
                days: points().count(),
                reconciled: rapproche,
                first: points().next(),
                last: points().last(),
                min: points().reduce(|a, b| if b.amount < a.amount { b } else { a }),
                max: points().reduce(|a, b| if b.amount > a.amount { b } else { a }),
                gaps: trous,
                segments,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cumul {
    pub current: i64,
    pub previous: i64,
    pub delta: i64,
    pub delta_bp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Comparaison {
    pub month: i64,
    pub month_previous: Option<Ecart>,
    pub ytd: i64,
    pub ytd_compared: Option<Cumul>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneAnnee {
    pub id: i64,
    pub name: String,
    pub kind: &'static str,
    pub color: i32,
    #[serde(flatten)]
    pub comparaison: Comparaison,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MoisAnnee {
    pub month: u32,
    pub current: Option<i64>,
    pub current_complete: Option<bool>,
    pub previous: Option<i64>,
    pub previous_complete: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Annees {
    pub mode: &'static str,
    pub period: String,
    pub year: i32,
    pub month: u32,
    pub previous_year: i32,
    pub has_previous: bool,
    pub compared_months: Vec<u32>,
    pub partial: bool,
    pub months: Vec<MoisAnnee>,
    pub total: Comparaison,
    pub categories: Vec<LigneAnnee>,
    pub unclassified: Comparaison,
}

pub fn annees(calcul: &Calcul, i: usize) -> Annees {
    let periodes = calcul.periodes();
    let cle = |k: usize| {
        let jour = match calcul.mode() {
            Mode::Calendaire => periodes[k].from,
            Mode::Releve => periodes[k].to,
        };
        (jour.year(), jour.month())
    };
    let (annee, mois) = cle(i);
    let mut par_mois: BTreeMap<(i32, u32), Vec<usize>> = BTreeMap::new();
    for k in 0..=i {
        par_mois.entry(cle(k)).or_default().push(k);
    }
    let de = |a: i32, m: u32| par_mois.get(&(a, m)).map(Vec::as_slice);
    let compares: Vec<u32> = (1..=mois)
        .filter(|m| de(annee, *m).is_some() && de(annee - 1, *m).is_some())
        .collect();
    let somme = |indices: Option<&[usize]>, f: &dyn Fn(usize) -> i64| -> i64 {
        indices.map_or(0, |l| l.iter().map(|k| f(*k)).sum())
    };
    let comparer = |f: &dyn Fn(usize) -> i64| {
        let du_mois = somme(de(annee, mois), f);
        let actuel: i64 = compares.iter().map(|m| somme(de(annee, *m), f)).sum();
        let avant: i64 = compares.iter().map(|m| somme(de(annee - 1, *m), f)).sum();
        Comparaison {
            month: du_mois,
            month_previous: de(annee - 1, mois).map(|l| ecart(du_mois, somme(Some(l), f))),
            ytd: (1..=mois).map(|m| somme(de(annee, m), f)).sum(),
            ytd_compared: (!compares.is_empty()).then(|| Cumul {
                current: actuel,
                previous: avant,
                delta: actuel - avant,
                delta_bp: bp(actuel - avant, avant),
            }),
        }
    };
    let complet =
        |indices: Option<&[usize]>| indices.map(|l| l.iter().all(|k| periodes[*k].complete));
    let mut categories: Vec<LigneAnnee> = calcul
        .categories_de_depense()
        .map(|c| LigneAnnee {
            id: c.id,
            name: c.nom.clone(),
            kind: nature_cle(c.nature),
            color: c.couleur,
            comparaison: comparer(&|k| calcul.montant(k, c.id)),
        })
        .filter(|l| {
            let c = &l.comparaison;
            c.ytd != 0
                || c.month != 0
                || c.month_previous.as_ref().is_some_and(|e| e.amount != 0)
                || c.ytd_compared.as_ref().is_some_and(|e| e.previous != 0)
        })
        .collect();
    categories.sort_by(|x, y| {
        y.comparaison
            .ytd
            .cmp(&x.comparaison.ytd)
            .then_with(|| x.name.cmp(&y.name))
    });
    Annees {
        mode: calcul.mode().cle(),
        period: periodes[i].id.clone(),
        year: annee,
        month: mois,
        previous_year: annee - 1,
        has_previous: par_mois.keys().any(|(a, _)| *a == annee - 1),
        partial: compares.iter().any(|m| {
            complet(de(annee, *m)) == Some(false) || complet(de(annee - 1, *m)) == Some(false)
        }),
        compared_months: compares.clone(),
        months: (1..=12)
            .map(|m| MoisAnnee {
                month: m,
                current: de(annee, m).map(|l| somme(Some(l), &|k| calcul.depenses(k))),
                current_complete: complet(de(annee, m)),
                previous: de(annee - 1, m).map(|l| somme(Some(l), &|k| calcul.depenses(k))),
                previous_complete: complet(de(annee - 1, m)),
            })
            .collect(),
        total: comparer(&|k| calcul.depenses(k)),
        unclassified: comparer(&|k| calcul.a_classer(k)),
        categories,
    }
}
