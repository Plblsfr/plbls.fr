use crate::appels::NouveauSondage;
use crate::format::{date_locale_de_ms, iso_de_ms};
use crate::roles::date_en_millisecondes;
use crate::types::{MessagePoll, MessagePollOptionsElement};
use std::collections::{BTreeMap, HashMap};

const DELAI_MAXIMAL_MS: i64 = i32::MAX as i64;
const HEURE_MS: i64 = 3_600_000;

pub const QUESTION_MAX: usize = 500;
pub const CHOIX_MAX: usize = 100;
pub const CHOIX_MIN: usize = 2;
pub const NOMBRE_DE_CHOIX_MAX: usize = 10;
pub const FORMAT_DATE: &str = "JJ/MM/AAAA HH:MM";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cloture {
    Duree(i64),
    Aucune,
    Personnalisee,
}

pub const CLOTURES: [(&str, Cloture); 6] = [
    ("1 h", Cloture::Duree(1)),
    ("24 h", Cloture::Duree(24)),
    ("3 jours", Cloture::Duree(72)),
    ("1 semaine", Cloture::Duree(168)),
    ("Sans clôture", Cloture::Aucune),
    ("Personnalisée", Cloture::Personnalisee),
];
pub const CLOTURE_PAR_DEFAUT: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurSondage {
    QuestionVide,
    QuestionTropLongue,
    ChoixInsuffisants,
    TropDeChoix,
    ChoixTropLong(usize),
    DateInvalide,
    DatePassee,
}

