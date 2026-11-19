use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use crate::config::ModeMateriel;

pub const PLAYLIST: &str = "index.m3u8";
pub const INIT: &str = "init.mp4";
pub const PAS_GRILLE: f64 = 4.0;
const DECALAGE_HORODATAGE: &str = "10";
const MARGE_PARTAGE: f64 = 4.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Parametres {
    pub source: PathBuf,
    pub index_video: i64,
    pub index_audio: Option<i64>,
    pub hauteur: Option<i64>,
    pub copie_video: bool,
    pub codec_video: String,
    pub debut: f64,
    pub origine: f64,
    pub mode: ModeMateriel,
    pub peripherique: String,
}

impl Parametres {
    pub fn sur_grille(&self) -> bool {
        !self.copie_video && self.mode == ModeMateriel::Cpu
    }

    pub fn fmp4(&self) -> bool {
        self.copie_video && self.codec_video != "h264"
    }

    pub fn meme_sortie(&self, autre: &Parametres) -> bool {
        self.source == autre.source
            && self.index_video == autre.index_video
            && self.index_audio == autre.index_audio
            && self.hauteur == autre.hauteur
            && self.copie_video == autre.copie_video
            && self.mode == autre.mode
    }

    pub fn serie(&self) -> String {
        if self.sur_grille() {
            "g".into()
        } else {
            format!("d{}", (self.origine * 1000.0).round() as i64)
        }
    }
}

