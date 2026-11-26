use axum::http::StatusCode;
use chrono::{Datelike, NaiveDate};
use serde_json::Value;

use crate::{
    bilan::Nature,
    stats::{
        Calcul, Categorie, Depassement, Donnees, Ecart, Mode, Moyenne, Operation, Projection,
        Releve, bp, couverture, division,
    },
    testutil::{Env, env},
};

pub(crate) const LOGEMENT: i64 = 1;
pub(crate) const COURSES: i64 = 2;
pub(crate) const RESTAURANTS: i64 = 3;
pub(crate) const EPARGNE: i64 = 4;
pub(crate) const REVENUS: i64 = 5;
const TRANSFERTS: i64 = 6;
pub(crate) const ENERGIE: i64 = 7;

pub(crate) fn jour(texte: &str) -> NaiveDate {
    NaiveDate::parse_from_str(texte, "%Y-%m-%d").unwrap()
}

pub(crate) fn categorie(id: i64, nom: &str, nature: Nature, plafond: Option<i64>) -> Categorie {
    Categorie {
        id,
        nom: nom.to_string(),
        nature,
        plafond,
        couleur: i32::try_from(id).unwrap(),
        archivee: false,
    }
}

pub(crate) fn categories() -> Vec<Categorie> {
    vec![
        categorie(LOGEMENT, "Logement", Nature::Fixe, Some(80_000)),
        categorie(COURSES, "Courses", Nature::Variable, Some(30_000)),
        categorie(RESTAURANTS, "Restaurants", Nature::Variable, None),
        categorie(EPARGNE, "Épargne", Nature::Epargne, None),
        categorie(REVENUS, "Revenus", Nature::Revenu, None),
        categorie(TRANSFERTS, "Transferts neutres", Nature::Neutre, None),
        categorie(ENERGIE, "Énergie", Nature::Fixe, None),
    ]
}

#[derive(Default)]
pub(crate) struct Jeu {
    pub(crate) d: Donnees,
}

impl Jeu {
    pub(crate) fn nouveau() -> Self {
        Self {
            d: Donnees {
                categories: categories(),
                ..Donnees::default()
            },
        }
    }

    pub(crate) fn releve(&mut self, debut: &str, fin: &str) -> i64 {
        let id = i64::try_from(self.d.releves.len()).unwrap() + 1;
        self.d.releves.push(Releve {
            id,
            debut: jour(debut),
            fin: jour(fin),
        });
        id
    }

    pub(crate) fn op(
        &mut self,
        releve: i64,
        date: &str,
        categorie: Option<i64>,
        montant: i64,
    ) -> i64 {
        let id = i64::try_from(self.d.operations.len()).unwrap() + 1;
        self.d.operations.push(Operation {
            id,
            releve,
            jour: jour(date),
            achat: None,
            categorie,
            credit: false,
            montant,
            commercant: None,
        });
        id
    }

    pub(crate) fn credit(&mut self, releve: i64, date: &str, categorie: Option<i64>, montant: i64) {
        self.op(releve, date, categorie, montant);
        self.d.operations.last_mut().unwrap().credit = true;
    }

    pub(crate) fn carte(
        &mut self,
        releve: i64,
        date: &str,
        achat: &str,
        categorie: i64,
        montant: i64,
    ) -> i64 {
        let id = self.op(releve, date, Some(categorie), montant);
        self.d.operations.last_mut().unwrap().achat = Some(jour(achat));
        id
    }

    fn chez(&mut self, commercant: i64) {
        self.d.operations.last_mut().unwrap().commercant = Some(commercant);
    }
}

fn quatre_mois() -> Jeu {
    let mut j = Jeu::nouveau();
    j.d.commercants.insert(10, "Bistrot Inventé".to_string());
    j.d.commercants.insert(11, "Cantine Fictive".to_string());
    let novembre = j.releve("2025-10-31", "2025-11-30");
    j.op(novembre, "2025-11-05", Some(LOGEMENT), 80_000);
    j.op(novembre, "2025-11-05", Some(ENERGIE), 6000);
    j.op(novembre, "2025-11-10", Some(COURSES), 20_000);
    j.op(novembre, "2025-11-20", Some(RESTAURANTS), 5000);
    j.credit(novembre, "2025-11-28", Some(REVENUS), 250_000);
    let decembre = j.releve("2025-11-30", "2025-12-31");
    j.op(decembre, "2025-12-05", Some(LOGEMENT), 80_000);
    j.op(decembre, "2025-12-05", Some(ENERGIE), 8000);
    j.op(decembre, "2025-12-12", Some(COURSES), 26_000);
    j.credit(decembre, "2025-12-15", Some(COURSES), 2000);
    j.op(decembre, "2025-12-20", Some(RESTAURANTS), 9000);
    j.chez(10);
    j.op(decembre, "2025-12-22", Some(EPARGNE), 30_000);
    j.credit(decembre, "2025-12-23", Some(EPARGNE), 5000);
    j.op(decembre, "2025-12-24", Some(TRANSFERTS), 40_000);
    j.credit(decembre, "2025-12-24", Some(TRANSFERTS), 40_000);
    j.credit(decembre, "2025-12-28", Some(REVENUS), 250_000);
    let janvier = j.releve("2025-12-31", "2026-01-31");
    j.carte(janvier, "2026-01-02", "2025-12-30", RESTAURANTS, 3000);
    j.chez(11);
    j.op(janvier, "2026-01-05", Some(LOGEMENT), 80_000);
    j.op(janvier, "2026-01-06", Some(ENERGIE), 7000);
    j.op(janvier, "2026-01-10", Some(COURSES), 34_000);
    j.op(janvier, "2026-01-15", Some(RESTAURANTS), 8000);
    j.chez(10);
    j.op(janvier, "2026-01-16", Some(RESTAURANTS), 4000);
    j.chez(11);
    j.op(janvier, "2026-01-18", None, 4000);
    j.credit(janvier, "2026-01-19", None, 1500);
    j.credit(janvier, "2026-01-28", Some(REVENUS), 260_000);
    let fevrier = j.releve("2026-01-31", "2026-02-10");
    j.op(fevrier, "2026-02-03", Some(COURSES), 12_000);
    j.op(fevrier, "2026-02-05", Some(LOGEMENT), 80_000);
    j.op(fevrier, "2026-02-08", Some(RESTAURANTS), 15_000);
    j.op(fevrier, "2026-02-09", None, 1000);
    j
}

