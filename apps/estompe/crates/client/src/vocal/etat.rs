use std::collections::{BTreeMap, BTreeSet};

use crate::audio::ReglagesMicro;

use super::erreurs::CauseMicro;
use super::mixage::Ecoute;
use super::salle::{est_partage, InfoParticipant, Qualite};

pub const ATTRIBUT_MUET: &str = "muted";
pub const ATTRIBUT_SOURD: &str = "deaf";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Inactif,
    Connexion,
    Connecte,
    Reconnexion,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EtatAffiche {
    pub muet: bool,
    pub sourd: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtatVocal {
    pub phase: Phase,
    pub salon: Option<String>,
    pub identite: Option<String>,
    pub micro_coupe: bool,
    pub casque_coupe: bool,
    micro_avant_casque: bool,
    pub micro_impose: bool,
    pub son_impose: bool,
    pub porte_ouverte: bool,
    pub micro_indisponible: Option<CauseMicro>,
    pub erreur: Option<String>,
    pub participants: BTreeMap<String, InfoParticipant>,
    pub parlent: BTreeSet<String>,
    pub micro: Option<String>,
    pub sortie: Option<String>,
    pub traitement: ReglagesMicro,
    pub ecoute: Ecoute,
}

impl Default for EtatVocal {
    fn default() -> Self {
        Self {
            phase: Phase::Inactif,
            salon: None,
            identite: None,
            micro_coupe: false,
            casque_coupe: false,
            micro_avant_casque: false,
            micro_impose: false,
            son_impose: false,
            porte_ouverte: true,
            micro_indisponible: None,
            erreur: None,
            participants: BTreeMap::new(),
            parlent: BTreeSet::new(),
            micro: None,
            sortie: None,
            traitement: ReglagesMicro::default(),
            ecoute: Ecoute::default(),
        }
    }
}

pub fn etats_affiches(
    participants: &BTreeMap<String, InfoParticipant>,
    parlent: &BTreeSet<String>,
) -> BTreeMap<String, EtatAffiche> {
    participants
        .values()
        .map(|p| {
            let attribut = |cle: &str| p.attributs.get(cle).is_some_and(|v| !v.is_empty());
            (
                p.identite.clone(),
                EtatAffiche {
                    muet: attribut(ATTRIBUT_MUET) && !parlent.contains(&p.identite),
                    sourd: attribut(ATTRIBUT_SOURD),
                },
            )
        })
        .collect()
}

impl EtatVocal {
    pub fn actif(&self) -> bool {
        self.phase != Phase::Inactif
    }

    pub fn dans(&self, salon: &str) -> bool {
        self.actif() && self.salon.as_deref() == Some(salon)
    }

    pub fn micro_emet(&self) -> bool {
        matches!(self.phase, Phase::Connecte | Phase::Reconnexion)
            && !self.micro_coupe
            && !self.micro_impose
            && self.micro_indisponible.is_none()
            && self.porte_ouverte
    }

    pub fn son_recu(&self) -> bool {
        !self.casque_coupe && !self.son_impose
    }

    pub fn affiches(&self) -> BTreeMap<String, EtatAffiche> {
        etats_affiches(&self.participants, &self.parlent)
    }

    pub fn qualite(&self, identite: &str) -> Qualite {
        self.participants
            .get(identite)
            .map_or(Qualite::Inconnue, |p| p.qualite)
    }

    pub fn attributs(&self) -> BTreeMap<String, String> {
        let drapeau = |actif: bool| if actif { "1" } else { "" }.to_string();
        BTreeMap::from([
            (ATTRIBUT_MUET.to_string(), drapeau(self.micro_coupe)),
            (ATTRIBUT_SOURD.to_string(), drapeau(self.casque_coupe)),
        ])
    }

    pub fn basculer_micro(&mut self) -> bool {
        if !self.actif() {
            return false;
        }
        if self.casque_coupe {
            return self.basculer_casque();
        }
        if self.micro_coupe && (self.micro_impose || self.micro_indisponible.is_some()) {
            return false;
        }
        self.micro_coupe = !self.micro_coupe;
        true
    }

    pub fn basculer_casque(&mut self) -> bool {
        if !self.actif() {
            return false;
        }
        if self.casque_coupe {
            self.casque_coupe = false;
            self.micro_coupe =
                self.micro_avant_casque || self.micro_impose || self.micro_indisponible.is_some();
        } else {
            self.casque_coupe = true;
            self.micro_avant_casque = self.micro_coupe;
            self.micro_coupe = true;
        }
        true
    }

    pub fn imposer(&mut self, micro: bool, son: bool) {
        self.micro_impose = micro;
        self.son_impose = son;
        if micro {
            self.micro_coupe = true;
        }
    }

    pub fn rejoindre(&mut self, salon: &str) {
        self.phase = Phase::Connexion;
        self.salon = Some(salon.to_string());
        self.erreur = None;
    }

    pub fn connecte(&mut self, identite: String, participants: Vec<InfoParticipant>) {
        self.phase = Phase::Connecte;
        self.identite = Some(identite);
        self.participants = participants
            .into_iter()
            .filter(|p| !est_partage(&p.identite))
            .map(|p| (p.identite.clone(), p))
            .collect();
        self.parlent.clear();
    }

    pub fn reinitialiser(&mut self) {
        *self = Self {
            micro: self.micro.take(),
            sortie: self.sortie.take(),
            traitement: self.traitement,
            ecoute: std::mem::take(&mut self.ecoute),
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connecte() -> EtatVocal {
        let mut e = EtatVocal::default();
        e.rejoindre("v1");
        e.connecte("moi".into(), vec![]);
        e
    }

    fn participant(identite: &str, attributs: &[(&str, &str)]) -> InfoParticipant {
        InfoParticipant {
            identite: identite.into(),
            nom: identite.into(),
            attributs: attributs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..InfoParticipant::default()
        }
    }

    #[test]
    fn affiche_le_micro_coupe_et_la_sourdine() {
        let participants: BTreeMap<_, _> = [
            participant("a", &[("muted", "1")]),
            participant("b", &[("muted", "1"), ("deaf", "1")]),
            participant("c", &[]),
        ]
        .into_iter()
        .map(|p| (p.identite.clone(), p))
        .collect();
        let affiches = etats_affiches(&participants, &BTreeSet::new());
        assert_eq!(
            affiches["a"],
            EtatAffiche {
                muet: true,
                sourd: false
            }
        );
        assert_eq!(
            affiches["b"],
            EtatAffiche {
                muet: true,
                sourd: true
            }
        );
        assert_eq!(affiches["c"], EtatAffiche::default());
    }

    #[test]
    fn n_affiche_jamais_le_micro_coupe_de_qui_parle() {
        let participants: BTreeMap<_, _> = [participant("a", &[("muted", "1")])]
            .into_iter()
            .map(|p| (p.identite.clone(), p))
            .collect();
        let parlent = BTreeSet::from(["a".to_string()]);
        assert!(!etats_affiches(&participants, &parlent)["a"].muet);
    }

    #[test]
    fn la_sourdine_coupe_le_micro_et_le_rend_comme_avant() {
        let mut e = connecte();
        assert!(e.micro_emet() && e.son_recu());
        e.basculer_casque();
        assert!(e.casque_coupe && e.micro_coupe && !e.son_recu() && !e.micro_emet());
        e.basculer_casque();
        assert!(!e.micro_coupe && e.micro_emet());
        e.basculer_micro();
        e.basculer_casque();
        e.basculer_micro();
        assert!(
            !e.casque_coupe && e.micro_coupe,
            "réactiver le micro lève la sourdine"
        );
        assert_eq!(e.attributs()["muted"], "1");
        assert_eq!(e.attributs()["deaf"], "");
    }

    #[test]
    fn coupe_par_un_moderateur_le_micro_ne_se_reactive_pas() {
        let mut e = connecte();
        e.imposer(true, false);
        assert!(e.micro_coupe && !e.micro_emet());
        assert!(!e.basculer_micro());
        assert!(e.micro_coupe);
        e.basculer_casque();
        e.basculer_casque();
        assert!(e.micro_coupe && !e.micro_emet());
        e.imposer(false, true);
        assert!(!e.son_recu());
        assert!(e.basculer_micro() && e.micro_emet());
    }

    #[test]
    fn la_porte_du_micro_et_le_micro_indisponible_bloquent_l_emission() {
        let mut e = connecte();
        e.porte_ouverte = false;
        assert!(!e.micro_emet());
        e.porte_ouverte = true;
        e.micro_indisponible = Some(CauseMicro::Absent);
        e.micro_coupe = true;
        assert!(!e.micro_emet() && !e.basculer_micro());
    }

    #[test]
    fn hors_vocal_rien_ne_bascule_et_la_reinitialisation_garde_les_peripheriques() {
        let mut e = EtatVocal {
            micro: Some("m".into()),
            ..EtatVocal::default()
        };
        assert!(!e.basculer_micro() && !e.basculer_casque());
        e.rejoindre("v1");
        e.connecte(
            "moi".into(),
            vec![participant("moi", &[]), participant("moi#screen", &[])],
        );
        assert_eq!(e.participants.len(), 1);
        assert!(e.dans("v1") && !e.dans("v2"));
        e.reinitialiser();
        assert_eq!(e.phase, Phase::Inactif);
        assert_eq!(e.micro.as_deref(), Some("m"));
    }
}
