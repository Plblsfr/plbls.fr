use chrono::{Datelike, NaiveDate};
use serde::Serialize;

use crate::mots::{Ligne, Mot, Page, joindre, lignes};

const ECART_MILLIERS: f64 = 4.0;
const RETRAIT_DETAIL: f64 = 30.0;
const MARGE_MONTANT: f64 = 60.0;
const MOIS: [&str; 12] = [
    "JANVIER",
    "FEVRIER",
    "MARS",
    "AVRIL",
    "MAI",
    "JUIN",
    "JUILLET",
    "AOUT",
    "SEPTEMBRE",
    "OCTOBRE",
    "NOVEMBRE",
    "DECEMBRE",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sens {
    Debit,
    Credit,
}

impl Sens {
    pub fn cle(self) -> &'static str {
        match self {
            Sens::Debit => "debit",
            Sens::Credit => "credit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genre {
    Carte,
    Prelevement,
    Virement,
    VirementInstantane,
    EcheancePret,
    Cotisation,
    Autre,
}

impl Genre {
    pub const TOUS: [Genre; 7] = [
        Genre::Carte,
        Genre::Prelevement,
        Genre::Virement,
        Genre::VirementInstantane,
        Genre::EcheancePret,
        Genre::Cotisation,
        Genre::Autre,
    ];

    pub fn cle(self) -> &'static str {
        match self {
            Genre::Carte => "carte",
            Genre::Prelevement => "prelevement",
            Genre::Virement => "virement",
            Genre::VirementInstantane => "virement_instantane",
            Genre::EcheancePret => "echeance_pret",
            Genre::Cotisation => "cotisation",
            Genre::Autre => "autre",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operation {
    pub page: u32,
    pub date_operation: NaiveDate,
    pub date_valeur: NaiveDate,
    pub date_achat: Option<NaiveDate>,
    pub section: String,
    pub genre: Genre,
    pub libelle: String,
    pub details: Vec<String>,
    pub commercant: Option<String>,
    pub montant: i64,
    pub sens: Sens,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Compte {
    pub iban: String,
    pub bic: String,
    pub numero: String,
    pub reference: String,
    pub titulaire: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Releve {
    pub numero: i32,
    pub annee: i32,
    pub debut: NaiveDate,
    pub date_arrete: NaiveDate,
    pub compte: Compte,
    pub ancien_solde: i64,
    pub nouveau_solde: i64,
    pub total_debits: i64,
    pub total_credits: i64,
    pub pages: u32,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ecart {
    #[serde(rename = "check")]
    pub controle: &'static str,
    #[serde(rename = "expected")]
    pub attendu: i64,
    #[serde(rename = "read")]
    pub lu: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LigneInconnue {
    pub page: u32,
    #[serde(rename = "text")]
    pub texte: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refus {
    pub format_inconnu: bool,
    pub manquants: Vec<String>,
    pub ecarts: Vec<Ecart>,
    pub lignes: Vec<LigneInconnue>,
}

pub fn sans_accent(texte: &str) -> String {
    texte
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'à' | 'â' | 'À' | 'Â' => 'A',
            'û' | 'ù' | 'ü' | 'Û' | 'Ù' | 'Ü' => 'U',
            'ô' | 'ö' | 'Ô' | 'Ö' => 'O',
            'î' | 'ï' | 'Î' | 'Ï' => 'I',
            'ç' | 'Ç' => 'C',
            c => c.to_ascii_uppercase(),
        })
        .collect()
}

fn chiffres(texte: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&texte.len()) && texte.bytes().all(|b| b.is_ascii_digit())
}

fn jour_mois(texte: &str) -> Option<(u32, u32)> {
    let (j, m) = texte.split_once('/')?;
    (chiffres(j, 2, 2) && chiffres(m, 2, 2)).then(|| Some((j.parse().ok()?, m.parse().ok()?)))?
}

fn date_complete(texte: &str) -> Option<NaiveDate> {
    let (jm, a) = texte.rsplit_once('/')?;
    let (j, m) = jour_mois(jm)?;
    if !chiffres(a, 4, 4) {
        return None;
    }
    NaiveDate::from_ymd_opt(a.parse().ok()?, m, j)
}

fn date_en_lettres(mots: &[&Mot]) -> Option<NaiveDate> {
    let [j, m, a, ..] = mots else { return None };
    if !chiffres(&j.texte, 1, 2) || !chiffres(&a.texte, 4, 4) {
        return None;
    }
    let mois = MOIS.iter().position(|x| *x == sans_accent(&m.texte))?;
    NaiveDate::from_ymd_opt(
        a.texte.parse().ok()?,
        u32::try_from(mois).ok()? + 1,
        j.texte.parse().ok()?,
    )
}

pub fn date_proche(jour: u32, mois: u32, reference: NaiveDate) -> Option<NaiveDate> {
    (-1..=1)
        .filter_map(|d| NaiveDate::from_ymd_opt(reference.year() + d, mois, jour))
        .min_by_key(|d| (*d - reference).num_days().abs())
}

fn centimes(entier: &str, decimales: &str) -> Option<i64> {
    let e: i64 = entier.parse().ok()?;
    let d: i64 = decimales.parse().ok()?;
    e.checked_mul(100)?.checked_add(d)
}

fn montant_final<'a, 'b>(mots: &'b [&'a Mot]) -> Option<(i64, &'a Mot, &'b [&'a Mot])> {
    let (dernier, mut reste) = mots.split_last()?;
    let (entier, decimales) = dernier.texte.split_once(',')?;
    if !chiffres(entier, 1, 12) || !chiffres(decimales, 2, 2) {
        return None;
    }
    let mut entier = entier.to_string();
    let mut debut = *dernier;
    let mut groupe = entier.len();
    while groupe == 3
        && entier.len() <= 12
        && let Some((m, avant)) = reste.split_last()
        && chiffres(&m.texte, 1, 3)
        && debut.x - m.fin() <= ECART_MILLIERS
        && debut.x >= m.fin()
    {
        entier.insert_str(0, &m.texte);
        groupe = m.texte.len();
        debut = m;
        reste = avant;
    }
    Some((centimes(&entier, decimales)?, debut, reste))
}

#[derive(Clone, Copy, Debug)]
struct Colonnes {
    valeur: f64,
    debit: f64,
    credit: f64,
}

impl Colonnes {
    fn de(ligne: &Ligne) -> Option<Self> {
        let x = |nom: &str, rang: usize| {
            ligne
                .mots
                .iter()
                .filter(|m| sans_accent(&m.texte) == nom)
                .nth(rang)
                .map(|m| m.x)
        };
        x("OPERATION", 0)?;
        Some(Self {
            valeur: x("DATE", 1)?,
            debit: x("DEBIT", 0)?,
            credit: x("CREDIT", 0)?,
        })
    }

    fn sens(&self, dernier: &Mot) -> Sens {
        if dernier.x >= self.credit {
            Sens::Credit
        } else {
            Sens::Debit
        }
    }
}

fn commence_par(mots: &[&Mot], debut: &[&str]) -> bool {
    mots.len() >= debut.len()
        && mots
            .iter()
            .zip(debut)
            .all(|(m, d)| sans_accent(&m.texte) == *d)
}

fn apres<'a, 'b>(mots: &'b [&'a Mot], suite: &[&str]) -> Option<&'b [&'a Mot]> {
    (0..mots.len())
        .find(|i| commence_par(&mots[*i..], suite))
        .map(|i| &mots[i + suite.len()..])
}

#[derive(Default)]
struct Entete {
    numero: Option<(i32, i32)>,
    date_arrete: Option<NaiveDate>,
    compte: Compte,
}

impl Entete {
    fn lire(&mut self, ligne: &Ligne) {
        let mots = &ligne.mots;
        if self.numero.is_none()
            && let Some([n, tiret, a, ..]) = apres(mots, &["N°"])
            && tiret.texte == "-"
            && chiffres(&n.texte, 1, 3)
            && chiffres(&a.texte, 4, 4)
        {
            self.numero = n.texte.parse().ok().zip(a.texte.parse().ok());
        }
        if self.date_arrete.is_none()
            && let Some(suite) = apres(mots, &["ARRETE", "AU"])
        {
            self.date_arrete = date_en_lettres(suite);
        }
        if self.compte.iban.is_empty()
            && let Some(suite) = apres(mots, &["IBAN"])
        {
            let fin = suite
                .iter()
                .position(|m| m.texte == "BIC")
                .unwrap_or(suite.len());
            self.compte.iban = suite[..fin].iter().map(|m| m.texte.as_str()).collect();
            if let Some(bic) = suite.get(fin + 1) {
                self.compte.bic = bic.texte.clone();
            }
        }
        if self.compte.numero.is_empty()
            && let Some(suite) = apres(mots, &["NUMERO", "DE", "COMPTE"])
        {
            self.compte.numero = joindre(suite);
        }
        if self.compte.titulaire.is_empty()
            && let Some(suite) = apres(mots, &["TITULAIRE(S)", "DU", "COMPTE", ":"])
        {
            self.compte.titulaire = joindre(suite);
        }
        if self.compte.reference.is_empty()
            && let Some(i) = mots.iter().position(|m| m.texte == "Compte")
        {
            let suite: Vec<&Mot> = mots[i + 1..]
                .iter()
                .take_while(|m| chiffres(&m.texte, 1, 20))
                .copied()
                .collect();
            self.compte.reference = joindre(&suite);
        }
    }
}

struct Solde {
    date: NaiveDate,
    montant: i64,
}

fn solde(mots: &[&Mot], colonnes: &Colonnes) -> Option<Solde> {
    let (signe, suite) = match sans_accent(&mots.first()?.texte).as_str() {
        "CREDITEUR" => (Some(1), &mots[1..]),
        "DEBITEUR" => (Some(-1), &mots[1..]),
        _ => (None, mots),
    };
    let [au, suite @ ..] = suite else {
        return None;
    };
    if sans_accent(&au.texte) != "AU" {
        return None;
    }
    let date = date_en_lettres(suite)?;
    let fin = suite.len() - usize::from(suite.last()?.texte == "€");
    let (montant, _, reste) = montant_final(&suite[..fin])?;
    if reste.len() != 3 {
        return None;
    }
    let signe = signe.unwrap_or(match colonnes.sens(suite[fin - 1]) {
        Sens::Credit => 1,
        Sens::Debit => -1,
    });
    Some(Solde {
        date,
        montant: signe * montant,
    })
}

fn totaux(mots: &[&Mot], colonnes: &Colonnes) -> Option<(i64, i64)> {
    let (mut debits, mut credits) = (None, None);
    let mut reste = mots;
    while !reste.is_empty() {
        let (montant, _, avant) = montant_final(reste)?;
        let cible = match colonnes.sens(reste[reste.len() - 1]) {
            Sens::Credit => &mut credits,
            Sens::Debit => &mut debits,
        };
        if cible.replace(montant).is_some() {
            return None;
        }
        reste = avant;
    }
    Some((debits.unwrap_or(0), credits.unwrap_or(0)))
}

fn section(mots: &[&Mot]) -> Option<String> {
    let fin = mots
        .iter()
        .position(|m| m.texte == "(Suite)" || m.texte == "Sous-total")
        .unwrap_or(mots.len());
    let (nom, suite) = mots.split_at(fin);
    let majuscules = |m: &&Mot| {
        m.texte.chars().any(char::is_uppercase)
            && !m
                .texte
                .chars()
                .any(|c| c.is_lowercase() || c.is_ascii_digit())
    };
    if nom.is_empty() || !nom.iter().all(majuscules) {
        return None;
    }
    let conforme = match suite {
        [] => true,
        [m] => m.texte == "(Suite)",
        [_, deux_points, montant @ .., euro] => {
            deux_points.texte == ":"
                && euro.texte == "€"
                && !montant.is_empty()
                && montant.iter().all(|m| {
                    m.texte
                        .chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | ','))
                })
        }
        _ => false,
    };
    conforme.then(|| joindre(nom))
}

fn genre(mots: &[&Mot], date_operation: NaiveDate) -> (Genre, Option<NaiveDate>, Option<String>) {
    let textes: Vec<&str> = mots.iter().map(|m| m.texte.as_str()).collect();
    match textes.as_slice() {
        ["CARTE", date, commercant @ ..] if !commercant.is_empty() && jour_mois(date).is_some() => {
            let achat = jour_mois(date).and_then(|(j, m)| date_proche(j, m, date_operation));
            match achat {
                Some(a) => (Genre::Carte, Some(a), Some(commercant.join(" "))),
                None => (Genre::Autre, None, None),
            }
        }
        ["PRLV", ..] => (Genre::Prelevement, None, None),
        ["VIR", "INST", ..] => (Genre::VirementInstantane, None, None),
        ["VIR", ..] => (Genre::Virement, None, None),
        ["ECH", "PRET", ..] => (Genre::EcheancePret, None, None),
        ["F", "COTISATION", ..] => (Genre::Cotisation, None, None),
        _ => (Genre::Autre, None, None),
    }
}

fn operation(mots: &[&Mot], colonnes: &Colonnes, page: u32, section: &str) -> Option<Operation> {
    let [date, valeur, suite @ ..] = mots else {
        return None;
    };
    let (jour, mois) = jour_mois(&date.texte)?;
    let date_valeur = date_complete(&valeur.texte)?;
    let date_operation = date_proche(jour, mois, date_valeur)?;
    let (montant, debut, libelle) = montant_final(suite)?;
    if libelle.is_empty() || debut.x < colonnes.debit - MARGE_MONTANT {
        return None;
    }
    let (genre, date_achat, commercant) = genre(libelle, date_operation);
    Some(Operation {
        page,
        date_operation,
        date_valeur,
        date_achat,
        section: section.to_string(),
        genre,
        libelle: joindre(libelle),
        details: Vec::new(),
        commercant,
        montant,
        sens: colonnes.sens(suite[suite.len() - 1]),
    })
}

#[derive(Default)]
struct Lecture {
    entete: Entete,
    section: Option<String>,
    operations: Vec<Operation>,
    rattachable: bool,
    ancien: Option<Solde>,
    nouveau: Option<Solde>,
    totaux: Option<(i64, i64)>,
    attend_totaux: bool,
    inconnues: Vec<LigneInconnue>,
    manquants: Vec<String>,
}

impl Lecture {
    fn inconnue(&mut self, ligne: &Ligne) {
        self.rattachable = false;
        self.inconnues.push(LigneInconnue {
            page: ligne.page,
            texte: ligne.texte(),
        });
    }

    fn corps(&mut self, ligne: &Ligne, colonnes: &Colonnes) {
        let mots = ligne.mots.as_slice();
        let Some(premier) = mots.first() else { return };
        if std::mem::take(&mut self.attend_totaux)
            && let Some(t) = totaux(mots, colonnes)
        {
            self.totaux = Some(t);
            return;
        }
        if commence_par(mots, &["NOUVEAU", "SOLDE"]) {
            match solde(&mots[2..], colonnes) {
                Some(s) => self.nouveau = Some(s),
                None => self.inconnue(ligne),
            }
            return;
        }
        if self.totaux.is_some() {
            return;
        }
        if commence_par(mots, &["ANCIEN", "SOLDE"]) {
            match solde(&mots[2..], colonnes) {
                Some(s) if self.ancien.is_none() => self.ancien = Some(s),
                _ => self.inconnue(ligne),
            }
            return;
        }
        if commence_par(mots, &["TOTAL", "DES", "OPERATIONS", "DU", "RELEVE"]) {
            self.rattachable = false;
            if mots.len() == 5 {
                self.attend_totaux = true;
                return;
            }
            match totaux(&mots[5..], colonnes) {
                Some(t) => self.totaux = Some(t),
                None => self.inconnue(ligne),
            }
            return;
        }
        if premier.x >= colonnes.valeur + RETRAIT_DETAIL {
            let dans_le_libelle = mots.iter().all(|m| m.fin() <= colonnes.debit);
            match self.operations.last_mut() {
                Some(op) if self.rattachable && dans_le_libelle => op.details.push(ligne.texte()),
                _ => self.inconnue(ligne),
            }
            return;
        }
        if jour_mois(&premier.texte).is_some() {
            let section = self.section.clone().unwrap_or_default();
            match operation(mots, colonnes, ligne.page, &section) {
                Some(op) if self.section.is_some() => {
                    self.operations.push(op);
                    self.rattachable = true;
                }
                _ => self.inconnue(ligne),
            }
            return;
        }
        match section(mots) {
            Some(nom) => {
                let suite = mots.iter().any(|m| m.texte == "(Suite)");
                self.rattachable = self.rattachable && suite && self.section.as_ref() == Some(&nom);
                self.section = Some(nom);
            }
            None => self.inconnue(ligne),
        }
    }

    fn page(&mut self, page: &Page) {
        let lignes = lignes(page);
        let tableau = lignes
            .iter()
            .enumerate()
            .find_map(|(i, l)| Colonnes::de(l).map(|c| (i, c)));
        let Some((rang, colonnes)) = tableau else {
            for l in &lignes {
                self.entete.lire(l);
            }
            if !lignes.is_empty() {
                self.manquants
                    .push(format!("en-tête du tableau (page {})", page.numero));
            }
            return;
        };
        for l in &lignes[..rang] {
            self.entete.lire(l);
        }
        for l in &lignes[rang + 1..] {
            if self.nouveau.is_some() {
                break;
            }
            self.corps(l, &colonnes);
        }
    }
}

pub fn analyser(pages: &[Page]) -> Result<Releve, Refus> {
    let mut lecture = Lecture::default();
    for page in pages {
        if lecture.nouveau.is_some() {
            break;
        }
        lecture.page(page);
    }
    let Lecture {
        entete,
        operations,
        ancien,
        nouveau,
        totaux,
        inconnues,
        mut manquants,
        ..
    } = lecture;
    if entete.numero.is_none() && ancien.is_none() && operations.is_empty() {
        return Err(Refus {
            format_inconnu: true,
            ..Refus::default()
        });
    }
    let mut absent = |present: bool, nom: &str| {
        if !present {
            manquants.push(nom.to_string());
        }
    };
    absent(entete.numero.is_some(), "numéro du relevé");
    absent(entete.date_arrete.is_some(), "date d'arrêté");
    absent(!entete.compte.iban.is_empty(), "IBAN");
    absent(ancien.is_some(), "ancien solde");
    absent(totaux.is_some(), "total des opérations");
    absent(nouveau.is_some(), "nouveau solde");
    let somme = |sens: Sens| -> i64 {
        operations
            .iter()
            .filter(|o| o.sens == sens)
            .map(|o| o.montant)
            .sum()
    };
    let (debits, credits) = (somme(Sens::Debit), somme(Sens::Credit));
    let mut ecarts = Vec::new();
    let mut comparer = |controle: &'static str, attendu: i64, lu: i64| {
        if attendu != lu {
            ecarts.push(Ecart {
                controle,
                attendu,
                lu,
            });
        }
    };
    if let Some((d, c)) = totaux {
        comparer("total_debits", d, debits);
        comparer("total_credits", c, credits);
    }
    if let (Some(a), Some(n)) = (&ancien, &nouveau) {
        comparer("new_balance", n.montant, a.montant + credits - debits);
    }
    match (entete.numero, entete.date_arrete, ancien, nouveau, totaux) {
        (Some((numero, annee)), Some(date_arrete), Some(ancien), Some(nouveau), Some(totaux))
            if manquants.is_empty() && ecarts.is_empty() && inconnues.is_empty() =>
        {
            Ok(Releve {
                numero,
                annee,
                debut: ancien.date,
                date_arrete,
                compte: entete.compte,
                ancien_solde: ancien.montant,
                nouveau_solde: nouveau.montant,
                total_debits: totaux.0,
                total_credits: totaux.1,
                pages: u32::try_from(pages.len()).unwrap_or(u32::MAX),
                operations,
            })
        }
        _ => Err(Refus {
            format_inconnu: false,
            manquants,
            ecarts,
            lignes: inconnues,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{PASSAGE_ANNEE, TOTAL_FAUX, TROIS_PAGES, pages};

    fn jour(a: i32, m: u32, j: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, j).unwrap()
    }

    fn mot(texte: &str, x: f64, largeur: f64) -> Mot {
        Mot {
            texte: texte.into(),
            x,
            y: 100.0,
            largeur,
            vertical: false,
        }
    }

    #[test]
    fn releve_de_trois_pages() {
        let r = analyser(&pages(TROIS_PAGES)).unwrap();
        assert_eq!((r.numero, r.annee, r.pages), (3, 2025, 3));
        assert_eq!(
            (r.debut, r.date_arrete),
            (jour(2025, 2, 7), jour(2025, 3, 7))
        );
        assert_eq!(
            r.compte,
            Compte {
                iban: "FR7612345678900123456789034".into(),
                bic: "FICTFRPPXXX".into(),
                numero: "12345 67890 01234567890 12".into(),
                reference: "01234567 89".into(),
                titulaire: "MME LEGRAND CAMILLE".into(),
            }
        );
        assert_eq!(
            (
                r.ancien_solde,
                r.total_debits,
                r.total_credits,
                r.nouveau_solde
            ),
            (120_436, 89_133, 192_839, 224_142)
        );
        assert_eq!(r.operations.len(), 15);
        let sections: Vec<(&str, usize)> = [
            "VIREMENTS RECUS",
            "VIREMENTS EMIS ET PRELEVEMENTS",
            "PAIEMENTS PAR CARTE",
            "SERVICES ET FRAIS BANCAIRES",
        ]
        .into_iter()
        .map(|s| (s, r.operations.iter().filter(|o| o.section == s).count()))
        .collect();
        assert_eq!(
            sections.iter().map(|s| s.1).collect::<Vec<_>>(),
            [2, 5, 7, 1]
        );
        let genres: Vec<&str> = r.operations.iter().map(|o| o.genre.cle()).collect();
        assert_eq!(
            genres,
            [
                "virement_instantane",
                "virement",
                "prelevement",
                "prelevement",
                "virement_instantane",
                "echeance_pret",
                "prelevement",
                "carte",
                "carte",
                "carte",
                "carte",
                "carte",
                "carte",
                "carte",
                "cotisation"
            ]
        );
        assert_eq!(
            r.operations.iter().map(|o| o.page).collect::<Vec<_>>(),
            [1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3]
        );
        for o in &r.operations {
            let textes = [vec![o.libelle.clone()], o.details.clone()].concat();
            assert!(
                textes
                    .iter()
                    .all(|t| !t.contains("Émis") && !t.contains("MRELFIC") && !t.contains("Date")),
                "{textes:?}"
            );
        }
    }

    #[test]
    fn details_sur_plusieurs_lignes_et_pages() {
        let r = analyser(&pages(TROIS_PAGES)).unwrap();
        let o = &r.operations[2];
        assert_eq!(o.libelle, "PRLV SEPA TELECOM IMAGINAIRE SA");
        assert_eq!(
            o.details,
            [
                "Forfait fictif 06XXXXX000 (reference: 1234",
                "567890)",
                "FR00ZZZ000001",
                "MANDAT0000000001"
            ]
        );
        assert_eq!((o.montant, o.sens), (1999, Sens::Debit));
        let o = &r.operations[4];
        assert_eq!(o.libelle, "VIR INST vers BERNARD PAUL");
        assert_eq!(
            o.details,
            [
                "de LEGRAND CAMILLE - Cadeau",
                "25045IP1RT000000002",
                "SUITE DU DETAIL PRECEDENT 0002"
            ]
        );
        assert_eq!(o.page, 1);
        assert!(r.operations[5].details.is_empty());
        assert_eq!(
            (r.operations[5].date_operation, r.operations[5].date_valeur),
            (jour(2025, 3, 3), jour(2025, 3, 5))
        );
        assert_eq!(r.operations[1].montant, 190_000);
    }

    #[test]
    fn remboursement_carte_au_credit() {
        let r = analyser(&pages(TROIS_PAGES)).unwrap();
        let o = &r.operations[10];
        assert_eq!(o.section, "PAIEMENTS PAR CARTE");
        assert_eq!(o.genre, Genre::Carte);
        assert_eq!((o.montant, o.sens), (1599, Sens::Credit));
        assert_eq!(
            (o.date_operation, o.date_achat),
            (jour(2025, 2, 20), Some(jour(2025, 2, 20)))
        );
        assert_eq!(
            o.commercant.as_deref(),
            Some("Jeux En Ligne Fictifs Fictiville")
        );
        let debit = &r.operations[11];
        assert_eq!((debit.montant, debit.sens), (1599, Sens::Debit));
        assert_eq!(debit.date_achat, Some(jour(2025, 2, 19)));
        let autres = r
            .operations
            .iter()
            .filter(|o| o.genre != Genre::Carte)
            .all(|o| o.date_achat.is_none() && o.commercant.is_none());
        assert!(autres);
    }

    #[test]
    fn passage_d_annee() {
        let r = analyser(&pages(PASSAGE_ANNEE)).unwrap();
        assert_eq!((r.numero, r.annee), (1, 2026));
        assert_eq!(
            (r.debut, r.date_arrete),
            (jour(2025, 12, 7), jour(2026, 1, 7))
        );
        assert_eq!((r.ancien_solde, r.nouveau_solde), (-12_000, 111_020));
        assert_eq!((r.total_debits, r.total_credits), (126_980, 250_000));
        let dates: Vec<(NaiveDate, NaiveDate, Option<NaiveDate>)> = r
            .operations
            .iter()
            .map(|o| (o.date_operation, o.date_valeur, o.date_achat))
            .collect();
        assert_eq!(
            dates,
            [
                (jour(2025, 12, 31), jour(2026, 1, 1), None),
                (
                    jour(2025, 12, 9),
                    jour(2025, 12, 9),
                    Some(jour(2025, 12, 8))
                ),
                (jour(2026, 1, 2), jour(2026, 1, 2), Some(jour(2025, 12, 30))),
                (jour(2026, 1, 2), jour(2026, 1, 2), Some(jour(2025, 12, 31))),
                (jour(2026, 1, 5), jour(2026, 1, 5), Some(jour(2026, 1, 3))),
            ]
        );
        assert_eq!(r.operations[2].montant, 123_456);
        assert_eq!(
            r.operations[2].commercant.as_deref(),
            Some("VOYAGES DU LARGE 29 QUIMPER")
        );
    }

    #[test]
    fn controle_en_echec_liste_ecarts_et_lignes() {
        let refus = analyser(&pages(TOTAL_FAUX)).unwrap_err();
        assert!(!refus.format_inconnu);
        assert!(refus.manquants.is_empty(), "{:?}", refus.manquants);
        assert_eq!(
            refus.ecarts,
            [Ecart {
                controle: "total_debits",
                attendu: 99_999,
                lu: 89_133
            }]
        );
        assert_eq!(
            refus.lignes,
            [
                LigneInconnue {
                    page: 1,
                    texte: "13/02 13/02/2025 VIR INST vers COMPTE SANS MONTANT".into()
                },
                LigneInconnue {
                    page: 1,
                    texte: "DETAIL ORPHELIN 0001".into()
                },
            ]
        );
    }

    #[test]
    fn solde_incoherent_et_elements_manquants() {
        let faux = TROIS_PAGES.replace(" 2,50\n", " 3,50\n");
        let refus = analyser(&pages(&faux)).unwrap_err();
        let controles: Vec<&str> = refus.ecarts.iter().map(|e| e.controle).collect();
        assert_eq!(controles, ["total_debits", "new_balance"]);
        assert_eq!(
            (refus.ecarts[1].attendu, refus.ecarts[1].lu),
            (224_142, 224_042)
        );
        let sans_fin: String = TROIS_PAGES
            .lines()
            .take_while(|l| !l.ends_with(" TOTAL"))
            .map(|l| format!("{l}\n"))
            .collect();
        let refus = analyser(&pages(&sans_fin)).unwrap_err();
        assert_eq!(refus.manquants, ["total des opérations", "nouveau solde"]);
        let sans_iban: String = TROIS_PAGES
            .lines()
            .filter(|l| !l.ends_with(" IBAN"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(
            analyser(&pages(&sans_iban)).unwrap_err().manquants,
            ["IBAN"]
        );
    }

    #[test]
    fn format_inconnu() {
        let refus = analyser(&pages("page 1\n10 10 20 Bonjour\n40 10 20 monde\n")).unwrap_err();
        assert!(refus.format_inconnu);
        assert!(analyser(&[]).unwrap_err().format_inconnu);
    }

    #[test]
    fn montants_avec_separateur_de_milliers() {
        let lire = |mots: &[Mot]| {
            let refs: Vec<&Mot> = mots.iter().collect();
            montant_final(&refs).map(|(m, debut, reste)| (m, debut.x, reste.len()))
        };
        assert_eq!(lire(&[mot("7,10", 536.0, 15.0)]), Some((710, 536.0, 0)));
        assert_eq!(
            lire(&[
                mot("X", 190.0, 5.0),
                mot("35", 200.0, 9.0),
                mot("4", 521.3, 4.4),
                mot("060,50", 527.8, 23.8)
            ]),
            Some((406_050, 521.3, 2))
        );
        assert_eq!(
            lire(&[
                mot("1", 500.0, 4.4),
                mot("234", 506.6, 13.3),
                mot("567,00", 522.1, 23.8)
            ]),
            Some((123_456_700, 500.0, 0))
        );
        assert_eq!(
            lire(&[mot("35", 300.0, 9.0), mot("060,50", 527.8, 23.8)]),
            Some((6050, 527.8, 1))
        );
        assert_eq!(
            lire(&[mot("12", 510.0, 9.0), mot("18,50", 521.2, 19.0)]),
            Some((1850, 521.2, 1))
        );
        assert_eq!(lire(&[mot("10/26", 261.0, 19.0)]), None);
        assert_eq!(lire(&[mot("7,1", 536.0, 15.0)]), None);
        assert_eq!(lire(&[]), None);
    }

    #[test]
    fn annee_deduite_de_la_date_voisine() {
        assert_eq!(
            date_proche(30, 12, jour(2026, 1, 2)),
            Some(jour(2025, 12, 30))
        );
        assert_eq!(
            date_proche(2, 1, jour(2025, 12, 31)),
            Some(jour(2026, 1, 2))
        );
        assert_eq!(
            date_proche(15, 6, jour(2026, 6, 16)),
            Some(jour(2026, 6, 15))
        );
        assert_eq!(
            date_proche(29, 2, jour(2024, 3, 1)),
            Some(jour(2024, 2, 29))
        );
        assert_eq!(date_proche(31, 2, jour(2024, 3, 1)), None);
    }

    #[test]
    fn lignes_de_section() {
        let lire = |texte: &str| {
            let mots: Vec<Mot> = texte.split(' ').map(|t| mot(t, 0.0, 1.0)).collect();
            section(&mots.iter().collect::<Vec<_>>())
        };
        assert_eq!(
            lire("VIREMENTS RECUS Sous-total : +1 912,40 €").as_deref(),
            Some("VIREMENTS RECUS")
        );
        assert_eq!(
            lire("PAIEMENTS PAR CARTE (Suite)").as_deref(),
            Some("PAIEMENTS PAR CARTE")
        );
        assert_eq!(lire("CHEQUES EMIS").as_deref(), Some("CHEQUES EMIS"));
        assert_eq!(lire("Garantie de vos dépôts"), None);
        assert_eq!(lire("VIREMENTS RECUS autre chose"), None);
        assert_eq!(lire("(Suite)"), None);
    }
}