fn ecart(amount: i64, delta: i64, delta_bp: Option<i64>) -> Option<Ecart> {
    Some(Ecart {
        amount,
        delta,
        delta_bp,
    })
}

fn moyenne(amount: i64, periods: usize, delta: i64, delta_bp: Option<i64>) -> Option<Moyenne> {
    Some(Moyenne {
        amount,
        periods,
        delta,
        delta_bp,
    })
}

#[test]
fn arrondis_au_plus_proche() {
    assert_eq!(division(5, 2), 3);
    assert_eq!(division(-5, 2), -3);
    assert_eq!(division(4, 3), 1);
    assert_eq!(division(29_000, 3), 9667);
    assert_eq!(bp(1, 3), Some(3333));
    assert_eq!(bp(-25_000, 25_000), Some(-10_000));
    assert_eq!(bp(100, 0), None);
    assert_eq!(bp(100, -5), None);
}

#[test]
fn periodes_calendaires_et_couverture() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let p = c.periodes();
    assert_eq!(
        p.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["2025-11", "2025-12", "2026-01", "2026-02"]
    );
    assert_eq!(
        couverture(&j.d.releves),
        [(jour("2025-11-01"), jour("2026-02-10"))]
    );
    assert!(p[..3].iter().all(|p| p.complete && !p.open));
    assert_eq!((p[0].days, p[1].days, p[3].days), (30, 31, 28));
    assert_eq!((p[3].complete, p[3].open), (false, true));
    assert_eq!(p[3].known_through, jour("2026-02-10"));
    assert_eq!(p[3].elapsed_days, 10);
    assert_eq!(p[2].statement_ids, [3]);
    assert_eq!(c.index(None), Some(3));
    assert_eq!(c.index(Some("2025-12")), Some(1));
    assert_eq!(c.index(Some("2025-10")), None);
}

#[test]
fn mois_calendaire_complet() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let m = c.mois(2);
    assert_eq!(
        (m.mode, m.period.id.as_str(), m.periods),
        ("calendar", "2026-01", 4)
    );
    assert_eq!(m.previous_id.as_deref(), Some("2025-12"));
    assert_eq!(m.next_id.as_deref(), Some("2026-02"));
    assert_eq!(m.operations, 8);
    let e = &m.expenses;
    assert_eq!(
        (e.total, e.fixed, e.variable, e.unclassified),
        (137_000, 87_000, 46_000, 4000)
    );
    assert_eq!(
        (e.fixed_bp, e.variable_bp, e.unclassified_bp),
        (Some(6350), Some(3358), Some(292))
    );
    assert_eq!(e.previous, ecart(124_000, 13_000, Some(1048)));
    assert_eq!(e.previous_to_date, None);
    assert_eq!(e.avg3, moyenne(117_500, 2, 19_500, Some(1660)));
    assert_eq!(e.avg12, moyenne(117_500, 2, 19_500, Some(1660)));
    assert_eq!(
        (m.income.total, m.income.classified, m.income.unclassified),
        (261_500, 260_000, 1500)
    );
    assert_eq!(m.income.unclassified_operations, 1);
    assert_eq!(m.income.previous, ecart(250_000, 11_500, Some(460)));
    assert_eq!((m.savings.total, m.savings.rate_bp), (0, Some(0)));
    assert_eq!(m.savings.previous, ecart(25_000, -25_000, Some(-10_000)));
    assert_eq!(m.remaining.total, 124_500);
    assert_eq!(m.remaining.previous, ecart(101_000, 23_500, Some(2327)));
    assert_eq!(
        (m.unclassified.amount, m.unclassified.operations),
        (4000, 1)
    );
    assert_eq!(m.unclassified.previous, ecart(0, 4000, None));
    assert_eq!(m.projection, None);
    assert_eq!(m.projection_reason, Some("complete"));
}

