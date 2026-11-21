use crate::entree::Evenement;
use crate::invitation_des_arguments;
use estompe_plateforme::instance::MessageInstance;
use estompe_plateforme::OPTION_ARRIERE_PLAN;

pub fn evenements_du_message(message: &MessageInstance) -> Vec<Evenement> {
    match invitation_des_arguments(message.arguments.iter().cloned()) {
        Some(code) => vec![Evenement::OuvrirInvitation, Evenement::Rejoindre(code)],
        None => Vec::new(),
    }
}

pub fn ramene_la_fenetre(message: &MessageInstance) -> bool {
    !message.arguments.iter().any(|a| a == OPTION_ARRIERE_PLAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(arguments: &[&str]) -> MessageInstance {
        MessageInstance {
            arguments: arguments.iter().map(|a| a.to_string()).collect(),
            jeton_activation: None,
        }
    }

    #[test]
    fn une_invitation_passee_au_second_lancement_ouvre_la_saisie_et_rejoint() {
        assert_eq!(
            evenements_du_message(&message(&[
                "--invitation",
                "https://chat.plbls.fr/#/join/ABCDE-23456"
            ])),
            [
                Evenement::OuvrirInvitation,
                Evenement::Rejoindre("https://chat.plbls.fr/#/join/ABCDE-23456".into())
            ]
        );
        assert!(evenements_du_message(&message(&[])).is_empty());
    }

    #[test]
    fn un_lancement_en_arriere_plan_ne_ramene_pas_la_fenetre() {
        assert!(ramene_la_fenetre(&message(&[])));
        assert!(!ramene_la_fenetre(&message(&["--arriere-plan"])));
    }
}
