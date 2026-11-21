use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, HCURSOR,
    ICONINFO,
};

use crate::capture::melanger_curseur;
use crate::traits::Image;

const TAILLE: i32 = 128;

pub(super) fn dessiner(image: &mut Image, origine: (i32, i32)) {
    unsafe {
        let mut info: CURSORINFO = zeroed();
        info.cbSize = size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut info) == 0 || info.flags & CURSOR_SHOWING == 0 {
            return;
        }
        let mut icone: ICONINFO = zeroed();
        if GetIconInfo(info.hCursor, &mut icone) == 0 {
            return;
        }
        if let (Some(noir), Some(blanc)) = (
            rendre(info.hCursor, 0x000000),
            rendre(info.hCursor, 0xFFFFFF),
        ) {
            let x = info.ptScreenPos.x - icone.xHotspot as i32 - origine.0;
            let y = info.ptScreenPos.y - icone.yHotspot as i32 - origine.1;
            melanger_curseur(image, x, y, TAILLE as u32, &noir, &blanc);
        }
        if !icone.hbmMask.is_null() {
            DeleteObject(icone.hbmMask);
        }
        if !icone.hbmColor.is_null() {
            DeleteObject(icone.hbmColor);
        }
    }
}

unsafe fn rendre(curseur: HCURSOR, fond: u32) -> Option<Vec<u8>> {
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return None;
    }
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = TAILLE;
    info.bmiHeader.biHeight = -TAILLE;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    let sortie = if bitmap.is_null() || bits.is_null() {
        None
    } else {
        let ancien = SelectObject(dc, bitmap);
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (TAILLE * TAILLE) as usize);
        pixels.fill(fond);
        DrawIconEx(dc, 0, 0, curseur, 0, 0, 0, null_mut(), DI_NORMAL);
        let copie =
            std::slice::from_raw_parts(bits as *const u8, (TAILLE * TAILLE * 4) as usize).to_vec();
        SelectObject(dc, ancien);
        Some(copie)
    };
    if !bitmap.is_null() {
        DeleteObject(bitmap);
    }
    DeleteDC(dc);
    sortie
}
