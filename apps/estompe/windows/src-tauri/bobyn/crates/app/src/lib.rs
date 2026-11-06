//! Bobyn : replay instantané, intégré à l'app Windows d'Estompe (service en arrière-plan piloté par des commandes Tauri).
pub mod config;
pub mod markers;
#[cfg(windows)]
mod control;
#[cfg(windows)]
pub mod detect;
#[cfg(windows)]
mod games;
#[cfg(windows)]
mod pipeline;
#[cfg(windows)]
mod save;
#[cfg(windows)]
mod service;

#[cfg(windows)]
pub use service::{Event, Service, Status};

#[cfg(windows)]
pub use bobyn_audio as audio;

#[cfg(windows)]
static LOG: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Destination des lignes de diagnostic (journal du mode debug d'Estompe) ; à défaut, stderr.
#[cfg(windows)]
pub fn set_logger(f: fn(&str)) {
    let _ = LOG.set(f);
    bobyn_audio::set_logger(f);
}

#[cfg(windows)]
fn log(msg: &str) {
    match LOG.get() {
        Some(f) => f(msg),
        None => eprintln!("bobyn : {msg}"),
    }
}

/// Codecs que le GPU sait réellement encoder.
#[cfg(windows)]
pub fn codecs() -> Vec<&'static str> {
    bobyn_capture::gpu::open().map(|g| bobyn_encode::supported(&g.device).iter().map(|c| c.label()).collect()).unwrap_or_default()
}
