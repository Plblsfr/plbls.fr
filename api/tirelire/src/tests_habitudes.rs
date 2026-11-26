use axum::http::StatusCode;
use chrono::{Days, NaiveDate};
use serde_json::{Value, json};

use crate::{
    bilan::Nature,
    evolution::Fenetre,
    habitudes::{Habitudes, Raison, fenetre, habitudes, hors_habitude, mediane_et_ecart},
    recurrents::{Cadence, Etat, Etats, Recurrent, Recurrents, detecter, proche, recurrents},
    stats::{Calcul, Ecart, Mode},
    tests_stats::{
        COURSES, ENERGIE, EPARGNE, Jeu, LOGEMENT, RESTAURANTS, REVENUS, categorie, categorie_de,
        entier, jour, lire, releve_de,
    },
    testutil::{Env, env},
};

const ABONNEMENTS: i64 = 20;
const MARCHE: i64 = 1;
const CAFE: i64 = 2;
const BISTROT: i64 = 3;
const REGIE: i64 = 4;

fn nommer(j: &mut Jeu, noms: &[(i64, &str)]) {
    for (id, nom) in noms {
        j.d.commercants.insert(*id, (*nom).to_string());
    }
}

fn releve_du(j: &Jeu, date: &str) -> i64 {
    let quand = jour(date);
    j.d.releves
        .iter()
        .find(|r| r.debut < quand && quand <= r.fin)
        .map(|r| r.id)
        .unwrap()
}

fn achat(j: &mut Jeu, date: &str, commercant: i64, categorie: i64, montant: i64) -> i64 {
    let r = releve_du(j, date);
    let id = j.carte(r, date, date, categorie, montant);
    j.chez(commercant);
    id
}

fn debit(j: &mut Jeu, date: &str, commercant: i64, categorie: Option<i64>, montant: i64) -> i64 {
    let r = releve_du(j, date);
    let id = j.op(r, date, categorie, montant);
    j.chez(commercant);
    id
}

fn preleve(j: &mut Jeu, date: &str, commercant: i64, categorie: Option<i64>, montant: i64) -> i64 {
    let id = debit(j, date, commercant, categorie, montant);
    j.d.operations.last_mut().unwrap().preleve = true;
    id
}

fn avril() -> Jeu {
    let mut j = Jeu::nouveau();
    nommer(
        &mut j,
        &[
            (MARCHE, "Marché Colline"),
            (CAFE, "Café Tamis"),
            (BISTROT, "Bistrot Lune"),
            (REGIE, "Régie Belvédère"),
        ],
    );
    j.releve("2026-01-10", "2026-01-31");
    j.releve("2026-01-31", "2026-02-28");
    j.releve("2026-02-28", "2026-03-31");
    let r4 = j.releve("2026-03-31", "2026-04-30");
    achat(&mut j, "2026-03-10", MARCHE, COURSES, 10_000);
    achat(&mut j, "2026-03-20", BISTROT, RESTAURANTS, 14_000);
    j.carte(r4, "2026-04-02", "2026-03-30", COURSES, 2000);
    j.chez(MARCHE);
    achat(&mut j, "2026-04-03", MARCHE, COURSES, 3000);
    achat(&mut j, "2026-04-10", MARCHE, COURSES, 5000);
    achat(&mut j, "2026-04-18", MARCHE, COURSES, 4000);
    j.credit(r4, "2026-04-20", Some(COURSES), 1000);
    j.d.operations.last_mut().unwrap().achat = Some(jour("2026-04-20"));
    j.chez(MARCHE);
    achat(&mut j, "2026-04-01", CAFE, RESTAURANTS, 350);
    achat(&mut j, "2026-04-02", CAFE, RESTAURANTS, 350);
    achat(&mut j, "2026-04-08", CAFE, RESTAURANTS, 400);
    achat(&mut j, "2026-04-15", CAFE, RESTAURANTS, 1499);
    achat(&mut j, "2026-04-22", CAFE, RESTAURANTS, 1500);
    achat(&mut j, "2026-04-25", BISTROT, RESTAURANTS, 6000);
    achat(&mut j, "2026-04-26", BISTROT, RESTAURANTS, 1000);
    debit(&mut j, "2026-04-05", REGIE, Some(LOGEMENT), 80_000);
    j.op(r4, "2026-04-12", None, 2000);
    j.op(r4, "2026-04-06", Some(EPARGNE), 10_000);
    j.credit(r4, "2026-04-28", Some(REVENUS), 250_000);
    j
}

fn calculer(j: &Jeu, mode: Mode, f: Fenetre) -> Habitudes {
    habitudes(&Calcul::nouveau(&j.d, mode), f)
}

fn ecart(amount: i64, delta: i64, delta_bp: Option<i64>) -> Option<Ecart> {
    Some(Ecart {
        amount,
        delta,
        delta_bp,
    })
}

#[test]
fn fenetres_des_habitudes() {
    assert_eq!(fenetre(None), Some(Fenetre::Periodes(3)));
    for (cle, n) in [("1", 1), ("3", 3), ("6", 6), ("12", 12)] {
        assert_eq!(fenetre(Some(cle)), Some(Fenetre::Periodes(n)));
    }
    assert_eq!(fenetre(Some("all")), Some(Fenetre::Tout));
    for refuse in ["0", "2", "24", "tout", "-1", "3 "] {
        assert_eq!(fenetre(Some(refuse)), None, "{refuse}");
    }
}

