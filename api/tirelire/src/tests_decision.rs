use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::{
    bilan::Nature,
    decision::{
        Levier, Reference, Reglage, Simulation, Synthese, Variation, economie, effet, leviers,
        resumer, significatif, simuler,
    },
    recurrents::{Etats, Recurrents, recurrents},
    stats::{Calcul, Ecart, Mode, Moyenne},
    tests_stats::{
        COURSES, ENERGIE, EPARGNE, Jeu, LOGEMENT, RESTAURANTS, REVENUS, categorie, categorie_de,
        debit, entier, jour, lire, operations_de, releve_de,
    },
    testutil::{Env, env},
};

const CAS: &str = include_str!("../tests/fixtures/simulation.json");

const ABONNEMENTS: i64 = 20;
const LOISIRS: i64 = 21;
const FLUX: i64 = 1;
const MARCHE: i64 = 2;
const BISTROT: i64 = 3;
const REGIE: i64 = 4;
const CAFE: i64 = 5;
const CINE: i64 = 6;
const LIBRAIRIE: i64 = 7;
const JEU: i64 = 8;

const MOIS: [(&str, &str); 4] = [
    ("2025-12-31", "2026-01-31"),
    ("2026-01-31", "2026-02-28"),
    ("2026-02-28", "2026-03-31"),
    ("2026-03-31", "2026-04-30"),
];

struct Saisie {
    courses: i64,
    restaurants: i64,
    energie: i64,
    cine: i64,
    librairie: i64,
    flux: i64,
    epargne: i64,
    revenus: i64,
}

const SAISIES: [Saisie; 4] = [
    Saisie {
        courses: 20_000,
        restaurants: 10_000,
        energie: 6000,
        cine: 0,
        librairie: 5000,
        flux: 1399,
        epargne: 20_000,
        revenus: 250_000,
    },
    Saisie {
        courses: 24_000,
        restaurants: 12_000,
        energie: 6000,
        cine: 0,
        librairie: 5000,
        flux: 1399,
        epargne: 20_000,
        revenus: 250_000,
    },
    Saisie {
        courses: 22_000,
        restaurants: 14_001,
        energie: 6000,
        cine: 1000,
        librairie: 4000,
        flux: 1399,
        epargne: 20_000,
        revenus: 250_000,
    },
    Saisie {
        courses: 35_000,
        restaurants: 30_000,
        energie: 4000,
        cine: 3900,
        librairie: 1900,
        flux: 1399,
        epargne: 10_000,
        revenus: 260_000,
    },
];

fn vide() -> Jeu {
    let mut j = Jeu::nouveau();
    let mut abonnements = categorie(ABONNEMENTS, "Abonnements", Nature::Fixe, None);
    abonnements.cle = Some("abonnements".to_string());
    j.d.categories.push(abonnements);
    j.d.categories
        .push(categorie(LOISIRS, "Loisirs", Nature::Variable, None));
    for (id, nom) in [
        (FLUX, "Fluxo"),
        (MARCHE, "Marché Colline"),
        (BISTROT, "Bistrot Lune"),
        (REGIE, "Régie Belvédère"),
        (CAFE, "Café Tamis"),
        (CINE, "Ciné Phare"),
        (LIBRAIRIE, "Librairie des Quais"),
        (JEU, "Arcadia"),
    ] {
        j.d.commercants.insert(id, nom.to_string());
    }
    j
}

fn preleve(j: &mut Jeu, releve: i64, date: &str, commercant: i64, categorie: Option<i64>, n: i64) {
    j.op(releve, date, categorie, n);
    j.chez(commercant);
    j.d.operations.last_mut().unwrap().preleve = true;
}

fn carte(j: &mut Jeu, releve: i64, date: &str, commercant: i64, categorie: i64, n: i64) {
    if n > 0 {
        j.carte(releve, date, date, categorie, n);
        j.chez(commercant);
    }
}

fn saisir(j: &mut Jeu, (debut, fin): (&str, &str), s: &Saisie) -> i64 {
    let r = j.releve(debut, fin);
    let le = |quantieme: u32| format!("{}-{quantieme:02}", &fin[..7]);
    preleve(j, r, &le(5), REGIE, Some(LOGEMENT), 80_000);
    j.op(r, &le(6), Some(ENERGIE), s.energie);
    carte(j, r, &le(10), MARCHE, COURSES, s.courses);
    preleve(j, r, &le(12), FLUX, Some(ABONNEMENTS), s.flux);
    carte(j, r, &le(15), BISTROT, RESTAURANTS, s.restaurants);
    carte(j, r, &le(18), CINE, LOISIRS, s.cine);
    carte(j, r, &le(20), LIBRAIRIE, LOISIRS, s.librairie);
    j.op(r, &le(22), Some(EPARGNE), s.epargne);
    j.credit(r, &le(28), Some(REVENUS), s.revenus);
    r
}

fn quatre() -> Jeu {
    let mut j = vide();
    let mut dernier = 0;
    for (m, s) in MOIS.iter().zip(&SAISIES) {
        dernier = saisir(&mut j, *m, s);
    }
    j.op(dernier, "2026-04-21", None, 3000);
    j
}

fn court() -> Jeu {
    let mut j = vide();
    let r = j.releve("2026-01-10", "2026-01-31");
    carte(&mut j, r, "2026-01-20", MARCHE, COURSES, 12_000);
    j.credit(r, "2026-01-28", Some(REVENUS), 100_000);
    j
}

fn connus(j: &Jeu) -> Recurrents {
    recurrents(&j.d, &Etats::new())
}

fn simule(j: &Jeu, mode: Mode, reglages: &[Reglage], arrets: &[&str]) -> Simulation {
    essai(j, mode, reglages, arrets).unwrap()
}

