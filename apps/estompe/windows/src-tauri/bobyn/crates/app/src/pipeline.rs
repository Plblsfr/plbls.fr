//! Chaîne vidéo : capture -> conversion NV12 -> encodage -> buffer, à cadence fixe ; les pts sont l'heure QPC en µs, base commune avec l'audio.
use bobyn_buffer::{Codec, Pacer, Ring};
use bobyn_capture::{clock::now_100ns, convert::Converter, gpu, wgc::Capture};
use bobyn_encode::Encoder;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

/// Ce qu'on capture (HWND / HMONITOR en entier pour traverser les threads).
#[derive(Clone, Copy, Debug)]
pub enum Target {
    Window(isize),
    Monitor(isize),
}

/// Pourquoi la chaîne s'est arrêtée d'elle-même.
#[derive(Clone, Debug)]
pub enum End {
    /// Fenêtre fermée ou redimensionnée : à recréer.
    Lost,
    /// Aucune image reçue : cette fenêtre n'est pas capturable par WGC.
    NoFrames,
    Error(String),
}

pub struct Pipeline {
    pub ended: Arc<Mutex<Option<End>>>,
    pub width: u32,
    pub height: u32,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub struct Params {
    pub target: Target,
    pub codec: Codec,
    pub fps: u32,
    pub mbps: u32,
    pub cursor: bool,
    pub height: Option<u32>,
}

impl Pipeline {
    /// Démarre la capture dans `ring` ; `on_end` est appelé (depuis le thread de la chaîne) si elle s'arrête seule.
    /// Renvoie l'erreur d'initialisation du GPU, de la capture ou de l'encodeur.
    pub fn start(p: Params, ring: Arc<Mutex<Ring>>, on_end: impl Fn() + Send + 'static) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let ended = Arc::new(Mutex::new(None));
        let (r, s, en) = (ring.clone(), stop.clone(), ended.clone());
        // Tout est créé dans le thread : les objets COM ne traversent pas les threads.
        let thread = std::thread::spawn(move || {
            // Réveils à 1 ms au lieu de ~15 ms ; priorité normale : le thread ne doit jamais passer devant ceux du jeu (EST-84).
            unsafe { timeBeginPeriod(1) };
            let init = (|| -> windows::core::Result<_> {
                unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
                let gpu = gpu::open()?;
                let half = 5_000_000 / p.fps as i64;
                let cap = match p.target {
                    Target::Window(h) => Capture::window(&gpu.device, crate::detect::hwnd(h), p.cursor, half)?,
                    Target::Monitor(h) => Capture::monitor(&gpu.device, HMONITOR(h as *mut _), p.cursor, half)?,
                };
                // NV12 : dimensions paires ; la mise à l'échelle se fait sur le GPU.
                let h = p.height.map_or(cap.height, |h| h.min(cap.height)) & !1;
                let w = (cap.width as u64 * h as u64 / cap.height as u64) as u32 & !1;
                let conv = Converter::new(&gpu.device, &gpu.context, (cap.width, cap.height), (w, h))?;
                let enc = bobyn_encode::open(&gpu.device, p.codec, w, h, p.fps, p.mbps, 1)
                    .map_err(|e| windows::core::Error::new(e.code(), format!("codec {} indisponible sur ce GPU : {e}", p.codec.label())))?;
                Ok((cap, conv, enc, w, h, gpu.context.clone()))
            })();
            match init {
                Ok((cap, conv, enc, w, h, ctx)) => {
                    let _ = tx.send(Ok((w, h)));
                    let window = matches!(p.target, Target::Window(_));
                    if let Some(end) = run(cap, conv, enc, ctx, p.fps, window, &r, &s) {
                        *en.lock().unwrap() = Some(end);
                        on_end();
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
            unsafe { timeEndPeriod(1) };
        });
        match rx.recv().map_err(|e| e.to_string())? {
            Ok((width, height)) => Ok(Self { ended, width, height, stop, thread: Some(thread) }),
            Err(e) => {
                let _ = thread.join();
                Err(e)
            }
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Renvoie la cause d'un arrêt spontané (None si l'arrêt a été demandé).
#[allow(clippy::too_many_arguments)]
fn run(cap: Capture, mut conv: Converter, mut enc: Box<dyn Encoder>, ctx: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext, fps: u32, window: bool, ring: &Mutex<Ring>, stop: &AtomicBool) -> Option<End> {
    let interval = 10_000_000 / fps as i64;
    let t0 = now_100ns();
    let mut pacer = Pacer::new(interval, t0);
    let mut latest = None;
    let (mut encoded, mut last_log, mut last_dropped) = (0u64, Instant::now(), 0u64);
    let (mut busy, mut worst) = (Duration::ZERO, Duration::ZERO);
    while !stop.load(Relaxed) {
        if cap.lost() {
            return Some(End::Lost);
        }
        // Une fenêtre sans aucune image après 3 s n'est pas capturable : l'appelant se replie sur l'écran.
        if window && latest.is_none() && now_100ns() - t0 > 30_000_000 {
            return Some(End::NoFrames);
        }
        let wait = (pacer.due() - now_100ns()).max(0);
        // Réveil par événement (image arrivée) ou par l'échéance du prochain tick, jamais d'attente active.
        if let Some(f) = cap.next_frame(Duration::from_nanos(wait as u64 * 100).min(Duration::from_millis(100))) {
            latest = Some(f); // seule la dernière image de l'intervalle est encodée
        }
        let now = now_100ns();
        if now < pacer.due() {
            continue;
        }
        let pts = pacer.emit(now);
        let Some(f) = &latest else { continue };
        let tex = f.texture.clone();
        let work = Instant::now();
        // Image statique : la dernière texture est réencodée sans recapture.
        let packets = conv.convert(&tex).and_then(|nv12| {
            unsafe { ctx.Flush() }; // pousse la conversion au GPU avant que l'encodeur lise la texture
            enc.encode(&nv12, pts / 10)
        });
        match packets {
            Ok(ps) => {
                let mut ring = ring.lock().unwrap();
                for p in ps {
                    ring.push(p);
                }
            }
            Err(e) => return Some(End::Error(e.to_string())),
        }
        let took = work.elapsed();
        (busy, worst) = (busy + took, worst.max(took));
        encoded += 1;
        if last_log.elapsed() >= Duration::from_secs(10) {
            let dropped = enc.dropped();
            crate::log(&format!(
                "encodeur {}, {encoded} images soumises, {} perdues ({dropped} au total), conversion+encodage {:.2} ms en moyenne, {:.2} ms au pire, tampon {} Mo, {}",
                enc.name(),
                dropped - last_dropped,
                busy.as_secs_f64() * 1000.0 / encoded.max(1) as f64,
                worst.as_secs_f64() * 1000.0,
                ring.lock().unwrap().bytes() >> 20,
                process_stats(),
            ));
            (encoded, last_log, last_dropped, busy, worst) = (0, Instant::now(), dropped, Duration::ZERO, Duration::ZERO);
        }
    }
    None
}

/// Mémoire et handles du processus, pour repérer une fuite sur une longue session.
fn process_stats() -> String {
    unsafe {
        let process = GetCurrentProcess();
        let mut mem = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
        let _ = GetProcessMemoryInfo(process, &mut mem, mem.cb);
        let mut handles = 0;
        let _ = GetProcessHandleCount(process, &mut handles);
        format!("mémoire {} Mo (pic {} Mo), {handles} handles", mem.WorkingSetSize >> 20, mem.PeakWorkingSetSize >> 20)
    }
}
