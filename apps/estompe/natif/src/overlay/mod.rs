pub mod reglages;
pub mod voix;

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::overlay::Placement;
use estompe_plateforme::{Calque, ImagePremultipliee, Overlay};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize};

pub const POMPE: Duration = Duration::from_millis(100);
pub const NOM_DU_FIL: &str = "estompe-overlay";

type Creation = Box<dyn FnOnce() -> Result<Box<dyn Any>, String> + Send>;
type Application = Box<dyn FnOnce(&dyn Any) + Send>;
type Preparation<E> = Arc<dyn Fn(E) -> Application + Send + Sync>;

enum Commande {
    Creer(u64, Creation),
    Afficher {
        id: u64,
        appliquer: Application,
        placement: Placement,
        opacite: u8,
    },
    Masquer(u64),
}

struct PlateformeOverlay {
    derniere: Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>,
}

impl Platform for PlateformeOverlay {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let fenetre = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.derniere.replace(Some(fenetre.clone()));
        Ok(fenetre)
    }
}

struct SceneVivante {
    composant: Box<dyn Any>,
    fenetre: Rc<MinimalSoftwareWindow>,
    calque: Box<dyn Calque>,
}

struct Rendu {
    tampon: Vec<PremultipliedRgbaColor>,
    image: ImagePremultipliee,
}

impl Rendu {
    fn rendre(
        &mut self,
        scene: &mut SceneVivante,
        appliquer: Application,
        placement: Placement,
        opacite: u8,
    ) -> bool {
        let (largeur, hauteur) = (placement.largeur.max(1), placement.hauteur.max(1));
        scene
            .fenetre
            .dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: placement.echelle,
            });
        scene.fenetre.set_size(PhysicalSize::new(largeur, hauteur));
        appliquer(scene.composant.as_ref());
        slint::platform::update_timers_and_animations();
        self.tampon.clear();
        self.tampon.resize(
            (largeur * hauteur) as usize,
            PremultipliedRgbaColor::default(),
        );
        scene.fenetre.request_redraw();
        let tampon = &mut self.tampon;
        scene.fenetre.draw_if_needed(|r| {
            r.render(tampon, largeur as usize);
        });
        self.image.largeur = largeur;
        self.image.hauteur = hauteur;
        self.image.rgba.clear();
        self.image.rgba.extend(
            self.tampon
                .iter()
                .flat_map(|c| [c.red, c.green, c.blue, c.alpha]),
        );
        scene
            .calque
            .afficher(&self.image, placement.x, placement.y, opacite)
            .is_ok()
    }
}

