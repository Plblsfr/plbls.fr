use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetForegroundWindow, GetWindowLongW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    GA_ROOT, GWL_EXSTYLE, GWL_STYLE, WS_CAPTION, WS_EX_TOOLWINDOW,
};

use crate::traits::{ApplicationPremierPlan, ZoneEcran};

fn processus_de(fenetre: HWND) -> u32 {
    let mut processus = 0;
    unsafe { GetWindowThreadProcessId(fenetre, &mut processus) };
    processus
}

fn chemin_du_processus(processus: u32) -> Option<String> {
    unsafe {
        let poignee = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, processus);
        if poignee.is_null() {
            return None;
        }
        let mut tampon = [0u16; 520];
        let mut longueur = tampon.len() as u32;
        let lu = QueryFullProcessImageNameW(
            poignee,
            PROCESS_NAME_WIN32,
            tampon.as_mut_ptr(),
            &mut longueur,
        ) != 0;
        CloseHandle(poignee);
        lu.then(|| String::from_utf16_lossy(&tampon[..longueur as usize]))
    }
}

fn titre_de(fenetre: HWND) -> String {
    let mut tampon = [0u16; 256];
    let n = unsafe { GetWindowTextW(fenetre, tampon.as_mut_ptr(), tampon.len() as i32) }.max(0)
        as usize;
    String::from_utf16_lossy(&tampon[..n])
}

fn ecran_de(fenetre: HWND) -> Option<RECT> {
    unsafe {
        let mut infos: MONITORINFO = std::mem::zeroed();
        infos.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(
            MonitorFromWindow(fenetre, MONITOR_DEFAULTTONEAREST),
            &mut infos,
        ) != 0)
            .then_some(infos.rcMonitor)
    }
}

fn utilisable(fenetre: HWND) -> bool {
    unsafe {
        !fenetre.is_null()
            && IsWindowVisible(fenetre) != 0
            && IsIconic(fenetre) == 0
            && GetAncestor(fenetre, GA_ROOT) == fenetre
    }
}

fn plein_ecran(fenetre: HWND, ecran: &RECT) -> bool {
    let style = unsafe { GetWindowLongW(fenetre, GWL_STYLE) } as u32;
    let mut r: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(fenetre, &mut r) } == 0 {
        return false;
    }
    let couvre = r.left <= ecran.left
        && r.top <= ecran.top
        && r.right >= ecran.right
        && r.bottom >= ecran.bottom;
    couvre && style & WS_CAPTION != WS_CAPTION
}

fn decrire(fenetre: HWND) -> Option<ApplicationPremierPlan> {
    if !utilisable(fenetre) {
        return None;
    }
    let processus = processus_de(fenetre);
    let chemin = chemin_du_processus(processus)?;
    let ecran = ecran_de(fenetre);
    Some(ApplicationPremierPlan {
        executable: super::nom_de_fichier(&chemin).to_string(),
        titre: titre_de(fenetre),
        plein_ecran: ecran.as_ref().is_some_and(|e| plein_ecran(fenetre, e)),
        ecran: ecran.map(|e| ZoneEcran {
            x: e.left,
            y: e.top,
            largeur: (e.right - e.left).max(0) as u32,
            hauteur: (e.bottom - e.top).max(0) as u32,
        }),
        fenetre: fenetre as usize as u64,
        processus,
        chemin,
    })
}

pub fn application() -> Option<ApplicationPremierPlan> {
    decrire(unsafe { GetForegroundWindow() })
}

pub fn fenetres() -> Vec<ApplicationPremierPlan> {
    unsafe extern "system" fn chaque(fenetre: HWND, parametre: LPARAM) -> i32 {
        let liste = &mut *(parametre as *mut Vec<HWND>);
        if utilisable(fenetre)
            && GetWindowLongW(fenetre, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
            && GetWindowTextLengthW(fenetre) > 0
        {
            liste.push(fenetre);
        }
        1
    }
    let mut liste: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(chaque), &mut liste as *mut Vec<HWND> as LPARAM) };
    let mut vues = std::collections::HashSet::new();
    liste
        .into_iter()
        .filter(|f| vues.insert(processus_de(*f)))
        .filter_map(decrire)
        .collect()
}
