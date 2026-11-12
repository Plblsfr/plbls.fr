use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::types::{Clip, Codec, Stockage};

pub const ANNEXES: &str = ".estompe";
pub const ENVOIS: &str = "envoi";
pub const PISTES_MAX: usize = 4;
const INTERDITS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVES: [&str; 22] = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Annexe {
    pub jeu: String,
    pub date: String,
    pub duree_s: f64,
    pub codec: Codec,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub pistes_audio: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origine: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Horodatage {
    pub annee: i64,
    pub mois: u32,
    pub jour: u32,
    pub heure: u32,
    pub minute: u32,
    pub seconde: u32,
}

impl Horodatage {
    pub fn depuis_epoque(secondes: i64) -> Self {
        let (jours, reste) = (secondes.div_euclid(86_400), secondes.rem_euclid(86_400) as u32);
        let z = jours + 719_468;
        let (ere, jour_ere) = (z.div_euclid(146_097), z.rem_euclid(146_097));
        let annee_ere = (jour_ere - jour_ere / 1460 + jour_ere / 36_524 - jour_ere / 146_096) / 365;
        let jour_annee = jour_ere - (365 * annee_ere + annee_ere / 4 - annee_ere / 100);
        let m = (5 * jour_annee + 2) / 153;
        let mois = if m < 10 { m + 3 } else { m - 9 };
        Self {
            annee: annee_ere + ere * 400 + i64::from(mois <= 2),
            mois: mois as u32,
            jour: (jour_annee - (153 * m + 2) / 5 + 1) as u32,
            heure: reste / 3600,
            minute: reste % 3600 / 60,
            seconde: reste % 60,
        }
    }

    #[cfg(windows)]
    pub fn maintenant() -> Self {
        let mut t = unsafe { std::mem::zeroed() };
        unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
        Self { annee: t.wYear as i64, mois: t.wMonth as u32, jour: t.wDay as u32, heure: t.wHour as u32, minute: t.wMinute as u32, seconde: t.wSecond as u32 }
    }

    #[cfg(not(windows))]
    pub fn maintenant() -> Self {
        Self::depuis_epoque(UNIX_EPOCH.elapsed().map_or(0, |d| d.as_secs() as i64))
    }

    pub fn pour_nom(&self) -> String {
        format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde)
    }

    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde)
    }
}

pub fn nettoyer(nom: &str) -> String {
    let propre: String = nom.chars().filter(|c| !INTERDITS.contains(c) && !c.is_control()).collect();
    let propre: String = propre.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
    let propre = propre.trim_end_matches(['.', ' ']).to_string();
    let racine = propre.split('.').next().unwrap_or("").trim();
    if RESERVES.iter().any(|r| r.eq_ignore_ascii_case(racine)) {
        format!("_{propre}")
    } else {
        propre
    }
}

pub fn nom_clip(jeu: &str, date: &Horodatage) -> String {
    let jeu = nettoyer(jeu);
    format!("{} {}", if jeu.is_empty() { "Jeu" } else { &jeu }, date.pour_nom())
}

pub fn chemin_libre(dossier: &Path, base: &str) -> PathBuf {
    (1u32..)
        .map(|n| dossier.join(if n == 1 { format!("{base}.mp4") } else { format!("{base} ({n}).mp4") }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dossier.join(format!("{base}.mp4")))
}

pub fn dossier_effectif(reglage: Option<&str>, videos: Option<PathBuf>) -> Result<PathBuf, String> {
    match reglage {
        Some(d) => Ok(PathBuf::from(d)),
        None => videos.map(|v| v.join("Estompe")).ok_or_else(|| "dossier Vidéos introuvable : choisir un dossier pour les clips".to_string()),
    }
}

pub fn annexe(dossier: &Path, clip: &Path, extension: &str) -> PathBuf {
    let nom = clip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    dossier.join(ANNEXES).join(format!("{nom}.{extension}"))
}

pub fn extensions() -> Vec<String> {
    let fixes = ["json", "jpg", "montage.json", "apercu.mp4"].map(String::from);
    fixes.into_iter().chain((0..PISTES_MAX).map(|i| format!("piste{i}.m4a"))).collect()
}

pub fn creer_annexes(dossier: &Path) -> std::io::Result<PathBuf> {
    let annexes = dossier.join(ANNEXES);
    if !annexes.is_dir() {
        fs::create_dir_all(&annexes)?;
        masquer(&annexes);
    }
    Ok(annexes)
}

#[cfg(windows)]
fn masquer(chemin: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};
    let large: Vec<u16> = chemin.as_os_str().encode_wide().chain([0]).collect();
    unsafe { SetFileAttributesW(large.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };
}

#[cfg(not(windows))]
fn masquer(_: &Path) {}

fn est_mp4(chemin: &Path) -> bool {
    chemin.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) && chemin.is_file()
}

