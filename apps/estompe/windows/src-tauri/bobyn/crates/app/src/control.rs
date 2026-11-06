//! Chef d'orchestre : décide quoi capturer et quand (jeu détecté, fenêtre réduite, fermeture), démarre et arrête la vidéo et l'audio.
//! Tout se passe sur le thread des messages ; les événements Windows y arrivent déjà triés (voir tray.rs).
use crate::config::{parse_key, Config, Effective};
use crate::detect;
use crate::markers::{self, Mark};
use crate::pipeline::{End, Params, Pipeline, Target};
use crate::save::{self, Pcm};
use bobyn_audio::{Audio, Source, Spec};
use bobyn_buffer::Ring;
use bobyn_capture::clock::now_100ns;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW};

pub const WM_PIPE_ENDED: u32 = 0x8000 + 2;
pub const WM_SAVED: u32 = 0x8000 + 3;

struct Session {
    name: String,
    exe: Option<String>,
    pid: u32,
    /// Fenêtre suivie (pause à la réduction, fin à la fermeture) ; None pour un écran.
    hwnd: Option<isize>,
    target: Target,
    fx: Effective,
    ring: Arc<Mutex<Ring>>,
    dims: (u32, u32),
    pipe: Option<Pipeline>,
    audio: Audio,
    note: Option<String>,
    marks: Vec<Mark>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Recording(String),
    Paused(String),
    Error(String),
}

pub struct Controller {
    pub cfg: Config,
    wake: isize,
    session: Option<Session>,
    error: Option<String>,
}

fn primary_monitor() -> Option<isize> {
    detect::monitors().first().map(|m| m.handle)
}

impl Controller {
    pub fn new(cfg: Config, wake: HWND) -> Self {
        Self { cfg, wake: wake.0 as isize, session: None, error: None }
    }

    pub fn state(&self) -> State {
        match (&self.session, &self.error) {
            (_, Some(e)) => State::Error(e.clone()),
            (None, _) => State::Idle,
            (Some(s), _) if s.pipe.is_none() => State::Paused(s.name.clone()),
            (Some(s), _) => State::Recording(s.name.clone()),
        }
    }

    pub fn tooltip(&self) -> String {
        let t = match self.state() {
            State::Idle => "Bobyn : en attente d'un jeu".to_string(),
            State::Recording(n) => format!("Bobyn : buffer de {n}"),
            State::Paused(n) => format!("Bobyn : en pause ({n} réduit)"),
            State::Error(e) => format!("Bobyn : {e}"),
        };
        match self.session.as_ref().and_then(|s| s.note.as_deref()) {
            Some(n) => format!("{t} — {n}"),
            None => t,
        }
    }

    /// Fenêtre suivie par la session (pour les hooks), 0 si aucune.
    pub fn tracked(&self) -> isize {
        self.session.as_ref().and_then(|s| s.hwnd).unwrap_or(0)
    }