fn essai(
    j: &Jeu,
    mode: Mode,
    reglages: &[Reglage],
    arrets: &[&str],
) -> Result<Simulation, &'static str> {
    let c = Calcul::nouveau(&j.d, mode);
    let i = c.index(None).unwrap();
    let arrets: Vec<String> = arrets.iter().map(|a| (*a).to_string()).collect();
    simuler(&c, i, &connus(j), reglages, &arrets)
}

fn resume(j: &Jeu, mode: Mode, periode: Option<&str>) -> Synthese {
    let c = Calcul::nouveau(&j.d, mode);
    resumer(&c, c.index(periode).unwrap(), &connus(j))
}

fn classes(j: &Jeu, periode: Option<&str>) -> Vec<Levier> {
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let i = c.index(periode).unwrap();
    leviers(&c, i, &c.mois(i), &connus(j))
}

fn pourcent(id: i64, p: i64) -> Reglage {
    Reglage {
        id,
        percent: Some(p),
        target: None,
    }
}

fn cible(id: i64, t: i64) -> Reglage {
    Reglage {
        id,
        percent: None,
        target: Some(t),
    }
}

fn ligne(s: &Simulation, id: i64) -> (i64, i64, i64, i64) {
    let l = s.categories.iter().find(|c| c.id == id).unwrap();
    (l.base, l.stopped, l.saving, l.amount)
}

fn variation(v: &Variation) -> (&str, i64, i64, i64, Option<i64>) {
    (
        v.name.as_str(),
        v.amount,
        v.reference_amount,
        v.delta,
        v.delta_bp,
    )
}

fn levier(l: &Levier) -> (usize, &str, Option<&str>, i64, i64, i64, i64) {
    (
        l.rank,
        l.kind,
        l.name.as_deref(),
        l.amount,
        l.reference_amount,
        l.monthly,
        l.yearly,
    )
}

fn nombre(v: &Value) -> Option<i64> {
    v.as_i64()
}

#[test]
fn formule_des_cas_partages_avec_le_front() {
    let cas: Value = serde_json::from_str(CAS).unwrap();
    let economies = cas["economie"].as_array().unwrap();
    assert_eq!(economies.len(), 19);
    for c in economies {
        assert_eq!(
            economie(
                entier(&c["base"]),
                entier(&c["arretes"]),
                nombre(&c["percent"]),
                nombre(&c["target"])
            ),
            (entier(&c["economie"]), entier(&c["montant"])),
            "{}",
            c["nom"]
        );
    }
    let effets = cas["effet"].as_array().unwrap();
    assert_eq!(effets.len(), 3);
    for c in effets {
        let reference: Reference = serde_json::from_value(c["reference"].clone()).unwrap();
        let e = effet(
            &reference,
            entier(&c["categories"]),
            entier(&c["abonnements_par_mois"]),
            entier(&c["abonnements_par_an"]),
        );
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            c["attendu"],
            "{}",
            c["nom"]
        );
    }
}

#[test]
fn base_du_simulateur_moyenne_des_periodes_completes() {
    let j = quatre();
    for (mode, periode, ids) in [
        (
            Mode::Calendaire,
            "2026-04",
            ["2026-02", "2026-03", "2026-04"],
        ),
        (
            Mode::Releve,
            "2026-04-30",
            ["2026-02-28", "2026-03-31", "2026-04-30"],
        ),
    ] {
        let s = simule(&j, mode, &[], &[]);
        assert_eq!((s.mode, s.period.as_str()), (mode.cle(), periode));
        assert_eq!(
            (s.basis.method, s.basis.periods, s.basis.fragile),
            ("average", 3, false)
        );
        assert_eq!(s.basis.period_ids, ids);
        assert_eq!(s.basis.to, jour("2026-04-30"));
        assert_eq!(
            s.categories
                .iter()
                .map(|c| (c.name.as_str(), c.kind, c.base))
                .collect::<Vec<_>>(),
            [
                ("Logement", "fixe", 80_000),
                ("Courses", "variable", 27_000),
                ("Restaurants", "variable", 18_667),
                ("Énergie", "fixe", 5333),
                ("Loisirs", "variable", 5267),
                ("Abonnements", "fixe", 1399),
            ]
        );
        assert_eq!(
            s.reference,
            Reference {
                expenses: 138_666,
                income: 253_333,
                savings: 16_667,
                remaining: 98_000,
                savings_rate_bp: Some(658),
                budget: Some(110_000),
            }
        );
        assert_eq!(
            (s.result.monthly, s.result.yearly, s.result.expenses),
            (0, 0, 138_666)
        );
        assert_eq!(
            (
                s.result.remaining,
                s.result.savings,
                s.result.savings_rate_bp
            ),
            (98_000, 16_667, Some(658))
        );
        assert_eq!(s.result.budget_delta, Some(28_666));
        assert_eq!(
            s.subscriptions
                .iter()
                .map(|a| (
                    a.id.as_str(),
                    a.name.as_deref(),
                    a.monthly,
                    a.yearly,
                    a.stopped
                ))
                .collect::<Vec<_>>(),
            [("1-monthly", Some("Fluxo"), 1399, 16_788, false)]
        );
    }
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let janvier = simuler(&c, 0, &connus(&j), &[], &[]).unwrap();
    assert_eq!(
        (janvier.basis.method, janvier.basis.periods),
        ("average", 1)
    );
    assert_eq!(janvier.basis.period_ids, ["2026-01"]);
    assert_eq!(ligne(&janvier, COURSES).0, 20_000);
}

