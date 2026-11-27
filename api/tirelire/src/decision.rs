use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{
    habitudes::{par_commercant, petits_achats_sur, variable},
    recurrents::{NOUVEAUTE_JOURS, Recurrents},
    stats::{
        Calcul, Ecart, LigneCategorie, Mois, Moyenne, Periode, SEUIL_MOYENNE, SEUIL_MOYENNE_BP, bp,
        division, nature_cle,
    },
};

pub const BASE_PERIODES: usize = 3;
pub const REGLAGES_MAX: usize = 100;
pub const VARIATIONS_CITEES: usize = 3;
pub const LEVIERS: usize = 3;
pub const SEUIL_A_CLASSER_BP: i64 = 1000;
pub const LEVIER_COMMERCANT: i64 = 2000;
pub const LEVIER_COMMERCANT_BP: i64 = 5000;
pub const LEVIER_PETITS_ACHATS_BP: i64 = 5000;
pub const FENETRE_PETITS_ACHATS: usize = 3;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reglage {
    pub id: i64,
    #[serde(default)]
    pub percent: Option<i64>,
    #[serde(default)]
    pub target: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Base {
    pub method: &'static str,
    pub periods: usize,
    pub period_ids: Vec<String>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub fragile: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Reference {
    pub expenses: i64,
    pub income: i64,
    pub savings: i64,
    pub remaining: i64,
    pub savings_rate_bp: Option<i64>,
    pub budget: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneSimulee {
    pub id: i64,
    pub name: String,
    pub kind: &'static str,
    pub color: i32,
    pub monthly_cap: Option<i64>,
    pub base: i64,
    pub stopped: i64,
    pub percent: Option<i64>,
    pub target: Option<i64>,
    pub saving: i64,
    pub amount: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AbonnementSimule {
    pub id: String,
    pub name: Option<String>,
    pub category_id: Option<i64>,
    pub cadence: &'static str,
    pub amount: i64,
    pub monthly: i64,
    pub yearly: i64,
    pub stopped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Effet {
    pub monthly: i64,
    pub yearly: i64,
    pub categories: i64,
    pub subscriptions: i64,
    pub expenses: i64,
    pub remaining: i64,
    pub savings: i64,
    pub savings_rate_bp: Option<i64>,
    pub budget_delta: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Simulation {
    pub mode: &'static str,
    pub period: String,
    pub basis: Base,
    pub reference: Reference,
    pub categories: Vec<LigneSimulee>,
    pub subscriptions: Vec<AbonnementSimule>,
    pub result: Effet,
}

pub fn economie(base: i64, arretes: i64, percent: Option<i64>, target: Option<i64>) -> (i64, i64) {
    let ajustable = (base - arretes).max(0);
    let economie = match (percent, target) {
        (Some(p), _) => division(i128::from(ajustable) * i128::from(p), 100),
        (None, Some(t)) => (ajustable - t).max(0),
        (None, None) => 0,
    };
    (economie, ajustable - economie)
}

pub fn effet(
    reference: &Reference,
    categories: i64,
    abonnements_par_mois: i64,
    abonnements_par_an: i64,
) -> Effet {
    let monthly = categories + abonnements_par_mois;
    let expenses = reference.expenses - monthly;
    Effet {
        monthly,
        yearly: 12 * categories + abonnements_par_an,
        categories,
        subscriptions: abonnements_par_mois,
        expenses,
        remaining: reference.remaining + monthly,
        savings: reference.savings + monthly,
        savings_rate_bp: bp(reference.savings + monthly, reference.income),
        budget_delta: reference.budget.map(|b| expenses - b),
    }
}

fn moyenne(retenues: &[usize], f: impl Fn(usize) -> i64) -> i64 {
    let somme: i128 = retenues.iter().map(|j| i128::from(f(*j))).sum();
    division(somme, (retenues.len() as i128).max(1))
}

pub fn simuler(
    calcul: &Calcul,
    i: usize,
    recurrents: &Recurrents,
    reglages: &[Reglage],
    arrets: &[String],
) -> Result<Simulation, &'static str> {
    if reglages.len() > REGLAGES_MAX || arrets.len() > REGLAGES_MAX {
        return Err("categories et subscriptions : 100 éléments au plus");
    }
    let periodes = calcul.periodes();
    let completes = calcul.completes_avant(i + 1, BASE_PERIODES);
    let fragile = completes.is_empty();
    let mut retenues = if fragile { vec![i] } else { completes };
    retenues.sort_unstable();
    let (Some(premiere), Some(derniere)) = (retenues.first(), retenues.last()) else {
        return Err("period : période inconnue");
    };
    let basis = Base {
        method: if fragile { "current" } else { "average" },
        periods: retenues.len(),
        period_ids: retenues.iter().map(|j| periodes[*j].id.clone()).collect(),
        from: periodes[*premiere].from,
        to: periodes[*derniere].known_through,
        fragile,
    };
    let depenses = moyenne(&retenues, |j| calcul.depenses(j));
    let revenus = moyenne(&retenues, |j| calcul.revenus(j));
    let epargne = moyenne(&retenues, |j| calcul.epargne(j));
    let reference = Reference {
        expenses: depenses,
        income: revenus,
        savings: epargne,
        remaining: revenus - depenses - epargne,
        savings_rate_bp: bp(epargne, revenus),
        budget: calcul.plafonds(),
    };

    let mut choisis: BTreeSet<&str> = BTreeSet::new();
    for id in arrets {
        if !choisis.insert(id.as_str()) {
            return Err("subscriptions : un abonnement est cité deux fois");
        }
    }
    let subscriptions: Vec<AbonnementSimule> = recurrents
        .items
        .iter()
        .filter(|r| !r.dismissed && r.status == "active" && r.group == "subscription")
        .map(|r| AbonnementSimule {
            id: r.id.clone(),
            name: r.name.clone(),
            category_id: r.category_id,
            cadence: r.cadence,
            amount: r.amount,
            monthly: r.monthly,
            yearly: r.yearly,
            stopped: choisis.contains(r.id.as_str()),
        })
        .collect();
    if subscriptions.iter().filter(|a| a.stopped).count() != choisis.len() {
        return Err("subscriptions : abonnement inconnu, arrêté ou écarté");
    }
    let mut arretes: BTreeMap<i64, i64> = BTreeMap::new();
    for a in subscriptions.iter().filter(|a| a.stopped) {
        if let Some(c) = a.category_id {
            *arretes.entry(c).or_default() += a.monthly;
        }
    }

    let mut par_id: BTreeMap<i64, &Reglage> = BTreeMap::new();
    for r in reglages {
        if par_id.insert(r.id, r).is_some() {
            return Err("categories : une catégorie est citée deux fois");
        }
    }
    let mut categories = Vec::new();
    for c in calcul.categories_de_depense().filter(|c| !c.archivee) {
        let base = moyenne(&retenues, |j| calcul.montant(j, c.id)).max(0);
        let stopped = arretes.get(&c.id).copied().unwrap_or(0).min(base);
        let reglage = par_id.remove(&c.id);
        let (percent, target) = match reglage {
            None => (None, None),
            Some(r) => match (r.percent, r.target) {
                (Some(p), None) if (0..=100).contains(&p) => (Some(p), None),
                (None, Some(t)) if (0..=base).contains(&t) => (None, Some(t)),
                (Some(_), None) => return Err("percent : entier de 0 à 100"),
                (None, Some(_)) => {
                    return Err("target : centimes, de 0 à la dépense habituelle de la catégorie");
                }
                _ => return Err("categories : percent ou target, l'un des deux"),
            },
        };
        let (saving, amount) = economie(base, stopped, percent, target);
        categories.push(LigneSimulee {
            id: c.id,
            name: c.nom.clone(),
            kind: nature_cle(c.nature),
            color: c.couleur,
            monthly_cap: c.plafond,
            base,
            stopped,
            percent,
            target,
            saving,
            amount,
        });
    }
    if !par_id.is_empty() {
        return Err("categories : catégorie de dépense inconnue ou archivée");
    }
    categories.sort_by(|x, y| y.base.cmp(&x.base).then_with(|| x.name.cmp(&y.name)));

    let result = effet(
        &reference,
        categories.iter().map(|c| c.saving).sum(),
        subscriptions
            .iter()
            .filter(|a| a.stopped)
            .map(|a| a.monthly)
            .sum(),
        subscriptions
            .iter()
            .filter(|a| a.stopped)
            .map(|a| a.yearly)
            .sum(),
    );
    Ok(Simulation {
        mode: calcul.mode().cle(),
        period: periodes[i].id.clone(),
        basis,
        reference,
        categories,
        subscriptions,
        result,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Variation {
    pub category_id: i64,
    pub name: String,
    pub kind: &'static str,
    pub color: i32,
    pub amount: i64,
    pub reference_amount: i64,
    pub delta: i64,
    pub delta_bp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Comparaison {
    pub period_id: Option<String>,
    pub periods: Option<usize>,
    pub up: Vec<Variation>,
    pub up_total: usize,
    pub down: Vec<Variation>,
    pub down_total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Levier {
    pub rank: usize,
    pub kind: &'static str,
    pub category_id: Option<i64>,
    pub merchant_id: Option<i64>,
    pub recurring_id: Option<String>,
    pub name: Option<String>,
    pub amount: i64,
    pub reference_amount: i64,
    pub periods: Option<usize>,
    pub count: Option<i64>,
    pub monthly: i64,
    pub yearly: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DepensesResumees {
    pub total: i64,
    pub previous: Option<Ecart>,
    pub usual: Option<Moyenne>,
    pub budget: Option<i64>,
    pub budget_delta: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EpargneResumee {
    pub total: i64,
    pub rate_bp: Option<i64>,
    pub previous_rate_bp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AClasser {
    pub amount: i64,
    pub operations: i64,
    pub share_bp: Option<i64>,
    pub distorting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Synthese {
    pub mode: &'static str,
    pub period: Periode,
    pub previous_id: Option<String>,
    pub next_id: Option<String>,
    pub history: &'static str,
    pub partial: bool,
    pub operations: i64,
    pub expenses: DepensesResumees,
    pub savings: EpargneResumee,
    pub unclassified: AClasser,
    pub previous: Option<Comparaison>,
    pub previous_reason: Option<&'static str>,
    pub usual: Option<Comparaison>,
    pub levers: Vec<Levier>,
    pub levers_total: usize,
}

pub fn significatif(delta: i64, delta_bp: Option<i64>) -> bool {
    delta.abs() >= SEUIL_MOYENNE && delta_bp.is_none_or(|b| b.abs() >= SEUIL_MOYENNE_BP)
}

fn comparer(
    mois: &Mois,
    partiel: bool,
    period_id: Option<String>,
    periods: Option<usize>,
    reference: impl Fn(&LigneCategorie) -> Option<(i64, i64, Option<i64>)>,
) -> Comparaison {
    let mut variations: Vec<Variation> = mois
        .categories
        .iter()
        .filter_map(|l| {
            let (montant, delta, delta_bp) = reference(l)?;
            significatif(delta, delta_bp).then(|| Variation {
                category_id: l.id,
                name: l.name.clone(),
                kind: l.kind,
                color: l.color,
                amount: l.amount,
                reference_amount: montant,
                delta,
                delta_bp,
            })
        })
        .collect();
    variations.sort_by(|x, y| {
        y.delta
            .abs()
            .cmp(&x.delta.abs())
            .then_with(|| x.name.cmp(&y.name))
            .then_with(|| x.category_id.cmp(&y.category_id))
    });
    let (up, down): (Vec<Variation>, Vec<Variation>) =
        variations.into_iter().partition(|v| v.delta > 0);
    let down = if partiel { Vec::new() } else { down };
    Comparaison {
        period_id,
        periods,
        up_total: up.len(),
        down_total: down.len(),
        up: up.into_iter().take(VARIATIONS_CITEES).collect(),
        down: down.into_iter().take(VARIATIONS_CITEES).collect(),
    }
}

fn rang_du_type(kind: &str) -> usize {
    [
        "over_cap",
        "over_average",
        "subscription_changed",
        "subscription_new",
        "merchant_up",
        "small_purchases",
    ]
    .iter()
    .position(|k| *k == kind)
    .unwrap_or(usize::MAX)
}

pub fn leviers(calcul: &Calcul, i: usize, mois: &Mois, recurrents: &Recurrents) -> Vec<Levier> {
    let d = calcul.donnees();
    let p = &calcul.periodes()[i];
    let mut res = Vec::new();
    let mut categories: BTreeSet<i64> = BTreeSet::new();
    for depassement in &mois.overruns {
        let Some(l) = mois
            .categories
            .iter()
            .find(|l| l.id == depassement.category_id && !l.archived)
        else {
            continue;
        };
        let kind = match depassement.reference {
            "cap" => "over_cap",
            _ if l.kind == "variable" => "over_average",
            _ => continue,
        };
        categories.insert(l.id);
        res.push(Levier {
            rank: 0,
            kind,
            category_id: Some(l.id),
            merchant_id: None,
            recurring_id: None,
            name: Some(l.name.clone()),
            amount: l.amount,
            reference_amount: depassement.reference_amount,
            periods: depassement.periods,
            count: None,
            monthly: depassement.excess,
            yearly: 12 * depassement.excess,
        });
    }

    for r in recurrents
        .items
        .iter()
        .filter(|r| !r.dismissed && r.status == "active")
    {
        for a in &r.alerts {
            let depuis = match a.kind {
                "new" => a.first_date.unwrap_or(a.date),
                _ => a.date,
            };
            let ecoule = (p.known_through - depuis).num_days();
            if !(0..=NOUVEAUTE_JOURS).contains(&ecoule) {
                continue;
            }
            let (kind, reference_amount, yearly) = match (a.kind, a.old_amount, a.yearly_delta) {
                ("amount_changed", Some(ancien), Some(par_an)) if par_an > 0 => {
                    ("subscription_changed", ancien, par_an)
                }
                ("new", _, _) => ("subscription_new", 0, r.yearly),
                _ => continue,
            };
            res.push(Levier {
                rank: 0,
                kind,
                category_id: r.category_id,
                merchant_id: Some(r.merchant_id),
                recurring_id: Some(r.id.clone()),
                name: r.name.clone(),
                amount: a.new_amount,
                reference_amount,
                periods: None,
                count: None,
                monthly: division(i128::from(yearly), 12),
                yearly,
            });
        }
    }

    if let Some(j) = calcul
        .precedente(i)
        .filter(|j| calcul.periodes()[*j].complete)
    {
        let avant = par_commercant(calcul, j..j + 1);
        let mut categorie_de: BTreeMap<i64, (NaiveDate, i64, Option<i64>)> = BTreeMap::new();
        for o in calcul.operations(i).filter(|o| variable(calcul, o)) {
            if let Some(c) = o.commercant {
                let cle = (o.reference(), o.id, o.categorie);
                let e = categorie_de.entry(c).or_insert(cle);
                if (cle.0, cle.1) > (e.0, e.1) {
                    *e = cle;
                }
            }
        }
        for (id, (montant, _)) in par_commercant(calcul, i..i + 1) {
            let reference = avant.get(&id).map_or(0, |x| x.0);
            let delta = montant - reference;
            let categorie = categorie_de.get(&id).and_then(|x| x.2);
            if reference <= 0
                || delta < LEVIER_COMMERCANT
                || bp(delta, reference).is_none_or(|b| b < LEVIER_COMMERCANT_BP)
                || categorie.is_some_and(|c| categories.contains(&c))
            {
                continue;
            }
            res.push(Levier {
                rank: 0,
                kind: "merchant_up",
                category_id: categorie,
                merchant_id: Some(id),
                recurring_id: None,
                name: d.commercants.get(&id).cloned(),
                amount: montant,
                reference_amount: reference,
                periods: None,
                count: None,
                monthly: delta,
                yearly: 12 * delta,
            });
        }
    }

    let debut = (i + 1).saturating_sub(FENETRE_PETITS_ACHATS);
    let petits = petits_achats_sur(calcul, debut..i + 1);
    if let Some(par_an) = petits.yearly.filter(|y| *y > 0) {
        let yearly = division(
            i128::from(par_an) * i128::from(LEVIER_PETITS_ACHATS_BP),
            10_000,
        );
        res.push(Levier {
            rank: 0,
            kind: "small_purchases",
            category_id: None,
            merchant_id: petits.merchants.first().map(|m| m.merchant_id),
            recurring_id: None,
            name: petits.merchants.first().and_then(|m| m.name.clone()),
            amount: petits.amount,
            reference_amount: par_an,
            periods: Some(i + 1 - debut),
            count: Some(petits.count),
            monthly: division(i128::from(yearly), 12),
            yearly,
        });
    }

    res.retain(|l| l.yearly > 0 && l.monthly > 0);
    res.sort_by(|x, y| {
        y.yearly
            .cmp(&x.yearly)
            .then_with(|| rang_du_type(x.kind).cmp(&rang_du_type(y.kind)))
            .then_with(|| x.name.cmp(&y.name))
            .then_with(|| x.category_id.cmp(&y.category_id))
            .then_with(|| x.merchant_id.cmp(&y.merchant_id))
            .then_with(|| x.recurring_id.cmp(&y.recurring_id))
    });
    for (rang, l) in res.iter_mut().enumerate() {
        l.rank = rang + 1;
    }
    res
}

pub fn resumer(calcul: &Calcul, i: usize, recurrents: &Recurrents) -> Synthese {
    let mois = calcul.mois(i);
    let periodes = calcul.periodes();
    let partiel = !mois.period.complete;
    let precedente = calcul.precedente(i);
    let (previous, previous_reason) = match precedente {
        None => (None, Some("none")),
        Some(j) if !periodes[j].complete => (None, Some("incomplete")),
        Some(j) => (
            Some(comparer(
                &mois,
                partiel,
                Some(periodes[j].id.clone()),
                None,
                |l| l.previous.as_ref().map(|e| (e.amount, e.delta, e.delta_bp)),
            )),
            None,
        ),
    };
    let habitude = mois.expenses.avg3.as_ref().map(|m| m.periods);
    let usual = habitude.map(|n| {
        comparer(&mois, partiel, None, Some(n), |l| {
            l.avg3.as_ref().map(|m| (m.amount, m.delta, m.delta_bp))
        })
    });
    let mut levers = leviers(calcul, i, &mois, recurrents);
    let levers_total = levers.len();
    levers.truncate(LEVIERS);
    let history = if i == 0 {
        "first"
    } else if previous.is_none() && usual.is_none() {
        "insufficient"
    } else {
        "ok"
    };
    Synthese {
        mode: mois.mode,
        period: mois.period.clone(),
        previous_id: mois.previous_id.clone(),
        next_id: mois.next_id.clone(),
        history,
        partial: partiel,
        operations: mois.operations,
        expenses: DepensesResumees {
            total: mois.expenses.total,
            previous: mois
                .expenses
                .previous
                .clone()
                .filter(|_| previous.is_some()),
            usual: mois.expenses.avg3.clone(),
            budget: mois.budget.as_ref().map(|b| b.total),
            budget_delta: mois.budget.as_ref().map(|b| b.delta),
        },
        savings: EpargneResumee {
            total: mois.savings.total,
            rate_bp: mois.savings.rate_bp,
            previous_rate_bp: precedente
                .filter(|_| previous.is_some())
                .and_then(|j| bp(calcul.epargne(j), calcul.revenus(j))),
        },
        unclassified: AClasser {
            amount: mois.unclassified.amount,
            operations: mois.unclassified.operations,
            share_bp: mois.unclassified.share_bp,
            distorting: mois
                .unclassified
                .share_bp
                .is_some_and(|b| b >= SEUIL_A_CLASSER_BP),
        },
        previous,
        previous_reason,
        usual,
        levers,
        levers_total,
    }
}
