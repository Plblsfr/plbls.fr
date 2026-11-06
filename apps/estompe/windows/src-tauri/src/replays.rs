//! Bibliothèque des replays de Bobyn (EST-127) : liste du dossier de sauvegarde, miniatures, renommage, suppression et export
//! (découpe, recadrage) par un ffmpeg embarqué. L'export tourne en priorité basse et hors du fil de l'interface.
use bobyn::config::Config;
use bobyn::markers::{self, Marker};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct Replays {
    exports: Mutex<HashMap<u32, Arc<Mutex<Option<Child>>>>>,
    next: Mutex<u32>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Replay {
    pub name: String,
    pub game: String,
    pub size: u64,
    /// Secondes depuis l'epoch.
    pub modified: u64,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    /// Images par seconde de la vidéo (0 si inconnu).
    pub fps: f64,
    pub markers: Vec<Marker>,
    /// Noms des pistes audio du fichier, dans l'ordre (la piste « Mix » vient en tête).
    pub tracks: Vec<String>,
}

#[derive(Serialize)]
pub struct Library {
    pub dir: String,
    pub replays: Vec<Replay>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ExportParams {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub crop: Option<Crop>,
    /// `original`, `1080` ou `720` (hauteur maximale).
    pub quality: String,
    /// Débit vidéo imposé en kb/s (envoi d'un replay trop lourd) ; absent : proportionnel à la source.
    #[serde(default)]
    pub target_kbps: Option<u32>,
    /// Pistes audio gardées et leur volume ; absent : le son du fichier sans choix (première piste, comme avant EST-131).
    #[serde(default)]
    pub audio: Option<AudioMix>,
    /// `auto` (le meilleur encodeur matériel, avec repli sur les suivants) ou le nom d'un encodeur de `replays_encoders`.
    #[serde(default)]
    pub encoder: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TrackGain {
    /// Rang de la piste parmi les pistes audio du fichier.
    pub index: usize,
    pub volume: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AudioMix {
    pub tracks: Vec<TrackGain>,
    /// Une seule piste mixée plutôt que les pistes séparées.
    pub mixed: bool,
}

#[derive(Serialize, Clone)]
struct ExportEvent {
    id: u32,
    /// `run`, `done`, `error` ou `cancel`.
    state: &'static str,
    pct: f64,
    file: Option<String>,
    mode: Option<&'static str>,
    message: Option<String>,
}

fn log(m: &str) {
    crate::debuglog::log("info", "natif/replays", m);
}

fn save_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app.path().app_config_dir().map_err(|e| e.to_string())?.join("bobyn.json");
    Ok(Config::load(&path).0.save_dir())
}

fn ffmpeg(app: &AppHandle) -> PathBuf {
    let exe = app.path().resource_dir().map(|d| d.join("ffmpeg.exe")).unwrap_or_default();
    if exe.exists() { exe } else { PathBuf::from("ffmpeg") }
}

fn command(app: &AppHandle) -> Command {
    let mut c = Command::new(ffmpeg(app));
    c.args(["-hide_banner", "-nostdin"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000 | 0x0000_4000); // sans console, priorité inférieure à la normale
    }
    c
}

/// Un nom de fichier du dossier de sauvegarde, jamais un chemin.
fn file_in(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let ok = !name.is_empty() && name == Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or("") && name.to_lowercase().ends_with(".mp4");
    if ok { Ok(dir.join(name)) } else { Err("nom de replay invalide".into()) }
}

/// `{jeu}_AAAA-MM-JJ_HH-MM-SS.mp4` → `jeu` ; autre nom : vide.
pub fn game_of(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |s| s.0);
    let parts: Vec<&str> = stem.rsplitn(3, '_').collect();
    let dated = parts.len() == 3 && parts[1].len() == 10 && parts[1].matches('-').count() == 2 && parts[0].len() == 8 && parts[0].matches('-').count() == 2;
    if dated { parts[2].to_string() } else { String::new() }
}

/// Durée en secondes, dimensions et noms des pistes audio, d'après la sortie de `ffmpeg -i`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Meta {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub tracks: Vec<String>,
}

pub fn parse_info(text: &str) -> Meta {
    let duration = text.split("Duration: ").nth(1).and_then(|s| s.get(..11)).map_or(0.0, |d| {
        let mut p = d.split(':').map(|x| x.parse::<f64>().unwrap_or(0.0));
        p.next().unwrap_or(0.0) * 3600.0 + p.next().unwrap_or(0.0) * 60.0 + p.next().unwrap_or(0.0)
    });
    let (w, h) = text
        .lines()
        .find(|l| l.contains("Video:"))
        .and_then(|l| {
            l.split(|c: char| c == ' ' || c == ',').find_map(|t| {
                let (a, b) = t.split_once('x')?;
                let (a, b) = (a.parse::<u32>().ok()?, b.parse::<u32>().ok()?);
                (a >= 16 && b >= 16).then_some((a, b))
            })
        })
        .unwrap_or((0, 0));
    let fps = text
        .lines()
        .find(|l| l.contains("Video:"))
        .and_then(|l| l.split(',').find_map(|t| t.trim().strip_suffix(" fps")?.parse::<f64>().ok()))
        .unwrap_or(0.0);
    // Le nom d'une piste est le `handler_name` qui suit sa ligne « Stream … Audio: ».
    let mut tracks: Vec<String> = vec![];
    let mut in_audio = false;
    for l in text.lines() {
        if l.contains("Stream #") {
            in_audio = l.contains("Audio:");
            if in_audio {
                tracks.push(format!("Piste {}", tracks.len() + 1));
            }
        } else if let (true, Some(n)) = (in_audio, l.trim().strip_prefix("handler_name")) {
            if let (Some(t), Some((_, n))) = (tracks.last_mut(), n.split_once(':')) {
                *t = n.trim().to_string();
            }
        }
    }
    Meta { duration, width: w, height: h, fps, tracks }
}

/// Instants des images clés d'après `showinfo`.
pub fn parse_keyframes(text: &str) -> Vec<f64> {
    text.lines().filter_map(|l| l.split("pts_time:").nth(1)?.split_whitespace().next()?.parse().ok()).collect()
}

fn meta_cache() -> &'static Mutex<HashMap<(PathBuf, u64, u64), Meta>> {
    static C: OnceLock<Mutex<HashMap<(PathBuf, u64, u64), Meta>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn probe(app: &AppHandle, path: &Path, size: u64, modified: u64) -> Meta {
    let key = (path.to_path_buf(), size, modified);
    if let Some(m) = meta_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return m;
    }
    let out = command(app).arg("-i").arg(path).output().map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default();
    let m = parse_info(&out);
    if let Ok(mut c) = meta_cache().lock() {
        c.insert(key, m.clone());
    }
    m
}

