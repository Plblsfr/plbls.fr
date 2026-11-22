use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, MonitorFromPoint,
    ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
    PeekMessageW, RegisterClassW, SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindow,
    HWND_TOPMOST, MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::traits::{Calque, ImagePremultipliee, ZoneEcran};
use crate::{Erreur, Resultat};

const DPI_DE_BASE: f32 = 96.0;

fn large(texte: &str) -> Vec<u16> {
    texte.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct CalqueWindows {
    hwnd: HWND,
    bgra: Vec<u8>,
    visible: bool,
}

impl CalqueWindows {
    pub fn nouveau() -> Option<Self> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let classe = large("EstompeNatifOverlay");
            let titre = large("Estompe — overlay");
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
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
            })
        }
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
}

impl Drop for CalqueWindows {
    fn drop(&mut self) {
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
