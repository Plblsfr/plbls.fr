//! Partage d'écran natif (prototype EST-18) : la source choisie dans la liste Estompe est capturée ici (Windows Graphics
//! Capture via `capture::run`), convertie en I420 puis publiée par un second participant LiveKit `<userId>#screen`,
//! sans repasser par le webview. Windows, cible MSVC et feature `livekit-native` uniquement ; ailleurs, les commandes
//! renvoient une erreur et l'interface garde le partage du webview.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// Drapeau d'arrêt du partage en cours.
#[derive(Default)]
#[cfg_attr(not(all(windows, feature = "livekit-native")), allow(dead_code))] // lu seulement par le partage natif
pub struct ScreenNative(Mutex<Option<Arc<AtomicBool>>>);

#[cfg(all(windows, feature = "livekit-native"))]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
    use livekit::track::{LocalAudioTrack, LocalTrack, LocalVideoTrack, TrackSource};
    use livekit::webrtc::native::yuv_helper::abgr_to_i420;
    use livekit::webrtc::prelude::{I420Buffer, RtcVideoSource, VideoEncoderBackend, VideoFrame, VideoResolution, VideoRotation};
    use livekit::webrtc::audio_frame::AudioFrame;
    use livekit::webrtc::audio_source::native::NativeAudioSource;
    use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
    use livekit::webrtc::video_source::native::NativeVideoSource;
    use livekit::{Room, RoomOptions};

    /// Image RGBA (octets R, G, B, A, soit « ABGR » pour libyuv) en I420, ramenée à `max_h` pixels de haut si besoin (0 = taille d'origine).
    fn to_i420(img: &image::RgbaImage, max_h: u32) -> I420Buffer {
        let (w, h) = img.dimensions();
        let mut buf = I420Buffer::new(w, h);
        let (sy, su, sv) = buf.strides();
        let (y, u, v) = buf.data_mut();
        abgr_to_i420(img.as_raw(), w * 4, y, sy, u, su, v, sv, w as i32, h as i32);
        if max_h > 0 && h > max_h {
            buf.scale((w as u64 * max_h as u64 / h as u64).max(2) as i32 & !1, max_h as i32 & !1)
        } else {
            buf
        }
    }

    const RATE: u32 = 48_000;
    const CHANNELS: u32 = 2;
    const FRAME: usize = (RATE / 100) as usize; // 10 ms par canal : taille imposée à la source sans file d'attente

    /// Son du partage (EST-52) : « process loopback » WASAPI (`son_wasapi::capturer`), converti en trames de 10 ms (PCM 16 bits) pour `source`.
    /// Fenêtre : le son de son processus (et de ses enfants) seul. Écran : tout le système sauf Estompe, pour ne pas renvoyer
    /// la voix des participants dans le partage (écho). `ready` reçoit le résultat de l'ouverture, puis la boucle tourne jusqu'à `stop`.
    fn capture_audio(pid: u32, include_tree: bool, source: NativeAudioSource, stop: Arc<AtomicBool>, ready: std::sync::mpsc::Sender<Result<(), String>>) {
        use crate::son_wasapi::{capturer, pcm16};
        let frame_bytes = FRAME * CHANNELS as usize * 4;
        capturer(
            || wasapi::AudioClient::new_application_loopback_client(pid, include_tree).map_err(|e| e.to_string()),
            CHANNELS as usize,
            &stop,
            |r| {
                let _ = ready.send(r);
            },
            |queue| {
                while queue.len() >= frame_bytes {
                    let bytes: Vec<u8> = queue.drain(..frame_bytes).collect();
                    let mut frame = AudioFrame::new(RATE, CHANNELS, FRAME as u32);
                    frame.data = pcm16(&bytes).into();
                    if tauri::async_runtime::block_on(source.capture_frame(&frame)).is_err() {
                        return false;
                    }
                }
                true
            },
        );
    }

    /// Renvoie les encodeurs vidéo disponibles (pour savoir si l'encodage matériel existe).
    #[allow(clippy::too_many_arguments)]
    pub async fn start(state: &ScreenNative, url: String, token: String, id: String, fps: u32, height: u32, bitrate: u64, audio: bool) -> Result<String, String> {
        stop(state);
        let (room, _events) = Room::connect(&url, &token, RoomOptions::default()).await.map_err(|e| e.to_string())?;
        // is_screencast : texte net plutôt qu'images fluides quand la bande passante manque
        let source = NativeVideoSource::new(VideoResolution { width: 1920, height: 1080 }, true);
        let track = LocalVideoTrack::create_video_track("écran", RtcVideoSource::Native(source.clone()));
        room.local_participant()
            .publish_track(LocalTrack::Video(track), TrackPublishOptions {
                    source: TrackSource::Screenshare,
                    video_codec: VideoCodec::H264,
                    simulcast: false,
                    video_encoding: Some(VideoEncoding { max_bitrate: bitrate, max_framerate: fps as f64 }),
                    video_encoder: VideoEncoderBackend::Hardware, // retombe sur le logiciel si aucun encodeur matériel n'est disponible
                    ..Default::default()
                })
            .await
            .map_err(|e| e.to_string())?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        if audio {
            // Fenêtre : son de son processus ; écran : tout sauf Estompe (pas d'écho de la voix des participants).
            let (pid, include) = crate::capture::window_pid(&id).map_or((std::process::id(), false), |p| (p, true));
            let source = NativeAudioSource::new(AudioSourceOptions::default(), RATE, CHANNELS, 0); // sans traitement : c'est du son d'application
            let track = LocalAudioTrack::create_audio_track("écran (son)", RtcAudioSource::Native(source.clone()));
            let (tx, rx) = std::sync::mpsc::channel();
            let flag = stop_flag.clone();
            std::thread::spawn(move || capture_audio(pid, include, source, flag, tx));
            let opened = rx.recv().map_err(|e| e.to_string()).and_then(|r| r);
            if let Err(e) = opened {
                stop_flag.store(true, Ordering::Relaxed);
                let _ = room.close().await;
                return Err(format!("capture du son impossible (Windows 10 2004 ou plus requis) : {e}"));
            }
            room.local_participant()
                .publish_track(LocalTrack::Audio(track), TrackPublishOptions { source: TrackSource::ScreenshareAudio, ..Default::default() })
                .await
                .map_err(|e| {
                    stop_flag.store(true, Ordering::Relaxed);
                    e.to_string()
                })?;
        }
        *state.0.lock().map_err(|e| e.to_string())? = Some(stop_flag.clone());
        tauri::async_runtime::spawn(async move {
            let flag = stop_flag.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                // `run` ne limite pas la cadence (120 : son seuil ferait tomber une image sur deux avec un écran à 60 Hz, soit ~20 images/s
                // irrégulières) ; elle est tenue ici sur une échéance qui avance de 1/fps à chaque image envoyée.
                let interval = Duration::from_micros(1_000_000 / fps.clamp(1, 120) as u64);
                let mut next = Instant::now();
                // Mesures pour le mode debug (EST-50) : images reçues de la capture, publiées, et durée moyenne / max de conversion + envoi.
                let (mut t0, mut got, mut sent, mut sum, mut max) = (Instant::now(), 0u32, 0u32, Duration::ZERO, Duration::ZERO);
                crate::capture::run(&id, &flag, &AtomicU32::new(120), |img| {
                    let now = Instant::now();
                    got += 1;
                    if now.duration_since(t0) >= Duration::from_secs(2) {
                        let s = now.duration_since(t0).as_secs_f64();
                        crate::debuglog::log("info", "natif/partage", &format!(
                            "mesures : capture {:.0} i/s ({}x{}), publié {:.0} i/s (cible {fps}), conversion+envoi moy. {:.1} ms, max {:.1} ms",
                            got as f64 / s, img.width(), img.height(), sent as f64 / s, sum.as_secs_f64() * 1000.0 / sent.max(1) as f64, max.as_secs_f64() * 1000.0));
                        (t0, got, sent, sum, max) = (now, 0, 0, Duration::ZERO, Duration::ZERO);
                    }
                    if now + interval / 4 < next {
                        return true;
                    }
                    next = (next + interval).max(now.checked_sub(interval).unwrap_or(now));
                    source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, to_i420(&img, height)));
                    let d = now.elapsed();
                    sent += 1;
                    sum += d;
                    max = max.max(d);
                    true
                })
            })
            .await;
            let _ = room.close().await;
        });
        Ok(format!("{:?}", VideoEncoderBackend::list_available().into_iter().collect::<Vec<_>>()))
    }

    pub fn stop(state: &ScreenNative) {
        if let Some(flag) = state.0.lock().ok().and_then(|mut g| g.take()) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(not(all(windows, feature = "livekit-native")))]
