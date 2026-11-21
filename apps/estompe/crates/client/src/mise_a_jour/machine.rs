use std::path::PathBuf;

use crate::version::MiseAJourDisponible;

pub const ATTENTE_AVANT_BLOCAGE: u32 = 120;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Contexte {
    pub saisie_en_cours: bool,
    pub en_appel: bool,
}

impl Contexte {
    pub fn occupe(&self) -> bool {
        self.saisie_en_cours || self.en_appel
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Inactif,
    Verification,
    AJour,
    Disponible,
    Telechargement {
        fait: u64,
        total: u64,
    },
    Prete,
    AttenteFinAppel,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatMaj {
    pub automatique: bool,
    pub phase: Phase,
    pub disponible: Option<MiseAJourDisponible>,
    pub erreur: String,
    pub compte_a_rebours: u32,
    pub bloquee: bool,
    pub refus: Option<String>,
    pub ecartee: String,
    pub verifications: u64,
}

impl EtatMaj {
    pub fn imposee(&self) -> bool {
        self.refus.is_some() || self.disponible.as_ref().is_some_and(|d| d.imposee)
    }

    pub fn en_cours(&self) -> bool {
        matches!(self.phase, Phase::Telechargement { .. } | Phase::Prete)
    }

    pub fn progression(&self) -> f32 {
        match self.phase {
            Phase::Telechargement { fait, total } if total > 0 => {
                (fait.min(total) as f64 / total as f64) as f32
            }
            Phase::Prete => 1.0,
            _ => 0.0,
        }
    }

    pub fn proposee(&self) -> bool {
        self.disponible
            .as_ref()
            .is_some_and(|d| !d.imposee && d.version != self.ecartee)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvenementMaj {
    Demarrer,
    Verifier { manuelle: bool },
    PremierPlan,
    ManifesteLu(Result<Option<MiseAJourDisponible>, String>),
    Seconde,
    RefusDeVersion(String),
    Installer,
    Confirmer,
    FinAppel,
    PlusTard,
    Progression { fait: u64, total: u64 },
    Telecharge(Result<PathBuf, String>),
    Lance(Result<(), String>),
}

impl EvenementMaj {
    pub fn demande_le_contexte(&self) -> bool {
        matches!(
            self,
            Self::ManifesteLu(_) | Self::RefusDeVersion(_) | Self::Installer
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffetMaj {
    Programmer,
    LireManifeste,
    Decompter,
    Bloquer {
        message: String,
        version_minimale: String,
    },
    Telecharger(MiseAJourDisponible),
    Lancer {
        installeur: PathBuf,
        mise_a_jour: MiseAJourDisponible,
    },
    Redemarrer,
}

#[derive(Debug, Clone, Default)]
pub struct MachineMaj {
    etat: EtatMaj,
    programmee: bool,
    manuelle: bool,
    installer_apres_lecture: bool,
    decompte: bool,
}

impl MachineMaj {
    pub fn nouvelle(automatique: bool) -> Self {
        Self {
            etat: EtatMaj {
                automatique,
                ..EtatMaj::default()
            },
            ..Self::default()
        }
    }

    pub fn etat(&self) -> &EtatMaj {
        &self.etat
    }

    fn occupee(&self) -> bool {
        matches!(
            self.etat.phase,
            Phase::Verification
                | Phase::Telechargement { .. }
                | Phase::Prete
                | Phase::AttenteFinAppel
        )
    }

    fn lire(&mut self, manuelle: bool) -> Vec<EffetMaj> {
        if self.occupee() {
            return Vec::new();
        }
        if manuelle {
            self.etat.erreur.clear();
        }
        self.manuelle = manuelle;
        self.etat.phase = Phase::Verification;
        vec![EffetMaj::LireManifeste]
    }

    fn bloquer(&mut self) -> Vec<EffetMaj> {
        self.decompte = false;
        self.etat.compte_a_rebours = 0;
        self.etat.bloquee = true;
        let disponible = self.etat.disponible.clone().unwrap_or_default();
        let message = match &self.etat.refus {
            Some(refus) if !refus.is_empty() => refus.clone(),
            _ => disponible.message,
        };
        vec![EffetMaj::Bloquer {
            message,
            version_minimale: disponible.version,
        }]
    }

    fn armer(&mut self, contexte: Contexte) -> Vec<EffetMaj> {
        if self.etat.bloquee || self.decompte {
            return Vec::new();
        }
        if !contexte.occupe() {
            return self.bloquer();
        }
        self.decompte = true;
        self.etat.compte_a_rebours = ATTENTE_AVANT_BLOCAGE;
        vec![EffetMaj::Decompter]
    }

    fn installer(&mut self, contexte: Contexte) -> Vec<EffetMaj> {
        let Some(disponible) = self.etat.disponible.clone() else {
            return Vec::new();
        };
        self.etat.erreur.clear();
        if contexte.en_appel {
            self.etat.phase = Phase::AttenteFinAppel;
            return Vec::new();
        }
        self.telecharger(disponible)
    }

    fn telecharger(&mut self, disponible: MiseAJourDisponible) -> Vec<EffetMaj> {
        self.etat.phase = Phase::Telechargement { fait: 0, total: 0 };
        vec![EffetMaj::Telecharger(disponible)]
    }

    fn apres_echec(&mut self, erreur: String) {
        self.etat.phase = if self.etat.disponible.is_some() {
            Phase::Disponible
        } else {
            Phase::Inactif
        };
        self.etat.erreur = erreur;
    }

    fn manifeste_lu(
        &mut self,
        resultat: Result<Option<MiseAJourDisponible>, String>,
        contexte: Contexte,
    ) -> Vec<EffetMaj> {
        let installer = std::mem::take(&mut self.installer_apres_lecture);
        let disponible = match resultat {
            Ok(disponible) => disponible,
            Err(erreur) => {
                let montree = self.manuelle || installer;
                self.apres_echec(if montree { erreur } else { String::new() });
                return Vec::new();
            }
        };
        self.etat.verifications += 1;
        self.etat.erreur.clear();
        self.etat.phase = if disponible.is_some() {
            Phase::Disponible
        } else {
            Phase::AJour
        };
        self.etat.disponible = disponible;
        let mut effets = Vec::new();
        if self.etat.imposee() {
            effets.extend(self.armer(contexte));
        }
        if installer {
            if self.etat.disponible.is_some() {
                effets.extend(self.installer(contexte));
            } else {
                self.etat.erreur = super::ErreurMiseAJour::AucuneMiseAJour.to_string();
            }
        }
        effets
    }

    pub fn traiter(&mut self, evenement: EvenementMaj, contexte: Contexte) -> Vec<EffetMaj> {
        if !self.etat.automatique {
            return Vec::new();
        }
        match evenement {
            EvenementMaj::Demarrer if !self.programmee => {
                self.programmee = true;
                vec![EffetMaj::Programmer]
            }
            EvenementMaj::Demarrer => Vec::new(),
            EvenementMaj::Verifier { manuelle } => self.lire(manuelle),
            EvenementMaj::PremierPlan => self.lire(false),
            EvenementMaj::ManifesteLu(resultat) if self.etat.phase == Phase::Verification => {
                self.manifeste_lu(resultat, contexte)
            }
            EvenementMaj::ManifesteLu(_) => Vec::new(),
            EvenementMaj::Seconde if self.decompte => {
                self.etat.compte_a_rebours = self.etat.compte_a_rebours.saturating_sub(1);
                if self.etat.compte_a_rebours == 0 {
                    self.bloquer()
                } else {
                    Vec::new()
                }
            }
            EvenementMaj::Seconde => Vec::new(),
            EvenementMaj::RefusDeVersion(message) => {
                self.etat.refus = Some(message);
                let mut effets = self.armer(contexte);
                effets.extend(self.lire(false));
                effets
            }
            EvenementMaj::Installer => {
                if self.etat.en_cours() || self.etat.phase == Phase::AttenteFinAppel {
                    return Vec::new();
                }
                if self.etat.disponible.is_some() && self.etat.phase != Phase::Verification {
                    return self.installer(contexte);
                }
                self.installer_apres_lecture = true;
                self.etat.erreur.clear();
                if self.etat.phase == Phase::Verification {
                    Vec::new()
                } else {
                    self.lire(false)
                }
            }
            EvenementMaj::Confirmer | EvenementMaj::FinAppel
                if self.etat.phase == Phase::AttenteFinAppel =>
            {
                match self.etat.disponible.clone() {
                    Some(disponible) => self.telecharger(disponible),
                    None => Vec::new(),
                }
            }
            EvenementMaj::Confirmer | EvenementMaj::FinAppel => Vec::new(),
            EvenementMaj::PlusTard => {
                if let Some(disponible) = &self.etat.disponible {
                    self.etat.ecartee = disponible.version.clone();
                }
                Vec::new()
            }
            EvenementMaj::Progression { fait, total } => {
                if matches!(self.etat.phase, Phase::Telechargement { .. }) {
                    self.etat.phase = Phase::Telechargement { fait, total };
                }
                Vec::new()
            }
            EvenementMaj::Telecharge(resultat) => {
                if !matches!(self.etat.phase, Phase::Telechargement { .. }) {
                    return Vec::new();
                }
                match (resultat, self.etat.disponible.clone()) {
                    (Ok(installeur), Some(mise_a_jour)) => {
                        self.etat.phase = Phase::Prete;
                        vec![EffetMaj::Lancer {
                            installeur,
                            mise_a_jour,
                        }]
                    }
                    (Ok(_), None) => {
                        self.apres_echec(super::ErreurMiseAJour::AucuneMiseAJour.to_string());
                        Vec::new()
                    }
                    (Err(erreur), _) => {
                        self.apres_echec(erreur);
                        Vec::new()
                    }
                }
            }
            EvenementMaj::Lance(resultat) => {
                if self.etat.phase != Phase::Prete {
                    return Vec::new();
                }
                match resultat {
                    Ok(()) => vec![EffetMaj::Redemarrer],
                    Err(erreur) => {
                        self.apres_echec(erreur);
                        Vec::new()
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{disponibilite, Publication};

    const LIBRE: Contexte = Contexte {
        saisie_en_cours: false,
        en_appel: false,
    };
    const ECRIT: Contexte = Contexte {
        saisie_en_cours: true,
        en_appel: false,
    };
    const EN_APPEL: Contexte = Contexte {
        saisie_en_cours: false,
        en_appel: true,
    };

    fn lu(json: &str) -> Result<Option<MiseAJourDisponible>, String> {
        let publication: Publication = serde_json::from_str(json).unwrap();
        Ok(disponibilite("0.1.0", &publication))
    }

    fn reportable() -> Result<Option<MiseAJourDisponible>, String> {
        lu(r#"{"version":"0.2.0","notes":"Pastille de mentions."}"#)
    }

    fn verifiee(
        resultat: Result<Option<MiseAJourDisponible>, String>,
        contexte: Contexte,
    ) -> (MachineMaj, Vec<EffetMaj>) {
        let mut m = MachineMaj::nouvelle(true);
        assert_eq!(
            m.traiter(EvenementMaj::Verifier { manuelle: false }, LIBRE),
            [EffetMaj::LireManifeste]
        );
        let effets = m.traiter(EvenementMaj::ManifesteLu(resultat), contexte);
        (m, effets)
    }

    #[test]
    fn lit_le_json_propose_et_ne_bloque_pas_un_utilisateur_occupe_avant_le_delai() {
        let (mut m, effets) = verifiee(
            lu(r#"{"version":"999.0.0","mandatory":true,"message":"urgent"}"#),
            ECRIT,
        );
        assert_eq!(effets, [EffetMaj::Decompter]);
        let dispo = m.etat().disponible.clone().unwrap();
        assert_eq!(
            (
                dispo.version.as_str(),
                dispo.imposee,
                dispo.message.as_str()
            ),
            ("999.0.0", true, "urgent")
        );
        assert!(!m.etat().bloquee);
        assert_eq!(m.etat().compte_a_rebours, 120);
        for _ in 0..119 {
            assert!(m.traiter(EvenementMaj::Seconde, ECRIT).is_empty());
        }
        assert!(!m.etat().bloquee);
        assert_eq!(m.etat().compte_a_rebours, 1);
        assert_eq!(
            m.traiter(EvenementMaj::Seconde, ECRIT),
            [EffetMaj::Bloquer {
                message: "urgent".into(),
                version_minimale: "999.0.0".into()
            }]
        );
        assert!(m.etat().bloquee);
        assert_eq!(m.etat().compte_a_rebours, 0);
        assert!(m.traiter(EvenementMaj::Seconde, ECRIT).is_empty());
    }

    #[test]
    fn un_utilisateur_en_appel_a_aussi_son_delai() {
        let (m, effets) = verifiee(
            lu(r#"{"version":"999.0.0","minVersion":"998.0.0"}"#),
            EN_APPEL,
        );
        assert_eq!(effets, [EffetMaj::Decompter]);
        assert!(!m.etat().bloquee);
    }

    #[test]
    fn bloque_aussitot_un_utilisateur_qui_n_ecrit_pas() {
        let (m, effets) = verifiee(lu(r#"{"version":"999.0.0","minVersion":"998.0.0"}"#), LIBRE);
        assert!(matches!(effets.as_slice(), [EffetMaj::Bloquer { .. }]));
        assert!(m.etat().bloquee);
        assert!(m.etat().imposee());
    }

    #[test]
    fn traduit_le_refus_426_en_ecran_de_mise_a_jour() {
        let mut m = MachineMaj::nouvelle(true);
        let effets = m.traiter(
            EvenementMaj::RefusDeVersion("version trop ancienne".into()),
            LIBRE,
        );
        assert_eq!(
            effets,
            [
                EffetMaj::Bloquer {
                    message: "version trop ancienne".into(),
                    version_minimale: String::new()
                },
                EffetMaj::LireManifeste
            ]
        );
        assert_eq!(m.etat().refus.as_deref(), Some("version trop ancienne"));
        assert!(m.etat().imposee());
        assert!(m.etat().bloquee);
    }

    #[test]
    fn ne_programme_les_verifications_qu_une_fois() {
        let mut m = MachineMaj::nouvelle(true);
        assert_eq!(
            m.traiter(EvenementMaj::Demarrer, LIBRE),
            [EffetMaj::Programmer]
        );
        assert!(m.traiter(EvenementMaj::Demarrer, LIBRE).is_empty());
    }

    #[test]
    fn une_verification_a_la_fois_et_rien_pendant_l_installation() {
        let mut m = MachineMaj::nouvelle(true);
        assert_eq!(
            m.traiter(EvenementMaj::PremierPlan, LIBRE),
            [EffetMaj::LireManifeste]
        );
        assert!(m
            .traiter(EvenementMaj::Verifier { manuelle: true }, LIBRE)
            .is_empty());
        m.traiter(EvenementMaj::ManifesteLu(reportable()), LIBRE);
        assert_eq!(m.etat().phase, Phase::Disponible);
        assert!(m.etat().proposee());
        assert!(matches!(
            m.traiter(EvenementMaj::Installer, LIBRE).as_slice(),
            [EffetMaj::Telecharger(_)]
        ));
        assert!(m
            .traiter(EvenementMaj::Verifier { manuelle: true }, LIBRE)
            .is_empty());
        assert!(m.traiter(EvenementMaj::Installer, LIBRE).is_empty());
    }

    #[test]
    fn l_erreur_n_est_montree_que_pour_une_verification_manuelle() {
        let (m, _) = verifiee(Err("injoignable".into()), LIBRE);
        assert_eq!(m.etat().erreur, "");
        assert_eq!(m.etat().phase, Phase::Inactif);
        let mut m = MachineMaj::nouvelle(true);
        m.traiter(EvenementMaj::Verifier { manuelle: true }, LIBRE);
        m.traiter(EvenementMaj::ManifesteLu(Err("injoignable".into())), LIBRE);
        assert_eq!(m.etat().erreur, "injoignable");
        m.traiter(EvenementMaj::Verifier { manuelle: true }, LIBRE);
        assert_eq!(m.etat().erreur, "", "un nouvel essai efface l'erreur");
        m.traiter(EvenementMaj::ManifesteLu(Ok(None)), LIBRE);
        assert_eq!(m.etat().phase, Phase::AJour);
        assert_eq!(m.etat().verifications, 1);
        m.traiter(EvenementMaj::Verifier { manuelle: true }, LIBRE);
        m.traiter(EvenementMaj::ManifesteLu(Err("injoignable".into())), LIBRE);
        m.traiter(EvenementMaj::Verifier { manuelle: false }, LIBRE);
        assert_eq!(
            m.etat().erreur,
            "injoignable",
            "une vérification automatique ne l'efface pas d'avance"
        );
        m.traiter(EvenementMaj::ManifesteLu(Ok(None)), LIBRE);
        assert_eq!(
            m.etat().erreur,
            "",
            "une vérification réussie efface l'erreur"
        );
    }

    #[test]
    fn plus_tard_ecarte_cette_version_seulement() {
        let (mut m, _) = verifiee(reportable(), LIBRE);
        m.traiter(EvenementMaj::PlusTard, LIBRE);
        assert_eq!(m.etat().ecartee, "0.2.0");
        assert!(!m.etat().proposee());
        m.traiter(EvenementMaj::Verifier { manuelle: false }, LIBRE);
        m.traiter(
            EvenementMaj::ManifesteLu(lu(r#"{"version":"0.3.0"}"#)),
            LIBRE,
        );
        assert!(m.etat().proposee());
    }

    #[test]
    fn installation_complete_jusqu_au_redemarrage() {
        let (mut m, _) = verifiee(reportable(), LIBRE);
        let dispo = m.etat().disponible.clone().unwrap();
        assert_eq!(
            m.traiter(EvenementMaj::Installer, LIBRE),
            [EffetMaj::Telecharger(dispo.clone())]
        );
        m.traiter(
            EvenementMaj::Progression {
                fait: 40,
                total: 100,
            },
            LIBRE,
        );
        assert!((m.etat().progression() - 0.4).abs() < 1e-6);
        assert!(m.etat().en_cours());
        let chemin = PathBuf::from("Estompe-natif-beta-0.2.0-setup.exe");
        assert_eq!(
            m.traiter(EvenementMaj::Telecharge(Ok(chemin.clone())), LIBRE),
            [EffetMaj::Lancer {
                installeur: chemin,
                mise_a_jour: dispo
            }]
        );
        assert_eq!(m.etat().phase, Phase::Prete);
        assert_eq!(
            m.traiter(EvenementMaj::Lance(Ok(())), LIBRE),
            [EffetMaj::Redemarrer]
        );
    }

    #[test]
    fn un_echec_de_telechargement_ou_d_installeur_laisse_l_app_en_place_avec_l_erreur() {
        let (mut m, _) = verifiee(reportable(), LIBRE);
        m.traiter(EvenementMaj::Installer, LIBRE);
        assert!(m
            .traiter(
                EvenementMaj::Telecharge(Err("signature invalide".into())),
                LIBRE
            )
            .is_empty());
        assert_eq!(m.etat().phase, Phase::Disponible);
        assert_eq!(m.etat().erreur, "signature invalide");
        m.traiter(EvenementMaj::Installer, LIBRE);
        assert_eq!(m.etat().erreur, "");
        m.traiter(EvenementMaj::Telecharge(Ok(PathBuf::from("i.exe"))), LIBRE);
        assert!(m
            .traiter(EvenementMaj::Lance(Err("refusé".into())), LIBRE)
            .is_empty());
        assert_eq!(m.etat().phase, Phase::Disponible);
        assert_eq!(m.etat().erreur, "refusé");
    }

    #[test]
    fn en_appel_l_installation_attend_la_fin_de_l_appel_ou_une_confirmation() {
        let (mut m, _) = verifiee(reportable(), LIBRE);
        assert!(m.traiter(EvenementMaj::Installer, EN_APPEL).is_empty());
        assert_eq!(m.etat().phase, Phase::AttenteFinAppel);
        assert!(m.traiter(EvenementMaj::Installer, EN_APPEL).is_empty());
        assert!(matches!(
            m.traiter(EvenementMaj::FinAppel, LIBRE).as_slice(),
            [EffetMaj::Telecharger(_)]
        ));
        let (mut m, _) = verifiee(reportable(), LIBRE);
        m.traiter(EvenementMaj::Installer, EN_APPEL);
        assert!(matches!(
            m.traiter(EvenementMaj::Confirmer, EN_APPEL).as_slice(),
            [EffetMaj::Telecharger(_)]
        ));
        assert!(m.traiter(EvenementMaj::FinAppel, LIBRE).is_empty());
    }

    #[test]
    fn installer_sans_manifeste_le_lit_puis_installe() {
        let mut m = MachineMaj::nouvelle(true);
        assert_eq!(
            m.traiter(EvenementMaj::Installer, LIBRE),
            [EffetMaj::LireManifeste]
        );
        assert!(matches!(
            m.traiter(EvenementMaj::ManifesteLu(reportable()), LIBRE)
                .as_slice(),
            [EffetMaj::Telecharger(_)]
        ));
        let mut m = MachineMaj::nouvelle(true);
        m.traiter(EvenementMaj::Installer, LIBRE);
        assert!(m
            .traiter(EvenementMaj::ManifesteLu(Ok(None)), LIBRE)
            .is_empty());
        assert_eq!(
            m.etat().erreur,
            "Aucune mise à jour à installer pour l'instant."
        );
        let mut m = MachineMaj::nouvelle(true);
        m.traiter(EvenementMaj::Installer, LIBRE);
        m.traiter(EvenementMaj::ManifesteLu(Err("injoignable".into())), LIBRE);
        assert_eq!(m.etat().erreur, "injoignable");
    }

    #[test]
    fn sans_installation_possible_rien_n_est_verifie() {
        let mut m = MachineMaj::nouvelle(false);
        for evenement in [
            EvenementMaj::Demarrer,
            EvenementMaj::Verifier { manuelle: true },
            EvenementMaj::Installer,
            EvenementMaj::RefusDeVersion("x".into()),
        ] {
            assert!(m.traiter(evenement, LIBRE).is_empty());
        }
        assert_eq!(*m.etat(), EtatMaj::default());
    }
}
