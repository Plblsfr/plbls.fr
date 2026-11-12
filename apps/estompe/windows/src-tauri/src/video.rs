use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

fn embedded_ffmpeg(resources: Option<PathBuf>) -> Result<PathBuf, String> {
    resources.map(|d| d.join("ffmpeg.exe")).filter(|exe| exe.is_file()).ok_or_else(|| "ffmpeg embarqué introuvable : réinstaller Estompe".to_string())
}

fn command(app: &AppHandle) -> Result<Command, String> {
    let mut c = Command::new(embedded_ffmpeg(app.path().resource_dir().ok())?);
    c.args(["-hide_banner", "-nostdin"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000 | 0x0000_4000);
    }
    Ok(c)
}

fn temp_tag() -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!("estompe-h264-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

fn parse_duration(text: &str) -> f64 {
    text.split("Duration: ").nth(1).and_then(|s| s.get(..11)).map_or(0.0, |d| {
        let mut p = d.split(':').map(|x| x.parse::<f64>().unwrap_or(0.0));
        p.next().unwrap_or(0.0) * 3600.0 + p.next().unwrap_or(0.0) * 60.0 + p.next().unwrap_or(0.0)
    })
}

fn duration(app: &AppHandle, path: &Path) -> f64 {
    let out = command(app).ok().and_then(|mut c| c.arg("-i").arg(path).output().ok()).map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default();
    parse_duration(&out)
}

fn encoders(app: &AppHandle) -> &'static Vec<&'static str> {
    static E: OnceLock<Vec<&'static str>> = OnceLock::new();
    E.get_or_init(|| {
        let out = command(app).ok().and_then(|mut c| c.arg("-encoders").output().ok()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        ["h264_nvenc", "h264_amf", "h264_qsv", "h264_mf"].into_iter().filter(|e| out.contains(e)).collect()
    })
}

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

#[tauri::command]
pub async fn video_to_h264(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<tauri::ipc::Response, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else { return Err("octets attendus".into()) };
    let data = data.clone();
    tauri::async_runtime::spawn_blocking(move || {
        command(&app)?;
        let candidates = encoders(&app);
        if candidates.is_empty() {
            return Err("aucun encodeur matériel H.264 disponible".to_string());
        }
        let tag = temp_tag();
        let (src, out) = (std::env::temp_dir().join(format!("{tag}.in")), std::env::temp_dir().join(format!("{tag}.mp4")));
        std::fs::write(&src, &data).map_err(|e| e.to_string())?;
        let kbps = convert_kbps(data.len() as u64, duration(&app, &src));
        let mut last = String::new();
        let mut done = false;
        for enc in candidates {
            match command(&app).and_then(|mut c| c.args(convert_args(&src, &out, enc, kbps)).output().map_err(|e| e.to_string())) {
                Ok(o) if o.status.success() => {
                    done = true;
                    break;
                }
                Ok(o) => last = String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string(),
                Err(e) => last = e,
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

    #[test]
    fn ffmpeg_embarque_seulement() {
        let dir = std::env::temp_dir().join(format!("{}-ressources", temp_tag()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(embedded_ffmpeg(Some(dir.clone())).unwrap_err().contains("ffmpeg embarqué introuvable"));
        assert!(embedded_ffmpeg(None).is_err());
        std::fs::write(dir.join("ffmpeg.exe"), b"").unwrap();
        assert_eq!(embedded_ffmpeg(Some(dir.clone())), Ok(dir.join("ffmpeg.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fichiers_temporaires_distincts() {
        let (a, b) = (temp_tag(), temp_tag());
        assert_ne!(a, b);
        assert!(a.starts_with(&format!("estompe-h264-{}-", std::process::id())));
    }

    #[test]
    fn duree_ffmpeg() {
        assert_eq!(parse_duration("  Duration: 00:01:02.50, start: 0.000000, bitrate: 20000 kb/s"), 62.5);
        assert_eq!(parse_duration("rien"), 0.0);
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
