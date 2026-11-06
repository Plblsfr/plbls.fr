//! Capture WASAPI : un thread par source, réveillé par l'événement du tampon, aucune attente active.
use bobyn_buffer::{PcmRing, CHANNELS, RATE};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use windows::core::{implement, w, Interface, Result, GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, BLOB, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::System::Com::STGM_READ;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, AvSetMmThreadCharacteristicsW, CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

#[derive(Clone, Debug)]
pub enum Source {
    /// Nom d'exécutable ; capte le processus racine et toute son arborescence (Discord répartit son audio sur plusieurs processus).
    App(String),
    /// Tout ce qui sort sur le périphérique de lecture par défaut.
    System,
    /// Comme `System`, sans le son d'Estompe lui-même (donc sans la voix des autres participants du vocal).
    SystemNoVoice,
    /// Seulement le son d'Estompe lui-même : la voix des autres participants du vocal.
    Voice,
    /// Périphérique par identifiant (None = défaut) ; avec `ptt` (code de touche virtuelle), le micro n'est enregistré que touche maintenue.
    Mic { device: Option<String>, ptt: Option<i32> },
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub source: Source,
    /// Nom de la piste dans le fichier.
    pub label: String,
    /// Volume linéaire (1.0 = inchangé).
    pub gain: f32,
}

impl Source {
    pub fn parse(s: &str) -> Option<Source> {
        match s {
            "system" => Some(Source::System),
            "system-novoice" => Some(Source::SystemNoVoice),
            "voice" => Some(Source::Voice),
            "mic" => Some(Source::Mic { device: None, ptt: None }),
            _ => s.strip_prefix("app:").filter(|n| !n.is_empty()).map(|n| Source::App(n.to_string())),
        }
    }

    /// Une source système contient déjà le son des applications ; `System` contient aussi le vocal.
    pub fn in_mix(&self, all: &[Source]) -> bool {
        let has = |f: fn(&Source) -> bool| all.iter().any(f);
        match self {
            Source::App(_) => !has(|s| matches!(s, Source::System | Source::SystemNoVoice)),
            Source::Voice => !has(|s| matches!(s, Source::System)),
            _ => true,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Source::App(n) => n.trim_end_matches(".exe").to_string(),
            Source::System | Source::SystemNoVoice => "Système".into(),
            Source::Voice => "Vocal".into(),
            Source::Mic { .. } => "Micro".into(),
        }
    }
}

pub struct Track {
    pub label: String,
    /// Faux si une source système couvre déjà cette piste : la sommer dans le Mix la doublerait (filtre en peigne, son « de bocal »).
    pub in_mix: bool,
    pub ring: Arc<Mutex<PcmRing>>,
}

impl Track {
    /// État du tampon pour le journal : (octets, amplitude maximale).
    pub fn stats(&self) -> (usize, u16) {
        let r = self.ring.lock().unwrap();
        (r.bytes(), r.peak())
    }
}

pub struct Audio {
    pub tracks: Vec<Track>,
    /// Sources qui n'ont pas pu démarrer (appli absente, périphérique occupé) ; les autres tournent.
    pub warnings: Vec<String>,
    stop: isize, // HANDLE d'un événement manuel, partagé avec les threads
    threads: Vec<JoinHandle<()>>,
}

impl Audio {
    /// Démarre une source par thread ; une source en échec est signalée dans `warnings` sans empêcher les autres.
    pub fn start(specs: &[Spec], span_us: i64) -> std::result::Result<Self, String> {
        let stop = unsafe { CreateEventW(None, true, false, None) }.map_err(|e| e.to_string())?.0 as isize;
        let mut me = Self { tracks: Vec::new(), warnings: Vec::new(), stop, threads: Vec::new() };
        let sources: Vec<Source> = specs.iter().map(|s| s.source.clone()).collect();
        for spec in specs {
            let ring = Arc::new(Mutex::new(PcmRing::new(span_us)));
            let (tx, rx) = mpsc::channel();
            let (sp, r) = (spec.clone(), ring.clone());
            me.threads.push(std::thread::spawn(move || run(sp, stop, r, tx)));
            match rx.recv().map_err(|e| e.to_string())? {
                Ok(()) => me.tracks.push(Track { label: spec.label.clone(), in_mix: spec.source.in_mix(&sources), ring }),
                Err(e) => me.warnings.push(e),
            }
        }
        Ok(me)
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        unsafe {
            let h = HANDLE(self.stop as *mut _);
            let _ = SetEvent(h);
            for t in self.threads.drain(..) {
                let _ = t.join();
            }
            let _ = CloseHandle(h);
        }
    }
}

/// Processus racine portant ce nom : celui dont le parent n'a pas le même nom.
fn root_pid(exe: &str) -> Option<u32> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut all = Vec::new();
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let n = &e.szExeFile[..e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len())];
            all.push((String::from_utf16_lossy(n), e.th32ProcessID, e.th32ParentProcessID));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
        let same: Vec<_> = all.iter().filter(|p| p.0.eq_ignore_ascii_case(exe)).collect();
        same.iter().find(|p| !same.iter().any(|q| q.1 == p.2)).map(|p| p.1)
    }
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Done(mpsc::Sender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Done_Impl {
    fn ActivateCompleted(&self, _op: windows::core::Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
        let _ = self.0.send(());
        Ok(())
    }
}

unsafe fn process_client(pid: u32, mode: PROCESS_LOOPBACK_MODE) -> Result<IAudioClient> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: mode },
        },
    };
    // ManuallyDrop : PropVariantClear libérerait le blob, qui est sur la pile.
    let pv = std::mem::ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { blob: BLOB { cbSize: std::mem::size_of_val(&params) as u32, pBlobData: &mut params as *mut _ as *mut u8 } },
            }),
        },
    });
    let (tx, rx) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Done(tx).into();
    let op = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*pv), &handler)?;
    rx.recv().map_err(|_| windows::core::Error::new(E_FAIL, "activation audio sans réponse"))?;
    let (mut hr, mut unk) = (windows::core::HRESULT(0), None);
    op.GetActivateResult(&mut hr, &mut unk)?;
    hr.ok()?;
    unk.ok_or_else(|| windows::core::Error::new(E_FAIL, "activation audio vide"))?.cast()
}

