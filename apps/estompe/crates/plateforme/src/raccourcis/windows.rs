use std::sync::{mpsc, Arc, OnceLock};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::{code_de_virtuelle, Registre};
use crate::traits::{ActionRaccourci, RaccourcisGlobaux};
use crate::{Erreur, Resultat};

static REGISTRE: OnceLock<Arc<Registre>> = OnceLock::new();
static CROCHET: OnceLock<Result<(), String>> = OnceLock::new();

fn registre() -> &'static Arc<Registre> {
    REGISTRE.get_or_init(|| Arc::new(Registre::default()))
}

unsafe extern "system" fn crochet(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam != 0 {
        let infos = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let appui = match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let (Some(appui), Some(touche)) = (
            appui,
            u16::try_from(infos.vkCode).ok().and_then(code_de_virtuelle),
        ) {
            registre().signaler(touche, appui);
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn installer() -> Result<(), String> {
    let (signal, reponse) = mpsc::channel();
    std::thread::Builder::new()
        .name("estompe-raccourcis".into())
        .spawn(move || {
            let module = unsafe { GetModuleHandleW(std::ptr::null()) };
            let poignee = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(crochet), module, 0) };
            if poignee.is_null() {
                let _ = signal.send(Err(format!(
                    "crochet du clavier refusé ({})",
                    std::io::Error::last_os_error()
                )));
                return;
            }
            let _ = signal.send(Ok(()));
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {}
        })
        .map_err(|e| e.to_string())?;
    reponse
        .recv()
        .unwrap_or_else(|_| Err("fil des raccourcis arrêté".to_string()))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RaccourcisSysteme;

impl RaccourcisGlobaux for RaccourcisSysteme {
    fn disponible(&self) -> bool {
        !matches!(CROCHET.get(), Some(Err(_)))
    }

    fn enregistrer(&self, touche: &str, action: ActionRaccourci) -> Resultat<()> {
        CROCHET
            .get_or_init(installer)
            .clone()
            .map_err(Erreur::Echec)?;
        registre().ajouter(touche, action).map(|_| ())
    }

    fn retirer(&self, touche: &str) -> Resultat<()> {
        registre().retirer(touche);
        Ok(())
    }
}
