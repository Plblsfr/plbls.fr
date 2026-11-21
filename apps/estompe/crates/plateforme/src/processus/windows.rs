use crate::traits::{CompteursProcessus, InfosMachine};
use windows_sys::Win32::Foundation::FILETIME;
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
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

const TOPS_PAR_MS: u64 = 10_000;

fn large(ft: &FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

fn large_chaine(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn texte(tampon: &[u16]) -> String {
    let fin = tampon.iter().position(|&c| c == 0).unwrap_or(tampon.len());
    String::from_utf16_lossy(&tampon[..fin]).trim().to_owned()
}

pub fn compteurs() -> Option<CompteursProcessus> {
    unsafe {
        let processus = GetCurrentProcess();
        let mut m: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        m.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if GetProcessMemoryInfo(
            processus,
            &mut m as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            m.cb,
        ) == 0
        {
            return None;
        }
        let (mut creation, mut sortie, mut noyau, mut utilisateur): (
            FILETIME,
            FILETIME,
            FILETIME,
            FILETIME,
        ) = std::mem::zeroed();
        let temps = GetProcessTimes(
            processus,
            &mut creation,
            &mut sortie,
            &mut noyau,
            &mut utilisateur,
        ) != 0;
        Some(CompteursProcessus {
            prive: m.PrivateUsage as u64,
            ws: m.WorkingSetSize as u64,
            pic_ws: m.PeakWorkingSetSize as u64,
            cpu_ms: if temps {
                (large(&noyau) + large(&utilisateur)) / TOPS_PAR_MS
            } else {
                0
            },
        })
    }
}

fn registre_texte(cle: &str, valeur: &str) -> Option<String> {
    let (cle, valeur) = (large_chaine(cle), large_chaine(valeur));
    let mut tampon = vec![0u16; 256];
    let mut taille = (tampon.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            cle.as_ptr(),
            valeur.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            tampon.as_mut_ptr().cast(),
            &mut taille,
        )
    };
    (r == 0).then(|| texte(&tampon)).filter(|s| !s.is_empty())
}

fn registre_nombre(cle: &str, valeur: &str) -> Option<u32> {
    let (cle, valeur) = (large_chaine(cle), large_chaine(valeur));
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

fn version_windows() -> Option<String> {
    let cle = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let build = registre_texte(cle, "CurrentBuild")?;
    let nom = if build.parse::<u32>().is_ok_and(|b| b >= 22_000) {
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

fn ram_mo() -> Option<u64> {
    unsafe {
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        (GlobalMemoryStatusEx(&mut m) != 0).then_some(m.ullTotalPhys / (1024 * 1024))
    }
}

fn gpus() -> Vec<String> {
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

pub fn machine() -> InfosMachine {
    InfosMachine {
        os: version_windows(),
        cpu: registre_texte(
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        ),
        coeurs: std::thread::available_parallelism().map_or(0, |n| n.get()),
        ram_mo: ram_mo(),
        gpu: gpus(),
    }
}