fn fichiers(dossier: &Path) -> Vec<(PathBuf, u64, i64)> {
    let entrees = fs::read_dir(dossier).into_iter().flatten().flatten();
    let mut liste: Vec<(PathBuf, u64, i64)> = entrees
        .filter(|e| est_mp4(&e.path()))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let modifie = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
            Some((e.path(), m.len(), modifie))
        })
        .collect();
    liste.sort_by(|a, b| (a.2, &a.0).cmp(&(b.2, &b.0)));
    liste
}

pub fn lister(dossier: &Path) -> Vec<Clip> {
    let clips = fichiers(dossier).into_iter().rev().map(|(chemin, taille, modifie)| {
        let a: Option<Annexe> = fs::read(annexe(dossier, &chemin, "json")).ok().and_then(|o| serde_json::from_slice(&o).ok());
        let vignette = Some(annexe(dossier, &chemin, "jpg")).filter(|v| v.is_file());
        Clip {
            nom: chemin.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            chemin: chemin.to_string_lossy().into_owned(),
            date: a.as_ref().map_or_else(|| format!("{}Z", Horodatage::depuis_epoque(modifie).iso()), |a| a.date.clone()),
            duree_s: a.as_ref().map_or(0.0, |a| a.duree_s),
            taille_octets: taille,
            vignette: vignette.map(|v| v.to_string_lossy().into_owned()),
            codec: a.as_ref().map(|a| a.codec),
            largeur: a.as_ref().map_or(0, |a| a.largeur),
            hauteur: a.as_ref().map_or(0, |a| a.hauteur),
            ips: a.as_ref().map_or(0, |a| a.ips),
            pistes_audio: a.as_ref().map_or(0, |a| a.pistes_audio),
            origine: a.as_ref().and_then(|a| a.origine.clone()),
            jeu: a.map(|a| a.jeu),
        }
    });
    clips.collect()
}

pub fn lire_annexe(dossier: &Path, clip: &Path) -> Option<Annexe> {
    serde_json::from_slice(&fs::read(annexe(dossier, clip, "json")).ok()?).ok()
}

pub fn dans_dossier(dossier: &Path, chemin: &str) -> Result<PathBuf, String> {
    let refus = || "chemin refusé : hors du dossier des clips".to_string();
    let racine = fs::canonicalize(dossier).map_err(|_| "dossier des clips inaccessible".to_string())?;
    let reel = fs::canonicalize(chemin).map_err(|_| "clip introuvable".to_string())?;
    if reel.parent() != Some(racine.as_path()) || !est_mp4(&reel) {
        return Err(refus());
    }
    reel.file_name().map(|n| dossier.join(n)).ok_or_else(refus)
}

pub fn renommer(dossier: &Path, chemin: &str, nom: &str) -> Result<PathBuf, String> {
    let source = dans_dossier(dossier, chemin)?;
    let sans_extension = nom.trim();
    let sans_extension = if sans_extension.to_ascii_lowercase().ends_with(".mp4") { &sans_extension[..sans_extension.len() - 4] } else { sans_extension };
    let propre = nettoyer(sans_extension);
    if propre.is_empty() {
        return Err("nom de clip invalide".into());
    }
    let cible = dossier.join(format!("{propre}.mp4"));
    if cible == source {
        return Ok(source);
    }
    if cible.exists() {
        return Err("un clip porte déjà ce nom".into());
    }
    fs::rename(&source, &cible).map_err(|e| format!("renommage impossible : {e}"))?;
    for extension in extensions() {
        let _ = fs::rename(annexe(dossier, &source, &extension), annexe(dossier, &cible, &extension));
    }
    Ok(cible)
}

pub fn supprimer(dossier: &Path, chemins: &[String]) -> Result<(), String> {
    let clips: Vec<PathBuf> = chemins.iter().map(|c| dans_dossier(dossier, c)).collect::<Result<_, _>>()?;
    for clip in clips {
        fs::remove_file(&clip).map_err(|e| format!("suppression impossible : {e}"))?;
        for extension in extensions() {
            let _ = fs::remove_file(annexe(dossier, &clip, &extension));
        }
    }
    Ok(())
}

