use crate::http::ErreurApi;
use crate::recherche::plier;
use crate::store::Etat;

pub const LIMITE_PROPOSITIONS: usize = 6;
pub const CONVERSATION_EXISTANTE: &str = "conversation existante";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonneConnue {
    pub id: String,
    pub pseudo: String,
    pub ou: String,
}

fn texte(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

pub fn personnes_connues(etat: &Etat) -> Vec<PersonneConnue> {
    let mut sortie: Vec<PersonneConnue> = Vec::new();
    let mut ajouter = |id: &str, pseudo: &str, ou: &str| {
        if !pseudo.is_empty() && !sortie.iter().any(|p| p.pseudo == pseudo) {
            sortie.push(PersonneConnue {
                id: id.to_string(),
                pseudo: pseudo.to_string(),
                ou: ou.to_string(),
            });
        }
    };
    for d in &etat.messages_prives {
        if let Some(autre) = &d.other {
            ajouter(
                texte(&autre.id),
                texte(&autre.pseudo),
                CONVERSATION_EXISTANTE,
            );
        }
    }
    for serveur in etat.serveurs_ordonnes() {
        let id = texte(&serveur.id);
        for m in etat.membres(id) {
            ajouter(texte(&m.id), texte(&m.pseudo), texte(&serveur.name));
        }
    }
    let moi = etat.pseudo().to_string();
    sortie.retain(|p| p.pseudo != moi);
    sortie.sort_by(|a, b| {
        plier(&a.pseudo)
            .cmp(&plier(&b.pseudo))
            .then_with(|| a.pseudo.cmp(&b.pseudo))
    });
    sortie
}

pub fn proposer(connues: &[PersonneConnue], saisie: &str) -> Vec<PersonneConnue> {
    let cherche = plier(saisie.trim());
    if cherche.is_empty() {
        return Vec::new();
    }
    connues
        .iter()
        .filter(|p| {
            let pseudo = plier(&p.pseudo);
            pseudo.contains(&cherche) && pseudo != cherche
        })
        .take(LIMITE_PROPOSITIONS)
        .cloned()
        .collect()
}

pub fn message_d_erreur(erreur: &ErreurApi) -> String {
    match erreur.code() {
        "user_not_found" => "Aucun utilisateur avec ce pseudo.".to_string(),
        "self_dm" => "Tu ne peux pas t'écrire à toi-même.".to_string(),
        _ => erreur.message().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn etat() -> Etat {
        let mut etat = Etat {
            compte: serde_json::from_value(json!({"id": "me", "pseudo": "moi"})).unwrap(),
            serveurs: serde_json::from_value(json!([
                {"id": "s1", "name": "Amis"},
                {"id": "s2", "name": "Travail"}
            ]))
            .unwrap(),
            messages_prives: serde_json::from_value(json!([
                {"id": "d1", "other": {"id": "carol", "pseudo": "carol"}}
            ]))
            .unwrap(),
            ..Etat::default()
        };
        for (id, nom, membres) in [
            (
                "s1",
                "Amis",
                json!([{"id": "me", "pseudo": "moi"}, {"id": "carol", "pseudo": "carol"}, {"id": "eve", "pseudo": "Ève"}]),
            ),
            (
                "s2",
                "Travail",
                json!([{"id": "eve", "pseudo": "Ève"}, {"id": "bob", "pseudo": "bob"}, {"id": "cara", "pseudo": "Cara"}]),
            ),
        ] {
            etat.details.insert(
                id.into(),
                serde_json::from_value(
                    json!({"server": {"id": id, "name": nom}, "channels": [], "members": membres}),
                )
                .unwrap(),
            );
        }
        etat
    }

    #[test]
    fn les_personnes_connues_viennent_des_conversations_puis_des_serveurs_sans_soi() {
        let connues = personnes_connues(&etat());
        let lignes: Vec<(&str, &str)> = connues
            .iter()
            .map(|p| (p.pseudo.as_str(), p.ou.as_str()))
            .collect();
        assert_eq!(
            lignes,
            [
                ("bob", "Travail"),
                ("Cara", "Travail"),
                ("carol", CONVERSATION_EXISTANTE),
                ("Ève", "Amis"),
            ]
        );
    }

    #[test]
    fn propose_les_pseudos_qui_contiennent_la_saisie_sauf_le_pseudo_exact() {
        let connues = personnes_connues(&etat());
        let pseudos = |saisie: &str| -> Vec<String> {
            proposer(&connues, saisie)
                .into_iter()
                .map(|p| p.pseudo)
                .collect()
        };
        assert_eq!(pseudos("car"), ["Cara", "carol"]);
        assert_eq!(pseudos(" eve "), Vec::<String>::new());
        assert_eq!(pseudos("ev"), ["Ève"]);
        assert_eq!(pseudos("carol"), Vec::<String>::new());
        assert_eq!(pseudos(""), Vec::<String>::new());
        let nombreuses: Vec<PersonneConnue> = (0..10)
            .map(|i| PersonneConnue {
                id: i.to_string(),
                pseudo: format!("ami{i}"),
                ou: String::new(),
            })
            .collect();
        assert_eq!(proposer(&nombreuses, "ami").len(), LIMITE_PROPOSITIONS);
    }
}
