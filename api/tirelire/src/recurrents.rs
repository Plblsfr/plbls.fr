use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, Months, NaiveDate};
use serde::Serialize;

use crate::{
    bilan::Nature,
    stats::{Donnees, Operation, bp, couverture, division},
};

pub const TOLERANCE_MONTANT_BP: i64 = 500;
pub const TOLERANCE_MONTANT_MIN: i64 = 100;
pub const CHANGEMENT_FACTEUR_MAX: i64 = 2;
pub const NOUVEAUTE_JOURS: i64 = 120;
pub const ARRET_VISIBLE_JOURS: i64 = 365;
pub const CLE_ABONNEMENTS: &str = "abonnements";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cadence {
    Hebdomadaire,
    Mensuelle,
    Trimestrielle,
    Annuelle,
}

impl Cadence {
    pub const TOUTES: [Cadence; 4] = [
        Cadence::Hebdomadaire,
        Cadence::Mensuelle,
        Cadence::Trimestrielle,
        Cadence::Annuelle,
    ];

    pub fn cle(self) -> &'static str {
        match self {
            Cadence::Hebdomadaire => "weekly",
            Cadence::Mensuelle => "monthly",
            Cadence::Trimestrielle => "quarterly",
            Cadence::Annuelle => "yearly",
        }
    }

    pub fn tolerance(self) -> i64 {
        match self {
            Cadence::Hebdomadaire => 2,
            Cadence::Mensuelle => 5,
            Cadence::Trimestrielle => 7,
            Cadence::Annuelle => 15,
        }
    }

    pub fn minimum(self, preleve: bool) -> usize {
        match (self, preleve) {
            (Cadence::Hebdomadaire, _) => 4,
            (Cadence::Mensuelle | Cadence::Trimestrielle, true) => 2,
            (Cadence::Mensuelle | Cadence::Trimestrielle, false) => 3,
            (Cadence::Annuelle, _) => 2,
        }
    }

    pub fn par_an(self) -> i64 {
        match self {
            Cadence::Hebdomadaire => 52,
            Cadence::Mensuelle => 12,
            Cadence::Trimestrielle => 4,
            Cadence::Annuelle => 1,
        }
    }

    pub fn arret(self) -> usize {
        match self {
            Cadence::Hebdomadaire | Cadence::Mensuelle => 2,
            Cadence::Trimestrielle | Cadence::Annuelle => 1,
        }
    }

    pub fn jours_detectables(self) -> i64 {
        match self {
            Cadence::Hebdomadaire => 21,
            Cadence::Mensuelle => 35,
            Cadence::Trimestrielle => 95,
            Cadence::Annuelle => 370,
        }
    }

    fn mois(self) -> u32 {
        match self {
            Cadence::Hebdomadaire => 0,
            Cadence::Mensuelle => 1,
            Cadence::Trimestrielle => 3,
            Cadence::Annuelle => 12,
        }
    }

    fn pas_x100(self) -> i64 {
        match self {
            Cadence::Hebdomadaire => 700,
            Cadence::Mensuelle => 3044,
            Cadence::Trimestrielle => 9131,
            Cadence::Annuelle => 36_525,
        }
    }

    pub fn echeance(self, ancre: NaiveDate, rang: i64) -> Option<NaiveDate> {
        let n = rang.unsigned_abs();
        match (self, rang >= 0) {
            (Cadence::Hebdomadaire, true) => ancre.checked_add_days(Days::new(7 * n)),
            (Cadence::Hebdomadaire, false) => ancre.checked_sub_days(Days::new(7 * n)),
            (_, true) => {
                ancre.checked_add_months(Months::new(u32::try_from(n).ok()? * self.mois()))
            }
            (_, false) => {
                ancre.checked_sub_months(Months::new(u32::try_from(n).ok()? * self.mois()))
            }
        }
    }

    pub fn rang(self, ancre: NaiveDate, jour: NaiveDate) -> Option<i64> {
        let ecart = (jour - ancre).num_days();
        let proche = division(i128::from(ecart) * 100, i128::from(self.pas_x100()));
        (proche - 1..=proche + 1)
            .filter_map(|k| {
                let distance = (jour - self.echeance(ancre, k)?).num_days().abs();
                (distance <= self.tolerance()).then_some((distance, k))
            })
            .min()
            .map(|(_, k)| k)
    }
}