    /// (Re)démarre selon le mode des réglages.
    pub fn begin(&mut self) {
        self.stop();
        self.error = None;
        match self.cfg.mode.as_str() {
            "screen" => {
                let mons = detect::monitors();
                let m = mons.get(self.cfg.screen).or(mons.first());
                match m {
                    Some(m) => self.start(format!("écran {}", m.name.trim_start_matches("\\\\.\\")), None, 0, None, Target::Monitor(m.handle)),
                    None => self.error = Some("aucun écran".into()),
                }
            }
            "window" => self.try_window(),
            _ => {
                if self.cfg.always_on {
                    self.start_idle_screen();
                }
                self.on_foreground(unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() });
            }
        }
    }

    fn start_idle_screen(&mut self) {
        if let Some(m) = primary_monitor() {
            self.start("écran principal".into(), None, 0, None, Target::Monitor(m));
        }
    }

    fn try_window(&mut self) {
        if self.session.is_some() {
            return;
        }
        if let Some(h) = detect::find_window(&self.cfg.window) {
            let w = detect::hwnd(h);
            let exe = self.cfg.window.clone();
            self.start(exe.trim_end_matches(".exe").to_string(), Some(exe), detect::pid_of(w), Some(h), Target::Window(h));
        }
    }

    pub fn on_foreground(&mut self, h: HWND) {
        self.check_alive();
        match self.cfg.mode.as_str() {
            "window" => self.try_window(),
            "game" => {
                let Some(g) = detect::classify(h, &self.cfg.games) else { return };
                if self.session.as_ref().is_some_and(|s| s.pid == g.pid && s.hwnd.is_some()) {
                    return;
                }
                self.start(g.exe.trim_end_matches(".exe").to_string(), Some(g.exe), g.pid, Some(g.hwnd), Target::Window(g.hwnd));
            }
            _ => {}
        }
    }

    pub fn on_minimize(&mut self, h: HWND, minimized: bool) {
        let Some(s) = self.session.as_mut().filter(|s| s.hwnd == Some(h.0 as isize)) else { return };
        if minimized {
            s.pipe = None; // pause : pas de capture de noir ; le buffer déjà rempli reste sauvegardable
        } else if s.pipe.is_none() {
            self.restart_pipe();
        }
    }

    /// Fenêtre suivie disparue (jeu fermé) : fin de session. Pas de hook de destruction, qui réveillerait le processus à chaque objet détruit.
    fn check_alive(&mut self) {
        if self.session.as_ref().and_then(|s| s.hwnd).is_some_and(|h| !unsafe { IsWindow(Some(detect::hwnd(h))) }.as_bool()) {
            self.stop();
            if self.cfg.mode == "game" && self.cfg.always_on {
                self.start_idle_screen();
            }
        }
    }

    pub fn on_pipe_ended(&mut self) {
        let Some(s) = self.session.as_mut() else { return };
        let end = s.pipe.as_ref().and_then(|p| p.ended.lock().unwrap().clone());
        match end {
            Some(End::Lost) => {
                self.check_alive();
                if self.session.is_some() {
                    self.restart_pipe();
                }
            }
            Some(End::NoFrames) => {
                // Repli : l'écran qui porte la fenêtre.
                if let Some(h) = s.hwnd {
                    let m = unsafe { MonitorFromWindow(detect::hwnd(h), MONITOR_DEFAULTTONEAREST) };
                    s.target = Target::Monitor(m.0 as isize);
                    s.note = Some("capture de fenêtre impossible, écran entier".into());
                    self.restart_pipe();
                }
            }
            Some(End::Error(e)) => {
                s.pipe = None;
                self.error = Some(e);
            }
            None => {}
        }
    }

    fn restart_pipe(&mut self) {
        let wake = self.wake;
        let Some(s) = self.session.as_mut() else { return };
        s.pipe = None;
        let params = Params { target: s.target, codec: s.fx.codec, fps: s.fx.fps, mbps: s.fx.mbps, cursor: s.fx.cursor, height: s.fx.height };
        match Pipeline::start(params, s.ring.clone(), move || unsafe {
            let _ = PostMessageW(Some(HWND(wake as *mut _)), WM_PIPE_ENDED, WPARAM(0), LPARAM(0));
        }) {
            Ok(p) => {
                // Taille changée (fenêtre redimensionnée) : l'ancien contenu ne se muxerait pas avec le nouveau.
                if s.dims != (0, 0) && s.dims != (p.width, p.height) {
                    *s.ring.lock().unwrap() = Ring::new(s.fx.seconds as i64 * 1_000_000);
                }
                s.dims = (p.width, p.height);
                s.pipe = Some(p);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn specs(&self, fx: &Effective, exe: Option<&str>) -> Vec<Spec> {
        let mut out: Vec<Spec> = Vec::new();
        for a in &fx.audio {
            let mut label = None;
            let source = match a.source.as_str() {
                // Pas de jeu (mode écran, buffer permanent) : tout le son du système plutôt que le silence.
                "game" => {
                    label = exe.map(|_| "Jeu".to_string());
                    exe.map_or(Source::System, |e| Source::App(e.to_string()))
                }
                "system" => Source::System,
                "mic" => Source::Mic { device: a.device.clone(), ptt: a.ptt.as_deref().and_then(parse_key).map(|k| k as i32) },
                s => match Source::parse(s) {
                    Some(s) => s,
                    None => continue,
                },
            };
            let same = |x: &Source| format!("{x:?}").eq_ignore_ascii_case(&format!("{source:?}"));
            if !out.iter().any(|o| same(&o.source)) {
                out.push(Spec { label: label.unwrap_or_else(|| source.label()), source, gain: a.gain });
            }
        }
        out
    }

    fn start(&mut self, name: String, exe: Option<String>, pid: u32, hwnd: Option<isize>, target: Target) {
        self.stop();
        self.error = None;
        let mut fx = self.cfg.effective(exe.as_deref(), hwnd.is_some() && self.cfg.mode == "game");
        if fx.fps == 0 {
            fx.fps = detect::refresh_rate();
        }
        let span = fx.seconds as i64 * 1_000_000;
        let audio = match Audio::start(&self.specs(&fx, exe.as_deref()), span) {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        for t in &audio.tracks {
            crate::log(&format!("audio : piste « {} » ouverte", t.label));
        }
        for w in &audio.warnings {
            crate::log(&format!("audio : source non ouverte, {w}"));
        }
        let note = (!audio.warnings.is_empty()).then(|| audio.warnings.join(" ; "));
        self.session = Some(Session { name, exe, pid, hwnd, target, fx, ring: Arc::new(Mutex::new(Ring::new(span))), dims: (0, 0), pipe: None, audio, note, marks: Vec::new() });
        self.restart_pipe();
        if self.error.is_some() {
            self.session = None; // rien ne tourne : pas de session fantôme qui garderait l'audio en RAM
        }
    }

    /// Arrête tout et libère la RAM (buffer vidéo et audio compris).
    pub fn stop(&mut self) {
        self.session = None;
    }

    /// Pose un marqueur à l'instant présent ; renvoie son rang parmi ceux encore dans le tampon.
    pub fn mark(&mut self) -> Result<usize, String> {
        let s = self.session.as_mut().ok_or("aucun buffer actif")?;
        if s.pipe.is_none() {
            return Err("enregistrement en pause".into());
        }
        let t = unsafe { GetLocalTime() };
        s.marks.push(Mark { pts_us: now_100ns() / 10, clock: format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond) });
        if let Some(oldest) = s.ring.lock().unwrap().oldest_pts() {
            markers::prune(&mut s.marks, oldest);
        }
        Ok(s.marks.len())
    }

    /// Sauvegarde le buffer (`last` = seulement les `last_seconds` dernières secondes) ; renvoie le chemin, écrit en arrière-plan.
    pub fn save(&mut self, last: bool) -> Result<PathBuf, String> {
        let s = self.session.as_ref().ok_or("aucun buffer actif")?;
        let ring = s.ring.lock().unwrap();
        let packets = if last { ring.snapshot_last(self.cfg.last_seconds as i64 * 1_000_000) } else { ring.snapshot() };
        drop(ring);
        let (first, lastp) = match (packets.first(), packets.last()) {
            (Some(a), Some(b)) => (a.pts_us, b.pts_us),
            _ => return Err("buffer vide".into()),
        };
        let end = lastp + 1_000_000 / s.fx.fps as i64;
        let pcm: Vec<Pcm> = s.audio.tracks.iter().map(|t| Pcm { name: t.label.clone(), in_mix: t.in_mix, samples: t.ring.lock().unwrap().render(first, end) }).collect();
        for (t, p) in s.audio.tracks.iter().zip(&pcm) {
            let (bytes, peak) = t.stats();
            crate::log(&format!("audio : piste « {} », tampon {} Ko (amplitude max {peak}), rendu {} échantillons, max {}", t.label, bytes >> 10, p.samples.len(), p.samples.iter().map(|x| x.unsigned_abs()).max().unwrap_or(0)));
        }
        if pcm.is_empty() {
            crate::log("audio : aucune piste ouverte, replay sans son");
        }
        let markers = markers::timeline(&s.marks, first, end);
        let path = save::default_path(&self.cfg.save_dir(), s.exe.as_deref().map(|e| e.trim_end_matches(".exe")).unwrap_or(&s.name));
        let (w, h, codec, wake, p2) = (s.dims.0, s.dims.1, s.fx.codec, self.wake, path.clone());
        // Le buffer continue pendant l'écriture : seule une copie de références est passée au thread.
        std::thread::spawn(move || {
            let msg = match save::save(&p2, w, h, codec, &packets, pcm, &markers) {
                Ok(()) => format!("ok|{}", p2.display()),
                Err(e) => format!("err|{e}"),
            };
            unsafe {
                let _ = PostMessageW(Some(HWND(wake as *mut _)), WM_SAVED, WPARAM(0), LPARAM(Box::into_raw(Box::new(msg)) as isize));
            }
        });
        Ok(path)
    }
}
