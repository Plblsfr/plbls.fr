use axum::http::StatusCode;
use serde_json::Value;

use crate::{
    bilan::Nature,
    evolution::{
        Annees, Fenetre, Jour, Mouvement, ReleveSolde, Tendance, Trou, annees, historique, soldes,
        tendance,
    },
    stats::{Calcul, Ecart, Mode},
    tests_stats::{
        COURSES, ENERGIE, EPARGNE, Jeu, LOGEMENT, RESTAURANTS, REVENUS, categorie, entier, jour,
        lire, semer,
    },
    testutil::env,
};

const TRANSPORTS: i64 = 8;
const SANTE: i64 = 9;
const LOISIRS: i64 = 10;
const DIVERS: i64 = 11;

fn neuf_mois() -> Jeu {
    let mut j = Jeu::nouveau();
    for (id, nom) in [
        (TRANSPORTS, "Transports"),
        (SANTE, "Santé"),
        (LOISIRS, "Loisirs"),
        (DIVERS, "Divers"),
    ] {
        j.d.categories
            .push(categorie(id, nom, Nature::Variable, None));
    }
    let fins = [
        "2025-12-31",
        "2026-01-31",
        "2026-02-28",
        "2026-03-31",
        "2026-04-30",
        "2026-05-31",
        "2026-06-30",
        "2026-07-31",
        "2026-08-31",
    ];
    for m in 1..=8 {
        let r = j.releve(fins[m - 1], fins[m]);
        let mois = format!("2026-{m:02}");
        j.op(r, &format!("{mois}-05"), Some(LOGEMENT), 80_000);
        j.op(
            r,
            &format!("{mois}-10"),
            Some(COURSES),
            18_000 + 2000 * i64::try_from(m).unwrap(),
        );
        j.op(r, &format!("{mois}-15"), Some(RESTAURANTS), 5000);
        j.credit(r, &format!("{mois}-28"), Some(REVENUS), 250_000);
        match m {
            3 => {
                j.op(r, "2026-03-20", Some(EPARGNE), 30_000);
            }
            4 => {
                j.op(r, "2026-04-12", Some(ENERGIE), 6000);
                j.op(r, "2026-04-13", Some(TRANSPORTS), 4000);
                j.op(r, "2026-04-14", Some(SANTE), 3000);
                j.op(r, "2026-04-15", Some(LOISIRS), 2000);
                j.op(r, "2026-04-16", Some(DIVERS), 1000);
            }
            5 => {
                j.op(r, "2026-05-18", Some(RESTAURANTS), 4000);
                j.credit(r, "2026-05-19", Some(RESTAURANTS), 1000);
            }
            8 => {
                j.op(r, "2026-08-20", None, 1500);
            }
            _ => {}
        }
    }
    let septembre = j.releve("2026-08-31", "2026-09-10");
    j.op(septembre, "2026-09-05", Some(LOGEMENT), 80_000);
    j.op(septembre, "2026-09-08", Some(COURSES), 5000);
    j
}

fn ligne(h: &crate::evolution::Historique, id: i64) -> &crate::evolution::LigneHistorique {
    h.categories.iter().find(|l| l.id == id).unwrap()
}

#[test]
fn fenetres_lues_et_appliquees() {
    assert_eq!(Fenetre::lire(None), Some(Fenetre::Periodes(12)));
    assert_eq!(Fenetre::lire(Some("6")), Some(Fenetre::Periodes(6)));
    assert_eq!(Fenetre::lire(Some("24")), Some(Fenetre::Periodes(24)));
    assert_eq!(Fenetre::lire(Some("all")), Some(Fenetre::Tout));
    for refuse in ["7", "0", "tout", "-6", "6 "] {
        assert_eq!(Fenetre::lire(Some(refuse)), None, "{refuse}");
    }
    let j = neuf_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let ids = |f: Fenetre| -> Vec<String> {
        historique(&c, f)
            .periods
            .into_iter()
            .map(|p| p.id)
            .collect()
    };
    assert_eq!(
        ids(Fenetre::Periodes(6)),
        [
            "2026-04", "2026-05", "2026-06", "2026-07", "2026-08", "2026-09"
        ]
    );
    for large in [Fenetre::Periodes(12), Fenetre::Periodes(24), Fenetre::Tout] {
        let liste = ids(large);
        assert_eq!(liste.len(), 9);
        assert_eq!(liste[0], "2026-01");
    }
    let h = historique(&c, Fenetre::Periodes(6));
    assert_eq!((h.window.as_str(), h.available, h.complete), ("6", 9, 5));
    assert_eq!(historique(&c, Fenetre::Tout).window, "all");
    let r = Calcul::nouveau(&j.d, Mode::Releve);
    let h = historique(&r, Fenetre::Periodes(6));
    assert_eq!(h.mode, "statement");
    assert_eq!(h.periods[0].id, "2026-04-30");
    assert_eq!(h.periods[5].id, "2026-09-10");
    assert_eq!(h.complete, 6);
}