#[test]
fn lignes_par_categorie_et_invariant() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let m = c.mois(2);
    assert_eq!(
        m.categories
            .iter()
            .map(|l| (l.name.as_str(), l.amount, l.operations))
            .collect::<Vec<_>>(),
        [
            ("Logement", 80_000, 1),
            ("Courses", 34_000, 1),
            ("Restaurants", 12_000, 2),
            ("Énergie", 7000, 1)
        ]
    );
    let logement = &m.categories[0];
    assert_eq!((logement.kind, logement.share_bp), ("fixe", Some(5839)));
    assert_eq!(logement.previous, ecart(80_000, 0, Some(0)));
    assert_eq!(
        (logement.cap_delta, logement.cap_bp),
        (Some(0), Some(10_000))
    );
    let courses = &m.categories[1];
    assert_eq!(courses.previous, ecart(24_000, 10_000, Some(4167)));
    assert_eq!(courses.avg3, moyenne(22_000, 2, 12_000, Some(5455)));
    assert_eq!(
        (courses.cap_delta, courses.cap_bp),
        (Some(4000), Some(11_333))
    );
    assert_eq!(courses.trend, [20_000, 24_000, 34_000]);
    let restaurants = &m.categories[2];
    assert_eq!(restaurants.previous, ecart(12_000, 0, Some(0)));
    assert_eq!(restaurants.avg3, moyenne(8500, 2, 3500, Some(4118)));
    assert_eq!((restaurants.cap_delta, restaurants.cap_bp), (None, None));
    let energie = &m.categories[3];
    assert_eq!(energie.previous, ecart(8000, -1000, Some(-1250)));
    assert_eq!(energie.avg3, moyenne(7000, 2, 0, Some(0)));
    for mode in [Mode::Calendaire, Mode::Releve] {
        let c = Calcul::nouveau(&j.d, mode);
        let mut operations = 0;
        for i in 0..c.periodes().len() {
            let m = c.mois(i);
            let somme: i64 = m.categories.iter().map(|l| l.amount).sum();
            assert_eq!(somme + m.unclassified.amount, m.expenses.total);
            assert_eq!(
                m.expenses.fixed + m.expenses.variable + m.expenses.unclassified,
                m.expenses.total
            );
            assert_eq!(
                m.remaining.total,
                m.income.total - m.expenses.total - m.savings.total
            );
            assert_eq!(m.cumulative.current.last(), Some(&m.expenses.total));
            operations += m.operations;
        }
        assert_eq!(operations, 28);
    }
}

#[test]
fn remboursement_epargne_et_neutre() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let m = c.mois(1);
    assert_eq!(
        (m.expenses.total, m.expenses.fixed, m.expenses.variable),
        (124_000, 88_000, 36_000)
    );
    let courses = m.categories.iter().find(|l| l.id == COURSES).unwrap();
    assert_eq!((courses.amount, courses.operations), (24_000, 2));
    assert_eq!((m.savings.total, m.savings.rate_bp), (25_000, Some(1000)));
    assert_eq!(m.income.total, 250_000);
    assert_eq!(m.remaining.total, 101_000);
    assert!(
        m.categories
            .iter()
            .all(|l| l.id != TRANSFERTS && l.id != EPARGNE)
    );
    assert_eq!(m.operations, 11);
}

#[test]
fn achat_par_carte_rattache_a_sa_date_d_achat_au_passage_d_annee() {
    let j = quatre_mois();
    let calendaire = Calcul::nouveau(&j.d, Mode::Calendaire);
    let restaurants = |c: &Calcul, i: usize| {
        c.mois(i)
            .categories
            .iter()
            .find(|l| l.id == RESTAURANTS)
            .map(|l| l.amount)
    };
    assert_eq!(restaurants(&calendaire, 1), Some(12_000));
    assert_eq!(restaurants(&calendaire, 2), Some(12_000));
    assert_eq!(calendaire.mois(1).cumulative.current[29], 124_000);
    assert_eq!(calendaire.mois(1).cumulative.current[28], 121_000);
    let releve = Calcul::nouveau(&j.d, Mode::Releve);
    assert_eq!(restaurants(&releve, 1), Some(9000));
    assert_eq!(restaurants(&releve, 2), Some(15_000));
    assert_eq!(releve.mois(2).cumulative.current[0], 3000);
}

#[test]
fn periodes_par_releve() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Releve);
    assert_eq!(
        c.periodes()
            .iter()
            .map(|p| (p.id.as_str(), p.days, p.complete, p.open))
            .collect::<Vec<_>>(),
        [
            ("2025-11-30", 30, true, false),
            ("2025-12-31", 31, true, false),
            ("2026-01-31", 31, true, false),
            ("2026-02-10", 10, true, false)
        ]
    );
    let m = c.mois(2);
    assert_eq!(
        (m.mode, m.period.from, m.period.to),
        ("statement", jour("2025-12-31"), jour("2026-01-31"))
    );
    assert_eq!(m.expenses.total, 140_000);
    assert_eq!(m.expenses.previous, ecart(121_000, 19_000, Some(1570)));
    assert_eq!(m.expenses.avg3, moyenne(116_000, 2, 24_000, Some(2069)));
    assert_eq!(m.operations, 9);
    let fevrier = c.mois(3);
    assert_eq!(fevrier.expenses.total, 108_000);
    assert_eq!(fevrier.cumulative.current.len(), 10);
    assert_eq!(
        (fevrier.projection, fevrier.projection_reason),
        (None, Some("complete"))
    );
    assert_eq!(fevrier.expenses.previous_to_date, None);
    let liste = c.liste();
    assert_eq!(liste.len(), 4);
    assert_eq!(
        (
            liste[2].expenses,
            liste[2].income,
            liste[2].savings,
            liste[2].remaining,
            liste[2].operations
        ),
        (140_000, 261_500, 0, 121_500, 9)
    );
}

#[test]
fn depassements_classes_en_euros_et_en_pourcentage() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let m = c.mois(2);
    assert_eq!(
        m.overruns,
        [
            Depassement {
                category_id: COURSES,
                reference: "cap",
                reference_amount: 30_000,
                periods: None,
                excess: 4000,
                excess_bp: Some(1333),
                rank_amount: 1,
                rank_percent: 2,
            },
            Depassement {
                category_id: RESTAURANTS,
                reference: "average",
                reference_amount: 8500,
                periods: Some(2),
                excess: 3500,
                excess_bp: Some(4118),
                rank_amount: 2,
                rank_percent: 1,
            }
        ]
    );
    assert_eq!(m.overrun_total, 7500);
    let fevrier = c.mois(3);
    assert_eq!(
        fevrier
            .overruns
            .iter()
            .map(|d| (
                d.category_id,
                d.reference_amount,
                d.periods,
                d.excess,
                d.excess_bp
            ))
            .collect::<Vec<_>>(),
        [(RESTAURANTS, 9667, Some(3), 5333, Some(5517))]
    );
}

