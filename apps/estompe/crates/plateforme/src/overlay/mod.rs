use std::sync::{Arc, Mutex, MutexGuard};

use crate::traits::{
    Calque, EvenementSaisie, ImagePremultipliee, Overlay, RecepteurSaisie, ZoneEcran,
};
use crate::Resultat;

#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlaySysteme;

#[cfg(any(windows, test))]
pub(crate) fn evenement_de_caractere(
    unite: u16,
    haute: &mut Option<u16>,
) -> Option<EvenementSaisie> {
    match unite {
        0x08 => Some(EvenementSaisie::Effacer),
        0x0D => Some(EvenementSaisie::Valider),
        0x1B => Some(EvenementSaisie::Annuler),
        0xD800..=0xDBFF => {
            *haute = Some(unite);
            None
        }
        0xDC00..=0xDFFF => {
            let h = haute.take()?;
            let point = 0x10000 + ((u32::from(h) - 0xD800) << 10) + (u32::from(unite) - 0xDC00);
            char::from_u32(point).map(|c| EvenementSaisie::Texte(c.to_string()))
        }
        u if u < 0x20 || u == 0x7F => None,
        u => char::from_u32(u32::from(u)).map(|c| EvenementSaisie::Texte(c.to_string())),
    }
}

#[cfg(windows)]
impl Overlay for OverlaySysteme {
    fn disponible(&self) -> bool {
        true
    }
    fn saisie_disponible(&self) -> bool {
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

type SaisieFactice = Arc<Mutex<RecepteurSaisie>>;

struct EtatFactice {
    calques: Vec<Option<AffichageFactice>>,
    affichages: usize,
    ecran: Option<ZoneEcran>,
    echelle: f32,
    saisie_disponible: bool,
    saisie: Option<(usize, SaisieFactice)>,
    focus_pris: usize,
    focus_rendus: usize,
}

#[derive(Clone)]
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
                saisie_disponible: true,
                saisie: None,
                focus_pris: 0,
                focus_rendus: 0,
            })),
        }
    }

    pub fn sans_saisie(self) -> Self {
        self.etat().saisie_disponible = false;
        self
    }

    pub fn en_saisie(&self) -> Option<usize> {
        self.etat().saisie.as_ref().map(|(indice, _)| *indice)
    }

    pub fn focus(&self) -> (usize, usize) {
        let etat = self.etat();
        (etat.focus_pris, etat.focus_rendus)
    }

    pub fn taper(&self, evenement: EvenementSaisie) -> bool {
        let Some((_, recepteur)) = self.etat().saisie.clone() else {
            return false;
        };
        let recepteur = recepteur.lock().unwrap_or_else(|e| e.into_inner());
        recepteur(evenement);
        true
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
    fn saisir(&mut self, recepteur: RecepteurSaisie) -> Resultat<()> {
        let mut etat = self.etat.lock().unwrap_or_else(|e| e.into_inner());
        if !etat.saisie_disponible {
            return Err(crate::Erreur::Indisponible("saisie dans l'overlay"));
        }
        etat.saisie = Some((self.indice, Arc::new(Mutex::new(recepteur))));
        etat.focus_pris += 1;
        Ok(())
    }
    fn rendre_le_focus(&mut self) -> Resultat<()> {
        let mut etat = self.etat.lock().unwrap_or_else(|e| e.into_inner());
        if etat
            .saisie
            .as_ref()
            .is_some_and(|(indice, _)| *indice == self.indice)
        {
            etat.saisie = None;
            etat.focus_rendus += 1;
        }
        Ok(())
    }
}

impl Overlay for OverlayFactice {
    fn disponible(&self) -> bool {
        true
    }
    fn saisie_disponible(&self) -> bool {
        self.etat().saisie_disponible
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
        assert_eq!(OverlaySysteme.saisie_disponible(), cfg!(windows));
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

    #[test]
    fn les_caracteres_du_clavier_deviennent_des_evenements_de_saisie() {
        let mut haute = None;
        let mut lire = |u: u16| evenement_de_caractere(u, &mut haute);
        assert_eq!(lire(0x08), Some(EvenementSaisie::Effacer));
        assert_eq!(lire(0x0D), Some(EvenementSaisie::Valider));
        assert_eq!(lire(0x1B), Some(EvenementSaisie::Annuler));
        assert_eq!(lire(0x09), None);
        assert_eq!(lire(0x7F), None);
        assert_eq!(
            lire(u16::from(b'a')),
            Some(EvenementSaisie::Texte("a".into()))
        );
        assert_eq!(lire(0xE9), Some(EvenementSaisie::Texte("é".into())));
        assert_eq!(lire(0xD83D), None);
        assert_eq!(lire(0xDE00), Some(EvenementSaisie::Texte("😀".into())));
        assert_eq!(lire(0xDE00), None, "moitié basse orpheline ignorée");
    }

    #[test]
    fn le_faux_overlay_prend_le_focus_le_temps_de_la_saisie() {
        let factice = OverlayFactice::nouveau(None, 1.0);
        assert!(factice.saisie_disponible());
        let mut calque = factice.calque().unwrap();
        assert!(!factice.taper(EvenementSaisie::Valider));
        let recus = Arc::new(Mutex::new(Vec::new()));
        let copie = recus.clone();
        calque
            .saisir(Box::new(move |e| copie.lock().unwrap().push(e)))
            .unwrap();
        assert_eq!(factice.en_saisie(), Some(0));
        assert!(factice.taper(EvenementSaisie::Texte("é".into())));
        assert!(factice.taper(EvenementSaisie::Valider));
        calque.rendre_le_focus().unwrap();
        assert_eq!(factice.en_saisie(), None);
        assert_eq!(factice.focus(), (1, 1));
        assert_eq!(
            *recus.lock().unwrap(),
            vec![EvenementSaisie::Texte("é".into()), EvenementSaisie::Valider]
        );
        let sans = OverlayFactice::nouveau(None, 1.0).sans_saisie();
        assert!(!sans.saisie_disponible());
        assert!(matches!(
            sans.calque().unwrap().saisir(Box::new(|_| {})),
            Err(Erreur::Indisponible(_))
        ));
    }
}
