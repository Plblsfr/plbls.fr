//! Écriture d'un replay sur disque, en arrière-plan et à priorité basse (CPU et E/S).
use crate::markers::{self, Marker};
use bobyn_buffer::{Codec, Packet};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};

/// `{jeu}_AAAA-MM-JJ_HH-MM-SS.mp4`, nom de jeu nettoyé des caractères interdits.
pub fn default_path(dir: &Path, name: &str) -> PathBuf {
    let t = unsafe { GetLocalTime() };
    let name: String = name.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    dir.join(format!("{name}_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.mp4", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond))
}

/// PCM d'une piste, calé sur la fenêtre de la vidéo (même longueur pour toutes).
pub struct Pcm {
    pub name: String,
    pub in_mix: bool,
    pub samples: Vec<i16>,
}

/// Les marqueurs deviennent des chapitres du MP4 et un fichier annexe pour la bibliothèque d'Estompe.
pub fn save(path: &Path, w: u32, h: u32, codec: Codec, packets: &[Packet], pcm: Vec<Pcm>, markers: &[Marker]) -> std::io::Result<()> {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let io = |e: windows::core::Error| std::io::Error::other(e.to_string());
    // Piste « Mix » en tête dès qu'il y a plusieurs sources, puis les pistes séparées.
    let mut tracks = pcm;
    if tracks.len() > 1 {
        let samples = bobyn_buffer::mix(&tracks.iter().filter(|t| t.in_mix).map(|t| t.samples.as_slice()).collect::<Vec<_>>());
        tracks.insert(0, Pcm { name: "Mix".into(), in_mix: false, samples });
    }
    let mut audio = Vec::new();
    for t in tracks {
        let frames = bobyn_audio::aac::encode(&t.samples).map_err(|e| {
            crate::log(&format!("audio : encodage AAC de « {} » en échec : {e}", t.name));
            io(e)
        })?;
        crate::log(&format!("audio : « {} » encodée, {} trames AAC", t.name, frames.len()));
        audio.push(bobyn_mux::Audio { name: t.name, frames });
    }
    let chapters: Vec<_> = markers.iter().map(|m| bobyn_mux::Chapter { start_us: (m.t * 1e6) as i64, title: m.name.clone() }).collect();
    bobyn_mux::write_mp4(BufWriter::new(std::fs::File::create(path)?), &bobyn_mux::Video { codec, width: w, height: h, packets }, &audio, &chapters)?;
    markers::write(path, markers)
}