#[test]
fn seuils_des_depassements() {
    let mut j = Jeu::nouveau();
    j.d.categories = vec![
        categorie(1, "Plafond à peine dépassé", Nature::Variable, Some(10_000)),
        categorie(2, "Plafond dépassé", Nature::Variable, Some(10_000)),
        categorie(3, "Moyenne, petit montant", Nature::Variable, None),
        categorie(4, "Moyenne, petit pourcentage", Nature::Variable, None),
        categorie(5, "Moyenne dépassée", Nature::Variable, None),
        categorie(6, "Plafond nul", Nature::Variable, Some(0)),
        categorie(7, "Nouvelle", Nature::Variable, None),
    ];
    let mars = j.releve("2026-02-28", "2026-03-31");
    j.op(mars, "2026-03-10", Some(3), 1000);
    j.op(mars, "2026-03-10", Some(4), 50_000);
    j.op(mars, "2026-03-10", Some(5), 10_000);
    let avril = j.releve("2026-03-31", "2026-04-30");
    j.op(avril, "2026-04-10", Some(1), 10_099);
    j.op(avril, "2026-04-10", Some(2), 10_100);
    j.op(avril, "2026-04-10", Some(3), 1999);
    j.op(avril, "2026-04-10", Some(4), 54_900);
    j.op(avril, "2026-04-10", Some(5), 11_000);
    j.op(avril, "2026-04-10", Some(6), 500);
    j.op(avril, "2026-04-10", Some(7), 90_000);
    let m = Calcul::nouveau(&j.d, Mode::Calendaire).mois(1);
    assert_eq!(
        m.overruns
            .iter()
            .map(|d| (
                d.category_id,
                d.reference,
                d.excess,
                d.excess_bp,
                d.rank_amount,
                d.rank_percent
            ))
            .collect::<Vec<_>>(),
        [
            (5, "average", 1000, Some(1000), 1, 1),
            (6, "cap", 500, None, 2, 3),
            (2, "cap", 100, Some(100), 3, 2),
        ]
    );
}

#[test]
fn budget_et_absence_de_plafond() {
    let mut j = quatre_mois();
    let b = Calcul::nouveau(&j.d, Mode::Calendaire)
        .mois(2)
        .budget
        .unwrap();
    assert_eq!(
        (b.total, b.spent, b.bp, b.delta, b.uncapped),
        (110_000, 137_000, Some(12_455), 27_000, 23_000)
    );
    for c in &mut j.d.categories {
        c.plafond = None;
    }
    let m = Calcul::nouveau(&j.d, Mode::Calendaire).mois(2);
    assert_eq!(m.budget, None);
    assert_eq!(
        m.overruns
            .iter()
            .map(|d| (d.category_id, d.reference))
            .collect::<Vec<_>>(),
        [(COURSES, "average"), (RESTAURANTS, "average")]
    );
    assert!(m.categories.iter().all(|l| l.cap_delta.is_none()));
}

#[test]
fn cumul_jour_par_jour_superpose() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let cumul = c.mois(2).cumulative;
    let v = &cumul.current;
    assert_eq!(v.len(), 31);
    assert_eq!(
        (v[3], v[4], v[5], v[9], v[14], v[15], v[17], v[30]),
        (
            0, 80_000, 87_000, 121_000, 129_000, 133_000, 137_000, 137_000
        )
    );
    let avant = cumul.previous.unwrap();
    assert_eq!((avant.id.as_str(), avant.values.len()), ("2025-12", 31));
    assert_eq!(
        (
            avant.values[4],
            avant.values[11],
            avant.values[14],
            avant.values[19],
            avant.values[30]
        ),
        (88_000, 114_000, 112_000, 121_000, 124_000)
    );
    let moyenne = cumul.average.unwrap();
    assert_eq!((moyenne.periods, moyenne.values.len()), (2, 31));
    assert_eq!(
        (
            moyenne.values[3],
            moyenne.values[4],
            moyenne.values[11],
            moyenne.values[14],
            moyenne.values[29],
            moyenne.values[30]
        ),
        (0, 87_000, 110_000, 109_000, 117_500, 117_500)
    );
    let decembre = c.mois(1).cumulative;
    assert_eq!(decembre.previous.unwrap().values[30], 111_000);
    assert_eq!(decembre.average.unwrap().periods, 1);
    let novembre = c.mois(0).cumulative;
    assert_eq!((novembre.previous, novembre.average), (None, None));
}

#[test]
fn projection_d_une_periode_en_cours() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let m = c.mois(3);
    assert_eq!(m.expenses.total, 108_000);
    assert_eq!(m.expenses.previous, ecart(137_000, -29_000, Some(-2117)));
    assert_eq!(
        m.expenses.previous_to_date,
        ecart(121_000, -13_000, Some(-1074))
    );
    assert_eq!(m.expenses.avg3, moyenne(124_000, 3, -16_000, Some(-1290)));
    assert_eq!(m.expenses.avg12.as_ref().map(|a| a.periods), Some(3));
    assert_eq!((m.savings.rate_bp, m.income.total), (None, 0));
    assert_eq!(
        m.projection,
        Some(Projection {
            total: 165_400,
            variable: 78_400,
            fixed: 87_000,
            fixed_pending: 7000,
            daily_variable: 2800,
            remaining_days: 18,
            budget_delta: Some(55_400),
        })
    );
    assert_eq!(m.projection_reason, None);
    let v = &m.cumulative.current;
    assert_eq!(v.len(), 10);
    assert_eq!(
        (v[1], v[2], v[4], v[7], v[8], v[9]),
        (0, 12_000, 92_000, 107_000, 108_000, 108_000)
    );
    assert_eq!(m.cumulative.previous.unwrap().values.len(), 28);
    assert_eq!(
        m.categories
            .iter()
            .map(|l| (l.id, l.amount))
            .collect::<Vec<_>>(),
        [
            (LOGEMENT, 80_000),
            (RESTAURANTS, 15_000),
            (COURSES, 12_000),
            (ENERGIE, 0)
        ]
    );
    assert_eq!(m.history.len(), 4);
    assert_eq!(
        m.history
            .iter()
            .map(|p| (p.expenses, p.complete))
            .collect::<Vec<_>>(),
        [
            (111_000, true),
            (124_000, true),
            (137_000, true),
            (108_000, false)
        ]
    );
    assert_eq!(m.history[1].savings_rate_bp, Some(1000));
}

