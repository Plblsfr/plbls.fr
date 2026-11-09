#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};

const INTERVALLE_MS: u64 = 1000;
const INTERVALLE_MIN_MS: u64 = 200;
const RAFRAICHIR_TOUS: u32 = 5;
const OPTIONS_TAURI: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
const FILETIME_EPOQUE_UNIX: u64 = 116_444_736_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Processus {
    pub pid: u32,
    #[serde(rename = "type")]
    pub genre: &'static str,
    pub role: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Compteurs {
    pub prive: u64,
    pub ws: u64,
    pub pic_ws: u64,
    pub cpu_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Mesure {
    #[serde(flatten)]
    pub processus: Processus,
    #[serde(flatten)]
    pub compteurs: Compteurs,
}

#[derive(Clone, Debug, Serialize)]
pub struct Echantillon {
    pub t: u64,
    pub processus: Vec<Mesure>,
}

#[derive(Debug, Serialize)]
pub struct Releve {
    pub debut: u64,
    pub fin: u64,
    pub intervalle_ms: u64,
    pub echantillons: Vec<Echantillon>,
}

#[derive(Debug, Serialize)]
pub struct Systeme {
    pub app: String,
    pub webview2: Option<String>,
    pub windows: Option<String>,
    pub cpu: Option<String>,
    pub coeurs: usize,
    pub ram_mo: Option<u64>,
    pub gpu: Vec<String>,
    pub facteur_echelle: Option<f64>,
    pub options_webview2: Vec<String>,
    pub demarrage_processus: Option<u64>,
}

struct Session {
    actif: Arc<AtomicBool>,
    debut: u64,
    intervalle: u64,
    echantillons: Arc<Mutex<Vec<Echantillon>>>,
    fil: JoinHandle<()>,
}

#[derive(Default)]
pub struct Mesures(Mutex<Option<Session>>);

pub fn maintenant_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn filetime_en_ms(ticks: u64) -> u64 {
    ticks / 10_000
}

pub fn filetime_en_epoque_ms(ticks: u64) -> Option<u64> {
    ticks.checked_sub(FILETIME_EPOQUE_UNIX).map(filetime_en_ms)
}

pub fn genre_processus(kind: i32) -> &'static str {
    match kind {
        0 => "navigateur",
        1 => "rendu",
        2 => "utilitaire",
        3 => "bac-a-sable",
        4 => "gpu",
        5 | 6 => "plugin",
        _ => "autre",
    }
}

fn role_source(source: &str) -> &'static str {
    match source.rsplit_once('#').map(|(_, ancre)| ancre) {
        Some("overlay") => "overlay-vocal",
        Some("msg-overlay") => "overlay-messages",
        Some("bobyn-overlay") => "overlay-bobyn",
        _ => "principale",
    }
}

pub fn role_rendu<'a>(sources: impl IntoIterator<Item = &'a str>) -> &'static str {
    let roles: BTreeSet<&'static str> = sources.into_iter().map(role_source).collect();
    match roles.len() {
        0 => "inactif",
        1 => roles.into_iter().next().unwrap_or("inactif"),
        _ => "mixte",
    }
}

pub fn options_webview2(config: Option<&str>, env: Option<&str>) -> Vec<String> {
    env.or(config)
        .unwrap_or(OPTIONS_TAURI)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn options_actives(app: &AppHandle) -> Vec<String> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .and_then(|w| w.additional_browser_args.clone());
    let env = std::env::var(crate::webview2_options::VARIABLE).ok();
    options_webview2(config.as_deref(), env.as_deref())
}

fn echantillonner(connus: &Mutex<Vec<Processus>>) -> Echantillon {
    let mut liste = vec![Processus {
        pid: std::process::id(),
        genre: "hote",
        role: "hote",
    }];
    liste.extend(connus.lock().map(|l| l.clone()).unwrap_or_default());
    let processus = liste
        .into_iter()
        .filter_map(|p| {
            plateforme::compteurs(p.pid).map(|compteurs| Mesure {
                processus: p,
                compteurs,
            })
        })
        .collect();
    Echantillon {
        t: maintenant_ms(),
        processus,
    }
}

