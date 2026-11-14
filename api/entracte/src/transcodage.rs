use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::config::ModeMateriel;

pub const PLAYLIST: &str = "index.m3u8";

#[derive(Clone, Debug, PartialEq)]
pub struct Parametres {
    pub source: PathBuf,
    pub index_video: i64,
    pub index_audio: Option<i64>,
    pub hauteur: Option<i64>,
    pub copie_video: bool,
    pub debut: f64,
    pub avance: f64,
    pub mode: ModeMateriel,
    pub peripherique: String,
}

pub fn debit(hauteur: Option<i64>) -> u32 {
    match hauteur {
        None => 12_000,
        Some(h) if h >= 1080 => 8_000,
        Some(h) if h >= 720 => 4_000,
        Some(_) => 1_500,
    }
}

fn entree_materielle(mode: ModeMateriel, peripherique: &str) -> Vec<String> {
    let v: Vec<&str> = match mode {
        ModeMateriel::Cpu => vec![],
        ModeMateriel::Vaapi => vec![
            "-hwaccel",
            "vaapi",
            "-hwaccel_device",
            peripherique,
            "-hwaccel_output_format",
            "vaapi",
        ],
        ModeMateriel::Qsv => vec![
            "-hwaccel",
            "qsv",
            "-qsv_device",
            peripherique,
            "-hwaccel_output_format",
            "qsv",
        ],
        ModeMateriel::Nvenc => vec!["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"],
    };
    v.into_iter().map(String::from).collect()
}

fn encodeur(mode: ModeMateriel, hauteur: Option<i64>) -> Vec<String> {
    let (filtre, codec): (String, &[&str]) = match (mode, hauteur) {
        (ModeMateriel::Cpu, Some(h)) => (
            format!("scale=-2:{h},format=yuv420p"),
            &[
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-profile:v",
                "high",
            ],
        ),
        (ModeMateriel::Cpu, None) => (
            "format=yuv420p".into(),
            &[
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-profile:v",
                "high",
            ],
        ),
        (ModeMateriel::Vaapi, Some(h)) => (
            format!("scale_vaapi=w=-2:h={h}:format=nv12"),
            &["-c:v", "h264_vaapi"],
        ),
        (ModeMateriel::Vaapi, None) => ("scale_vaapi=format=nv12".into(), &["-c:v", "h264_vaapi"]),
        (ModeMateriel::Qsv, Some(h)) => (
            format!("scale_qsv=w=-2:h={h}"),
            &["-c:v", "h264_qsv", "-preset", "veryfast"],
        ),
        (ModeMateriel::Qsv, None) => (
            "scale_qsv=format=nv12".into(),
            &["-c:v", "h264_qsv", "-preset", "veryfast"],
        ),
        (ModeMateriel::Nvenc, Some(h)) => (
            format!("scale_cuda=-2:{h}:format=yuv420p"),
            &["-c:v", "h264_nvenc", "-preset", "p4"],
        ),
        (ModeMateriel::Nvenc, None) => (
            "scale_cuda=format=yuv420p".into(),
            &["-c:v", "h264_nvenc", "-preset", "p4"],
        ),
    };
    let b = debit(hauteur);
    let mut v = vec!["-vf".to_string(), filtre];
    v.extend(codec.iter().map(|s| s.to_string()));
    v.extend([
        "-b:v".into(),
        format!("{b}k"),
        "-maxrate".into(),
        format!("{b}k"),
        "-bufsize".into(),
        format!("{}k", b * 2),
        "-g".into(),
        "48".into(),
    ]);
    v
}

