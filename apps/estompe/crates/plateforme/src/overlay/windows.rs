use std::cell::Cell;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, MonitorFromPoint,
    ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetForegroundWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow,
    PeekMessageW, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, UpdateLayeredWindow, GWLP_USERDATA, GWL_EXSTYLE, HWND_TOPMOST, MSG,
    PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_CHAR, WM_KILLFOCUS, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use super::evenement_de_caractere;
use crate::traits::{Calque, EvenementSaisie, ImagePremultipliee, RecepteurSaisie, ZoneEcran};
use crate::{Erreur, Resultat};

const INTOUCHABLE: u32 = WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;

struct Saisie {
    recepteur: RecepteurSaisie,
    haute: Cell<Option<u16>>,
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let saisie = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Saisie;
    if let Some(saisie) = unsafe { saisie.as_ref() } {
        match message {
            WM_CHAR => {
                let mut haute = saisie.haute.get();
                let evenement = evenement_de_caractere(wparam as u16, &mut haute);
                saisie.haute.set(haute);
                if let Some(evenement) = evenement {
                    (saisie.recepteur)(evenement);
                }
                return 0;
            }
            WM_KILLFOCUS => (saisie.recepteur)(EvenementSaisie::FocusPerdu),
            _ => {}
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe fn activer(hwnd: HWND) -> bool {
    unsafe {
        let devant = GetForegroundWindow();
        let fil_devant = if devant.is_null() {
            0
        } else {
            GetWindowThreadProcessId(devant, std::ptr::null_mut())
        };
        let moi = GetCurrentThreadId();
        let attache =
            fil_devant != 0 && fil_devant != moi && AttachThreadInput(moi, fil_devant, 1) != 0;
        BringWindowToTop(hwnd);
        let reussi = SetForegroundWindow(hwnd) != 0;
        if attache {
            AttachThreadInput(moi, fil_devant, 0);
        }
        reussi
    }
}

unsafe fn styles(hwnd: HWND, intouchable: bool) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let nouveau = if intouchable {
            style | INTOUCHABLE
        } else {
            style & !INTOUCHABLE
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, nouveau as isize);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

const DPI_DE_BASE: f32 = 96.0;

fn large(texte: &str) -> Vec<u16> {
    texte.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct CalqueWindows {
    hwnd: HWND,
    bgra: Vec<u8>,
    visible: bool,
    precedente: Option<HWND>,
}

impl CalqueWindows {
    pub fn nouveau() -> Option<Self> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let classe = large("EstompeNatifOverlay");
            let titre = large("Estompe — overlay");
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(procedure);
            wc.hInstance = instance;
            wc.lpszClassName = classe.as_ptr();
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                classe.as_ptr(),
                titre.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            (!hwnd.is_null()).then(|| CalqueWindows {
                hwnd,
                bgra: Vec::new(),
                visible: false,
                precedente: None,
            })
        }
    }

    fn liberer_saisie(&mut self) -> bool {
        let ancienne = unsafe { SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0) } as *mut Saisie;
        if ancienne.is_null() {
            return false;
        }
        drop(unsafe { Box::from_raw(ancienne) });
        true
    }
}

impl Calque for CalqueWindows {
    fn afficher(
        &mut self,
        image: &ImagePremultipliee,
        x: i32,
        y: i32,
        opacite: u8,
    ) -> Resultat<()> {
        let (largeur, hauteur) = (image.largeur.max(1), image.hauteur.max(1));
        if image.rgba.len() < (largeur * hauteur * 4) as usize {
            return Err(Erreur::Echec("image d'overlay incomplète".into()));
        }
        self.bgra.clear();
        self.bgra.extend(
            image
                .rgba
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0], p[3]]),
        );
        let reussi = unsafe {
            let ecran = GetDC(std::ptr::null_mut());
            let memoire = CreateCompatibleDC(ecran);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: largeur as i32,
                biHeight: -(hauteur as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                memoire,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            let mut reussi = false;
            if !bitmap.is_null() && !bits.is_null() {
                std::ptr::copy_nonoverlapping(
                    self.bgra.as_ptr(),
                    bits.cast::<u8>(),
                    self.bgra.len(),
                );
                let ancien = SelectObject(memoire, bitmap);
                let position = POINT { x, y };
                let taille = SIZE {
                    cx: largeur as i32,
                    cy: hauteur as i32,
                };
                let origine = POINT { x: 0, y: 0 };
                let melange = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: opacite,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                reussi = UpdateLayeredWindow(
                    self.hwnd, ecran, &position, &taille, memoire, &origine, 0, &melange, ULW_ALPHA,
                ) != 0;
                SelectObject(memoire, ancien);
                DeleteObject(bitmap);
                if reussi && !self.visible {
                    ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                    self.visible = true;
                }
                if reussi {
                    SetWindowPos(
                        self.hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
            DeleteDC(memoire);
            ReleaseDC(std::ptr::null_mut(), ecran);
            reussi
        };
        if reussi {
            Ok(())
        } else {
            Err(Erreur::Echec("affichage de l'overlay refusé".into()))
        }
    }

    fn masquer(&mut self) -> Resultat<()> {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
        Ok(())
    }

    fn saisir(&mut self, recepteur: RecepteurSaisie) -> Resultat<()> {
        self.liberer_saisie();
        let boite = Box::into_raw(Box::new(Saisie {
            recepteur,
            haute: Cell::new(None),
        }));
        let active = unsafe {
            let devant = GetForegroundWindow();
            if !devant.is_null() && devant != self.hwnd {
                self.precedente = Some(devant);
            }
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, boite as isize);
            styles(self.hwnd, false);
            activer(self.hwnd)
        };
        if active {
            Ok(())
        } else {
            let _ = self.rendre_le_focus();
            Err(Erreur::Echec(
                "la fenêtre de réponse n'a pas pu prendre le focus".into(),
            ))
        }
    }

    fn rendre_le_focus(&mut self) -> Resultat<()> {
        self.liberer_saisie();
        unsafe {
            let au_premier_plan = GetForegroundWindow() == self.hwnd;
            styles(self.hwnd, true);
            if let Some(precedente) = self.precedente.take() {
                if au_premier_plan && IsWindow(precedente) != 0 {
                    activer(precedente);
                }
            }
        }
        Ok(())
    }
}

impl Drop for CalqueWindows {
    fn drop(&mut self) {
        self.liberer_saisie();
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

pub fn pomper() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn ecran_principal() -> Option<ZoneEcran> {
    let (largeur, hauteur) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    (largeur > 0 && hauteur > 0).then_some(ZoneEcran {
        x: 0,
        y: 0,
        largeur: largeur as u32,
        hauteur: hauteur as u32,
    })
}

pub fn echelle(ecran: &ZoneEcran) -> f32 {
    let centre = POINT {
        x: ecran.x + (ecran.largeur / 2) as i32,
        y: ecran.y + (ecran.hauteur / 2) as i32,
    };
    let (mut x, mut y) = (0u32, 0u32);
    let lu = unsafe {
        let moniteur = MonitorFromPoint(centre, MONITOR_DEFAULTTONEAREST);
        GetDpiForMonitor(moniteur, MDT_EFFECTIVE_DPI, &mut x, &mut y)
    };
    if lu == 0 && x > 0 {
        x as f32 / DPI_DE_BASE
    } else {
        1.0
    }
}