unsafe fn endpoint_client(flow: EDataFlow, id: Option<&str>) -> Result<IAudioClient> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let dev = match id {
        Some(id) => en.GetDevice(&windows::core::HSTRING::from(id))?,
        None => en.GetDefaultAudioEndpoint(flow, eConsole)?,
    };
    dev.Activate(CLSCTX_ALL, None)
}

/// Exécutable (nom de fichier) d'un processus.
pub fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        ok.then(|| path.rsplit('\\').next().unwrap_or(&path).to_string())
    }
}

/// Applications qui ont une session audio sur le périphérique de lecture par défaut (celles qui émettent ou ont émis du son).
pub fn sessions() -> Vec<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let list = (|| -> Result<Vec<String>> {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let mgr: IAudioSessionManager2 = en.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)?;
            let sessions = mgr.GetSessionEnumerator()?;
            let mut out = Vec::new();
            for i in 0..sessions.GetCount()? {
                let pid = sessions.GetSession(i)?.cast::<IAudioSessionControl2>()?.GetProcessId()?;
                if let Some(n) = (pid != 0).then(|| exe_of(pid)).flatten() {
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
            Ok(out)
        })();
        list.unwrap_or_default()
    }
}

/// Microphones actifs : (identifiant, nom affiché).
pub fn mics() -> Vec<(String, String)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let list = (|| -> Result<Vec<(String, String)>> {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let devs = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let mut out = Vec::new();
            for i in 0..devs.GetCount()? {
                let d = devs.Item(i)?;
                let id = d.GetId()?.to_string()?;
                let name = d.OpenPropertyStore(STGM_READ)?.GetValue(&PKEY_Device_FriendlyName).map(|v| v.to_string()).unwrap_or_else(|_| id.clone());
                out.push((id, name));
            }
            Ok(out)
        })();
        list.unwrap_or_default()
    }
}