#[test]
fn classement_des_commercants() {
    let j = avril();
    let h = calculer(&j, Mode::Calendaire, Fenetre::Periodes(1));
    assert_eq!(
        (h.mode, h.window.as_str(), h.available),
        ("calendar", "1", 4)
    );
    let courant = h.current.as_ref().unwrap();
    assert_eq!(
        (courant.from, courant.to, courant.periods),
        (jour("2026-04-01"), jour("2026-04-30"), 1)
    );
    let avant = h.previous.as_ref().unwrap();
    assert_eq!(
        (avant.from, avant.to),
        (jour("2026-03-01"), jour("2026-03-31"))
    );
    assert_eq!(
        (h.operations_from, h.operations_to),
        (Some(jour("2026-04-01")), Some(jour("2026-04-30")))
    );
    assert_eq!(h.merchants_total, 3);
    let lignes: Vec<_> = h
        .merchants
        .iter()
        .map(|m| {
            (
                m.merchant_id,
                m.amount,
                m.count,
                m.average,
                m.rank_amount,
                m.rank_count,
            )
        })
        .collect();
    assert_eq!(
        lignes,
        [
            (MARCHE, 11_000, 3, Some(3667), 1, 2),
            (BISTROT, 7000, 2, Some(3500), 2, 3),
            (CAFE, 4099, 5, Some(820), 3, 1),
        ]
    );
    assert_eq!(h.merchants[0].name.as_deref(), Some("Marché Colline"));
    assert_eq!(h.merchants[0].category_id, Some(COURSES));
    assert_eq!(h.merchants[0].previous, ecart(12_000, -1000, Some(-833)));
    assert_eq!(h.merchants[1].previous, ecart(14_000, -7000, Some(-5000)));
    assert_eq!(h.merchants[2].previous, ecart(0, 4099, None));
    assert!(h.merchants.iter().all(|m| m.merchant_id != REGIE));
}

#[test]
fn fenetre_precedente_absente_quand_elle_n_existe_pas() {
    let j = avril();
    for f in [Fenetre::Periodes(3), Fenetre::Periodes(12), Fenetre::Tout] {
        let h = calculer(&j, Mode::Calendaire, f);
        assert_eq!(h.previous, None);
        assert!(h.merchants.iter().all(|m| m.previous.is_none()));
    }
    let tout = calculer(&j, Mode::Calendaire, Fenetre::Tout);
    assert_eq!(tout.covered_days, 110);
    assert_eq!(
        tout.weekdays.buckets.iter().map(|t| t.days).sum::<i64>(),
        110
    );
    assert_eq!(tout.weeks.buckets.iter().map(|t| t.days).sum::<i64>(), 110);
    let marche = tout
        .merchants
        .iter()
        .find(|m| m.merchant_id == MARCHE)
        .unwrap();
    assert_eq!((marche.amount, marche.count), (23_000, 5));
    assert_eq!(
        calculer(&Jeu::nouveau(), Mode::Calendaire, Fenetre::Tout).current,
        None
    );
}

#[test]
fn rattachement_par_releve() {
    let j = avril();
    let h = calculer(&j, Mode::Releve, Fenetre::Periodes(1));
    assert_eq!(h.covered_days, 30);
    assert_eq!(h.operations_from, Some(jour("2026-04-01")));
    let marche = &h.merchants[0];
    assert_eq!(
        (marche.merchant_id, marche.amount, marche.count),
        (MARCHE, 13_000, 4)
    );
    assert_eq!(marche.previous, ecart(10_000, 3000, Some(3000)));
    let lundi = &h.weekdays.buckets[0];
    assert_eq!((lundi.amount, lundi.count), (1000, 1));
    let fin = &h.weeks.buckets[4];
    assert_eq!(
        (fin.amount, fin.count, fin.days, fin.per_day),
        (2000, 1, 2, Some(1000))
    );
    assert_eq!(h.variable, 26_099);
}

#[test]
fn petits_achats_et_annualisation() {
    let mut j = avril();
    let h = calculer(&j, Mode::Calendaire, Fenetre::Periodes(1));
    let p = &h.small;
    assert_eq!((p.threshold, p.min_repeats, p.min_days), (1500, 3, 28));
    assert_eq!((p.count, p.amount, p.covered_days), (4, 2599, 30));
    assert_eq!(p.yearly, Some(31_621));
    assert_eq!(p.merchants_total, 1);
    assert_eq!(
        (
            p.merchants[0].merchant_id,
            p.merchants[0].count,
            p.merchants[0].amount,
            p.merchants[0].yearly
        ),
        (CAFE, 4, 2599, Some(31_621))
    );
    j.releve("2026-04-30", "2026-05-10");
    for date in ["2026-05-02", "2026-05-04", "2026-05-06"] {
        achat(&mut j, date, CAFE, RESTAURANTS, 300);
    }
    let mai = calculer(&j, Mode::Calendaire, Fenetre::Periodes(1));
    assert_eq!(mai.known_through, Some(jour("2026-05-10")));
    assert_eq!(
        (mai.small.count, mai.small.amount, mai.small.covered_days),
        (3, 900, 10)
    );
    assert_eq!(mai.small.yearly, None);
    assert_eq!(mai.small.merchants[0].yearly, None);
    let trois = calculer(&j, Mode::Calendaire, Fenetre::Periodes(3));
    assert_eq!(
        (
            trois.small.count,
            trois.small.amount,
            trois.small.covered_days
        ),
        (7, 3499, 71)
    );
    assert_eq!(trois.small.yearly, Some(17_988));
    let sans = calculer(&Jeu::nouveau(), Mode::Calendaire, Fenetre::Tout);
    assert_eq!((sans.small.count, sans.small.yearly), (0, None));
    assert!(sans.small.merchants.is_empty());
}

#[test]
fn repartition_par_jour_et_par_semaine() {
    let j = avril();
    let h = calculer(&j, Mode::Calendaire, Fenetre::Periodes(1));
    let jours: Vec<_> = h
        .weekdays
        .buckets
        .iter()
        .map(|t| (t.rank, t.amount, t.count, t.days, t.per_day))
        .collect();
    assert_eq!(
        jours,
        [
            (1, -1000, 0, 4, Some(-250)),
            (2, 0, 0, 4, Some(0)),
            (3, 3749, 4, 5, Some(750)),
            (4, 350, 1, 5, Some(70)),
            (5, 8000, 2, 4, Some(2000)),
            (6, 10_000, 2, 4, Some(2500)),
            (7, 1000, 1, 4, Some(250)),
        ]
    );
    assert_eq!((h.weekdays.amount, h.weekdays.count), (22_099, 10));
    let semaines: Vec<_> = h
        .weeks
        .buckets
        .iter()
        .map(|t| (t.from_day, t.to_day, t.amount, t.count, t.days, t.per_day))
        .collect();
    assert_eq!(
        semaines,
        [
            (Some(1), Some(7), 3700, 3, 7, Some(529)),
            (Some(8), Some(14), 7400, 3, 7, Some(1057)),
            (Some(15), Some(21), 4499, 2, 7, Some(643)),
            (Some(22), Some(28), 8500, 3, 7, Some(1214)),
            (Some(29), Some(31), 0, 0, 2, Some(0)),
        ]
    );
    assert_eq!(
        (h.weeks.amount, h.weeks.count, h.variable),
        (24_099, 11, 24_099)
    );
    let c = Calcul::nouveau(&j.d, Mode::Calendaire);
    let mois = c.mois(c.index(None).unwrap());
    assert_eq!(
        h.variable,
        mois.expenses.variable + mois.expenses.unclassified
    );
}

