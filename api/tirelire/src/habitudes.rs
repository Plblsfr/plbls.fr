use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Days, NaiveDate};
use serde::Serialize;

use crate::{
    bilan::Nature,
    evolution::Fenetre,
    stats::{Calcul, Ecart, Mode, Operation, couverture, division, ecart},
};

pub const FENETRE_DEFAUT: usize = 3;
pub const FENETRES: [usize; 4] = [1, 3, 6, 12];
pub const COMMERCANTS_DU_CLASSEMENT: usize = 10;
pub const SEUIL_PETIT_ACHAT: i64 = 1500;
pub const PETITS_ACHATS_REPETES: i64 = 3;
pub const COMMERCANTS_DES_PETITS_ACHATS: usize = 5;
pub const ANNUALISATION_JOURS_MIN: i64 = 28;
pub const INHABITUEL_COMMERCANT_MIN: usize = 4;
pub const INHABITUEL_CATEGORIE_MIN: usize = 8;
pub const INHABITUEL_ECARTS_X10: i128 = 35;
pub const INHABITUEL_RATIO_BP: i128 = 15_000;
pub const PLANCHER_COMMERCANT: i64 = 2000;
pub const PLANCHER_CATEGORIE: i64 = 5000;
pub const INHABITUELLES_MAX: usize = 20;
const ECHELLE_MAD_X10000: i128 = 14_826;

