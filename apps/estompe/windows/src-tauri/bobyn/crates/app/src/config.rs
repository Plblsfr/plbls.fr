//! Réglages : un fichier JSON dans le dossier de configuration d'Estompe, écrit par l'onglet Bobyn et appliqué à chaud.
use bobyn_buffer::Codec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AudioSrc {
    /// `game` (l'exécutable détecté), `system`, `mic` ou `app:X.exe`.
    pub source: String,
    pub gain: f32,
    /// Micro seulement : identifiant du périphérique (défaut si absent) et touche push-to-talk.
    pub device: Option<String>,
    pub ptt: Option<String>,
}

impl Default for AudioSrc {
    fn default() -> Self {
        Self { source: "game".into(), gain: 1.0, device: None, ptt: None }
    }
}

/// Surcharges propres à un jeu : tout champ absent reprend le réglage général.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Profile {
    pub seconds: Option<u32>,
    pub fps: Option<u32>,
    pub mbps: Option<u32>,
    pub height: Option<u32>,
    pub codec: Option<String>,
    pub cursor: Option<bool>,
    pub audio: Option<Vec<AudioSrc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    /// Service actif : tant que c'est faux, rien n'est capturé ni enregistré.
    pub enabled: bool,
    pub seconds: u32,
    pub fps: u32,
    pub mbps: u32,
    pub height: Option<u32>,
    pub codec: String,
    /// `game` (jeu au premier plan), `window` (fenêtre de l'exécutable `window`) ou `screen` (moniteur `screen`).
    pub mode: String,
    pub window: String,
    pub screen: usize,
    /// Buffer actif en permanence (sur l'écran principal hors jeu) au lieu de démarrer à la détection d'un jeu.
    pub always_on: bool,
    pub cursor: Option<bool>,
    pub dir: Option<String>,
    pub hotkey: String,
    pub hotkey_last: Option<String>,
    /// Pose un marqueur dans l'enregistrement en cours.
    pub hotkey_marker: Option<String>,
    pub last_seconds: u32,
    /// Exécutables toujours considérés comme des jeux, même en fenêtré.
    pub games: Vec<String>,
    pub audio: Vec<AudioSrc>,
    pub profiles: HashMap<String, Profile>,
    pub beep: bool,
    pub notify: bool,
    /// Son à la pose d'un marqueur, et sauvegarde automatique du replay à ce moment.
    pub marker_beep: bool,
    pub marker_autosave: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            seconds: 60,
            fps: 60,
            mbps: 20,
            height: None,
            codec: "h264".into(),
            mode: "game".into(),
            window: String::new(),
            screen: 0,
            always_on: false,
            cursor: None,
            dir: None,
            hotkey: "Alt+F10".into(),
            hotkey_last: None,
            hotkey_marker: Some("Alt+F9".into()),
            last_seconds: 30,
            games: Vec::new(),
            audio: ["game", "voice", "mic"].map(|source| AudioSrc { source: source.into(), ..Default::default() }).into(),
            profiles: HashMap::new(),
            beep: true,
            notify: true,
            marker_beep: true,
            marker_autosave: false,
        }
    }
}

/// Réglages effectifs d'une session, profil du jeu appliqué.
#[derive(Clone, Debug)]
pub struct Effective {
    pub seconds: u32,
    pub fps: u32,
    pub mbps: u32,
    pub height: Option<u32>,
    pub codec: Codec,
    pub cursor: bool,
    pub audio: Vec<AudioSrc>,
}

impl Config {
    /// Fichier absent ou illisible : réglages par défaut (l'erreur est renvoyée pour l'affichage).
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(t) => match serde_json::from_str(&t) {
                Ok(c) => (c, None),
                Err(e) => (Self::default(), Some(format!("réglages Bobyn invalides : {e}"))),
            },
            Err(_) => (Self::default(), None),
        }
    }

    pub fn store(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)
    }

    pub fn save_dir(&self) -> PathBuf {
        self.dir.as_ref().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Videos").join("Replays"))
    }

    /// `is_game` : mode jeu (curseur masqué par défaut) ou autre.
    pub fn effective(&self, exe: Option<&str>, is_game: bool) -> Effective {
        let p = exe.and_then(|e| self.profiles.iter().find(|(k, _)| k.eq_ignore_ascii_case(e)).map(|(_, v)| v.clone())).unwrap_or_default();
        Effective {
            seconds: p.seconds.unwrap_or(self.seconds).clamp(15, 600),
            fps: p.fps.unwrap_or(self.fps).min(240), // 0 = fréquence de l'écran, résolue au démarrage de la session
            mbps: p.mbps.unwrap_or(self.mbps).max(1),
            height: p.height.or(self.height),
            codec: Codec::parse(p.codec.as_deref().unwrap_or(&self.codec)).unwrap_or(Codec::H264),
            cursor: p.cursor.or(self.cursor).unwrap_or(!is_game),
            audio: p.audio.unwrap_or_else(|| self.audio.clone()),
        }
    }
}

/// Code de touche virtuelle : F1-F24, A-Z, 0-9.
pub fn parse_key(s: &str) -> Option<u32> {
    let s = s.to_ascii_uppercase();
    match s.as_bytes() {
        [b'F', rest @ ..] if !rest.is_empty() => s[1..].parse::<u32>().ok().filter(|n| (1..=24).contains(n)).map(|n| 0x6F + n),
        [c] if c.is_ascii_alphanumeric() => Some(*c as u32),
        _ => None,
    }
}

/// « Alt+F10 », « Ctrl+Shift+S » -> (modificateurs MOD_*, touche).
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let (mut mods, mut key) = (0, None);
    for part in s.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "alt" => mods |= 1,
            "ctrl" | "control" => mods |= 2,
            "shift" => mods |= 4,
            "win" => mods |= 8,
            _ => key = Some(parse_key(part)?),
        }
    }
    key.map(|k| (mods, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys() {
        assert_eq!(parse_hotkey("Alt+F10"), Some((1, 0x79)));
        assert_eq!(parse_hotkey("ctrl + shift + S"), Some((6, b'S' as u32)));
        assert_eq!(parse_hotkey("Alt"), None);
        assert_eq!(parse_hotkey("Alt+F99"), None);
    }

    #[test]
    fn profile_overrides_and_defaults() {
        let mut c = Config::default();
        c.profiles.insert("cod.exe".into(), Profile { mbps: Some(40), cursor: Some(true), ..Default::default() });
        let e = c.effective(Some("COD.exe"), true);
        assert_eq!((e.mbps, e.cursor, e.fps), (40, true, 60));
        assert!(!c.effective(Some("x.exe"), true).cursor);
        assert!(c.effective(None, false).cursor);
        let partial: Config = serde_json::from_str(r#"{"seconds":120}"#).unwrap();
        assert_eq!((partial.seconds, partial.hotkey.as_str()), (120, "Alt+F10"));
        let sources: Vec<_> = partial.audio.iter().map(|a| a.source.as_str()).collect();
        assert_eq!(sources, ["game", "voice", "mic"]);
    }
}