#[test]
fn seuils_des_operations_inhabituelles() {
    assert_eq!(
        mediane_et_ecart(&[1400, 1500, 1500, 1550, 1600]),
        (1500, 50)
    );
    assert_eq!(mediane_et_ecart(&[1000, 3000, 5000, 7000]), (4000, 2000));
    assert_eq!(mediane_et_ecart(&[]), (0, 0));
    let stable = [1500, 1500, 1500, 1500];
    assert_eq!(hors_habitude(3499, &stable, 2000), None);
    assert_eq!(hors_habitude(3500, &stable, 2000), Some((1500, 2000)));
    let cher = [10_000, 10_000, 10_000, 10_000];
    assert_eq!(hors_habitude(14_999, &cher, 2000), None);
    assert_eq!(hors_habitude(15_000, &cher, 2000), Some((10_000, 5000)));
    let disperse = [1000, 3000, 5000, 7000, 9000];
    assert_eq!(hors_habitude(15_000, &disperse, 2000), None);
    assert_eq!(hors_habitude(15_400, &disperse, 2000), Some((5000, 10_400)));
    assert_eq!(hors_habitude(500, &stable, 2000), None);
}

fn inhabituel() -> (Jeu, i64, i64) {
    let mut j = Jeu::nouveau();
    j.releve("2025-12-31", "2026-04-30");
    nommer(&mut j, &[(10, "Livréo"), (28, "Halle Neuve")]);
    for (date, montant) in [
        ("2026-01-05", 1500),
        ("2026-01-19", 1600),
        ("2026-02-02", 1400),
        ("2026-02-16", 1550),
        ("2026-03-02", 1500),
    ] {
        achat(&mut j, date, 10, RESTAURANTS, montant);
    }
    let livraison = achat(&mut j, "2026-04-10", 10, RESTAURANTS, 4800);
    for k in 0..8 {
        achat(
            &mut j,
            &format!("2026-02-{:02}", k + 1),
            20 + k,
            COURSES,
            2000 + 200 * k,
        );
    }
    let courses = achat(&mut j, "2026-04-15", 28, COURSES, 20_000);
    achat(&mut j, "2026-04-16", 29, COURSES, 7000);
    for date in ["2026-01-05", "2026-02-05", "2026-03-05"] {
        debit(&mut j, date, 30, Some(LOGEMENT), 80_000);
    }
    debit(&mut j, "2026-04-05", 30, Some(LOGEMENT), 250_000);
    debit(&mut j, "2026-04-20", 31, None, 50_000);
    let r = releve_du(&j, "2026-04-21");
    j.credit(r, "2026-04-21", Some(RESTAURANTS), 90_000);
    j.chez(10);
    (j, livraison, courses)
}

#[test]
fn operations_inhabituelles_par_commercant_puis_par_categorie() {
    let (j, livraison, courses) = inhabituel();
    for f in [Fenetre::Periodes(1), Fenetre::Tout] {
        let h = calculer(&j, Mode::Calendaire, f);
        assert_eq!(h.unusual.total, 2);
        let ids: Vec<i64> = h.unusual.operations.iter().map(|o| o.id).collect();
        assert_eq!(ids, [courses, livraison]);
        let o = &h.unusual.operations[1];
        assert_eq!(
            (o.date, o.amount, o.merchant_id, o.category_id),
            (jour("2026-04-10"), 4800, Some(10), Some(RESTAURANTS))
        );
        assert_eq!(o.merchant_name.as_deref(), Some("Livréo"));
        assert_eq!(
            o.reason,
            Raison {
                basis: "merchant",
                median: 1500,
                history: 5,
                ratio_x10: Some(32),
                excess: 3300,
                name: Some("Livréo".to_string()),
            }
        );
        assert_eq!(
            h.unusual.operations[0].reason,
            Raison {
                basis: "category",
                median: 2800,
                history: 9,
                ratio_x10: Some(71),
                excess: 17_200,
                name: Some("Courses".to_string()),
            }
        );
    }
    let mai = {
        let mut j = inhabituel().0;
        j.releve("2026-04-30", "2026-05-31");
        achat(&mut j, "2026-05-10", 10, RESTAURANTS, 1500);
        calculer(&j, Mode::Calendaire, Fenetre::Periodes(1))
    };
    assert_eq!(mai.unusual.total, 0);
}

