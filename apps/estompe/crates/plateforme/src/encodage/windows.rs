use std::cell::Cell;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};

use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::{E_UNEXPECTED, HWND, LUID, VARIANT_BOOL};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIFactory6, DXGI_ADAPTER_FLAG_SOFTWARE,
    DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, DEVMODEW, ENUM_CURRENT_SETTINGS,
    MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_CBR, eAVEncH264VProfile_High, eAVEncH265VProfile_Main_420_8,
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
    CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncMPVGOPSize,
    CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode, ICodecAPI, IMFActivate,
    IMFMediaEventGenerator, IMFMediaType, IMFSample, IMFShutdown, IMFTransform,
    METransformHaveOutput, METransformNeedInput, MFAudioFormat_AAC, MFAudioFormat_PCM,
    MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Audio,
    MFMediaType_Video, MFSampleExtension_CleanPoint, MFSampleExtension_Discontinuity, MFStartup,
    MFTEnum2, MFTEnumEx, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_H264, MFVideoFormat_HEVC,
    MFVideoFormat_NV12, MFVideoInterlace_Progressive, MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS,
    MFSTARTUP_FULL, MFT_CATEGORY_AUDIO_ENCODER, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_ADAPTER_LUID,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_END_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
    MFT_REGISTER_TYPE_INFO, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE,
    MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE,
    MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT,
    MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE,
    MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE,
    MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
    MF_VIDEO_MAX_MB_PER_SEC,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_UI4};

use super::{
    CapaciteEncodeur, CodecVideo, EncodeurAudio, EncodeurVideo, ImageNv12, MaterielEncodage,
    Paquet, ParametresAudio, ParametresVideo, RecepteurPaquets, SignalArret, UNITE_TEMPS,
};
use crate::{Erreur, Resultat};

const MB_S_H264: u64 = 2_073_600;
const MB_S_HEVC: u64 = 16_711_680;
const BLOC_AAC: usize = 1024;
const SANS_ENCODEUR: &str =
    "aucun encodeur vidéo matériel (AMD, NVIDIA ou Intel) : l'instant replay est indisponible sur cette machine";

struct Envoi<T>(T);

unsafe impl<T> Send for Envoi<T> {}

impl<T> Envoi<T> {
    fn prendre(self) -> T {
        self.0
    }
}

fn texte(e: windows::core::Error) -> String {
    format!("{} ({:#010x})", e.message(), e.code().0)
}

fn echec(e: windows::core::Error) -> Erreur {
    Erreur::Echec(texte(e))
}

fn initialiser() -> Resultat<()> {
    static MF: OnceLock<Result<(), String>> = OnceLock::new();
    thread_local! {
        static COM: Cell<bool> = const { Cell::new(false) };
    }
    COM.with(|com| {
        if !com.get() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            com.set(true);
        }
    });
    MF.get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(texte) })
        .clone()
        .map_err(Erreur::Echec)
}

fn adaptateurs() -> Vec<IDXGIAdapter1> {
    unsafe {
        let Ok(fabrique) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return Vec::new();
        };
        let par_preference = fabrique.cast::<IDXGIFactory6>().ok();
        let suivant = |i: u32| match &par_preference {
            Some(f) => f
                .EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                    i,
                    DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                )
                .ok(),
            None => fabrique.EnumAdapters1(i).ok(),
        };
        let materiel = |a: &IDXGIAdapter1| {
            a.GetDesc1()
                .is_ok_and(|d| d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0)
        };
        (0..16).map_while(suivant).filter(materiel).collect()
    }
}

