//! Sélecteur de partage d'écran « façon Discord » (alpha, EST-18) : le webview ne sait pas désigner un écran ou une fenêtre précis
//! (getDisplayMedia affiche son propre sélecteur). Ici : liste des sources, miniatures et boucle de capture (`run`), que
//! `screen_native` publie. Windows uniquement ; ailleurs, les commandes renvoient une erreur.

use serde::Serialize;

#[derive(Serialize)]
pub struct Source {
    id: String,
    kind: &'static str, // "screen" ou "window"
    title: String,
    app: String,
    width: u32,
    height: u32,
}

/// Superpose un curseur sur `img` à (`x`, `y`). Le curseur (`size` × `size`, BGRA) est fourni dessiné sur fond noir puis sur fond
/// blanc : la transparence de chaque pixel est ce que le blanc a perdu en route, et le rendu sur noir est sa couleur déjà
/// multipliée par cette transparence (`dst = dst × (1 − a) + noir`). Un curseur « inversion » (barre de texte) devient opaque.
#[cfg(windows)]
fn blend_cursor(img: &mut image::RgbaImage, x: i32, y: i32, size: u32, on_black: &[u8], on_white: &[u8]) {
    for cy in 0..size {
        for cx in 0..size {
            let (px, py) = (x + cx as i32, y + cy as i32);
            if px < 0 || py < 0 || px as u32 >= img.width() || py as u32 >= img.height() {
                continue;
            }
            let i = ((cy * size + cx) * 4) as usize;
            let (b, w) = (&on_black[i..i + 3], &on_white[i..i + 3]);
            let lost: i32 = (0..3).map(|c| w[c] as i32 - b[c] as i32).sum::<i32>() / 3;
            let alpha = (255 - lost).clamp(0, 255) as u32;
            if alpha == 0 {
                continue;
            }
            let dst = img.get_pixel_mut(px as u32, py as u32);
            for (c, src) in [b[2], b[1], b[0]].into_iter().enumerate() { // BGRA -> RGBA
                dst[c] = ((dst[c] as u32 * (255 - alpha) / 255) + src as u32).min(255) as u8;
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use image::{codecs::jpeg::JpegEncoder, imageops, ExtendedColorType, RgbaImage};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, Instant};
    use xcap::{Monitor, Window};

    /// Le curseur n'est pas capturé par xcap (Windows Graphics Capture lancé avec le curseur désactivé) : on le dessine sur l'image.
    mod cursor {
        use super::super::blend_cursor;
        use image::RgbaImage;
        use std::{ffi::c_void, mem::zeroed, ptr::null_mut};
        use windows_sys::Win32::Graphics::Gdi::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        const SIZE: i32 = 128; // taille maximale d'un curseur (le standard fait 32, les curseurs « grande taille » 96)

        /// Dessine le curseur du système sur `img`, dont le coin haut gauche est à `origin` à l'écran.
        pub fn draw(img: &mut RgbaImage, origin: (i32, i32)) {
            unsafe {
                let mut ci: CURSORINFO = zeroed();
                ci.cbSize = size_of::<CURSORINFO>() as u32;
                if GetCursorInfo(&mut ci) == 0 || ci.flags & CURSOR_SHOWING == 0 {
                    return;
                }
                let mut ii: ICONINFO = zeroed();
                if GetIconInfo(ci.hCursor, &mut ii) == 0 {
                    return;
                }
                // Le même curseur est dessiné sur fond noir puis sur fond blanc : l'écart donne la transparence (blend_cursor).
                let on_black = render(ci.hCursor, 0x000000);
                let on_white = render(ci.hCursor, 0xFFFFFF);
                if let (Some(b), Some(w)) = (on_black, on_white) {
                    let x = ci.ptScreenPos.x - ii.xHotspot as i32 - origin.0;
                    let y = ci.ptScreenPos.y - ii.yHotspot as i32 - origin.1;
                    blend_cursor(img, x, y, SIZE as u32, &b, &w);
                }
                if !ii.hbmMask.is_null() {
                    DeleteObject(ii.hbmMask);
                }
                if !ii.hbmColor.is_null() {
                    DeleteObject(ii.hbmColor);
                }
            }
        }

        /// Pixels BGRA (SIZE × SIZE) du curseur dessiné sur un fond uni.
        unsafe fn render(cursor: HCURSOR, bg: u32) -> Option<Vec<u8>> {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return None;
            }
            let mut bi: BITMAPINFO = zeroed();
            bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = SIZE;
            bi.bmiHeader.biHeight = -SIZE; // origine en haut à gauche
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            let mut bits: *mut c_void = null_mut();
            let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            let out = if bmp.is_null() || bits.is_null() {
                None
            } else {
                let old = SelectObject(dc, bmp);
                let px = std::slice::from_raw_parts_mut(bits as *mut u32, (SIZE * SIZE) as usize);
                px.fill(bg);
                DrawIconEx(dc, 0, 0, cursor, 0, 0, 0, null_mut(), DI_NORMAL);
                let copy = std::slice::from_raw_parts(bits as *const u8, (SIZE * SIZE * 4) as usize).to_vec();
                SelectObject(dc, old);
                Some(copy)
            };
            if !bmp.is_null() {
                DeleteObject(bmp);
            }
            DeleteDC(dc);
            out
        }
    }

    pub fn sources() -> Result<Vec<Source>, String> {
        let mut out = vec![];
        for m in Monitor::all().map_err(|e| e.to_string())? {
            let (Ok(id), Ok(w), Ok(h)) = (m.id(), m.width(), m.height()) else { continue };
            let primary = m.is_primary().unwrap_or(false);
            let name = m.friendly_name().or_else(|_| m.name()).unwrap_or_default();
            out.push(Source { id: format!("m:{id}"), kind: "screen", title: if primary { format!("{name} (principal)") } else { name }, app: String::new(), width: w, height: h });
        }
        for w in Window::all().map_err(|e| e.to_string())? {
            let (Ok(id), Ok(title), Ok(width), Ok(height)) = (w.id(), w.title(), w.width(), w.height()) else { continue };
            if w.pid().ok() == Some(std::process::id()) || title.trim().is_empty() || width < 120 || height < 80 || w.is_minimized().unwrap_or(true) {
                continue;
            }
            out.push(Source { id: format!("w:{id}"), kind: "window", title, app: w.app_name().unwrap_or_default(), width, height });
        }
        Ok(out)
    }

    /// Processus propriétaire d'une fenêtre (`w:…`) ; `None` pour un écran.
    #[cfg_attr(not(feature = "livekit-native"), allow(dead_code))]
    pub fn window_pid(id: &str) -> Option<u32> {
        let n: u32 = id.strip_prefix("w:")?.parse().ok()?;
        Window::all().ok()?.into_iter().find(|w| w.id().ok() == Some(n))?.pid().ok()
    }

    /// Image de la source et position de son coin haut gauche à l'écran (pour placer le curseur).
    fn grab_at(id: &str) -> Result<(RgbaImage, (i32, i32)), String> {
        let (kind, n) = id.split_once(':').ok_or("source inconnue")?;
        let n: u32 = n.parse().map_err(|_| "source inconnue")?;
        match kind {
            "m" => {
                let m = Monitor::all().map_err(|e| e.to_string())?.into_iter().find(|m| m.id().ok() == Some(n)).ok_or("écran introuvable")?;
                Ok((m.capture_image().map_err(|e| e.to_string())?, (m.x().unwrap_or(0), m.y().unwrap_or(0))))
            }
            _ => {
                let w = Window::all().map_err(|e| e.to_string())?.into_iter().find(|w| w.id().ok() == Some(n)).ok_or("fenêtre introuvable")?;
                Ok((w.capture_image().map_err(|e| e.to_string())?, (w.x().unwrap_or(0), w.y().unwrap_or(0))))
            }
        }
    }

    fn grab(id: &str) -> Result<RgbaImage, String> {
        grab_at(id).map(|(img, _)| img)
    }

    /// JPEG de l'image, ramenée à `max_h` pixels de haut si elle est plus grande.
    fn jpeg(img: RgbaImage, max_h: u32, quality: u8) -> Result<Vec<u8>, String> {
        let img = if max_h > 0 && img.height() > max_h {
            let w = (img.width() as u64 * max_h as u64 / img.height() as u64).max(1) as u32;
            imageops::resize(&img, w, max_h, imageops::FilterType::Triangle)
        } else {
            img
        };
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        let mut buf = Vec::with_capacity(rgb.len() / 8);
        JpegEncoder::new_with_quality(&mut buf, quality)
            .encode(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        Ok(buf)
    }

    pub fn thumb(id: &str) -> Result<Vec<u8>, String> {
        jpeg(grab(id)?, 180, 70)
    }

    /// Fenêtre d'Estompe elle-même (la plus grande de ce processus), pour un signalement de bug.
    pub fn own() -> Result<Vec<u8>, String> {
        let pid = std::process::id();
        let w = Window::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|w| w.pid().ok() == Some(pid) && !w.is_minimized().unwrap_or(true))
            .max_by_key(|w| w.width().unwrap_or(0) as u64 * w.height().unwrap_or(0) as u64)
            .ok_or("fenêtre d'Estompe introuvable")?;
        jpeg(w.capture_image().map_err(|e| e.to_string())?, 1440, 80)
    }

    /// Boucle de capture d'une source : appelle `send` pour chaque image au rythme `fps` (relu à chaque image) jusqu'à `stop`
    /// ou jusqu'à ce que `send` renvoie false.
    #[cfg_attr(not(feature = "livekit-native"), allow(dead_code))] // seul le partage natif (feature livekit-native) l'appelle
    pub fn run(id: &str, stop_flag: &AtomicBool, fps: &AtomicU32, mut send: impl FnMut(RgbaImage) -> bool) {
        let monitor_id = id.strip_prefix("m:").and_then(|n| n.parse::<u32>().ok()); // HMONITOR n'est pas Send : l'écran est retrouvé ici
        let interval = || Duration::from_micros(1_000_000 / fps.load(Ordering::Relaxed).max(1) as u64);
        let mut last = Instant::now() - Duration::from_secs(1);
        if let Some(m) = monitor_id.and_then(|n| Monitor::all().ok()?.into_iter().find(|m| m.id().ok() == Some(n))) {
            let origin = (m.x().unwrap_or(0), m.y().unwrap_or(0));
            // Écran : flux continu de Windows Graphics Capture ; on ne garde que les images au rythme demandé.
            let Ok((rec, rx)) = m.video_recorder() else { return };
            if rec.start().is_err() {
                return;
            }
            while !stop_flag.load(Ordering::Relaxed) {
                let Ok(f) = rx.recv_timeout(Duration::from_millis(500)) else { continue };
                if last.elapsed() < interval() {
                    continue;
                }
                last = Instant::now();
                if let Some(mut img) = RgbaImage::from_raw(f.width, f.height, f.raw) {
                    cursor::draw(&mut img, origin);
                    if !send(img) {
                        break;
                    }
                }
            }
            let _ = rec.stop();
        } else {
            // Fenêtre : une capture par image (plus lente : quelques dizaines d'images/s au mieux).
            while !stop_flag.load(Ordering::Relaxed) {
                let t = Instant::now();
                match grab_at(id) {
                    Ok((mut img, origin)) => {
                        cursor::draw(&mut img, origin);
                        if !send(img) {
                            break;
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
                if let Some(rest) = interval().checked_sub(t.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    const NO: &str = "capture d'écran native : Windows uniquement";
    pub fn sources() -> Result<Vec<Source>, String> { Err(NO.into()) }
    pub fn thumb(_: &str) -> Result<Vec<u8>, String> { Err(NO.into()) }
    pub fn own() -> Result<Vec<u8>, String> { Err(NO.into()) }
}

#[cfg(windows)]
#[cfg_attr(not(feature = "livekit-native"), allow(unused_imports))]
pub use imp::{run, window_pid};

/// Écrans et fenêtres capturables (sans miniatures : voir `capture_thumb`).
#[tauri::command]
pub async fn capture_sources() -> Result<Vec<Source>, String> {
    tauri::async_runtime::spawn_blocking(imp::sources).await.map_err(|e| e.to_string())?
}

/// Miniature JPEG d'une source.
#[tauri::command]
pub async fn capture_thumb(id: String) -> Result<tauri::ipc::Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(move || imp::thumb(&id)).await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Capture JPEG de la fenêtre d'Estompe (signalement de bug).
#[tauri::command]
pub async fn capture_self() -> Result<tauri::ipc::Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(imp::own).await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn fond() -> RgbaImage {
        RgbaImage::from_pixel(1, 1, Rgba([100, 100, 100, 255]))
    }

    #[test]
    fn curseur_opaque_remplace_le_fond() {
        let mut img = fond();
        blend_cursor(&mut img, 0, 0, 1, &[0, 0, 255, 0], &[0, 0, 255, 0]); // rouge (BGRA) identique sur noir et sur blanc
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn curseur_transparent_laisse_le_fond() {
        let mut img = fond();
        blend_cursor(&mut img, 0, 0, 1, &[0, 0, 0, 0], &[255, 255, 255, 0]);
        assert_eq!(img.get_pixel(0, 0).0, [100, 100, 100, 255]);
    }

    #[test]
    fn curseur_semi_transparent_se_melange() {
        let mut img = fond();
        blend_cursor(&mut img, 0, 0, 1, &[128, 128, 128, 0], &[255, 255, 255, 0]); // blanc à 50 %
        assert_eq!(img.get_pixel(0, 0).0, [177, 177, 177, 255]);
    }

    #[test]
    fn curseur_hors_image_est_ignore() {
        let mut img = fond();
        blend_cursor(&mut img, -1, 0, 1, &[0, 0, 255, 0], &[0, 0, 255, 0]);
        blend_cursor(&mut img, 1, 0, 1, &[0, 0, 255, 0], &[0, 0, 255, 0]);
        assert_eq!(img.get_pixel(0, 0).0, [100, 100, 100, 255]);
    }
}
