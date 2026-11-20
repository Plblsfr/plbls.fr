use crate::roles::date_en_millisecondes;
use crate::types::{MessagePoll, MessagePollOptionsElement};
use std::collections::HashMap;

const DELAI_MAXIMAL_MS: i64 = i32::MAX as i64;

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