pub fn fenetre(cle: Option<&str>) -> Option<Fenetre> {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Plage {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub periods: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneCommercant {
    pub merchant_id: i64,
    pub name: Option<String>,
    pub category_id: Option<i64>,
    pub amount: i64,
    pub count: i64,
    pub average: Option<i64>,
    pub previous: Option<Ecart>,
    pub rank_amount: usize,
    pub rank_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PetitCommercant {
    pub merchant_id: i64,
    pub name: Option<String>,
    pub count: i64,
    pub amount: i64,
    pub yearly: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PetitsAchats {
    pub threshold: i64,
    pub min_repeats: i64,
    pub count: i64,
    pub amount: i64,
    pub yearly: Option<i64>,
    pub covered_days: i64,
    pub min_days: i64,
    pub merchants: Vec<PetitCommercant>,
    pub merchants_total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Tranche {
    pub rank: u32,
    pub from_day: Option<u32>,
    pub to_day: Option<u32>,
    pub amount: i64,
    pub count: i64,
    pub days: i64,
    pub per_day: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Repartition {
    pub amount: i64,
    pub count: i64,
    pub buckets: Vec<Tranche>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Raison {
    pub basis: &'static str,
    pub median: i64,
    pub history: usize,
    pub ratio_x10: Option<i64>,
    pub excess: i64,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Inhabituelle {
    pub id: i64,
    pub date: NaiveDate,
    pub amount: i64,
    pub merchant_id: Option<i64>,
    pub merchant_name: Option<String>,
    pub category_id: Option<i64>,
    pub reason: Raison,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Inhabituelles {
    pub total: usize,
    pub operations: Vec<Inhabituelle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Habitudes {
    pub mode: &'static str,
    pub window: String,
    pub available: usize,
    pub current: Option<Plage>,
    pub previous: Option<Plage>,
    pub known_through: Option<NaiveDate>,
    pub covered_days: i64,
    pub operations_from: Option<NaiveDate>,
    pub operations_to: Option<NaiveDate>,
    pub variable: i64,
    pub merchants: Vec<LigneCommercant>,
    pub merchants_total: usize,
    pub small: PetitsAchats,
    pub weekdays: Repartition,
    pub weeks: Repartition,
    pub unusual: Inhabituelles,
}

fn mediane(tries: &[i64]) -> i64 {
    let n = tries.len();
    if n == 0 {
        return 0;
    }
    if n % 2 == 1 {
        tries[n / 2]
    } else {
        division(i128::from(tries[n / 2 - 1]) + i128::from(tries[n / 2]), 2)
    }
}

pub fn mediane_et_ecart(valeurs: &[i64]) -> (i64, i64) {
    let mut tries = valeurs.to_vec();
    tries.sort_unstable();
    let m = mediane(&tries);
    let mut ecarts: Vec<i64> = tries.iter().map(|v| (v - m).abs()).collect();
    ecarts.sort_unstable();
    (m, mediane(&ecarts))
}

pub fn hors_habitude(montant: i64, autres: &[i64], plancher: i64) -> Option<(i64, i64)> {
    let (m, mad) = mediane_et_ecart(autres);
    let exces = montant - m;
    let loin =
        i128::from(exces) * 100_000 > INHABITUEL_ECARTS_X10 * ECHELLE_MAD_X10000 * i128::from(mad);
    let ratio = i128::from(montant) * 10_000 >= INHABITUEL_RATIO_BP * i128::from(m);
    (exces >= plancher && loin && ratio).then_some((m, exces))
}

pub fn variable(calcul: &Calcul, o: &Operation) -> bool {
    match calcul.nature(o) {
        Some(Nature::Variable) => true,
        None => !o.credit,
        _ => false,
    }
}

pub fn inhabituelles(calcul: &Calcul) -> BTreeMap<i64, Raison> {
    let d = calcul.donnees();
    let juges: Vec<&Operation> = d
        .operations
        .iter()
        .filter(|o| !o.credit && variable(calcul, o))
        .collect();
    let mut par_commercant: BTreeMap<i64, Vec<&Operation>> = BTreeMap::new();
    let mut par_categorie: BTreeMap<i64, Vec<&Operation>> = BTreeMap::new();
    for o in &juges {
        if let Some(c) = o.commercant {
            par_commercant.entry(c).or_default().push(o);
        }
        if let Some(c) = o.categorie {
            par_categorie.entry(c).or_default().push(o);
        }
    }
    let autres = |groupe: Option<&Vec<&Operation>>, o: &Operation| -> Vec<i64> {
        groupe
            .into_iter()
            .flatten()
            .filter(|a| a.id != o.id)
            .map(|a| a.montant)
            .collect()
    };
    let mut res = BTreeMap::new();
    for o in &juges {
        let chez = autres(o.commercant.and_then(|c| par_commercant.get(&c)), o);
        let (basis, historique, plancher, name) = if chez.len() >= INHABITUEL_COMMERCANT_MIN {
            (
                "merchant",
                chez,
                PLANCHER_COMMERCANT,
                o.commercant.and_then(|c| d.commercants.get(&c).cloned()),
            )
        } else {
            let dans = autres(o.categorie.and_then(|c| par_categorie.get(&c)), o);
            if dans.len() < INHABITUEL_CATEGORIE_MIN {
                continue;
            }
            (
                "category",
                dans,
                PLANCHER_CATEGORIE,
                o.categorie
                    .and_then(|c| calcul.categorie(c))
                    .map(|c| c.nom.clone()),
            )
        };
        if let Some((m, exces)) = hors_habitude(o.montant, &historique, plancher) {
            res.insert(
                o.id,
                Raison {
                    basis,
                    median: m,
                    history: historique.len(),
                    ratio_x10: (m > 0).then(|| division(i128::from(o.montant) * 10, i128::from(m))),
                    excess: exces,
                    name,
                },
            );
        }
    }
    res
}

fn jours_couverts(calcul: &Calcul, plage: std::ops::Range<usize>) -> Vec<NaiveDate> {
    let plages = couverture(&calcul.donnees().releves);
    let mut jours = BTreeSet::new();
    for p in &calcul.periodes()[plage] {
        let (du, au) = match calcul.mode() {
            Mode::Calendaire => (p.from, p.known_through),
            Mode::Releve => (
                p.from.checked_add_days(Days::new(1)).unwrap_or(p.from),
                p.to,
            ),
        };
        let mut j = du;
        while j <= au {
            if plages.iter().any(|(a, b)| *a <= j && j <= *b) {
                jours.insert(j);
            }
            let Some(suivant) = j.succ_opt() else { break };
            j = suivant;
        }
    }
    jours.into_iter().collect()
}

fn annualise(montant: i64, jours: i64) -> Option<i64> {
    (jours >= ANNUALISATION_JOURS_MIN)
        .then(|| division(i128::from(montant) * 365, i128::from(jours)))
}

pub fn par_commercant(calcul: &Calcul, plage: std::ops::Range<usize>) -> BTreeMap<i64, (i64, i64)> {
    let mut res: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for i in plage {
        for o in calcul.operations(i).filter(|o| variable(calcul, o)) {
            let Some(c) = o.commercant else { continue };
            let e = res.entry(c).or_default();
            e.0 += o.depense(calcul.nature(o));
            e.1 += i64::from(!o.credit);
        }
    }
    res
}

fn plage_de(calcul: &Calcul, plage: &std::ops::Range<usize>) -> Option<Plage> {
    let p = calcul.periodes();
    (!plage.is_empty()).then(|| Plage {
        from: p[plage.start].from,
        to: p[plage.end - 1].to,
        periods: plage.len(),
    })
}

fn semaine(jour: NaiveDate) -> usize {
    (jour.day0() / 7).min(4) as usize
}

fn repartition(tranches: Vec<Tranche>) -> Repartition {
    Repartition {
        amount: tranches.iter().map(|t| t.amount).sum(),
        count: tranches.iter().map(|t| t.count).sum(),
        buckets: tranches
            .into_iter()
            .map(|t| Tranche {
                per_day: (t.days > 0).then(|| division(i128::from(t.amount), i128::from(t.days))),
                ..t
            })
            .collect(),
    }
}

fn petits_achats(calcul: &Calcul, operations: &[&Operation], couverts: i64) -> PetitsAchats {
    let d = calcul.donnees();
    let mut petits: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for o in operations
        .iter()
        .filter(|o| !o.credit && o.montant < SEUIL_PETIT_ACHAT)
    {
        if let Some(c) = o.commercant {
            let e = petits.entry(c).or_default();
            e.0 += 1;
            e.1 += o.montant;
        }
    }
    let mut repetes: Vec<PetitCommercant> = petits
        .into_iter()
        .filter(|(_, (nombre, _))| *nombre >= PETITS_ACHATS_REPETES)
        .map(|(id, (nombre, montant))| PetitCommercant {
            merchant_id: id,
            name: d.commercants.get(&id).cloned(),
            count: nombre,
            amount: montant,
            yearly: annualise(montant, couverts),
        })
        .collect();
    repetes.sort_by(|x, y| {
        y.amount
            .cmp(&x.amount)
            .then_with(|| x.merchant_id.cmp(&y.merchant_id))
    });
    let petit_total: i64 = repetes.iter().map(|p| p.amount).sum();
    PetitsAchats {
        threshold: SEUIL_PETIT_ACHAT,
        min_repeats: PETITS_ACHATS_REPETES,
        count: repetes.iter().map(|p| p.count).sum(),
        amount: petit_total,
        yearly: annualise(petit_total, couverts),
        covered_days: couverts,
        min_days: ANNUALISATION_JOURS_MIN,
        merchants_total: repetes.len(),
        merchants: repetes
            .into_iter()
            .take(COMMERCANTS_DES_PETITS_ACHATS)
            .collect(),
    }
}

pub fn petits_achats_sur(calcul: &Calcul, plage: std::ops::Range<usize>) -> PetitsAchats {
    let couverts = jours_couverts(calcul, plage.clone()).len() as i64;
    let operations: Vec<&Operation> = plage
        .flat_map(|i| calcul.operations(i))
        .filter(|o| variable(calcul, o))
        .collect();
    petits_achats(calcul, &operations, couverts)
}

pub fn habitudes(calcul: &Calcul, fenetre: Fenetre) -> Habitudes {
    let d = calcul.donnees();
    let periodes = calcul.periodes();
    let plage = fenetre.plage(periodes.len());
    let avant = match fenetre {
        Fenetre::Periodes(n) if !plage.is_empty() && plage.start >= n => {
            Some(plage.start - n..plage.start)
        }
        _ => None,
    };
    let jours = jours_couverts(calcul, plage.clone());
    let couverts = jours.len() as i64;
    let operations: Vec<&Operation> = plage
        .clone()
        .flat_map(|i| calcul.operations(i))
        .filter(|o| variable(calcul, o))
        .collect();

    let actuels = par_commercant(calcul, plage.clone());
    let precedents = avant.clone().map(|a| par_commercant(calcul, a));
    let mut derniere: BTreeMap<i64, (NaiveDate, i64, Option<i64>)> = BTreeMap::new();
    for o in &operations {
        if let Some(c) = o.commercant {
            let cle = (o.reference(), o.id, o.categorie);
            let e = derniere.entry(c).or_insert(cle);
            if (cle.0, cle.1) > (e.0, e.1) {
                *e = cle;
            }
        }
    }
    let mut commercants: Vec<LigneCommercant> = actuels
        .iter()
        .filter(|(_, (_, nombre))| *nombre > 0)
        .map(|(id, (montant, nombre))| LigneCommercant {
            merchant_id: *id,
            name: d.commercants.get(id).cloned(),
            category_id: derniere.get(id).and_then(|x| x.2),
            amount: *montant,
            count: *nombre,
            average: Some(division(i128::from(*montant), i128::from(*nombre))),
            previous: precedents
                .as_ref()
                .map(|p| ecart(*montant, p.get(id).map_or(0, |x| x.0))),
            rank_amount: 0,
            rank_count: 0,
        })
        .collect();
    commercants.sort_by(|x, y| {
        y.count
            .cmp(&x.count)
            .then_with(|| y.amount.cmp(&x.amount))
            .then_with(|| x.merchant_id.cmp(&y.merchant_id))
    });
    for (rang, c) in commercants.iter_mut().enumerate() {
        c.rank_count = rang + 1;
    }
    commercants.sort_by(|x, y| {
        y.amount
            .cmp(&x.amount)
            .then_with(|| y.count.cmp(&x.count))
            .then_with(|| x.merchant_id.cmp(&y.merchant_id))
    });
    for (rang, c) in commercants.iter_mut().enumerate() {
        c.rank_amount = rang + 1;
    }
    let merchants_total = commercants.len();
    commercants.retain(|c| {
        c.rank_amount <= COMMERCANTS_DU_CLASSEMENT || c.rank_count <= COMMERCANTS_DU_CLASSEMENT
    });

    let small = petits_achats(calcul, &operations, couverts);

    let mut semaine_du_jour: Vec<Tranche> = (1..=7)
        .map(|rank| Tranche {
            rank,
            from_day: None,
            to_day: None,
            amount: 0,
            count: 0,
            days: 0,
            per_day: None,
        })
        .collect();
    let mut semaines: Vec<Tranche> = (0..5u32)
        .map(|k| Tranche {
            rank: k + 1,
            from_day: Some(7 * k + 1),
            to_day: Some(if k == 4 { 31 } else { 7 * k + 7 }),
            amount: 0,
            count: 0,
            days: 0,
            per_day: None,
        })
        .collect();
    for j in &jours {
        semaine_du_jour[j.weekday().num_days_from_monday() as usize].days += 1;
        semaines[semaine(*j)].days += 1;
    }
    for o in &operations {
        let depense = o.depense(calcul.nature(o));
        let quand = o.reference();
        let s = &mut semaines[semaine(quand)];
        s.amount += depense;
        s.count += i64::from(!o.credit);
        if o.achat.is_some() {
            let t = &mut semaine_du_jour[quand.weekday().num_days_from_monday() as usize];
            t.amount += depense;
            t.count += i64::from(!o.credit);
        }
    }

    let raisons = inhabituelles(calcul);
    let mut hors: Vec<Inhabituelle> = operations
        .iter()
        .filter_map(|o| {
            raisons.get(&o.id).map(|r| Inhabituelle {
                id: o.id,
                date: o.reference(),
                amount: o.montant,
                merchant_id: o.commercant,
                merchant_name: o.commercant.and_then(|c| d.commercants.get(&c).cloned()),
                category_id: o.categorie,
                reason: r.clone(),
            })
        })
        .collect();
    hors.sort_by(|x, y| y.date.cmp(&x.date).then_with(|| y.id.cmp(&x.id)));
    let unusual = Inhabituelles {
        total: hors.len(),
        operations: hors.into_iter().take(INHABITUELLES_MAX).collect(),
    };

    let current = plage_de(calcul, &plage);
    let fin = plage.clone().last().map(|i| periodes[i].known_through);
    Habitudes {
        mode: calcul.mode().cle(),
        window: fenetre.cle(),
        available: periodes.len(),
        previous: avant.and_then(|a| plage_de(calcul, &a)),
        known_through: fin,
        covered_days: couverts,
        operations_from: current.as_ref().map(|c| match calcul.mode() {
            Mode::Calendaire => c.from,
            Mode::Releve => c.from.checked_add_days(Days::new(1)).unwrap_or(c.from),
        }),
        operations_to: fin,
        current,
        variable: operations.iter().map(|o| o.depense(calcul.nature(o))).sum(),
        merchants: commercants,
        merchants_total,
        small,
        weekdays: repartition(semaine_du_jour),
        weeks: repartition(semaines),
        unusual,
    }
}