fn boucle(recepteur: Receiver<Commande>, overlay: Arc<dyn Overlay>, rendus: Arc<AtomicU64>) {
    let derniere = Rc::new(RefCell::new(None));
    if slint::platform::set_platform(Box::new(PlateformeOverlay {
        derniere: derniere.clone(),
    }))
    .is_err()
    {
        return;
    }
    let mut scenes: HashMap<u64, SceneVivante> = HashMap::new();
    let mut rendu = Rendu {
        tampon: Vec::new(),
        image: ImagePremultipliee::default(),
    };
    loop {
        match recepteur.recv_timeout(POMPE) {
            Ok(Commande::Creer(id, creer)) => {
                let Ok(composant) = creer() else { continue };
                let Some(fenetre) = derniere.borrow_mut().take() else {
                    continue;
                };
                let Ok(calque) = overlay.calque() else {
                    continue;
                };
                scenes.insert(
                    id,
                    SceneVivante {
                        composant,
                        fenetre,
                        calque,
                    },
                );
            }
            Ok(Commande::Afficher {
                id,
                appliquer,
                placement,
                opacite,
            }) => {
                if let Some(scene) = scenes.get_mut(&id) {
                    rendu.rendre(scene, appliquer, placement, opacite);
                    rendus.fetch_add(1, Ordering::SeqCst);
                }
            }
            Ok(Commande::Masquer(id)) => {
                if let Some(scene) = scenes.get_mut(&id) {
                    let _ = scene.calque.masquer();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        overlay.pomper();
    }
}

struct Interne {
    overlay: Arc<dyn Overlay>,
    emetteur: Mutex<Option<Sender<Commande>>>,
    rendus: Arc<AtomicU64>,
    demandes: AtomicU64,
    suivant: AtomicU64,
}

#[derive(Clone)]
pub struct MoteurOverlay {
    interne: Arc<Interne>,
}

fn verrou<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl MoteurOverlay {
    pub fn nouveau(overlay: Arc<dyn Overlay>) -> Self {
        Self {
            interne: Arc::new(Interne {
                overlay,
                emetteur: Mutex::new(None),
                rendus: Arc::new(AtomicU64::new(0)),
                demandes: AtomicU64::new(0),
                suivant: AtomicU64::new(0),
            }),
        }
    }

    pub fn disponible(&self) -> bool {
        self.interne.overlay.disponible()
    }

    pub fn overlay(&self) -> &Arc<dyn Overlay> {
        &self.interne.overlay
    }

    pub fn rendus(&self) -> u64 {
        self.interne.rendus.load(Ordering::SeqCst)
    }

    pub fn demandes(&self) -> u64 {
        self.interne.demandes.load(Ordering::SeqCst)
    }

    pub fn demarre(&self) -> bool {
        verrou(&self.interne.emetteur).is_some()
    }

    pub fn scene<C, E>(
        &self,
        creer: fn() -> Result<C, slint::PlatformError>,
        appliquer: fn(&C, E),
    ) -> SceneOverlay<E>
    where
        C: ComponentHandle + 'static,
        E: Send + 'static,
    {
        let preparer: Preparation<E> = Arc::new(move |etat: E| -> Application {
            Box::new(move |composant: &dyn Any| {
                if let Some(c) = composant.downcast_ref::<C>() {
                    appliquer(c, etat);
                }
            })
        });
        let creation: Creation = Box::new(move || {
            let composant = creer().map_err(|e| e.to_string())?;
            composant.show().map_err(|e| e.to_string())?;
            Ok(Box::new(composant) as Box<dyn Any>)
        });
        SceneOverlay {
            moteur: self.clone(),
            id: self.interne.suivant.fetch_add(1, Ordering::SeqCst),
            preparer,
            etat: Mutex::new(EtatScene {
                creation: Some(creation),
                dernier: None,
            }),
            _type: PhantomData,
        }
    }

    fn envoyer(&self, commandes: Vec<Commande>) -> bool {
        if !self.disponible() {
            return false;
        }
        let mut emetteur = verrou(&self.interne.emetteur);
        if emetteur.is_none() {
            let (tx, rx) = mpsc::channel();
            let (overlay, rendus) = (self.interne.overlay.clone(), self.interne.rendus.clone());
            if std::thread::Builder::new()
                .name(NOM_DU_FIL.into())
                .spawn(move || boucle(rx, overlay, rendus))
                .is_err()
            {
                return false;
            }
            *emetteur = Some(tx);
        }
        let Some(tx) = emetteur.as_ref() else {
            return false;
        };
        commandes.into_iter().all(|c| tx.send(c).is_ok())
    }
}

struct EtatScene<E> {
    creation: Option<Creation>,
    dernier: Option<(E, Placement, u8)>,
}

pub struct SceneOverlay<E> {
    moteur: MoteurOverlay,
    id: u64,
    preparer: Preparation<E>,
    etat: Mutex<EtatScene<E>>,
    _type: PhantomData<fn(E)>,
}

impl<E: Clone + PartialEq + Send + 'static> SceneOverlay<E> {
    pub fn afficher(&self, etat: E, placement: Placement, opacite: u8) -> bool {
        let mut interne = verrou(&self.etat);
        let affichage = (etat, placement, opacite);
        if interne.dernier.as_ref() == Some(&affichage) {
            return false;
        }
        let mut commandes = Vec::new();
        if let Some(creation) = interne.creation.take() {
            commandes.push(Commande::Creer(self.id, creation));
        }
        commandes.push(Commande::Afficher {
            id: self.id,
            appliquer: (self.preparer)(affichage.0.clone()),
            placement,
            opacite,
        });
        let envoye = self.moteur.envoyer(commandes);
        if envoye {
            self.moteur.interne.demandes.fetch_add(1, Ordering::SeqCst);
            interne.dernier = Some(affichage);
        }
        envoye
    }

    pub fn masquer(&self) -> bool {
        let mut interne = verrou(&self.etat);
        if interne.dernier.take().is_none() {
            return false;
        }
        self.moteur.envoyer(vec![Commande::Masquer(self.id)])
    }

    pub fn affichee(&self) -> bool {
        verrou(&self.etat).dernier.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::voix::{appliquer, EtatVoix, LigneVoix};
    use super::*;
    use estompe_client::overlay::TailleOverlay;
    use estompe_plateforme::{AffichageFactice, Indisponible, OverlayFactice};
    use std::time::Instant;

    use crate::OverlayVoix;

    fn attendre(condition: impl Fn() -> bool, quoi: &str) {
        let limite = Instant::now() + Duration::from_secs(20);
        while !condition() {
            assert!(Instant::now() < limite, "condition non atteinte : {quoi}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn ligne(pseudo: &str, parle: bool, muet: bool) -> LigneVoix {
        LigneVoix {
            id: pseudo.into(),
            pseudo: pseudo.into(),
            initiales: pseudo.chars().take(2).collect::<String>().to_uppercase(),
            avatar: None,
            parle,
            muet,
            sourd: false,
        }
    }

    fn placement(largeur: u32, hauteur: u32) -> Placement {
        Placement {
            x: 30,
            y: 40,
            largeur,
            hauteur,
            echelle: 1.0,
        }
    }

    fn affichage(factice: &OverlayFactice) -> Option<AffichageFactice> {
        factice.calques().first().cloned().flatten()
    }

    #[test]
    fn rend_hors_ecran_une_image_transparente_avec_le_halo_du_locuteur() {
        let factice = OverlayFactice::nouveau(None, 1.0);
        let moteur = MoteurOverlay::nouveau(Arc::new(factice.clone()));
        assert!(!moteur.demarre());
        let scene = moteur.scene(OverlayVoix::new, appliquer);
        let etat = EtatVoix {
            taille: TailleOverlay::Moyenne,
            lignes: vec![ligne("alice", true, false), ligne("bob", false, true)],
        };
        assert!(scene.afficher(etat.clone(), placement(260, 96), 200));
        assert!(moteur.demarre());
        attendre(|| affichage(&factice).is_some(), "premier rendu");
        let a = affichage(&factice).unwrap();
        assert_eq!((a.x, a.y, a.opacite), (30, 40, 200));
        assert_eq!((a.image.largeur, a.image.hauteur), (260, 96));
        let px = |x: u32, y: u32| {
            let i = ((y * 260 + x) * 4) as usize;
            [
                a.image.rgba[i],
                a.image.rgba[i + 1],
                a.image.rgba[i + 2],
                a.image.rgba[i + 3],
            ]
        };
        assert_eq!(px(255, 90)[3], 0, "fond transparent");
        let anneau = px(20, 1);
        assert!(
            anneau[1] > 150 && anneau[0] < 120 && anneau[3] > 200,
            "{anneau:?}"
        );
        let sans_halo = px(20, 48 + 1);
        assert!(sans_halo[1] < 120, "{sans_halo:?}");
        let pseudo = (60..200).filter(|x| px(*x, 20)[3] > 100).count();
        assert!(pseudo > 20, "{pseudo}");
        assert_eq!(moteur.rendus(), 1);

        assert!(!scene.afficher(etat.clone(), placement(260, 96), 200));
        assert_eq!(moteur.demandes(), 1);
        let mut parle = etat.clone();
        parle.lignes[0].parle = false;
        assert!(scene.afficher(parle, placement(260, 96), 200));
        attendre(|| moteur.rendus() == 2, "rendu après un changement");
        assert!(scene.masquer());
        attendre(|| factice.calques() == vec![None], "calque masqué");
        assert!(!scene.masquer());
        assert!(scene.afficher(etat, placement(260, 96), 200));
        attendre(|| moteur.rendus() == 3, "réaffiché");
        assert_eq!(factice.calques().len(), 1, "un seul calque par scène");
    }

    #[test]
    fn sans_overlay_le_moteur_ne_demarre_pas() {
        let moteur = MoteurOverlay::nouveau(Arc::new(Indisponible));
        let scene = moteur.scene(OverlayVoix::new, appliquer);
        assert!(!scene.afficher(
            EtatVoix {
                taille: TailleOverlay::Petite,
                lignes: vec![ligne("alice", false, false)],
            },
            placement(10, 10),
            255
        ));
        assert!(!moteur.demarre());
        assert!(!scene.affichee());
    }
}