/// Identifiant du périphérique par défaut (pour détecter un changement de sortie ou d'entrée en cours d'enregistrement).
unsafe fn default_id(flow: EDataFlow) -> Option<String> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
    en.GetDefaultAudioEndpoint(flow, eConsole).ok()?.GetId().ok()?.to_string().ok()
}

/// Ouvre la source ; renvoie aussi le périphérique par défaut suivi (None : source liée à un processus ou à un périphérique choisi).
unsafe fn open(src: &Source) -> Result<(IAudioClient, IAudioCaptureClient, HANDLE, Option<(EDataFlow, String)>)> {
    let (client, flags, tracked) = match src {
        Source::App(name) => {
            let pid = root_pid(name).ok_or_else(|| windows::core::Error::new(E_FAIL, format!("{name} ne tourne pas")))?;
            (process_client(pid, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE)?, AUDCLNT_STREAMFLAGS_LOOPBACK, None)
        }
        Source::SystemNoVoice => (process_client(std::process::id(), PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE)?, AUDCLNT_STREAMFLAGS_LOOPBACK, None),
        Source::Voice => (process_client(std::process::id(), PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE)?, AUDCLNT_STREAMFLAGS_LOOPBACK, None),
        Source::System => (endpoint_client(eRender, None)?, AUDCLNT_STREAMFLAGS_LOOPBACK, default_id(eRender).map(|i| (eRender, i))),
        Source::Mic { device, .. } => (endpoint_client(eCapture, device.as_deref())?, 0, device.is_none().then(|| default_id(eCapture)).flatten().map(|i| (eCapture, i))),
    };
    // Format imposé à toutes les sources ; le moteur audio convertit (fréquence, canaux).
    let fmt = WAVEFORMATEX {
        wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
        nChannels: CHANNELS as u16,
        nSamplesPerSec: RATE as u32,
        nAvgBytesPerSec: RATE as u32 * CHANNELS as u32 * 4,
        nBlockAlign: CHANNELS as u16 * 4,
        wBitsPerSample: 32,
        cbSize: 0,
    };
    let event = CreateEventW(None, false, false, None)?;
    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        flags | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        2_000_000,
        0,
        &fmt,
        None::<*const GUID>,
    )?;
    client.SetEventHandle(event)?;
    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    Ok((client, capture, event, tracked))
}

fn now_100ns() -> i64 {
    let (mut c, mut f) = (0, 0);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    (c as i128 * 10_000_000 / f.max(1) as i128) as i64
}

/// Intervalle de vérification du périphérique : une source silencieuse (boucle système) ne réveille pas le thread, il faut sonder.
const PROBE_MS: u32 = 2000;