#[test]
fn historique_par_categorie_totaux_et_plafonds() {
    let j = neuf_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let h = historique(&c, Fenetre::Periodes(6));
    let courses = ligne(&h, COURSES);
    assert_eq!(
        courses.amounts,
        [26_000, 28_000, 30_000, 32_000, 34_000, 5000]
    );
    assert_eq!(
        (courses.total, courses.average, courses.over_cap),
        (155_000, Some(30_000), Some(2))
    );
    let logement = ligne(&h, LOGEMENT);
    assert_eq!(logement.amounts, [80_000; 6]);
    assert_eq!(
        (logement.total, logement.average, logement.over_cap),
        (480_000, Some(80_000), Some(0))
    );
    let restaurants = ligne(&h, RESTAURANTS);
    assert_eq!(restaurants.amounts, [5000, 8000, 5000, 5000, 5000, 0]);
    assert_eq!(
        (restaurants.total, restaurants.average, restaurants.over_cap),
        (28_000, Some(5600), None)
    );
    assert_eq!(h.unclassified.amounts, [0, 0, 0, 0, 1500, 0]);
    assert_eq!(
        (h.unclassified.total, h.unclassified.average),
        (1500, Some(300))
    );
    assert_eq!(
        h.total.amounts,
        [127_000, 116_000, 115_000, 117_000, 120_500, 85_000]
    );
    assert_eq!(
        (
            h.total.total,
            h.total.average,
            h.total.budget,
            h.total.over_budget
        ),
        (680_500, Some(119_100), Some(110_000), Some(5))
    );
    assert_eq!(
        h.categories.iter().map(|l| l.id).collect::<Vec<_>>(),
        [
            LOGEMENT,
            COURSES,
            RESTAURANTS,
            ENERGIE,
            TRANSPORTS,
            SANTE,
            LOISIRS,
            DIVERS
        ]
    );
    assert!(
        h.categories
            .iter()
            .all(|l| l.id != EPARGNE && l.id != REVENUS)
    );
    assert_eq!(
        h.chart.category_ids,
        [LOGEMENT, COURSES, RESTAURANTS, ENERGIE, TRANSPORTS]
    );
    assert_eq!(h.chart.others, [6000, 0, 0, 0, 0, 0]);
    assert_eq!(h.chart.others_categories, 3);
    assert_eq!(
        h.periods
            .iter()
            .map(|p| (p.complete, p.open))
            .collect::<Vec<_>>(),
        [
            (true, false),
            (true, false),
            (true, false),
            (true, false),
            (true, false),
            (false, true)
        ]
    );
}

#[test]
fn somme_des_categories_egale_aux_depenses_du_mois() {
    let j = neuf_mois();
    for mode in [Mode::Calendaire, Mode::Releve] {
        let c = Calcul::nouveau(&j.d, mode);
        for fenetre in [Fenetre::Periodes(6), Fenetre::Tout] {
            let h = historique(&c, fenetre);
            for (k, p) in h.periods.iter().enumerate() {
                let i = c.index(Some(&p.id)).unwrap();
                let mois = c.mois(i);
                let categories: i64 = h.categories.iter().map(|l| l.amounts[k]).sum();
                assert_eq!(categories + h.unclassified.amounts[k], h.total.amounts[k]);
                assert_eq!(h.total.amounts[k], mois.expenses.total);
                assert_eq!(p.expenses, mois.expenses.total);
                assert_eq!(h.unclassified.amounts[k], mois.unclassified.amount);
                for l in &mois.categories {
                    assert_eq!(ligne(&h, l.id).amounts[k], l.amount);
                }
                let graphique: i64 = h
                    .chart
                    .category_ids
                    .iter()
                    .map(|id| ligne(&h, *id).amounts[k])
                    .sum();
                assert_eq!(
                    graphique + h.chart.others[k] + h.unclassified.amounts[k],
                    h.total.amounts[k]
                );
            }
            assert_eq!(h.total.total, h.total.amounts.iter().sum::<i64>());
        }
    }
}

fn pente(
    direction: &'static str,
    average: i64,
    delta: i64,
    delta_bp: Option<i64>,
) -> (&'static str, Option<i64>, Option<i64>, Option<i64>) {
    (direction, Some(average), Some(delta), delta_bp)
}

