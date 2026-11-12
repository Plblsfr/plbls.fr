#[cfg_attr(not(windows), allow(dead_code))]
mod connus;

#[cfg(windows)]
mod detection {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    const NOT_GAMES: &[&str] = &[
        "explorer.exe", "estompe.exe", "applicationframehost.exe", "searchhost.exe", "shellexperiencehost.exe",
        "startmenuexperiencehost.exe", "textinputhost.exe", "lockapp.exe", "taskmgr.exe", "chrome.exe", "msedge.exe", "firefox.exe",
        "brave.exe", "opera.exe", "vivaldi.exe", "mpv.exe", "vlc.exe", "mpc-hc64.exe", "wmplayer.exe", "microsoft.media.player.exe", "powerpnt.exe", "discord.exe",
    ];

    pub struct Foreground {
        pub game: bool,
        pub monitor: Option<[i32; 4]>,
        pub running: Option<String>,
    }

    fn pid_of(h: HWND) -> u32 {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(h, &mut pid) };
        pid
    }

    fn exe_path(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(h);
            ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    fn exe_name(path: &str) -> String {
        path.rsplit('\\').next().unwrap_or(path).to_string()
    }

    fn title_of(h: HWND) -> String {
        let mut t = [0u16; 256];
        let n = unsafe { GetWindowTextW(h, t.as_mut_ptr(), t.len() as i32) }.max(0) as usize;
        String::from_utf16_lossy(&t[..n])
    }

    fn known_game(exe: &str, path: &str) -> Option<String> {
        if NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(exe)) {
            return None;
        }
        super::connus::name_of(exe, path)
    }

    fn monitor_rect(h: HWND) -> Option<RECT> {
        unsafe {
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            (GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut mi) != 0).then_some(mi.rcMonitor)
        }
    }

    fn usable(h: HWND) -> bool {
        unsafe { !h.is_null() && IsWindowVisible(h) != 0 && IsIconic(h) == 0 && GetAncestor(h, GA_ROOT) == h }
    }

    fn classify(h: HWND) -> Option<String> {
        if !usable(h) {
            return None;
        }
        let path = exe_path(pid_of(h))?;
        let exe = exe_name(&path);
        let known = known_game(&exe, &path);
        if known.is_none() {
            if NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(&exe)) {
                return None;
            }
            let style = unsafe { GetWindowLongW(h, GWL_STYLE) } as u32;
            let m = monitor_rect(h)?;
            let mut r: RECT = unsafe { std::mem::zeroed() };
            if unsafe { GetWindowRect(h, &mut r) } == 0 {
                return None;
            }
            let covers = r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom;
            if !covers || style & WS_CAPTION == WS_CAPTION {
                return None;
            }
        }
        let stem = exe.trim_end_matches(".exe").to_string();
        Some(known.unwrap_or_else(|| match stem.to_lowercase().as_str() {
            "java" | "javaw" if !title_of(h).is_empty() => title_of(h),
            _ => stem,
        }))
    }

    fn open_known_game() -> Option<String> {
        unsafe extern "system" fn each(h: HWND, l: LPARAM) -> i32 {
            let found = &mut *(l as *mut Option<String>);
            if usable(h) && GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0 && GetWindowTextLengthW(h) > 0 {
                if let Some(path) = exe_path(pid_of(h)) {
                    if let Some(name) = known_game(&exe_name(&path), &path) {
                        *found = Some(name);
                        return 0;
                    }
                }
            }
            1
        }
        let mut found: Option<String> = None;
        unsafe { EnumWindows(Some(each), &mut found as *mut _ as LPARAM) };
        found
    }

    pub fn foreground() -> Foreground {
        let h: HWND = unsafe { GetForegroundWindow() };
        let monitor = (!h.is_null()).then(|| monitor_rect(h)).flatten().map(|m| [m.left, m.top, m.right - m.left, m.bottom - m.top]);
        let game = classify(h);
        let running = game.clone().or_else(open_known_game);
        Foreground { game: game.is_some(), monitor, running }
    }
}

#[cfg(windows)]
pub use detection::{foreground, Foreground};
