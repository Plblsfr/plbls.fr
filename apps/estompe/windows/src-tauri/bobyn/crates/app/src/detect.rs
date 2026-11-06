//! Fenêtres et écrans : détection d'un jeu au premier plan, listes pour la fenêtre de réglages.
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Processus du shell, de Bobyn et lecteurs plein écran courants : jamais pris pour un jeu sauf s'ils sont dans la liste de l'utilisateur.
const NOT_GAMES: &[&str] = &[
    "explorer.exe", "estompe.exe", "applicationframehost.exe", "searchhost.exe", "shellexperiencehost.exe",
    "startmenuexperiencehost.exe", "textinputhost.exe", "lockapp.exe", "taskmgr.exe", "chrome.exe", "msedge.exe", "firefox.exe",
    "brave.exe", "opera.exe", "vivaldi.exe", "mpv.exe", "vlc.exe", "mpc-hc64.exe", "wmplayer.exe", "microsoft.media.player.exe", "powerpnt.exe", "discord.exe",
];

pub fn hwnd(h: isize) -> HWND {
    HWND(h as *mut _)
}

pub fn pid_of(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

pub fn exe_of_window(h: HWND) -> Option<String> {
    bobyn_audio::exe_of(pid_of(h))
}

pub struct Game {
    pub hwnd: isize,
    pub pid: u32,
    pub exe: String,
    /// Nom à afficher : jeu reconnu, sinon exécutable sans « .exe » (le titre de la fenêtre pour Java).
    pub name: String,
}

/// Chemin complet de l'exécutable d'un processus.
pub fn exe_path(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn title_of(h: HWND) -> String {
    let mut t = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut t) }.max(0) as usize;
    String::from_utf16_lossy(&t[..n])
}

/// Jeu que porte un exécutable : celui de la liste de l'utilisateur, sinon un jeu connu (`games.rs`). Les programmes du shell, navigateurs et lecteurs n'en sont jamais.
fn known_game(exe: &str, path: &str, games: &[String]) -> Option<String> {
    if games.iter().any(|g| g.eq_ignore_ascii_case(exe)) {
        return Some(exe.trim_end_matches(".exe").to_string());
    }
    if NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(exe)) {
        return None;
    }
    crate::games::name_of(exe, path)
}

fn monitor_rect(h: HWND) -> Option<RECT> {
    unsafe {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool().then_some(mi.rcMonitor)
    }
}

/// Fenêtre visible de premier niveau, ni réduite, ni outil.
fn usable(h: HWND) -> bool {
    unsafe { !h.0.is_null() && IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() && GetAncestor(h, GA_ROOT) == h }
}

/// Jeu = exécutable de la liste de l'utilisateur ou jeu connu, ou fenêtre plein écran sans bordure/titre qui couvre tout son moniteur
/// (couvre le plein écran exclusif, vu comme une fenêtre popup de la taille de l'écran).
pub fn classify(h: HWND, games: &[String]) -> Option<Game> {
    if !usable(h) {
        return None;
    }
    let path = exe_path(pid_of(h))?;
    let exe = path.rsplit('\\').next().unwrap_or(&path).to_string();
    let known = known_game(&exe, &path, games);
    if known.is_none() {
        if NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(&exe)) {
            return None;
        }
        let (mut r, style) = (RECT::default(), unsafe { GetWindowLongW(h, GWL_STYLE) } as u32);
        let m = monitor_rect(h)?;
        unsafe { GetWindowRect(h, &mut r).ok()? };
        let covers = r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom;
        if !covers || style & WS_CAPTION.0 == WS_CAPTION.0 {
            return None;
        }
    }
    let stem = exe.trim_end_matches(".exe").to_string();
    let name = known.unwrap_or_else(|| match stem.to_lowercase().as_str() {
        "java" | "javaw" if !title_of(h).is_empty() => title_of(h),
        _ => stem,
    });
    Some(Game { hwnd: h.0 as isize, pid: pid_of(h), exe, name })
}

/// Fenêtre au premier plan : le jeu qu'elle porte (même règle que Bobyn), son écran (x, y, largeur, hauteur en pixels physiques),
/// et le jeu en cours où qu'il soit (au premier plan d'abord, sinon une fenêtre ouverte d'un jeu connu : alt-tab, jeu en fenêtre).
pub struct Foreground {
    pub game: Option<Game>,
    pub monitor: Option<[i32; 4]>,
    pub running: Option<String>,
}

pub fn foreground(games: &[String]) -> Foreground {
    let h = unsafe { GetForegroundWindow() };
    let monitor = (!h.0.is_null()).then(|| monitor_rect(h)).flatten().map(|m| [m.left, m.top, m.right - m.left, m.bottom - m.top]);
    let game = classify(h, games);
    let running = game.as_ref().map(|g| g.name.clone()).or_else(|| {
        windows().iter().find_map(|w| {
            let path = exe_path(pid_of(hwnd(w.hwnd)))?;
            known_game(&w.exe, &path, games)
        })
    });
    Foreground { game, monitor, running }
}

pub struct WinInfo {
    pub hwnd: isize,
    pub exe: String,
    pub title: String,
}

/// Fenêtres de premier niveau visibles avec un titre.
pub fn windows() -> Vec<WinInfo> {
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        let out = &mut *(l.0 as *mut Vec<WinInfo>);
        if usable(h) && GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0 {
            let mut t = [0u16; 256];
            let n = GetWindowTextW(h, &mut t) as usize;
            if n > 0 {
                if let Some(exe) = exe_of_window(h) {
                    out.push(WinInfo { hwnd: h.0 as isize, exe, title: String::from_utf16_lossy(&t[..n]) });
                }
            }
        }
        true.into()
    }
    let mut out: Vec<WinInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

pub fn find_window(exe: &str) -> Option<isize> {
    windows().into_iter().find(|w| w.exe.eq_ignore_ascii_case(exe)).map(|w| w.hwnd)
}

pub struct MonInfo {
    pub handle: isize,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

/// Moniteurs, le principal en premier.
pub fn monitors() -> Vec<MonInfo> {
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
        let out = &mut *(l.0 as *mut Vec<MonInfo>);
        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(m, &mut mi as *mut _ as *mut MONITORINFO).as_bool() {
            let r = mi.monitorInfo.rcMonitor;
            let n = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
            out.push(MonInfo { handle: m.0 as isize, name: String::from_utf16_lossy(&mi.szDevice[..n]), width: r.right - r.left, height: r.bottom - r.top, primary: mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0 });
        }
        true.into()
    }
    let mut out: Vec<MonInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|m| !m.primary);
    out
}

#[allow(dead_code)]
pub fn title(h: HWND) -> String {
    let mut t = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut t) } as usize;
    String::from_utf16_lossy(&t[..n])
}

/// Fréquence de l'écran principal (60 si inconnue).
pub fn refresh_rate() -> u32 {
    let mut dm = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    let ok = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut dm) }.as_bool();
    if ok && dm.dmDisplayFrequency > 1 { dm.dmDisplayFrequency.min(240) } else { 60 }
}