fn abonnes() -> Jeu {
    let mut j = Jeu::nouveau();
    let mut abonnements = categorie(ABONNEMENTS, "Abonnements", Nature::Fixe, None);
    abonnements.cle = Some("abonnements".to_string());
    j.d.categories.push(abonnements);
    nommer(
        &mut j,
        &[
            (1, "Régie Belvédère"),
            (2, "Streamo"),
            (3, "Arcadia"),
            (4, "Salle Tonus"),
            (5, "Panier Vert"),
            (6, "Nuagebox"),
            (7, "Hyper Colline"),
            (8, "Assurance Havre"),
            (9, "Livret"),
            (10, "Bistrot Lune"),
        ],
    );
    j.releve("2025-02-28", "2025-03-31");
    for (debut, fin) in [
        ("2025-12-31", "2026-01-31"),
        ("2026-01-31", "2026-02-28"),
        ("2026-02-28", "2026-03-31"),
        ("2026-03-31", "2026-04-30"),
        ("2026-04-30", "2026-05-31"),
        ("2026-05-31", "2026-06-30"),
        ("2026-06-30", "2026-07-31"),
        ("2026-07-31", "2026-08-31"),
    ] {
        j.releve(debut, fin);
    }
    for date in [
        "2026-01-05",
        "2026-02-05",
        "2026-03-05",
        "2026-04-07",
        "2026-05-05",
        "2026-06-05",
        "2026-07-06",
        "2026-08-05",
    ] {
        debit(&mut j, date, 1, Some(LOGEMENT), 80_000);
    }
    for m in 1..=8 {
        let montant = if m >= 6 { 1399 } else { 1199 };
        achat(&mut j, &format!("2026-{m:02}-12"), 2, ABONNEMENTS, montant);
        debit(&mut j, &format!("2026-{m:02}-02"), 9, Some(EPARGNE), 20_000);
    }
    preleve(&mut j, "2026-07-08", 3, None, 699);
    preleve(&mut j, "2026-08-08", 3, None, 699);
    for m in 1..=5 {
        preleve(
            &mut j,
            &format!("2026-{m:02}-03"),
            4,
            Some(ABONNEMENTS),
            2990,
        );
    }
    for date in [
        "2026-07-06",
        "2026-07-13",
        "2026-07-21",
        "2026-07-27",
        "2026-08-03",
        "2026-08-10",
        "2026-08-17",
        "2026-08-24",
    ] {
        achat(&mut j, date, 5, COURSES, 1500);
    }
    preleve(&mut j, "2025-03-15", 6, Some(ABONNEMENTS), 3588);
    preleve(&mut j, "2026-03-20", 6, Some(ABONNEMENTS), 3588);
    let samedi = jour("2026-01-03");
    for k in 0..35u64 {
        let date = samedi.checked_add_days(Days::new(7 * k)).unwrap();
        let montant = 4000 + i64::try_from(k * 1373 % 5000).unwrap();
        achat(&mut j, &date.to_string(), 7, COURSES, montant);
    }
    for date in ["2026-01-10", "2026-04-10", "2026-07-10"] {
        preleve(&mut j, date, 8, Some(ENERGIE), 15_000);
    }
    achat(&mut j, "2026-03-14", 10, RESTAURANTS, 2500);
    achat(&mut j, "2026-04-14", 10, RESTAURANTS, 2500);
    j
}

fn de(r: &Recurrents, commercant: i64) -> &Recurrent {
    r.items
        .iter()
        .find(|x| x.merchant_id == commercant)
        .unwrap()
}

#[test]
fn echeances_et_tolerances() {
    let fin = jour("2026-01-31");
    let m = Cadence::Mensuelle;
    assert_eq!(m.echeance(fin, 1), Some(jour("2026-02-28")));
    assert_eq!(m.echeance(fin, 2), Some(jour("2026-03-31")));
    assert_eq!(m.echeance(fin, -2), Some(jour("2025-11-30")));
    assert_eq!(m.rang(fin, jour("2026-03-02")), Some(1));
    assert_eq!(m.rang(fin, jour("2026-03-05")), Some(1));
    assert_eq!(m.rang(fin, jour("2026-03-10")), None);
    assert_eq!(m.rang(jour("2026-01-05"), jour("2026-04-07")), Some(3));
    let h = Cadence::Hebdomadaire;
    assert_eq!(h.rang(jour("2026-07-06"), jour("2026-07-21")), Some(2));
    assert_eq!(h.rang(jour("2026-07-06"), jour("2026-07-23")), None);
    assert_eq!(h.echeance(jour("2026-07-06"), -1), Some(jour("2026-06-29")));
    assert_eq!(
        Cadence::Trimestrielle.rang(jour("2026-01-10"), jour("2026-04-15")),
        Some(1)
    );
    assert_eq!(
        Cadence::Annuelle.rang(jour("2025-03-15"), jour("2026-03-30")),
        Some(1)
    );
    assert_eq!(
        Cadence::Annuelle.rang(jour("2025-03-15"), jour("2026-03-31")),
        None
    );
    assert!(proche(1199, 1259));
    assert!(proche(100, 190));
    assert!(!proche(1199, 1399));
    assert!(proche(10_000, 10_500));
    assert!(!proche(10_000, 10_600));
    assert_eq!(
        (m.minimum(true), m.minimum(false), h.minimum(true)),
        (2, 3, 4)
    );
    assert_eq!(
        (
            Cadence::Trimestrielle.minimum(false),
            Cadence::Annuelle.minimum(false)
        ),
        (3, 2)
    );
}

#[test]
fn detection_des_recurrents() {
    let j = abonnes();
    let r = recurrents(&j.d, &Etats::new());
    assert_eq!(r.as_of, Some(jour("2026-08-31")));
    assert_eq!(r.from, Some(jour("2025-03-01")));
    assert_eq!((r.span_days, r.statements), (549, 9));
    assert_eq!(r.detectable, ["weekly", "monthly", "quarterly", "yearly"]);
    let resume: Vec<_> = r
        .items
        .iter()
        .map(|x| (x.merchant_id, x.cadence, x.amount, x.occurrences, x.status))
        .collect();
    assert_eq!(
        resume,
        [
            (1, "monthly", 80_000, 8, "active"),
            (5, "weekly", 1500, 8, "active"),
            (8, "quarterly", 15_000, 3, "active"),
            (2, "monthly", 1399, 8, "active"),
            (3, "monthly", 699, 2, "active"),
            (6, "yearly", 3588, 2, "active"),
            (4, "monthly", 2990, 5, "stopped"),
        ]
    );
    let loyer = de(&r, 1);
    assert_eq!(loyer.id, "1-monthly");
    assert_eq!(
        (loyer.since, loyer.last_date, loyer.next_date),
        (
            jour("2026-01-05"),
            jour("2026-08-05"),
            Some(jour("2026-09-05"))
        )
    );
    assert_eq!(
        (loyer.monthly, loyer.yearly, loyer.group, loyer.missed),
        (80_000, 960_000, "charge", 0)
    );
    assert_eq!(loyer.category_id, Some(LOGEMENT));
    assert!(loyer.alerts.is_empty());
    let panier = de(&r, 5);
    assert_eq!(
        (
            panier.next_date,
            panier.monthly,
            panier.yearly,
            panier.group
        ),
        (Some(jour("2026-08-31")), 6500, 78_000, "subscription")
    );
    let assurance = de(&r, 8);
    assert_eq!(
        (
            assurance.next_date,
            assurance.monthly,
            assurance.yearly,
            assurance.group
        ),
        (Some(jour("2026-10-10")), 5000, 60_000, "charge")
    );
    let nuage = de(&r, 6);
    assert_eq!(
        (nuage.since, nuage.next_date, nuage.monthly, nuage.yearly),
        (jour("2025-03-15"), Some(jour("2027-03-15")), 299, 3588)
    );
    assert!(nuage.alerts.is_empty());
    assert_eq!(de(&r, 2).group, "subscription");
    assert_eq!(de(&r, 3).category_id, None);
    assert!(r.items.iter().all(|x| ![7, 9, 10].contains(&x.merchant_id)));
    assert_eq!(
        (r.total.count, r.total.monthly, r.total.yearly),
        (6, 93_897, 1_126_764)
    );
    assert_eq!(
        (r.charges.count, r.charges.monthly, r.charges.yearly),
        (2, 85_000, 1_020_000)
    );
    assert_eq!(
        (
            r.subscriptions.count,
            r.subscriptions.monthly,
            r.subscriptions.yearly
        ),
        (4, 8897, 106_764)
    );
}