pub fn proche(a: i64, b: i64) -> bool {
    let marge = (a.max(b) * TOLERANCE_MONTANT_BP / 10_000).max(TOLERANCE_MONTANT_MIN);
    (a - b).abs() <= marge
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub id: i64,
    pub jour: NaiveDate,
    pub montant: i64,
    pub rang: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Serie {
    pub commercant: i64,
    pub cadence: Cadence,
    pub ancre: NaiveDate,
    pub occurrences: Vec<Occurrence>,
    pub categorie: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Etat {
    pub ecarte: bool,
    pub nouveau_vu: bool,
    pub montant_vu: Option<i64>,
}

pub type Etats = BTreeMap<(i64, String), Etat>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Alerte {
    pub kind: &'static str,
    pub seen: bool,
    pub date: NaiveDate,
    pub operation_id: i64,
    pub first_date: Option<NaiveDate>,
    pub old_amount: Option<i64>,
    pub new_amount: i64,
    pub delta: Option<i64>,
    pub delta_bp: Option<i64>,
    pub yearly_delta: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recurrent {
    pub id: String,
    pub merchant_id: i64,
    pub name: Option<String>,
    pub category_id: Option<i64>,
    pub group: &'static str,
    pub cadence: &'static str,
    pub amount: i64,
    pub occurrences: usize,
    pub since: NaiveDate,
    pub last_date: NaiveDate,
    pub last_operation_id: i64,
    pub next_date: Option<NaiveDate>,
    pub missed: usize,
    pub status: &'static str,
    pub monthly: i64,
    pub yearly: i64,
    pub dismissed: bool,
    pub alerts: Vec<Alerte>,
    #[serde(skip)]
    pub commercant: i64,
    #[serde(skip)]
    pub cle: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Totaux {
    pub count: usize,
    pub monthly: i64,
    pub yearly: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recurrents {
    pub as_of: Option<NaiveDate>,
    pub from: Option<NaiveDate>,
    pub span_days: i64,
    pub statements: usize,
    pub detectable: Vec<&'static str>,
    pub items: Vec<Recurrent>,
    pub alerts_unseen: usize,
    pub total: Totaux,
    pub subscriptions: Totaux,
    pub charges: Totaux,
}

struct Contexte {
    plages: Vec<(NaiveDate, NaiveDate)>,
    debut: NaiveDate,
    fin: NaiveDate,
}

impl Contexte {
    fn couvert(&self, jour: NaiveDate) -> bool {
        self.plages.iter().any(|(a, b)| *a <= jour && jour <= *b)
    }
}

fn occurrence(o: &Operation, rang: i64) -> Occurrence {
    Occurrence {
        id: o.id,
        jour: o.reference(),
        montant: o.montant,
        rang,
    }
}

struct Suite {
    ancre: NaiveDate,
    occurrences: Vec<Occurrence>,
    anomalies: usize,
    consecutives: bool,
    preleve: bool,
}

impl Suite {
    fn nouvelle(o: &Operation) -> Self {
        Self {
            ancre: o.reference(),
            occurrences: vec![occurrence(o, 0)],
            anomalies: 0,
            consecutives: false,
            preleve: o.preleve,
        }
    }

    fn valide(&self, cadence: Cadence) -> bool {
        let n = self.occurrences.len();
        self.consecutives && n >= cadence.minimum(self.preleve) && self.anomalies <= (n - 1) / 4
    }
}

fn noyaux(operations: &[&Operation], cadence: Cadence, contexte: &Contexte) -> Vec<Suite> {
    let mut suites: Vec<Suite> = Vec::new();
    for o in operations {
        let jour = o.reference();
        let Some(suite) = suites.last_mut() else {
            suites.push(Suite::nouvelle(o));
            continue;
        };
        let avant = suite.occurrences.last().map_or(0, |x| x.rang);
        let mut manquees = 0;
        let mut rang = avant + 1;
        while let Some(attendue) = cadence.echeance(suite.ancre, rang) {
            if (jour - attendue).num_days() <= cadence.tolerance() {
                break;
            }
            manquees += usize::from(contexte.couvert(attendue));
            rang += 1;
        }
        if manquees >= cadence.arret() {
            suites.push(Suite::nouvelle(o));
            continue;
        }
        match cadence.rang(suite.ancre, jour).filter(|k| *k > avant) {
            Some(rang) => {
                suite.anomalies += manquees;
                suite.consecutives |= rang == avant + 1;
                suite.preleve &= o.preleve;
                suite.occurrences.push(occurrence(o, rang));
            }
            None => suite.anomalies += 1,
        }
    }
    suites.retain(|s| s.valide(cadence));
    suites
}

fn groupes_de_montant<'a>(operations: &[&'a Operation]) -> Vec<Vec<&'a Operation>> {
    let mut tries = operations.to_vec();
    tries.sort_by_key(|o| (o.montant, o.reference(), o.id));
    let mut groupes: Vec<Vec<&Operation>> = Vec::new();
    for o in tries {
        match groupes.last_mut() {
            Some(g) if proche(g[0].montant, o.montant) => g.push(o),
            _ => groupes.push(vec![o]),
        }
    }
    for g in &mut groupes {
        g.sort_by_key(|o| (o.reference(), o.id));
    }
    groupes
}

fn prolonger(
    serie: &mut Serie,
    operations: &[&Operation],
    prises: &mut BTreeSet<i64>,
    contexte: &Contexte,
    sens: i64,
) {
    let tolerance = serie.cadence.tolerance();
    loop {
        let Some(bord) = (if sens > 0 {
            serie.occurrences.last()
        } else {
            serie.occurrences.first()
        })
        .cloned() else {
            return;
        };
        let mut rang = bord.rang + sens;
        let mut manquees = 0;
        let trouvee = loop {
            let Some(attendue) = serie.cadence.echeance(serie.ancre, rang) else {
                return;
            };
            let dehors = if sens > 0 {
                (attendue - contexte.fin).num_days() > tolerance
            } else {
                (contexte.debut - attendue).num_days() > tolerance
            };
            if dehors {
                return;
            }
            let candidate = operations
                .iter()
                .filter(|o| !prises.contains(&o.id))
                .filter(|o| (o.reference() - attendue).num_days().abs() <= tolerance)
                .filter(|o| {
                    if sens > 0 {
                        o.reference() > bord.jour
                    } else {
                        o.reference() < bord.jour
                    }
                })
                .filter(|o| {
                    o.montant * CHANGEMENT_FACTEUR_MAX >= bord.montant
                        && o.montant <= bord.montant * CHANGEMENT_FACTEUR_MAX
                })
                .min_by_key(|o| {
                    (
                        (o.montant - bord.montant).abs(),
                        (o.reference() - attendue).num_days().abs(),
                        o.id,
                    )
                });
            match candidate {
                Some(o) => break (rang, *o),
                None => {
                    manquees += usize::from(contexte.couvert(attendue));
                    if manquees >= serie.cadence.arret() {
                        return;
                    }
                    rang += sens;
                }
            }
        };
        let (rang, o) = trouvee;
        prises.insert(o.id);
        let occurrence = Occurrence {
            id: o.id,
            jour: o.reference(),
            montant: o.montant,
            rang,
        };
        if sens > 0 {
            serie.occurrences.push(occurrence);
        } else {
            serie.occurrences.insert(0, occurrence);
        }
    }
}

fn eligibles(d: &Donnees) -> BTreeMap<i64, Vec<&Operation>> {
    let natures: BTreeMap<i64, Nature> = d.categories.iter().map(|c| (c.id, c.nature)).collect();
    let mut res: BTreeMap<i64, Vec<&Operation>> = BTreeMap::new();
    for o in &d.operations {
        let nature = o.categorie.and_then(|c| natures.get(&c));
        let exclue = matches!(
            nature,
            Some(Nature::Epargne | Nature::Revenu | Nature::Neutre)
        );
        if let (false, false, Some(c)) = (o.credit, exclue, o.commercant) {
            res.entry(c).or_default().push(o);
        }
    }
    res
}

fn contexte(d: &Donnees) -> Option<Contexte> {
    let plages = couverture(&d.releves);
    Some(Contexte {
        debut: plages.first()?.0,
        fin: plages.last()?.1,
        plages,
    })
}

pub fn detecter(d: &Donnees) -> Vec<Serie> {
    let Some(contexte) = contexte(d) else {
        return Vec::new();
    };
    let mut series = Vec::new();
    for (commercant, operations) in eligibles(d) {
        let mut trouves: Vec<(Cadence, Suite, Option<i64>)> = Vec::new();
        for groupe in groupes_de_montant(&operations) {
            let categorie = groupe.last().and_then(|o| o.categorie);
            let meilleure = Cadence::TOUTES
                .into_iter()
                .map(|c| (c, noyaux(&groupe, c, &contexte)))
                .max_by_key(|(c, suites)| {
                    (
                        suites.iter().map(|s| s.occurrences.len()).sum::<usize>(),
                        std::cmp::Reverse(*c),
                    )
                });
            if let Some((cadence, suites)) = meilleure {
                trouves.extend(suites.into_iter().map(|s| (cadence, s, categorie)));
            }
        }
        trouves.sort_by_key(|(_, s, _)| (s.occurrences[0].jour, s.occurrences[0].id));
        let mut prises: BTreeSet<i64> = BTreeSet::new();
        for (cadence, suite, categorie) in trouves {
            if suite.occurrences.iter().any(|o| prises.contains(&o.id)) {
                continue;
            }
            prises.extend(suite.occurrences.iter().map(|o| o.id));
            let mut serie = Serie {
                commercant,
                cadence,
                ancre: suite.ancre,
                occurrences: suite.occurrences,
                categorie,
            };
            prolonger(&mut serie, &operations, &mut prises, &contexte, 1);
            prolonger(&mut serie, &operations, &mut prises, &contexte, -1);
            if let Some(derniere) = serie.occurrences.last() {
                serie.categorie = operations
                    .iter()
                    .find(|o| o.id == derniere.id)
                    .and_then(|o| o.categorie);
            }
            series.push(serie);
        }
    }
    series
}

fn groupe(d: &Donnees, categorie: Option<i64>) -> &'static str {
    let c = categorie.and_then(|id| d.categories.iter().find(|c| c.id == id));
    match c {
        Some(c) if c.nature == Nature::Fixe && c.cle.as_deref() != Some(CLE_ABONNEMENTS) => {
            "charge"
        }
        _ => "subscription",
    }
}

fn ajouter(t: &mut Totaux, r: &Recurrent) {
    t.count += 1;
    t.monthly += r.monthly;
    t.yearly += r.yearly;
}

pub fn recurrents(d: &Donnees, etats: &Etats) -> Recurrents {
    let Some(contexte) = contexte(d) else {
        return Recurrents {
            as_of: None,
            from: None,
            span_days: 0,
            statements: 0,
            detectable: Vec::new(),
            items: Vec::new(),
            alerts_unseen: 0,
            total: Totaux::default(),
            subscriptions: Totaux::default(),
            charges: Totaux::default(),
        };
    };
    let fin = contexte.fin;
    let mut series = detecter(d);
    series.sort_by_key(|s| (s.commercant, s.cadence, s.occurrences[0].jour));
    let mut numeros: BTreeMap<(i64, Cadence), usize> = BTreeMap::new();
    let mut items = Vec::new();
    for s in &series {
        let numero = numeros.entry((s.commercant, s.cadence)).or_default();
        *numero += 1;
        let cle = if *numero == 1 {
            s.cadence.cle().to_string()
        } else {
            format!("{}-{numero}", s.cadence.cle())
        };
        let (Some(premiere), Some(derniere)) = (s.occurrences.first(), s.occurrences.last()) else {
            continue;
        };
        let tolerance = s.cadence.tolerance();
        let mut rang = derniere.rang + 1;
        let mut manquees = 0;
        let prochaine = loop {
            let Some(attendue) = s.cadence.echeance(s.ancre, rang) else {
                break None;
            };
            if (fin - attendue).num_days() < tolerance {
                break Some(attendue);
            }
            manquees += usize::from(contexte.couvert(attendue));
            rang += 1;
        };
        let arrete = manquees >= s.cadence.arret();
        if arrete && (fin - derniere.jour).num_days() > ARRET_VISIBLE_JOURS {
            continue;
        }
        let etat = etats
            .get(&(s.commercant, cle.clone()))
            .copied()
            .unwrap_or_default();
        let mut alertes = Vec::new();
        if !arrete {
            let avant = s
                .cadence
                .echeance(s.ancre, premiere.rang - 1)
                .is_some_and(|j| contexte.couvert(j));
            if avant && (fin - premiere.jour).num_days() <= NOUVEAUTE_JOURS {
                alertes.push(Alerte {
                    kind: "new",
                    seen: etat.nouveau_vu,
                    date: derniere.jour,
                    operation_id: derniere.id,
                    first_date: Some(premiere.jour),
                    old_amount: None,
                    new_amount: derniere.montant,
                    delta: None,
                    delta_bp: None,
                    yearly_delta: None,
                });
            }
            let changement = (1..s.occurrences.len())
                .rev()
                .find(|i| !proche(s.occurrences[*i].montant, s.occurrences[*i - 1].montant));
            if let Some(i) = changement {
                let (ancien, nouveau) = (&s.occurrences[i - 1], &s.occurrences[i]);
                if (fin - nouveau.jour).num_days() <= NOUVEAUTE_JOURS {
                    let delta = nouveau.montant - ancien.montant;
                    alertes.push(Alerte {
                        kind: "amount_changed",
                        seen: etat.montant_vu == Some(nouveau.montant),
                        date: nouveau.jour,
                        operation_id: nouveau.id,
                        first_date: None,
                        old_amount: Some(ancien.montant),
                        new_amount: nouveau.montant,
                        delta: Some(delta),
                        delta_bp: bp(delta, ancien.montant),
                        yearly_delta: Some(delta * s.cadence.par_an()),
                    });
                }
            }
        }
        let annuel = derniere.montant * s.cadence.par_an();
        items.push(Recurrent {
            id: format!("{}-{cle}", s.commercant),
            merchant_id: s.commercant,
            name: d.commercants.get(&s.commercant).cloned(),
            category_id: s.categorie,
            group: groupe(d, s.categorie),
            cadence: s.cadence.cle(),
            amount: derniere.montant,
            occurrences: s.occurrences.len(),
            since: premiere.jour,
            last_date: derniere.jour,
            last_operation_id: derniere.id,
            next_date: prochaine.filter(|_| !arrete),
            missed: manquees,
            status: if arrete { "stopped" } else { "active" },
            monthly: division(i128::from(annuel), 12),
            yearly: annuel,
            dismissed: etat.ecarte,
            alerts: alertes,
            commercant: s.commercant,
            cle,
        });
    }
    items.sort_by(|x, y| {
        (x.dismissed, x.status == "stopped")
            .cmp(&(y.dismissed, y.status == "stopped"))
            .then_with(|| y.yearly.cmp(&x.yearly))
            .then_with(|| x.id.cmp(&y.id))
    });
    let mut total = Totaux::default();
    let mut subscriptions = Totaux::default();
    let mut charges = Totaux::default();
    let mut alerts_unseen = 0;
    for r in items
        .iter()
        .filter(|r| !r.dismissed && r.status == "active")
    {
        ajouter(&mut total, r);
        ajouter(
            if r.group == "charge" {
                &mut charges
            } else {
                &mut subscriptions
            },
            r,
        );
        alerts_unseen += r.alerts.iter().filter(|a| !a.seen).count();
    }
    let span = (fin - contexte.debut).num_days() + 1;
    Recurrents {
        as_of: Some(fin),
        from: Some(contexte.debut),
        span_days: span,
        statements: d.releves.len(),
        detectable: Cadence::TOUTES
            .into_iter()
            .filter(|c| span >= c.jours_detectables())
            .map(Cadence::cle)
            .collect(),
        items,
        alerts_unseen,
        total,
        subscriptions,
        charges,
    }
}