pub fn depart_de_grille(position: f64) -> f64 {
    (position / PAS_GRILLE).floor().max(0.0) * PAS_GRILLE
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

fn encodeur(mode: ModeMateriel, hauteur: Option<i64>, grille: bool) -> Vec<String> {
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
    ]);
    if grille {
        v.extend(
            [
                "-g",
                "600",
                "-keyint_min",
                "600",
                "-sc_threshold",
                "0",
                "-force_key_frames",
                "expr:gte(t,n_forced*4)",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    } else {
        v.extend(["-g".into(), "48".into()]);
    }
    v
}

pub fn arguments(p: &Parametres, dossier: &Path) -> Vec<String> {
    let grille = p.sur_grille();
    let mut a: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if !p.copie_video {
        a.extend(entree_materielle(p.mode, &p.peripherique));
    }
    if grille {
        a.extend(["-ss".into(), format!("{:.6}", p.debut), "-copyts".into()]);
    } else if p.debut > 0.0 {
        a.extend(["-ss".into(), format!("{:.6}", p.debut)]);
    }
    a.extend(["-i".into(), p.source.to_string_lossy().to_string()]);
    a.extend(["-map".into(), format!("0:{}", p.index_video)]);
    if let Some(i) = p.index_audio {
        a.extend(["-map".into(), format!("0:{i}")]);
    }
    if p.copie_video {
        a.extend(["-c:v".into(), "copy".into()]);
        if p.codec_video == "hevc" {
            a.extend(["-tag:v".into(), "hvc1".into()]);
        }
    } else {
        a.extend(encodeur(p.mode, p.hauteur, grille));
    }
    if p.index_audio.is_some() {
        a.extend(
            ["-c:a", "aac", "-ac", "2", "-b:a", "160k"]
                .iter()
                .map(|s| s.to_string()),
        );
    }
    a.extend(
        ["-sn", "-dn", "-map_metadata", "-1", "-map_chapters", "-1"]
            .iter()
            .map(|s| s.to_string()),
    );
    if grille {
        a.extend(["-output_ts_offset".into(), DECALAGE_HORODATAGE.into()]);
    }
    a.extend(
        [
            "-f",
            "hls",
            "-hls_time",
            if grille { "3" } else { "4" },
            "-hls_list_size",
            "0",
            "-hls_playlist_type",
            "event",
            "-hls_flags",
            "independent_segments+temp_file",
            "-hls_segment_type",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    let extension = if p.fmp4() {
        a.extend(["fmp4".into(), "-hls_fmp4_init_filename".into(), INIT.into()]);
        "m4s"
    } else {
        a.push("mpegts".into());
        "ts"
    };
    if grille {
        a.extend([
            "-start_number".into(),
            format!("{}", (p.debut / PAS_GRILLE).round() as i64),
        ]);
    }
    a.extend([
        "-hls_segment_filename".into(),
        dossier
            .join(format!("seg%05d.{extension}"))
            .to_string_lossy()
            .to_string(),
        dossier.join(PLAYLIST).to_string_lossy().to_string(),
    ]);
    a
}

pub fn type_de_segment(nom: &str) -> Option<&'static str> {
    if nom == INIT {
        return Some("video/mp4");
    }
    let (corps, mime) = if let Some(c) = nom.strip_suffix(".ts") {
        (c, "video/mp2t")
    } else {
        (nom.strip_suffix(".m4s")?, "video/mp4")
    };
    let chiffres = corps.strip_prefix("seg")?;
    (chiffres.len() == 5 && chiffres.bytes().all(|b| b.is_ascii_digit())).then_some(mime)
}

pub fn segment_valide(nom: &str) -> bool {
    type_de_segment(nom).is_some()
}

pub fn duree_playlist(texte: &str) -> (f64, bool) {
    let duree = texte
        .lines()
        .filter_map(|l| l.strip_prefix("#EXTINF:"))
        .filter_map(|l| l.split(',').next()?.trim().parse::<f64>().ok())
        .sum();
    (duree, texte.contains("#EXT-X-ENDLIST"))
}

struct Tache {
    processus: tokio::process::Child,
    dossier: PathBuf,
    parametres: Parametres,
    depuis: SystemTime,
    lecteurs: usize,
    sommet: usize,
}

struct Abonne {
    tache: u64,
    avance: f64,
}

#[derive(Default)]
struct Registre {
    taches: HashMap<u64, Tache>,
    flux: HashMap<String, Abonne>,
}

impl Registre {
    fn tache(&mut self, flux: &str) -> Option<&mut Tache> {
        let rang = self.flux.get(flux)?.tache;
        self.taches.get_mut(&rang)
    }

    fn detacher(&mut self, flux: &str) -> Option<(u64, Tache)> {
        let rang = self.flux.remove(flux)?.tache;
        let t = self.taches.get_mut(&rang)?;
        t.lecteurs = t.lecteurs.saturating_sub(1);
        if t.lecteurs > 0 {
            return None;
        }
        self.taches.remove(&rang).map(|t| (rang, t))
    }

    fn attacher(&mut self, flux: &str, rang: u64, avance: f64) -> bool {
        let Some(t) = self.taches.get_mut(&rang) else {
            return false;
        };
        t.lecteurs += 1;
        t.sommet = t.sommet.max(t.lecteurs);
        self.flux.insert(
            flux.to_string(),
            Abonne {
                tache: rang,
                avance,
            },
        );
        true
    }
}

pub struct Transcodeur {
    racine: PathBuf,
    registre: Mutex<Registre>,
    rangs: AtomicU64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Etat {
    EnCours,
    Termine,
    Echec,
    Absent,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refus {
    Charge,
    Erreur(String),
}

fn nature(p: &Parametres) -> String {
    let palier = p
        .hauteur
        .map_or("originale".to_string(), |h| format!("{h}p"));
    if p.copie_video {
        format!("source {}, recopie, {palier}", p.codec_video)
    } else {
        format!(
            "source {}, réencodage {}, {palier}",
            p.codec_video,
            p.mode.code()
        )
    }
}

fn lancer(ffmpeg: &str, p: &Parametres, dossier: &Path) -> Result<tokio::process::Child, String> {
    tokio::process::Command::new(ffmpeg)
        .args(arguments(p, dossier))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("ffmpeg : {e}"))
}

impl Transcodeur {
    pub fn nouveau(racine: PathBuf) -> Self {
        Self {
            racine,
            registre: Mutex::default(),
            rangs: AtomicU64::new(0),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Registre> {
        self.registre.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub async fn preparer(&self) {
        let _ = tokio::fs::remove_dir_all(&self.racine).await;
        if let Err(e) = tokio::fs::create_dir_all(&self.racine).await {
            tracing::error!("dossier de transcodage {} : {e}", self.racine.display());
        }
    }

    async fn finir(&self, rang: u64, mut t: Tache) {
        let _ = t.processus.start_kill();
        let _ = t.processus.wait().await;
        let playlist = t.dossier.join(PLAYLIST);
        let (produit, _) = tokio::fs::read_to_string(&playlist)
            .await
            .map(|texte| duree_playlist(&texte))
            .unwrap_or_default();
        let ecoule = tokio::fs::metadata(&playlist)
            .await
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(t.depuis).ok())
            .map_or(0.0, |d| d.as_secs_f64());
        let vitesse = if ecoule > 0.0 { produit / ecoule } else { 0.0 };
        tracing::info!(
            "transcodage t{rang} arrêté : {}, {produit:.0} s produites en {ecoule:.0} s (x{vitesse:.1}), {} lecteur(s) au plus",
            nature(&t.parametres),
            t.sommet
        );
        let _ = tokio::fs::remove_dir_all(&t.dossier).await;
    }

    pub async fn partageable(&self, p: &Parametres, position: f64) -> Option<(u64, f64)> {
        let mut candidats: Vec<(u64, PathBuf, f64, bool)> = {
            let mut r = self.verrou();
            r.taches
                .iter_mut()
                .filter(|(_, t)| t.parametres.meme_sortie(p))
                .filter_map(|(rang, t)| {
                    let fini = match t.processus.try_wait() {
                        Ok(None) => false,
                        Ok(Some(s)) if s.success() => true,
                        _ => return None,
                    };
                    Some((*rang, t.dossier.clone(), t.parametres.origine, fini))
                })
                .filter(|(_, _, origine, _)| *origine <= position + 0.001)
                .collect()
        };
        candidats.sort_by(|a, b| b.2.total_cmp(&a.2));
        for (rang, dossier, origine, fini) in candidats {
            let (duree, _) = tokio::fs::read_to_string(dossier.join(PLAYLIST))
                .await
                .map(|texte| duree_playlist(&texte))
                .unwrap_or_default();
            let couvert = origine + duree;
            if (fini && position < couvert) || (!fini && position <= couvert + MARGE_PARTAGE) {
                return Some((rang, origine));
            }
        }
        None
    }

    pub async fn rejoindre(&self, flux: &str, rang: u64, avance: f64) -> bool {
        let (orpheline, ok) = {
            let mut r = self.verrou();
            if !r.taches.contains_key(&rang) {
                return false;
            }
            if let Some(a) = r.flux.get_mut(flux)
                && a.tache == rang
            {
                a.avance = avance;
                return true;
            }
            let orpheline = r.detacher(flux);
            let ok = r.attacher(flux, rang, avance);
            if ok && let Some(t) = r.taches.get(&rang) {
                tracing::info!("transcodage t{rang} partagé : {} lecteur(s)", t.lecteurs);
            }
            (orpheline, ok)
        };
        if let Some((r, t)) = orpheline {
            self.finir(r, t).await;
        }
        ok
    }

    pub async fn demarrer(
        &self,
        ffmpeg: &str,
        flux: &str,
        p: &Parametres,
        avance: f64,
        max: usize,
    ) -> Result<PathBuf, Refus> {
        self.arreter(flux).await;
        let rang = self.rangs.fetch_add(1, Ordering::SeqCst);
        let dossier = self.racine.join(format!("t{rang}"));
        tokio::fs::create_dir_all(&dossier)
            .await
            .map_err(|e| Refus::Erreur(format!("dossier temporaire : {e}")))?;
        let resultat = {
            let mut r = self.verrou();
            if r.taches.len() >= max {
                Err(Refus::Charge)
            } else {
                lancer(ffmpeg, p, &dossier)
                    .map(|processus| {
                        r.taches.insert(
                            rang,
                            Tache {
                                processus,
                                dossier: dossier.clone(),
                                parametres: p.clone(),
                                depuis: SystemTime::now(),
                                lecteurs: 0,
                                sommet: 0,
                            },
                        );
                        r.attacher(flux, rang, avance);
                    })
                    .map_err(Refus::Erreur)
            }
        };
        match resultat {
            Ok(()) => {
                tracing::info!(
                    "transcodage t{rang} démarré : {}, depuis {:.1} s",
                    nature(p),
                    p.origine
                );
                Ok(dossier)
            }
            Err(e) => {
                let _ = tokio::fs::remove_dir_all(&dossier).await;
                Err(e)
            }
        }
    }

    pub fn relancer(&self, ffmpeg: &str, flux: &str, p: &Parametres) -> Result<(), String> {
        let mut r = self.verrou();
        let t = r.tache(flux).ok_or("transcodage absent")?;
        let _ = t.processus.start_kill();
        let _ = std::fs::remove_file(t.dossier.join(PLAYLIST));
        t.processus = lancer(ffmpeg, p, &t.dossier)?;
        t.parametres = p.clone();
        t.depuis = SystemTime::now();
        Ok(())
    }

    pub async fn arreter(&self, flux: &str) {
        let orpheline = self.verrou().detacher(flux);
        if let Some((rang, t)) = orpheline {
            self.finir(rang, t).await;
        }
    }

    pub fn etat(&self, flux: &str) -> Etat {
        let mut r = self.verrou();
        let Some(t) = r.tache(flux) else {
            return Etat::Absent;
        };
        match t.processus.try_wait() {
            Ok(None) => Etat::EnCours,
            Ok(Some(s)) if s.success() => Etat::Termine,
            _ => Etat::Echec,
        }
    }

    pub fn dossier(&self, flux: &str) -> Option<PathBuf> {
        self.verrou().tache(flux).map(|t| t.dossier.clone())
    }

    pub fn parametres(&self, flux: &str) -> Option<Parametres> {
        self.verrou().tache(flux).map(|t| t.parametres.clone())
    }

    pub fn avance(&self, flux: &str) -> f64 {
        self.verrou().flux.get(flux).map_or(0.0, |a| a.avance)
    }

    pub fn lecteurs(&self, flux: &str) -> usize {
        self.verrou().tache(flux).map_or(0, |t| t.lecteurs)
    }

    pub fn identifiants(&self) -> Vec<String> {
        self.verrou().flux.keys().cloned().collect()
    }

    pub fn nombre(&self) -> usize {
        self.verrou().taches.len()
    }

    pub fn pid(&self, flux: &str) -> Option<u32> {
        self.verrou().tache(flux).and_then(|t| t.processus.id())
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
            let retiree = {
                let mut r = self.verrou();
                let Some(rang) = r.taches.keys().min().copied() else {
                    break;
                };
                let lecteurs: Vec<String> = r
                    .flux
                    .iter()
                    .filter(|(_, a)| a.tache == rang)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in &lecteurs {
                    r.flux.remove(id);
                }
                r.taches.remove(&rang).map(|t| (rang, t, lecteurs))
            };
            let Some((rang, t, mut lecteurs)) = retiree else {
                break;
            };
            self.finir(rang, t).await;
            lecteurs.sort();
            arretes.extend(lecteurs);
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
            codec_video: "h264".into(),
            debut: 124.0,
            origine: 124.0,
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
        assert!(l.starts_with("-nostdin -hide_banner -loglevel error -ss 124.000000 -copyts -i /medias/Film.mkv -map 0:0 -map 0:2 "), "{l}");
        assert!(l.contains("-vf scale=-2:720,format=yuv420p -c:v libx264 -preset veryfast -profile:v high -b:v 4000k -maxrate 4000k -bufsize 8000k -g 600 -keyint_min 600 -sc_threshold 0 -force_key_frames expr:gte(t,n_forced*4)"), "{l}");
        assert!(
            l.contains("-output_ts_offset 10 -f hls -hls_time 3 ")
                && l.contains("-hls_segment_type mpegts -start_number 31 "),
            "{l}"
        );
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
        assert!(
            l.contains("-hls_time 4 ") && l.ends_with("-hls_segment_type mpegts -hls_segment_filename /tmp/e/f1/seg%05d.ts /tmp/e/f1/index.m3u8"),
            "{l}"
        );
    }

    #[test]
    fn ligne_materielle_hors_grille() {
        let l = ligne(&params(ModeMateriel::Nvenc, Some(720)));
        assert!(
            l.contains("-ss 124.000000 -i ") && l.contains(" -g 48 ") && l.contains("-hls_time 4 "),
            "{l}"
        );
        assert!(
            !l.contains("-copyts")
                && !l.contains("-start_number")
                && !l.contains("force_key_frames"),
            "{l}"
        );
    }

    #[test]
    fn ligne_copie_hevc_en_fmp4() {
        let mut p = params(ModeMateriel::Cpu, None);
        p.copie_video = true;
        p.codec_video = "hevc".into();
        p.debut = 61.5;
        let l = ligne(&p);
        assert!(
            l.contains("-ss 61.500000 -i /medias/Film.mkv -map 0:0 -map 0:2 -c:v copy -tag:v hvc1 -c:a aac -ac 2 -b:a 160k"),
            "{l}"
        );
        assert!(
            l.ends_with("-hls_segment_type fmp4 -hls_fmp4_init_filename init.mp4 -hls_segment_filename /tmp/e/f1/seg%05d.m4s /tmp/e/f1/index.m3u8"),
            "{l}"
        );
        assert!(!l.contains("-copyts") && !l.contains("libx264"), "{l}");
        assert!(p.fmp4() && !p.sur_grille());
        assert_eq!(p.serie(), "d124000");
        assert_eq!(params(ModeMateriel::Cpu, Some(720)).serie(), "g");
    }

    #[test]
    fn grille_de_depart() {
        assert_eq!(depart_de_grille(0.0), 0.0);
        assert_eq!(depart_de_grille(3.99), 0.0);
        assert_eq!(depart_de_grille(125.5), 124.0);
        assert_eq!(depart_de_grille(-2.0), 0.0);
    }

    fn faux_ffmpeg(dossier: &Path) -> String {
        use std::os::unix::fs::PermissionsExt;
        let chemin = dossier.join("faux-ffmpeg");
        std::fs::write(&chemin, "#!/bin/sh\nexec sleep 60\n").unwrap();
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        chemin.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn un_seul_transcodage_pour_des_flux_identiques() {
        let racine = tempfile::tempdir().unwrap();
        let outils = tempfile::tempdir().unwrap();
        let ffmpeg = faux_ffmpeg(outils.path());
        let t = Transcodeur::nouveau(racine.path().to_path_buf());
        t.preparer().await;
        let p = params(ModeMateriel::Cpu, Some(720));
        let dossier = t.demarrer(&ffmpeg, "a", &p, 1.5, 1).await.unwrap();
        let (rang, origine) = t.partageable(&p, 125.5).await.unwrap();
        assert_eq!(origine, 124.0);
        assert!(t.rejoindre("b", rang, 1.5).await);
        assert_eq!((t.nombre(), t.lecteurs("a"), t.lecteurs("b")), (1, 2, 2));
        assert_eq!(t.pid("a"), t.pid("b"));
        assert_eq!(t.dossier("b"), Some(dossier.clone()));
        assert_eq!(t.avance("b"), 1.5);

        let mut autre_palier = p.clone();
        autre_palier.hauteur = Some(480);
        let mut autre_piste = p.clone();
        autre_piste.index_audio = Some(3);
        for autre in [&autre_palier, &autre_piste] {
            assert_eq!(t.partageable(autre, 125.5).await, None);
            assert_eq!(
                t.demarrer(&ffmpeg, "c", autre, 0.0, 1).await,
                Err(Refus::Charge)
            );
        }
        let autre = t
            .demarrer(&ffmpeg, "c", &autre_palier, 0.0, 2)
            .await
            .unwrap();
        assert_eq!((t.nombre(), t.lecteurs("c")), (2, 1));
        assert_ne!(autre, dossier);
        t.arreter("c").await;
        assert!(!autre.exists());

        t.arreter("a").await;
        assert_eq!((t.nombre(), t.lecteurs("b")), (1, 1));
        assert_eq!(t.etat("b"), Etat::EnCours);
        assert!(dossier.exists());
        assert_eq!(t.etat("a"), Etat::Absent);
        t.arreter("b").await;
        assert_eq!(t.nombre(), 0);
        assert!(!dossier.exists());
        assert!(!t.rejoindre("d", rang, 0.0).await);
    }

    #[tokio::test]
    async fn partage_selon_la_position_demandee() {
        let racine = tempfile::tempdir().unwrap();
        let outils = tempfile::tempdir().unwrap();
        let ffmpeg = faux_ffmpeg(outils.path());
        let t = Transcodeur::nouveau(racine.path().to_path_buf());
        t.preparer().await;
        let p = params(ModeMateriel::Cpu, Some(720));
        let dossier = t.demarrer(&ffmpeg, "a", &p, 0.0, 4).await.unwrap();
        assert!(t.partageable(&p, 123.0).await.is_none());
        assert!(t.partageable(&p, 124.0).await.is_some());
        assert!(t.partageable(&p, 128.0).await.is_some());
        assert!(t.partageable(&p, 129.0).await.is_none());
        std::fs::write(
            dossier.join(PLAYLIST),
            "#EXTM3U\n#EXTINF:4.004,\nseg00031.ts\n#EXTINF:96.0,\nseg00032.ts\n",
        )
        .unwrap();
        assert!(t.partageable(&p, 228.0).await.is_some());
        assert!(t.partageable(&p, 229.0).await.is_none());

        let mut plus_loin = p.clone();
        plus_loin.debut = 200.0;
        plus_loin.origine = 200.0;
        t.demarrer(&ffmpeg, "b", &plus_loin, 0.0, 4).await.unwrap();
        let (rang, origine) = t.partageable(&p, 201.0).await.unwrap();
        assert_eq!(origine, 200.0);
        assert!(t.rejoindre("a", rang, 1.0).await);
        assert_eq!((t.nombre(), t.lecteurs("a")), (1, 2));
        assert!(!dossier.exists());

        let fini = t
            .demarrer(
                "true",
                "c",
                &{
                    let mut q = p.clone();
                    q.hauteur = Some(480);
                    q
                },
                0.0,
                4,
            )
            .await
            .unwrap();
        let mut q = p.clone();
        q.hauteur = Some(480);
        std::fs::write(
            fini.join(PLAYLIST),
            "#EXTM3U\n#EXTINF:10.0,\nseg00031.ts\n#EXT-X-ENDLIST\n",
        )
        .unwrap();
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while t.etat("c") == Etat::EnCours {
            assert!(std::time::Instant::now() < limite);
            tokio::task::yield_now().await;
        }
        assert!(t.partageable(&q, 133.0).await.is_some());
        assert!(t.partageable(&q, 134.0).await.is_none());

        let echec = t
            .demarrer(
                "false",
                "d",
                &{
                    let mut q = p.clone();
                    q.hauteur = Some(1080);
                    q
                },
                0.0,
                4,
            )
            .await
            .unwrap();
        assert!(echec.exists());
        let mut q = p.clone();
        q.hauteur = Some(1080);
        while t.etat("d") == Etat::EnCours {
            assert!(std::time::Instant::now() < limite);
            tokio::task::yield_now().await;
        }
        assert!(t.partageable(&q, 124.0).await.is_none());
    }

    #[tokio::test]
    async fn plafond_depasse_en_cours_arrete_le_plus_ancien() {
        let racine = tempfile::tempdir().unwrap();
        let t = Transcodeur::nouveau(racine.path().to_path_buf());
        t.preparer().await;
        let p = params(ModeMateriel::Cpu, None);
        let a = t.demarrer("false", "a", &p, 0.0, 9).await.unwrap();
        let b = t.demarrer("false", "b", &p, 0.0, 9).await.unwrap();
        assert!(t.rejoindre("b2", 1, 0.0).await);
        std::fs::write(a.join("seg00000.ts"), [0u8; 600]).unwrap();
        assert!(t.plafonner(1000).await.is_empty());
        assert_eq!(t.nombre(), 2);
        std::fs::write(b.join("seg00000.ts"), [0u8; 600]).unwrap();
        assert_eq!(t.plafonner(1000).await, vec!["a".to_string()]);
        let mut restants = t.identifiants();
        restants.sort();
        assert_eq!(restants, vec!["b".to_string(), "b2".to_string()]);
        assert!(!a.exists() && b.exists());
        assert_eq!(t.taille().await, 600);
        let a = t.demarrer("false", "a", &p, 0.0, 9).await.unwrap();
        std::fs::write(a.join("seg00000.ts"), [0u8; 600]).unwrap();
        assert_eq!(
            t.plafonner(1000).await,
            vec!["b".to_string(), "b2".to_string()]
        );
        assert_eq!(t.plafonner(0).await, vec!["a".to_string()]);
        assert_eq!(t.nombre(), 0);
    }

    #[test]
    fn noms_de_segment() {
        assert_eq!(type_de_segment("seg00012.ts"), Some("video/mp2t"));
        assert_eq!(type_de_segment("seg00012.m4s"), Some("video/mp4"));
        assert_eq!(type_de_segment("init.mp4"), Some("video/mp4"));
        for n in [
            "init.mp4.tmp",
            "seg00012.mp4",
            "seg0001.m4s",
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