fn taille_dossier(dossier: &Path) -> u64 {
    let entrees = fs::read_dir(dossier).into_iter().flatten().flatten();
    entrees.map(|e| if e.path().is_dir() { taille_dossier(&e.path()) } else { e.metadata().map_or(0, |m| m.len()) }).sum()
}

pub fn utilise(dossier: &Path) -> u64 {
    fichiers(dossier).iter().map(|f| f.1).sum::<u64>() + taille_dossier(&dossier.join(ANNEXES))
}

pub fn plus_anciens(clips: &[(PathBuf, u64, i64)], utilise: u64, quota: u64) -> Vec<String> {
    let mut reste = utilise;
    let a_liberer = clips.iter().take_while(|c| {
        let depasse = reste >= quota;
        reste = reste.saturating_sub(c.1);
        depasse
    });
    a_liberer.map(|c| c.0.to_string_lossy().into_owned()).collect()
}

pub fn stockage(dossier: &Path, quota: u64) -> Stockage {
    let utilise = utilise(dossier);
    Stockage { utilise_octets: utilise, quota_octets: quota, depasse: utilise >= quota, plus_anciens: plus_anciens(&fichiers(dossier), utilise, quota) }
}

pub fn purger_envois(dossier: &Path) {
    let _ = fs::remove_dir_all(dossier.join(ANNEXES).join(ENVOIS));
}

pub fn debit_pour_taille(taille_max: u64, duree_s: f64, audio_kbps: u32) -> Option<u32> {
    let total = taille_max as f64 * 8.0 / 1000.0 / duree_s.max(1.0) * 0.95;
    let video = total - audio_kbps as f64;
    (video >= 300.0).then_some(video as u32)
}

pub fn hauteur_pour_debit(kbps: u32, hauteur: u32) -> Option<u32> {
    let cible = match kbps {
        0..1200 => 480,
        1200..3000 => 720,
        3000..6000 => 1080,
        _ => return None,
    };
    (hauteur > cible).then_some(cible)
}

pub fn arguments_envoi(source: &Path, sortie: &Path, video: Option<(&str, u32, Option<u32>)>, pistes_audio: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-i"].map(String::from).into();
    a.push(source.to_string_lossy().into_owned());
    if pistes_audio > 1 {
        a.extend(["-filter_complex".into(), format!("amix=inputs={pistes_audio}:normalize=0[son]"), "-map".into(), "0:v:0".into(), "-map".into(), "[son]".into()]);
    }
    match video {
        None => a.extend(["-c:v".into(), "copy".into()]),
        Some((encodeur, kbps, hauteur)) => {
            if let Some(h) = hauteur {
                a.extend(["-vf".into(), format!("scale=-2:{h}")]);
            }
            a.extend(["-c:v".into(), encodeur.into(), "-b:v".into(), format!("{kbps}k"), "-maxrate".into(), format!("{kbps}k"), "-bufsize".into(), format!("{}k", kbps * 2), "-pix_fmt".into(), "yuv420p".into()]);
        }
    }
    a.extend(["-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", "-f", "mp4", "-y"].map(String::from));
    a.push(sortie.to_string_lossy().into_owned());
    a
}

