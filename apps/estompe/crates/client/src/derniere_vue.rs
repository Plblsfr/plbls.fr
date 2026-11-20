use serde::{Deserialize, Serialize};

use crate::route::decoder_composant;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DerniereVue {
    pub compte: String,
    pub adresse: String,
}

impl DerniereVue {
    pub fn enregistrer(compte: &str, adresse: &str) -> Self {
        Self {
            compte: compte.to_string(),
            adresse: adresse.to_string(),
        }
    }
}

fn morceaux(adresse: &str) -> Vec<String> {
    adresse
        .strip_prefix("#/")
        .unwrap_or(adresse)
        .split('/')
        .map(decoder_composant)
        .collect()
}

pub fn adresse_a_rouvrir<'a, S: AsRef<str>, M: AsRef<str>>(
    vue: Option<&'a DerniereVue>,
    compte: &str,
    serveurs: &[S],
    messages_prives: &[M],
) -> Option<&'a str> {
    let vue = vue?;
    if vue.compte != compte || vue.adresse.is_empty() {
        return None;
    }
    let parties = morceaux(&vue.adresse);
    let genre = parties.first().map(String::as_str).unwrap_or_default();
    let id = parties.get(1).map(String::as_str).unwrap_or_default();
    let existe = match genre {
        "s" => serveurs.iter().any(|x| x.as_ref() == id),
        "dm" => messages_prives.iter().any(|x| x.as_ref() == id),
        _ => false,
    };
    existe.then_some(vue.adresse.as_str())
}

pub fn conversation_de_la_derniere_vue<S: AsRef<str>, M: AsRef<str>>(
    vue: Option<&DerniereVue>,
    compte: &str,
    serveurs: &[S],
    messages_prives: &[M],
) -> Option<String> {
    let adresse = adresse_a_rouvrir(vue, compte, serveurs, messages_prives)?;
    let parties = morceaux(adresse);
    let genre = parties.first().map(String::as_str).unwrap_or_default();
    match genre {
        "dm" => Some(format!(
            "d:{}",
            parties.get(1).map(String::as_str).unwrap_or_default()
        )),
        "s" => parties
            .get(2)
            .filter(|salon| !salon.is_empty())
            .map(|salon| format!("c:{salon}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVEURS: [&str; 1] = ["s1"];
    const MESSAGES: [&str; 1] = ["d1"];

    #[test]
    fn retrouve_le_salon_et_le_message_prive_du_compte() {
        let vue = DerniereVue::enregistrer("u1", "#/s/s1/c9");
        assert_eq!(
            adresse_a_rouvrir(Some(&vue), "u1", &SERVEURS, &MESSAGES),
            Some("#/s/s1/c9")
        );
        let vue = DerniereVue::enregistrer("u1", "#/dm/d1");
        assert_eq!(
            adresse_a_rouvrir(Some(&vue), "u1", &SERVEURS, &MESSAGES),
            Some("#/dm/d1")
        );
    }

    #[test]
    fn ignore_un_autre_compte_un_serveur_disparu_une_deconnexion() {
        let vue = DerniereVue::enregistrer("u1", "#/s/s1/c9");
        assert_eq!(
            adresse_a_rouvrir(Some(&vue), "u2", &SERVEURS, &MESSAGES),
            None
        );
        assert_eq!(
            adresse_a_rouvrir(Some(&vue), "u1", &[] as &[&str], &MESSAGES),
            None
        );
        let effacee: Option<DerniereVue> = None;
        assert_eq!(
            adresse_a_rouvrir(effacee.as_ref(), "u1", &SERVEURS, &MESSAGES),
            None
        );
    }

    #[test]
    fn donne_la_conversation_de_la_derniere_vue() {
        let vue = DerniereVue::enregistrer("u1", "#/s/s1/c9");
        assert_eq!(
            conversation_de_la_derniere_vue(Some(&vue), "u1", &SERVEURS, &MESSAGES),
            Some("c:c9".to_string())
        );
        let vue = DerniereVue::enregistrer("u1", "#/dm/d1");
        assert_eq!(
            conversation_de_la_derniere_vue(Some(&vue), "u1", &SERVEURS, &MESSAGES),
            Some("d:d1".to_string())
        );
        let vue = DerniereVue::enregistrer("u1", "#/s/s1");
        assert_eq!(
            conversation_de_la_derniere_vue(Some(&vue), "u1", &SERVEURS, &MESSAGES),
            None
        );
    }

    #[test]
    fn se_serialise_pour_etre_persistee() {
        let vue = DerniereVue::enregistrer("u1", "#/dm/d1");
        let json = serde_json::to_string(&vue).unwrap();
        assert_eq!(serde_json::from_str::<DerniereVue>(&json).unwrap(), vue);
        assert_eq!(
            serde_json::from_str::<DerniereVue>("{}").unwrap(),
            DerniereVue::default()
        );
    }
}