fn run(spec: Spec, stop: isize, ring: Arc<Mutex<PcmRing>>, ready: mpsc::Sender<std::result::Result<(), String>>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut task = 0;
        let _ = AvSetMmThreadCharacteristicsW(w!("Audio"), &mut task);
        let src = spec.source.clone();
        let ptt = match &src {
            Source::Mic { ptt, .. } => *ptt,
            _ => None,
        };
        let mut pcm: Vec<i16> = Vec::new();
        let mut first = true;
        let label = src.label();
        // Compteurs sur 10 s : paquets lus, images, paquets marqués silencieux ou avec trou, amplitude max.
        let (mut packets, mut frames_n, mut silent, mut glitch, mut peak) = (0u64, 0u64, 0u64, 0u64, 0i16);
        let mut last_log = std::time::Instant::now();
        // Périphérique débranché ou changé : on rouvre la source (le trou est comblé par du silence au rendu).
        loop {
            let (client, capture, event, tracked) = match open(&src) {
                Ok(v) => v,
                Err(e) => {
                    crate::log(&format!("{label} : ouverture impossible ({e}){}", if first { "" } else { ", nouvel essai" }));
                    if first {
                        let _ = ready.send(Err(format!("{} : {e}", src.label())));
                        return;
                    }
                    if WaitForSingleObject(HANDLE(stop as *mut _), 1000) != WAIT_TIMEOUT {
                        return;
                    }
                    continue;
                }
            };
            crate::log(&format!("{label} : source ouverte{}", if first { "" } else { " de nouveau" }));
            if std::mem::take(&mut first) {
                let _ = ready.send(Ok(()));
            }
            let waits = [event, HANDLE(stop as *mut _)];
            let mut stopped = false;
            'capture: loop {
                let w = WaitForMultipleObjects(&waits, false, PROBE_MS);
                if w != WAIT_OBJECT_0 && w != WAIT_TIMEOUT {
                    stopped = true;
                    break;
                }
                if w == WAIT_TIMEOUT && tracked.as_ref().is_some_and(|(flow, id)| default_id(*flow).as_ref() != Some(id)) {
                    break;
                }
                loop {
                    match capture.GetNextPacketSize() {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(_) => break 'capture,
                    }
                    let (mut data, mut frames, mut flags, mut qpc) = (std::ptr::null_mut(), 0u32, 0u32, 0u64);
                    if capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc)).is_err() {
                        break 'capture;
                    }
                    pcm.clear();
                    let n = frames as usize * CHANNELS;
                    // Push-to-talk : touche relâchée = silence (la piste garde sa durée).
                    let muted = ptt.map_or(false, |vk| GetAsyncKeyState(vk) as u16 & 0x8000 == 0);
                    if muted || flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                        pcm.resize(n, 0);
                    } else {
                        let f = std::slice::from_raw_parts(data as *const f32, n);
                        pcm.extend(f.iter().map(|x| ((x * spec.gain).clamp(-1.0, 1.0) * 32767.0) as i16));
                    }
                    packets += 1;
                    frames_n += frames as u64;
                    silent += (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0) as u64;
                    glitch += (flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0) as u64;
                    peak = pcm.iter().fold(peak, |m, x| m.max(x.saturating_abs()));
                    let _ = capture.ReleaseBuffer(frames);
                    // Position QPC en 100 ns -> µs, même base que la vidéo ; absente (0) sur certains flux : heure courante, sinon le morceau tombe avant la fenêtre et le rendu est muet.
                    let at = if qpc == 0 { now_100ns() } else { qpc as i64 };
                    ring.lock().unwrap().push(at / 10, &pcm);
                }
                if last_log.elapsed() >= std::time::Duration::from_secs(10) {
                    crate::log(&format!("{label} : {packets} paquets, {frames_n} images, {silent} silencieux, {glitch} avec trou, amplitude max {peak}"));
                    (packets, frames_n, silent, glitch, peak, last_log) = (0, 0, 0, 0, 0, std::time::Instant::now());
                }
            }
            crate::log(&format!("{label} : {}", if stopped { "arrêt" } else { "flux interrompu ou périphérique changé, réouverture" }));
            let _ = client.Stop();
            let _ = CloseHandle(event);
            if stopped {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_sans_doublon() {
        let app = Source::App("jeu.exe".into());
        let mic = Source::Mic { device: None, ptt: None };
        let all = [app.clone(), Source::System, Source::Voice, mic.clone()];
        assert!(!app.in_mix(&all) && !Source::Voice.in_mix(&all));
        assert!(Source::System.in_mix(&all) && mic.in_mix(&all));
        let all = [app.clone(), Source::SystemNoVoice, Source::Voice];
        assert!(!app.in_mix(&all) && Source::Voice.in_mix(&all));
        assert!(app.in_mix(&[app.clone(), Source::Voice]));
    }
}