fn resume(t: &Tendance) -> (&'static str, Option<i64>, Option<i64>, Option<i64>) {
    (t.direction, t.average, t.delta, t.delta_bp)
}

#[test]
fn tendance_hausse_stable_baisse_et_donnees_insuffisantes() {
    let hausse = tendance(&[10_000, 12_000, 14_000, 16_000, 18_000, 20_000]);
    assert_eq!(resume(&hausse), pente("up", 15_000, 10_000, Some(6667)));
    assert_eq!(hausse.periods, 6);
    assert_eq!(
        resume(&tendance(&[30_000, 26_000, 25_000, 20_000])),
        pente("down", 25_250, -9500, Some(-3762))
    );
    assert_eq!(
        resume(&tendance(&[10_000, 10_000, 50_000, 10_000, 10_000, 10_000])),
        pente("stable", 16_667, 0, Some(0))
    );
    assert_eq!(
        resume(&tendance(&[20_000, 20_500, 21_000])),
        pente("stable", 20_500, 1000, Some(488))
    );
    assert_eq!(
        resume(&tendance(&[2000, 2300, 2600])),
        pente("stable", 2300, 600, Some(2609))
    );
    assert_eq!(
        resume(&tendance(&[20_000, 21_000, 22_000])),
        pente("stable", 21_000, 2000, Some(952))
    );
    assert_eq!(
        resume(&tendance(&[20_000, 21_100, 22_200])),
        pente("up", 21_100, 2200, Some(1043))
    );
    assert_eq!(resume(&tendance(&[0, 0, 0])), pente("stable", 0, 0, None));
    for court in [&[][..], &[5000], &[5000, 9000]] {
        let t = tendance(court);
        assert_eq!(resume(&t), ("insufficient", None, None, None));
        assert_eq!(t.periods, court.len());
    }
}

#[test]
fn tendance_sur_les_six_dernieres_periodes_completes() {
    let j = neuf_mois();
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    for fenetre in [Fenetre::Periodes(6), Fenetre::Tout] {
        let h = historique(&c, fenetre);
        assert_eq!(
            h.trend_period_ids,
            [
                "2026-03", "2026-04", "2026-05", "2026-06", "2026-07", "2026-08"
            ]
        );
        let courses = &ligne(&h, COURSES).trend;
        assert_eq!(
            courses.values,
            [24_000, 26_000, 28_000, 30_000, 32_000, 34_000]
        );
        assert_eq!(resume(courses), pente("up", 29_000, 10_000, Some(3448)));
        assert_eq!(
            resume(&ligne(&h, LOGEMENT).trend),
            pente("stable", 80_000, 0, Some(0))
        );
        assert_eq!(
            resume(&ligne(&h, RESTAURANTS).trend),
            pente("stable", 5500, 0, Some(0))
        );
        assert_eq!(
            h.total.trend.values,
            [109_000, 127_000, 116_000, 115_000, 117_000, 120_500]
        );
        assert_eq!(
            resume(&h.total.trend),
            pente("stable", 117_417, 10_000, Some(852))
        );
    }
    let mut court = Jeu::nouveau();
    let janvier = court.releve("2025-12-31", "2026-01-31");
    court.op(janvier, "2026-01-10", Some(COURSES), 20_000);
    let fevrier = court.releve("2026-01-31", "2026-02-28");
    court.op(fevrier, "2026-02-10", Some(COURSES), 40_000);
    let c = Calcul::nouveau(&court.d, Mode::Calendaire);
    let h = historique(&c, Fenetre::Periodes(12));
    assert_eq!(ligne(&h, COURSES).trend.direction, "insufficient");
    assert_eq!(ligne(&h, COURSES).trend.values, [20_000, 40_000]);
    assert_eq!(h.total.trend.direction, "insufficient");
    assert_eq!(ligne(&h, COURSES).over_cap, Some(1));
    let vide = Jeu::nouveau();
    let c = Calcul::nouveau(&vide.d, Mode::Calendaire);
    let h = historique(&c, Fenetre::Tout);
    assert!(h.periods.is_empty() && h.total.amounts.is_empty());
    assert_eq!(
        (h.total.average, h.total.trend.direction),
        (None, "insufficient")
    );
}

fn releve(id: i64, compte: i64, debut: &str, fin: &str, ancien: i64, nouveau: i64) -> ReleveSolde {
    ReleveSolde {
        id,
        compte,
        debut: jour(debut),
        fin: jour(fin),
        ancien,
        nouveau,
    }
}

fn mouvement(releve: i64, date: &str, credit: bool, montant: i64) -> Mouvement {
    Mouvement {
        releve,
        jour: jour(date),
        credit,
        montant,
    }
}