fn nom_adaptateur(adaptateur: &IDXGIAdapter1) -> String {
    unsafe {
        adaptateur
            .GetDesc1()
            .map(|d| {
                String::from_utf16_lossy(&d.Description)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default()
    }
}

fn activations(codec: CodecVideo, adaptateur: &IDXGIAdapter1) -> Vec<IMFActivate> {
    let sortie = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: match codec {
            CodecVideo::H264 => MFVideoFormat_H264,
            CodecVideo::Hevc => MFVideoFormat_HEVC,
        },
    };
    unsafe {
        let Ok(luid) = adaptateur.GetDesc1().map(|d| d.AdapterLuid) else {
            return Vec::new();
        };
        let mut attributs = None;
        if MFCreateAttributes(&mut attributs, 1).is_err() {
            return Vec::new();
        }
        let Some(attributs) = attributs else {
            return Vec::new();
        };
        let octets = std::slice::from_raw_parts(
            &luid as *const LUID as *const u8,
            std::mem::size_of::<LUID>(),
        );
        if attributs.SetBlob(&MFT_ENUM_ADAPTER_LUID, octets).is_err() {
            return Vec::new();
        }
        let (mut liste, mut nombre) = (std::ptr::null_mut::<Option<IMFActivate>>(), 0u32);
        let enumere = MFTEnum2(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&sortie),
            &attributs,
            &mut liste,
            &mut nombre,
        );
        if enumere.is_err() || liste.is_null() {
            return Vec::new();
        }
        let trouves = (0..nombre as usize)
            .filter_map(|i| std::ptr::read(liste.add(i)))
            .collect();
        CoTaskMemFree(Some(liste as *const _));
        trouves
    }
}

fn nom_activation(activation: &IMFActivate) -> String {
    unsafe {
        let (mut nom, mut longueur) = (PWSTR::null(), 0u32);
        if activation
            .GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut nom, &mut longueur)
            .is_err()
            || nom.is_null()
        {
            return "encodeur matériel".into();
        }
        let lu = nom.to_string().unwrap_or_default();
        CoTaskMemFree(Some(nom.0 as *const _));
        lu
    }
}

fn capacite(codec: CodecVideo, adaptateur: &IDXGIAdapter1) -> Option<CapaciteEncodeur> {
    let activation = activations(codec, adaptateur).into_iter().next()?;
    let (defaut, cote) = match codec {
        CodecVideo::H264 => (MB_S_H264, 4096),
        CodecVideo::Hevc => (MB_S_HEVC, 8192),
    };
    let annonce = unsafe {
        let lu = activation
            .ActivateObject::<IMFTransform>()
            .ok()
            .and_then(|t| t.GetAttributes().ok())
            .and_then(|a| a.GetUINT32(&MF_VIDEO_MAX_MB_PER_SEC).ok());
        let _ = activation.ShutdownObject();
        lu
    };
    Some(CapaciteEncodeur {
        nom: nom_activation(&activation),
        mb_par_seconde: annonce.filter(|v| *v > 0).map_or(defaut, u64::from),
        largeur_max: cote,
        hauteur_max: cote,
    })
}

fn carte() -> Resultat<(
    IDXGIAdapter1,
    Option<CapaciteEncodeur>,
    Option<CapaciteEncodeur>,
)> {
    initialiser()?;
    adaptateurs()
        .into_iter()
        .find_map(|a| {
            let (h264, hevc) = (
                capacite(CodecVideo::H264, &a),
                capacite(CodecVideo::Hevc, &a),
            );
            (h264.is_some() || hevc.is_some()).then_some((a, h264, hevc))
        })
        .ok_or_else(|| Erreur::Echec(SANS_ENCODEUR.into()))
}