mod imp {
    use super::*;
    #[allow(clippy::too_many_arguments)]
    pub async fn start(_: &ScreenNative, _: String, _: String, _: String, _: u32, _: u32, _: u64, _: bool) -> Result<String, String> {
        Err("partage d'écran natif indisponible : installeur compilé sans la feature livekit-native (variable de dépôt NATIVE_ALPHA=1 absente)".into())
    }
    pub fn stop(_: &ScreenNative) {}
}

/// Publie la source `id` (écran `m:…` ou fenêtre `w:…`, voir `capture_sources`) avec le jeton du participant `#screen`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn screen_native_start(state: tauri::State<'_, ScreenNative>, url: String, token: String, id: String, fps: u32, height: u32, bitrate: u64, audio: bool) -> Result<String, String> {
    crate::debuglog::log("info", "natif/partage", &format!("démarrage : source {id}, {fps} i/s, {height} px, {bitrate} bit/s, son {audio}"));
    let r = imp::start(&state, url, token, id, fps, height, bitrate, audio).await;
    match &r {
        Ok(enc) => crate::debuglog::log("info", "natif/partage", &format!("publié, encodeurs {enc}")),
        Err(e) => crate::debuglog::log("error", "natif/partage", &format!("échec : {e}")),
    }
    r
}

#[tauri::command]
pub fn screen_native_stop(state: tauri::State<ScreenNative>) {
    crate::debuglog::log("info", "natif/partage", "arrêt");
    imp::stop(&state)
}
