use crate::Principale;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EtapeFermeture {
    Geometrie,
    Coquille,
    Vocal,
    Zone,
}

type Etape = (EtapeFermeture, Box<dyn FnOnce()>);

struct EtatFermeture {
    fenetre: slint::Weak<Principale>,
    etapes: Vec<Etape>,
    quittee: bool,
}

#[derive(Clone)]
pub struct Fermeture {
    etat: Rc<RefCell<EtatFermeture>>,
}

impl Fermeture {
    pub fn nouvelle(fenetre: &Principale) -> Self {
        Self {
            etat: Rc::new(RefCell::new(EtatFermeture {
                fenetre: fenetre.as_weak(),
                etapes: Vec::new(),
                quittee: false,
            })),
        }
    }

    pub fn avant_de_quitter(&self, etape: EtapeFermeture, action: impl FnOnce() + 'static) {
        self.etat
            .borrow_mut()
            .etapes
            .push((etape, Box::new(action)));
    }

    pub fn quittee(&self) -> bool {
        self.etat.borrow().quittee
    }

    pub fn quitter(&self) {
        let (mut etapes, fenetre) = {
            let mut etat = self.etat.borrow_mut();
            if etat.quittee {
                return;
            }
            etat.quittee = true;
            (std::mem::take(&mut etat.etapes), etat.fenetre.clone())
        };
        etapes.sort_by_key(|(etape, _)| *etape);
        for (_, action) in etapes {
            action();
        }
        if let Some(fenetre) = fenetre.upgrade() {
            let _ = fenetre.hide();
        }
        let _ = slint::quit_event_loop();
    }
}