#[test]
fn simulation_d_une_puis_de_plusieurs_categories() {
    let j = quatre();
    let une = simule(&j, Mode::Calendaire, &[pourcent(RESTAURANTS, 30)], &[]);
    assert_eq!(ligne(&une, RESTAURANTS), (18_667, 0, 5600, 13_067));
    assert_eq!(ligne(&une, COURSES), (27_000, 0, 0, 27_000));
    let r = &une.result;
    assert_eq!(
        (r.monthly, r.yearly, r.categories, r.subscriptions),
        (5600, 67_200, 5600, 0)
    );
    assert_eq!(
        (r.expenses, r.remaining, r.savings, r.savings_rate_bp),
        (133_066, 103_600, 22_267, Some(879))
    );
    assert_eq!(r.budget_delta, Some(23_066));

    let plusieurs = simule(
        &j,
        Mode::Calendaire,
        &[
            pourcent(RESTAURANTS, 30),
            cible(COURSES, 25_000),
            pourcent(ABONNEMENTS, 50),
        ],
        &["1-monthly"],
    );
    assert_eq!(ligne(&plusieurs, RESTAURANTS), (18_667, 0, 5600, 13_067));
    assert_eq!(ligne(&plusieurs, COURSES), (27_000, 0, 2000, 25_000));
    assert_eq!(ligne(&plusieurs, ABONNEMENTS), (1399, 1399, 0, 0));
    assert!(plusieurs.subscriptions[0].stopped);
    let r = &plusieurs.result;
    assert_eq!(
        (r.monthly, r.yearly, r.categories, r.subscriptions),
        (8999, 107_988, 7600, 1399)
    );
    assert_eq!(
        (r.expenses, r.remaining, r.savings, r.savings_rate_bp),
        (129_667, 106_999, 25_666, Some(1013))
    );
    assert_eq!(r.budget_delta, Some(19_667));

    let sans_arret = simule(&j, Mode::Calendaire, &[pourcent(ABONNEMENTS, 50)], &[]);
    assert_eq!(ligne(&sans_arret, ABONNEMENTS), (1399, 0, 700, 699));
    let bornes = simule(
        &j,
        Mode::Calendaire,
        &[pourcent(COURSES, 0), pourcent(LOGEMENT, 100)],
        &[],
    );
    assert_eq!(ligne(&bornes, COURSES), (27_000, 0, 0, 27_000));
    assert_eq!(ligne(&bornes, LOGEMENT), (80_000, 0, 80_000, 0));
    assert_eq!(bornes.result.yearly, 960_000);
}

#[test]
fn base_fragile_et_categorie_sans_historique() {
    let j = court();
    let s = simule(&j, Mode::Calendaire, &[pourcent(RESTAURANTS, 50)], &[]);
    assert_eq!(
        (s.basis.method, s.basis.periods, s.basis.fragile),
        ("current", 1, true)
    );
    assert_eq!(s.basis.period_ids, ["2026-01"]);
    assert_eq!(ligne(&s, COURSES), (12_000, 0, 0, 12_000));
    assert_eq!(ligne(&s, RESTAURANTS), (0, 0, 0, 0));
    assert_eq!(
        (
            s.reference.expenses,
            s.reference.remaining,
            s.result.monthly
        ),
        (12_000, 88_000, 0)
    );
    assert!(s.subscriptions.is_empty());
    let par_releve = simule(&j, Mode::Releve, &[], &[]);
    assert_eq!(
        (par_releve.basis.method, par_releve.basis.fragile),
        ("average", false)
    );
    assert_eq!(
        essai(&j, Mode::Calendaire, &[cible(RESTAURANTS, 100)], &[]).unwrap_err(),
        "target : centimes, de 0 à la dépense habituelle de la catégorie"
    );
    assert!(essai(&j, Mode::Calendaire, &[cible(RESTAURANTS, 0)], &[]).is_ok());
}

#[test]
fn validation_des_entrees_de_la_simulation() {
    let mut j = quatre();
    let refuse = |j: &Jeu, reglages: &[Reglage], arrets: &[&str]| {
        essai(j, Mode::Calendaire, reglages, arrets).unwrap_err()
    };
    for p in [-5, -1, 101, 1000] {
        assert_eq!(
            refuse(&j, &[pourcent(COURSES, p)], &[]),
            "percent : entier de 0 à 100",
            "{p}"
        );
    }
    for t in [-1, 27_001] {
        assert!(refuse(&j, &[cible(COURSES, t)], &[]).starts_with("target"));
    }
    assert!(essai(&j, Mode::Calendaire, &[cible(COURSES, 27_000)], &[]).is_ok());
    let deux = Reglage {
        id: COURSES,
        percent: Some(10),
        target: Some(1000),
    };
    let aucun = Reglage {
        id: COURSES,
        percent: None,
        target: None,
    };
    for r in [deux, aucun] {
        assert_eq!(
            refuse(&j, &[r], &[]),
            "categories : percent ou target, l'un des deux"
        );
    }
    for id in [999, EPARGNE, REVENUS, 0] {
        assert_eq!(
            refuse(&j, &[pourcent(id, 10)], &[]),
            "categories : catégorie de dépense inconnue ou archivée",
            "{id}"
        );
    }
    assert_eq!(
        refuse(&j, &[pourcent(COURSES, 10), cible(COURSES, 100)], &[]),
        "categories : une catégorie est citée deux fois"
    );
    for inconnu in ["9-monthly", "4-monthly", "abc", ""] {
        assert_eq!(
            refuse(&j, &[], &[inconnu]),
            "subscriptions : abonnement inconnu, arrêté ou écarté",
            "{inconnu}"
        );
    }
    assert_eq!(
        refuse(&j, &[], &["1-monthly", "1-monthly"]),
        "subscriptions : un abonnement est cité deux fois"
    );
    let trop: Vec<Reglage> = (0..101).map(|k| pourcent(1000 + k, 10)).collect();
    assert_eq!(
        refuse(&j, &trop, &[]),
        "categories et subscriptions : 100 éléments au plus"
    );
    j.d.categories
        .iter_mut()
        .find(|c| c.id == COURSES)
        .unwrap()
        .archivee = true;
    assert_eq!(
        refuse(&j, &[pourcent(COURSES, 10)], &[]),
        "categories : catégorie de dépense inconnue ou archivée"
    );
    assert!(
        simule(&j, Mode::Calendaire, &[], &[])
            .categories
            .iter()
            .all(|c| c.id != COURSES)
    );
}

