//! Journal du mode debug (EST-48) : lignes JSON (`t` en ms, `l` niveau, `s` source, `m` message) ajoutées à un fichier du
//! dossier de logs de l'app. Rien n'est écrit tant que le mode est désactivé. Rotation : au-delà de `MAX`, le fichier
//! devient `estompe-debug.1.log` (l'ancien `.1` est écrasé). Le webview envoie ses lignes déjà expurgées ; les lignes natives
//! ne contiennent jamais de jeton.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX: u64 = 512 * 1024;
static ENABLED: AtomicBool = AtomicBool::new(false);
static FILE: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = FILE.set(dir.join("estompe-debug.log"));
}

fn old(p: &Path) -> PathBuf {
    p.with_file_name("estompe-debug.1.log")
}

fn append(p: &Path, line: &str) {
    let _g = LOCK.lock();
    if fs::metadata(p).map(|m| m.len() >= MAX).unwrap_or(false) {
        let _ = fs::rename(p, old(p));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{line}");
    }
}

/// Ligne native (capture, partage) ; sans effet si le mode debug est désactivé.
pub fn log(level: &str, source: &str, msg: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Some(p) = FILE.get() {
        append(p, &serde_json::json!({ "t": t, "l": level, "s": source, "m": msg }).to_string());
    }
}

#[tauri::command]
pub fn debug_set(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    log("info", "debug", if enabled { "mode debug activé" } else { "mode debug désactivé" });
}

/// Ligne produite par le webview, déjà au format JSON.
#[tauri::command]
pub fn debug_write(line: String) {
    if let (true, Some(p)) = (ENABLED.load(Ordering::Relaxed), FILE.get()) {
        append(p, &line);
    }
}

/// Contenu du journal (ancien fichier puis courant), une ligne JSON par entrée.
#[tauri::command]
pub fn debug_read() -> String {
    let Some(p) = FILE.get() else { return String::new() };
    let _g = LOCK.lock();
    fs::read_to_string(old(p)).unwrap_or_default() + &fs::read_to_string(p).unwrap_or_default()
}

#[tauri::command]
pub fn debug_clear() {
    if let Some(p) = FILE.get() {
        let _g = LOCK.lock();
        let _ = fs::remove_file(p);
        let _ = fs::remove_file(old(p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_au_dela_du_plafond() {
        let dir = std::env::temp_dir().join(format!("estompe-debug-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("estompe-debug.log");
        fs::write(&p, vec![b'x'; MAX as usize]).unwrap();
        append(&p, "{\"m\":\"neuf\"}");
        assert_eq!(fs::metadata(old(&p)).unwrap().len(), MAX);
        assert_eq!(fs::read_to_string(&p).unwrap(), "{\"m\":\"neuf\"}\n");
        fs::remove_dir_all(dir).unwrap();
    }
}