fn boucle(
    app: AppHandle,
    actif: Arc<AtomicBool>,
    echantillons: Arc<Mutex<Vec<Echantillon>>>,
    intervalle: u64,
) {
    let connus: Arc<Mutex<Vec<Processus>>> = Arc::default();
    let mut tour = 0u32;
    while actif.load(Ordering::Acquire) {
        if tour == 0 {
            plateforme::rafraichir(&app, connus.clone());
        }
        tour = (tour + 1) % RAFRAICHIR_TOUS;
        let e = echantillonner(&connus);
        if let Ok(mut l) = echantillons.lock() {
            l.push(e);
        }
        std::thread::park_timeout(Duration::from_millis(intervalle));
    }
}

#[tauri::command]
pub fn mesures_demarrer(
    app: AppHandle,
    etat: State<'_, Mesures>,
    intervalle_ms: Option<u64>,
) -> Result<u64, String> {
    let mut session = etat.0.lock().map_err(|e| e.to_string())?;
    if session.is_some() {
        return Err("une session de mesure est déjà en cours".into());
    }
    let intervalle = intervalle_ms
        .unwrap_or(INTERVALLE_MS)
        .max(INTERVALLE_MIN_MS);
    let actif = Arc::new(AtomicBool::new(true));
    let echantillons: Arc<Mutex<Vec<Echantillon>>> = Arc::default();
    let debut = maintenant_ms();
    let fil = {
        let (actif, echantillons) = (actif.clone(), echantillons.clone());
        std::thread::Builder::new()
            .name("mesures".into())
            .spawn(move || boucle(app, actif, echantillons, intervalle))
            .map_err(|e| e.to_string())?
    };
    *session = Some(Session {
        actif,
        debut,
        intervalle,
        echantillons,
        fil,
    });
    Ok(debut)
}

#[tauri::command]
pub fn mesures_arreter(etat: State<'_, Mesures>) -> Result<Releve, String> {
    let session = etat
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("aucune session de mesure en cours")?;
    session.actif.store(false, Ordering::Release);
    session.fil.thread().unpark();
    let _ = session.fil.join();
    let echantillons =
        std::mem::take(&mut *session.echantillons.lock().map_err(|e| e.to_string())?);
    Ok(Releve {
        debut: session.debut,
        fin: maintenant_ms(),
        intervalle_ms: session.intervalle,
        echantillons,
    })
}

#[tauri::command]
pub fn mesures_systeme(app: AppHandle) -> Systeme {
    Systeme {
        app: app.package_info().version.to_string(),
        webview2: tauri::webview_version().ok(),
        windows: plateforme::version_windows(),
        cpu: plateforme::nom_cpu(),
        coeurs: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        ram_mo: plateforme::ram_mo(),
        gpu: plateforme::gpus(),
        facteur_echelle: app
            .get_webview_window("main")
            .and_then(|w| w.scale_factor().ok()),
        options_webview2: options_actives(&app),
        demarrage_processus: plateforme::creation_processus(),
    }
}