fn ecran_hz(fenetre: Option<u64>) -> u32 {
    unsafe {
        let ecran = match fenetre {
            Some(f) => MonitorFromWindow(HWND(f as isize as *mut _), MONITOR_DEFAULTTONEAREST),
            None => MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY),
        };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(ecran, &mut info as *mut MONITORINFOEXW as *mut _).as_bool() {
            return 60;
        }
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        let lu = EnumDisplaySettingsW(
            windows::core::PCWSTR(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
        )
        .as_bool();
        if lu && mode.dmDisplayFrequency > 1 {
            mode.dmDisplayFrequency
        } else {
            60
        }
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

pub(super) fn materiel(fenetre: Option<u64>) -> Resultat<MaterielEncodage> {
    let (adaptateur, h264, hevc) = carte()?;
    Ok(MaterielEncodage {
        carte: nom_adaptateur(&adaptateur),
        h264,
        hevc,
        ecran_hz: ecran_hz(fenetre),
        ram_libre: ram_libre(),
    })
}

unsafe fn regler(api: &ICodecAPI, cle: &GUID, valeur: Option<u32>) {
    let mut variante = VARIANT::default();
    let contenu = &mut *variante.Anonymous.Anonymous;
    match valeur {
        Some(v) => {
            contenu.vt = VT_UI4;
            contenu.Anonymous.ulVal = v;
        }
        None => {
            contenu.vt = VT_BOOL;
            contenu.Anonymous.boolVal = VARIANT_BOOL(-1);
        }
    }
    let _ = api.SetValue(cle, &variante);
}

unsafe fn type_propose(t: &IMFTransform, sortie: bool, format: &GUID) -> Option<IMFMediaType> {
    (0..64)
        .map_while(|i| {
            if sortie {
                t.GetOutputAvailableType(0, i)
            } else {
                t.GetInputAvailableType(0, i)
            }
            .ok()
        })
        .find(|m| m.GetGUID(&MF_MT_SUBTYPE).is_ok_and(|g| g == *format))
}

unsafe fn type_video(
    t: &IMFTransform,
    sortie: bool,
    format: &GUID,
    p: &ParametresVideo,
) -> windows::core::Result<IMFMediaType> {
    let m = match type_propose(t, sortie, format) {
        Some(m) => m,
        None => {
            let m = MFCreateMediaType()?;
            m.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            m.SetGUID(&MF_MT_SUBTYPE, format)?;
            m
        }
    };
    m.SetUINT64(
        &MF_MT_FRAME_SIZE,
        (p.largeur as u64) << 32 | p.hauteur as u64,
    )?;
    m.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    m.SetUINT64(&MF_MT_FRAME_RATE, (p.ips as u64) << 32 | 1)?;
    Ok(m)
}

unsafe fn configurer(
    activation: &IMFActivate,
    p: &ParametresVideo,
) -> windows::core::Result<(IMFTransform, IMFMediaEventGenerator)> {
    let t: IMFTransform = activation.ActivateObject()?;
    t.GetAttributes()?
        .SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
    if let Ok(api) = t.cast::<ICodecAPI>() {
        regler(
            &api,
            &CODECAPI_AVEncCommonMeanBitRate,
            Some(p.debit_kbps * 1000),
        );
        regler(
            &api,
            &CODECAPI_AVEncCommonRateControlMode,
            Some(eAVEncCommonRateControlMode_CBR.0 as u32),
        );
        regler(&api, &CODECAPI_AVEncMPVGOPSize, Some(p.images_par_groupe));
        regler(&api, &CODECAPI_AVEncMPVDefaultBPictureCount, Some(0));
        regler(&api, &CODECAPI_AVLowLatencyMode, None);
    }
    let (format, profil) = match p.codec {
        CodecVideo::H264 => (MFVideoFormat_H264, eAVEncH264VProfile_High.0),
        CodecVideo::Hevc => (MFVideoFormat_HEVC, eAVEncH265VProfile_Main_420_8.0),
    };
    let sortie = type_video(&t, true, &format, p)?;
    sortie.SetUINT32(&MF_MT_MPEG2_PROFILE, profil as u32)?;
    sortie.SetUINT32(&MF_MT_AVG_BITRATE, p.debit_kbps * 1000)?;
    t.SetOutputType(0, &sortie, 0)?;
    t.SetInputType(0, &type_video(&t, false, &MFVideoFormat_NV12, p)?, 0)?;
    let evenements = t.cast::<IMFMediaEventGenerator>()?;
    t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
    t.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
    Ok((t, evenements))
}

unsafe fn entete(t: &IMFTransform) -> Vec<u8> {
    let Ok(sortie) = t.GetOutputCurrentType(0) else {
        return Vec::new();
    };
    let taille = sortie.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).unwrap_or(0) as usize;
    let mut blob = vec![0u8; taille];
    if taille == 0
        || sortie
            .GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None)
            .is_err()
    {
        return Vec::new();
    }
    blob
}

unsafe fn octets(echantillon: &IMFSample) -> windows::core::Result<Vec<u8>> {
    let tampon = echantillon.ConvertToContiguousBuffer()?;
    let (mut pointeur, mut longueur) = (std::ptr::null_mut(), 0u32);
    tampon.Lock(&mut pointeur, None, Some(&mut longueur))?;
    let copie = std::slice::from_raw_parts(pointeur, longueur as usize).to_vec();
    tampon.Unlock()?;
    Ok(copie)
}