pub fn arguments_vignette(source: &Path, sortie: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-i"].map(String::from).into();
    a.push(source.to_string_lossy().into_owned());
    a.extend(["-frames:v", "1", "-vf", "scale=480:-2", "-q:v", "5", "-f", "mjpeg", "-y"].map(String::from));
    a.push(sortie.to_string_lossy().into_owned());
    a
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn dossier_test(nom: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("estompe-replay-{}-{n}-{nom}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn clip(dossier: &Path, nom: &str, taille: usize) -> String {
        let chemin = dossier.join(nom);
        fs::write(&chemin, vec![0u8; taille]).unwrap();
        chemin.to_string_lossy().into_owned()
    }

    #[test]
    fn nom_de_fichier_propre() {
        let date = Horodatage { annee: 2026, mois: 3, jour: 7, heure: 9, minute: 5, seconde: 2 };
        assert_eq!(nom_clip("Counter-Strike 2", &date), "Counter-Strike 2 2026-03-07 09-05-02");
        assert_eq!(nom_clip(r#"Jeu: le <retour> / "2" | a\b?*"#, &date), "Jeu le retour 2 ab 2026-03-07 09-05-02");
        assert_eq!(nom_clip("  \t ", &date), "Jeu 2026-03-07 09-05-02");
        assert_eq!(nettoyer("con"), "_con");
        assert_eq!(nettoyer("LPT1.txt"), "_LPT1.txt");
        assert_eq!(nettoyer("fin. . "), "fin");
        assert_eq!(nettoyer(&"é".repeat(200)).chars().count(), 80);
        assert_eq!(nettoyer("a\u{0}b\nc"), "abc");
    }

    #[test]
    fn dates_civiles() {
        assert_eq!(Horodatage::depuis_epoque(0).iso(), "1970-01-01T00:00:00");
        assert_eq!(Horodatage::depuis_epoque(1_709_251_199).iso(), "2024-02-29T23:59:59");
        assert_eq!(Horodatage::depuis_epoque(1_794_441_600).pour_nom(), "2026-11-12 00-00-00");
        assert_eq!(Horodatage::depuis_epoque(-1).iso(), "1969-12-31T23:59:59");
    }

    #[test]
    fn collisions_de_noms() {
        let d = dossier_test("collisions");
        assert_eq!(chemin_libre(&d, "Jeu 1"), d.join("Jeu 1.mp4"));
        clip(&d, "Jeu 1.mp4", 1);
        assert_eq!(chemin_libre(&d, "Jeu 1"), d.join("Jeu 1 (2).mp4"));
        clip(&d, "Jeu 1 (2).mp4", 1);
        assert_eq!(chemin_libre(&d, "Jeu 1"), d.join("Jeu 1 (3).mp4"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn dossier_par_defaut_ou_choisi() {
        assert_eq!(dossier_effectif(None, Some(PathBuf::from("/v"))), Ok(PathBuf::from("/v/Estompe")));
        assert_eq!(dossier_effectif(Some("/clips"), None), Ok(PathBuf::from("/clips")));
        assert!(dossier_effectif(None, None).is_err());
    }

    #[test]
    fn jamais_hors_du_dossier_des_clips() {
        let racine = dossier_test("galerie");
        let d = racine.join("clips");
        fs::create_dir_all(d.join("sous")).unwrap();
        let dehors = clip(&racine, "dehors.mp4", 3);
        let dedans = clip(&d, "a.mp4", 3);
        let profond = clip(&d.join("sous"), "b.mp4", 3);
        let texte = clip(&d, "notes.txt", 3);
        assert_eq!(dans_dossier(&d, &dedans), Ok(d.join("a.mp4")));
        let detour = d.join("sous").join("..").join("..").join("dehors.mp4");
        for refuse in [dehors.as_str(), profond.as_str(), texte.as_str(), &detour.to_string_lossy(), &d.to_string_lossy()] {
            assert!(dans_dossier(&d, refuse).is_err(), "{refuse}");
            assert!(supprimer(&d, &[dedans.clone(), refuse.to_string()]).is_err());
            assert!(renommer(&d, refuse, "x").is_err());
        }
        assert!(Path::new(&dedans).exists() && Path::new(&dehors).exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&dehors, d.join("lien.mp4")).unwrap();
            assert!(dans_dossier(&d, &d.join("lien.mp4").to_string_lossy()).is_err());
        }
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn liste_renomme_et_supprime_avec_les_annexes() {
        let d = dossier_test("liste");
        let a = clip(&d, "Jeu 2026-01-01 10-00-00.mp4", 10);
        clip(&d, "copie.mp4", 5);
        creer_annexes(&d).unwrap();
        let info = Annexe { jeu: "Jeu".into(), date: "2026-01-01T10:00:00".into(), duree_s: 30.5, codec: Codec::Hevc, largeur: 2560, hauteur: 1440, ips: 144, pistes_audio: 2, origine: None };
        fs::write(annexe(&d, Path::new(&a), "json"), serde_json::to_vec(&info).unwrap()).unwrap();
        fs::write(annexe(&d, Path::new(&a), "jpg"), b"jpg").unwrap();
        for derive in ["montage.json", "apercu.mp4", "piste1.m4a"] {
            fs::write(annexe(&d, Path::new(&a), derive), b"x").unwrap();
        }
        let clips = lister(&d);
        assert_eq!(clips.len(), 2);
        let c = clips.iter().find(|c| c.nom.starts_with("Jeu")).unwrap();
        assert_eq!((c.jeu.as_deref(), c.duree_s, c.codec, c.pistes_audio, c.taille_octets), (Some("Jeu"), 30.5, Some(Codec::Hevc), 2, 10));
        assert!(c.vignette.as_ref().unwrap().ends_with(".jpg"));
        let nu = clips.iter().find(|c| c.nom == "copie").unwrap();
        assert!(nu.jeu.is_none() && nu.vignette.is_none() && nu.date.ends_with('Z'));
        assert_eq!(serde_json::to_value(c).unwrap()["pistesAudio"], 2);

        assert!(renommer(&d, &a, "copie").is_err());
        assert!(renommer(&d, &a, " ?? ").is_err());
        let b = renommer(&d, &a, "Beau: tir.mp4").unwrap();
        assert_eq!(b, d.join("Beau tir.mp4"));
        assert_eq!(lire_annexe(&d, &b), Some(info));
        assert!(annexe(&d, &b, "jpg").is_file() && !annexe(&d, Path::new(&a), "jpg").exists());
        assert!(annexe(&d, &b, "montage.json").is_file() && annexe(&d, &b, "piste1.m4a").is_file() && !annexe(&d, Path::new(&a), "apercu.mp4").exists());
        supprimer(&d, &[b.to_string_lossy().into_owned()]).unwrap();
        assert!(!b.exists() && !annexe(&d, &b, "json").exists() && !annexe(&d, &b, "jpg").exists());
        assert!(extensions().iter().all(|e| !annexe(&d, &b, e).exists()));
        assert_eq!(lister(&d).len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn quota_sans_suppression_automatique() {
        let fichier = |n: &str, taille: u64, date: i64| (PathBuf::from(n), taille, date);
        let clips = [fichier("a", 40, 1), fichier("b", 30, 2), fichier("c", 30, 3)];
        assert!(plus_anciens(&clips, 100, 101).is_empty());
        assert_eq!(plus_anciens(&clips, 100, 100), ["a"]);
        assert_eq!(plus_anciens(&clips, 100, 50), ["a", "b"]);
        assert_eq!(plus_anciens(&clips, 100, 1), ["a", "b", "c"]);

        let d = dossier_test("quota");
        clip(&d, "a.mp4", 600);
        clip(&d, "b.mp4", 500);
        creer_annexes(&d).unwrap();
        fs::write(annexe(&d, &d.join("a.mp4"), "jpg"), vec![0u8; 100]).unwrap();
        let s = stockage(&d, 1000);
        assert_eq!((s.utilise_octets, s.depasse, s.plus_anciens.len()), (1200, true, 1));
        assert_eq!(lister(&d).len(), 2);
        assert!(!stockage(&d, 5000).depasse);
        assert_eq!(stockage(&d.join("absent"), 10).utilise_octets, 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn reencodage_pour_tenir_dans_une_taille() {
        assert_eq!(debit_pour_taille(25_000_000, 30.0, 128), Some(6205));
        assert_eq!(debit_pour_taille(25_000_000, 300.0, 128), Some(505));
        assert_eq!(debit_pour_taille(8_000_000, 300.0, 128), None);
        let kbps = debit_pour_taille(100_000_000, 120.0, 128).unwrap();
        assert!((kbps as u64 + 128) * 125 * 120 <= 100_000_000);
        assert_eq!(hauteur_pour_debit(505, 1440), Some(480));
        assert_eq!(hauteur_pour_debit(2000, 1080), Some(720));
        assert_eq!(hauteur_pour_debit(2000, 720), None);
        assert_eq!(hauteur_pour_debit(8000, 2160), None);
        let a = arguments_envoi(Path::new("in.mp4"), Path::new("out.mp4"), Some(("h264_amf", 505, Some(480))), 2);
        assert!(a.windows(2).any(|w| w == ["-c:v", "h264_amf"]) && a.windows(2).any(|w| w == ["-b:v", "505k"]));
        assert!(a.windows(2).any(|w| w == ["-vf", "scale=-2:480"]) && a.contains(&"amix=inputs=2:normalize=0[son]".to_string()));
        let copie = arguments_envoi(Path::new("i"), Path::new("o"), None, 1);
        assert!(!copie.contains(&"-filter_complex".to_string()) && copie.windows(2).any(|w| w == ["-c:v", "copy"]) && !copie.contains(&"-b:v".to_string()));
        assert_eq!(arguments_vignette(Path::new("i.mp4"), Path::new("v.jpg")).last().unwrap(), "v.jpg");
    }
}
