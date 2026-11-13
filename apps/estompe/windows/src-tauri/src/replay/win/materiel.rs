use std::sync::OnceLock;

use windows::core::{Interface, PWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{HWND, LUID, RECT};
use windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIFactory6, IDXGIOutput6, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE};
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, DEVMODEW, ENUM_CURRENT_SETTINGS, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFTransform, MFCreateAttributes, MFStartup, MFTEnum2, MFMediaType_Video, MFVideoFormat_H264, MFVideoFormat_HEVC, MFSTARTUP_FULL, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_ADAPTER_LUID, MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_FRIENDLY_NAME_Attribute, MFT_REGISTER_TYPE_INFO, MF_VERSION, MF_VIDEO_MAX_MB_PER_SEC,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

use crate::replay::garde::Essai;
use crate::replay::reglages::{Encodeur, Materiel, MB_S_H264, MB_S_HEVC};
use crate::replay::types::Codec;

pub fn texte(e: windows::core::Error) -> String {
    format!("{} ({:#010x})", e.message(), e.code().0)
}

pub fn initialiser() -> Result<(), String> {
    static MF: OnceLock<Result<(), String>> = OnceLock::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MF.get_or_init(|| MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(texte)).clone()
    }
}

pub struct Gpu {
    pub adaptateur: IDXGIAdapter1,
    pub nom: String,
    pub h264: Option<Encodeur>,
    pub hevc: Option<Encodeur>,
    pub amf: Essai,
}

fn adaptateurs() -> Vec<IDXGIAdapter1> {
    unsafe {
        let Ok(fabrique) = CreateDXGIFactory1::<IDXGIFactory1>() else { return Vec::new() };
        let par_preference = fabrique.cast::<IDXGIFactory6>().ok();
        let suivant = |i: u32| match &par_preference {
            Some(f) => f.EnumAdapterByGpuPreference::<IDXGIAdapter1>(i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE).ok(),
            None => fabrique.EnumAdapters1(i).ok(),
        };
        let materiel = |a: &IDXGIAdapter1| a.GetDesc1().is_ok_and(|d| d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0);
        (0..16).map_while(suivant).filter(materiel).collect()
    }
}

fn luid(adaptateur: &IDXGIAdapter1) -> Option<LUID> {
    unsafe { adaptateur.GetDesc1().ok().map(|d| d.AdapterLuid) }
}

pub fn activations(codec: Codec, adaptateur: &IDXGIAdapter1) -> Vec<IMFActivate> {
    let sortie = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: match codec {
            Codec::H264 => MFVideoFormat_H264,
            Codec::Hevc => MFVideoFormat_HEVC,
        },
    };
    unsafe {
        let Some(luid) = luid(adaptateur) else { return Vec::new() };
        let mut attributs = None;
        if MFCreateAttributes(&mut attributs, 1).is_err() {
            return Vec::new();
        }
        let Some(attributs) = attributs else { return Vec::new() };
        let octets = std::slice::from_raw_parts(&luid as *const LUID as *const u8, std::mem::size_of::<LUID>());
        if attributs.SetBlob(&MFT_ENUM_ADAPTER_LUID, octets).is_err() {
            return Vec::new();
        }
        let (mut liste, mut nombre) = (std::ptr::null_mut::<Option<IMFActivate>>(), 0u32);
        if MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER, None, Some(&sortie), &attributs, &mut liste, &mut nombre).is_err() || liste.is_null() {
            return Vec::new();
        }
        let trouves = (0..nombre as usize).filter_map(|i| std::ptr::read(liste.add(i))).collect();
        CoTaskMemFree(Some(liste as *const _));
        trouves
    }
}

pub fn nom_activation(activation: &IMFActivate) -> String {
    unsafe {
        let (mut nom, mut longueur) = (PWSTR::null(), 0u32);
        if activation.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut nom, &mut longueur).is_err() || nom.is_null() {
            return "encodeur matériel".into();
        }
        let lu = nom.to_string().unwrap_or_default();
        CoTaskMemFree(Some(nom.0 as *const _));
        lu
    }
}

fn encodeur(codec: Codec, adaptateur: &IDXGIAdapter1) -> Option<Encodeur> {
    let activation = activations(codec, adaptateur).into_iter().next()?;
    let (defaut, cote) = match codec {
        Codec::H264 => (MB_S_H264, 4096),
        Codec::Hevc => (MB_S_HEVC, 8192),
    };
    let annonce = unsafe {
        let lu = activation.ActivateObject::<IMFTransform>().ok().and_then(|t| t.GetAttributes().ok()).and_then(|a| a.GetUINT32(&MF_VIDEO_MAX_MB_PER_SEC).ok());
        let _ = activation.ShutdownObject();
        lu
    };
    Some(Encodeur { nom: nom_activation(&activation), mb_par_seconde: annonce.filter(|v| *v > 0).map_or(defaut, u64::from), largeur_max: cote, hauteur_max: cote })
}

pub fn adaptateur_de(haut: i32, bas: u32) -> Option<IDXGIAdapter1> {
    adaptateurs().into_iter().find(|a| luid(a).is_some_and(|l| l.HighPart == haut && l.LowPart == bas))
}

fn encodeur_amf(codec: Codec, taille: Option<(u32, u32)>) -> Option<Encodeur> {
    let (largeur_max, hauteur_max) = taille?;
    let (nom, mb_par_seconde) = match codec {
        Codec::H264 => ("AMD AMF H.264", MB_S_H264),
        Codec::Hevc => ("AMD AMF HEVC", MB_S_HEVC),
    };
    Some(Encodeur { nom: nom.into(), mb_par_seconde, largeur_max, hauteur_max })
}