#[test]
fn seuils_des_ecarts_cites() {
    assert!(significatif(1000, Some(1000)));
    assert!(significatif(-1000, Some(-1000)));
    assert!(!significatif(999, Some(5000)));
    assert!(!significatif(5000, Some(999)));
    assert!(!significatif(-5000, Some(-999)));
    assert!(significatif(1500, None));
    assert!(!significatif(500, None));
    assert!(!significatif(0, Some(0)));
}

#[test]
fn hausses_baisses_et_leviers_d_avril() {
    let j = quatre();
    for (mode, periode, avant) in [
        (Mode::Calendaire, "2026-04", "2026-03"),
        (Mode::Releve, "2026-04-30", "2026-03-31"),
    ] {
        let s = resume(&j, mode, None);
        assert_eq!((s.mode, s.period.id.as_str()), (mode.cle(), periode));
        assert_eq!((s.history, s.partial, s.operations), ("ok", false, 10));
        assert_eq!(s.previous_reason, None);
        let p = s.previous.as_ref().unwrap();
        assert_eq!((p.period_id.as_deref(), p.periods), (Some(avant), None));
        assert_eq!(
            p.up.iter().map(variation).collect::<Vec<_>>(),
            [
                ("Restaurants", 30_000, 14_001, 15_999, Some(11_427)),
                ("Courses", 35_000, 22_000, 13_000, Some(5909)),
            ]
        );
        assert_eq!(
            p.down.iter().map(variation).collect::<Vec<_>>(),
            [("Énergie", 4000, 6000, -2000, Some(-3333))]
        );
        assert_eq!((p.up_total, p.down_total), (2, 1));
        let u = s.usual.as_ref().unwrap();
        assert_eq!((u.period_id.as_deref(), u.periods), (None, Some(3)));
        assert_eq!(
            u.up.iter().map(variation).collect::<Vec<_>>(),
            [
                ("Restaurants", 30_000, 12_000, 18_000, Some(15_000)),
                ("Courses", 35_000, 22_000, 13_000, Some(5909)),
            ]
        );
        assert_eq!(
            u.down.iter().map(variation).collect::<Vec<_>>(),
            [("Énergie", 4000, 6000, -2000, Some(-3333))]
        );
        assert_eq!(s.expenses.total, 159_199);
        assert_eq!(
            s.expenses.previous,
            Some(Ecart {
                amount: 128_400,
                delta: 30_799,
                delta_bp: Some(2399),
            })
        );
        assert_eq!(
            s.expenses.usual,
            Some(Moyenne {
                amount: 126_399,
                periods: 3,
                delta: 32_800,
                delta_bp: Some(2595),
            })
        );
        assert_eq!(
            (s.expenses.budget, s.expenses.budget_delta),
            (Some(110_000), Some(49_199))
        );
        assert_eq!(
            (
                s.savings.total,
                s.savings.rate_bp,
                s.savings.previous_rate_bp
            ),
            (10_000, Some(385), Some(800))
        );
        assert_eq!(
            (
                s.unclassified.amount,
                s.unclassified.operations,
                s.unclassified.share_bp,
                s.unclassified.distorting
            ),
            (3000, 1, Some(188), false)
        );
        assert_eq!(
            s.levers.iter().map(levier).collect::<Vec<_>>(),
            [
                (
                    1,
                    "over_average",
                    Some("Restaurants"),
                    30_000,
                    12_000,
                    18_000,
                    216_000
                ),
                (2, "over_cap", Some("Courses"), 35_000, 30_000, 5000, 60_000),
                (
                    3,
                    "merchant_up",
                    Some("Ciné Phare"),
                    3900,
                    1000,
                    2900,
                    34_800
                ),
            ]
        );
        assert_eq!(s.levers_total, 3);
        assert_eq!(
            (s.levers[0].category_id, s.levers[0].periods),
            (Some(RESTAURANTS), Some(3))
        );
        assert_eq!(
            (s.levers[2].category_id, s.levers[2].merchant_id),
            (Some(LOISIRS), Some(CINE))
        );
    }
}

#[test]
fn leviers_des_abonnements_et_troncature_a_trois() {
    let mut j = vide();
    let mut dernier = 0;
    for (k, (m, s)) in MOIS.iter().zip(&SAISIES).enumerate() {
        let flux = if k < 2 { 1199 } else { 1399 };
        dernier = saisir(&mut j, *m, &Saisie { flux, ..*s });
        if k >= 2 {
            let date = format!("{}-08", &m.1[..7]);
            preleve(&mut j, dernier, &date, JEU, None, 699);
        }
    }
    j.op(dernier, "2026-04-21", None, 3000);
    let tous = classes(&j, None);
    assert_eq!(
        tous.iter().map(levier).collect::<Vec<_>>(),
        [
            (
                1,
                "over_average",
                Some("Restaurants"),
                30_000,
                12_000,
                18_000,
                216_000
            ),
            (2, "over_cap", Some("Courses"), 35_000, 30_000, 5000, 60_000),
            (
                3,
                "merchant_up",
                Some("Ciné Phare"),
                3900,
                1000,
                2900,
                34_800
            ),
            (4, "subscription_new", Some("Arcadia"), 699, 0, 699, 8388),
            (
                5,
                "subscription_changed",
                Some("Fluxo"),
                1399,
                1199,
                200,
                2400
            ),
        ]
    );
    assert_eq!(tous[3].recurring_id.as_deref(), Some("8-monthly"));
    assert_eq!(tous[4].recurring_id.as_deref(), Some("1-monthly"));
    let s = resume(&j, Mode::Calendaire, None);
    assert_eq!((s.levers.len(), s.levers_total), (3, 5));
    assert!(
        classes(&j, Some("2026-01"))
            .iter()
            .all(|l| !l.kind.starts_with("subscription"))
    );
    assert_eq!(
        classes(&j, Some("2026-03"))
            .iter()
            .filter(|l| l.kind.starts_with("subscription"))
            .count(),
        2
    );
}

