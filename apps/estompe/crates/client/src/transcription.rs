use crate::http::ErreurApi;
use crate::types::GetMeReponseTranscription;

pub const ECHEC_DEMANDE: &str = "La transcription a échoué";
pub const ECHEC_ENREGISTREMENT: &str = "La transcription n'a pas pu être enregistrée";
pub const TROP_DE_DEMANDES: &str = "Trop de demandes, patientez quelques secondes.";
pub const REPLIEE_AU_DELA: usize = 140;

pub fn proposee(reglage: Option<GetMeReponseTranscription>, alpha: bool) -> bool {
    match reglage {
        Some(GetMeReponseTranscription::On) => true,
        Some(GetMeReponseTranscription::Alpha) => alpha,
        _ => false,
    }
}

pub fn message_d_erreur(erreur: &ErreurApi, repli: &str) -> String {
    match erreur {
        ErreurApi::Reseau(_) | ErreurApi::DelaiDepasse => erreur.message().to_string(),
        _ if erreur.statut() == 429 && erreur.code() != "slowmode" => TROP_DE_DEMANDES.to_string(),
        _ if erreur.statut() == 0 || erreur.statut() >= 500 || erreur.message().is_empty() => {
            repli.to_string()
        }
        _ => erreur.message().to_string(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suivi {
    pub demandee: bool,
    pub enregistrement: bool,
    pub masquee: bool,
    pub depliee: bool,
    pub brouillon: Option<String>,
    pub erreur: String,
}

impl Suivi {
    pub fn demander(&mut self) -> bool {
        if self.demandee || self.enregistrement {
            return false;
        }
        self.demandee = true;
        self.erreur.clear();
        true
    }

    pub fn demande_aboutie(&mut self) {
        self.demandee = false;
        self.erreur.clear();
    }

    pub fn demande_echouee(&mut self, erreur: &ErreurApi) {
        self.demandee = false;
        self.erreur = message_d_erreur(erreur, ECHEC_DEMANDE);
    }

    pub fn basculer_masquee(&mut self) {
        self.masquee = !self.masquee;
    }

    pub fn basculer_depliee(&mut self) {
        self.depliee = !self.depliee;
    }

    pub fn modifier(&mut self, texte: &str) {
        if self.enregistrement {
            return;
        }
        self.brouillon = Some(texte.to_string());
        self.erreur.clear();
    }

    pub fn ecrire(&mut self, texte: &str) {
        if let Some(brouillon) = self.brouillon.as_mut() {
            *brouillon = texte.to_string();
        }
    }

    pub fn annuler(&mut self) {
        if !self.enregistrement {
            self.brouillon = None;
        }
    }

    pub fn enregistrer(&mut self, texte: &str) -> Option<String> {
        if self.enregistrement || self.brouillon.is_none() {
            return None;
        }
        self.brouillon = Some(texte.to_string());
        self.enregistrement = true;
        self.erreur.clear();
        Some(texte.to_string())
    }

    pub fn enregistrement_abouti(&mut self) {
        self.enregistrement = false;
        self.brouillon = None;
        self.erreur.clear();
    }

    pub fn enregistrement_echoue(&mut self, erreur: &ErreurApi) {
        self.enregistrement = false;
        self.erreur = message_d_erreur(erreur, ECHEC_ENREGISTREMENT);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Etat {
    #[default]
    Aucune,
    Proposee,
    Demandee,
    Prete,
    Edition,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Affichage {
    pub etat: Etat,
    pub texte: String,
    pub auto: bool,
    pub repliable: bool,
    pub depliee: bool,
    pub masquee: bool,
    pub modifiable: bool,
    pub brouillon: String,
    pub occupee: bool,
    pub erreur: String,
}

pub struct Piece<'a> {
    pub texte: Option<&'a str>,
    pub auto: bool,
    pub mienne: bool,
}

pub fn affichage(proposee: bool, piece: &Piece, suivi: &Suivi) -> Affichage {
    if !proposee {
        return Affichage::default();
    }
    let texte = piece.texte.filter(|t| !t.is_empty());
    let etat = match (texte, &suivi.brouillon) {
        (Some(_), Some(_)) if piece.mienne => Etat::Edition,
        (Some(_), _) => Etat::Prete,
        (None, _) if suivi.demandee => Etat::Demandee,
        (None, _) => Etat::Proposee,
    };
    let texte = texte.unwrap_or_default();
    Affichage {
        etat,
        texte: texte.to_string(),
        auto: piece.auto,
        repliable: !suivi.masquee && texte.chars().count() > REPLIEE_AU_DELA,
        depliee: suivi.depliee,
        masquee: suivi.masquee,
        modifiable: piece.mienne,
        brouillon: if etat == Etat::Edition {
            suivi.brouillon.clone().unwrap_or_default()
        } else {
            String::new()
        },
        occupee: suivi.enregistrement,
        erreur: suivi.erreur.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use super::*;
    use crate::http::Refus;

    fn refus(statut: u16, code: &str, message: &str) -> Refus {
        Refus {
            statut,
            code: code.to_string(),
            message: message.to_string(),
            donnees: Map::new(),
        }
    }

    fn piece(texte: Option<&str>, mienne: bool) -> Piece<'_> {
        Piece {
            texte,
            auto: true,
            mienne,
        }
    }

    #[test]
    fn proposee_selon_le_serveur_et_les_fonctions_alpha() {
        use GetMeReponseTranscription::*;
        assert!(proposee(Some(On), false));
        assert!(proposee(Some(Alpha), true));
        assert!(!proposee(Some(Alpha), false));
        assert!(!proposee(Some(Vide), true));
        assert!(!proposee(Some(Inconnu), true));
        assert!(!proposee(None, true));
        let moi: crate::types::GetMeReponse =
            serde_json::from_value(json!({"transcription": "on"})).unwrap();
        assert!(proposee(moi.transcription, false));
    }

    #[test]
    fn messages_d_erreur_comme_le_front() {
        let e = |statut, code| ErreurApi::Autre(refus(statut, code, "détail du serveur"));
        assert_eq!(
            message_d_erreur(&e(429, "rate_limited"), ECHEC_DEMANDE),
            TROP_DE_DEMANDES
        );
        assert_eq!(
            message_d_erreur(&e(429, "transcription_busy"), ECHEC_DEMANDE),
            TROP_DE_DEMANDES
        );
        assert_eq!(
            message_d_erreur(
                &ErreurApi::Serveur(refus(502, "transcription_failed", "x")),
                ECHEC_DEMANDE
            ),
            ECHEC_DEMANDE
        );
        assert_eq!(
            message_d_erreur(
                &ErreurApi::Serveur(refus(501, "transcription_disabled", "x")),
                ECHEC_DEMANDE
            ),
            ECHEC_DEMANDE
        );
        assert_eq!(
            message_d_erreur(&e(400, "invalid_transcript"), ECHEC_ENREGISTREMENT),
            "détail du serveur"
        );
        assert_eq!(
            message_d_erreur(&ErreurApi::Autre(refus(400, "x", "")), ECHEC_ENREGISTREMENT),
            ECHEC_ENREGISTREMENT
        );
        assert_eq!(
            message_d_erreur(&ErreurApi::Reseau("refusé".into()), ECHEC_DEMANDE),
            "Connexion au serveur impossible."
        );
    }

    #[test]
    fn rien_quand_la_transcription_n_est_pas_proposee() {
        let mut suivi = Suivi::default();
        suivi.demander();
        assert_eq!(
            affichage(false, &piece(Some("bonjour"), true), &suivi),
            Affichage::default()
        );
    }

    #[test]
    fn demandee_puis_prete() {
        let mut suivi = Suivi::default();
        let a = affichage(true, &piece(None, false), &suivi);
        assert_eq!(a.etat, Etat::Proposee);
        assert!(suivi.demander());
        assert!(!suivi.demander());
        assert_eq!(
            affichage(true, &piece(None, false), &suivi).etat,
            Etat::Demandee
        );
        assert_eq!(
            affichage(true, &piece(Some(""), false), &suivi).etat,
            Etat::Demandee
        );
        suivi.demande_aboutie();
        let a = affichage(true, &piece(Some("Bonjour à tous"), false), &suivi);
        assert_eq!(a.etat, Etat::Prete);
        assert_eq!(a.texte, "Bonjour à tous");
        assert!(a.auto && !a.repliable && !a.modifiable && a.erreur.is_empty());
    }

    #[test]
    fn la_transcription_arrivee_par_le_temps_reel_remplace_l_attente() {
        let mut suivi = Suivi::default();
        suivi.demander();
        let a = affichage(
            true,
            &piece(Some("arrivée par message.updated"), false),
            &suivi,
        );
        assert_eq!(a.etat, Etat::Prete);
        assert_eq!(a.texte, "arrivée par message.updated");
    }

    #[test]
    fn echec_puis_nouvelle_demande() {
        let mut suivi = Suivi::default();
        suivi.demander();
        suivi.demande_echouee(&ErreurApi::Serveur(refus(502, "transcription_failed", "x")));
        let a = affichage(true, &piece(None, false), &suivi);
        assert_eq!(a.etat, Etat::Proposee);
        assert_eq!(a.erreur, ECHEC_DEMANDE);
        assert!(suivi.demander());
        assert!(affichage(true, &piece(None, false), &suivi)
            .erreur
            .is_empty());
    }

    #[test]
    fn repliee_au_dela_de_140_caracteres_et_masquable() {
        let long = "é".repeat(141);
        let mut suivi = Suivi::default();
        assert!(!affichage(true, &piece(Some(&"é".repeat(140)), false), &suivi).repliable);
        let a = affichage(true, &piece(Some(&long), false), &suivi);
        assert!(a.repliable && !a.depliee);
        suivi.basculer_depliee();
        assert!(affichage(true, &piece(Some(&long), false), &suivi).depliee);
        suivi.basculer_masquee();
        let a = affichage(true, &piece(Some(&long), false), &suivi);
        assert!(a.masquee && !a.repliable);
        assert_eq!(a.etat, Etat::Prete);
        suivi.basculer_masquee();
        assert!(!affichage(true, &piece(Some(&long), false), &suivi).masquee);
    }

    #[test]
    fn l_auteur_modifie_et_enregistre() {
        let mut suivi = Suivi::default();
        assert!(affichage(true, &piece(Some("bonjur"), true), &suivi).modifiable);
        suivi.modifier("bonjur");
        let a = affichage(true, &piece(Some("bonjur"), true), &suivi);
        assert_eq!(a.etat, Etat::Edition);
        assert_eq!(a.brouillon, "bonjur");
        suivi.ecrire("bonjour");
        assert_eq!(
            affichage(true, &piece(Some("bonjur"), true), &suivi).brouillon,
            "bonjour"
        );
        assert_eq!(
            suivi.enregistrer("bonjour !"),
            Some("bonjour !".to_string())
        );
        assert_eq!(suivi.enregistrer("encore"), None);
        let a = affichage(true, &piece(Some("bonjur"), true), &suivi);
        assert!(a.occupee && a.etat == Etat::Edition);
        suivi.annuler();
        assert!(suivi.brouillon.is_some());
        suivi.enregistrement_abouti();
        let a = affichage(true, &piece(Some("bonjour !"), true), &suivi);
        assert_eq!(a.etat, Etat::Prete);
        assert!(!a.occupee);
    }

    #[test]
    fn un_enregistrement_refuse_garde_le_brouillon() {
        let mut suivi = Suivi::default();
        suivi.modifier("texte");
        suivi.enregistrer("texte corrigé");
        suivi.enregistrement_echoue(&ErreurApi::Interdit(refus(403, "forbidden", "")));
        let a = affichage(true, &piece(Some("texte"), true), &suivi);
        assert_eq!(a.etat, Etat::Edition);
        assert_eq!(a.brouillon, "texte corrigé");
        assert_eq!(a.erreur, ECHEC_ENREGISTREMENT);
        suivi.annuler();
        assert_eq!(
            affichage(true, &piece(Some("texte"), true), &suivi).etat,
            Etat::Prete
        );
    }

    #[test]
    fn retirer_la_transcription_revient_au_bouton() {
        let mut suivi = Suivi::default();
        suivi.modifier("texte");
        suivi.enregistrer("");
        suivi.enregistrement_abouti();
        assert_eq!(
            affichage(true, &piece(Some(""), true), &suivi).etat,
            Etat::Proposee
        );
    }

    #[test]
    fn seul_l_auteur_peut_passer_en_edition() {
        let mut suivi = Suivi::default();
        suivi.modifier("texte");
        assert_eq!(
            affichage(true, &piece(Some("texte"), false), &suivi).etat,
            Etat::Prete
        );
    }
}