pub fn gpu(amf_permis: bool) -> Result<Gpu, String> {
    initialiser()?;
    for adaptateur in adaptateurs() {
        let (h264, hevc) = (encodeur(Codec::H264, &adaptateur), encodeur(Codec::Hevc, &adaptateur));
        if h264.is_some() || hevc.is_some() {
            let nom = unsafe { adaptateur.GetDesc1().map(|d| String::from_utf16_lossy(&d.Description).trim_end_matches('\0').to_string()).unwrap_or_default() };
            let amf = if amf_permis {
                super::amf::essai_hors_processus(&adaptateur).unwrap_or_else(|e| {
                    crate::debuglog::log("warn", super::SOURCE, &format!("AMF écarté : {e}"));
                    Essai::default()
                })
            } else {
                Essai::default()
            };
            let (h264, hevc) = (encodeur_amf(Codec::H264, amf.h264).or(h264), encodeur_amf(Codec::Hevc, amf.hevc).or(hevc));
            return Ok(Gpu { adaptateur, nom, h264, hevc, amf });
        }
    }
    Err("aucun encodeur vidéo matériel (AMD, NVIDIA ou Intel) : l'instant replay est indisponible sur cette machine".into())
}

fn ecran(fenetre: Option<isize>) -> HMONITOR {
    unsafe {
        match fenetre {
            Some(f) => MonitorFromWindow(HWND(f as *mut _), MONITOR_DEFAULTTONEAREST),
            None => MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY),
        }
    }
}

fn nom_ecran(ecran: HMONITOR) -> Option<[u16; 32]> {
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        GetMonitorInfoW(ecran, &mut info as *mut MONITORINFOEXW as *mut _).as_bool().then_some(info.szDevice)
    }
}

fn mode(fenetre: Option<isize>) -> (u32, (u32, u32)) {
    unsafe {
        let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        let lu = nom_ecran(ecran(fenetre)).is_some_and(|nom| EnumDisplaySettingsW(windows::core::PCWSTR(nom.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode).as_bool());
        if lu {
            (if mode.dmDisplayFrequency > 1 { mode.dmDisplayFrequency } else { 60 }, (mode.dmPelsWidth, mode.dmPelsHeight))
        } else {
            (60, (1920, 1080))
        }
    }
}

pub fn taille_fenetre(fenetre: isize) -> Option<(u32, u32)> {
    unsafe {
        let mut r = RECT::default();
        GetWindowRect(HWND(fenetre as *mut _), &mut r).ok()?;
        (r.right > r.left && r.bottom > r.top).then(|| ((r.right - r.left) as u32, (r.bottom - r.top) as u32))
    }
}

fn ram_libre() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) != 0 {
            m.ullAvailPhys
        } else {
            0
        }
    }
}

pub fn lire(gpu: &Gpu, fenetre: Option<isize>, source: Option<(u32, u32)>) -> Materiel {
    let (ecran_hz, bureau) = mode(fenetre);
    let source = source.or_else(|| fenetre.and_then(taille_fenetre)).unwrap_or(bureau);
    Materiel { ecran_hz, source, h264: gpu.h264.clone(), hevc: gpu.hevc.clone(), ram_libre: ram_libre() }
}

fn blanc_sdr(nom: &[u16; 32]) -> Option<f32> {
    unsafe {
        let (mut chemins, mut modes) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut chemins, &mut modes).0 != 0 {
            return None;
        }
        let mut liste = vec![DISPLAYCONFIG_PATH_INFO::default(); chemins as usize];
        let mut infos = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut chemins, liste.as_mut_ptr(), &mut modes, infos.as_mut_ptr(), None).0 != 0 {
            return None;
        }
        liste.truncate(chemins as usize);
        for chemin in liste {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source.header.adapterId = chemin.sourceInfo.adapterId;
            source.header.id = chemin.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut source.header) != 0 || source.viewGdiDeviceName != *nom {
                continue;
            }
            let mut blanc = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
            blanc.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL;
            blanc.header.size = std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32;
            blanc.header.adapterId = chemin.targetInfo.adapterId;
            blanc.header.id = chemin.targetInfo.id;
            if DisplayConfigGetDeviceInfo(&mut blanc.header) == 0 && blanc.SDRWhiteLevel > 0 {
                return Some(blanc.SDRWhiteLevel as f32 * 80.0 / 1000.0);
            }
        }
        None
    }
}

pub fn blanc_hdr(fenetre: isize) -> Option<f32> {
    unsafe {
        let ecran = ecran(Some(fenetre));
        let fabrique = CreateDXGIFactory1::<IDXGIFactory1>().ok()?;
        let adaptateurs = (0..16).map_while(|i| fabrique.EnumAdapters1(i).ok());
        let sorties = adaptateurs.flat_map(|a| (0..16).map_while(move |i| a.EnumOutputs(i).ok()));
        let sortie = sorties.filter_map(|s| s.cast::<IDXGIOutput6>().ok()).find_map(|s| s.GetDesc1().ok().filter(|d| d.Monitor == ecran))?;
        if sortie.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
            return None;
        }
        Some(nom_ecran(ecran).and_then(|n| blanc_sdr(&n)).unwrap_or(200.0))
    }
}