#[test]
fn petits_achats_repetes_reduits_de_moitie() {
    let mut j = vide();
    let r = j.releve("2026-03-31", "2026-04-30");
    for quantieme in [3, 9, 17, 24] {
        let date = format!("2026-04-{quantieme:02}");
        carte(&mut j, r, &date, CAFE, RESTAURANTS, 500);
    }
    let s = resume(&j, Mode::Calendaire, None);
    assert_eq!(
        s.levers.iter().map(levier).collect::<Vec<_>>(),
        [(
            1,
            "small_purchases",
            Some("Café Tamis"),
            2000,
            24_333,
            1014,
            12_167
        )]
    );
    assert_eq!(
        (
            s.levers[0].count,
            s.levers[0].periods,
            s.levers[0].merchant_id
        ),
        (Some(4), Some(1), Some(CAFE))
    );
    assert_eq!(s.history, "first");
    assert_eq!(
        (s.previous.is_none(), s.previous_reason, s.usual.is_none()),
        (true, Some("none"), true)
    );
    assert_eq!(s.savings.previous_rate_bp, None);
}

#[test]
fn egalites_departagees_par_type_puis_par_nom() {
    const ZEBRE: i64 = 30;
    const ABEILLE: i64 = 31;
    let mut j = vide();
    j.d.categories
        .push(categorie(ZEBRE, "Zèbre", Nature::Variable, None));
    j.d.categories
        .push(categorie(ABEILLE, "Abeille", Nature::Variable, None));
    for (k, (debut, fin)) in MOIS.iter().enumerate() {
        let r = j.releve(debut, fin);
        let date = format!("{}-10", &fin[..7]);
        let (courses, autre) = if k == 3 {
            (35_000, 15_000)
        } else {
            (20_000, 10_000)
        };
        j.op(r, &date, Some(COURSES), courses);
        j.op(r, &date, Some(ZEBRE), autre);
        j.op(r, &date, Some(ABEILLE), autre);
    }
    assert_eq!(
        classes(&j, None)
            .iter()
            .map(|l| (l.rank, l.kind, l.name.as_deref(), l.yearly))
            .collect::<Vec<_>>(),
        [
            (1, "over_cap", Some("Courses"), 60_000),
            (2, "over_average", Some("Abeille"), 60_000),
            (3, "over_average", Some("Zèbre"), 60_000),
        ]
    );
}

#[test]
fn periode_en_cours_sans_baisse_et_periode_precedente_incomplete() {
    let mut j = quatre();
    let mai = j.releve("2026-04-30", "2026-05-10");
    carte(&mut j, mai, "2026-05-05", MARCHE, COURSES, 1000);
    let s = resume(&j, Mode::Calendaire, None);
    assert_eq!(
        (s.period.id.as_str(), s.partial, s.history),
        ("2026-05", true, "ok")
    );
    let p = s.previous.as_ref().unwrap();
    assert_eq!((p.up.len(), p.down.len(), p.down_total), (0, 0, 0));
    let u = s.usual.as_ref().unwrap();
    assert_eq!((u.periods, u.up.len(), u.down.len()), (Some(3), 0, 0));
    assert!(s.levers.is_empty());

    let mut j = court();
    let fevrier = j.releve("2026-01-31", "2026-02-28");
    carte(&mut j, fevrier, "2026-02-10", MARCHE, COURSES, 20_000);
    let s = resume(&j, Mode::Calendaire, None);
    assert_eq!(
        (s.period.id.as_str(), s.history),
        ("2026-02", "insufficient")
    );
    assert_eq!(
        (s.previous.is_none(), s.previous_reason, s.usual.is_none()),
        (true, Some("incomplete"), true)
    );
    assert_eq!(s.expenses.previous, None);
    let janvier = resume(&j, Mode::Calendaire, Some("2026-01"));
    assert_eq!(
        (janvier.history, janvier.previous_reason),
        ("first", Some("none"))
    );
    let par_releve = resume(&j, Mode::Releve, None);
    assert_eq!(par_releve.history, "ok");
    assert_eq!(
        par_releve
            .previous
            .as_ref()
            .unwrap()
            .up
            .iter()
            .map(variation)
            .collect::<Vec<_>>(),
        [("Courses", 20_000, 12_000, 8000, Some(6667))]
    );
}

#[test]
fn rien_a_dire_et_operations_a_classer_qui_faussent() {
    let mut j = vide();
    for (debut, fin) in &MOIS[..2] {
        let r = j.releve(debut, fin);
        j.op(r, &format!("{}-10", &fin[..7]), Some(COURSES), 20_000);
    }
    let s = resume(&j, Mode::Calendaire, None);
    assert_eq!(s.history, "ok");
    let p = s.previous.as_ref().unwrap();
    assert_eq!((p.up_total, p.down_total), (0, 0));
    assert_eq!(s.usual.as_ref().unwrap().periods, Some(1));
    assert_eq!((s.levers.len(), s.levers_total), (0, 0));
    assert!(!s.unclassified.distorting);

    let mut j = court();
    j.op(1, "2026-01-21", None, 5000);
    let s = resume(&j, Mode::Calendaire, None);
    assert_eq!(
        (
            s.unclassified.amount,
            s.unclassified.share_bp,
            s.unclassified.distorting
        ),
        (5000, Some(2941), true)
    );
}