fn point(date: &str, amount: i64) -> Option<Jour> {
    Some(Jour {
        date: jour(date),
        amount,
    })
}

fn deux_releves() -> (Vec<ReleveSolde>, Vec<Mouvement>) {
    (
        vec![
            releve(1, 7, "2026-01-31", "2026-02-28", 100_000, 274_000),
            releve(2, 7, "2026-03-31", "2026-04-30", 250_000, 50_000),
        ],
        vec![
            mouvement(1, "2026-01-30", false, 1000),
            mouvement(1, "2026-02-05", false, 30_000),
            mouvement(1, "2026-02-05", true, 5000),
            mouvement(1, "2026-02-20", true, 200_000),
            mouvement(2, "2026-04-10", false, 300_000),
            mouvement(2, "2026-04-25", true, 100_000),
        ],
    )
}

#[test]
fn solde_jour_par_jour_et_trous_entre_releves() {
    let (releves, mouvements) = deux_releves();
    let comptes = soldes(
        &releves,
        &mouvements,
        jour("2026-02-01"),
        jour("2026-04-30"),
    );
    assert_eq!(comptes.len(), 1);
    let s = &comptes[0];
    assert_eq!((s.account_id, s.days, s.reconciled), (7, 59, true));
    assert_eq!(s.segments.len(), 2);
    let fevrier = &s.segments[0];
    assert_eq!(
        (fevrier.from, fevrier.to),
        (jour("2026-02-01"), jour("2026-02-28"))
    );
    assert_eq!(fevrier.values.len(), 28);
    assert_eq!(fevrier.values[..4], [99_000; 4]);
    assert_eq!(fevrier.values[4..19], [74_000; 15]);
    assert_eq!(fevrier.values[19..], [274_000; 9]);
    let avril = &s.segments[1];
    assert_eq!(
        (avril.from, avril.to),
        (jour("2026-03-31"), jour("2026-04-30"))
    );
    assert_eq!(avril.values.len(), 31);
    assert_eq!(avril.values[..10], [250_000; 10]);
    assert_eq!(avril.values[10..25], [-50_000; 15]);
    assert_eq!(avril.values[25..], [50_000; 6]);
    assert_eq!(avril.values.last(), Some(&releves[1].nouveau));
    assert_eq!(
        s.gaps,
        [Trou {
            from: jour("2026-03-01"),
            to: jour("2026-03-30"),
            days: 30
        }]
    );
    assert_eq!(s.first, point("2026-02-01", 99_000));
    assert_eq!(s.last, point("2026-04-30", 50_000));
    assert_eq!(s.min, point("2026-04-10", -50_000));
    assert_eq!(s.max, point("2026-02-20", 274_000));
}

#[test]
fn solde_borne_a_la_fenetre_et_par_compte() {
    let (mut releves, mut mouvements) = deux_releves();
    let large = soldes(
        &releves,
        &mouvements,
        jour("2026-01-01"),
        jour("2026-05-10"),
    );
    assert_eq!(large[0].days, 60);
    assert_eq!(large[0].segments[0].from, jour("2026-01-31"));
    assert_eq!(large[0].segments[0].values[0], 100_000);
    assert_eq!(
        large[0]
            .gaps
            .iter()
            .map(|t| (t.from, t.to, t.days))
            .collect::<Vec<_>>(),
        [
            (jour("2026-01-01"), jour("2026-01-30"), 30),
            (jour("2026-03-01"), jour("2026-03-30"), 30),
            (jour("2026-05-01"), jour("2026-05-10"), 10),
        ]
    );
    let etroit = soldes(
        &releves,
        &mouvements,
        jour("2026-02-10"),
        jour("2026-02-21"),
    );
    assert_eq!(etroit[0].segments.len(), 1);
    assert_eq!(etroit[0].segments[0].values.len(), 12);
    assert!(etroit[0].gaps.is_empty());
    assert_eq!(etroit[0].min, point("2026-02-10", 74_000));
    assert_eq!(etroit[0].max, point("2026-02-20", 274_000));
    let hors = soldes(
        &releves,
        &mouvements,
        jour("2026-03-05"),
        jour("2026-03-20"),
    );
    assert!(hors[0].segments.is_empty());
    assert_eq!(
        (hors[0].days, hors[0].min.clone(), hors[0].last.clone()),
        (0, None, None)
    );
    assert_eq!(hors[0].gaps.len(), 1);
    releves.push(releve(3, 9, "2026-02-28", "2026-03-31", -2000, 3000));
    releves.push(releve(4, 7, "2026-02-28", "2026-03-31", 274_000, 250_000));
    mouvements.push(mouvement(3, "2026-03-15", true, 5000));
    mouvements.push(mouvement(4, "2026-03-31", false, 24_000));
    let deux = soldes(
        &releves,
        &mouvements,
        jour("2026-02-01"),
        jour("2026-04-30"),
    );
    assert_eq!(deux.len(), 2);
    assert_eq!(
        (deux[0].account_id, deux[0].segments.len(), deux[0].days),
        (7, 1, 89)
    );
    assert!(deux[0].gaps.is_empty() && deux[0].reconciled);
    assert_eq!(deux[0].segments[0].values[27..58], [274_000; 31]);
    assert_eq!(deux[0].segments[0].values[58..68], [250_000; 10]);
    assert_eq!(deux[1].account_id, 9);
    assert_eq!(deux[1].first, point("2026-02-28", -2000));
    assert_eq!(deux[1].last, point("2026-03-31", 3000));
    assert_eq!(deux[1].gaps.len(), 2);
    releves[0].nouveau = 274_001;
    assert!(
        !soldes(
            &releves,
            &mouvements,
            jour("2026-02-01"),
            jour("2026-04-30")
        )[0]
        .reconciled
    );
    assert!(soldes(&[], &[], jour("2026-02-01"), jour("2026-04-30")).is_empty());
}