#[test]
fn projection_absente_quand_elle_n_a_pas_de_sens() {
    let mut j = Jeu::nouveau();
    let r = j.releve("2026-03-10", "2026-04-20");
    j.op(r, "2026-03-15", Some(COURSES), 10_000);
    j.op(r, "2026-04-02", Some(LOGEMENT), 80_000);
    j.op(r, "2026-04-10", Some(COURSES), 20_000);
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let mars = c.mois(0);
    assert_eq!((mars.period.complete, mars.period.open), (false, false));
    assert_eq!(
        (mars.projection.clone(), mars.projection_reason),
        (None, Some("partial"))
    );
    let avril = c.mois(1);
    assert_eq!((avril.period.open, avril.period.elapsed_days), (true, 20));
    assert_eq!(avril.expenses.previous, ecart(10_000, 90_000, Some(90_000)));
    assert_eq!(
        (avril.expenses.avg3.clone(), avril.expenses.avg12.clone()),
        (None, None)
    );
    assert_eq!(avril.cumulative.average, None);
    assert!(!avril.history[0].complete);
    let p = avril.projection.unwrap();
    assert_eq!(
        (p.variable, p.fixed, p.fixed_pending, p.total),
        (30_000, 80_000, 0, 110_000)
    );
    assert_eq!(
        (p.daily_variable, p.remaining_days, p.budget_delta),
        (1000, 10, Some(0))
    );

    let mut j = Jeu::nouveau();
    let r = j.releve("2026-04-30", "2026-05-04");
    j.op(r, "2026-05-02", Some(COURSES), 3000);
    let mai = Calcul::nouveau(&j.d, Mode::Calendaire).mois(0);
    assert_eq!((mai.period.open, mai.period.elapsed_days), (true, 4));
    assert_eq!(
        (mai.projection, mai.projection_reason),
        (None, Some("too_early"))
    );

    let mut j = Jeu::nouveau();
    let r = j.releve("2026-05-09", "2026-05-20");
    j.op(r, "2026-05-12", Some(COURSES), 3000);
    let mai = Calcul::nouveau(&j.d, Mode::Calendaire).mois(0);
    assert_eq!((mai.period.open, mai.period.complete), (true, false));
    assert_eq!(
        (mai.projection, mai.projection_reason),
        (None, Some("partial"))
    );
}

#[test]
fn une_seule_periode_et_mois_manquant() {
    let mut j = Jeu::nouveau();
    let septembre = j.releve("2025-08-31", "2025-09-30");
    j.op(septembre, "2025-09-10", Some(RESTAURANTS), 6000);
    let m = Calcul::nouveau(&j.d, Mode::Calendaire).mois(0);
    assert_eq!(
        (m.periods, m.previous_id.clone(), m.next_id.clone()),
        (1, None, None)
    );
    assert_eq!(
        (
            m.expenses.previous.clone(),
            m.expenses.avg3.clone(),
            m.expenses.avg12.clone()
        ),
        (None, None, None)
    );
    assert_eq!(
        (
            m.categories[0].previous.clone(),
            m.categories[0].avg3.clone()
        ),
        (None, None)
    );
    assert!(m.overruns.is_empty());
    assert_eq!(m.history.len(), 1);

    let novembre = j.releve("2025-10-31", "2025-11-30");
    j.op(novembre, "2025-11-12", Some(RESTAURANTS), 9000);
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    assert_eq!(
        c.periodes()
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["2025-09", "2025-11"]
    );
    let m = c.mois(1);
    assert_eq!(m.previous_id.as_deref(), Some("2025-09"));
    assert_eq!(m.expenses.previous, None);
    assert_eq!(m.cumulative.previous, None);
    assert_eq!(m.expenses.avg3, moyenne(6000, 1, 3000, Some(5000)));
    let par_releve = Calcul::nouveau(&j.d, Mode::Releve).mois(1);
    assert_eq!(par_releve.expenses.previous, ecart(6000, 3000, Some(5000)));
}

#[test]
fn moyenne_sur_douze_periodes_au_plus() {
    let mut j = Jeu::nouveau();
    let mut debut = jour("2024-12-31");
    for rang in 0..15_i64 {
        let fin = debut.checked_add_months(chrono::Months::new(1)).unwrap();
        let fin = fin
            .with_day(1)
            .unwrap()
            .checked_add_months(chrono::Months::new(1))
            .unwrap()
            .pred_opt()
            .unwrap();
        let r = j.releve(&debut.to_string(), &fin.to_string());
        j.op(r, &fin.to_string(), Some(COURSES), (rang + 1) * 1000);
        debut = fin;
    }
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    assert_eq!(c.periodes().len(), 15);
    let m = c.mois(14);
    assert_eq!(m.period.id, "2026-03");
    assert_eq!(m.expenses.avg3, moyenne(13_000, 3, 2000, Some(1538)));
    assert_eq!(m.expenses.avg12, moyenne(8500, 12, 6500, Some(7647)));
    assert_eq!(m.history.len(), 12);
    assert_eq!(m.categories[0].trend.len(), 12);
    assert_eq!(m.cumulative.average.unwrap().periods, 12);
}