#[cfg(windows)]
mod plateforme {
    use super::{
        filetime_en_epoque_ms, filetime_en_ms, genre_processus, role_rendu, Compteurs, Processus,
    };
    use std::sync::{Arc, Mutex};
    use tauri::{AppHandle, Manager};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Controller, ICoreWebView2Environment13, ICoreWebView2ProcessExtendedInfo,
        ICoreWebView2ProcessExtendedInfoCollection, ICoreWebView2_2, COREWEBVIEW2_PROCESS_KIND,
    };
    use webview2_com::{CoTaskMemPWSTR, GetProcessExtendedInfosCompletedHandler};
    use windows_core::{Interface, BOOL, PWSTR};
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    fn large(ft: &FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn texte(buf: &[u16]) -> String {
        let fin = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..fin]).trim().to_owned()
    }

    pub fn compteurs(pid: u32) -> Option<Compteurs> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut m: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            m.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            let memoire =
                GetProcessMemoryInfo(h, &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, m.cb)
                    != 0;
            let (mut c, mut s, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
                std::mem::zeroed();
            let temps = GetProcessTimes(h, &mut c, &mut s, &mut k, &mut u) != 0;
            CloseHandle(h);
            memoire.then(|| Compteurs {
                prive: m.PrivateUsage as u64,
                ws: m.WorkingSetSize as u64,
                pic_ws: m.PeakWorkingSetSize as u64,
                cpu_ms: if temps {
                    filetime_en_ms(large(&k) + large(&u))
                } else {
                    0
                },
            })
        }
    }

    pub fn creation_processus() -> Option<u64> {
        unsafe {
            let (mut c, mut s, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
                std::mem::zeroed();
            (GetProcessTimes(GetCurrentProcess(), &mut c, &mut s, &mut k, &mut u) != 0)
                .then(|| filetime_en_epoque_ms(large(&c)))
                .flatten()
        }
    }

    fn registre_texte(cle: &str, valeur: &str) -> Option<String> {
        let (cle, valeur) = (wide(cle), wide(valeur));
        let mut buf = vec![0u16; 256];
        let mut taille = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                cle.as_ptr(),
                valeur.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut taille,
            )
        };
        (r == 0).then(|| texte(&buf)).filter(|s| !s.is_empty())
    }

    fn registre_nombre(cle: &str, valeur: &str) -> Option<u32> {
        let (cle, valeur) = (wide(cle), wide(valeur));
        let mut v = 0u32;
        let mut taille = 4u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                cle.as_ptr(),
                valeur.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut v as *mut u32).cast(),
                &mut taille,
            )
        };
        (r == 0).then_some(v)
    }

    pub fn version_windows() -> Option<String> {
        let cle = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let build = registre_texte(cle, "CurrentBuild")?;
        let nom = if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
            "Windows 11".to_owned()
        } else {
            registre_texte(cle, "ProductName").unwrap_or_else(|| "Windows".into())
        };
        let affichage = registre_texte(cle, "DisplayVersion")
            .map(|v| format!(" {v}"))
            .unwrap_or_default();
        let ubr = registre_nombre(cle, "UBR")
            .map(|u| format!(".{u}"))
            .unwrap_or_default();
        Some(format!("{nom}{affichage} (build {build}{ubr})"))
    }

    pub fn nom_cpu() -> Option<String> {
        registre_texte(
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        )
    }

    pub fn ram_mo() -> Option<u64> {
        unsafe {
            let mut m: MEMORYSTATUSEX = std::mem::zeroed();
            m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            (GlobalMemoryStatusEx(&mut m) != 0).then_some(m.ullTotalPhys / (1024 * 1024))
        }
    }

    pub fn gpus() -> Vec<String> {
        let mut noms: Vec<String> = Vec::new();
        for i in 0.. {
            let mut d: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
            d.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
            if unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut d, 0) } == 0 {
                break;
            }
            let nom = texte(&d.DeviceString);
            if d.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != 0
                && !nom.is_empty()
                && !noms.contains(&nom)
            {
                noms.push(nom);
            }
        }
        noms
    }

    unsafe fn sources(
        info: &ICoreWebView2ProcessExtendedInfo,
    ) -> windows_core::Result<Vec<String>> {
        let mut liste = Vec::new();
        let it = info.AssociatedFrameInfos()?.GetIterator()?;
        let mut encore = BOOL::default();
        it.HasCurrent(&mut encore)?;
        while encore.as_bool() {
            let mut source = PWSTR::null();
            it.GetCurrent()?.Source(&mut source)?;
            liste.push(CoTaskMemPWSTR::from(source).to_string());
            it.MoveNext(&mut encore)?;
        }
        Ok(liste)
    }

    unsafe fn lire(
        liste: &ICoreWebView2ProcessExtendedInfoCollection,
    ) -> windows_core::Result<Vec<Processus>> {
        let mut n = 0u32;
        liste.Count(&mut n)?;
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let info = liste.GetValueAtIndex(i)?;
            let p = info.ProcessInfo()?;
            let (mut pid, mut kind) = (0i32, COREWEBVIEW2_PROCESS_KIND::default());
            p.ProcessId(&mut pid)?;
            p.Kind(&mut kind)?;
            let genre = genre_processus(kind.0);
            let role = if genre == "rendu" {
                let s = sources(&info).unwrap_or_default();
                role_rendu(s.iter().map(String::as_str))
            } else {
                "commun"
            };
            out.push(Processus {
                pid: pid as u32,
                genre,
                role,
            });
        }
        Ok(out)
    }

    unsafe fn demander(
        ctrl: &ICoreWebView2Controller,
        connus: Arc<Mutex<Vec<Processus>>>,
    ) -> windows_core::Result<()> {
        let env = ctrl
            .CoreWebView2()?
            .cast::<ICoreWebView2_2>()?
            .Environment()?
            .cast::<ICoreWebView2Environment13>()?;
        let rappel =
            GetProcessExtendedInfosCompletedHandler::create(Box::new(move |resultat, liste| {
                resultat?;
                if let Some(liste) = liste {
                    let lus = lire(&liste)?;
                    if let Ok(mut c) = connus.lock() {
                        *c = lus;
                    }
                }
                Ok(())
            }));
        env.GetProcessExtendedInfos(&rappel)
    }

    pub fn rafraichir(app: &AppHandle, connus: Arc<Mutex<Vec<Processus>>>) {
        let Some(w) = app.get_webview_window("main") else {
            return;
        };
        let _ = w.with_webview(move |wv| {
            let _ = unsafe { demander(&wv.controller(), connus) };
        });
    }
}