fn deux_annees() -> Jeu {
    let mut j = Jeu::nouveau();
    let r = j.releve("2024-12-31", "2025-01-31");
    j.op(r, "2025-01-05", Some(LOGEMENT), 70_000);
    j.op(r, "2025-01-10", Some(COURSES), 20_000);
    let r = j.releve("2025-01-31", "2025-02-28");
    j.op(r, "2025-02-05", Some(LOGEMENT), 70_000);
    j.op(r, "2025-02-10", Some(COURSES), 25_000);
    j.op(r, "2025-02-11", None, 2000);
    let r = j.releve("2025-11-30", "2025-12-31");
    j.op(r, "2025-12-05", Some(LOGEMENT), 80_000);
    j.op(r, "2025-12-10", Some(COURSES), 30_000);
    let r = j.releve("2025-12-31", "2026-01-31");
    j.carte(r, "2026-01-02", "2025-12-30", RESTAURANTS, 3000);
    j.op(r, "2026-01-05", Some(LOGEMENT), 80_000);
    j.op(r, "2026-01-10", Some(COURSES), 22_000);
    let r = j.releve("2026-01-31", "2026-02-28");
    j.op(r, "2026-02-05", Some(LOGEMENT), 80_000);
    j.op(r, "2026-02-10", Some(COURSES), 21_000);
    j.op(r, "2026-02-12", Some(RESTAURANTS), 4000);
    j
}

fn ecart(amount: i64, delta: i64, delta_bp: Option<i64>) -> Option<Ecart> {
    Some(Ecart {
        amount,
        delta,
        delta_bp,
    })
}

fn annee(j: &Jeu, mode: Mode, cle: &str) -> Annees {
    let c = Calcul::nouveau(&j.d, mode);
    annees(&c, c.index(Some(cle)).unwrap())
}

fn cumul(c: &crate::evolution::Comparaison) -> Option<(i64, i64, i64, Option<i64>)> {
    c.ytd_compared
        .as_ref()
        .map(|x| (x.current, x.previous, x.delta, x.delta_bp))
}

#[test]
fn annee_sur_l_autre_meme_mois_et_cumul() {
    let j = deux_annees();
    let a = annee(&j, Mode::Calendaire, "2026-02");
    assert_eq!((a.year, a.month, a.previous_year), (2026, 2, 2025));
    assert!(a.has_previous && !a.partial);
    assert_eq!(a.compared_months, [1, 2]);
    assert_eq!(a.total.month, 105_000);
    assert_eq!(a.total.month_previous, ecart(97_000, 8000, Some(825)));
    assert_eq!(a.total.ytd, 207_000);
    assert_eq!(
        cumul(&a.total),
        Some((207_000, 187_000, 20_000, Some(1070)))
    );
    assert_eq!(
        a.categories.iter().map(|l| l.id).collect::<Vec<_>>(),
        [LOGEMENT, COURSES, RESTAURANTS]
    );
    let courses = &a.categories[1].comparaison;
    assert_eq!(courses.month, 21_000);
    assert_eq!(courses.month_previous, ecart(25_000, -4000, Some(-1600)));
    assert_eq!(cumul(courses), Some((43_000, 45_000, -2000, Some(-444))));
    let restaurants = &a.categories[2].comparaison;
    assert_eq!(restaurants.month_previous, ecart(0, 4000, None));
    assert_eq!(cumul(restaurants), Some((4000, 0, 4000, None)));
    assert_eq!(
        a.unclassified.month_previous,
        ecart(2000, -2000, Some(-10_000))
    );
    assert_eq!(a.months.len(), 12);
    assert_eq!(
        (a.months[0].current, a.months[0].previous),
        (Some(102_000), Some(90_000))
    );
    assert_eq!(
        (a.months[1].current, a.months[1].previous),
        (Some(105_000), Some(97_000))
    );
    assert_eq!((a.months[2].current, a.months[2].previous), (None, None));
    assert_eq!(
        (a.months[11].current, a.months[11].previous),
        (None, Some(113_000))
    );
    assert_eq!(a.months[11].previous_complete, Some(true));
    let somme: i64 = a.categories.iter().map(|l| l.comparaison.ytd).sum();
    assert_eq!(somme + a.unclassified.ytd, a.total.ytd);
}

