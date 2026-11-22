use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::traits::{ActionRaccourci, EtatTouche, RaccourcisGlobaux};
use crate::{Erreur, Resultat};

#[cfg(windows)]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

#[cfg(windows)]
pub use self::windows::RaccourcisSysteme;
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) use self::x11::session_wayland;
#[cfg(all(unix, not(target_os = "macos")))]
pub use self::x11::RaccourcisSysteme;
#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub use crate::Indisponible as RaccourcisSysteme;

pub struct Touche {
    pub code: &'static str,
    pub libelle: &'static str,
    pub virtuelle: u16,
    pub evdev: u16,
}

macro_rules! touches {
    ($(($code:literal, $libelle:literal, $vk:literal, $evdev:literal)),* $(,)?) => {
        pub const TOUCHES: &[Touche] = &[$(Touche { code: $code, libelle: $libelle, virtuelle: $vk, evdev: $evdev }),*];
    };
}

touches![
    ("KeyA", "A", 0x41, 30),
    ("KeyB", "B", 0x42, 48),
    ("KeyC", "C", 0x43, 46),
    ("KeyD", "D", 0x44, 32),
    ("KeyE", "E", 0x45, 18),
    ("KeyF", "F", 0x46, 33),
    ("KeyG", "G", 0x47, 34),
    ("KeyH", "H", 0x48, 35),
    ("KeyI", "I", 0x49, 23),
    ("KeyJ", "J", 0x4A, 36),
    ("KeyK", "K", 0x4B, 37),
    ("KeyL", "L", 0x4C, 38),
    ("KeyM", "M", 0x4D, 50),
    ("KeyN", "N", 0x4E, 49),
    ("KeyO", "O", 0x4F, 24),
    ("KeyP", "P", 0x50, 25),
    ("KeyQ", "Q", 0x51, 16),
    ("KeyR", "R", 0x52, 19),
    ("KeyS", "S", 0x53, 31),
    ("KeyT", "T", 0x54, 20),
    ("KeyU", "U", 0x55, 22),
    ("KeyV", "V", 0x56, 47),
    ("KeyW", "W", 0x57, 17),
    ("KeyX", "X", 0x58, 45),
    ("KeyY", "Y", 0x59, 21),
    ("KeyZ", "Z", 0x5A, 44),
    ("Digit0", "0", 0x30, 11),
    ("Digit1", "1", 0x31, 2),
    ("Digit2", "2", 0x32, 3),
    ("Digit3", "3", 0x33, 4),
    ("Digit4", "4", 0x34, 5),
    ("Digit5", "5", 0x35, 6),
    ("Digit6", "6", 0x36, 7),
    ("Digit7", "7", 0x37, 8),
    ("Digit8", "8", 0x38, 9),
    ("Digit9", "9", 0x39, 10),
    ("F1", "F1", 0x70, 59),
    ("F2", "F2", 0x71, 60),
    ("F3", "F3", 0x72, 61),
    ("F4", "F4", 0x73, 62),
    ("F5", "F5", 0x74, 63),
    ("F6", "F6", 0x75, 64),
    ("F7", "F7", 0x76, 65),
    ("F8", "F8", 0x77, 66),
    ("F9", "F9", 0x78, 67),
    ("F10", "F10", 0x79, 68),
    ("F11", "F11", 0x7A, 87),
    ("F12", "F12", 0x7B, 88),
    ("F13", "F13", 0x7C, 183),
    ("F14", "F14", 0x7D, 184),
    ("F15", "F15", 0x7E, 185),
    ("F16", "F16", 0x7F, 186),
    ("F17", "F17", 0x80, 187),
    ("F18", "F18", 0x81, 188),
    ("F19", "F19", 0x82, 189),
    ("F20", "F20", 0x83, 190),
    ("F21", "F21", 0x84, 191),
    ("F22", "F22", 0x85, 192),
    ("F23", "F23", 0x86, 193),
    ("F24", "F24", 0x87, 194),
    ("Space", "Espace", 0x20, 57),
    ("Tab", "Tab", 0x09, 15),
    ("CapsLock", "Verr. maj", 0x14, 58),
    ("ShiftLeft", "Maj gauche", 0xA0, 42),
    ("ShiftRight", "Maj droite", 0xA1, 54),
    ("ControlLeft", "Ctrl gauche", 0xA2, 29),
    ("ControlRight", "Ctrl droite", 0xA3, 97),
    ("AltLeft", "Alt gauche", 0xA4, 56),
    ("AltRight", "Alt droite", 0xA5, 100),
    ("Insert", "Inser", 0x2D, 110),
    ("Delete", "Suppr", 0x2E, 111),
    ("Home", "Origine", 0x24, 102),
    ("End", "Fin", 0x23, 107),
    ("PageUp", "Page préc.", 0x21, 104),
    ("PageDown", "Page suiv.", 0x22, 109),
    ("ArrowUp", "Flèche haut", 0x26, 103),
    ("ArrowDown", "Flèche bas", 0x28, 108),
    ("ArrowLeft", "Flèche gauche", 0x25, 105),
    ("ArrowRight", "Flèche droite", 0x27, 106),
    ("Pause", "Pause", 0x13, 119),
    ("ScrollLock", "Arrêt défil.", 0x91, 70),
];

pub fn touche(code: &str) -> Option<&'static Touche> {
    TOUCHES.iter().find(|t| t.code == code)
}