pub fn arguments(p: &Parametres, dossier: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if !p.copie_video {
        a.extend(entree_materielle(p.mode, &p.peripherique));
    }
    if p.debut > 0.0 {
        a.extend(["-ss".into(), format!("{:.6}", p.debut)]);
    }
    a.extend(["-i".into(), p.source.to_string_lossy().to_string()]);
    a.extend(["-map".into(), format!("0:{}", p.index_video)]);
    if let Some(i) = p.index_audio {
        a.extend(["-map".into(), format!("0:{i}")]);
    }
    if p.copie_video {
        a.extend(["-c:v".into(), "copy".into()]);
    } else {
        a.extend(encodeur(p.mode, p.hauteur));
    }
    if p.index_audio.is_some() {
        a.extend(
            ["-c:a", "aac", "-ac", "2", "-b:a", "160k"]
                .iter()
                .map(|s| s.to_string()),
        );
    }
    a.extend(
        [
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-f",
            "hls",
            "-hls_time",
            "4",
            "-hls_list_size",
            "0",
            "-hls_playlist_type",
            "event",
            "-hls_flags",
            "independent_segments+temp_file",
            "-hls_segment_type",
            "mpegts",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    a.extend([
        "-hls_segment_filename".into(),
        dossier.join("seg%05d.ts").to_string_lossy().to_string(),
        dossier.join(PLAYLIST).to_string_lossy().to_string(),
    ]);
    a
}

pub fn segment_valide(nom: &str) -> bool {
    nom.len() == 11
        && nom.starts_with("seg")
        && nom.ends_with(".ts")
        && nom[3..8].bytes().all(|b| b.is_ascii_digit())
}

struct Actif {
    processus: tokio::process::Child,
    dossier: PathBuf,
    parametres: Parametres,
    rang: u64,
}

pub struct Transcodeur {
    racine: PathBuf,
    actifs: Mutex<HashMap<String, Actif>>,
    rangs: AtomicU64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Etat {
    EnCours,
    Termine,
    Echec,
    Absent,
}

impl Transcodeur {
    pub fn nouveau(racine: PathBuf) -> Self {
        Self {
            racine,
            actifs: Mutex::default(),
            rangs: AtomicU64::new(0),
        }
    }

    pub async fn preparer(&self) {
        let _ = tokio::fs::remove_dir_all(&self.racine).await;
        if let Err(e) = tokio::fs::create_dir_all(&self.racine).await {
            tracing::error!("dossier de transcodage {} : {e}", self.racine.display());
        }
    }

    pub async fn demarrer(
        &self,
        ffmpeg: &str,
        flux: &str,
        p: &Parametres,
    ) -> Result<PathBuf, String> {
        self.arreter(flux).await;
        let dossier = self.racine.join(flux);
        tokio::fs::create_dir_all(&dossier)
            .await
            .map_err(|e| format!("dossier temporaire : {e}"))?;
        let processus = tokio::process::Command::new(ffmpeg)
            .args(arguments(p, &dossier))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("ffmpeg : {e}"))?;
        self.actifs.lock().unwrap().insert(
            flux.to_string(),
            Actif {
                processus,
                dossier: dossier.clone(),
                parametres: p.clone(),
                rang: self.rangs.fetch_add(1, Ordering::SeqCst),
            },
        );
        Ok(dossier)
    }

    pub async fn arreter(&self, flux: &str) {
        let actif = self.actifs.lock().unwrap().remove(flux);
        if let Some(mut a) = actif {
            let _ = a.processus.start_kill();
            let _ = a.processus.wait().await;
            let _ = tokio::fs::remove_dir_all(&a.dossier).await;
        }
    }

    pub fn etat(&self, flux: &str) -> Etat {
        let mut actifs = self.actifs.lock().unwrap();
        let Some(a) = actifs.get_mut(flux) else {
            return Etat::Absent;
        };
        match a.processus.try_wait() {
            Ok(None) => Etat::EnCours,
            Ok(Some(s)) if s.success() => Etat::Termine,
            _ => Etat::Echec,
        }
    }

    pub fn dossier(&self, flux: &str) -> Option<PathBuf> {
        self.actifs
            .lock()
            .unwrap()
            .get(flux)
            .map(|a| a.dossier.clone())
    }

    pub fn parametres(&self, flux: &str) -> Option<Parametres> {
        self.actifs
            .lock()
            .unwrap()
            .get(flux)
            .map(|a| a.parametres.clone())
    }

    pub fn identifiants(&self) -> Vec<String> {
        self.actifs.lock().unwrap().keys().cloned().collect()
    }

    pub fn nombre(&self) -> usize {
        self.actifs.lock().unwrap().len()
    }

    pub fn pid(&self, flux: &str) -> Option<u32> {
        self.actifs
            .lock()
            .unwrap()
            .get(flux)
            .and_then(|a| a.processus.id())
    }

    pub async fn taille(&self) -> u64 {
        let mut total = 0;
        let Ok(mut l) = tokio::fs::read_dir(&self.racine).await else {
            return 0;
        };
        while let Ok(Some(d)) = l.next_entry().await {
            if let Ok(mut f) = tokio::fs::read_dir(d.path()).await {
                while let Ok(Some(e)) = f.next_entry().await {
                    total += e.metadata().await.map(|m| m.len()).unwrap_or(0);
                }
            }
        }
        total
    }

    pub async fn plafonner(&self, max: u64) -> Vec<String> {
        let mut arretes = Vec::new();
        while self.taille().await > max {
            let plus_ancien = self
                .actifs
                .lock()
                .unwrap()
                .iter()
                .min_by_key(|(_, a)| a.rang)
                .map(|(id, _)| id.clone());
            let Some(id) = plus_ancien else { break };
            self.arreter(&id).await;
            arretes.push(id);
        }
        arretes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(mode: ModeMateriel, hauteur: Option<i64>) -> Parametres {
        Parametres {
            source: PathBuf::from("/medias/Film.mkv"),
            index_video: 0,
            index_audio: Some(2),
            hauteur,
            copie_video: false,
            debut: 125.5,
            avance: 0.0,
            mode,
            peripherique: "/dev/dri/renderD128".into(),
        }
    }

    fn ligne(p: &Parametres) -> String {
        arguments(p, Path::new("/tmp/e/f1")).join(" ")
    }

    #[test]
    fn ligne_cpu() {
        let l = ligne(&params(ModeMateriel::Cpu, Some(720)));
        assert!(l.starts_with("-nostdin -hide_banner -loglevel error -ss 125.500000 -i /medias/Film.mkv -map 0:0 -map 0:2 "), "{l}");
        assert!(l.contains("-vf scale=-2:720,format=yuv420p -c:v libx264 -preset veryfast -profile:v high -b:v 4000k -maxrate 4000k -bufsize 8000k"), "{l}");
        assert!(l.contains("-c:a aac -ac 2 -b:a 160k"), "{l}");
        assert!(
            l.ends_with("-hls_segment_filename /tmp/e/f1/seg%05d.ts /tmp/e/f1/index.m3u8"),
            "{l}"
        );
        assert!(!l.contains("hwaccel"), "{l}");
    }

    #[test]
    fn ligne_vaapi() {
        let l = ligne(&params(ModeMateriel::Vaapi, Some(1080)));
        assert!(l.contains("-hwaccel vaapi -hwaccel_device /dev/dri/renderD128 -hwaccel_output_format vaapi -ss"), "{l}");
        assert!(
            l.contains("-vf scale_vaapi=w=-2:h=1080:format=nv12 -c:v h264_vaapi -b:v 8000k"),
            "{l}"
        );
        let l = ligne(&params(ModeMateriel::Vaapi, None));
        assert!(
            l.contains("-vf scale_vaapi=format=nv12 -c:v h264_vaapi -b:v 12000k"),
            "{l}"
        );
    }

    #[test]
    fn ligne_qsv() {
        let l = ligne(&params(ModeMateriel::Qsv, Some(480)));
        assert!(
            l.contains("-hwaccel qsv -qsv_device /dev/dri/renderD128 -hwaccel_output_format qsv"),
            "{l}"
        );
        assert!(
            l.contains("-vf scale_qsv=w=-2:h=480 -c:v h264_qsv -preset veryfast -b:v 1500k"),
            "{l}"
        );
    }

    #[test]
    fn ligne_nvenc() {
        let l = ligne(&params(ModeMateriel::Nvenc, Some(720)));
        assert!(
            l.contains("-hwaccel cuda -hwaccel_output_format cuda"),
            "{l}"
        );
        assert!(!l.contains("renderD128"), "{l}");
        assert!(
            l.contains(
                "-vf scale_cuda=-2:720:format=yuv420p -c:v h264_nvenc -preset p4 -b:v 4000k"
            ),
            "{l}"
        );
    }

    #[test]
    fn ligne_copie_sans_audio_ni_debut() {
        let mut p = params(ModeMateriel::Vaapi, None);
        p.copie_video = true;
        p.index_audio = None;
        p.debut = 0.0;
        let l = ligne(&p);
        assert!(
            l.contains("-i /medias/Film.mkv -map 0:0 -c:v copy -sn"),
            "{l}"
        );
        assert!(
            !l.contains("hwaccel") && !l.contains("-ss") && !l.contains("-c:a"),
            "{l}"
        );
    }

    #[tokio::test]
    async fn plafond_depasse_en_cours_arrete_le_plus_ancien() {
        let racine = tempfile::tempdir().unwrap();
        let t = Transcodeur::nouveau(racine.path().to_path_buf());
        t.preparer().await;
        let p = params(ModeMateriel::Cpu, None);
        let a = t.demarrer("false", "a", &p).await.unwrap();
        let b = t.demarrer("false", "b", &p).await.unwrap();
        std::fs::write(a.join("seg00000.ts"), [0u8; 600]).unwrap();
        assert!(t.plafonner(1000).await.is_empty());
        assert_eq!(t.nombre(), 2);
        std::fs::write(b.join("seg00000.ts"), [0u8; 600]).unwrap();
        assert_eq!(t.plafonner(1000).await, vec!["a".to_string()]);
        assert_eq!(t.identifiants(), vec!["b".to_string()]);
        assert!(!a.exists() && b.exists());
        assert_eq!(t.taille().await, 600);
        let a = t.demarrer("false", "a", &p).await.unwrap();
        std::fs::write(a.join("seg00000.ts"), [0u8; 600]).unwrap();
        assert_eq!(t.plafonner(1000).await, vec!["b".to_string()]);
        assert_eq!(t.plafonner(0).await, vec!["a".to_string()]);
        assert_eq!(t.nombre(), 0);
    }

    #[test]
    fn noms_de_segment() {
        assert!(segment_valide("seg00012.ts"));
        for n in [
            "seg1.ts",
            "../seg00001.ts",
            "seg0001a.ts",
            "index.m3u8",
            "seg00001.ts.tmp",
        ] {
            assert!(!segment_valide(n), "{n}");
        }
    }
}
