use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use estompe_plateforme::commande_d_arriere_plan;

use crate::fichiers::est_une_video;

pub const ENCODEURS_MATERIELS: [&str; 4] = ["h264_nvenc", "h264_amf", "h264_qsv", "h264_mf"];
pub const CODEC_COMPATIBLE: &str = "avc1";
pub const DOSSIER_CONVERSIONS: &str = "conversions";
const MOOV_MAX: u64 = 16 << 20;
const SCRUTATION: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurConversion {
    Annulee,
    Echec(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct FichierTemporaire(PathBuf);

impl FichierTemporaire {
    pub fn nouveau(chemin: PathBuf) -> Self {
        Self(chemin)
    }

    pub fn chemin(&self) -> &Path {
        &self.0
    }
}

impl Drop for FichierTemporaire {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn codec_du_moov(moov: &[u8]) -> Option<String> {
    let video = moov.windows(4).position(|f| f == b"vide")?;
    let stsd = video + moov[video..].windows(4).position(|f| f == b"stsd")?;
    moov.get(stsd + 16..stsd + 20)
        .map(|c| String::from_utf8_lossy(c).into_owned())
}

pub fn codec_mp4(lecteur: &mut (impl Read + Seek)) -> io::Result<Option<String>> {
    let fin = lecteur.seek(SeekFrom::End(0))?;
    let mut position = 0u64;
    while position.saturating_add(8) <= fin {
        lecteur.seek(SeekFrom::Start(position))?;
        let mut entete = Vec::with_capacity(16);
        lecteur.by_ref().take(16).read_to_end(&mut entete)?;
        let mut taille = u64::from(u32::from_be_bytes([
            entete[0], entete[1], entete[2], entete[3],
        ]));
        let mut tete = 8;
        if taille == 1 {
            let Some(grande) = entete.get(8..16) else {
                return Ok(None);
            };
            taille = u64::from_be_bytes(grande.try_into().unwrap_or_default());
            tete = 16;
        } else if taille == 0 {
            taille = fin - position;
        }
        if taille < tete {
            return Ok(None);
        }
        if &entete[4..8] == b"moov" {
            if taille > MOOV_MAX {
                return Ok(None);
            }
            lecteur.seek(SeekFrom::Start(position + tete))?;
            let mut moov = Vec::new();
            lecteur
                .by_ref()
                .take(taille - tete)
                .read_to_end(&mut moov)?;
            return Ok(codec_du_moov(&moov));
        }
        position = position.saturating_add(taille);
    }
    Ok(None)
}

pub fn a_convertir(type_mime: &str, codec: Option<&str>) -> bool {
    est_une_video(type_mime) && codec != Some(CODEC_COMPATIBLE)
}

pub fn duree_ffmpeg(texte: &str) -> f64 {
    texte
        .split("Duration: ")
        .nth(1)
        .and_then(|s| s.get(..11))
        .map_or(0.0, |d| {
            let mut p = d.split(':').map(|x| x.parse::<f64>().unwrap_or(0.0));
            p.next().unwrap_or(0.0) * 3600.0
                + p.next().unwrap_or(0.0) * 60.0
                + p.next().unwrap_or(0.0)
        })
}

pub fn debit_kbps(taille: u64, duree: f64) -> u32 {
    ((taille as f64 * 8.0 / 1000.0 / duree.max(1.0)) * 1.4).clamp(2000.0, 12000.0) as u32
}

pub fn arguments(source: &Path, sortie: &Path, encodeur: &str, kbps: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-nostats", "-progress", "pipe:1", "-i"]
        .map(String::from)
        .into();
    a.extend([
        source.to_string_lossy().into_owned(),
        "-c:v".into(),
        encodeur.into(),
        "-b:v".into(),
        format!("{kbps}k"),
        "-pix_fmt".into(),
        "yuv420p".into(),
    ]);
    a.extend(["-c:a", "aac", "-movflags", "+faststart", "-f", "mp4", "-y"].map(String::from));
    a.push(sortie.to_string_lossy().into_owned());
    a
}

pub fn avancement(ligne: &str, duree: f64) -> Option<f32> {
    if ligne.trim() == "progress=end" {
        return Some(1.0);
    }
    let micro: f64 = ligne.strip_prefix("out_time_us=")?.trim().parse().ok()?;
    (duree > 0.0).then(|| (micro / 1e6 / duree).clamp(0.0, 1.0) as f32)
}

pub fn encodeurs_materiels(liste: &str) -> Vec<String> {
    ENCODEURS_MATERIELS
        .into_iter()
        .filter(|e| liste.split_whitespace().any(|mot| mot == *e))
        .map(String::from)
        .collect()
}

fn nom_temporaire() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "estompe-h264-{}-{}.mp4",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Convertisseur {
    pub ffmpeg: PathBuf,
    pub encodeurs: Vec<String>,
    pub dossier: PathBuf,
}

impl Convertisseur {
    fn commande(&self) -> Command {
        let mut c = commande_d_arriere_plan(&self.ffmpeg);
        c.args(["-hide_banner", "-nostdin"]).stdin(Stdio::null());
        c
    }

    pub fn duree(&self, source: &Path) -> f64 {
        self.commande()
            .arg("-i")
            .arg(source)
            .stdout(Stdio::null())
            .output()
            .map(|o| duree_ffmpeg(&String::from_utf8_lossy(&o.stderr)))
            .unwrap_or(0.0)
    }

    pub fn sortie_temporaire(&self) -> io::Result<FichierTemporaire> {
        std::fs::create_dir_all(&self.dossier)?;
        Ok(FichierTemporaire(self.dossier.join(nom_temporaire())))
    }

    pub fn executer(
        &self,
        arguments: &[String],
        duree: f64,
        annule: &AtomicBool,
        progres: &(dyn Fn(f32) + Sync),
    ) -> Result<(), ErreurConversion> {
        if annule.load(Ordering::SeqCst) {
            return Err(ErreurConversion::Annulee);
        }
        let enfant = self
            .commande()
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ErreurConversion::Echec(e.to_string()))?;
        suivre(enfant, duree, annule, progres)
    }

    pub fn convertir(
        &self,
        source: &Path,
        annule: &AtomicBool,
        progres: &(dyn Fn(f32) + Sync),
    ) -> Result<FichierTemporaire, ErreurConversion> {
        let echec = |e: io::Error| ErreurConversion::Echec(e.to_string());
        std::fs::create_dir_all(&self.dossier).map_err(echec)?;
        let taille = std::fs::metadata(source).map_err(echec)?.len();
        let duree = self.duree(source);
        let kbps = debit_kbps(taille, duree);
        let sortie = FichierTemporaire(self.dossier.join(nom_temporaire()));
        let mut derniere = ErreurConversion::Echec("aucun encodeur H.264 disponible".into());
        for encodeur in &self.encodeurs {
            if annule.load(Ordering::SeqCst) {
                return Err(ErreurConversion::Annulee);
            }
            let enfant = self
                .commande()
                .args(arguments(source, sortie.chemin(), encodeur, kbps))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(echec)?;
            match suivre(enfant, duree, annule, progres) {
                Ok(()) => return Ok(sortie),
                Err(ErreurConversion::Annulee) => return Err(ErreurConversion::Annulee),
                Err(e) => derniere = e,
            }
        }
        Err(derniere)
    }
}

fn attendre_ou_arreter(enfant: &mut Child, annule: &AtomicBool) -> Option<ExitStatus> {
    loop {
        if annule.load(Ordering::SeqCst) {
            break;
        }
        match enfant.try_wait() {
            Ok(Some(statut)) => return Some(statut),
            Ok(None) => std::thread::sleep(SCRUTATION),
            Err(_) => break,
        }
    }
    let _ = enfant.kill();
    let _ = enfant.wait();
    None
}

fn suivre(
    mut enfant: Child,
    duree: f64,
    annule: &AtomicBool,
    progres: &(dyn Fn(f32) + Sync),
) -> Result<(), ErreurConversion> {
    let sortie = enfant.stdout.take();
    let erreurs = enfant.stderr.take();
    std::thread::scope(|s| {
        let lecteur = s.spawn(move || {
            for ligne in sortie
                .map(BufReader::new)
                .into_iter()
                .flat_map(|l| l.lines().map_while(Result::ok))
            {
                if let Some(fraction) = avancement(&ligne, duree) {
                    progres(fraction);
                }
            }
        });
        let messages = s.spawn(move || {
            let mut texte = String::new();
            if let Some(mut erreurs) = erreurs {
                let _ = erreurs.read_to_string(&mut texte);
            }
            texte
        });
        let statut = attendre_ou_arreter(&mut enfant, annule);
        let _ = lecteur.join();
        let texte = messages.join().unwrap_or_default();
        match statut {
            _ if annule.load(Ordering::SeqCst) => Err(ErreurConversion::Annulee),
            None => Err(ErreurConversion::Echec("ffmpeg interrompu".into())),
            Some(s) if s.success() => Ok(()),
            Some(_) => Err(ErreurConversion::Echec(
                texte
                    .lines()
                    .last()
                    .unwrap_or("échec de ffmpeg")
                    .to_string(),
            )),
        }
    })
}

pub fn ffmpeg_du_systeme() -> PathBuf {
    let nom = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&nom)))
        .filter(|c| c.is_file())
        .unwrap_or_else(|| PathBuf::from(nom))
}