#[test]
fn detail_d_une_categorie() {
    let j = quatre_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let d = c.detail(2, RESTAURANTS).unwrap();
    assert_eq!((d.category.amount, d.category.operations), (12_000, 2));
    assert_eq!(
        d.history
            .iter()
            .map(|h| (h.id.as_str(), h.amount))
            .collect::<Vec<_>>(),
        [("2025-11", 5000), ("2025-12", 12_000), ("2026-01", 12_000)]
    );
    assert_eq!(d.over_cap, 0);
    assert_eq!(
        d.merchants
            .iter()
            .map(|m| (
                m.merchant_id,
                m.name.as_deref(),
                m.amount,
                m.operations,
                m.average,
                m.previous.clone()
            ))
            .collect::<Vec<_>>(),
        [
            (
                Some(10),
                Some("Bistrot Inventé"),
                8000,
                1,
                8000,
                ecart(9000, -1000, Some(-1111))
            ),
            (
                Some(11),
                Some("Cantine Fictive"),
                4000,
                1,
                4000,
                ecart(3000, 1000, Some(3333))
            )
        ]
    );
    assert_eq!((d.operations_total, d.merchants_total), (2, 2));
    assert_eq!(d.dernieres, [21, 20]);
    let courses = c.detail(2, COURSES).unwrap();
    assert_eq!(courses.over_cap, 1);
    assert_eq!(courses.merchants[0].merchant_id, None);
    let epargne = c.detail(1, EPARGNE).unwrap();
    assert_eq!(
        (epargne.category.amount, epargne.category.share_bp),
        (25_000, None)
    );
    assert!(c.detail(2, 99).is_none());
    let par_releve = Calcul::nouveau(&j.d, Mode::Releve)
        .detail(2, RESTAURANTS)
        .unwrap();
    assert_eq!(
        (par_releve.category.amount, par_releve.operations_total),
        (15_000, 3)
    );
    assert_eq!(par_releve.dernieres, [21, 20, 16]);
}

pub(crate) struct Semis {
    pub(crate) courses: i64,
    pub(crate) logement: i64,
    pub(crate) revenus: i64,
}

pub(crate) async fn categorie_de(e: &Env, qui: &str, cle: &str) -> i64 {
    e.get(qui, "/categories").await.json()["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == cle)
        .and_then(|c| c["id"].as_i64())
        .unwrap()
}

pub(crate) async fn releve_de(
    e: &Env,
    sub: &str,
    numero: i32,
    debut: &str,
    fin: &str,
) -> (i64, i64) {
    let compte: i64 = sqlx::query_scalar(
        "INSERT INTO comptes (sub, banque, iban) VALUES ($1, 'cmb', 'FR00 0000 0000 0000 0000 0000 000')
         ON CONFLICT (sub, banque, iban) DO UPDATE SET iban = EXCLUDED.iban RETURNING id",
    )
    .bind(sub)
    .fetch_one(e.state.pool())
    .await
    .unwrap();
    let releve: i64 = sqlx::query_scalar(
        "INSERT INTO releves (compte_id, sub, numero, annee, debut, date_arrete, ancien_solde, nouveau_solde,
                              total_debits, total_credits, pages, empreinte, controle)
         VALUES ($1, $2, $3, 2026, $4, $5, 0, 0, 0, 0, 1, $6, '{}') RETURNING id",
    )
    .bind(compte)
    .bind(sub)
    .bind(numero)
    .bind(jour(debut))
    .bind(jour(fin))
    .bind(format!("{sub}-{numero}"))
    .fetch_one(e.state.pool())
    .await
    .unwrap();
    (compte, releve)
}

pub(crate) struct Ligne<'a> {
    pub(crate) date: &'a str,
    pub(crate) achat: Option<&'a str>,
    pub(crate) categorie: Option<i64>,
    pub(crate) montant: i64,
    pub(crate) sens: &'a str,
}

pub(crate) async fn operations_de(
    e: &Env,
    sub: &str,
    (compte, releve): (i64, i64),
    lignes: &[Ligne<'_>],
) {
    for (rang, l) in lignes.iter().enumerate() {
        sqlx::query(
            "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur, date_achat,
                                     section, type, libelle, montant, sens, categorie_id, origine)
             VALUES ($1, $2, $3, $4, 1, $5, $5, $6, 'AUTRES OPERATIONS', $7, $8, $9, $10, $11, $12)",
        )
        .bind(releve)
        .bind(compte)
        .bind(sub)
        .bind(i32::try_from(rang).unwrap())
        .bind(jour(l.date))
        .bind(l.achat.map(jour))
        .bind(if l.achat.is_some() { "carte" } else { "autre" })
        .bind(format!("ZZQ INVENTE {rang}"))
        .bind(l.montant)
        .bind(l.sens)
        .bind(l.categorie)
        .bind(l.categorie.map(|_| "operation"))
        .execute(e.state.pool())
        .await
        .unwrap();
    }
}

pub(crate) fn debit(date: &str, categorie: Option<i64>, montant: i64) -> Ligne<'_> {
    Ligne {
        date,
        achat: None,
        categorie,
        montant,
        sens: "debit",
    }
}

