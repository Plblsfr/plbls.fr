use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use estompe_plateforme::{Resultat, SystemeAudio};

use super::format::TamponLecture;
use super::micro::{Apm, ChaineMicro, MesureMicro, NiveauMicro, ReglagesMicro};
use super::peripheriques::{Capture, Lecture};

pub struct EssaiMicro {
    _capture: Capture,
    lecture: Option<Lecture>,
    chaine: Arc<Mutex<ChaineMicro>>,
    mesure: Arc<MesureMicro>,
    retour: Arc<AtomicBool>,
    tampon: Arc<Mutex<TamponLecture>>,
}

impl EssaiMicro {
    pub fn ouvrir(
        systeme: Arc<dyn SystemeAudio>,
        micro: Option<String>,
        sortie: Option<String>,
        reglages: ReglagesMicro,
        apm: Option<Arc<Apm>>,
    ) -> Resultat<Self> {
        let mesure = Arc::new(MesureMicro::default());
        let chaine = Arc::new(Mutex::new(ChaineMicro::nouvelle(
            reglages,
            apm,
            mesure.clone(),
        )));
        let tampon = Arc::new(Mutex::new(TamponLecture::default()));
        let retour = Arc::new(AtomicBool::new(false));
        let lecture = Lecture::ouvrir(systeme.clone(), sortie, tampon.clone()).ok();
        let (vers_chaine, vers_tampon, vers_retour) =
            (chaine.clone(), tampon.clone(), retour.clone());
        let capture = Capture::ouvrir(
            systeme,
            micro,
            Box::new(move |trame| {
                let Ok(mut chaine) = vers_chaine.lock() else {
                    return;
                };
                let traitee = chaine.traiter(trame);
                if vers_retour.load(Ordering::Relaxed) {
                    if let Ok(mut tampon) = vers_tampon.lock() {
                        tampon.pousser(traitee);
                    }
                }
            }),
        )?;
        Ok(Self {
            _capture: capture,
            lecture,
            chaine,
            mesure,
            retour,
            tampon,
        })
    }

    pub fn niveau(&self) -> NiveauMicro {
        self.mesure.lire()
    }

    pub fn regler(&self, reglages: ReglagesMicro) {
        if let Ok(mut chaine) = self.chaine.lock() {
            chaine.appliquer(reglages);
        }
    }

    pub fn ecouter(&self, retour: bool) {
        self.retour.store(retour, Ordering::Relaxed);
        if !retour {
            if let Ok(mut tampon) = self.tampon.lock() {
                tampon.vider();
            }
        }
    }

    pub fn ecoute(&self) -> bool {
        self.retour.load(Ordering::Relaxed)
    }

    pub fn avec_retour(&self) -> bool {
        self.lecture.is_some()
    }
}

#[cfg(test)]
mod tests {
    use estompe_plateforme::{FormatAudio, SensAudio};

    use super::*;
    use crate::audio::factice::SystemeFactice;
    use crate::audio::{ModeBruit, ECHANTILLONS_PAR_TRAME};

    fn systeme() -> SystemeFactice {
        let s = SystemeFactice::nouveau();
        let format = FormatAudio {
            frequence: 48_000,
            canaux: 1,
        };
        s.brancher(SensAudio::Entree, "micro", "Micro", format);
        s.brancher(SensAudio::Sortie, "hp", "Haut-parleurs", format);
        s
    }

    fn sans_bruit() -> ReglagesMicro {
        ReglagesMicro {
            bruit: ModeBruit::Aucun,
            ..ReglagesMicro::default()
        }
    }

    #[test]
    fn mesure_le_niveau_et_renvoie_sa_voix_traitee_seulement_sur_demande() {
        let s = systeme();
        let essai =
            EssaiMicro::ouvrir(Arc::new(s.clone()), None, None, sans_bruit(), None).unwrap();
        assert!(essai.avec_retour() && !essai.ecoute());
        s.pousser("micro", &[0.25; 3 * ECHANTILLONS_PAR_TRAME + 1]);
        assert!(
            (essai.niveau().db + 12.04).abs() < 0.1,
            "{:?}",
            essai.niveau()
        );
        let silence = s.tirer("hp", ECHANTILLONS_PAR_TRAME).unwrap();
        assert!(silence.iter().all(|e| *e == 0.0));
        essai.ecouter(true);
        essai.regler(ReglagesMicro {
            gain: 200,
            ..sans_bruit()
        });
        s.pousser("micro", &[0.25; 4 * ECHANTILLONS_PAR_TRAME]);
        s.tirer("hp", ECHANTILLONS_PAR_TRAME).unwrap();
        let retour = s.tirer("hp", ECHANTILLONS_PAR_TRAME).unwrap();
        assert!(
            retour.iter().all(|e| (e - 0.5).abs() < 1e-3),
            "{:?}",
            &retour[..4]
        );
        essai.ecouter(false);
        drop(essai);
        assert!(s.flux_ouverts().is_empty());
    }

    #[test]
    fn sans_micro_l_essai_ne_s_ouvre_pas() {
        let s = SystemeFactice::nouveau();
        assert!(EssaiMicro::ouvrir(Arc::new(s), None, None, sans_bruit(), None).is_err());
    }
}
