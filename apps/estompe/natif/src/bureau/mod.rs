pub mod fermeture;
pub mod instance;
pub mod notifications;
pub mod zone;

pub use fermeture::{EtapeFermeture, Fermeture};

use crate::entree::Evenement;
use crate::session::{retenir_geometrie, Memoire};
use crate::Principale;
use estompe_client::journal::{Journal, Niveau};
use estompe_client::pastille::{description_pastille, icone_avec_pastille, image_pastille};
use estompe_plateforme::instance::{MessageInstance, RecepteurInstance};
use estompe_plateforme::{DemarrageSession, EvenementZone, Image, Pastille, ZoneNotification};
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

pub const DELAI_ARRET: Duration = Duration::from_secs(3);
const SOURCE_JOURNAL: &str = "bureau";

pub type EntreeBureau = Arc<dyn Fn(Evenement) + Send + Sync>;

pub struct OptionsBureau {
    pub zone: Box<dyn ZoneNotification>,
    pub pastille: Box<dyn Pastille>,
    pub demarrage: Box<dyn DemarrageSession>,
    pub memoire: Memoire,
    pub entree: EntreeBureau,
    pub journal: Option<Arc<Journal>>,
    pub suivi_du_focus: Option<Box<dyn Fn(bool)>>,
}

struct Interne {
    fenetre: slint::Weak<Principale>,
    zone: Box<dyn ZoneNotification>,
    zone_installee: Cell<bool>,
    icone: Image,
    pastille: Box<dyn Pastille>,
    demarrage: Box<dyn DemarrageSession>,
    memoire: Memoire,
    entree: EntreeBureau,
    journal: Option<Arc<Journal>>,
    suivi_du_focus: Option<Box<dyn Fn(bool)>>,
    fermeture: Fermeture,
}

thread_local! {
    static BUREAU: RefCell<Weak<Interne>> = const { RefCell::new(Weak::new()) };
}

#[derive(Clone)]
pub struct Bureau {
    interne: Rc<Interne>,
}

pub fn sur_le_bureau(action: impl FnOnce(&Bureau) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(interne) = BUREAU.with(|b| b.borrow().upgrade()) {
            action(&Bureau { interne });
        }
    });
}

impl Bureau {
    pub fn installer(fenetre: &Principale, options: OptionsBureau) -> Self {
        let fermeture = Fermeture::nouvelle(fenetre);
        let interne = Rc::new(Interne {
            fenetre: fenetre.as_weak(),
            zone: options.zone,
            zone_installee: Cell::new(false),
            icone: zone::icone_de_l_app(),
            pastille: options.pastille,
            demarrage: options.demarrage,
            memoire: options.memoire,
            entree: options.entree,
            journal: options.journal,
            suivi_du_focus: options.suivi_du_focus,
            fermeture,
        });
        BUREAU.with(|b| *b.borrow_mut() = Rc::downgrade(&interne));
        let bureau = Self { interne };
        bureau.preparer_fermeture(fenetre);
        bureau.installer_zone();
        bureau.brancher_reglages(fenetre);
        bureau
    }

    fn noter(&self, niveau: Niveau, message: &str) {
        if let Some(journal) = &self.interne.journal {
            journal.ecrire(niveau, SOURCE_JOURNAL, message);
        }
    }

