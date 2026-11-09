use super::moteur::Sortie;
use slint::platform::software_renderer::PremultipliedRgbaColor;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW, RegisterClassW,
    SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_REMOVE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

pub struct Calque {
    hwnd: HWND,
    bgra: Vec<u8>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Calque {
    pub fn nouveau() -> Option<Self> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let classe = wide("EstompeOverlayNatif");
            let titre = wide("Estompe — overlay");
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
            (!hwnd.is_null()).then(|| Calque {
                hwnd,
                bgra: Vec::new(),
            })
        }
    }
}

impl Sortie for Calque {
    fn afficher(
        &mut self,
        pixels: &[PremultipliedRgbaColor],
        largeur: u32,
        hauteur: u32,
        x: i32,
        y: i32,
    ) {
        self.bgra.clear();
        self.bgra.extend(
            pixels
                .iter()
                .flat_map(|c| [c.blue, c.green, c.red, c.alpha]),
        );
        unsafe {
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
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                UpdateLayeredWindow(
                    self.hwnd, ecran, &position, &taille, memoire, &origine, 0, &melange, ULW_ALPHA,
                );
                SelectObject(memoire, ancien);
                DeleteObject(bitmap);
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
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
            DeleteDC(memoire);
            ReleaseDC(std::ptr::null_mut(), ecran);
        }
    }

    fn masquer(&mut self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    fn pomper(&mut self) {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

impl Drop for Calque {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}
