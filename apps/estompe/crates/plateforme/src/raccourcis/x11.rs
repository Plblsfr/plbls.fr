use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use super::{code_d_evdev, Registre};
use crate::traits::{ActionRaccourci, RaccourcisGlobaux};
use crate::{Erreur, Resultat};

const DECALAGE_EVDEV: u32 = 8;
const LETTRES: [&str; 26] = [
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL",
    "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX",
    "KeyY", "KeyZ",
];

static REGISTRE: OnceLock<Arc<Registre>> = OnceLock::new();
static ECOUTE: OnceLock<Result<(), String>> = OnceLock::new();

fn registre() -> &'static Arc<Registre> {
    REGISTRE.get_or_init(|| Arc::new(Registre::default()))
}

pub(crate) fn session_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"))
}

pub(crate) fn code_de_touche(keysym: Option<u32>, keycode: u32) -> Option<&'static str> {
    let lettre = keysym.and_then(|k| match k {
        0x61..=0x7a => Some(LETTRES[(k - 0x61) as usize]),
        0x41..=0x5a => Some(LETTRES[(k - 0x41) as usize]),
        _ => None,
    });
    lettre.or_else(|| {
        keycode
            .checked_sub(DECALAGE_EVDEV)
            .and_then(|e| u16::try_from(e).ok())
            .and_then(code_d_evdev)
    })
}

fn table_des_symboles(connexion: &RustConnection) -> HashMap<u32, u32> {
    let setup = connexion.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let Ok(reponse) = connexion
        .get_keyboard_mapping(min, max - min + 1)
        .map_err(|_| ())
        .and_then(|c| c.reply().map_err(|_| ()))
    else {
        return HashMap::new();
    };
    let largeur = usize::from(reponse.keysyms_per_keycode.max(1));
    reponse
        .keysyms
        .chunks(largeur)
        .enumerate()
        .filter_map(|(i, symboles)| {
            let premier = *symboles.first()?;
            (premier != 0).then_some((u32::from(min) + i as u32, premier))
        })
        .collect()
}

fn ouvrir() -> Result<(RustConnection, HashMap<u32, u32>), String> {
    if session_wayland() {
        return Err("session Wayland".to_string());
    }
    let (connexion, ecran) = x11rb::connect(None).map_err(|e| e.to_string())?;
    let racine = connexion.setup().roots[ecran].root;
    connexion
        .xinput_xi_query_version(2, 0)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| format!("XInput 2 absent : {e}"))?;
    connexion
        .xinput_xi_select_events(
            racine,
            &[xinput::EventMask {
                deviceid: xinput::Device::ALL_MASTER.into(),
                mask: vec![
                    xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE,
                ],
            }],
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let symboles = table_des_symboles(&connexion);
    Ok((connexion, symboles))
}

fn ecouter() -> Result<(), String> {
    let (connexion, symboles) = ouvrir()?;
    std::thread::Builder::new()
        .name("estompe-raccourcis".into())
        .spawn(move || {
            while let Ok(evenement) = connexion.wait_for_event() {
                let (keycode, appui) = match evenement {
                    Event::XinputRawKeyPress(e) => (e.detail, true),
                    Event::XinputRawKeyRelease(e) => (e.detail, false),
                    _ => continue,
                };
                if let Some(code) = code_de_touche(symboles.get(&keycode).copied(), keycode) {
                    registre().signaler(code, appui);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RaccourcisSysteme;

impl RaccourcisGlobaux for RaccourcisSysteme {
    fn disponible(&self) -> bool {
        match ECOUTE.get() {
            Some(resultat) => resultat.is_ok(),
            None => {
                !session_wayland() && std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty())
            }
        }
    }

    fn enregistrer(&self, touche: &str, action: ActionRaccourci) -> Resultat<()> {
        if session_wayland() {
            return Err(Erreur::Indisponible("raccourcis globaux sous Wayland"));
        }
        ECOUTE
            .get_or_init(ecouter)
            .clone()
            .map_err(|_| Erreur::Indisponible("raccourcis globaux"))?;
        registre().ajouter(touche, action).map(|_| ())
    }

    fn retirer(&self, touche: &str) -> Resultat<()> {
        registre().retirer(touche);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::EtatTouche;
    use std::sync::mpsc;
    use std::time::Duration;
    use x11rb::protocol::xtest::ConnectionExt as _;
    use x11rb::wrapper::ConnectionExt as _;

    #[test]
    fn traduit_la_lettre_tapee_sinon_la_touche_physique() {
        assert_eq!(code_de_touche(Some(0x61), 38), Some("KeyA"));
        assert_eq!(code_de_touche(Some(0x61), 24), Some("KeyA"));
        assert_eq!(code_de_touche(Some(0x56), 55), Some("KeyV"));
        assert_eq!(code_de_touche(Some(0xffe1), 50), Some("ShiftLeft"));
        assert_eq!(code_de_touche(None, 65), Some("Space"));
        assert_eq!(code_de_touche(None, 3), None);
    }

    #[test]
    fn x11_signale_appui_et_relachement_et_laisse_la_touche_a_la_fenetre_active() {
        use x11rb::protocol::xproto::{
            CreateWindowAux, EventMask, InputFocus, WindowClass, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
        };
        if session_wayland() || std::env::var_os("DISPLAY").is_none() {
            eprintln!("ignoré : aucun serveur X (DISPLAY absent ou session Wayland)");
            return;
        }
        let r = RaccourcisSysteme;
        assert!(r.disponible());
        let (envoi, reception) = mpsc::channel();
        r.enregistrer(
            "F20",
            Box::new(move |etat| {
                let _ = envoi.send(etat);
            }),
        )
        .unwrap();
        let (jeu, ecran) = x11rb::connect(None).unwrap();
        let racine = jeu.setup().roots[ecran].root;
        let fenetre = jeu.generate_id().unwrap();
        jeu.create_window(
            0,
            fenetre,
            racine,
            0,
            0,
            64,
            64,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE),
        )
        .unwrap();
        jeu.map_window(fenetre).unwrap();
        jeu.sync().unwrap();
        jeu.set_input_focus(InputFocus::PARENT, fenetre, 0u32)
            .unwrap();
        jeu.sync().unwrap();
        let keycode = 190 + DECALAGE_EVDEV as u8;
        for type_evenement in [KEY_PRESS_EVENT, KEY_RELEASE_EVENT] {
            jeu.xtest_fake_input(type_evenement, keycode, 0, racine, 0, 0, 0)
                .unwrap();
            jeu.flush().unwrap();
        }
        let attente = Duration::from_secs(5);
        assert_eq!(reception.recv_timeout(attente).unwrap(), EtatTouche::Appui);
        assert_eq!(
            reception.recv_timeout(attente).unwrap(),
            EtatTouche::Relache
        );
        let mut recues = Vec::new();
        while recues.len() < 2 {
            match jeu.wait_for_event().unwrap() {
                Event::KeyPress(e) if e.detail == keycode => recues.push(true),
                Event::KeyRelease(e) if e.detail == keycode => recues.push(false),
                _ => {}
            }
        }
        assert_eq!(
            recues,
            [true, false],
            "la fenêtre active reçoit toujours la touche"
        );
        r.retirer("F20").unwrap();
    }
}
