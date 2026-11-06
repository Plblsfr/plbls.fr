//! Service Bobyn : fenêtre cachée sur son propre thread, raccourcis globaux et hooks d'événements (premier plan, réduction).
//! Le thread dort dans GetMessage ; rien ne le réveille hors événement. Estompe le pilote par `Service` et reçoit des `Event`.
use crate::config::{parse_hotkey, Config};
use crate::control::{Controller, State, WM_PIPE_ENDED, WM_SAVED};
use serde::Serialize;
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_HOOK: u32 = 0x8000 + 1;
const WM_APPLY: u32 = 0x8000 + 4;
const WM_SAVE: u32 = 0x8000 + 5;
const WM_STOP: u32 = 0x8000 + 6;
const EVENT_FOREGROUND: u32 = 0x0003;
const EVENT_MINIMIZESTART: u32 = 0x0016;
const EVENT_MINIMIZEEND: u32 = 0x0017;
const TIMER_RECHECK: usize = 1;
/// Revérifications après un changement de premier plan : un jeu passe en plein écran après avoir créé sa fenêtre.
const RECHECK_MS: [u32; 3] = [300, 1000, 3000];

static WND: AtomicIsize = AtomicIsize::new(0);
static TRACKED: AtomicIsize = AtomicIsize::new(0);

/// État affiché dans l'onglet : `state` vaut idle, recording, paused ou error.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Status {
    pub state: &'static str,
    pub text: String,
}

pub enum Event {
    Status(Status),
    /// Message à montrer à l'utilisateur (réglages `notify`).
    Notice { title: String, text: String },
    /// Replay écrit (nom du fichier), indépendamment du réglage `notify`.
    Saved(String),
    /// Marqueur posé (rang parmi ceux du tampon).
    Marker(usize),
}

struct App {
    ctrl: Controller,
    hwnd: HWND,
    recheck: usize,
    shown: Option<Status>,
    status: Arc<Mutex<Status>>,
    emit: Box<dyn Fn(Event)>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

unsafe extern "system" fn hook(_: HWINEVENTHOOK, event: u32, h: HWND, id_object: i32, id_child: i32, _: u32, _: u32) {
    if id_object != 0 || id_child != 0 {
        return; // seulement la fenêtre elle-même
    }
    if event != EVENT_FOREGROUND && h.0 as isize != TRACKED.load(Relaxed) {
        return;
    }
    let _ = PostMessageW(Some(HWND(WND.load(Relaxed) as *mut _)), WM_HOOK, WPARAM(event as usize), LPARAM(h.0 as isize));
}

fn status_of(ctrl: &Controller) -> Status {
    let state = match ctrl.state() {
        State::Idle => "idle",
        State::Recording(_) => "recording",
        State::Paused(_) => "paused",
        State::Error(_) => "error",
    };
    Status { state, text: ctrl.tooltip() }
}

impl App {
    /// Publie l'état si il a changé.
    fn refresh(&mut self) {
        TRACKED.store(self.ctrl.tracked(), Relaxed);
        let now = status_of(&self.ctrl);
        if self.shown.as_ref() == Some(&now) {
            return;
        }
        *self.status.lock().unwrap() = now.clone();
        self.shown = Some(now.clone());
        (self.emit)(Event::Status(now));
    }

    fn notice(&self, title: &str, text: &str) {
        if self.ctrl.cfg.notify {
            (self.emit)(Event::Notice { title: title.into(), text: text.into() });
        }
    }