unsafe fn echantillon(donnees: &[u8], pts: i64, duree: i64) -> windows::core::Result<IMFSample> {
    let tampon = MFCreateMemoryBuffer(donnees.len() as u32)?;
    let mut pointeur = std::ptr::null_mut();
    tampon.Lock(&mut pointeur, None, None)?;
    std::ptr::copy_nonoverlapping(donnees.as_ptr(), pointeur, donnees.len());
    tampon.Unlock()?;
    tampon.SetCurrentLength(donnees.len() as u32)?;
    let e = MFCreateSample()?;
    e.AddBuffer(&tampon)?;
    e.SetSampleTime(pts)?;
    e.SetSampleDuration(duree)?;
    Ok(e)
}

struct Sortie {
    t: IMFTransform,
    parametres: ParametresVideo,
    entete: Vec<u8>,
    sortie: RecepteurPaquets,
    encodees: Arc<AtomicU64>,
}

impl Sortie {
    unsafe fn lire(&mut self) -> windows::core::Result<()> {
        let mut change = false;
        let resultat = loop {
            let mut sorties = [MFT_OUTPUT_DATA_BUFFER::default()];
            let flux = self.t.GetOutputStreamInfo(0)?;
            let fournit = (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0
                | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32;
            if flux.dwFlags & fournit == 0 {
                let place = flux
                    .cbSize
                    .max(self.parametres.largeur * self.parametres.hauteur * 2);
                let fourni = MFCreateSample()?;
                fourni.AddBuffer(&MFCreateMemoryBuffer(place)?)?;
                sorties[0].pSample = ManuallyDrop::new(Some(fourni));
            }
            let mut statut = 0u32;
            let resultat = self.t.ProcessOutput(0, &mut sorties, &mut statut);
            let [sortie] = sorties;
            let e = ManuallyDrop::into_inner(sortie.pSample);
            drop(ManuallyDrop::into_inner(sortie.pEvents));
            match resultat {
                Err(r) if r.code() == MF_E_TRANSFORM_STREAM_CHANGE && !change => {
                    let disponible = self.t.GetOutputAvailableType(0, 0)?;
                    self.t.SetOutputType(0, &disponible, 0)?;
                    self.entete = entete(&self.t);
                    change = true;
                }
                Err(r)
                    if change
                        && (r.code() == E_UNEXPECTED
                            || r.code() == MF_E_TRANSFORM_NEED_MORE_INPUT) =>
                {
                    return Ok(())
                }
                autre => {
                    autre?;
                    break e;
                }
            }
        };
        let Some(e) = resultat else {
            return Ok(());
        };
        let cle = e.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
        let pts = e.GetSampleTime()?;
        let duree = e.GetSampleDuration().unwrap_or(0);
        let brut = octets(&e)?;
        let donnees = if cle && !self.entete.is_empty() {
            [self.entete.as_slice(), &brut].concat()
        } else {
            brut
        };
        let duree = if duree > 0 {
            duree
        } else {
            UNITE_TEMPS / i64::from(self.parametres.ips.max(1))
        };
        self.encodees.fetch_add(1, Ordering::Relaxed);
        (self.sortie)(Paquet {
            pts,
            duree,
            cle,
            donnees,
        });
        Ok(())
    }
}

unsafe fn boucle(
    evenements: &IMFMediaEventGenerator,
    sortie: &mut Sortie,
    rx: &Receiver<Envoi<IMFSample>>,
) -> windows::core::Result<()> {
    let mut entrees = 0u64;
    loop {
        let evenement = evenements.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))?;
        evenement.GetStatus()?.ok()?;
        let genre = evenement.GetType()?;
        if genre == METransformNeedInput.0 as u32 {
            let Ok(e) = rx.recv().map(Envoi::prendre) else {
                return Ok(());
            };
            if entrees == 0 {
                e.SetUINT32(&MFSampleExtension_Discontinuity, 1)?;
                if let Ok(api) = sortie.t.cast::<ICodecAPI>() {
                    regler(&api, &CODECAPI_AVEncVideoForceKeyFrame, Some(1));
                }
            }
            sortie.t.ProcessInput(0, &e, 0)?;
            entrees += 1;
        } else if genre == METransformHaveOutput.0 as u32 {
            sortie.lire()?;
        }
    }
}