pub(crate) async fn semer(e: &Env) -> Semis {
    let s = Semis {
        courses: categorie_de(e, "alice", "courses").await,
        logement: categorie_de(e, "alice", "logement").await,
        revenus: categorie_de(e, "alice", "revenus").await,
    };
    let mars = releve_de(e, "sub-alice", 3, "2026-02-28", "2026-03-31").await;
    operations_de(
        e,
        "sub-alice",
        mars,
        &[
            debit("2026-03-05", Some(s.logement), 70_000),
            debit("2026-03-12", Some(s.courses), 20_000),
            Ligne {
                date: "2026-03-28",
                achat: None,
                categorie: Some(s.revenus),
                montant: 200_000,
                sens: "credit",
            },
        ],
    )
    .await;
    let avril = releve_de(e, "sub-alice", 4, "2026-03-31", "2026-04-30").await;
    operations_de(
        e,
        "sub-alice",
        avril,
        &[
            Ligne {
                date: "2026-04-02",
                achat: Some("2026-03-30"),
                categorie: Some(s.courses),
                montant: 5000,
                sens: "debit",
            },
            debit("2026-04-05", Some(s.logement), 70_000),
            debit("2026-04-10", Some(s.courses), 26_000),
            debit("2026-04-11", None, 1500),
        ],
    )
    .await;
    let courses_bruno = categorie_de(e, "bruno", "courses").await;
    let bruno = releve_de(e, "sub-bruno", 4, "2026-03-31", "2026-04-30").await;
    operations_de(
        e,
        "sub-bruno",
        bruno,
        &[debit("2026-04-10", Some(courses_bruno), 77_700)],
    )
    .await;
    s
}

pub(crate) async fn lire(e: &Env, qui: &str, chemin: &str) -> Value {
    let r = e.get(qui, chemin).await;
    assert_eq!(r.statut, StatusCode::OK, "{chemin} : {}", r.texte());
    r.json()
}

#[tokio::test]
async fn routes_des_statistiques() {
    let Some(e) = env().await else { return };
    let vide = lire(&e, "alice", "/stats/months").await;
    assert_eq!(vide["periods"].as_array().unwrap().len(), 0);
    assert_eq!(
        e.get("alice", "/stats/month").await.statut,
        StatusCode::NOT_FOUND
    );
    let s = semer(&e).await;
    let liste = lire(&e, "alice", "/stats/months").await;
    assert_eq!(liste["mode"], "calendar");
    let periodes = liste["periods"].as_array().unwrap();
    assert_eq!(periodes.len(), 2);
    assert_eq!(periodes[0]["id"], "2026-03");
    assert_eq!(periodes[0]["expenses"], 95_000);
    assert_eq!(periodes[0]["income"], 200_000);
    assert_eq!(periodes[1]["expenses"], 97_500);
    assert_eq!(periodes[1]["complete"], true);
    let mois = lire(&e, "alice", "/stats/month").await;
    assert_eq!(mois["period"]["id"], "2026-04");
    assert_eq!(mois["period"]["from"], "2026-04-01");
    assert_eq!(mois["expenses"]["total"], 97_500);
    assert_eq!(mois["expenses"]["unclassified"], 1500);
    assert_eq!(mois["expenses"]["previous"]["amount"], 95_000);
    assert_eq!(mois["expenses"]["avg3"]["periods"], 1);
    assert_eq!(mois["budget"], Value::Null);
    assert_eq!(mois["projection"], Value::Null);
    assert_eq!(mois["projection_reason"], "complete");
    assert_eq!(mois["cumulative"]["current"].as_array().unwrap().len(), 30);
    assert_eq!(mois["categories"][0]["id"], s.logement);
    let mars = lire(&e, "alice", "/stats/month?period=2026-03").await;
    assert_eq!(mars["expenses"]["variable"], 25_000);
    let releve = lire(&e, "alice", "/stats/month?mode=statement&period=2026-04-30").await;
    assert_eq!(releve["mode"], "statement");
    assert_eq!(releve["expenses"]["total"], 102_500);
    assert_eq!(
        lire(&e, "alice", "/stats/months?mode=statement").await["periods"][0]["id"],
        "2026-03-31"
    );
    for chemin in [
        "/stats/month?mode=semaine",
        "/stats/months?mode=semaine",
        "/stats/month?period=2026-13",
        "/stats/month?period=avril",
        "/stats/month?mode=statement&period=2026-04",
    ] {
        assert_eq!(
            e.get("alice", chemin).await.statut,
            StatusCode::BAD_REQUEST,
            "{chemin}"
        );
    }
    for chemin in [
        "/stats/month?period=2025-01",
        "/stats/month?mode=statement&period=2026-04-29",
    ] {
        assert_eq!(
            e.get("alice", chemin).await.statut,
            StatusCode::NOT_FOUND,
            "{chemin}"
        );
    }
}