    fn register_hotkeys(&self) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), 1);
            let _ = UnregisterHotKey(Some(self.hwnd), 2);
            let _ = UnregisterHotKey(Some(self.hwnd), 3);
            let cfg = &self.ctrl.cfg;
            for (id, spec) in [(1, Some(cfg.hotkey.as_str())), (2, cfg.hotkey_last.as_deref()), (3, cfg.hotkey_marker.as_deref())] {
                let Some(s) = spec else { continue };
                match parse_hotkey(s) {
                    Some((m, k)) if RegisterHotKey(Some(self.hwnd), id, HOT_KEY_MODIFIERS(m) | MOD_NOREPEAT, k).is_ok() => {}
                    Some(_) => self.notice("Bobyn", &format!("{s} est déjà pris par une autre application")),
                    None => self.notice("Bobyn", &format!("raccourci invalide : {s}")),
                }
            }
        }
    }

    fn save(&mut self, last: bool) {
        if self.ctrl.cfg.beep {
            unsafe { let _ = MessageBeep(MB_OK); }
        }
        if let Err(e) = self.ctrl.save(last) {
            self.notice("Bobyn", &e);
        }
    }

    fn mark(&mut self) {
        match self.ctrl.mark() {
            Ok(n) => {
                if self.ctrl.cfg.marker_beep {
                    unsafe { let _ = MessageBeep(MB_ICONASTERISK); }
                }
                (self.emit)(Event::Marker(n));
                if self.ctrl.cfg.marker_autosave {
                    self.save(false);
                }
            }
            Err(e) => self.notice("Bobyn", &e),
        }
    }

    fn apply(&mut self, cfg: Config) {
        self.ctrl.cfg = cfg;
        self.register_hotkeys();
        self.ctrl.begin();
    }
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            f(app);
            app.refresh();
        }
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOOK => with_app(|a| {
            let h = HWND(lp.0 as *mut _);
            match wp.0 as u32 {
                EVENT_FOREGROUND => {
                    a.ctrl.on_foreground(h);
                    // Pas encore de jeu ? Quelques revérifications différées (la fenêtre passe en plein écran après sa création).
                    if a.ctrl.cfg.mode == "game" && a.ctrl.state() == State::Idle {
                        a.recheck = 0;
                        SetTimer(Some(hwnd), TIMER_RECHECK, RECHECK_MS[0], None);
                    }
                }
                EVENT_MINIMIZESTART => a.ctrl.on_minimize(h, true),
                EVENT_MINIMIZEEND => a.ctrl.on_minimize(h, false),
                _ => {}
            }
        }),
        WM_TIMER if wp.0 == TIMER_RECHECK => with_app(|a| {
            let _ = KillTimer(Some(hwnd), TIMER_RECHECK);
            a.ctrl.on_foreground(GetForegroundWindow());
            a.recheck += 1;
            if a.ctrl.state() == State::Idle && a.recheck < RECHECK_MS.len() {
                SetTimer(Some(hwnd), TIMER_RECHECK, RECHECK_MS[a.recheck], None);
            }
        }),
        WM_PIPE_ENDED => with_app(|a| a.ctrl.on_pipe_ended()),
        WM_SAVED => {
            let msg = *Box::from_raw(lp.0 as *mut String);
            with_app(|a| match msg.split_once('|') {
                Some(("ok", path)) => {
                    let name = path.rsplit('\\').next().unwrap_or(path);
                    (a.emit)(Event::Saved(name.into()));
                }
                Some((_, e)) => a.notice("Sauvegarde échouée", e),
                None => {}
            });
        }
        WM_APPLY => {
            let cfg = *Box::from_raw(lp.0 as *mut Config);
            with_app(|a| a.apply(cfg));
        }
        WM_SAVE => with_app(|a| a.save(wp.0 == 1)),
        WM_HOTKEY => with_app(|a| if wp.0 == 3 { a.mark() } else { a.save(wp.0 == 2) }),
        WM_STOP => PostQuitMessage(0),
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    LRESULT(0)
}

/// Poignée du service : l'arrêter (drop) libère la capture, l'audio et les buffers.
pub struct Service {
    hwnd: isize,
    status: Arc<Mutex<Status>>,
    thread: Option<JoinHandle<()>>,
}

impl Service {
    /// Démarre le thread des messages ; `emit` est appelé depuis ce thread.
    pub fn start(cfg: Config, emit: impl Fn(Event) + Send + 'static) -> Result<Self, String> {
        let status = Arc::new(Mutex::new(Status { state: "idle", text: String::new() }));
        let (tx, rx) = mpsc::channel();
        let st = status.clone();
        let thread = std::thread::spawn(move || unsafe {
            let created = (|| -> windows::core::Result<_> {
                let inst = GetModuleHandleW(None)?;
                let class = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.into(), lpszClassName: w!("EstompeBobyn"), ..Default::default() };
                RegisterClassW(&class); // déjà enregistrée après un redémarrage du service : sans conséquence
                CreateWindowExW(Default::default(), w!("EstompeBobyn"), w!("Bobyn"), Default::default(), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(inst.into()), None)
            })();
            let hwnd = match created {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };
            WND.store(hwnd.0 as isize, Relaxed);
            let hooks = [
                SetWinEventHook(EVENT_FOREGROUND, EVENT_FOREGROUND, None, Some(hook), 0, 0, 2 /* WINEVENT_SKIPOWNPROCESS */),
                SetWinEventHook(EVENT_MINIMIZESTART, EVENT_MINIMIZEEND, None, Some(hook), 0, 0, 2),
            ];
            let mut app = App { ctrl: Controller::new(cfg, hwnd), hwnd, recheck: 0, shown: None, status: st, emit: Box::new(emit) };
            app.register_hotkeys();
            app.ctrl.begin();
            app.refresh();
            APP.with(|a| *a.borrow_mut() = Some(app));
            let _ = tx.send(Ok(hwnd.0 as isize));
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            for h in hooks {
                let _ = UnhookWinEvent(h);
            }
            let _ = UnregisterHotKey(Some(hwnd), 1);
            let _ = UnregisterHotKey(Some(hwnd), 2);
            let _ = UnregisterHotKey(Some(hwnd), 3);
            APP.with(|a| *a.borrow_mut() = None); // arrête session, capture et audio
            let _ = DestroyWindow(hwnd);
        });
        let hwnd = rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { hwnd, status, thread: Some(thread) })
    }

    fn post(&self, msg: u32, wp: usize, lp: isize) -> bool {
        unsafe { PostMessageW(Some(HWND(self.hwnd as *mut _)), msg, WPARAM(wp), LPARAM(lp)).is_ok() }
    }

    /// Nouveaux réglages, pris en compte tout de suite (la session repart).
    pub fn apply(&self, cfg: Config) {
        let p = Box::into_raw(Box::new(cfg));
        if !self.post(WM_APPLY, 0, p as isize) {
            drop(unsafe { Box::from_raw(p) });
        }
    }

    /// Sauvegarde le buffer (`last` : seulement les dernières secondes).
    pub fn save(&self, last: bool) {
        self.post(WM_SAVE, last as usize, 0);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.post(WM_STOP, 0, 0);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