struct VideoMf {
    tx: SyncSender<Envoi<IMFSample>>,
    nom: String,
    encodees: Arc<AtomicU64>,
}

impl EncodeurVideo for VideoMf {
    fn nom(&self) -> &str {
        &self.nom
    }

    fn soumettre(&mut self, image: ImageNv12, pts: i64, duree: i64) -> Resultat<bool> {
        initialiser()?;
        let e = unsafe { echantillon(&image.donnees, pts, duree) }.map_err(echec)?;
        match self.tx.try_send(Envoi(e)) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => {
                Err(Erreur::Echec("l'encodeur matériel est arrêté".into()))
            }
        }
    }

    fn encodees(&self) -> u64 {
        self.encodees.load(Ordering::Relaxed)
    }
}

pub(super) fn video(
    parametres: ParametresVideo,
    sortie: RecepteurPaquets,
    arret: SignalArret,
) -> Resultat<Box<dyn EncodeurVideo>> {
    let (adaptateur, ..) = carte()?;
    let mut derniere = "aucun encodeur matériel pour ce format".to_string();
    for activation in activations(parametres.codec, &adaptateur) {
        match unsafe { configurer(&activation, &parametres) } {
            Ok((t, evenements)) => {
                let nom = nom_activation(&activation);
                let (tx, rx) = sync_channel::<Envoi<IMFSample>>(2);
                let encodees = Arc::new(AtomicU64::new(0));
                let compteur = encodees.clone();
                let fil = Envoi((t, evenements, activation, sortie));
                std::thread::spawn(move || {
                    let (t, evenements, activation, recepteur) = fil.prendre();
                    let _ = initialiser();
                    let mut sortie = Sortie {
                        entete: unsafe { entete(&t) },
                        t,
                        parametres,
                        sortie: recepteur,
                        encodees: compteur,
                    };
                    let resultat = unsafe { boucle(&evenements, &mut sortie, &rx) };
                    unsafe {
                        let _ = sortie.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
                        if let Ok(fermeture) = sortie.t.cast::<IMFShutdown>() {
                            let _ = fermeture.Shutdown();
                        }
                        let _ = activation.ShutdownObject();
                    }
                    if let Err(e) = resultat {
                        arret(format!("l'encodeur matériel s'est arrêté : {}", texte(e)));
                    }
                });
                return Ok(Box::new(VideoMf { tx, nom, encodees }));
            }
            Err(e) => {
                derniere = format!("{} : {}", nom_activation(&activation), texte(e));
                unsafe {
                    let _ = activation.ShutdownObject();
                }
            }
        }
    }
    Err(Erreur::Echec(format!(
        "session d'encodage matériel refusée ({derniere})"
    )))
}

struct AacMf {
    t: IMFTransform,
    parametres: ParametresAudio,
    sortie: RecepteurPaquets,
    attente: Vec<i16>,
    entres: u64,
    sortis: u64,
    taille_sortie: u32,
}

unsafe impl Send for AacMf {}

impl AacMf {
    fn temps(&self, echantillons: u64) -> i64 {
        (echantillons as i128 * UNITE_TEMPS as i128 / i128::from(self.parametres.frequence.max(1)))
            as i64
    }

