use std::sync::{Arc, Mutex, MutexGuard};

use crate::traits::{Calque, ImagePremultipliee, Overlay, ZoneEcran};
use crate::Resultat;

#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlaySysteme;

#[cfg(windows)]
impl Overlay for OverlaySysteme {
    fn disponible(&self) -> bool {
        true
    }
    fn calque(&self) -> Resultat<Box<dyn Calque>> {
        windows::CalqueWindows::nouveau()
            .map(|c| Box::new(c) as Box<dyn Calque>)
            .ok_or_else(|| crate::Erreur::Echec("fenêtre d'overlay impossible à créer".into()))
    }
    fn pomper(&self) {
        windows::pomper();
    }
    fn ecran_principal(&self) -> Option<ZoneEcran> {
        windows::ecran_principal()
    }
    fn echelle(&self, ecran: &ZoneEcran) -> f32 {
        windows::echelle(ecran)
    }
}

#[cfg(not(windows))]
impl Overlay for OverlaySysteme {
    fn disponible(&self) -> bool {
        false
    }
    fn calque(&self) -> Resultat<Box<dyn Calque>> {
        Err(crate::Erreur::Indisponible("overlay"))
    }
    fn ecran_principal(&self) -> Option<ZoneEcran> {
        None
    }
    fn echelle(&self, _: &ZoneEcran) -> f32 {
        1.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffichageFactice {
    pub image: ImagePremultipliee,
    pub x: i32,
    pub y: i32,
    pub opacite: u8,
}

#[derive(Debug)]
struct EtatFactice {
    calques: Vec<Option<AffichageFactice>>,
    affichages: usize,
    ecran: Option<ZoneEcran>,
    echelle: f32,
}

#[derive(Debug, Clone)]
pub struct OverlayFactice {
    etat: Arc<Mutex<EtatFactice>>,
}

impl OverlayFactice {
    pub fn nouveau(ecran: Option<ZoneEcran>, echelle: f32) -> Self {
        Self {
            etat: Arc::new(Mutex::new(EtatFactice {
                calques: Vec::new(),
                affichages: 0,
                ecran,
                echelle,
            })),
        }
    }

    fn etat(&self) -> MutexGuard<'_, EtatFactice> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn calques(&self) -> Vec<Option<AffichageFactice>> {
        self.etat().calques.clone()
    }

    pub fn affichages(&self) -> usize {
        self.etat().affichages
    }

    pub fn definir_echelle(&self, echelle: f32) {
        self.etat().echelle = echelle;
    }
}

struct CalqueFactice {
    indice: usize,
    etat: Arc<Mutex<EtatFactice>>,
}

impl CalqueFactice {
    fn modifier(&self, affichage: Option<AffichageFactice>) {
        let mut etat = self.etat.lock().unwrap_or_else(|e| e.into_inner());
        if affichage.is_some() {
            etat.affichages += 1;
        }
        etat.calques[self.indice] = affichage;
    }
}

impl Calque for CalqueFactice {
    fn afficher(
        &mut self,
        image: &ImagePremultipliee,
        x: i32,
        y: i32,
        opacite: u8,
    ) -> Resultat<()> {
        self.modifier(Some(AffichageFactice {
            image: image.clone(),
            x,
            y,
            opacite,
        }));
        Ok(())
    }
    fn masquer(&mut self) -> Resultat<()> {
        self.modifier(None);
        Ok(())
    }
}

impl Overlay for OverlayFactice {
    fn disponible(&self) -> bool {
        true
    }
    fn calque(&self) -> Resultat<Box<dyn Calque>> {
        let indice = {
            let mut etat = self.etat();
            etat.calques.push(None);
            etat.calques.len() - 1
        };
        Ok(Box::new(CalqueFactice {
            indice,
            etat: self.etat.clone(),
        }))
    }
    fn ecran_principal(&self) -> Option<ZoneEcran> {
        self.etat().ecran
    }
    fn echelle(&self, _: &ZoneEcran) -> f32 {
        self.etat().echelle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Erreur;

    #[test]
    fn l_overlay_n_existe_que_sous_windows() {
        assert_eq!(OverlaySysteme.disponible(), cfg!(windows));
        if !cfg!(windows) {
            assert!(matches!(
                OverlaySysteme.calque().map(|_| ()),
                Err(Erreur::Indisponible(_))
            ));
            assert_eq!(OverlaySysteme.ecran_principal(), None);
        }
    }

    #[test]
    fn le_faux_overlay_retient_chaque_calque() {
        let ecran = ZoneEcran {
            x: 0,
            y: 0,
            largeur: 1920,
            hauteur: 1080,
        };
        let factice = OverlayFactice::nouveau(Some(ecran), 1.5);
        assert_eq!(factice.ecran_principal(), Some(ecran));
        assert_eq!(factice.echelle(&ecran), 1.5);
        let mut a = factice.calque().unwrap();
        let mut b = factice.calque().unwrap();
        let image = ImagePremultipliee {
            largeur: 1,
            hauteur: 1,
            rgba: vec![0, 0, 0, 255],
        };
        a.afficher(&image, 10, 20, 200).unwrap();
        b.afficher(&image, 0, 0, 255).unwrap();
        b.masquer().unwrap();
        let calques = factice.calques();
        assert_eq!(calques.len(), 2);
        assert_eq!(
            calques[0],
            Some(AffichageFactice {
                image,
                x: 10,
                y: 20,
                opacite: 200
            })
        );
        assert_eq!(calques[1], None);
        assert_eq!(factice.affichages(), 2);
    }
}
