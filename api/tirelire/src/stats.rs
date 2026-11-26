use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Days, Months, NaiveDate};
use serde::Serialize;
use serde_json::Value;

use crate::bilan::{Bilan, Ligne, Nature, bilan};

pub const SEUIL_PLAFOND: i64 = 100;
pub const SEUIL_MOYENNE: i64 = 1000;
pub const SEUIL_MOYENNE_BP: i64 = 1000;
pub const PROJECTION_JOURS_MIN: i64 = 5;
pub const HISTORIQUE: usize = 12;
pub const OPERATIONS_DU_DETAIL: usize = 5;
pub const COMMERCANTS_DU_DETAIL: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Calendaire,
    Releve,
}

impl Mode {
    pub fn lire(cle: Option<&str>) -> Option<Mode> {
        match cle {
            None | Some("calendar") => Some(Mode::Calendaire),
            Some("statement") => Some(Mode::Releve),
            Some(_) => None,
        }
    }

    pub fn cle(self) -> &'static str {
        match self {
            Mode::Calendaire => "calendar",
            Mode::Releve => "statement",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Releve {
    pub id: i64,
    pub debut: NaiveDate,
    pub fin: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct Operation {
    pub id: i64,
    pub releve: i64,
    pub jour: NaiveDate,
    pub achat: Option<NaiveDate>,
    pub categorie: Option<i64>,
    pub credit: bool,
    pub montant: i64,
    pub commercant: Option<i64>,
}

impl Operation {
    pub fn reference(&self) -> NaiveDate {
        self.achat.unwrap_or(self.jour)
    }
}

#[derive(Clone, Debug)]
pub struct Categorie {
    pub id: i64,
    pub nom: String,
    pub nature: Nature,
    pub plafond: Option<i64>,
    pub couleur: i32,
    pub archivee: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Donnees {
    pub releves: Vec<Releve>,
    pub operations: Vec<Operation>,
    pub categories: Vec<Categorie>,
    pub commercants: BTreeMap<i64, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Periode {
    pub id: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: i64,
    pub known_through: NaiveDate,
    pub elapsed_days: i64,
    pub complete: bool,
    pub open: bool,
    pub statement_ids: Vec<i64>,
    #[serde(skip)]
    debut_couvert: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ecart {
    pub amount: i64,
    pub delta: i64,
    pub delta_bp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Moyenne {
    pub amount: i64,
    pub periods: usize,
    pub delta: i64,
    pub delta_bp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Resume {
    #[serde(flatten)]
    pub period: Periode,
    pub operations: i64,
    pub expenses: i64,
    pub income: i64,
    pub savings: i64,
    pub remaining: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Depenses {
    pub total: i64,
    pub fixed: i64,
    pub variable: i64,
    pub unclassified: i64,
    pub fixed_bp: Option<i64>,
    pub variable_bp: Option<i64>,
    pub unclassified_bp: Option<i64>,
    pub previous: Option<Ecart>,
    pub previous_to_date: Option<Ecart>,
    pub avg3: Option<Moyenne>,
    pub avg12: Option<Moyenne>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Revenus {
    pub total: i64,
    pub classified: i64,
    pub unclassified: i64,
    pub unclassified_operations: i64,
    pub previous: Option<Ecart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Epargne {
    pub total: i64,
    pub rate_bp: Option<i64>,
    pub previous: Option<Ecart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reste {
    pub total: i64,
    pub previous: Option<Ecart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub total: i64,
    pub spent: i64,
    pub bp: Option<i64>,
    pub delta: i64,
    pub uncapped: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneCategorie {
    pub id: i64,
    pub name: String,
    pub kind: &'static str,
    pub color: i32,
    pub archived: bool,
    pub monthly_cap: Option<i64>,
    pub amount: i64,
    pub operations: i64,
    pub share_bp: Option<i64>,
    pub previous: Option<Ecart>,
    pub avg3: Option<Moyenne>,
    pub avg12: Option<Moyenne>,
    pub cap_delta: Option<i64>,
    pub cap_bp: Option<i64>,
    pub trend: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NonClasse {
    pub amount: i64,
    pub operations: i64,
    pub share_bp: Option<i64>,
    pub previous: Option<Ecart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Depassement {
    pub category_id: i64,
    pub reference: &'static str,
    pub reference_amount: i64,
    pub periods: Option<usize>,
    pub excess: i64,
    pub excess_bp: Option<i64>,
    pub rank_amount: usize,
    pub rank_percent: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Serie {
    pub id: String,
    pub values: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SerieMoyenne {
    pub periods: usize,
    pub values: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cumul {
    pub current: Vec<i64>,
    pub previous: Option<Serie>,
    pub average: Option<SerieMoyenne>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Projection {
    pub total: i64,
    pub variable: i64,
    pub fixed: i64,
    pub fixed_pending: i64,
    pub daily_variable: i64,
    pub remaining_days: i64,
    pub budget_delta: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Point {
    pub id: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub complete: bool,
    pub expenses: i64,
    pub income: i64,
    pub savings: i64,
    pub remaining: i64,
    pub savings_rate_bp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Mois {
    pub mode: &'static str,
    pub period: Periode,
    pub previous_id: Option<String>,
    pub next_id: Option<String>,
    pub periods: usize,
    pub operations: i64,
    pub expenses: Depenses,
    pub income: Revenus,
    pub savings: Epargne,
    pub remaining: Reste,
    pub budget: Option<Budget>,
    pub categories: Vec<LigneCategorie>,
    pub unclassified: NonClasse,
    pub overruns: Vec<Depassement>,
    pub overrun_total: i64,
    pub cumulative: Cumul,
    pub projection: Option<Projection>,
    pub projection_reason: Option<&'static str>,
    pub history: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PointCategorie {
    pub id: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub complete: bool,
    pub amount: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneCommercant {
    pub merchant_id: Option<i64>,
    pub name: Option<String>,
    pub amount: i64,
    pub operations: i64,
    pub average: i64,
    pub previous: Option<Ecart>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Detail {
    pub mode: &'static str,
    pub period: Periode,
    pub previous_id: Option<String>,
    pub next_id: Option<String>,
    pub periods: usize,
    pub category: LigneCategorie,
    pub history: Vec<PointCategorie>,
    pub over_cap: usize,
    pub merchants: Vec<LigneCommercant>,
    pub merchants_total: usize,
    pub operations: Vec<Value>,
    pub operations_total: usize,
    #[serde(skip)]
    pub dernieres: Vec<i64>,
}

pub fn division(a: i128, b: i128) -> i64 {
    let q = (2 * a.abs() + b) / (2 * b);
    i64::try_from(if a < 0 { -q } else { q }).unwrap_or(i64::MAX)
}

pub fn bp(part: i64, total: i64) -> Option<i64> {
    (total > 0).then(|| division(i128::from(part) * 10_000, i128::from(total)))
}

pub fn ecart(actuel: i64, reference: i64) -> Ecart {
    Ecart {
        amount: reference,
        delta: actuel - reference,
        delta_bp: bp(actuel - reference, reference),
    }
}

pub fn nature_cle(n: Nature) -> &'static str {
    match n {
        Nature::Fixe => "fixe",
        Nature::Variable => "variable",
        Nature::Epargne => "epargne",
        Nature::Revenu => "revenu",
        Nature::Neutre => "neutre",
    }
}

fn demain(j: NaiveDate) -> NaiveDate {
    j.checked_add_days(Days::new(1)).unwrap_or(j)
}

fn debut_de_mois(j: NaiveDate) -> NaiveDate {
    j.with_day(1).unwrap_or(j)
}

fn fin_de_mois(debut: NaiveDate) -> NaiveDate {
    debut
        .checked_add_months(Months::new(1))
        .and_then(|d| d.pred_opt())
        .unwrap_or(debut)
}

fn mois_suivant(debut: NaiveDate) -> NaiveDate {
    debut.checked_add_months(Months::new(1)).unwrap_or(debut)
}

pub fn couverture(releves: &[Releve]) -> Vec<(NaiveDate, NaiveDate)> {
    let mut plages: Vec<(NaiveDate, NaiveDate)> = releves
        .iter()
        .map(|r| (demain(r.debut).min(r.fin), r.fin))
        .collect();
    plages.sort();
    let mut fusion: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    for (du, au) in plages {
        match fusion.last_mut() {
            Some(derniere) if du <= demain(derniere.1) => derniere.1 = derniere.1.max(au),
            _ => fusion.push((du, au)),
        }
    }
    fusion
}

fn contenu(plages: &[(NaiveDate, NaiveDate)], du: NaiveDate, au: NaiveDate) -> bool {
    plages.iter().any(|(a, b)| *a <= du && au <= *b)
}

pub fn periodes(mode: Mode, d: &Donnees) -> Vec<Periode> {
    match mode {
        Mode::Calendaire => periodes_calendaires(d),
        Mode::Releve => periodes_par_releve(d),
    }
}

fn periodes_calendaires(d: &Donnees) -> Vec<Periode> {
    let plages = couverture(&d.releves);
    let Some(fin_donnees) = d.releves.iter().map(|r| r.fin).max() else {
        return Vec::new();
    };
    let mut mois: BTreeSet<NaiveDate> = d
        .operations
        .iter()
        .map(|o| debut_de_mois(o.reference()))
        .collect();
    for (du, au) in &plages {
        let mut m = debut_de_mois(*du);
        while m <= *au {
            mois.insert(m);
            m = mois_suivant(m);
        }
    }
    mois.into_iter()
        .map(|du| {
            let au = fin_de_mois(du);
            let open = du <= fin_donnees && fin_donnees < au;
            let connu = if open { fin_donnees } else { au };
            Periode {
                id: du.format("%Y-%m").to_string(),
                from: du,
                to: au,
                days: (au - du).num_days() + 1,
                known_through: connu,
                elapsed_days: (connu - du).num_days() + 1,
                complete: contenu(&plages, du, au),
                open,
                statement_ids: d
                    .releves
                    .iter()
                    .filter(|r| demain(r.debut).min(r.fin) <= au && du <= r.fin)
                    .map(|r| r.id)
                    .collect(),
                debut_couvert: contenu(&plages, du, connu),
            }
        })
        .collect()
}

fn periodes_par_releve(d: &Donnees) -> Vec<Periode> {
    let mut par_fin: BTreeMap<NaiveDate, (NaiveDate, Vec<i64>)> = BTreeMap::new();
    for r in &d.releves {
        let e = par_fin.entry(r.fin).or_insert((r.debut, Vec::new()));
        e.0 = e.0.min(r.debut);
        e.1.push(r.id);
    }
    par_fin
        .into_iter()
        .map(|(fin, (debut, mut ids))| {
            ids.sort_unstable();
            let jours = (fin - debut).num_days().max(1);
            Periode {
                id: fin.format("%Y-%m-%d").to_string(),
                from: debut,
                to: fin,
                days: jours,
                known_through: fin,
                elapsed_days: jours,
                complete: true,
                open: false,
                statement_ids: ids,
                debut_couvert: true,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
struct Agregat {
    bilan: Bilan,
    nombre: i64,
    par_categorie: BTreeMap<i64, i64>,
    debits_a_classer: i64,
    credits_a_classer: i64,
    par_jour: Vec<i64>,
    operations: Vec<usize>,
}

impl Agregat {
    fn depenses(&self) -> i64 {
        self.bilan.depenses() + self.bilan.a_classer_debits
    }

    fn revenus(&self) -> i64 {
        self.bilan.revenus + self.bilan.a_classer_credits
    }

    fn reste(&self) -> i64 {
        self.revenus() - self.depenses() - self.bilan.epargne
    }

    fn montant(&self, categorie: i64) -> i64 {
        self.bilan
            .par_categorie
            .get(&categorie)
            .copied()
            .unwrap_or(0)
    }

    fn cumul(&self) -> Vec<i64> {
        self.par_jour
            .iter()
            .scan(0, |total, j| {
                *total += j;
                Some(*total)
            })
            .collect()
    }
}

fn aligne(cumul: &[i64], jours: usize) -> Vec<i64> {
    let total = cumul.last().copied().unwrap_or(0);
    (1..=jours)
        .map(|j| {
            if j == jours || j > cumul.len() {
                total
            } else {
                cumul[j - 1]
            }
        })
        .collect()
}

pub struct Calcul<'a> {
    d: &'a Donnees,
    mode: Mode,
    periodes: Vec<Periode>,
    agregats: Vec<Agregat>,
    categories: BTreeMap<i64, &'a Categorie>,
}

impl<'a> Calcul<'a> {
    pub fn nouveau(d: &'a Donnees, mode: Mode) -> Self {
        let periodes = periodes(mode, d);
        let categories: BTreeMap<i64, &Categorie> =
            d.categories.iter().map(|c| (c.id, c)).collect();
        let mut agregats: Vec<Agregat> = periodes
            .iter()
            .map(|p| Agregat {
                par_jour: vec![0; usize::try_from(p.days).unwrap_or(1)],
                ..Agregat::default()
            })
            .collect();
        let par_releve: BTreeMap<i64, usize> = periodes
            .iter()
            .enumerate()
            .flat_map(|(i, p)| p.statement_ids.iter().map(move |r| (*r, i)))
            .collect();
        let mut lignes: Vec<Vec<Ligne>> = vec![Vec::new(); periodes.len()];
        for (rang, o) in d.operations.iter().enumerate() {
            let reference = o.reference();
            let place = match mode {
                Mode::Calendaire => periodes
                    .binary_search_by(|p| p.from.cmp(&debut_de_mois(reference)))
                    .ok()
                    .map(|i| (i, i64::from(reference.day0()))),
                Mode::Releve => par_releve.get(&o.releve).map(|i| {
                    let p = &periodes[*i];
                    (*i, (reference - p.from).num_days().clamp(1, p.days) - 1)
                }),
            };
            let Some((i, jour)) = place else { continue };
            let nature = o
                .categorie
                .and_then(|c| categories.get(&c))
                .map(|c| c.nature);
            let categorie = o.categorie.zip(nature);
            let a = &mut agregats[i];
            let sortie = if o.credit { -o.montant } else { o.montant };
            let depense = match nature {
                Some(Nature::Fixe | Nature::Variable) => sortie,
                None if !o.credit => o.montant,
                _ => 0,
            };
            if let Some(j) = a.par_jour.get_mut(usize::try_from(jour).unwrap_or(0)) {
                *j += depense;
            }
            a.nombre += 1;
            match categorie {
                Some((c, _)) => *a.par_categorie.entry(c).or_default() += 1,
                None if o.credit => a.credits_a_classer += 1,
                None => a.debits_a_classer += 1,
            }
            a.operations.push(rang);
            lignes[i].push(Ligne {
                categorie,
                credit: o.credit,
                montant: o.montant,
            });
        }
        for (a, l) in agregats.iter_mut().zip(&lignes) {
            a.bilan = bilan(l);
        }
        Self {
            d,
            mode,
            periodes,
            agregats,
            categories,
        }
    }

    pub fn periodes(&self) -> &[Periode] {
        &self.periodes
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn depenses(&self, i: usize) -> i64 {
        self.agregats[i].depenses()
    }

    pub fn a_classer(&self, i: usize) -> i64 {
        self.agregats[i].bilan.a_classer_debits
    }

    pub fn montant(&self, i: usize, categorie: i64) -> i64 {
        self.agregats[i].montant(categorie)
    }

    pub fn categories_de_depense(&self) -> impl Iterator<Item = &'a Categorie> {
        self.d
            .categories
            .iter()
            .filter(|c| matches!(c.nature, Nature::Fixe | Nature::Variable))
    }

    pub fn plafonds(&self) -> Option<i64> {
        let mut plafonds = self
            .categories_de_depense()
            .filter(|c| !c.archivee)
            .filter_map(|c| c.plafond)
            .peekable();
        plafonds.peek().is_some().then(|| plafonds.sum())
    }

    pub fn index(&self, cle: Option<&str>) -> Option<usize> {
        match cle {
            None => self.periodes.len().checked_sub(1),
            Some(cle) => self.periodes.iter().position(|p| p.id == cle),
        }
    }

    pub fn liste(&self) -> Vec<Resume> {
        self.periodes
            .iter()
            .zip(&self.agregats)
            .map(|(p, a)| Resume {
                period: p.clone(),
                operations: a.nombre,
                expenses: a.depenses(),
                income: a.revenus(),
                savings: a.bilan.epargne,
                remaining: a.reste(),
            })
            .collect()
    }

    fn precedente(&self, i: usize) -> Option<usize> {
        let j = i.checked_sub(1)?;
        match self.mode {
            Mode::Releve => Some(j),
            Mode::Calendaire => {
                (mois_suivant(self.periodes[j].from) == self.periodes[i].from).then_some(j)
            }
        }
    }

    fn completes_avant(&self, i: usize, n: usize) -> Vec<usize> {
        (0..i)
            .rev()
            .filter(|j| self.periodes[*j].complete)
            .take(n)
            .collect()
    }

    fn moyenne(&self, i: usize, n: usize, f: &dyn Fn(&Agregat) -> i64) -> Option<Moyenne> {
        let retenues = self.completes_avant(i, n);
        if retenues.is_empty() {
            return None;
        }
        let somme: i128 = retenues
            .iter()
            .map(|j| i128::from(f(&self.agregats[*j])))
            .sum();
        let reference = division(somme, retenues.len() as i128);
        let e = ecart(f(&self.agregats[i]), reference);
        Some(Moyenne {
            amount: reference,
            periods: retenues.len(),
            delta: e.delta,
            delta_bp: e.delta_bp,
        })
    }

    fn avant(&self, i: usize, f: &dyn Fn(&Agregat) -> i64) -> Option<Ecart> {
        self.precedente(i)
            .map(|j| ecart(f(&self.agregats[i]), f(&self.agregats[j])))
    }

    fn fenetre(&self, i: usize) -> std::ops::RangeInclusive<usize> {
        (i + 1).saturating_sub(HISTORIQUE)..=i
    }

    fn ligne(&self, i: usize, c: &Categorie) -> LigneCategorie {
        let a = &self.agregats[i];
        let montant = a.montant(c.id);
        let de = |a: &Agregat| a.montant(c.id);
        let depense = matches!(c.nature, Nature::Fixe | Nature::Variable);
        LigneCategorie {
            id: c.id,
            name: c.nom.clone(),
            kind: nature_cle(c.nature),
            color: c.couleur,
            archived: c.archivee,
            monthly_cap: c.plafond,
            amount: montant,
            operations: a.par_categorie.get(&c.id).copied().unwrap_or(0),
            share_bp: if depense {
                bp(montant, a.depenses())
            } else {
                None
            },
            previous: self.avant(i, &de),
            avg3: self.moyenne(i, 3, &de),
            avg12: self.moyenne(i, HISTORIQUE, &de),
            cap_delta: c.plafond.map(|p| montant - p),
            cap_bp: c.plafond.and_then(|p| bp(montant, p)),
            trend: self
                .fenetre(i)
                .map(|j| self.agregats[j].montant(c.id))
                .collect(),
        }
    }

    fn lignes(&self, i: usize) -> Vec<LigneCategorie> {
        let mut lignes: Vec<LigneCategorie> = self
            .d
            .categories
            .iter()
            .filter(|c| matches!(c.nature, Nature::Fixe | Nature::Variable))
            .map(|c| self.ligne(i, c))
            .filter(|l| {
                l.amount != 0
                    || l.operations > 0
                    || (!l.archived
                        && (l.monthly_cap.is_some()
                            || l.previous.as_ref().is_some_and(|p| p.amount != 0)
                            || l.avg3.as_ref().is_some_and(|m| m.amount != 0)))
            })
            .collect();
        lignes.sort_by(|x, y| y.amount.cmp(&x.amount).then_with(|| x.name.cmp(&y.name)));
        lignes
    }

    fn depassements(lignes: &[LigneCategorie]) -> Vec<Depassement> {
        let mut liste: Vec<(Depassement, &str)> = lignes
            .iter()
            .filter_map(|l| {
                let (reference, montant, periodes) = match (l.monthly_cap, &l.avg3) {
                    (Some(p), _) => ("cap", p, None),
                    (None, Some(m)) => ("average", m.amount, Some(m.periods)),
                    (None, None) => return None,
                };
                let exces = l.amount - montant;
                let exces_bp = bp(exces, montant);
                let signale = match reference {
                    "cap" => exces >= SEUIL_PLAFOND,
                    _ => exces >= SEUIL_MOYENNE && exces_bp.is_some_and(|b| b >= SEUIL_MOYENNE_BP),
                };
                signale.then_some((
                    Depassement {
                        category_id: l.id,
                        reference,
                        reference_amount: montant,
                        periods: periodes,
                        excess: exces,
                        excess_bp: exces_bp,
                        rank_amount: 0,
                        rank_percent: 0,
                    },
                    l.name.as_str(),
                ))
            })
            .collect();
        liste.sort_by(|(x, nx), (y, ny)| y.excess.cmp(&x.excess).then_with(|| nx.cmp(ny)));
        for (rang, (d, _)) in liste.iter_mut().enumerate() {
            d.rank_amount = rang + 1;
        }
        let mut ordre: Vec<usize> = (0..liste.len()).collect();
        ordre.sort_by(|x, y| {
            let (x, y) = (&liste[*x].0, &liste[*y].0);
            y.excess_bp
                .is_some()
                .cmp(&x.excess_bp.is_some())
                .then_with(|| y.excess_bp.cmp(&x.excess_bp))
                .then_with(|| x.rank_amount.cmp(&y.rank_amount))
        });
        for (rang, position) in ordre.into_iter().enumerate() {
            liste[position].0.rank_percent = rang + 1;
        }
        liste.into_iter().map(|(d, _)| d).collect()
    }

    fn budget(&self, i: usize) -> Option<Budget> {
        let a = &self.agregats[i];
        let plafonnees: Vec<&Categorie> = self
            .d
            .categories
            .iter()
            .filter(|c| {
                !c.archivee
                    && matches!(c.nature, Nature::Fixe | Nature::Variable)
                    && c.plafond.is_some()
            })
            .collect();
        if plafonnees.is_empty() {
            return None;
        }
        let total: i64 = plafonnees.iter().filter_map(|c| c.plafond).sum();
        let sous_plafond: i64 = plafonnees.iter().map(|c| a.montant(c.id)).sum();
        Some(Budget {
            total,
            spent: a.depenses(),
            bp: bp(a.depenses(), total),
            delta: a.depenses() - total,
            uncapped: a.depenses() - sous_plafond,
        })
    }

    fn projection(&self, i: usize, budget: Option<&Budget>) -> Result<Projection, &'static str> {
        let p = &self.periodes[i];
        if !p.open {
            return Err(if p.complete { "complete" } else { "partial" });
        }
        if !p.debut_couvert {
            return Err("partial");
        }
        if p.elapsed_days < PROJECTION_JOURS_MIN {
            return Err("too_early");
        }
        let a = &self.agregats[i];
        let variable_actuel = a.bilan.depenses_variables + a.bilan.a_classer_debits;
        let variable = division(
            i128::from(variable_actuel) * i128::from(p.days),
            i128::from(p.elapsed_days),
        );
        let mut fixe = 0;
        for c in self
            .d
            .categories
            .iter()
            .filter(|c| c.nature == Nature::Fixe)
        {
            let actuel = a.montant(c.id);
            let habituel = self
                .moyenne(i, 3, &|x: &Agregat| x.montant(c.id))
                .map_or(actuel, |m| m.amount);
            fixe += actuel.max(habituel);
        }
        let total = variable + fixe;
        Ok(Projection {
            total,
            variable,
            fixed: fixe,
            fixed_pending: fixe - a.bilan.depenses_fixes,
            daily_variable: division(i128::from(variable_actuel), i128::from(p.elapsed_days)),
            remaining_days: p.days - p.elapsed_days,
            budget_delta: budget.map(|b| total - b.total),
        })
    }

    fn cumul(&self, i: usize) -> Cumul {
        let p = &self.periodes[i];
        let jours = usize::try_from(p.days).unwrap_or(1);
        let mut actuel = self.agregats[i].cumul();
        if p.open {
            actuel.truncate(usize::try_from(p.elapsed_days).unwrap_or(jours));
        }
        let retenues = self.completes_avant(i, HISTORIQUE);
        let alignees: Vec<Vec<i64>> = retenues
            .iter()
            .map(|j| aligne(&self.agregats[*j].cumul(), jours))
            .collect();
        Cumul {
            current: actuel,
            previous: self.precedente(i).map(|j| Serie {
                id: self.periodes[j].id.clone(),
                values: aligne(&self.agregats[j].cumul(), jours),
            }),
            average: (!alignees.is_empty()).then(|| SerieMoyenne {
                periods: alignees.len(),
                values: (0..jours)
                    .map(|j| {
                        division(
                            alignees.iter().map(|s| i128::from(s[j])).sum(),
                            alignees.len() as i128,
                        )
                    })
                    .collect(),
            }),
        }
    }

    fn voisines(&self, i: usize) -> (Option<String>, Option<String>) {
        (
            i.checked_sub(1).map(|j| self.periodes[j].id.clone()),
            self.periodes.get(i + 1).map(|p| p.id.clone()),
        )
    }

    pub fn mois(&self, i: usize) -> Mois {
        let p = &self.periodes[i];
        let a = &self.agregats[i];
        let depenses = a.depenses();
        let lignes = self.lignes(i);
        let depassements = Self::depassements(&lignes);
        let budget = self.budget(i);
        let cumul = self.cumul(i);
        let (projection, raison) = match self.projection(i, budget.as_ref()) {
            Ok(p) => (Some(p), None),
            Err(r) => (None, Some(r)),
        };
        let jour = usize::try_from(p.elapsed_days).unwrap_or(1);
        let a_date = cumul
            .previous
            .as_ref()
            .filter(|_| p.open)
            .and_then(|s| s.values.get(jour.saturating_sub(1)))
            .map(|v| ecart(depenses, *v));
        let (previous_id, next_id) = self.voisines(i);
        Mois {
            mode: self.mode.cle(),
            period: p.clone(),
            previous_id,
            next_id,
            periods: self.periodes.len(),
            operations: a.nombre,
            expenses: Depenses {
                total: depenses,
                fixed: a.bilan.depenses_fixes,
                variable: a.bilan.depenses_variables,
                unclassified: a.bilan.a_classer_debits,
                fixed_bp: bp(a.bilan.depenses_fixes, depenses),
                variable_bp: bp(a.bilan.depenses_variables, depenses),
                unclassified_bp: bp(a.bilan.a_classer_debits, depenses),
                previous: self.avant(i, &Agregat::depenses),
                previous_to_date: a_date,
                avg3: self.moyenne(i, 3, &Agregat::depenses),
                avg12: self.moyenne(i, HISTORIQUE, &Agregat::depenses),
            },
            income: Revenus {
                total: a.revenus(),
                classified: a.bilan.revenus,
                unclassified: a.bilan.a_classer_credits,
                unclassified_operations: a.credits_a_classer,
                previous: self.avant(i, &Agregat::revenus),
            },
            savings: Epargne {
                total: a.bilan.epargne,
                rate_bp: bp(a.bilan.epargne, a.revenus()),
                previous: self.avant(i, &|x: &Agregat| x.bilan.epargne),
            },
            remaining: Reste {
                total: a.reste(),
                previous: self.avant(i, &Agregat::reste),
            },
            overrun_total: depassements.iter().map(|d| d.excess).sum(),
            overruns: depassements,
            unclassified: NonClasse {
                amount: a.bilan.a_classer_debits,
                operations: a.debits_a_classer,
                share_bp: bp(a.bilan.a_classer_debits, depenses),
                previous: self.avant(i, &|x: &Agregat| x.bilan.a_classer_debits),
            },
            categories: lignes,
            budget,
            cumulative: cumul,
            projection,
            projection_reason: raison,
            history: self
                .fenetre(i)
                .map(|j| {
                    let (p, a) = (&self.periodes[j], &self.agregats[j]);
                    Point {
                        id: p.id.clone(),
                        from: p.from,
                        to: p.to,
                        complete: p.complete,
                        expenses: a.depenses(),
                        income: a.revenus(),
                        savings: a.bilan.epargne,
                        remaining: a.reste(),
                        savings_rate_bp: bp(a.bilan.epargne, a.revenus()),
                    }
                })
                .collect(),
        }
    }

    fn par_commercant(&self, i: usize, categorie: &Categorie) -> BTreeMap<Option<i64>, (i64, i64)> {
        let mut groupes: BTreeMap<Option<i64>, (i64, i64)> = BTreeMap::new();
        for o in self.agregats[i]
            .operations
            .iter()
            .map(|rang| &self.d.operations[*rang])
            .filter(|o| o.categorie == Some(categorie.id))
        {
            let sortie = if o.credit { -o.montant } else { o.montant };
            let g = groupes.entry(o.commercant).or_default();
            g.0 += match categorie.nature {
                Nature::Fixe | Nature::Variable | Nature::Epargne => sortie,
                Nature::Revenu | Nature::Neutre => -sortie,
            };
            g.1 += 1;
        }
        groupes
    }

    pub fn detail(&self, i: usize, categorie: i64) -> Option<Detail> {
        let c = *self.categories.get(&categorie)?;
        let p = &self.periodes[i];
        let avant = self.precedente(i).map(|j| self.par_commercant(j, c));
        let mut commercants: Vec<LigneCommercant> = self
            .par_commercant(i, c)
            .into_iter()
            .map(|(id, (montant, nombre))| LigneCommercant {
                merchant_id: id,
                name: id.and_then(|id| self.d.commercants.get(&id).cloned()),
                amount: montant,
                operations: nombre,
                average: division(i128::from(montant), i128::from(nombre.max(1))),
                previous: avant
                    .as_ref()
                    .map(|a| ecart(montant, a.get(&id).map_or(0, |g| g.0))),
            })
            .collect();
        commercants.sort_by(|x, y| y.amount.cmp(&x.amount).then_with(|| x.name.cmp(&y.name)));
        let merchants_total = commercants.len();
        commercants.truncate(COMMERCANTS_DU_DETAIL);
        let mut operations: Vec<&Operation> = self.agregats[i]
            .operations
            .iter()
            .map(|rang| &self.d.operations[*rang])
            .filter(|o| o.categorie == Some(c.id))
            .collect();
        operations.sort_by(|x, y| {
            y.reference()
                .cmp(&x.reference())
                .then_with(|| y.id.cmp(&x.id))
        });
        let history: Vec<PointCategorie> = self
            .fenetre(i)
            .map(|j| PointCategorie {
                id: self.periodes[j].id.clone(),
                from: self.periodes[j].from,
                to: self.periodes[j].to,
                complete: self.periodes[j].complete,
                amount: self.agregats[j].montant(c.id),
            })
            .collect();
        let (previous_id, next_id) = self.voisines(i);
        Some(Detail {
            mode: self.mode.cle(),
            period: p.clone(),
            previous_id,
            next_id,
            periods: self.periodes.len(),
            category: self.ligne(i, c),
            over_cap: c
                .plafond
                .map_or(0, |p| history.iter().filter(|h| h.amount > p).count()),
            history,
            merchants: commercants,
            merchants_total,
            operations: Vec::new(),
            operations_total: operations.len(),
            dernieres: operations
                .iter()
                .take(OPERATIONS_DU_DETAIL)
                .map(|o| o.id)
                .collect(),
        })
    }
}