pub fn libelle_touche(code: &str) -> String {
    touche(code).map_or_else(|| code.to_string(), |t| t.libelle.to_string())
}

pub fn code_de_virtuelle(virtuelle: u16) -> Option<&'static str> {
    TOUCHES
        .iter()
        .find(|t| t.virtuelle == virtuelle)
        .map(|t| t.code)
}

pub fn code_d_evdev(evdev: u16) -> Option<&'static str> {
    TOUCHES.iter().find(|t| t.evdev == evdev).map(|t| t.code)
}

type Action = Arc<dyn Fn(EtatTouche) + Send + Sync>;

#[derive(Default)]
pub(crate) struct Registre {
    actions: Mutex<HashMap<&'static str, (Action, bool)>>,
}

impl Registre {
    pub(crate) fn ajouter(&self, code: &str, action: ActionRaccourci) -> Resultat<&'static Touche> {
        let touche =
            touche(code).ok_or_else(|| Erreur::Refuse(format!("touche inconnue : {code}")))?;
        self.verrou()
            .insert(touche.code, (Arc::from(action), false));
        Ok(touche)
    }

    pub(crate) fn retirer(&self, code: &str) {
        self.verrou().retain(|c, _| *c != code);
    }

    pub(crate) fn signaler(&self, code: &str, appui: bool) {
        let action = {
            let mut actions = self.verrou();
            let Some((action, enfoncee)) = actions.get_mut(code) else {
                return;
            };
            if *enfoncee == appui {
                return;
            }
            *enfoncee = appui;
            action.clone()
        };
        action(if appui {
            EtatTouche::Appui
        } else {
            EtatTouche::Relache
        });
    }

    pub(crate) fn codes(&self) -> Vec<&'static str> {
        let mut codes: Vec<_> = self.verrou().keys().copied().collect();
        codes.sort_unstable();
        codes
    }

    fn verrou(&self) -> std::sync::MutexGuard<'_, HashMap<&'static str, (Action, bool)>> {
        self.actions.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct RaccourcisFactices {
    registre: Registre,
    disponible: bool,
}

impl RaccourcisFactices {
    pub fn nouveaux(disponible: bool) -> Self {
        Self {
            registre: Registre::default(),
            disponible,
        }
    }

    pub fn appuyer(&self, code: &str) {
        self.registre.signaler(code, true);
    }

    pub fn relacher(&self, code: &str) {
        self.registre.signaler(code, false);
    }

    pub fn enregistres(&self) -> Vec<&'static str> {
        self.registre.codes()
    }
}

impl RaccourcisGlobaux for RaccourcisFactices {
    fn disponible(&self) -> bool {
        self.disponible
    }

    fn enregistrer(&self, touche: &str, action: ActionRaccourci) -> Resultat<()> {
        if !self.disponible {
            return Err(Erreur::Indisponible("raccourcis globaux"));
        }
        self.registre.ajouter(touche, action).map(|_| ())
    }

    fn retirer(&self, touche: &str) -> Resultat<()> {
        if !self.disponible {
            return Err(Erreur::Indisponible("raccourcis globaux"));
        }
        self.registre.retirer(touche);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn chaque_touche_a_un_code_virtuel_et_evdev_uniques() {
        for (i, a) in TOUCHES.iter().enumerate() {
            for b in &TOUCHES[i + 1..] {
                assert_ne!(a.code, b.code);
                assert_ne!(a.virtuelle, b.virtuelle, "{} et {}", a.code, b.code);
                assert_ne!(a.evdev, b.evdev, "{} et {}", a.code, b.code);
            }
        }
        assert_eq!(code_de_virtuelle(0x56), Some("KeyV"));
        assert_eq!(code_d_evdev(47), Some("KeyV"));
        assert_eq!(libelle_touche("KeyV"), "V");
        assert_eq!(libelle_touche("Space"), "Espace");
        assert_eq!(libelle_touche("Inconnue"), "Inconnue");
    }

    #[test]
    fn appui_et_relachement_sans_repetition() {
        let r = RaccourcisFactices::nouveaux(true);
        let appuis = Arc::new(AtomicUsize::new(0));
        let relachements = Arc::new(AtomicUsize::new(0));
        let (a, b) = (appuis.clone(), relachements.clone());
        r.enregistrer(
            "KeyV",
            Box::new(move |etat| {
                let compteur = if etat == EtatTouche::Appui { &a } else { &b };
                compteur.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap();
        r.appuyer("KeyV");
        r.appuyer("KeyV");
        r.appuyer("KeyB");
        r.relacher("KeyV");
        r.relacher("KeyV");
        assert_eq!(appuis.load(Ordering::SeqCst), 1);
        assert_eq!(relachements.load(Ordering::SeqCst), 1);
        assert_eq!(r.enregistres(), ["KeyV"]);
        r.retirer("KeyV").unwrap();
        r.appuyer("KeyV");
        assert_eq!(appuis.load(Ordering::SeqCst), 1);
        assert!(r.enregistrer("Inconnue", Box::new(|_| {})).is_err());
    }

    #[test]
    fn indisponibles_ils_refusent_sans_paniquer() {
        let r = RaccourcisFactices::nouveaux(false);
        assert!(!r.disponible());
        assert!(matches!(
            r.enregistrer("KeyV", Box::new(|_| {})),
            Err(Erreur::Indisponible(_))
        ));
    }
}