#[tokio::test]
async fn detail_et_operations_filtrees_par_date_d_achat() {
    let Some(e) = env().await else { return };
    let s = semer(&e).await;
    let detail = lire(
        &e,
        "alice",
        &format!("/stats/month/categories/{}?period=2026-03", s.courses),
    )
    .await;
    assert_eq!(detail["category"]["amount"], 25_000);
    assert_eq!(detail["operations_total"], 2);
    let operations = detail["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 2);
    assert_eq!(operations[0]["purchase_date"], "2026-03-30");
    assert_eq!(operations[0]["amount"], 5000);
    assert_eq!(operations[1]["date"], "2026-03-12");
    assert_eq!(detail["history"].as_array().unwrap().len(), 1);
    let filtre = format!(
        "/operations?from=2026-03-01&to=2026-03-31&category={}",
        s.courses
    );
    assert_eq!(lire(&e, "alice", &filtre).await["total"], 1);
    let par_achat = lire(&e, "alice", &format!("{filtre}&date=purchase")).await;
    assert_eq!(par_achat["total"], 2);
    assert_eq!(par_achat["total_debits"], 25_000);
    let avril = lire(
        &e,
        "alice",
        &format!(
            "/operations?from=2026-04-01&to=2026-04-30&date=purchase&category={}",
            s.courses
        ),
    )
    .await;
    assert_eq!(avril["total"], 1);
    assert_eq!(
        e.get("alice", "/operations?date=valeur").await.statut,
        StatusCode::BAD_REQUEST
    );
    let export = e
        .get(
            "alice",
            &format!(
                "/operations/export.csv?from=2026-03-01&to=2026-03-31&date=purchase&category={}",
                s.courses
            ),
        )
        .await;
    assert_eq!(export.statut, StatusCode::OK);
    assert_eq!(export.texte().lines().count(), 3);
    assert_eq!(
        e.get("alice", "/stats/month/categories/999999")
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get("alice", "/stats/month/categories/abc").await.statut,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn statistiques_cloisonnees_par_personne() {
    let Some(e) = env().await else { return };
    let s = semer(&e).await;
    let bruno = lire(&e, "bruno", "/stats/month").await;
    assert_eq!(bruno["expenses"]["total"], 77_700);
    assert_eq!(bruno["periods"], 1);
    assert_eq!(bruno["operations"], 1);
    assert_eq!(
        lire(&e, "bruno", "/stats/months").await["periods"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        e.get("bruno", &format!("/stats/month/categories/{}", s.courses))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    let alice = lire(&e, "alice", "/stats/month").await;
    assert_eq!(alice["expenses"]["total"], 97_500);
    assert_eq!(
        e.get("personne", "/stats/month").await.statut,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.appel(None, "GET", "/stats/month").await.statut,
        StatusCode::UNAUTHORIZED
    );
}

pub(crate) fn entier(v: &Value) -> i64 {
    v.as_i64().expect("entier")
}

#[tokio::test]
async fn releves_reels_statistiques() {
    let Ok(dossier) = std::env::var("TIRELIRE_RELEVES_REELS") else {
        return;
    };
    let Some(e) = env().await else { return };
    let mut importes = 0;
    for entree in std::fs::read_dir(&dossier).expect("TIRELIRE_RELEVES_REELS : dossier lisible") {
        let chemin = entree.unwrap().path();
        if !chemin
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("pdf"))
        {
            continue;
        }
        let r = e
            .importer("alice", &[("releve.pdf", &std::fs::read(&chemin).unwrap())])
            .await
            .json();
        assert_eq!(r["results"][0]["status"], "imported", "relevé réel refusé");
        importes += 1;
    }
    assert!(importes > 0, "TIRELIRE_RELEVES_REELS : aucun PDF");
    let (total, debits, credits): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(montant) FILTER (WHERE sens = 'debit'), 0)::bigint,
                coalesce(sum(montant) FILTER (WHERE sens = 'credit'), 0)::bigint FROM operations",
    )
    .fetch_one(e.state.pool())
    .await
    .unwrap();
    let neutre: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(CASE WHEN o.sens = 'credit' THEN o.montant ELSE -o.montant END), 0)::bigint
         FROM operations o JOIN categories c ON c.id = o.categorie_id WHERE c.nature = 'neutre'",
    )
    .fetch_one(e.state.pool())
    .await
    .unwrap();
    for mode in ["calendar", "statement"] {
        let liste = lire(&e, "alice", &format!("/stats/months?mode={mode}")).await;
        let periodes = liste["periods"].as_array().unwrap();
        assert!(!periodes.is_empty());
        let (mut operations, mut solde) = (0, 0);
        for p in periodes {
            let id = p["id"].as_str().unwrap();
            let m = lire(
                &e,
                "alice",
                &format!("/stats/month?mode={mode}&period={id}"),
            )
            .await;
            let depenses = entier(&m["expenses"]["total"]);
            let categories = m["categories"].as_array().unwrap();
            let somme: i64 = categories.iter().map(|c| entier(&c["amount"])).sum();
            assert_eq!(somme + entier(&m["unclassified"]["amount"]), depenses);
            assert_eq!(
                entier(&m["expenses"]["fixed"])
                    + entier(&m["expenses"]["variable"])
                    + entier(&m["expenses"]["unclassified"]),
                depenses
            );
            assert_eq!(
                entier(&m["remaining"]["total"]),
                entier(&m["income"]["total"]) - depenses - entier(&m["savings"]["total"])
            );
            assert_eq!(
                entier(
                    m["cumulative"]["current"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap()
                ),
                depenses
            );
            assert_eq!(entier(&p["expenses"]), depenses);
            let par_categorie: i64 = categories.iter().map(|c| entier(&c["operations"])).sum();
            assert!(
                par_categorie + entier(&m["unclassified"]["operations"])
                    <= entier(&m["operations"])
            );
            for c in categories {
                let detail = lire(
                    &e,
                    "alice",
                    &format!(
                        "/stats/month/categories/{}?mode={mode}&period={id}",
                        c["id"]
                    ),
                )
                .await;
                assert_eq!(detail["category"]["amount"], c["amount"]);
                assert_eq!(detail["operations_total"], c["operations"]);
                let commercants: i64 = detail["merchants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| entier(&x["amount"]))
                    .sum();
                if detail["merchants_total"].as_u64()
                    == Some(detail["merchants"].as_array().unwrap().len() as u64)
                {
                    assert_eq!(commercants, entier(&c["amount"]));
                }
            }
            operations += entier(&m["operations"]);
            solde += entier(&m["remaining"]["total"]);
        }
        assert_eq!(operations, total);
        assert_eq!(solde + neutre, credits - debits);
        eprintln!(
            "relevés réels, mode {mode} : {} période(s), invariants tenus",
            periodes.len()
        );
    }
}