#[test]
fn alertes_montant_change_nouveau_et_arrete() {
    let j = abonnes();
    let r = recurrents(&j.d, &Etats::new());
    assert_eq!(r.alerts_unseen, 3);
    let streamo = de(&r, 2);
    assert_eq!(streamo.alerts.len(), 1);
    let a = &streamo.alerts[0];
    assert_eq!(
        (a.kind, a.seen, a.date),
        ("amount_changed", false, jour("2026-06-12"))
    );
    assert_eq!(
        (
            a.old_amount,
            a.new_amount,
            a.delta,
            a.delta_bp,
            a.yearly_delta
        ),
        (Some(1199), 1399, Some(200), Some(1668), Some(2400))
    );
    let arcadia = de(&r, 3);
    assert_eq!(arcadia.alerts.len(), 1);
    assert_eq!(
        (
            arcadia.alerts[0].kind,
            arcadia.alerts[0].first_date,
            arcadia.alerts[0].date,
            arcadia.alerts[0].new_amount
        ),
        ("new", Some(jour("2026-07-08")), jour("2026-08-08"), 699)
    );
    assert_eq!(de(&r, 5).alerts[0].kind, "new");
    let salle = de(&r, 4);
    assert_eq!(
        (salle.status, salle.missed, salle.next_date, salle.last_date),
        ("stopped", 3, None, jour("2026-05-03"))
    );
    assert!(salle.alerts.is_empty());

    let mut ancien = abonnes();
    for o in &mut ancien.d.operations {
        if o.commercant == Some(2) && o.montant == 1399 {
            o.montant = 1199;
        }
        if o.commercant == Some(2) && o.jour <= jour("2026-02-12") {
            o.montant = 999;
        }
    }
    let r = recurrents(&ancien.d, &Etats::new());
    let streamo = de(&r, 2);
    assert_eq!((streamo.amount, streamo.occurrences), (1199, 8));
    assert!(streamo.alerts.is_empty());

    let mut seul = abonnes();
    seul.d
        .operations
        .retain(|o| o.commercant != Some(4) || o.jour != jour("2026-05-03"));
    let r = recurrents(&seul.d, &Etats::new());
    let salle = de(&r, 4);
    assert_eq!(
        (salle.status, salle.missed, salle.occurrences),
        ("stopped", 4, 4)
    );

    let mut un_retard = abonnes();
    for m in [6, 7] {
        preleve(
            &mut un_retard,
            &format!("2026-{m:02}-03"),
            4,
            Some(ABONNEMENTS),
            2990,
        );
    }
    let r = recurrents(&un_retard.d, &Etats::new());
    let salle = de(&r, 4);
    assert_eq!(
        (salle.status, salle.missed, salle.next_date),
        ("active", 1, Some(jour("2026-09-03")))
    );
}

#[test]
fn ecarter_et_marquer_vu() {
    let j = abonnes();
    let mut etats = Etats::new();
    etats.insert(
        (2, "monthly".to_string()),
        Etat {
            montant_vu: Some(1299),
            ..Etat::default()
        },
    );
    assert_eq!(recurrents(&j.d, &etats).alerts_unseen, 3);
    etats.insert(
        (2, "monthly".to_string()),
        Etat {
            montant_vu: Some(1399),
            ..Etat::default()
        },
    );
    etats.insert(
        (5, "weekly".to_string()),
        Etat {
            nouveau_vu: true,
            ..Etat::default()
        },
    );
    let r = recurrents(&j.d, &etats);
    assert_eq!(r.alerts_unseen, 1);
    assert!(de(&r, 2).alerts[0].seen);
    assert!(de(&r, 5).alerts[0].seen);
    assert!(!de(&r, 3).alerts[0].seen);
    etats.insert(
        (3, "monthly".to_string()),
        Etat {
            ecarte: true,
            ..Etat::default()
        },
    );
    etats.insert(
        (1, "monthly".to_string()),
        Etat {
            ecarte: true,
            ..Etat::default()
        },
    );
    let r = recurrents(&j.d, &etats);
    assert_eq!(r.alerts_unseen, 0);
    assert!(de(&r, 3).dismissed && de(&r, 1).dismissed);
    assert_eq!(
        (r.total.count, r.total.monthly, r.total.yearly),
        (4, 13_198, 158_376)
    );
    assert_eq!((r.charges.count, r.charges.yearly), (1, 60_000));
    assert_eq!((r.subscriptions.count, r.subscriptions.yearly), (3, 98_376));
    let rangs: Vec<i64> = r.items.iter().map(|x| x.merchant_id).collect();
    assert_eq!(rangs, [5, 8, 2, 6, 4, 1, 3]);
}