#[test]
fn annee_precedente_absente_et_passage_d_annee() {
    let j = deux_annees();
    let decembre = annee(&j, Mode::Calendaire, "2025-12");
    assert_eq!(
        (decembre.year, decembre.month, decembre.previous_year),
        (2025, 12, 2024)
    );
    assert!(!decembre.has_previous);
    assert!(decembre.compared_months.is_empty());
    assert_eq!(decembre.total.month, 113_000);
    assert_eq!(decembre.total.month_previous, None);
    assert_eq!(decembre.total.ytd, 300_000);
    assert_eq!(decembre.total.ytd_compared, None);
    assert!(decembre.months.iter().all(|m| m.previous.is_none()));
    assert_eq!(
        decembre
            .categories
            .iter()
            .find(|l| l.id == RESTAURANTS)
            .unwrap()
            .comparaison
            .month,
        3000
    );
    let janvier = annee(&j, Mode::Calendaire, "2026-01");
    assert_eq!((janvier.year, janvier.month), (2026, 1));
    assert_eq!(janvier.total.ytd, 102_000);
    assert_eq!(
        janvier.total.month_previous,
        ecart(90_000, 12_000, Some(1333))
    );
    assert_eq!(
        cumul(&janvier.total),
        Some((102_000, 90_000, 12_000, Some(1333)))
    );
    assert!(janvier.categories.iter().all(|l| l.id != RESTAURANTS));
    assert_eq!(janvier.months[1].current, None);
    assert_eq!(janvier.months[11].previous, Some(113_000));
    let par_releve = annee(&j, Mode::Releve, "2026-01-31");
    assert_eq!((par_releve.year, par_releve.month), (2026, 1));
    assert_eq!(par_releve.total.month, 105_000);
    assert_eq!(par_releve.months[11].previous, Some(110_000));
    let mut partiel = deux_annees();
    let mars = partiel.releve("2026-02-28", "2026-03-10");
    partiel.op(mars, "2026-03-05", Some(LOGEMENT), 80_000);
    let a = annee(&partiel, Mode::Calendaire, "2026-03");
    assert_eq!(a.compared_months, [1, 2]);
    assert_eq!(a.total.month_previous, None);
    assert_eq!(a.total.ytd, 287_000);
    assert_eq!(
        cumul(&a.total),
        Some((207_000, 187_000, 20_000, Some(1070)))
    );
    assert_eq!(a.months[2].current_complete, Some(false));
}