#[cfg(not(windows))]
mod plateforme {
    use super::{Compteurs, Processus};
    use std::sync::{Arc, Mutex};
    use tauri::AppHandle;

    pub fn compteurs(_pid: u32) -> Option<Compteurs> {
        None
    }
    pub fn creation_processus() -> Option<u64> {
        None
    }
    pub fn version_windows() -> Option<String> {
        None
    }
    pub fn nom_cpu() -> Option<String> {
        None
    }
    pub fn ram_mo() -> Option<u64> {
        None
    }
    pub fn gpus() -> Vec<String> {
        Vec::new()
    }
    pub fn rafraichir(_app: &AppHandle, _connus: Arc<Mutex<Vec<Processus>>>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_des_processus_webview2() {
        assert_eq!(genre_processus(0), "navigateur");
        assert_eq!(genre_processus(1), "rendu");
        assert_eq!(genre_processus(2), "utilitaire");
        assert_eq!(genre_processus(4), "gpu");
        assert_eq!(genre_processus(42), "autre");
    }

    #[test]
    fn role_d_un_processus_de_rendu_selon_ses_cadres() {
        assert_eq!(role_rendu(["https://tauri.localhost/"]), "principale");
        assert_eq!(
            role_rendu([
                "https://tauri.localhost/",
                "https://www.youtube-nocookie.com/embed/x"
            ]),
            "principale"
        );
        assert_eq!(
            role_rendu(["https://tauri.localhost/index.html#overlay"]),
            "overlay-vocal"
        );
        assert_eq!(
            role_rendu(["https://tauri.localhost/index.html#msg-overlay"]),
            "overlay-messages"
        );
        assert_eq!(
            role_rendu(["https://tauri.localhost/index.html#bobyn-overlay"]),
            "overlay-bobyn"
        );
        assert_eq!(
            role_rendu([
                "https://tauri.localhost/",
                "https://tauri.localhost/index.html#overlay"
            ]),
            "mixte"
        );
        assert_eq!(role_rendu(std::iter::empty()), "inactif");
    }

    #[test]
    fn conversion_des_filetime() {
        assert_eq!(filetime_en_ms(25_000_000), 2500);
        assert_eq!(
            filetime_en_epoque_ms(FILETIME_EPOQUE_UNIX + 10_000),
            Some(1)
        );
        assert_eq!(filetime_en_epoque_ms(1), None);
    }

    #[test]
    fn options_par_defaut_de_tauri_et_variable_d_environnement_prioritaire() {
        assert_eq!(options_webview2(None, None), vec![OPTIONS_TAURI.to_owned()]);
        assert_eq!(
            options_webview2(Some("--a  --b=1"), None),
            vec!["--a".to_owned(), "--b=1".to_owned()]
        );
        assert_eq!(options_webview2(Some("--a"), Some("--c  --d")), vec!["--c".to_owned(), "--d".to_owned()]);
    }

    #[test]
    fn echantillon_horodate_et_limite_aux_processus_connus() {
        let connus = Mutex::new(vec![Processus {
            pid: 1,
            genre: "rendu",
            role: "principale",
        }]);
        let e = echantillonner(&connus);
        assert!(e.t > 0);
        assert!(e
            .processus
            .iter()
            .all(|m| m.processus.pid == std::process::id() || m.processus.pid == 1));
    }

    #[test]
    fn mesure_serialisee_a_plat() {
        let m = Mesure {
            processus: Processus {
                pid: 7,
                genre: "gpu",
                role: "commun",
            },
            compteurs: Compteurs {
                prive: 1,
                ws: 2,
                pic_ws: 3,
                cpu_ms: 4,
            },
        };
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            serde_json::json!({ "pid": 7, "type": "gpu", "role": "commun", "prive": 1, "ws": 2, "pic_ws": 3, "cpu_ms": 4 })
        );
    }
}