fn mensuel(mois_couverts: &[u32], mois_preleves: &[u32], preleve_: bool) -> Recurrents {
    let mut j = Jeu::nouveau();
    nommer(&mut j, &[(1, "Mutuelle Soléa")]);
    for m in mois_couverts {
        let debut = NaiveDate::from_ymd_opt(2026, *m, 1)
            .unwrap()
            .pred_opt()
            .unwrap();
        let fin = NaiveDate::from_ymd_opt(2026, m + 1, 1)
            .unwrap()
            .pred_opt()
            .unwrap();
        j.releve(&debut.to_string(), &fin.to_string());
    }
    for m in mois_preleves {
        let date = format!("2026-{m:02}-10");
        if preleve_ {
            preleve(&mut j, &date, 1, Some(ENERGIE), 2501);
        } else {
            debit(&mut j, &date, 1, Some(ENERGIE), 2501);
        }
    }
    recurrents(&j.d, &Etats::new())
}

#[test]
fn trous_de_couverture_et_echeances_manquees() {
    let trou = mensuel(&[1, 2, 4, 5], &[1, 2, 4, 5], true);
    assert_eq!(trou.items.len(), 1);
    assert_eq!(
        (
            trou.items[0].occurrences,
            trou.items[0].status,
            trou.items[0].missed
        ),
        (4, "active", 0)
    );
    let saute = mensuel(&[1, 2, 3, 4, 5, 6, 7, 8], &[1, 2, 3, 4, 6, 7, 8], true);
    assert_eq!(saute.items.len(), 1);
    assert_eq!(
        (saute.items[0].occurrences, saute.items[0].status),
        (7, "active")
    );
    let peu = mensuel(&[1, 2, 3, 4, 5], &[1, 2, 4, 5], false);
    assert!(peu.items.is_empty());
    let deux = mensuel(&[1, 2, 3], &[2, 3], false);
    assert!(deux.items.is_empty());
    let deux_preleves = mensuel(&[1, 2, 3], &[2, 3], true);
    assert_eq!(deux_preleves.items.len(), 1);
    assert_eq!(deux_preleves.items[0].alerts[0].kind, "new");
    let reprise = mensuel(&[1, 2, 3, 4, 5, 6, 7, 8], &[1, 2, 3, 7, 8], true);
    let etats: Vec<_> = reprise
        .items
        .iter()
        .map(|x| (x.id.as_str(), x.status, x.occurrences))
        .collect();
    assert_eq!(
        etats,
        [("1-monthly-2", "active", 2), ("1-monthly", "stopped", 3)]
    );
}

#[test]
fn historique_trop_court() {
    let seul = mensuel(&[4], &[4], true);
    assert!(seul.items.is_empty());
    assert_eq!((seul.span_days, seul.statements), (30, 1));
    assert_eq!(seul.detectable, ["weekly"]);
    let vide = recurrents(&Jeu::nouveau().d, &Etats::new());
    assert_eq!((vide.as_of, vide.span_days, vide.items.len()), (None, 0, 0));
    assert!(vide.detectable.is_empty());
    assert!(detecter(&Jeu::nouveau().d).is_empty());
}

async fn commercant(e: &Env, sub: &str, nom: &str) -> i64 {
    sqlx::query_scalar("INSERT INTO commercants (sub, nom) VALUES ($1, $2) RETURNING id")
        .bind(sub)
        .bind(nom)
        .fetch_one(e.state.pool())
        .await
        .unwrap()
}

struct Saisie<'a> {
    date: &'a str,
    genre: &'a str,
    commercant: i64,
    categorie: Option<i64>,
    montant: i64,
}

async fn inserer(e: &Env, sub: &str, (compte, releve): (i64, i64), s: &Saisie<'_>) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur, date_achat,
                                 section, type, libelle, montant, sens, categorie_id, origine, commercant_id)
         VALUES ($1, $2, $3, (SELECT count(*) FROM operations WHERE releve_id = $1), 1, $4, $4, $5,
                 'AUTRES OPERATIONS', $6, 'ZZQ INVENTE', $7, 'debit', $8, $9, $10) RETURNING id",
    )
    .bind(releve)
    .bind(compte)
    .bind(sub)
    .bind(jour(s.date))
    .bind((s.genre == "carte").then(|| jour(s.date)))
    .bind(s.genre)
    .bind(s.montant)
    .bind(s.categorie)
    .bind(s.categorie.map(|_| "operation"))
    .bind(s.commercant)
    .fetch_one(e.state.pool())
    .await
    .unwrap()
}

struct Semis {
    flux: i64,
    jeu: i64,
    table: i64,
    grosse: i64,
    normale: i64,
}

async fn semer(e: &Env) -> Semis {
    let restaurants = categorie_de(e, "alice", "restaurants").await;
    let abonnements = categorie_de(e, "alice", "abonnements").await;
    let flux = commercant(e, "sub-alice", "ZZQ Flux").await;
    let jeu = commercant(e, "sub-alice", "ZZQ Jeu").await;
    let table = commercant(e, "sub-alice", "ZZQ Table").await;
    let fins = [
        "2025-12-31",
        "2026-01-31",
        "2026-02-28",
        "2026-03-31",
        "2026-04-30",
        "2026-05-31",
        "2026-06-30",
    ];
    let mut grosse = 0;
    let mut normale = 0;
    for m in 1..=6usize {
        let r = releve_de(
            e,
            "sub-alice",
            i32::try_from(m).unwrap(),
            fins[m - 1],
            fins[m],
        )
        .await;
        inserer(
            e,
            "sub-alice",
            r,
            &Saisie {
                date: &format!("2026-{m:02}-12"),
                genre: "carte",
                commercant: flux,
                categorie: Some(abonnements),
                montant: if m >= 5 { 1399 } else { 1199 },
            },
        )
        .await;
        if m >= 5 {
            inserer(
                e,
                "sub-alice",
                r,
                &Saisie {
                    date: &format!("2026-{m:02}-08"),
                    genre: "prelevement",
                    commercant: jeu,
                    categorie: None,
                    montant: 699,
                },
            )
            .await;
        }
        let id = inserer(
            e,
            "sub-alice",
            r,
            &Saisie {
                date: &format!("2026-{m:02}-{:02}", [20, 3, 27, 11, 19, 20][m - 1]),
                genre: "carte",
                commercant: table,
                categorie: Some(restaurants),
                montant: if m == 6 { 4800 } else { 1500 },
            },
        )
        .await;
        if m == 6 {
            grosse = id;
        } else {
            normale = id;
        }
    }
    let autre = commercant(e, "sub-bruno", "ZZQ Flux").await;
    for m in 1..=3usize {
        let r = releve_de(
            e,
            "sub-bruno",
            i32::try_from(m).unwrap(),
            fins[m - 1],
            fins[m],
        )
        .await;
        inserer(
            e,
            "sub-bruno",
            r,
            &Saisie {
                date: &format!("2026-{m:02}-15"),
                genre: "prelevement",
                commercant: autre,
                categorie: None,
                montant: 7770,
            },
        )
        .await;
    }
    Semis {
        flux,
        jeu,
        table,
        grosse,
        normale,
    }
}