    unsafe fn creer(p: ParametresAudio, sortie: RecepteurPaquets) -> windows::core::Result<Self> {
        let voulu = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Audio,
            guidSubtype: MFAudioFormat_AAC,
        };
        let (mut liste, mut nombre) = (std::ptr::null_mut::<Option<IMFActivate>>(), 0u32);
        MFTEnumEx(
            MFT_CATEGORY_AUDIO_ENCODER,
            MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&voulu),
            &mut liste,
            &mut nombre,
        )?;
        let trouves: Vec<IMFActivate> = if liste.is_null() {
            Vec::new()
        } else {
            (0..nombre as usize)
                .filter_map(|i| std::ptr::read(liste.add(i)))
                .collect()
        };
        CoTaskMemFree(Some(liste as *const _));
        let t: IMFTransform = trouves
            .first()
            .ok_or_else(windows::core::Error::empty)?
            .ActivateObject()?;
        let canaux = u32::from(p.canaux);
        let entree = MFCreateMediaType()?;
        entree.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        entree.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        entree.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        entree.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, p.frequence)?;
        entree.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, canaux)?;
        entree.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, canaux * 2)?;
        entree.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, p.frequence * canaux * 2)?;
        let voulue = MFCreateMediaType()?;
        voulue.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        voulue.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        voulue.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        voulue.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, p.frequence)?;
        voulue.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, canaux)?;
        voulue.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, p.debit_kbps * 125)?;
        voulue.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        voulue.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
        t.SetInputType(0, &entree, 0)?;
        t.SetOutputType(0, &voulue, 0)?;
        t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
        let taille_sortie = t.GetOutputStreamInfo(0)?.cbSize.max(8192);
        Ok(Self {
            t,
            parametres: p,
            sortie,
            attente: Vec::new(),
            entres: 0,
            sortis: 0,
            taille_sortie,
        })
    }

    unsafe fn coder(&mut self, pcm: &[i16]) -> windows::core::Result<()> {
        self.attente.extend_from_slice(pcm);
        let par_bloc = BLOC_AAC * usize::from(self.parametres.canaux.max(1));
        while self.attente.len() >= par_bloc {
            let bloc: Vec<u8> = self
                .attente
                .drain(..par_bloc)
                .flat_map(i16::to_le_bytes)
                .collect();
            let e = echantillon(&bloc, self.temps(self.entres), self.temps(BLOC_AAC as u64))?;
            self.entres += BLOC_AAC as u64;
            self.t.ProcessInput(0, &e, 0)?;
            self.vider()?;
        }
        Ok(())
    }

    unsafe fn vider(&mut self) -> windows::core::Result<()> {
        loop {
            let e = MFCreateSample()?;
            e.AddBuffer(&MFCreateMemoryBuffer(self.taille_sortie)?)?;
            let mut sorties = [MFT_OUTPUT_DATA_BUFFER {
                pSample: ManuallyDrop::new(Some(e)),
                ..Default::default()
            }];
            let mut statut = 0u32;
            let resultat = self.t.ProcessOutput(0, &mut sorties, &mut statut);
            let [sortie] = sorties;
            let e = ManuallyDrop::into_inner(sortie.pSample);
            drop(ManuallyDrop::into_inner(sortie.pEvents));
            match resultat {
                Err(r) if r.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                autre => autre?,
            }
            let Some(e) = e else {
                return Ok(());
            };
            let donnees = octets(&e)?;
            if !donnees.is_empty() {
                let pts = e
                    .GetSampleTime()
                    .unwrap_or_else(|_| self.temps(self.sortis));
                let duree = self.temps(BLOC_AAC as u64);
                self.sortis += BLOC_AAC as u64;
                (self.sortie)(Paquet {
                    pts,
                    duree,
                    cle: true,
                    donnees,
                });
            }
        }
    }
}

impl EncodeurAudio for AacMf {
    fn soumettre(&mut self, pcm: &[i16]) -> Resultat<()> {
        initialiser()?;
        unsafe { self.coder(pcm) }.map_err(echec)
    }
}

pub(super) fn audio(
    parametres: ParametresAudio,
    sortie: RecepteurPaquets,
) -> Resultat<Box<dyn EncodeurAudio>> {
    initialiser()?;
    let aac = unsafe { AacMf::creer(parametres, sortie) }
        .map_err(|e| Erreur::Echec(format!("encodeur AAC indisponible : {}", texte(e))))?;
    Ok(Box::new(aac))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_materiel_se_lit_sans_panique() {
        match materiel(None) {
            Ok(m) => assert!(m.ecran_hz > 0 && (m.h264.is_some() || m.hevc.is_some())),
            Err(e) => assert!(e.to_string().contains("encodeur")),
        }
    }
}