async fn commercant(e: &Env, sub: &str, nom: &str) -> i64 {
    sqlx::query_scalar("INSERT INTO commercants (sub, nom) VALUES ($1, $2) RETURNING id")
        .bind(sub)
        .bind(nom)
        .fetch_one(e.state.pool())
        .await
        .unwrap()
}

async fn prelever(
    e: &Env,
    sub: &str,
    (compte, releve): (i64, i64),
    date: &str,
    commercant: i64,
    categorie: i64,
    montant: i64,
) {
    sqlx::query(
        "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur,
                                 section, type, libelle, montant, sens, categorie_id, origine, commercant_id)
         VALUES ($1, $2, $3, (SELECT count(*) FROM operations WHERE releve_id = $1), 1, $4, $4,
                 'AUTRES OPERATIONS', 'prelevement', 'ZZQ INVENTE', $5, 'debit', $6, 'operation', $7)",
    )
    .bind(releve)
    .bind(compte)
    .bind(sub)
    .bind(jour(date))
    .bind(montant)
    .bind(categorie)
    .bind(commercant)
    .execute(e.state.pool())
    .await
    .unwrap();
}

struct Semis {
    courses: i64,
    restaurants: i64,
    epargne: i64,
    flux: String,
}

async fn semer(e: &Env) -> Semis {
    let courses = categorie_de(e, "alice", "courses").await;
    let restaurants = categorie_de(e, "alice", "restaurants").await;
    let abonnements = categorie_de(e, "alice", "abonnements").await;
    let revenus = categorie_de(e, "alice", "revenus").await;
    let epargne = categorie_de(e, "alice", "epargne").await;
    let flux = commercant(e, "sub-alice", "ZZQ Flux").await;
    for (k, (courses_du_mois, restaurants_du_mois, salaire)) in [
        (24_000, 12_000, 250_000),
        (22_000, 14_001, 250_000),
        (35_000, 30_000, 260_000),
    ]
    .into_iter()
    .enumerate()
    {
        let (debut, fin) = MOIS[k + 1];
        let mois = &fin[..7];
        let r = releve_de(e, "sub-alice", i32::try_from(k).unwrap() + 2, debut, fin).await;
        let (le_10, le_15, le_28) = (
            format!("{mois}-10"),
            format!("{mois}-15"),
            format!("{mois}-28"),
        );
        let mut salaire = debit(&le_28, Some(revenus), salaire);
        salaire.sens = "credit";
        operations_de(
            e,
            "sub-alice",
            r,
            &[
                debit(&le_10, Some(courses), courses_du_mois),
                debit(&le_15, Some(restaurants), restaurants_du_mois),
                salaire,
            ],
        )
        .await;
        prelever(
            e,
            "sub-alice",
            r,
            &format!("{mois}-12"),
            flux,
            abonnements,
            1399,
        )
        .await;
    }
    let courses_bruno = categorie_de(e, "bruno", "courses").await;
    let bruno = releve_de(e, "sub-bruno", 4, "2026-03-31", "2026-04-30").await;
    operations_de(
        e,
        "sub-bruno",
        bruno,
        &[debit("2026-04-10", Some(courses_bruno), 77_700)],
    )
    .await;
    Semis {
        courses,
        restaurants,
        epargne,
        flux: format!("{flux}-monthly"),
    }
}

async fn simuler_par(e: &Env, qui: &str, corps: &Value) -> Value {
    let r = e.envoyer(qui, "POST", "/stats/simulate", corps).await;
    assert_eq!(r.statut, StatusCode::OK, "{corps} : {}", r.texte());
    r.json()
}

fn de<'a>(liste: &'a Value, cle: &str, id: i64) -> &'a Value {
    liste
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x[cle] == id)
        .unwrap()
}

