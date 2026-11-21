use std::collections::{BTreeMap, BTreeSet};

use crate::audio::ReglagesMicro;

use super::camera::QualiteCamera;
use super::erreurs::CauseMicro;
use super::incrustation::ReglagesIncrustation;
use super::mixage::Ecoute;
use super::partage::{PartageLocal, PlafondPartage, ReglagesPartage};
use super::salle::{est_partage, InfoParticipant, Qualite};
use super::scene::{doit_recevoir, rassembler_spectateurs, SourcePiste, ATTRIBUT_SPECTATEUR};
use super::video::{CleVideo, SourceVideo};

pub const ATTRIBUT_MUET: &str = "muted";
pub const ATTRIBUT_SOURD: &str = "deaf";
pub const ATTRIBUT_INCRUSTATION: &str = "overlay";

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
    pub videos: BTreeSet<CleVideo>,
    pub pistes: BTreeMap<CleVideo, String>,
    pub sons_partage: BTreeSet<String>,
    pub regardes: BTreeMap<String, bool>,
    pub regarder_auto: bool,
    pub vus: BTreeSet<String>,
    pub camera_active: bool,
    pub camera_demarrage: bool,
    pub camera: Option<String>,
    pub qualite_camera: QualiteCamera,
    pub camera_erreur: Option<String>,
    pub partage: Option<PartageLocal>,
    pub partage_demarrage: bool,
    pub partage_erreur: Option<String>,
    pub reglages_partage: ReglagesPartage,
    pub plafond_partage: PlafondPartage,
    pub incrustation: bool,
    pub reglages_incrustation: ReglagesIncrustation,
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
            videos: BTreeSet::new(),
            pistes: BTreeMap::new(),
            sons_partage: BTreeSet::new(),
            regardes: BTreeMap::new(),
            regarder_auto: false,
            vus: BTreeSet::new(),
            camera_active: false,
            camera_demarrage: false,
            camera: None,
            qualite_camera: QualiteCamera::default(),
            camera_erreur: None,
            partage: None,
            partage_demarrage: false,
            partage_erreur: None,
            reglages_partage: ReglagesPartage::default(),
            plafond_partage: PlafondPartage::default(),
            incrustation: false,
            reglages_incrustation: ReglagesIncrustation::default(),
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
            (
                ATTRIBUT_INCRUSTATION.to_string(),
                drapeau(self.incrustation),
            ),
            (ATTRIBUT_SPECTATEUR.to_string(), self.regardees()),
        ])
    }

    pub fn regardees(&self) -> String {
        self.vus.iter().cloned().collect::<Vec<_>>().join(",")
    }

    pub fn propre(&self, proprietaire: &str) -> bool {
        self.identite.as_deref() == Some(proprietaire)
    }

    pub fn regarde(&self, proprietaire: &str) -> bool {
        doit_recevoir(
            SourcePiste::Partage,
            self.regardes.get(proprietaire).copied(),
            self.regarder_auto,
            self.propre(proprietaire),
        )
    }

    pub fn ecoute_partage(&self, proprietaire: &str) -> bool {
        doit_recevoir(
            SourcePiste::SonPartage,
            self.regardes.get(proprietaire).copied(),
            self.regarder_auto,
            self.propre(proprietaire),
        )
    }

    pub fn camera_incrustee(&self, identite: &str) -> bool {
        if self.propre(identite) {
            return self.incrustation;
        }
        self.participants
            .get(identite)
            .and_then(|p| p.attributs.get(ATTRIBUT_INCRUSTATION))
            .is_some_and(|v| !v.is_empty())
    }

    pub fn recoit(&self, cle: &CleVideo) -> bool {
        match cle.source {
            SourceVideo::Camera => !self.camera_incrustee(&cle.identite),
            SourceVideo::Partage => self.regarde(&cle.identite),
        }
    }

    pub fn spectateurs(&self) -> BTreeMap<CleVideo, Vec<(String, String)>> {
        let personnes: Vec<(String, String, Option<String>)> = self
            .participants
            .values()
            .filter(|p| !est_partage(&p.identite))
            .map(|p| {
                let regarde = if self.propre(&p.identite) {
                    Some(self.regardees())
                } else {
                    p.attributs.get(ATTRIBUT_SPECTATEUR).cloned()
                };
                (p.identite.clone(), p.nom.clone(), regarde)
            })
            .collect();
        let parpiste = rassembler_spectateurs(&personnes);
        self.pistes
            .iter()
            .filter_map(|(cle, piste)| parpiste.get(piste).map(|s| (cle.clone(), s.clone())))
            .collect()
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
        self.videos.clear();
        self.pistes.clear();
        self.sons_partage.clear();
        self.vus.clear();
    }

    pub fn video(&self, cle: &CleVideo) -> bool {
        self.videos.contains(cle)
    }

    pub fn reinitialiser(&mut self) {
        *self = Self {
            micro: self.micro.take(),
            sortie: self.sortie.take(),
            traitement: self.traitement,
            ecoute: std::mem::take(&mut self.ecoute),
            porte_ouverte: self.porte_ouverte,
            camera: self.camera.take(),
            qualite_camera: self.qualite_camera,
            camera_erreur: self.camera_erreur.take(),
            partage_erreur: self.partage_erreur.take(),
            reglages_partage: self.reglages_partage,
            reglages_incrustation: self.reglages_incrustation,
            regarder_auto: self.regarder_auto,
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
    fn une_camera_incrustee_dans_le_partage_n_est_pas_recue() {
        let mut e = connecte();
        for p in [
            participant("bob", &[(ATTRIBUT_INCRUSTATION, "1")]),
            participant("carol", &[(ATTRIBUT_INCRUSTATION, "")]),
        ] {
            e.participants.insert(p.identite.clone(), p);
        }
        assert!(e.camera_incrustee("bob"));
        assert!(!e.recoit(&CleVideo::camera("bob")));
        assert!(e.recoit(&CleVideo::camera("carol")));
        assert!(!e.camera_incrustee("moi"));
        assert_eq!(e.attributs()[ATTRIBUT_INCRUSTATION], "");
        e.incrustation = true;
        assert!(e.camera_incrustee("moi"));
        assert_eq!(e.attributs()[ATTRIBUT_INCRUSTATION], "1");
        e.reglages_incrustation.taille = 40;
        e.reinitialiser();
        assert!(!e.incrustation);
        assert_eq!(e.reglages_incrustation.taille, 40);
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