/// Fichiers `.mp4` du dossier : (nom, taille, date de modification), sans les fichiers de travail d'un export.
pub fn scan(dir: &Path) -> Vec<(String, u64, u64)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let md = e.metadata().ok()?;
            let ok = md.is_file() && name.to_lowercase().ends_with(".mp4");
            ok.then(|| (name, md.len(), md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())))
        })
        .collect()
}

fn allow(app: &AppHandle, dir: &Path) {
    let _ = app.asset_protocol_scope().allow_directory(dir, false);
}

#[tauri::command]
pub async fn replays_list(app: AppHandle) -> Result<Library, String> {
    let dir = save_dir(&app)?;
    allow(&app, &dir);
    tauri::async_runtime::spawn_blocking(move || {
        let replays = scan(&dir)
            .into_iter()
            .map(|(name, size, modified)| {
                let Meta { duration, width, height, fps, tracks } = probe(&app, &dir.join(&name), size, modified);
                Replay { game: game_of(&name), markers: markers::read(&dir.join(&name)), name, size, modified, duration, width, height, fps, tracks }
            })
            .collect();
        Library { dir: dir.to_string_lossy().into_owned(), replays }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Miniature JPEG en cache (dossier de cache de l'app) ; renvoie son chemin, lisible par le protocole `asset`.
#[tauri::command]
pub async fn replays_thumb(app: AppHandle, name: String) -> Result<String, String> {
    let src = file_in(&save_dir(&app)?, &name)?;
    let md = std::fs::metadata(&src).map_err(|e| e.to_string())?;
    let stamp = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("replays");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    allow(&app, &dir);
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let thumb = dir.join(format!("{safe}-{}-{stamp}.jpg", md.len()));
    if !thumb.exists() {
        let out = thumb.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // 1 s : le tout début est souvent noir ; une vidéo plus courte retombe sur la première image.
            for ss in ["1", "0"] {
                let ok = command(&app).args(["-v", "error", "-ss", ss, "-i"]).arg(&src).args(["-frames:v", "1", "-vf", "scale=320:-2", "-y"]).arg(&out).status().map(|s| s.success()).unwrap_or(false);
                if ok && out.exists() {
                    return Ok(());
                }
            }
            Err("miniature impossible".to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    Ok(thumb.to_string_lossy().into_owned())
}

/// Pistes audio du replay extraites sans réencodage (cache), pour l'écoute en direct du mélange : le lecteur du webview ne lit qu'une piste.
/// Renvoie un chemin lisible par le protocole `asset` par piste, dans l'ordre du fichier.
#[tauri::command]
pub async fn replays_tracks(app: AppHandle, name: String) -> Result<Vec<String>, String> {
    let src = file_in(&save_dir(&app)?, &name)?;
    let md = std::fs::metadata(&src).map_err(|e| e.to_string())?;
    let stamp = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("replays");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    allow(&app, &dir);
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    tauri::async_runtime::spawn_blocking(move || {
        let count = probe(&app, &src, md.len(), stamp).tracks.len();
        let files: Vec<PathBuf> = (0..count).map(|i| dir.join(format!("{safe}-{}-{stamp}-p{i}.m4a", md.len()))).collect();
        if files.iter().any(|f| !f.exists()) {
            let mut c = command(&app);
            c.args(["-v", "error", "-i"]).arg(&src).arg("-vn");
            for (i, f) in files.iter().enumerate() {
                c.args(["-map".to_string(), format!("0:a:{i}"), "-c".into(), "copy".into(), "-y".into()]).arg(f);
            }
            if !c.status().map(|s| s.success()).unwrap_or(false) {
                return Err("extraction des pistes audio impossible".to_string());
            }
        }
        Ok(files.iter().map(|f| f.to_string_lossy().into_owned()).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Nom de fichier saisi par l'utilisateur : sans caractère interdit, extension `.mp4`.
pub fn clean_name(raw: &str) -> Option<String> {
    let stem = raw.trim().trim_end_matches(".mp4").trim_end_matches(".MP4");
    let stem: String = stem.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()).collect();
    let stem = stem.trim().trim_matches('.').to_string();
    (!stem.is_empty() && stem.len() <= 150).then(|| format!("{stem}.mp4"))
}

#[tauri::command]
pub fn replays_rename(app: AppHandle, name: String, to: String) -> Result<String, String> {
    let dir = save_dir(&app)?;
    let from = file_in(&dir, &name)?;
    let new = clean_name(&to).ok_or("nom invalide")?;
    let dest = dir.join(&new);
    if dest.exists() && new != name {
        return Err("un replay porte déjà ce nom".into());
    }
    std::fs::rename(&from, &dest).map_err(|e| e.to_string())?;
    let _ = std::fs::rename(markers::sidecar(&from), markers::sidecar(&dest));
    log(&format!("renommé : {name} → {new}"));
    Ok(new)
}

#[tauri::command]
pub fn replays_delete(app: AppHandle, name: String) -> Result<(), String> {
    let path = file_in(&save_dir(&app)?, &name)?;
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(markers::sidecar(&path));
    log(&format!("supprimé : {name}"));
    Ok(())
}

/// Remplace les marqueurs d'un replay (ajout, renommage, suppression depuis le lecteur).
#[tauri::command]
pub fn replays_markers_set(app: AppHandle, name: String, markers: Vec<Marker>) -> Result<(), String> {
    let path = file_in(&save_dir(&app)?, &name)?;
    markers::write(&path, &markers::sanitize(markers)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn replays_reveal(app: AppHandle, name: Option<String>) -> Result<(), String> {
    let dir = save_dir(&app)?;
    match name {
        Some(n) => tauri_plugin_opener::reveal_item_in_dir(file_in(&dir, &n)?),
        None => tauri_plugin_opener::open_path(dir, None::<&str>),
    }
    .map_err(|e| e.to_string())
}

/// Met le fichier dans le presse-papiers (collable dans l'Explorateur).
#[tauri::command]
pub fn replays_copy(app: AppHandle, name: String) -> Result<(), String> {
    let path = file_in(&save_dir(&app)?, &name)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let ok = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "Set-Clipboard -LiteralPath $args[0]"])
            .arg(&path)
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| e.to_string())?
            .success();
        if ok { Ok(()) } else { Err("copie impossible".into()) }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Windows seulement".into())
    }
}

/// Encodeurs matériels de H.264 proposés par ce ffmpeg, du plus au moins souhaitable.
fn encoders(app: &AppHandle) -> &'static Vec<&'static str> {
    static E: OnceLock<Vec<&'static str>> = OnceLock::new();
    E.get_or_init(|| {
        let out = command(app).arg("-encoders").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        ["h264_nvenc", "h264_amf", "h264_qsv", "h264_mf"].into_iter().filter(|e| out.contains(e)).collect()
    })
}

/// Pourquoi la découpe devra être réencodée ; `None` si elle peut être copiée telle quelle (ni recadrage, ni changement de qualité, début sur une image clé ou au début).
pub fn reencode_reason(p: &ExportParams, keyframes: &[f64]) -> Option<&'static str> {
    if p.crop.is_some() {
        Some("recadrage")
    } else if p.quality != "original" {
        Some("qualité réduite")
    } else if p.start >= 0.05 && !keyframes.iter().any(|k| (k - p.start).abs() < 0.05) {
        Some("début hors image clé")
    } else {
        None
    }
}

pub fn can_copy(p: &ExportParams, keyframes: &[f64]) -> bool {
    reencode_reason(p, keyframes).is_none()
}

/// Encodeurs à essayer : `auto` ou absent, tous ceux disponibles ; sinon celui demandé s'il l'est.
pub fn pick_encoders(available: &[&'static str], wanted: Option<&str>) -> Result<Vec<&'static str>, String> {
    match wanted {
        None | Some("auto") => Ok(available.to_vec()),
        Some(w) => available.iter().find(|e| **e == w).map(|e| vec![*e]).ok_or_else(|| format!("encodeur {w} indisponible")),
    }
}

/// Images clés de la vidéo (instants), pour l'aimantation de la découpe et le choix de la copie.
fn keyframes_of(app: &AppHandle, src: &Path) -> Vec<f64> {
    let o = command(app).args(["-skip_frame", "nokey", "-i"]).arg(src).args(["-an", "-vf", "showinfo", "-f", "null", "-"]).output();
    parse_keyframes(&o.map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default())
}

#[derive(Serialize)]
pub struct Plan {
    pub copy: bool,
    pub reason: Option<&'static str>,
}

/// Ce que ferait l'export : copie sans réencodage, ou réencodage et pourquoi.
#[tauri::command]
pub async fn replays_plan(app: AppHandle, params: ExportParams) -> Result<Plan, String> {
    let src = file_in(&save_dir(&app)?, &params.name)?;
    tauri::async_runtime::spawn_blocking(move || {
        let keyframes = if params.crop.is_none() && params.quality == "original" && params.start >= 0.05 { keyframes_of(&app, &src) } else { vec![] };
        let reason = reencode_reason(&params, &keyframes);
        Plan { copy: reason.is_none(), reason }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Images clés d'un replay (secondes).
#[tauri::command]
pub async fn replays_keyframes(app: AppHandle, name: String) -> Result<Vec<f64>, String> {
    let src = file_in(&save_dir(&app)?, &name)?;
    tauri::async_runtime::spawn_blocking(move || keyframes_of(&app, &src)).await.map_err(|e| e.to_string())
}

/// Encodeurs matériels H.264 disponibles sur cette machine.
#[tauri::command]
pub fn replays_encoders(app: AppHandle) -> Vec<&'static str> {
    encoders(&app).clone()
}

fn even(n: u32) -> u32 {
    n & !1
}

/// Sélection et mixage des pistes audio d'un export : (arguments, réencodage de l'audio nécessaire).
/// Pistes séparées au volume inchangé : copiées telles quelles ; sinon filtrées (volume, mixage) puis réencodées.
pub fn audio_args(mix: &AudioMix, names: &[String]) -> (Vec<String>, bool) {
    let mut a: Vec<String> = vec!["-map".into(), "0:v".into()];
    let vol = |t: &TrackGain| if t.volume.is_finite() { t.volume.clamp(0.0, 2.0) } else { 1.0 };
    if mix.tracks.is_empty() {
        a.push("-an".into());
        return (a, false);
    }
    if !mix.mixed && mix.tracks.iter().all(|t| vol(t) == 1.0) {
        for (k, t) in mix.tracks.iter().enumerate() {
            a.extend(["-map".into(), format!("0:a:{}", t.index)]);
            a.extend([format!("-metadata:s:a:{k}"), format!("handler_name={}", names.get(t.index).map_or("", |n| n.as_str()))]);
        }
        return (a, false);
    }
    let mut graph: Vec<String> = mix.tracks.iter().enumerate().map(|(k, t)| format!("[0:a:{}]volume={:.3}[a{k}]", t.index, vol(t))).collect();
    if mix.mixed {
        let inputs: String = (0..mix.tracks.len()).map(|k| format!("[a{k}]")).collect();
        graph.push(format!("{inputs}amix=inputs={}:normalize=0:duration=longest[out]", mix.tracks.len()));
        a.extend(["-map".into(), "[out]".into(), "-metadata:s:a:0".into(), "handler_name=Mix".into()]);
    } else {
        for (k, t) in mix.tracks.iter().enumerate() {
            a.extend(["-map".into(), format!("[a{k}]"), format!("-metadata:s:a:{k}"), format!("handler_name={}", names.get(t.index).map_or("", |n| n.as_str()))]);
        }
    }
    a.splice(0..0, ["-filter_complex".to_string(), graph.join(";")]);
    (a, true)
}

/// Arguments ffmpeg d'un export. `src_height` et `src_kbps` servent au réencodage (hauteur de sortie et débit proportionnel).
/// `meta` : fichier `ffmetadata` des chapitres de l'extrait. `tracks` : noms des pistes audio de la source.
#[allow(clippy::too_many_arguments)]
pub fn export_args(src: &Path, out: &Path, p: &ExportParams, copy: bool, encoder: &str, src_height: u32, src_kbps: u32, meta: Option<&Path>, tracks: &[String]) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-progress", "pipe:1", "-ss"].map(String::from).into();
    a.push(format!("{:.3}", p.start));
    a.extend(["-i".into(), src.to_string_lossy().into_owned()]);
    if let Some(m) = meta {
        a.extend(["-f".into(), "ffmetadata".into(), "-i".into(), m.to_string_lossy().into_owned(), "-map_chapters".into(), "1".into()]);
    }
    a.extend(["-t".into(), format!("{:.3}", (p.end - p.start).max(0.1))]);
    let (audio, reencode) = p.audio.as_ref().map_or((vec![], false), |m| audio_args(m, tracks));
    a.extend(audio);
    if copy {
        a.extend(["-c:v", "copy", "-c:a", if reencode { "aac" } else { "copy" }, "-avoid_negative_ts", "make_zero"].map(String::from));
    } else {
        let mut vf = vec![];
        let (mut h, mut w) = (src_height, 0);
        if let Some(c) = &p.crop {
            vf.push(format!("crop={}:{}:{}:{}", even(c.w), even(c.h), even(c.x), even(c.y)));
            (w, h) = (c.w, c.h);
        }
        let max: u32 = p.quality.parse().unwrap_or(0);
        if max > 0 && h > max {
            vf.push(format!("scale=-2:{max}"));
            w = w * max / h.max(1);
            h = max;
        }
        if !vf.is_empty() {
            a.extend(["-vf".into(), vf.join(",")]);
        }
        // Débit proportionnel aux pixels de la source quand on les connaît, sinon celui de la source.
        let ratio = if src_height > 0 && h > 0 && w > 0 { (f64::from(h) / f64::from(src_height)).powi(2).min(1.0) } else { 1.0 };
        let kbps = p.target_kbps.unwrap_or_else(|| ((f64::from(src_kbps) * if p.crop.is_some() || max > 0 { ratio.max(0.25) } else { 1.0 }) as u32).max(1000));
        a.extend(["-c:v".into(), encoder.into(), "-b:v".into(), format!("{kbps}k"), "-pix_fmt".into(), "yuv420p".into(), "-c:a".into(), "aac".into()]);
    }
    a.extend(["-movflags".into(), "+faststart".into(), "-f".into(), "mp4".into(), "-y".into(), out.to_string_lossy().into_owned()]);
    a
}

fn unique_output(dir: &Path, stem: &str) -> PathBuf {
    (1..)
        .map(|n| dir.join(format!("{stem}_edit{}.mp4", if n == 1 { String::new() } else { format!("_{n}") })))
        .find(|p| !p.exists())
        .unwrap_or_default()
}

/// Lance l'export en arrière-plan ; la progression arrive par l'évènement `replays-export`.
#[tauri::command]
pub async fn replays_export(app: AppHandle, state: State<'_, Replays>, params: ExportParams) -> Result<u32, String> {
    let dir = save_dir(&app)?;
    let src = file_in(&dir, &params.name)?;
    if !(params.start >= 0.0 && params.end > params.start) {
        return Err("plage de découpe invalide".into());
    }
    let id = {
        let mut n = state.next.lock().map_err(|e| e.to_string())?;
        *n += 1;
        *n
    };
    let slot = Arc::new(Mutex::new(None));
    state.exports.lock().map_err(|e| e.to_string())?.insert(id, slot.clone());
    let a = app.clone();
    let meta_path = std::env::temp_dir().join(format!("estompe-export-{}-{id}.ffmeta", std::process::id()));
    std::thread::spawn(move || {
        let ev = |state, pct, file, mode, message| {
            let _ = a.emit("replays-export", ExportEvent { id, state, pct, file, mode, message });
        };
        let end = |slot: &Arc<Mutex<Option<Child>>>| {
            let _ = std::fs::remove_file(&meta_path);
            let _ = slot.lock().map(|mut c| c.take());
            if let Ok(mut m) = a.state::<Replays>().exports.lock() {
                m.remove(&id);
            }
        };
        let md = std::fs::metadata(&src).ok();
        let size = md.as_ref().map_or(0, |m| m.len());
        let Meta { duration, height, tracks, .. } = probe(&a, &src, size, md.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()));
        if let Some(bad) = params.audio.as_ref().and_then(|m| m.tracks.iter().find(|t| t.index >= tracks.len())) {
            ev("error", 0.0, None, None, Some(format!("piste audio {} absente du fichier", bad.index + 1)));
            return end(&slot);
        }
        let params = ExportParams { end: params.end.min(duration.max(params.start + 0.1)), ..params };
        let kbps = (size as f64 * 8.0 / 1000.0 / duration.max(1.0)) as u32;
        let keyframes = if params.crop.is_none() && params.quality == "original" && params.start >= 0.05 {
            keyframes_of(&a, &src)
        } else {
            vec![]
        };
        let copy = can_copy(&params, &keyframes);
        let stem = params.name.trim_end_matches(".mp4").trim_end_matches(".MP4").to_string();
        let out = unique_output(&dir, &stem);
        let kept = markers::clip(&markers::read(&src), params.start, params.end);
        let meta = (!kept.is_empty() && std::fs::write(&meta_path, markers::ffmetadata(&kept, params.end - params.start)).is_ok()).then_some(meta_path.as_path());
        let part = out.with_extension("mp4.part");
        let started = std::time::Instant::now();
        let candidates = if copy { Ok(vec!["copy"]) } else { pick_encoders(encoders(&a), params.encoder.as_deref()) };
        let candidates = match candidates {
            Ok(c) if !c.is_empty() => c,
            other => {
                let msg = other.err().unwrap_or_else(|| "aucun encodeur matériel H.264 disponible".into());
                log(&format!("export impossible : {msg}"));
                ev("error", 0.0, None, None, Some(msg));
                return end(&slot);
            }
        };
        let mut last_err = String::new();
        for enc in candidates {
            let args = export_args(&src, &part, &params, copy, enc, height, kbps, meta, &tracks);
            let mut child = match command(&a).args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
                Ok(c) => c,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            };
            let stdout = child.stdout.take();
            let mut stderr = child.stderr.take();
            if let Ok(mut s) = slot.lock() {
                *s = Some(child);
            }
            let err_thread = std::thread::spawn(move || {
                let mut t = String::new();
                if let Some(e) = stderr.as_mut() {
                    let _ = e.read_to_string(&mut t);
                }
                t
            });
            let span = params.end - params.start;
            for line in BufReader::new(stdout.unwrap()).lines().map_while(Result::ok) {
                if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
                    ev("run", (us / 1e6 / span).clamp(0.0, 1.0), None, None, None);
                }
            }
            let stderr_text = err_thread.join().unwrap_or_default();
            let status = slot.lock().ok().and_then(|mut s| s.take()).and_then(|mut c| c.wait().ok());
            if slot_cancelled(&a, id) {
                let _ = std::fs::remove_file(&part);
                log("export annulé");
                ev("cancel", 0.0, None, None, None);
                return end(&slot);
            }
            if status.is_some_and(|s| s.success()) && std::fs::rename(&part, &out).is_ok() {
                let file = out.file_name().map(|n| n.to_string_lossy().into_owned());
                let mode = if copy { "copy" } else { "encode" };
                log(&format!("export terminé : {} → {file:?} ({mode}, encodeur {enc}, {:.1} s)", params.name, started.elapsed().as_secs_f64()));
                let _ = markers::write(&out, &kept);
                ev("done", 1.0, file, Some(if copy { "copy" } else { "encode" }), None);
                return end(&slot);
            }
            last_err = stderr_text.lines().last().unwrap_or("échec de ffmpeg").to_string();
            log(&format!("export échoué avec {enc} : {last_err}"));
            let _ = std::fs::remove_file(&part);
        }
        ev("error", 0.0, None, None, Some(last_err));
        end(&slot);
    });
    Ok(id)
}

/// Annulation demandée : `replays_cancel` retire l'export de la table avant de tuer ffmpeg.
fn slot_cancelled(app: &AppHandle, id: u32) -> bool {
    app.state::<Replays>().exports.lock().map(|m| !m.contains_key(&id)).unwrap_or(false)
}

#[tauri::command]
pub fn replays_cancel(state: State<Replays>, id: u32) -> Result<(), String> {
    let slot = state.exports.lock().map_err(|e| e.to_string())?.remove(&id);
    if let Some(c) = slot.and_then(|s| s.lock().ok().and_then(|mut c| c.take())) {
        let mut c = c;
        let _ = c.kill();
        let _ = c.wait();
    }
    Ok(())
}

/// Débit H.264 d'une vidéo convertie : celui de la source, majoré (le H.264 est moins efficace que HEVC/AV1), borné.
pub fn convert_kbps(size: u64, duration: f64) -> u32 {
    ((size as f64 * 8.0 / 1000.0 / duration.max(1.0)) * 1.4).clamp(2000.0, 12000.0) as u32
}

pub fn convert_args(src: &Path, out: &Path, encoder: &str, kbps: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-i"].map(String::from).into();
    a.extend([src.to_string_lossy().into_owned(), "-c:v".into(), encoder.into(), "-b:v".into(), format!("{kbps}k"), "-pix_fmt".into(), "yuv420p".into()]);
    a.extend(["-c:a", "aac", "-movflags", "+faststart", "-f", "mp4", "-y"].map(String::from));
    a.push(out.to_string_lossy().into_owned());
    a
}

/// Convertit en H.264 + AAC (faststart) une vidéo reçue en octets (le webview n'a pas de chemin de fichier) pour qu'elle se lise partout.
#[tauri::command]
pub async fn video_to_h264(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<tauri::ipc::Response, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else { return Err("octets attendus".into()) };
    let data = data.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let candidates = encoders(&app);
        if candidates.is_empty() {
            return Err("aucun encodeur matériel H.264 disponible".to_string());
        }
        let tag = format!("estompe-h264-{}", std::process::id());
        let (src, out) = (std::env::temp_dir().join(format!("{tag}.in")), std::env::temp_dir().join(format!("{tag}.mp4")));
        std::fs::write(&src, &data).map_err(|e| e.to_string())?;
        let kbps = convert_kbps(data.len() as u64, probe(&app, &src, data.len() as u64, 0).duration);
        let mut last = String::new();
        let mut done = false;
        for enc in candidates {
            match command(&app).args(convert_args(&src, &out, enc, kbps)).output() {
                Ok(o) if o.status.success() => {
                    done = true;
                    break;
                }
                Ok(o) => last = String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string(),
                Err(e) => last = e.to_string(),
            }
        }
        let bytes = if done { std::fs::read(&out).map_err(|e| e.to_string()) } else { Err(last) };
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
        bytes.map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(start: f64, crop: bool, quality: &str) -> ExportParams {
        ExportParams { name: "a.mp4".into(), start, end: start + 10.0, crop: crop.then_some(Crop { x: 11, y: 5, w: 641, h: 361 }), quality: quality.into(), target_kbps: None, audio: None, encoder: None }
    }

    #[test]
    fn jeu_dans_le_nom() {
        assert_eq!(game_of("Hades_2026-10-27_20-15-03.mp4"), "Hades");
        assert_eq!(game_of("Dead_Cells_2026-10-27_20-15-03.mp4"), "Dead_Cells");
        assert_eq!(game_of("vacances.mp4"), "");
    }

    #[test]
    fn infos_ffmpeg() {
        let t = "  Duration: 00:01:02.50, start: 0.000000, bitrate: 20000 kb/s\n  Stream #0:0: Video: h264 (High), yuv420p(tv), 1920x1080 [SAR 1:1 DAR 16:9], 60 fps\n";
        assert_eq!(parse_info(t), Meta { duration: 62.5, width: 1920, height: 1080, fps: 60.0, tracks: vec![] });
        assert_eq!(parse_info("  Duration: 00:00:01.00, start: 0\n  Stream #0:0: Video: h264, 1280x720, 29.97 fps, 30 tbr").fps, 29.97);
        assert_eq!(parse_info("rien"), Meta::default());
    }

    #[test]
    fn pistes_audio_nommees() {
        let t = "  Duration: 00:00:10.00, start: 0.000000, bitrate: 1 kb/s\n  Stream #0:0[0x1](und): Video: h264, 1280x720\n    Metadata:\n      handler_name    : VideoHandler\n  Stream #0:1[0x2](und): Audio: aac (LC), 48000 Hz, stereo\n    Metadata:\n      handler_name    : Mix\n  Stream #0:2[0x3](und): Audio: aac (LC), 48000 Hz, stereo\n    Metadata:\n      handler_name    : Micro\n  Stream #0:3[0x4](und): Audio: aac (LC), 48000 Hz, stereo\n";
        assert_eq!(parse_info(t).tracks, ["Mix", "Micro", "Piste 3"]);
    }

    #[test]
    fn images_cles() {
        let t = "[Parsed_showinfo_0 @ 0x1] n:   0 pts:      0 pts_time:0       pos: 1\n[Parsed_showinfo_0 @ 0x1] n:   1 pts:  90000 pts_time:1.5     pos: 2\n";
        assert_eq!(parse_keyframes(t), vec![0.0, 1.5]);
    }

    #[test]
    fn decoupe_sans_reencodage() {
        assert!(can_copy(&params(0.0, false, "original"), &[]));
        assert!(can_copy(&params(1.5, false, "original"), &[0.0, 1.52]));
        assert!(!can_copy(&params(1.0, false, "original"), &[0.0, 1.52]));
        assert!(!can_copy(&params(0.0, true, "original"), &[]));
        assert!(!can_copy(&params(0.0, false, "720"), &[]));
        assert_eq!(reencode_reason(&params(0.0, true, "original"), &[]), Some("recadrage"));
        assert_eq!(reencode_reason(&params(0.0, false, "1080"), &[]), Some("qualité réduite"));
        assert_eq!(reencode_reason(&params(1.0, false, "original"), &[0.0, 1.52]), Some("début hors image clé"));
        assert_eq!(reencode_reason(&params(1.5, false, "original"), &[0.0, 1.52]), None);
    }

    #[test]
    fn choix_de_l_encodeur() {
        let dispo = ["h264_nvenc", "h264_mf"];
        assert_eq!(pick_encoders(&dispo, None).unwrap(), dispo);
        assert_eq!(pick_encoders(&dispo, Some("auto")).unwrap(), dispo);
        assert_eq!(pick_encoders(&dispo, Some("h264_mf")).unwrap(), ["h264_mf"]);
        assert!(pick_encoders(&dispo, Some("h264_amf")).is_err());
    }

    #[test]
    fn arguments_d_export() {
        let (s, o) = (Path::new("a.mp4"), Path::new("b.mp4"));
        let copy = export_args(s, o, &params(2.0, false, "original"), true, "copy", 1080, 20000, None, &[]);
        assert!(copy.windows(2).any(|w| w == ["-c:v", "copy"]) && copy.windows(2).any(|w| w == ["-c:a", "copy"]) && !copy.contains(&"-vf".to_string()));
        let enc = export_args(s, o, &params(2.0, true, "720"), false, "h264_nvenc", 1080, 20000, None, &[]);
        let vf = enc.iter().position(|a| a == "-vf").map(|i| enc[i + 1].clone()).unwrap();
        assert_eq!(vf, "crop=640:360:10:4"); // 361 de haut ≤ 720 : pas de mise à l'échelle, dimensions paires
        assert!(enc.contains(&"h264_nvenc".to_string()));
        let big = ExportParams { crop: Some(Crop { x: 0, y: 0, w: 1920, h: 1080 }), ..params(0.0, false, "720") };
        let vf = export_args(s, o, &big, false, "h264_mf", 1080, 20000, None, &[]);
        assert!(vf.contains(&"crop=1920:1080:0:0,scale=-2:720".to_string()));
        // envoi d'un replay trop lourd : le débit vidéo imposé remplace celui de la source
        let sized = ExportParams { target_kbps: Some(5000), ..params(0.0, false, "720") };
        let a = export_args(s, o, &sized, false, "h264_nvenc", 1080, 20000, None, &[]);
        assert!(a.windows(2).any(|w| w == ["-b:v", "5000k"]));
        // marqueurs de l'extrait : les chapitres viennent du fichier de métadonnées, la durée reste une option de sortie
        let m = export_args(s, o, &params(2.0, false, "original"), true, "copy", 1080, 20000, Some(Path::new("m.ffmeta")), &[]);
        let (meta, dur) = (m.iter().position(|a| a == "m.ffmeta").unwrap(), m.iter().position(|a| a == "-t").unwrap());
        assert!(m[meta - 3..meta - 1] == ["-f", "ffmetadata"] && m.windows(2).any(|w| w == ["-map_chapters", "1"]) && dur > meta);
    }

    #[test]
    fn pistes_a_l_export() {
        let names: Vec<String> = ["Mix", "Jeu", "Vocal", "Micro"].map(String::from).into();
        let gain = |index, volume| TrackGain { index, volume };
        let (s, o) = (Path::new("a.mp4"), Path::new("b.mp4"));
        // vocal et micro coupés, volumes inchangés : pistes copiées telles quelles, sans filtre
        let sep = AudioMix { tracks: vec![gain(0, 1.0), gain(1, 1.0)], mixed: false };
        let (a, re) = audio_args(&sep, &names);
        assert!(!re && a.windows(2).any(|w| w == ["-map", "0:a:1"]) && !a.contains(&"0:a:2".to_string()) && !a.contains(&"-filter_complex".to_string()));
        // volume modifié : filtré et réencodé, une sortie par piste, nommée
        let (a, re) = audio_args(&AudioMix { tracks: vec![gain(1, 0.5), gain(3, 1.0)], mixed: false }, &names);
        assert!(re && a.contains(&"[0:a:1]volume=0.500[a0];[0:a:3]volume=1.000[a1]".to_string()) && a.contains(&"handler_name=Micro".to_string()));
        // une piste mixée : amix sans normalisation, volume plafonné à 2
        let (a, re) = audio_args(&AudioMix { tracks: vec![gain(1, 9.0), gain(2, f32::NAN)], mixed: true }, &names);
        assert!(re && a.iter().any(|x| x.contains("volume=2.000[a0];[0:a:2]volume=1.000[a1];[a0][a1]amix=inputs=2:normalize=0")) && a.contains(&"[out]".to_string()));
        // tout coupé : vidéo seule
        assert_eq!(audio_args(&AudioMix { tracks: vec![], mixed: true }, &names).0, ["-map", "0:v", "-an"]);
        // dans l'export : la découpe sans réencodage garde la vidéo en copie et réencode seulement l'audio filtré
        let p = ExportParams { audio: Some(AudioMix { tracks: vec![gain(1, 0.5)], mixed: true }), ..params(0.0, false, "original") };
        let args = export_args(s, o, &p, true, "copy", 1080, 20000, None, &names);
        assert!(args.windows(2).any(|w| w == ["-c:v", "copy"]) && args.windows(2).any(|w| w == ["-c:a", "aac"]));
    }

    #[test]
    fn noms_et_dossier() {
        assert_eq!(clean_name("  mon: clip?.mp4 ").as_deref(), Some("mon clip.mp4"));
        assert_eq!(clean_name("..."), None);
        let dir = std::env::temp_dir().join(format!("estompe-replays-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["a.mp4", "b.MP4", "c.txt", "d.mp4.part"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let mut names: Vec<String> = scan(&dir).into_iter().map(|f| f.0).collect();
        names.sort();
        assert_eq!(names, ["a.mp4", "b.MP4"]);
        assert!(file_in(&dir, "../x.mp4").is_err() && file_in(&dir, "a.txt").is_err() && file_in(&dir, "a.mp4").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn convert_bitrate_and_args() {
        assert_eq!(convert_kbps(1_000_000, 100.0), 2000);
        assert_eq!(convert_kbps(50_000_000, 60.0), 9333);
        assert_eq!(convert_kbps(u64::MAX / 2, 1.0), 12000);
        let a = convert_args(Path::new("in"), Path::new("out.mp4"), "h264_nvenc", 5000);
        assert!(a.windows(2).any(|w| w == ["-c:v", "h264_nvenc"]) && a.windows(2).any(|w| w == ["-b:v", "5000k"]));
        assert!(a.windows(2).any(|w| w == ["-movflags", "+faststart"]) && a.last().unwrap() == "out.mp4");
    }
}
