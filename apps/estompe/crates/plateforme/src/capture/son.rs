use std::sync::{Arc, Mutex, MutexGuard};

use crate::traits::{
    CaptureSon, CibleSon, FluxSon, FormatAudio, RecepteurSonCapture, SourceCapture, TypeSource,
};
use crate::{Erreur, Resultat};

pub const FREQUENCE_SON: u32 = 48_000;
pub const CANAUX_SON: u16 = 2;

pub fn pcm16(octets: &[u8]) -> Vec<i16> {
    flottants(octets)
        .map(|f| (f.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

pub fn flottants(octets: &[u8]) -> impl Iterator<Item = f32> + '_ {
    octets
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
}

pub fn cible_du_son(source: &SourceCapture, estompe: u32) -> Option<CibleSon> {
    match source.genre {
        TypeSource::Fenetre => source.processus.map(CibleSon::Application),
        TypeSource::Ecran => Some(CibleSon::SystemeSauf(estompe)),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SonSysteme;

#[cfg(windows)]
impl CaptureSon for SonSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn demarrer(&self, cible: CibleSon, recepteur: RecepteurSonCapture) -> Resultat<FluxSon> {
        super::son_windows::demarrer(cible, recepteur)
    }
}

#[cfg(not(windows))]
impl CaptureSon for SonSysteme {
    fn disponible(&self) -> bool {
        false
    }

    fn demarrer(&self, _: CibleSon, _: RecepteurSonCapture) -> Resultat<FluxSon> {
        Err(Erreur::Indisponible("capture du son"))
    }
}

struct Ouvert {
    cible: CibleSon,
    recepteur: Arc<Mutex<RecepteurSonCapture>>,
    actif: Arc<Mutex<bool>>,
}

#[derive(Default)]
struct EtatFactice {
    ouverts: Vec<Ouvert>,
    refus: Option<String>,
}

#[derive(Clone, Default)]
pub struct SonsFactices {
    etat: Arc<Mutex<EtatFactice>>,
}

struct Garde(Arc<Mutex<bool>>);

impl Drop for Garde {
    fn drop(&mut self) {
        if let Ok(mut actif) = self.0.lock() {
            *actif = false;
        }
    }
}

impl SonsFactices {
    pub fn nouveaux() -> Self {
        Self::default()
    }

    fn etat(&self) -> MutexGuard<'_, EtatFactice> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn actifs(&self) -> Vec<(CibleSon, Arc<Mutex<RecepteurSonCapture>>)> {
        let mut etat = self.etat();
        etat.ouverts
            .retain(|o| o.actif.lock().map(|a| *a).unwrap_or(false));
        etat.ouverts
            .iter()
            .map(|o| (o.cible, o.recepteur.clone()))
            .collect()
    }

    pub fn refuser(&self, refus: Option<&str>) {
        self.etat().refus = refus.map(str::to_string);
    }

    pub fn ouverts(&self) -> Vec<CibleSon> {
        self.actifs().into_iter().map(|(c, _)| c).collect()
    }

    pub fn pousser(&self, entrelace: &[f32], format: FormatAudio) -> usize {
        let actifs = self.actifs();
        for (_, recepteur) in &actifs {
            if let Ok(mut r) = recepteur.lock() {
                r(entrelace, format);
            }
        }
        actifs.len()
    }
}

impl CaptureSon for SonsFactices {
    fn disponible(&self) -> bool {
        true
    }

    fn demarrer(&self, cible: CibleSon, recepteur: RecepteurSonCapture) -> Resultat<FluxSon> {
        let mut etat = self.etat();
        if let Some(refus) = &etat.refus {
            return Err(Erreur::Echec(refus.clone()));
        }
        let actif = Arc::new(Mutex::new(true));
        etat.ouverts.push(Ouvert {
            cible,
            recepteur: Arc::new(Mutex::new(recepteur)),
            actif: actif.clone(),
        });
        Ok(FluxSon::nouveau(Garde(actif)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convertit_et_borne_les_echantillons() {
        let octets: Vec<u8> = [0.0f32, 1.0, -1.0, 2.0, 0.5]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        assert_eq!(
            pcm16(&octets),
            vec![0, i16::MAX, -i16::MAX, i16::MAX, 16383]
        );
        assert_eq!(flottants(&octets[..9]).count(), 2);
    }

    #[test]
    fn la_fenetre_donne_son_processus_l_ecran_tout_sauf_estompe() {
        let mut source = SourceCapture {
            id: "fenetre:1".into(),
            nom: "Lecteur".into(),
            application: String::new(),
            genre: TypeSource::Fenetre,
            largeur: 800,
            hauteur: 600,
            principale: false,
            processus: Some(42),
            estompe: false,
        };
        assert_eq!(cible_du_son(&source, 7), Some(CibleSon::Application(42)));
        source.processus = None;
        assert_eq!(cible_du_son(&source, 7), None);
        source.genre = TypeSource::Ecran;
        assert_eq!(cible_du_son(&source, 7), Some(CibleSon::SystemeSauf(7)));
    }

    #[test]
    fn le_son_n_est_disponible_que_sous_windows() {
        assert_eq!(SonSysteme.disponible(), cfg!(windows));
    }

    #[test]
    fn le_son_factice_s_arrete_quand_le_flux_est_lache() {
        let sons = SonsFactices::nouveaux();
        let recus = Arc::new(Mutex::new(0usize));
        let compte = recus.clone();
        let flux = sons
            .demarrer(
                CibleSon::Application(3),
                Box::new(move |e, _| *compte.lock().unwrap() += e.len()),
            )
            .unwrap();
        let format = FormatAudio {
            frequence: 48_000,
            canaux: 2,
        };
        assert_eq!(sons.pousser(&[0.1; 960], format), 1);
        assert_eq!(*recus.lock().unwrap(), 960);
        assert_eq!(sons.ouverts(), vec![CibleSon::Application(3)]);
        drop(flux);
        assert_eq!(sons.pousser(&[0.1; 960], format), 0);
        sons.refuser(Some("refus"));
        assert!(sons
            .demarrer(CibleSon::SystemeSauf(1), Box::new(|_, _| {}))
            .is_err());
    }
}