async fn soldes_de(e: &crate::testutil::Env, sub: &str, numero: i32, ancien: i64, nouveau: i64) {
    sqlx::query(
        "UPDATE releves SET ancien_solde = $3, nouveau_solde = $4 WHERE sub = $1 AND numero = $2",
    )
    .bind(sub)
    .bind(numero)
    .bind(ancien)
    .bind(nouveau)
    .execute(e.state.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn routes_de_l_evolution() {
    let Some(e) = env().await else { return };
    let vide = lire(&e, "alice", "/stats/history").await;
    assert_eq!(vide["periods"].as_array().unwrap().len(), 0);
    assert_eq!(
        lire(&e, "alice", "/stats/balance").await["accounts"],
        Value::Array(vec![])
    );
    assert_eq!(
        e.get("alice", "/stats/years").await.statut,
        StatusCode::NOT_FOUND
    );
    let s = semer(&e).await;
    soldes_de(&e, "sub-alice", 3, 10_000, 120_000).await;
    soldes_de(&e, "sub-alice", 4, 120_000, 17_500).await;
    let h = lire(&e, "alice", "/stats/history").await;
    assert_eq!(
        (h["mode"].as_str(), h["window"].as_str()),
        (Some("calendar"), Some("12"))
    );
    assert_eq!(h["periods"].as_array().unwrap().len(), 2);
    assert_eq!(h["total"]["amounts"], serde_json::json!([95_000, 97_500]));
    assert_eq!(h["unclassified"]["amounts"], serde_json::json!([0, 1500]));
    assert_eq!(h["categories"][0]["id"], s.logement);
    assert_eq!(
        h["categories"][0]["amounts"],
        serde_json::json!([70_000, 70_000])
    );
    assert_eq!(h["categories"][1]["id"], s.courses);
    assert_eq!(
        h["categories"][1]["amounts"],
        serde_json::json!([25_000, 26_000])
    );
    assert_eq!(h["categories"][1]["trend"]["direction"], "insufficient");
    assert_eq!(h["total"]["budget"], Value::Null);
    for (k, p) in h["periods"].as_array().unwrap().iter().enumerate() {
        let mois = lire(
            &e,
            "alice",
            &format!("/stats/month?period={}", p["id"].as_str().unwrap()),
        )
        .await;
        assert_eq!(h["total"]["amounts"][k], mois["expenses"]["total"]);
        let somme: i64 = h["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| entier(&c["amounts"][k]))
            .sum();
        assert_eq!(
            somme + entier(&h["unclassified"]["amounts"][k]),
            entier(&mois["expenses"]["total"])
        );
    }
    let par_releve = lire(&e, "alice", "/stats/history?mode=statement&window=6").await;
    assert_eq!(par_releve["periods"][1]["id"], "2026-04-30");
    assert_eq!(
        par_releve["total"]["amounts"],
        serde_json::json!([90_000, 102_500])
    );
    let b = lire(&e, "alice", "/stats/balance").await;
    assert_eq!(
        (b["from"].as_str(), b["to"].as_str()),
        (Some("2026-03-01"), Some("2026-04-30"))
    );
    let compte = &b["accounts"][0];
    assert_eq!(b["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(
        (entier(&compte["days"]), compte["reconciled"].as_bool()),
        (61, Some(true))
    );
    assert_eq!(compte["segments"].as_array().unwrap().len(), 1);
    assert_eq!(compte["gaps"].as_array().unwrap().len(), 0);
    assert_eq!(compte["segments"][0]["values"][30], 120_000);
    assert_eq!(
        compte["last"],
        serde_json::json!({ "date": "2026-04-30", "amount": 17_500 })
    );
    assert_eq!(
        compte["min"],
        serde_json::json!({ "date": "2026-03-12", "amount": -80_000 })
    );
    assert_eq!(
        compte["max"],
        serde_json::json!({ "date": "2026-03-28", "amount": 120_000 })
    );
    let b = lire(&e, "alice", "/stats/balance?mode=statement&window=all").await;
    assert_eq!(b["from"], "2026-02-28");
    assert_eq!(b["accounts"][0]["days"], 62);
    assert_eq!(
        b["accounts"][0]["first"],
        serde_json::json!({ "date": "2026-02-28", "amount": 10_000 })
    );
    let a = lire(&e, "alice", "/stats/years").await;
    assert_eq!(
        (
            entier(&a["year"]),
            entier(&a["month"]),
            a["has_previous"].as_bool()
        ),
        (2026, 4, Some(false))
    );
    assert_eq!(a["total"]["month"], 97_500);
    assert_eq!(a["total"]["month_previous"], Value::Null);
    assert_eq!(a["total"]["ytd"], 192_500);
    assert_eq!(a["total"]["ytd_compared"], Value::Null);
    assert_eq!(a["months"][2]["current"], 95_000);
    assert_eq!(
        lire(&e, "alice", "/stats/years?period=2026-03").await["total"]["ytd"],
        95_000
    );
    for chemin in [
        "/stats/history?window=7",
        "/stats/history?window=tout",
        "/stats/history?mode=semaine",
        "/stats/balance?window=0",
        "/stats/balance?mode=semaine",
        "/stats/years?mode=semaine",
        "/stats/years?period=avril",
    ] {
        assert_eq!(
            e.get("alice", chemin).await.statut,
            StatusCode::BAD_REQUEST,
            "{chemin}"
        );
    }
    assert_eq!(
        e.get("alice", "/stats/years?period=2025-01").await.statut,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn evolution_cloisonnee_par_personne() {
    let Some(e) = env().await else { return };
    semer(&e).await;
    soldes_de(&e, "sub-alice", 3, 10_000, 120_000).await;
    soldes_de(&e, "sub-alice", 4, 120_000, 17_500).await;
    let h = lire(&e, "bruno", "/stats/history").await;
    assert_eq!(h["periods"].as_array().unwrap().len(), 1);
    assert_eq!(h["total"]["amounts"], serde_json::json!([77_700]));
    assert_eq!(h["categories"].as_array().unwrap().len(), 1);
    let b = lire(&e, "bruno", "/stats/balance").await;
    assert_eq!(b["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(b["accounts"][0]["last"]["amount"], -77_700);
    assert_eq!(b["accounts"][0]["reconciled"], false);
    let alice = lire(&e, "alice", "/stats/balance").await;
    assert_ne!(
        alice["accounts"][0]["account_id"],
        b["accounts"][0]["account_id"]
    );
    assert_eq!(
        lire(&e, "bruno", "/stats/years").await["total"]["ytd"],
        77_700
    );
    for chemin in ["/stats/history", "/stats/balance", "/stats/years"] {
        assert_eq!(
            e.get("personne", chemin).await.statut,
            StatusCode::FORBIDDEN,
            "{chemin}"
        );
        assert_eq!(
            e.appel(None, "GET", chemin).await.statut,
            StatusCode::UNAUTHORIZED,
            "{chemin}"
        );
    }
}

#[tokio::test]
async fn releves_reels_evolution() {
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
    let releves = lire(&e, "alice", "/statements").await;
    let releves = releves["statements"].as_array().unwrap();
    for mode in ["calendar", "statement"] {
        for fenetre in ["6", "12", "24", "all"] {
            let h = lire(
                &e,
                "alice",
                &format!("/stats/history?mode={mode}&window={fenetre}"),
            )
            .await;
            let periodes = h["periods"].as_array().unwrap();
            assert!(!periodes.is_empty());
            for (k, p) in periodes.iter().enumerate() {
                let mois = lire(
                    &e,
                    "alice",
                    &format!(
                        "/stats/month?mode={mode}&period={}",
                        p["id"].as_str().unwrap()
                    ),
                )
                .await;
                let somme: i64 = h["categories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| entier(&c["amounts"][k]))
                    .sum();
                assert_eq!(
                    somme + entier(&h["unclassified"]["amounts"][k]),
                    entier(&h["total"]["amounts"][k])
                );
                assert_eq!(h["total"]["amounts"][k], mois["expenses"]["total"]);
                let graphique: i64 = h["chart"]["category_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| {
                        entier(
                            &h["categories"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .find(|c| c["id"] == *id)
                                .unwrap()["amounts"][k],
                        )
                    })
                    .sum();
                assert_eq!(
                    graphique
                        + entier(&h["chart"]["others"][k])
                        + entier(&h["unclassified"]["amounts"][k]),
                    entier(&h["total"]["amounts"][k])
                );
            }
            let b = lire(
                &e,
                "alice",
                &format!("/stats/balance?mode={mode}&window={fenetre}"),
            )
            .await;
            for compte in b["accounts"].as_array().unwrap() {
                assert_eq!(compte["reconciled"], true);
                let jours: usize = compte["segments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s["values"].as_array().unwrap().len())
                    .sum();
                let trous: i64 = compte["gaps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| entier(&t["days"]))
                    .sum();
                assert_eq!(jours as i64, entier(&compte["days"]));
                let du = jour(b["from"].as_str().unwrap());
                let au = jour(b["to"].as_str().unwrap());
                assert_eq!(jours as i64 + trous, (au - du).num_days() + 1);
                for s in compte["segments"].as_array().unwrap() {
                    for r in releves
                        .iter()
                        .filter(|r| r["account_id"] == compte["account_id"])
                    {
                        let fin = jour(r["to"].as_str().unwrap());
                        let (de, a) = (
                            jour(s["from"].as_str().unwrap()),
                            jour(s["to"].as_str().unwrap()),
                        );
                        if de <= fin && fin <= a {
                            let rang = usize::try_from((fin - de).num_days()).unwrap();
                            assert_eq!(s["values"][rang], r["new_balance"]);
                        }
                    }
                }
                let valeurs: Vec<i64> = compte["segments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|s| s["values"].as_array().unwrap().iter().map(entier))
                    .collect();
                assert_eq!(
                    valeurs.iter().min().copied(),
                    compte["min"]["amount"].as_i64()
                );
                assert_eq!(
                    valeurs.iter().max().copied(),
                    compte["max"]["amount"].as_i64()
                );
            }
        }
        let a = lire(&e, "alice", &format!("/stats/years?mode={mode}")).await;
        let somme: i64 = a["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| entier(&c["ytd"]))
            .sum();
        assert_eq!(
            somme + entier(&a["unclassified"]["ytd"]),
            entier(&a["total"]["ytd"])
        );
        let mois: i64 = a["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| entier(&c["month"]))
            .sum();
        assert_eq!(
            mois + entier(&a["unclassified"]["month"]),
            entier(&a["total"]["month"])
        );
        eprintln!("relevés réels, évolution, mode {mode} : invariants tenus");
    }
}
