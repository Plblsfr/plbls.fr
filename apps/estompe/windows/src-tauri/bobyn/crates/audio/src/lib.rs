//! Audio : capture WASAPI par source (application, système, micro) vers des tampons PCM, encodage AAC à la sauvegarde.
#![cfg(windows)]
pub mod aac;
mod capture;
pub use capture::{exe_of, mics, sessions, Audio, Source, Spec, Track};

static LOG: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Destination des lignes de diagnostic (celle de l'application) ; à défaut, stderr.
pub fn set_logger(f: fn(&str)) {
    let _ = LOG.set(f);
}

pub(crate) fn log(msg: &str) {
    match LOG.get() {
        Some(f) => f(msg),
        None => eprintln!("bobyn audio : {msg}"),
    }
}