#[tokio::test]
async fn routes_du_resume_et_de_la_simulation() {
    let Some(e) = env().await else { return };
    assert_eq!(
        e.get("alice", "/stats/summary").await.statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.envoyer("alice", "POST", "/stats/simulate", &json!({}))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    let s = semer(&e).await;

    let resume = lire(&e, "alice", "/stats/summary").await;
    assert_eq!(
        (resume["mode"].as_str(), resume["period"]["id"].as_str()),
        (Some("calendar"), Some("2026-04"))
    );
    assert_eq!(resume["history"], "ok");
    assert_eq!(resume["previous"]["period_id"], "2026-03");
    assert_eq!(resume["previous"]["up"][0]["category_id"], s.restaurants);
    assert_eq!(resume["previous"]["up"][0]["delta"], 15_999);
    assert_eq!(resume["previous"]["up"][1]["delta"], 13_000);
    assert_eq!(resume["usual"]["periods"], 2);
    assert_eq!(resume["expenses"]["total"], 66_399);
    assert_eq!(resume["expenses"]["budget"], Value::Null);
    assert_eq!(resume["levers_total"], 2);
    assert_eq!(resume["levers"][0]["kind"], "over_average");
    assert_eq!(resume["levers"][0]["category_id"], s.restaurants);
    assert_eq!(resume["levers"][0]["reference_amount"], 13_001);
    assert_eq!(resume["levers"][0]["yearly"], 16_999 * 12);
    assert_eq!(resume["levers"][1]["category_id"], s.courses);
    let par_releve = lire(
        &e,
        "alice",
        "/stats/summary?mode=statement&period=2026-03-31",
    )
    .await;
    assert_eq!(par_releve["period"]["id"], "2026-03-31");
    assert_eq!(par_releve["previous"]["period_id"], "2026-02-28");
    assert_eq!(par_releve["next_id"], "2026-04-30");
    let fevrier = lire(&e, "alice", "/stats/summary?period=2026-02").await;
    assert_eq!(fevrier["history"], "first");
    assert_eq!(fevrier["previous"], Value::Null);
    assert_eq!(fevrier["levers"].as_array().unwrap().len(), 0);
    for (requete, statut) in [
        ("mode=semaine", StatusCode::BAD_REQUEST),
        ("period=avril", StatusCode::BAD_REQUEST),
        ("mode=statement&period=2026-04", StatusCode::BAD_REQUEST),
        ("period=2020-01", StatusCode::NOT_FOUND),
    ] {
        assert_eq!(
            e.get("alice", &format!("/stats/summary?{requete}"))
                .await
                .statut,
            statut,
            "{requete}"
        );
    }

    let base = simuler_par(&e, "alice", &json!({})).await;
    assert_eq!(base["basis"]["method"], "average");
    assert_eq!(base["basis"]["periods"], 3);
    assert_eq!(base["basis"]["fragile"], false);
    assert_eq!(de(&base["categories"], "id", s.courses)["base"], 27_000);
    assert_eq!(de(&base["categories"], "id", s.restaurants)["base"], 18_667);
    assert_eq!(base["reference"]["expenses"], 47_066);
    assert_eq!(base["reference"]["remaining"], 206_267);
    assert_eq!(base["reference"]["budget"], Value::Null);
    assert_eq!(base["subscriptions"][0]["id"], s.flux.as_str());
    assert_eq!(base["subscriptions"][0]["yearly"], 16_788);
    assert_eq!(base["result"]["monthly"], 0);

    let corps = json!({
        "mode": "calendar",
        "categories": [{ "id": s.restaurants, "percent": 30 }],
        "subscriptions": [s.flux],
    });
    let simule = simuler_par(&e, "alice", &corps).await;
    let restaurants = de(&simule["categories"], "id", s.restaurants);
    assert_eq!(
        (
            entier(&restaurants["saving"]),
            entier(&restaurants["amount"])
        ),
        (5600, 13_067)
    );
    assert_eq!(simule["result"]["monthly"], 6999);
    assert_eq!(simule["result"]["yearly"], 83_988);
    assert_eq!(simule["result"]["expenses"], 40_067);
    assert_eq!(simule["result"]["remaining"], 213_266);
    assert_eq!(simule["result"]["savings_rate_bp"], 276);
    assert_eq!(simule["result"]["budget_delta"], Value::Null);
    let relu = simuler_par(&e, "alice", &json!({})).await;
    assert_eq!(relu, base);
    let par_releve = simuler_par(
        &e,
        "alice",
        &json!({ "mode": "statement", "period": "2026-03-31" }),
    )
    .await;
    assert_eq!(par_releve["basis"]["periods"], 2);
    assert_eq!(par_releve["period"], "2026-03-31");

    for corps in [
        json!({ "categories": [{ "id": s.restaurants, "percent": 101 }] }),
        json!({ "categories": [{ "id": s.restaurants, "percent": -1 }] }),
        json!({ "categories": [{ "id": s.restaurants, "percent": 2.5 }] }),
        json!({ "categories": [{ "id": s.restaurants, "percent": "30" }] }),
        json!({ "categories": [{ "id": s.restaurants, "percent": 10, "target": 100 }] }),
        json!({ "categories": [{ "id": s.restaurants }] }),
        json!({ "categories": [{ "id": s.restaurants, "target": 18_668 }] }),
        json!({ "categories": [{ "id": s.restaurants, "target": -1 }] }),
        json!({ "categories": [{ "id": s.restaurants, "percent": 5 }, { "id": s.restaurants, "percent": 10 }] }),
        json!({ "categories": [{ "id": s.epargne, "percent": 10 }] }),
        json!({ "categories": [{ "id": 999_999, "percent": 10 }] }),
        json!({ "categories": [{ "id": s.restaurants, "percent": 10, "autre": 1 }] }),
        json!({ "categories": { "id": s.restaurants } }),
        json!({ "subscriptions": ["0-monthly"] }),
        json!({ "subscriptions": [s.flux, s.flux] }),
        json!({ "subscriptions": [12] }),
        json!({ "mode": "semaine" }),
        json!({ "period": "avril" }),
        json!({ "autre": true }),
        json!([]),
    ] {
        let r = e.envoyer("alice", "POST", "/stats/simulate", &corps).await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
        assert_eq!(r.json()["error"], "invalid", "{corps}");
    }
    assert_eq!(
        e.envoyer(
            "alice",
            "POST",
            "/stats/simulate",
            &json!({ "period": "2020-01" })
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
    let operations: i64 = sqlx::query_scalar("SELECT count(*) FROM operations")
        .fetch_one(e.state.pool())
        .await
        .unwrap();
    assert_eq!(operations, 13);
}

#[tokio::test]
async fn fixer_les_cibles_simulees_comme_plafonds() {
    let Some(e) = env().await else { return };
    let s = semer(&e).await;
    let corps = json!({ "categories": [
        { "id": s.restaurants, "percent": 30 },
        { "id": s.courses, "target": 25_000 },
    ] });
    let simule = simuler_par(&e, "alice", &corps).await;
    for id in [s.restaurants, s.courses] {
        let cible = entier(&de(&simule["categories"], "id", id)["amount"]);
        let r = e
            .envoyer(
                "alice",
                "PATCH",
                &format!("/categories/{id}"),
                &json!({ "monthly_cap": cible }),
            )
            .await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        assert_eq!(r.json()["category"]["monthly_cap"], cible);
    }
    let apres = simuler_par(&e, "alice", &corps).await;
    assert_eq!(
        de(&apres["categories"], "id", s.restaurants)["monthly_cap"],
        13_067
    );
    assert_eq!(
        de(&apres["categories"], "id", s.courses)["monthly_cap"],
        25_000
    );
    assert_eq!(apres["reference"]["budget"], 38_067);
    assert_eq!(apres["result"]["expenses"], 47_066 - 7600);
    assert_eq!(apres["result"]["budget_delta"], 47_066 - 7600 - 38_067);
    let mois = lire(&e, "alice", "/stats/month").await;
    assert_eq!(mois["budget"]["total"], 38_067);
    let resume = lire(&e, "alice", "/stats/summary").await;
    assert_eq!(resume["expenses"]["budget_delta"], 66_399 - 38_067);
    assert_eq!(
        resume["levers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (
                l["kind"].as_str().unwrap(),
                entier(&l["category_id"]),
                entier(&l["reference_amount"]),
                entier(&l["monthly"])
            ))
            .collect::<Vec<_>>(),
        [
            ("over_cap", s.restaurants, 13_067, 16_933),
            ("over_cap", s.courses, 25_000, 10_000),
        ]
    );
}

#[tokio::test]
async fn decision_cloisonnee_par_personne() {
    let Some(e) = env().await else { return };
    let s = semer(&e).await;
    let bruno = lire(&e, "bruno", "/stats/summary").await;
    assert_eq!(bruno["history"], "first");
    assert_eq!(bruno["expenses"]["total"], 77_700);
    assert_eq!(bruno["levers"].as_array().unwrap().len(), 0);
    let base = simuler_par(&e, "bruno", &json!({})).await;
    assert_eq!(base["reference"]["expenses"], 77_700);
    assert_eq!(base["subscriptions"].as_array().unwrap().len(), 0);
    assert!(
        base["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != s.restaurants && c["id"] != s.courses)
    );
    for corps in [
        json!({ "categories": [{ "id": s.restaurants, "percent": 10 }] }),
        json!({ "subscriptions": [s.flux] }),
    ] {
        let r = e.envoyer("bruno", "POST", "/stats/simulate", &corps).await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
    }
    let r = e
        .envoyer(
            "bruno",
            "PATCH",
            &format!("/categories/{}", s.restaurants),
            &json!({ "monthly_cap": 100 }),
        )
        .await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
    let alice = simuler_par(&e, "alice", &json!({})).await;
    assert_eq!(
        de(&alice["categories"], "id", s.restaurants)["monthly_cap"],
        Value::Null
    );
    for (methode, chemin) in [("GET", "/stats/summary"), ("POST", "/stats/simulate")] {
        assert_eq!(
            e.envoyer("personne", methode, chemin, &json!({}))
                .await
                .statut,
            StatusCode::FORBIDDEN,
            "{chemin}"
        );
        assert_eq!(
            e.appel(None, methode, chemin).await.statut,
            StatusCode::UNAUTHORIZED,
            "{chemin}"
        );
    }
}

#[tokio::test]
async fn releves_reels_decision() {
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
    let nombre = |v: &Value| v.as_array().map_or(0, Vec::len);
    for mode in ["calendar", "statement"] {
        let liste = lire(&e, "alice", &format!("/stats/months?mode={mode}")).await;
        for p in liste["periods"].as_array().unwrap() {
            let id = p["id"].as_str().unwrap();
            let s = lire(
                &e,
                "alice",
                &format!("/stats/summary?mode={mode}&period={id}"),
            )
            .await;
            let m = lire(
                &e,
                "alice",
                &format!("/stats/month?mode={mode}&period={id}"),
            )
            .await;
            assert_eq!(s["expenses"]["total"], m["expenses"]["total"]);
            assert_eq!(s["unclassified"]["amount"], m["unclassified"]["amount"]);
            let leviers = s["levers"].as_array().unwrap();
            assert!(leviers.len() <= 3);
            assert!(i64::try_from(leviers.len()).unwrap() <= entier(&s["levers_total"]));
            for (k, l) in leviers.iter().enumerate() {
                assert_eq!(entier(&l["rank"]), i64::try_from(k).unwrap() + 1);
                assert!(entier(&l["yearly"]) > 0 && entier(&l["monthly"]) > 0);
                if k > 0 {
                    assert!(entier(&leviers[k - 1]["yearly"]) >= entier(&l["yearly"]));
                }
            }
            let base = simuler_par(&e, "alice", &json!({ "mode": mode, "period": id })).await;
            let reglages: Vec<Value> = base["categories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| json!({ "id": c["id"], "percent": 10 }))
                .collect();
            let arrets: Vec<Value> = base["subscriptions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["id"].clone())
                .collect();
            let simule = simuler_par(
                &e,
                "alice",
                &json!({ "mode": mode, "period": id, "categories": reglages, "subscriptions": arrets }),
            )
            .await;
            let economies: i64 = simule["categories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| entier(&c["saving"]))
                .sum();
            let abonnements: i64 = simule["subscriptions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| entier(&a["monthly"]))
                .sum();
            let r = &simule["result"];
            assert_eq!(entier(&r["categories"]), economies);
            assert_eq!(entier(&r["monthly"]), economies + abonnements);
            assert_eq!(
                entier(&r["expenses"]),
                entier(&simule["reference"]["expenses"]) - entier(&r["monthly"])
            );
            assert_eq!(
                entier(&r["remaining"]),
                entier(&simule["reference"]["remaining"]) + entier(&r["monthly"])
            );
            eprintln!(
                "relevés réels, décision, mode {mode} : historique {}, {} hausse(s) et {} baisse(s) citées, {} levier(s) sur {}, à classer {}, base sur {} période(s){}, {} catégorie(s) ajustable(s), {} abonnement(s)",
                s["history"].as_str().unwrap(),
                nombre(&s["previous"]["up"]) + nombre(&s["usual"]["up"]),
                nombre(&s["previous"]["down"]) + nombre(&s["usual"]["down"]),
                leviers.len(),
                s["levers_total"],
                if s["unclassified"]["distorting"] == true {
                    "qui fausse le résumé"
                } else {
                    "sans effet notable"
                },
                base["basis"]["periods"],
                if base["basis"]["fragile"] == true {
                    " (fragile)"
                } else {
                    ""
                },
                base["categories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|c| entier(&c["base"]) > 0)
                    .count(),
                arrets.len(),
            );
        }
    }
}
