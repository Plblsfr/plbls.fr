use crate::menu::Entree;

pub const REACTIONS_RAPIDES: [&str; 3] = ["👍", "❤️", "😂"];

pub const REPONDRE: &str = "repondre";
pub const MODIFIER: &str = "modifier";
pub const EPINGLER: &str = "epingler";
pub const DESEPINGLER: &str = "desepingler";
pub const REAGIR: &str = "reagir";
pub const COPIER_TEXTE: &str = "copier-texte";
pub const COPIER_LIEN: &str = "copier-lien";
pub const NON_LU: &str = "non-lu";
pub const SIGNALER: &str = "signaler";
pub const SUPPRIMER: &str = "supprimer";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DroitsConversation {
    pub ecrire: bool,
    pub gerer: bool,
    pub reagir: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DroitsMessage {
    pub repondre: bool,
    pub modifier: bool,
    pub epingler: bool,
    pub supprimer: bool,
    pub reagir: bool,
    pub menu: bool,
}

pub struct Sujet<'a> {
    pub auteur: &'a str,
    pub moi: &'a str,
    pub systeme: bool,
    pub local: bool,
    pub sondage: bool,
}

pub fn droits_du_message(sujet: &Sujet, conversation: DroitsConversation) -> DroitsMessage {
    if sujet.systeme || sujet.local {
        return DroitsMessage::default();
    }
    let mien = !sujet.moi.is_empty() && sujet.auteur == sujet.moi;
    DroitsMessage {
        repondre: conversation.ecrire,
        modifier: mien && conversation.ecrire && !sujet.sondage,
        epingler: mien || conversation.gerer,
        supprimer: mien || conversation.gerer,
        reagir: conversation.reagir,
        menu: true,
    }
}

pub fn entrees_du_message(droits: DroitsMessage, epingle: bool, texte: bool) -> Vec<Entree> {
    if !droits.menu {
        return Vec::new();
    }
    let mut entrees = Vec::new();
    if droits.repondre {
        entrees.push(Entree::nouvelle(REPONDRE, "Répondre", "repondre"));
    }
    if droits.modifier {
        entrees.push(Entree::nouvelle(MODIFIER, "Modifier", "crayon"));
    }
    if droits.epingler {
        entrees.push(if epingle {
            Entree::nouvelle(DESEPINGLER, "Désépingler", "epingle")
        } else {
            Entree::nouvelle(EPINGLER, "Épingler", "epingle")
        });
    }
    if droits.reagir {
        entrees.push(Entree::nouvelle(REAGIR, "Ajouter une réaction", "sourire"));
    }
    let copie = entrees.len();
    if texte {
        entrees.push(Entree::nouvelle(COPIER_TEXTE, "Copier le texte", "copie"));
    }
    entrees.push(Entree::nouvelle(COPIER_LIEN, "Copier le lien", "lien"));
    entrees.push(Entree::nouvelle(NON_LU, "Marquer comme non lu", "oeil"));
    entrees.push(Entree::nouvelle(SIGNALER, "Signaler", "drapeau").bientot());
    if copie > 0 {
        entrees[copie] = entrees[copie].clone().separee();
    }
    if droits.supprimer {
        entrees.push(
            Entree::nouvelle(SUPPRIMER, "Supprimer", "corbeille")
                .danger()
                .separee(),
        );
    }
    entrees
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sujet<'a>(auteur: &'a str) -> Sujet<'a> {
        Sujet {
            auteur,
            moi: "moi",
            systeme: false,
            local: false,
            sondage: false,
        }
    }

    fn ids(entrees: &[Entree]) -> Vec<&str> {
        entrees.iter().map(|e| e.id.as_str()).collect()
    }

    const MEMBRE: DroitsConversation = DroitsConversation {
        ecrire: true,
        gerer: false,
        reagir: true,
    };

    #[test]
    fn sans_le_droit_de_gerer_les_messages_supprimer_et_epingler_sont_absents_sur_le_message_d_un_autre(
    ) {
        let droits = droits_du_message(&sujet("bob"), MEMBRE);
        assert!(!droits.supprimer && !droits.epingler && !droits.modifier);
        assert_eq!(
            ids(&entrees_du_message(droits, false, true)),
            [
                REPONDRE,
                REAGIR,
                COPIER_TEXTE,
                COPIER_LIEN,
                NON_LU,
                SIGNALER
            ]
        );
        let moderateur = DroitsConversation {
            gerer: true,
            ..MEMBRE
        };
        assert_eq!(
            ids(&entrees_du_message(
                droits_du_message(&sujet("bob"), moderateur),
                true,
                true
            )),
            [
                REPONDRE,
                DESEPINGLER,
                REAGIR,
                COPIER_TEXTE,
                COPIER_LIEN,
                NON_LU,
                SIGNALER,
                SUPPRIMER
            ]
        );
    }

    #[test]
    fn sur_mon_message_tout_est_permis_sauf_ce_que_le_salon_refuse() {
        let droits = droits_du_message(&sujet("moi"), MEMBRE);
        assert_eq!(
            ids(&entrees_du_message(droits, false, false)),
            [
                REPONDRE,
                MODIFIER,
                EPINGLER,
                REAGIR,
                COPIER_LIEN,
                NON_LU,
                SIGNALER,
                SUPPRIMER
            ]
        );
        let lecture_seule = droits_du_message(
            &sujet("moi"),
            DroitsConversation {
                ecrire: false,
                gerer: false,
                reagir: false,
            },
        );
        assert!(!lecture_seule.repondre && !lecture_seule.modifier && !lecture_seule.reagir);
        assert!(lecture_seule.supprimer);
        let sondage = Sujet {
            sondage: true,
            ..sujet("moi")
        };
        assert!(!droits_du_message(&sondage, MEMBRE).modifier);
    }

    #[test]
    fn un_message_systeme_ou_pas_encore_envoye_n_a_pas_de_menu() {
        for s in [
            Sujet {
                systeme: true,
                ..sujet("moi")
            },
            Sujet {
                local: true,
                ..sujet("moi")
            },
        ] {
            assert!(entrees_du_message(droits_du_message(&s, MEMBRE), false, true).is_empty());
        }
    }

    #[test]
    fn signaler_est_visible_mais_marque_bientot_et_supprimer_est_dangereux_et_separe() {
        let entrees = entrees_du_message(droits_du_message(&sujet("moi"), MEMBRE), false, true);
        let signaler = entrees.iter().find(|e| e.id == SIGNALER).unwrap();
        assert!(signaler.bientot && !signaler.danger);
        let supprimer = entrees.last().unwrap();
        assert!(supprimer.danger && supprimer.separe);
        assert!(
            entrees
                .iter()
                .find(|e| e.id == COPIER_TEXTE)
                .unwrap()
                .separe
        );
    }
}