impl ErreurSondage {
    pub fn libelle(&self) -> String {
        match self {
            Self::QuestionVide => "Écris une question.".to_string(),
            Self::QuestionTropLongue => {
                format!("La question dépasse {QUESTION_MAX} caractères.")
            }
            Self::ChoixInsuffisants => format!("Il faut au moins {CHOIX_MIN} choix remplis."),
            Self::TropDeChoix => format!("{NOMBRE_DE_CHOIX_MAX} choix au plus."),
            Self::ChoixTropLong(n) => {
                format!("Le choix {n} dépasse {CHOIX_MAX} caractères.")
            }
            Self::DateInvalide => format!("Date de clôture attendue : {FORMAT_DATE}."),
            Self::DatePassee => "La date de clôture doit être dans le futur.".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formulaire {
    pub question: String,
    pub choix: Vec<String>,
    pub multiple: bool,
    pub cloture: Cloture,
    pub date: String,
}

impl Default for Formulaire {
    fn default() -> Self {
        Self {
            question: String::new(),
            choix: vec![String::new(); CHOIX_MIN],
            multiple: false,
            cloture: CLOTURES[CLOTURE_PAR_DEFAUT].1,
            date: String::new(),
        }
    }
}

pub fn date_saisie(texte: &str, decalage_minutes: i32) -> Option<i64> {
    let (jour, heure) = texte.trim().split_once(' ')?;
    let mut j = jour.split('/');
    let (quantieme, mois, annee) = (j.next()?, j.next()?, j.next()?);
    let (h, m) = heure.trim().split_once(':')?;
    if j.next().is_some() || quantieme.len() > 2 || mois.len() > 2 || h.len() > 2 {
        return None;
    }
    let n = |t: &str| t.parse::<u32>().ok();
    let (quantieme, mois, h, m) = (n(quantieme)?, n(mois)?, n(h)?, n(m)?);
    let annee: i64 = annee.parse().ok().filter(|a| (1000..10_000).contains(a))?;
    let iso = format!("{annee:04}-{mois:02}-{quantieme:02}T{h:02}:{m:02}:00Z");
    let ms = date_en_millisecondes(&iso)? - i64::from(decalage_minutes) * 60_000;
    let d = date_locale_de_ms(ms, decalage_minutes);
    (d.annee == annee && d.mois == mois && d.jour == quantieme && d.heure == h && d.minute == m)
        .then_some(ms)
}

pub fn valider(
    f: &Formulaire,
    maintenant_ms: i64,
    decalage_minutes: i32,
) -> Result<(String, NouveauSondage), ErreurSondage> {
    let question = f.question.trim();
    if question.is_empty() {
        return Err(ErreurSondage::QuestionVide);
    }
    if question.chars().count() > QUESTION_MAX {
        return Err(ErreurSondage::QuestionTropLongue);
    }
    if let Some(n) = f
        .choix
        .iter()
        .position(|c| c.trim().chars().count() > CHOIX_MAX)
    {
        return Err(ErreurSondage::ChoixTropLong(n + 1));
    }
    let options: Vec<String> = f
        .choix
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect();
    if options.len() < CHOIX_MIN {
        return Err(ErreurSondage::ChoixInsuffisants);
    }
    if options.len() > NOMBRE_DE_CHOIX_MAX {
        return Err(ErreurSondage::TropDeChoix);
    }
    let fin = match f.cloture {
        Cloture::Duree(heures) => Some(maintenant_ms + heures * HEURE_MS),
        Cloture::Aucune => None,
        Cloture::Personnalisee => {
            let fin = date_saisie(&f.date, decalage_minutes).ok_or(ErreurSondage::DateInvalide)?;
            if fin <= maintenant_ms {
                return Err(ErreurSondage::DatePassee);
            }
            Some(fin)
        }
    };
    Ok((
        question.to_string(),
        NouveauSondage {
            options,
            multiple: f.multiple,
            closes_at: fin.map(iso_de_ms),
        },
    ))
}

pub fn mes_choix(sondage: &Sondage) -> Vec<String> {
    sondage
        .options
        .iter()
        .flatten()
        .filter(|o| o.me == Some(true))
        .filter_map(|o| o.id.clone())
        .collect()
}

pub fn choix_apres_vote(sondage: &Sondage, option_id: &str) -> Vec<String> {
    let mes = mes_choix(sondage);
    let options = sondage.options.as_deref().unwrap_or_default();
    match action_de_vote(options, option_id) {
        ActionVote::Retirer => mes.into_iter().filter(|c| c != option_id).collect(),
        ActionVote::Voter if sondage.multiple == Some(true) => {
            mes.into_iter().chain([option_id.to_string()]).collect()
        }
        ActionVote::Voter => vec![option_id.to_string()],
    }
}

pub fn comptes_avec(sondage: &Sondage, choix: &[String]) -> BTreeMap<String, i64> {
    sondage
        .options
        .iter()
        .flatten()
        .map(|o| {
            let id = o.id.clone().unwrap_or_default();
            let avant = i64::from(o.me == Some(true));
            let apres = i64::from(choix.contains(&id));
            (id, (o.votes.unwrap_or(0) - avant + apres).max(0))
        })
        .collect()
}

pub type OptionSondage = MessagePollOptionsElement;
pub type Sondage = MessagePoll;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionVote {
    Voter,
    Retirer,
}

fn arrondi(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

pub fn total(options: &[OptionSondage]) -> i64 {
    options.iter().map(|o| o.votes.unwrap_or(0)).sum()
}

pub fn gagnants(options: &[OptionSondage]) -> Vec<String> {
    let meilleur = options
        .iter()
        .map(|o| o.votes.unwrap_or(0))
        .fold(0, i64::max);
    if meilleur == 0 {
        return Vec::new();
    }
    options
        .iter()
        .filter(|o| o.votes.unwrap_or(0) == meilleur)
        .map(|o| o.id.clone().unwrap_or_default())
        .collect()
}

pub fn pourcentage(votes: i64, total: i64) -> i64 {
    if total == 0 {
        0
    } else {
        arrondi(votes as f64 / total as f64 * 100.0)
    }
}

pub fn reste(fin_ms: i64, maintenant_ms: i64) -> String {
    let minutes = arrondi((fin_ms - maintenant_ms) as f64 / 60_000.0).max(1);
    if minutes < 60 {
        format!("{minutes} min")
    } else if minutes < 1440 {
        format!("{} h", arrondi(minutes as f64 / 60.0))
    } else {
        format!("{} j", arrondi(minutes as f64 / 1440.0))
    }
}

pub fn fin_en_ms(clos_a: Option<&str>) -> Option<i64> {
    clos_a
        .filter(|c| !c.is_empty())
        .and_then(date_en_millisecondes)
}

pub fn est_clos(clos_a: Option<&str>, maintenant_ms: i64) -> bool {
    fin_en_ms(clos_a).is_some_and(|fin| fin <= maintenant_ms)
}

pub fn delai_de_reveil_ms(clos_a: Option<&str>, maintenant_ms: i64) -> Option<i64> {
    let fin = fin_en_ms(clos_a)?;
    if fin <= maintenant_ms {
        None
    } else {
        Some((fin - maintenant_ms + 50).min(DELAI_MAXIMAL_MS))
    }
}

pub fn libelle_du_pied(
    total: i64,
    multiple: bool,
    clos_a: Option<&str>,
    maintenant_ms: i64,
) -> String {
    let pluriel = if total > 1 { "s" } else { "" };
    let reponses = if multiple {
        "plusieurs réponses possibles"
    } else {
        "réponse unique"
    };
    let cloture = match (clos_a.filter(|c| !c.is_empty()), fin_en_ms(clos_a)) {
        (None, _) => "sans clôture".to_string(),
        (Some(_), Some(fin)) if fin > maintenant_ms => {
            format!("reste {}", reste(fin, maintenant_ms))
        }
        _ => "Clos".to_string(),
    };
    format!("{total} vote{pluriel} · {reponses} · {cloture}")
}

pub fn action_de_vote(options: &[OptionSondage], option_id: &str) -> ActionVote {
    if options
        .iter()
        .any(|o| o.id.as_deref() == Some(option_id) && o.me == Some(true))
    {
        ActionVote::Retirer
    } else {
        ActionVote::Voter
    }
}

pub fn appliquer_vote(
    sondage: &mut Sondage,
    votant: &str,
    moi: &str,
    options_choisies: &[String],
    comptes: &HashMap<String, i64>,
) {
    for option in sondage.options.iter_mut().flatten() {
        let id = option.id.clone().unwrap_or_default();
        option.votes = Some(comptes.get(&id).copied().unwrap_or(0));
        if votant == moi {
            option.me = Some(options_choisies.contains(&id));
        }
    }
}

pub fn appliquer_cloture(sondage: &mut Sondage, clos_a: &str) {
    sondage.closes_at = Some(clos_a.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Person;

    fn opt(id: &str, votes: i64) -> OptionSondage {
        OptionSondage {
            id: Some(id.to_string()),
            label: Some(id.to_string()),
            votes: Some(votes),
            me: Some(false),
        }
    }

    fn sondage(options: Vec<OptionSondage>) -> Sondage {
        Sondage {
            multiple: Some(false),
            closes_at: None,
            created_by: Some(Person {
                id: Some("bob".to_string()),
                pseudo: Some("bob".to_string()),
            }),
            options: Some(options),
        }
    }

    fn comptes(valeurs: &[(&str, i64)]) -> HashMap<String, i64> {
        valeurs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn votes_et_moi(s: &Sondage) -> Vec<(i64, bool)> {
        s.options
            .iter()
            .flatten()
            .map(|o| (o.votes.unwrap(), o.me.unwrap()))
            .collect()
    }

    #[test]
    fn reprend_les_totaux_de_l_evenement_et_ne_marque_moi_que_pour_son_propre_vote() {
        let mut s = sondage(vec![opt("5", 0), opt("6", 0)]);
        appliquer_vote(
            &mut s,
            "bob",
            "me",
            &["5".to_string()],
            &comptes(&[("5", 1), ("6", 0)]),
        );
        assert_eq!(votes_et_moi(&s), vec![(1, false), (0, false)]);
        appliquer_vote(
            &mut s,
            "me",
            "me",
            &["6".to_string()],
            &comptes(&[("5", 1), ("6", 1)]),
        );
        assert_eq!(votes_et_moi(&s), vec![(1, false), (1, true)]);
    }

    #[test]
    fn le_vote_d_un_autre_garde_mon_choix_et_un_compte_absent_vaut_zero() {
        let mut s = sondage(vec![opt("5", 3), opt("6", 2)]);
        s.options.as_mut().unwrap()[1].me = Some(true);
        appliquer_vote(&mut s, "bob", "me", &[], &comptes(&[("5", 4)]));
        assert_eq!(votes_et_moi(&s), vec![(4, false), (0, true)]);
        appliquer_vote(&mut s, "me", "me", &[], &comptes(&[("5", 4)]));
        assert_eq!(votes_et_moi(&s), vec![(4, false), (0, false)]);
    }

    #[test]
    fn met_en_avant_les_choix_en_tete_ex_aequo_compris_et_aucun_sans_vote() {
        assert_eq!(
            gagnants(&[opt("a", 2), opt("b", 3), opt("c", 3)]),
            vec!["b".to_string(), "c".to_string()]
        );
        assert_eq!(gagnants(&[opt("a", 0), opt("b", 0)]), Vec::<String>::new());
        assert_eq!(gagnants(&[]), Vec::<String>::new());
    }

    #[test]
    fn marque_le_sondage_clos() {
        let mut s = sondage(vec![]);
        assert_eq!(s.closes_at, None);
        appliquer_cloture(&mut s, "2026-01-01T00:00:00Z");
        assert_eq!(s.closes_at.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn totalise_les_votes() {
        assert_eq!(total(&[]), 0);
        assert_eq!(total(&[opt("a", 2), opt("b", 3)]), 5);
    }

    #[test]
    fn les_pourcentages_sont_arrondis_et_nuls_sans_vote() {
        assert_eq!(pourcentage(0, 0), 0);
        assert_eq!(pourcentage(1, 3), 33);
        assert_eq!(pourcentage(2, 3), 67);
        assert_eq!(pourcentage(1, 2), 50);
        assert_eq!(pourcentage(1, 8), 13);
        assert_eq!(pourcentage(5, 5), 100);
    }

    #[test]
    fn le_temps_restant_a_la_bonne_unite() {
        let minute = 60_000;
        assert_eq!(reste(1000, 0), "1 min");
        assert_eq!(reste(0, 5000), "1 min");
        assert_eq!(reste(90 * 1000, 0), "2 min");
        assert_eq!(reste(59 * minute, 0), "59 min");
        assert_eq!(reste(60 * minute, 0), "1 h");
        assert_eq!(reste(89 * minute, 0), "1 h");
        assert_eq!(reste(90 * minute, 0), "2 h");
        assert_eq!(reste(1439 * minute, 0), "24 h");
        assert_eq!(reste(1440 * minute, 0), "1 j");
        assert_eq!(reste(3 * 1440 * minute, 0), "3 j");
    }

    #[test]
    fn l_etat_clos_depend_de_l_echeance() {
        let maintenant = date_en_millisecondes("2026-10-26T12:00:00Z").unwrap();
        assert!(!est_clos(None, maintenant));
        assert!(!est_clos(Some(""), maintenant));
        assert!(!est_clos(Some("2026-10-26T12:00:01Z"), maintenant));
        assert!(est_clos(Some("2026-10-26T12:00:00Z"), maintenant));
        assert!(est_clos(Some("2026-01-01T00:00:00Z"), maintenant));
        assert!(!est_clos(Some("n'importe quoi"), maintenant));
    }

    #[test]
    fn le_reveil_est_planifie_a_l_echeance_et_borne() {
        let maintenant = date_en_millisecondes("2026-10-26T12:00:00Z").unwrap();
        assert_eq!(delai_de_reveil_ms(None, maintenant), None);
        assert_eq!(
            delai_de_reveil_ms(Some("2026-10-26T12:00:00Z"), maintenant),
            None
        );
        assert_eq!(
            delai_de_reveil_ms(Some("2026-10-26T12:00:10Z"), maintenant),
            Some(10_050)
        );
        assert_eq!(
            delai_de_reveil_ms(Some("2027-10-26T12:00:00Z"), maintenant),
            Some(2_147_483_647)
        );
    }

    #[test]
    fn le_pied_de_carte() {
        let maintenant = date_en_millisecondes("2026-10-26T12:00:00Z").unwrap();
        assert_eq!(
            libelle_du_pied(0, false, None, maintenant),
            "0 vote · réponse unique · sans clôture"
        );
        assert_eq!(
            libelle_du_pied(1, true, None, maintenant),
            "1 vote · plusieurs réponses possibles · sans clôture"
        );
        assert_eq!(
            libelle_du_pied(2, false, Some("2026-10-26T12:30:00Z"), maintenant),
            "2 votes · réponse unique · reste 30 min"
        );
        assert_eq!(
            libelle_du_pied(7, false, Some("2026-10-26T11:00:00Z"), maintenant),
            "7 votes · réponse unique · Clos"
        );
    }

    #[test]
    fn recliquer_sur_son_choix_le_retire() {
        let mut options = vec![opt("a", 1), opt("b", 0)];
        options[0].me = Some(true);
        assert_eq!(action_de_vote(&options, "a"), ActionVote::Retirer);
        assert_eq!(action_de_vote(&options, "b"), ActionVote::Voter);
        assert_eq!(action_de_vote(&options, "inconnu"), ActionVote::Voter);
    }

    fn formulaire(question: &str, choix: &[&str], cloture: Cloture, date: &str) -> Formulaire {
        Formulaire {
            question: question.into(),
            choix: choix.iter().map(|c| c.to_string()).collect(),
            multiple: false,
            cloture,
            date: date.into(),
        }
    }

    #[test]
    fn valide_le_formulaire_comme_le_front() {
        let maintenant = date_en_millisecondes("2026-10-26T12:00:00Z").unwrap();
        let ok = valider(
            &formulaire(" Q ? ", &[" a ", "", "b"], Cloture::Duree(24), ""),
            maintenant,
            0,
        )
        .unwrap();
        assert_eq!(ok.0, "Q ?");
        assert_eq!(ok.1.options, ["a", "b"]);
        assert_eq!(ok.1.closes_at.as_deref(), Some("2026-10-27T12:00:00.000Z"));
        let sans = valider(
            &formulaire("Q", &["a", "b"], Cloture::Aucune, ""),
            maintenant,
            0,
        );
        assert_eq!(sans.unwrap().1.closes_at, None);
        let erreur = |f: Formulaire| valider(&f, maintenant, 0).unwrap_err();
        assert_eq!(
            erreur(formulaire("  ", &["a", "b"], Cloture::Aucune, "")),
            ErreurSondage::QuestionVide
        );
        assert_eq!(
            erreur(formulaire(
                &"q".repeat(501),
                &["a", "b"],
                Cloture::Aucune,
                ""
            )),
            ErreurSondage::QuestionTropLongue
        );
        assert!(valider(
            &formulaire(&"é".repeat(500), &["a", "b"], Cloture::Aucune, ""),
            maintenant,
            0
        )
        .is_ok());
        assert_eq!(
            erreur(formulaire("Q", &["a", " "], Cloture::Aucune, "")),
            ErreurSondage::ChoixInsuffisants
        );
        assert_eq!(
            erreur(formulaire(
                "Q",
                &["a", &"b".repeat(101)],
                Cloture::Aucune,
                ""
            )),
            ErreurSondage::ChoixTropLong(2)
        );
        assert_eq!(
            erreur(formulaire("Q", &["a"; 11], Cloture::Aucune, "")),
            ErreurSondage::TropDeChoix
        );
        assert_eq!(
            erreur(formulaire(
                "Q",
                &["a", "b"],
                Cloture::Personnalisee,
                "demain"
            )),
            ErreurSondage::DateInvalide
        );
        assert_eq!(
            erreur(formulaire(
                "Q",
                &["a", "b"],
                Cloture::Personnalisee,
                "26/10/2026 12:00"
            )),
            ErreurSondage::DatePassee
        );
        let futur = valider(
            &formulaire("Q", &["a", "b"], Cloture::Personnalisee, "26/10/2026 14:30"),
            maintenant,
            60,
        )
        .unwrap();
        assert_eq!(
            futur.1.closes_at.as_deref(),
            Some("2026-10-26T13:30:00.000Z")
        );
        assert_eq!(
            ErreurSondage::ChoixTropLong(2).libelle(),
            "Le choix 2 dépasse 100 caractères."
        );
    }

    #[test]
    fn lit_une_date_locale_et_refuse_les_dates_impossibles() {
        assert_eq!(
            date_saisie("1/3/2027 9:05", -120),
            date_en_millisecondes("2027-03-01T11:05:00Z")
        );
        assert_eq!(date_saisie("31/02/2027 10:00", 0), None);
        assert_eq!(date_saisie("01/13/2027 10:00", 0), None);
        assert_eq!(date_saisie("01/01/2027 24:00", 0), None);
        assert_eq!(date_saisie("01/01/2027", 0), None);
        assert_eq!(date_saisie("2027-01-01 10:00", 0), None);
    }

    #[test]
    fn le_vote_local_suit_le_choix_unique_ou_multiple() {
        let mut s = sondage(vec![opt("a", 2), opt("b", 1), opt("c", 0)]);
        s.options.as_mut().unwrap()[0].me = Some(true);
        assert_eq!(mes_choix(&s), ["a"]);
        let apres = choix_apres_vote(&s, "b");
        assert_eq!(apres, ["b"]);
        let comptes = comptes_avec(&s, &apres);
        assert_eq!(
            comptes.into_iter().collect::<Vec<_>>(),
            [
                ("a".to_string(), 1),
                ("b".to_string(), 2),
                ("c".to_string(), 0)
            ]
        );
        assert!(choix_apres_vote(&s, "a").is_empty());
        s.multiple = Some(true);
        assert_eq!(choix_apres_vote(&s, "c"), ["a", "c"]);
        assert_eq!(comptes_avec(&s, &["a".into(), "c".into()])["c"], 1);
    }

    #[test]
    fn se_lit_depuis_la_reponse_de_l_api() {
        let json = r#"{"multiple":true,"closes_at":null,"created_by":{"id":"bob","pseudo":"bob"},"options":[{"id":"5","label":"a","votes":2,"me":true}]}"#;
        let s: Sondage = serde_json::from_str(json).unwrap();
        assert_eq!(s.multiple, Some(true));
        assert_eq!(s.closes_at, None);
        let options = s.options.unwrap();
        assert_eq!(options[0].label.as_deref(), Some("a"));
        assert_eq!(options[0].me, Some(true));
    }
}
