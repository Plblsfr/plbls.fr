use crate::http::ErreurApi;

pub const SUITE_ECOUTE_SEULE: &str = "Vous êtes en écoute seule.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurVocal {
    SalonPlein,
    DroitManquant(String),
    PasUnSalonVocal,
    Indisponible,
    Connexion,
    Autre(String),
}

impl ErreurVocal {
    pub fn depuis_api(erreur: &ErreurApi) -> Self {
        match (erreur.statut(), erreur.code()) {
            (409, "voice_full") => Self::SalonPlein,
            (400, "not_voice_channel") => Self::PasUnSalonVocal,
            (502, _) => Self::Indisponible,
            (403, _) => Self::DroitManquant(erreur.message().to_string()),
            (0, _) => Self::Connexion,
            _ if erreur.message().is_empty() => Self::Connexion,
            _ => Self::Autre(erreur.message().to_string()),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::SalonPlein => "Salon vocal complet (10 participants maximum).".to_string(),
            Self::DroitManquant(m) if !m.is_empty() => m.clone(),
            Self::DroitManquant(_) => "Tu n'as pas le droit de rejoindre ce salon.".to_string(),
            Self::PasUnSalonVocal => "Ce salon n'est pas un salon vocal.".to_string(),
            Self::Indisponible => "Le vocal est indisponible pour le moment.".to_string(),
            Self::Connexion => "Connexion au vocal impossible.".to_string(),
            Self::Autre(m) => m.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CauseMicro {
    Refuse,
    Absent,
    Inutilisable,
}

impl CauseMicro {
    pub fn message(self) -> &'static str {
        match self {
            Self::Refuse => "Accès au micro refusé.",
            Self::Absent => {
                "Aucun micro disponible (vérifiez le micro choisi dans les Paramètres)."
            }
            Self::Inutilisable => "Le micro est inutilisable.",
        }
    }

    pub fn erreur(self) -> String {
        format!("{} {SUITE_ECOUTE_SEULE}", self.message())
    }

    pub fn depuis_plateforme(erreur: &estompe_plateforme::Erreur) -> Self {
        match erreur {
            estompe_plateforme::Erreur::Indisponible(_) => Self::Absent,
            estompe_plateforme::Erreur::Refuse(m) if m.contains("introuvable") => Self::Absent,
            estompe_plateforme::Erreur::Refuse(_) => Self::Refuse,
            estompe_plateforme::Erreur::Echec(_) => Self::Inutilisable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Refus;
    use estompe_plateforme::Erreur;

    fn refus(statut: u16, code: &str, message: &str) -> Refus {
        Refus {
            statut,
            code: code.to_string(),
            message: message.to_string(),
            donnees: Default::default(),
        }
    }

    #[test]
    fn indique_la_cause_du_micro_et_l_ecoute_seule() {
        let refuse = Erreur::Refuse("permission refusée".into());
        let absent = Erreur::Refuse("périphérique audio introuvable".into());
        let autre = Erreur::Echec("x".into());
        assert!(CauseMicro::depuis_plateforme(&refuse)
            .erreur()
            .starts_with("Accès au micro refusé."));
        assert!(CauseMicro::depuis_plateforme(&absent)
            .erreur()
            .starts_with("Aucun micro"));
        assert!(CauseMicro::depuis_plateforme(&autre)
            .erreur()
            .starts_with("Le micro est inutilisable."));
        for cause in [
            CauseMicro::Refuse,
            CauseMicro::Absent,
            CauseMicro::Inutilisable,
        ] {
            assert!(cause.erreur().ends_with("écoute seule."));
        }
    }

    #[test]
    fn traduit_les_refus_de_l_api() {
        let plein = ErreurApi::Conflit(refus(409, "voice_full", "complet"));
        assert_eq!(ErreurVocal::depuis_api(&plein), ErreurVocal::SalonPlein);
        assert_eq!(
            ErreurVocal::SalonPlein.message(),
            "Salon vocal complet (10 participants maximum)."
        );
        let interdit = ErreurApi::Interdit(refus(403, "forbidden", "Permission manquante"));
        assert_eq!(
            ErreurVocal::depuis_api(&interdit).message(),
            "Permission manquante"
        );
        let indisponible = ErreurApi::Serveur(refus(502, "voice_unavailable", ""));
        assert_eq!(
            ErreurVocal::depuis_api(&indisponible),
            ErreurVocal::Indisponible
        );
        assert_eq!(
            ErreurVocal::depuis_api(&ErreurApi::Reseau("hors ligne".into())).message(),
            "Connexion au vocal impossible."
        );
    }
}