pub struct Conversions {
    dossier: PathBuf,
    convertisseur: OnceLock<Option<Convertisseur>>,
}

impl Conversions {
    pub fn aucune() -> Self {
        Self {
            dossier: PathBuf::new(),
            convertisseur: OnceLock::from(None),
        }
    }

    pub fn du_systeme(dossier: PathBuf) -> Self {
        let _ = std::fs::remove_dir_all(&dossier);
        Self {
            dossier,
            convertisseur: OnceLock::new(),
        }
    }

    pub fn avec(convertisseur: Convertisseur) -> Self {
        let _ = std::fs::remove_dir_all(&convertisseur.dossier);
        Self {
            dossier: convertisseur.dossier.clone(),
            convertisseur: OnceLock::from(Some(convertisseur)),
        }
    }

    pub fn dossier(&self) -> &Path {
        &self.dossier
    }

    pub fn convertisseur(&self) -> Option<&Convertisseur> {
        self.convertisseur
            .get_or_init(|| {
                let ffmpeg = ffmpeg_du_systeme();
                let liste = commande_d_arriere_plan(&ffmpeg)
                    .args(["-hide_banner", "-encoders"])
                    .stdin(Stdio::null())
                    .output()
                    .ok()?;
                let encodeurs = encodeurs_materiels(&String::from_utf8_lossy(&liste.stdout));
                (!encodeurs.is_empty()).then(|| Convertisseur {
                    ffmpeg,
                    encodeurs,
                    dossier: self.dossier.clone(),
                })
            })
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Mutex;

    fn boite(nom: &[u8; 4], contenu: &[u8]) -> Vec<u8> {
        let mut b = ((contenu.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(nom);
        b.extend_from_slice(contenu);
        b
    }

    fn moov(codec: &[u8; 4]) -> Vec<u8> {
        let mut stsd = vec![0u8; 12];
        stsd.extend_from_slice(codec);
        let trak = [
            b"hdlr\0\0\0\0\0\0\0\0vide".as_slice(),
            &boite(b"stsd", &stsd),
        ]
        .concat();
        boite(b"moov", &boite(b"trak", &trak))
    }

    fn mp4(codec: &[u8; 4], moov_en_tete: bool) -> Vec<u8> {
        let ftyp = boite(b"ftyp", b"isom\0\0\0\0");
        let mdat = boite(b"mdat", &[0u8; 100]);
        if moov_en_tete {
            [ftyp, moov(codec), mdat].concat()
        } else {
            [ftyp, mdat, moov(codec)].concat()
        }
    }

    #[test]
    fn le_codec_est_lu_dans_le_moov_en_tete_ou_en_fin() {
        for en_tete in [true, false] {
            assert_eq!(
                codec_mp4(&mut Cursor::new(mp4(b"hvc1", en_tete))).unwrap(),
                Some("hvc1".into())
            );
        }
        assert_eq!(
            codec_mp4(&mut Cursor::new(mp4(b"avc1", false))).unwrap(),
            Some("avc1".into())
        );
        let sans_moov = [boite(b"ftyp", b"isom"), boite(b"mdat", &[0; 10])].concat();
        assert_eq!(codec_mp4(&mut Cursor::new(sans_moov)).unwrap(), None);
        assert_eq!(codec_mp4(&mut Cursor::new(vec![0u8; 3])).unwrap(), None);
        assert_eq!(codec_mp4(&mut Cursor::new(vec![0u8; 16])).unwrap(), None);
    }

    #[test]
    fn seul_un_mp4_qui_n_est_pas_en_h264_est_converti() {
        assert!(!a_convertir("video/mp4", Some("avc1")));
        assert!(a_convertir("video/mp4", Some("hvc1")));
        assert!(a_convertir("video/mp4", Some("av01")));
        assert!(a_convertir("video/mp4", None));
        assert!(!a_convertir("video/webm", Some("vp09")));
        assert!(!a_convertir("image/png", None));
    }

    #[test]
    fn debit_arguments_duree_et_avancement_comme_l_app_tauri() {
        assert_eq!(debit_kbps(1_000_000, 100.0), 2000);
        assert_eq!(debit_kbps(50_000_000, 60.0), 9333);
        assert_eq!(debit_kbps(u64::MAX / 2, 1.0), 12000);
        let a = arguments(Path::new("in"), Path::new("out.mp4"), "h264_nvenc", 5000);
        assert!(a.windows(2).any(|w| w == ["-c:v", "h264_nvenc"]));
        assert!(a.windows(2).any(|w| w == ["-b:v", "5000k"]));
        assert!(a.windows(2).any(|w| w == ["-pix_fmt", "yuv420p"]));
        assert!(a.windows(2).any(|w| w == ["-movflags", "+faststart"]));
        assert_eq!(a.last().unwrap(), "out.mp4");
        assert_eq!(
            duree_ffmpeg("  Duration: 00:01:02.50, start: 0.000000, bitrate: 20000 kb/s"),
            62.5
        );
        assert_eq!(duree_ffmpeg("rien"), 0.0);
        assert_eq!(avancement("out_time_us=31250000", 62.5), Some(0.5));
        assert_eq!(avancement("out_time_us=N/A", 62.5), None);
        assert_eq!(avancement("out_time_us=1000", 0.0), None);
        assert_eq!(avancement("progress=end", 0.0), Some(1.0));
        assert_eq!(
            encodeurs_materiels(
                " V....D h264_nvenc  NVIDIA\n V....D libx264 x\n V..... h264_mf MF"
            ),
            ["h264_nvenc", "h264_mf"]
        );
    }

    #[test]
    fn un_fichier_temporaire_disparait_avec_sa_piece() {
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join(nom_temporaire());
        std::fs::write(&chemin, b"x").unwrap();
        drop(FichierTemporaire(chemin.clone()));
        assert!(!chemin.exists());
        assert_ne!(nom_temporaire(), nom_temporaire());
    }

    #[test]
    fn sans_encodeur_aucune_conversion() {
        assert!(Conversions::aucune().convertisseur().is_none());
    }

    fn ffmpeg_de_test() -> Option<PathBuf> {
        let liste = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        if liste.contains("libx264") && liste.contains("libx265") {
            return Some(PathBuf::from("ffmpeg"));
        }
        assert!(
            std::env::var_os("ESTOMPE_FFMPEG_EXIGE").is_none(),
            "ffmpeg avec libx264 et libx265 exigé (ESTOMPE_FFMPEG_EXIGE)"
        );
        println!("ignoré : ffmpeg avec libx264 et libx265 indisponible");
        None
    }

    fn video_hevc(dossier: &Path, taille: &str, duree: u32) -> PathBuf {
        let chemin = dossier.join("hevc.mp4");
        let statut = Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!("testsrc=size={taille}:rate=25:duration={duree}"))
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=duration={duree}"))
            .args(["-c:v", "libx265", "-preset", "ultrafast", "-x265-params"])
            .args([
                "log-level=error",
                "-tag:v",
                "hvc1",
                "-c:a",
                "aac",
                "-shortest",
                "-y",
            ])
            .arg(&chemin)
            .status()
            .unwrap();
        assert!(statut.success());
        chemin
    }

    fn contenu(dossier: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dossier)
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default()
    }

    #[test]
    fn une_video_hevc_devient_un_mp4_h264_lisible_par_le_web() {
        let Some(ffmpeg) = ffmpeg_de_test() else {
            return;
        };
        let source = tempfile::tempdir().unwrap();
        let hevc = video_hevc(source.path(), "320x240", 2);
        assert_eq!(
            codec_mp4(&mut std::fs::File::open(&hevc).unwrap()).unwrap(),
            Some("hvc1".into())
        );
        let dossier = source.path().join(DOSSIER_CONVERSIONS);
        let conversions = Conversions::avec(Convertisseur {
            ffmpeg,
            encodeurs: vec!["h264_absent".into(), "libx264".into()],
            dossier: dossier.clone(),
        });
        let avancements = Mutex::new(Vec::new());
        let sortie = conversions
            .convertisseur()
            .unwrap()
            .convertir(&hevc, &AtomicBool::new(false), &|f| {
                avancements.lock().unwrap().push(f)
            })
            .unwrap();
        let octets = std::fs::read(sortie.chemin()).unwrap();
        assert_eq!(
            codec_mp4(&mut Cursor::new(&octets)).unwrap(),
            Some("avc1".into())
        );
        let position = |nom: &[u8]| octets.windows(4).position(|f| f == nom).unwrap();
        assert!(
            position(b"moov") < position(b"mdat"),
            "moov en tête (faststart)"
        );
        let probe = Command::new("ffmpeg")
            .args(["-hide_banner", "-i"])
            .arg(sortie.chemin())
            .output()
            .unwrap();
        let description = String::from_utf8_lossy(&probe.stderr);
        assert!(description.contains("Video: h264"), "{description}");
        assert!(description.contains("yuv420p"), "{description}");
        assert!(description.contains("Audio: aac"), "{description}");
        assert_eq!(avancements.lock().unwrap().last(), Some(&1.0));
        assert_eq!(contenu(&dossier), [sortie.chemin().to_path_buf()]);
        drop(sortie);
        assert!(contenu(&dossier).is_empty(), "fichier temporaire supprimé");
        println!("317 C1 : vidéo HEVC convertie en H.264 (yuv420p, AAC, moov en tête)");
    }

    #[test]
    fn annuler_la_conversion_ne_laisse_aucun_fichier() {
        let Some(ffmpeg) = ffmpeg_de_test() else {
            return;
        };
        let source = tempfile::tempdir().unwrap();
        let hevc = video_hevc(source.path(), "640x480", 10);
        let dossier = source.path().join(DOSSIER_CONVERSIONS);
        let convertisseur = Convertisseur {
            ffmpeg,
            encodeurs: vec!["libx264".into()],
            dossier: dossier.clone(),
        };
        let annule = AtomicBool::new(false);
        let resultat =
            convertisseur.convertir(&hevc, &annule, &|_| annule.store(true, Ordering::SeqCst));
        assert_eq!(resultat, Err(ErreurConversion::Annulee));
        assert!(contenu(&dossier).is_empty(), "aucun fichier temporaire");
        assert!(hevc.is_file(), "l'original est gardé");
        println!("317 C3 : conversion annulée, aucun fichier temporaire");
    }
}