fn recurrent(r: &Value, commercant: i64) -> &Value {
    r["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["merchant_id"] == commercant)
        .unwrap()
}

#[tokio::test]
async fn routes_des_habitudes() {
    let Some(e) = env().await else { return };
    let vide = lire(&e, "alice", "/stats/habits").await;
    assert_eq!(vide["current"], Value::Null);
    assert_eq!(vide["merchants"].as_array().unwrap().len(), 0);
    let s = semer(&e).await;
    let h = lire(&e, "alice", "/stats/habits").await;
    assert_eq!(
        (h["mode"].as_str(), h["window"].as_str()),
        (Some("calendar"), Some("3"))
    );
    assert_eq!(h["current"]["from"], "2026-04-01");
    assert_eq!(h["previous"]["from"], "2026-01-01");
    assert_eq!(h["covered_days"], 91);
    assert_eq!(h["merchants_total"], 2);
    let table = &h["merchants"][0];
    assert_eq!(table["merchant_id"], s.table);
    assert_eq!(
        (entier(&table["amount"]), entier(&table["count"])),
        (7800, 3)
    );
    assert_eq!(table["previous"]["delta"], 3300);
    let filtrees = lire(
        &e,
        "alice",
        &format!(
            "/operations?merchant={}&from={}&to={}&date=purchase&direction=debit",
            s.table,
            h["operations_from"].as_str().unwrap(),
            h["operations_to"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(filtrees["total"], table["count"]);
    assert_eq!(filtrees["total_debits"], table["amount"]);
    assert_eq!(h["unusual"]["total"], 1);
    assert_eq!(h["unusual"]["operations"][0]["id"], s.grosse);
    assert_eq!(h["unusual"]["operations"][0]["reason"]["ratio_x10"], 32);
    assert_eq!(h["weekdays"]["amount"], 7800);
    assert_eq!(h["weeks"]["amount"], h["variable"]);
    for (fenetre, periodes) in [("1", 1), ("3", 3), ("6", 6), ("12", 6), ("all", 6)] {
        for mode in ["calendar", "statement"] {
            let h = lire(
                &e,
                "alice",
                &format!("/stats/habits?mode={mode}&window={fenetre}"),
            )
            .await;
            assert_eq!(h["current"]["periods"], periodes, "{mode} {fenetre}");
        }
    }
    for refuse in ["window=2", "window=24", "mode=semaine"] {
        assert_eq!(
            e.get("alice", &format!("/stats/habits?{refuse}"))
                .await
                .statut,
            StatusCode::BAD_REQUEST,
            "{refuse}"
        );
    }
    let u = lire(&e, "alice", &format!("/operations/{}/unusual", s.grosse)).await;
    assert_eq!(u["unusual"]["basis"], "merchant");
    assert_eq!(u["unusual"]["median"], 1500);
    let n = lire(&e, "alice", &format!("/operations/{}/unusual", s.normale)).await;
    assert_eq!(n["unusual"], Value::Null);
    assert_eq!(
        e.get("bruno", &format!("/operations/{}/unusual", s.grosse))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get("alice", "/operations/abc/unusual").await.statut,
        StatusCode::NOT_FOUND
    );
    let bruno = lire(&e, "bruno", "/stats/habits?window=all").await;
    assert_eq!(bruno["merchants"].as_array().unwrap().len(), 1);
    assert_eq!(bruno["merchants"][0]["amount"], 23_310);
    assert_eq!(bruno["variable"], 23_310);
}

#[tokio::test]
async fn routes_des_recurrents() {
    let Some(e) = env().await else { return };
    let vide = lire(&e, "alice", "/stats/recurring").await;
    assert_eq!(vide["items"].as_array().unwrap().len(), 0);
    assert_eq!(vide["as_of"], Value::Null);
    let s = semer(&e).await;
    let r = lire(&e, "alice", "/stats/recurring").await;
    assert_eq!(r["as_of"], "2026-06-30");
    assert_eq!(r["items"].as_array().unwrap().len(), 2);
    assert_eq!(r["alerts_unseen"], 2);
    assert_eq!(r["total"]["yearly"], 1399 * 12 + 699 * 12);
    let flux = recurrent(&r, s.flux);
    let id_flux = flux["id"].as_str().unwrap().to_string();
    assert_eq!(id_flux, format!("{}-monthly", s.flux));
    assert_eq!(flux["group"], "subscription");
    assert_eq!(flux["alerts"][0]["kind"], "amount_changed");
    assert_eq!(flux["alerts"][0]["old_amount"], 1199);
    let id_jeu = recurrent(&r, s.jeu)["id"].as_str().unwrap().to_string();
    assert_eq!(recurrent(&r, s.jeu)["alerts"][0]["kind"], "new");

    let vu = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/stats/recurring/{id_flux}"),
            &json!({ "seen": true }),
        )
        .await;
    assert_eq!(vu.statut, StatusCode::OK, "{}", vu.texte());
    assert_eq!(vu.json()["alerts_unseen"], 1);
    assert_eq!(recurrent(&vu.json(), s.flux)["alerts"][0]["seen"], true);
    let relu = lire(&e, "alice", "/stats/recurring").await;
    assert_eq!(relu["alerts_unseen"], 1);
    assert_eq!(recurrent(&relu, s.jeu)["alerts"][0]["seen"], false);

    let ecarte = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/stats/recurring/{id_jeu}"),
            &json!({ "dismissed": true }),
        )
        .await
        .json();
    assert_eq!(recurrent(&ecarte, s.jeu)["dismissed"], true);
    assert_eq!(ecarte["alerts_unseen"], 0);
    assert_eq!(ecarte["total"]["yearly"], 1399 * 12);
    assert_eq!(ecarte["total"]["count"], 1);
    let relu = lire(&e, "alice", "/stats/recurring").await;
    assert_eq!(recurrent(&relu, s.jeu)["dismissed"], true);
    assert_eq!(recurrent(&relu, s.flux)["alerts"][0]["seen"], true);
    let retabli = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/stats/recurring/{id_jeu}"),
            &json!({ "dismissed": false }),
        )
        .await
        .json();
    assert_eq!(recurrent(&retabli, s.jeu)["dismissed"], false);
    assert_eq!(retabli["alerts_unseen"], 1);
    assert_eq!(retabli["total"]["count"], 2);

    for corps in [
        json!({}),
        json!({ "seen": false }),
        json!({ "dismissed": "oui" }),
        json!({ "dismissed": true, "autre": 1 }),
    ] {
        let rep = e
            .envoyer(
                "alice",
                "PATCH",
                &format!("/stats/recurring/{id_flux}"),
                &corps,
            )
            .await;
        assert_eq!(rep.statut, StatusCode::BAD_REQUEST, "{corps}");
    }
    for inconnu in ["0-monthly", "abc", &format!("{}-yearly", s.flux)] {
        let rep = e
            .envoyer(
                "alice",
                "PATCH",
                &format!("/stats/recurring/{inconnu}"),
                &json!({ "dismissed": true }),
            )
            .await;
        assert_eq!(rep.statut, StatusCode::NOT_FOUND, "{inconnu}");
    }

    let bruno = lire(&e, "bruno", "/stats/recurring").await;
    assert_eq!(bruno["items"].as_array().unwrap().len(), 1);
    assert_eq!(bruno["items"][0]["amount"], 7770);
    assert_eq!(bruno["items"][0]["dismissed"], false);
    let chez_alice = e
        .envoyer(
            "bruno",
            "PATCH",
            &format!("/stats/recurring/{id_flux}"),
            &json!({ "dismissed": true }),
        )
        .await;
    assert_eq!(chez_alice.statut, StatusCode::NOT_FOUND);
    assert_eq!(
        recurrent(&lire(&e, "alice", "/stats/recurring").await, s.flux)["dismissed"],
        false
    );
    let etats: i64 = sqlx::query_scalar("SELECT count(*) FROM recurrents WHERE sub <> 'sub-alice'")
        .fetch_one(e.state.pool())
        .await
        .unwrap();
    assert_eq!(etats, 0);
    for chemin in [
        "/stats/habits",
        "/stats/recurring",
        &format!("/operations/{}/unusual", s.grosse),
    ] {
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
    let refuse = e
        .envoyer(
            "personne",
            "PATCH",
            &format!("/stats/recurring/{id_flux}"),
            &json!({ "dismissed": true }),
        )
        .await;
    assert_eq!(refuse.statut, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn releves_reels_habitudes() {
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
    let somme = |v: &Value, champ: &str| -> i64 {
        v["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| entier(&t[champ]))
            .sum()
    };
    for mode in ["calendar", "statement"] {
        for fenetre in ["1", "3", "6", "12", "all"] {
            let h = lire(
                &e,
                "alice",
                &format!("/stats/habits?mode={mode}&window={fenetre}"),
            )
            .await;
            assert_eq!(somme(&h["weeks"], "amount"), entier(&h["weeks"]["amount"]));
            assert_eq!(h["weeks"]["amount"], h["variable"]);
            assert_eq!(
                somme(&h["weekdays"], "amount"),
                entier(&h["weekdays"]["amount"])
            );
            assert_eq!(somme(&h["weekdays"], "days"), entier(&h["covered_days"]));
            assert_eq!(somme(&h["weeks"], "days"), entier(&h["covered_days"]));
            let mois = lire(&e, "alice", &format!("/stats/months?mode={mode}")).await;
            let periodes = mois["periods"].as_array().unwrap();
            let n = usize::try_from(entier(&h["current"]["periods"])).unwrap();
            let mut attendu = 0;
            for p in &periodes[periodes.len() - n..] {
                let m = lire(
                    &e,
                    "alice",
                    &format!(
                        "/stats/month?mode={mode}&period={}",
                        p["id"].as_str().unwrap()
                    ),
                )
                .await;
                attendu +=
                    entier(&m["expenses"]["variable"]) + entier(&m["expenses"]["unclassified"]);
            }
            assert_eq!(entier(&h["variable"]), attendu);
            eprintln!(
                "relevés réels, habitudes, mode {mode}, fenêtre {fenetre} : {} jours couverts, {} commerçants, {} petits achats répétés chez {} commerçants (annualisation {}), {} paiements par carte, {} opérations inhabituelles",
                h["covered_days"],
                h["merchants_total"],
                h["small"]["count"],
                h["small"]["merchants_total"],
                if h["small"]["yearly"].is_null() {
                    "absente"
                } else {
                    "présente"
                },
                h["weekdays"]["count"],
                h["unusual"]["total"],
            );
        }
    }
    let r = lire(&e, "alice", "/stats/recurring").await;
    let items = r["items"].as_array().unwrap();
    let actifs: Vec<&Value> = items.iter().filter(|x| x["status"] == "active").collect();
    assert_eq!(
        entier(&r["total"]["yearly"]),
        actifs.iter().map(|x| entier(&x["yearly"])).sum::<i64>()
    );
    assert_eq!(
        entier(&r["total"]["count"]),
        entier(&r["subscriptions"]["count"]) + entier(&r["charges"]["count"])
    );
    eprintln!(
        "relevés réels, récurrents : {} relevés, {} jours couverts, cadences détectables {}, {} récurrents ({} abonnements, {} charges, {} arrêtés), {} alertes",
        r["statements"],
        r["span_days"],
        r["detectable"].as_array().unwrap().len(),
        items.len(),
        r["subscriptions"]["count"],
        r["charges"]["count"],
        items.len() - actifs.len(),
        r["alerts_unseen"],
    );
}