    fn preparer_fermeture(&self, fenetre: &Principale) {
        let memoire = self.interne.memoire.clone();
        let faible = fenetre.as_weak();
        self.interne
            .fermeture
            .avant_de_quitter(EtapeFermeture::Geometrie, move || {
                if let Some(f) = faible.upgrade() {
                    retenir_geometrie(&memoire, &f);
                }
            });
        let zone = Rc::downgrade(&self.interne);
        self.interne
            .fermeture
            .avant_de_quitter(EtapeFermeture::Zone, move || {
                if let Some(interne) = zone.upgrade() {
                    interne.zone.retirer();
                    interne.zone_installee.set(false);
                }
            });
        let faible = Rc::downgrade(&self.interne);
        fenetre.window().on_close_requested(move || {
            if let Some(interne) = faible.upgrade() {
                Bureau { interne }.demande_de_fermeture();
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    fn demande_de_fermeture(&self) {
        if self.zone_presente() {
            if let Some(f) = self.interne.fenetre.upgrade() {
                retenir_geometrie(&self.interne.memoire, &f);
            }
            if let Some(suivi) = &self.interne.suivi_du_focus {
                suivi(false);
            }
            return;
        }
        self.interne.fermeture.quitter();
    }

    fn installer_zone(&self) {
        if !self.interne.zone.disponible() {
            return;
        }
        let recepteur = Box::new(|evenement: EvenementZone| {
            sur_le_bureau(move |bureau| bureau.evenement_de_zone(evenement));
        });
        match self
            .interne
            .zone
            .installer(&self.interne.icone, zone::INFO_ZONE, recepteur)
        {
            Ok(()) => self.interne.zone_installee.set(true),
            Err(e) => self.noter(
                Niveau::Avertissement,
                &format!("icône de la zone de notification : {e}"),
            ),
        }
    }

    pub fn evenement_de_zone(&self, evenement: EvenementZone) {
        match evenement {
            EvenementZone::Clic | EvenementZone::Ouvrir => self.ramener_fenetre(),
            EvenementZone::Quitter => self.interne.fermeture.quitter(),
        }
    }

    fn brancher_reglages(&self, fenetre: &Principale) {
        let demarrage = self.interne.demarrage.disponible();
        fenetre.set_demarrage_disponible(demarrage);
        if demarrage {
            fenetre.set_demarrage_actif(self.interne.demarrage.actif().unwrap_or(false));
        }
        fenetre.set_notifications_bureau(self.interne.memoire.lire(|p| p.notifications));
        let faible = Rc::downgrade(&self.interne);
        fenetre.on_demarrage_change(move |actif| {
            let Some(interne) = faible.upgrade() else {
                return;
            };
            let bureau = Bureau { interne };
            if let Err(e) = bureau.interne.demarrage.definir(actif) {
                bureau.noter(Niveau::Erreur, &format!("démarrage avec la session : {e}"));
            }
            let reel = bureau.interne.demarrage.actif().unwrap_or(false);
            if let Some(f) = bureau.interne.fenetre.upgrade() {
                f.set_demarrage_actif(reel);
            }
        });
        let memoire = self.interne.memoire.clone();
        fenetre.on_notifications_change(move |actif| {
            memoire.modifier(|p| p.notifications = actif);
        });
    }

    pub fn fermeture(&self) -> Fermeture {
        self.interne.fermeture.clone()
    }

    pub fn fenetre(&self) -> Option<Principale> {
        self.interne.fenetre.upgrade()
    }

    pub fn definir_pastille(&self, mentions: usize) {
        if self.interne.zone_installee.get() {
            let icone = icone_avec_pastille(&self.interne.icone, mentions);
            if let Err(e) = self.interne.zone.definir_icone(&icone) {
                self.noter(Niveau::Avertissement, &format!("icône de la zone : {e}"));
            }
        }
        let pastille = &self.interne.pastille;
        if !pastille.disponible() {
            return;
        }
        if let Some(poignee) = self.poignee_de_fenetre() {
            let _ = pastille.attacher(poignee);
        }
        if let Err(e) = pastille.definir(
            image_pastille(mentions).as_ref(),
            &description_pastille(mentions),
        ) {
            self.noter(Niveau::Avertissement, &format!("pastille : {e}"));
        }
    }

    fn poignee_de_fenetre(&self) -> Option<estompe_plateforme::RawWindowHandle> {
        use slint::winit_030::winit::raw_window_handle::HasWindowHandle;
        use slint::winit_030::WinitWindowAccessor;
        let f = self.interne.fenetre.upgrade()?;
        f.window()
            .with_winit_window(|w| w.window_handle().ok().map(|p| p.as_raw()))
            .flatten()
    }

    pub fn zone_presente(&self) -> bool {
        self.interne.zone_installee.get() && self.interne.zone.presente()
    }

    pub fn ramener_fenetre(&self) {
        use slint::winit_030::WinitWindowAccessor;
        let Some(f) = self.interne.fenetre.upgrade() else {
            return;
        };
        let _ = f.show();
        f.window().set_minimized(false);
        f.window().with_winit_window(|w| {
            w.set_visible(true);
            w.set_minimized(false);
            w.focus_window();
        });
    }

    pub fn recevoir(&self, message: MessageInstance) {
        if instance::ramene_la_fenetre(&message) {
            self.ramener_fenetre();
        }
        for evenement in instance::evenements_du_message(&message) {
            (self.interne.entree)(evenement);
        }
    }

    pub fn recepteur_instance(&self) -> RecepteurInstance {
        Box::new(|message| sur_le_bureau(move |bureau| bureau.recevoir(message)))
    }
}
