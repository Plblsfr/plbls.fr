use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::types::{Clip, Codec, Stockage};

pub const ANNEXES: &str = ".estompe";
pub const SOUS_DOSSIER: &str = "Estompe";
pub const PISTES_MAX: usize = 4;
pub const PARTIEL: &str = "part";
const INTERDITS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

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
        let (jours, reste) = (
            secondes.div_euclid(86_400),
            secondes.rem_euclid(86_400) as u32,
        );
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

    pub fn pour_nom(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}-{:02}-{:02}",
            self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde
        )
    }

    pub fn iso(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde
        )
    }
}

pub fn nettoyer(nom: &str) -> String {
    let propre: String = nom
        .chars()
        .filter(|c| !INTERDITS.contains(c) && !c.is_control())
        .collect();
    let propre: String = propre
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect();
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
    format!(
        "{} {}",
        if jeu.is_empty() { "Jeu" } else { &jeu },
        date.pour_nom()
    )
}

pub fn chemin_libre(dossier: &Path, base: &str) -> PathBuf {
    (1u32..)
        .map(|n| {
            dossier.join(if n == 1 {
                format!("{base}.mp4")
            } else {
                format!("{base} ({n}).mp4")
            })
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dossier.join(format!("{base}.mp4")))
}

pub fn dossier_effectif(reglage: Option<&str>, videos: Option<PathBuf>) -> Result<PathBuf, String> {
    match reglage {
        Some(d) => Ok(PathBuf::from(d)),
        None => videos.map(|v| v.join(SOUS_DOSSIER)).ok_or_else(|| {
            "dossier Vidéos introuvable : choisir un dossier pour les clips".to_string()
        }),
    }
}

pub fn annexe(dossier: &Path, clip: &Path, extension: &str) -> PathBuf {
    let nom = clip
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dossier.join(ANNEXES).join(format!("{nom}.{extension}"))
}

pub fn extensions() -> Vec<String> {
    let fixes = ["json", "jpg", "montage.json", "apercu.mp4"].map(String::from);
    fixes
        .into_iter()
        .chain((0..PISTES_MAX).map(|i| format!("piste{i}.m4a")))
        .collect()
}

pub fn creer_annexes(dossier: &Path) -> std::io::Result<PathBuf> {
    let annexes = dossier.join(ANNEXES);
    if !annexes.is_dir() {
        fs::create_dir_all(&annexes)?;
        estompe_plateforme::masquer(&annexes);
    }
    Ok(annexes)
}

fn est_mp4(chemin: &Path) -> bool {
    chemin
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
        && chemin.is_file()
}

fn fichiers(dossier: &Path) -> Vec<(PathBuf, u64, i64)> {
    let entrees = fs::read_dir(dossier).into_iter().flatten().flatten();
    let mut liste: Vec<(PathBuf, u64, i64)> = entrees
        .filter(|e| est_mp4(&e.path()))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let modifie = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            Some((e.path(), m.len(), modifie))
        })
        .collect();
    liste.sort_by(|a, b| (a.2, &a.0).cmp(&(b.2, &b.0)));
    liste
}

pub fn lire_annexe(dossier: &Path, clip: &Path) -> Option<Annexe> {
    serde_json::from_slice(&fs::read(annexe(dossier, clip, "json")).ok()?).ok()
}

pub fn lister(dossier: &Path) -> Vec<Clip> {
    fichiers(dossier)
        .into_iter()
        .rev()
        .map(|(chemin, taille, modifie)| {
            let a = lire_annexe(dossier, &chemin);
            let vignette = Some(annexe(dossier, &chemin, "jpg")).filter(|v| v.is_file());
            Clip {
                nom: chemin
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                date: a.as_ref().map_or_else(
                    || format!("{}Z", Horodatage::depuis_epoque(modifie).iso()),
                    |a| a.date.clone(),
                ),
                duree_s: a.as_ref().map_or(0.0, |a| a.duree_s),
                taille_octets: taille,
                vignette,
                codec: a.as_ref().map(|a| a.codec),
                largeur: a.as_ref().map_or(0, |a| a.largeur),
                hauteur: a.as_ref().map_or(0, |a| a.hauteur),
                ips: a.as_ref().map_or(0, |a| a.ips),
                pistes_audio: a.as_ref().map_or(0, |a| a.pistes_audio),
                origine: a.as_ref().and_then(|a| a.origine.clone()),
                jeu: a.map(|a| a.jeu),
                chemin,
            }
        })
        .collect()
}

fn taille_dossier(dossier: &Path) -> u64 {
    let entrees = fs::read_dir(dossier).into_iter().flatten().flatten();
    entrees
        .map(|e| {
            if e.path().is_dir() {
                taille_dossier(&e.path())
            } else {
                e.metadata().map_or(0, |m| m.len())
            }
        })
        .sum()
}

pub fn utilise(dossier: &Path) -> u64 {
    fichiers(dossier).iter().map(|f| f.1).sum::<u64>() + taille_dossier(&dossier.join(ANNEXES))
}

pub fn dans_dossier(dossier: &Path, chemin: &Path) -> Result<PathBuf, String> {
    let refus = || "chemin refusé : hors du dossier des clips".to_string();
    let racine =
        fs::canonicalize(dossier).map_err(|_| "dossier des clips inaccessible".to_string())?;
    let reel = fs::canonicalize(chemin).map_err(|_| "clip introuvable".to_string())?;
    if reel.parent() != Some(racine.as_path()) || !est_mp4(&reel) {
        return Err(refus());
    }
    reel.file_name().map(|n| dossier.join(n)).ok_or_else(refus)
}

pub fn renommer(dossier: &Path, chemin: &Path, nom: &str) -> Result<PathBuf, String> {
    let source = dans_dossier(dossier, chemin)?;
    let saisi = nom.trim();
    let saisi = if saisi.to_ascii_lowercase().ends_with(".mp4") {
        &saisi[..saisi.len() - 4]
    } else {
        saisi
    };
    let propre = nettoyer(saisi);
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
        let _ = fs::rename(
            annexe(dossier, &source, &extension),
            annexe(dossier, &cible, &extension),
        );
    }
    Ok(cible)
}

pub fn supprimer(dossier: &Path, chemins: &[PathBuf]) -> Result<(), String> {
    let clips: Vec<PathBuf> = chemins
        .iter()
        .map(|c| dans_dossier(dossier, c))
        .collect::<Result<_, _>>()?;
    for clip in clips {
        fs::remove_file(&clip).map_err(|e| format!("suppression impossible : {e}"))?;
        for extension in extensions() {
            let _ = fs::remove_file(annexe(dossier, &clip, &extension));
        }
    }
    Ok(())
}

fn protege(proteges: &[PathBuf], chemin: &Path) -> bool {
    proteges
        .iter()
        .any(|p| p == chemin || (p.file_name().is_some() && p.file_name() == chemin.file_name()))
}

pub fn plus_anciens(
    clips: &[(PathBuf, u64, i64)],
    utilise: u64,
    quota: u64,
    proteges: &[PathBuf],
) -> Vec<PathBuf> {
    let mut reste = utilise;
    let mut liste = Vec::new();
    for (chemin, taille, _) in clips {
        if reste < quota {
            break;
        }
        if protege(proteges, chemin) {
            continue;
        }
        reste = reste.saturating_sub(*taille);
        liste.push(chemin.clone());
    }
    liste
}

pub fn stockage(dossier: &Path, quota: u64, proteges: &[PathBuf]) -> Stockage {
    let utilise = utilise(dossier);
    Stockage {
        utilise_octets: utilise,
        quota_octets: quota,
        depasse: utilise >= quota,
        plus_anciens: plus_anciens(&fichiers(dossier), utilise, quota, proteges),
    }
}

pub fn purger(dossier: &Path, quota: u64, proteges: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let anciens = stockage(dossier, quota, proteges).plus_anciens;
    supprimer(dossier, &anciens)?;
    Ok(anciens)
}

pub fn debit_pour_taille(taille_max: u64, duree_s: f64, audio_kbps: u32) -> Option<u32> {
    let total = taille_max as f64 * 8.0 / 1000.0 / duree_s.max(1.0) * 0.95;
    let video = total - f64::from(audio_kbps);
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

pub fn arguments_envoi(
    source: &Path,
    sortie: &Path,
    video: Option<(&str, u32, Option<u32>)>,
    pistes_audio: u32,
) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-nostats", "-progress", "pipe:1", "-i"]
        .map(String::from)
        .into();
    a.push(source.to_string_lossy().into_owned());
    if pistes_audio > 1 {
        a.extend([
            "-filter_complex".into(),
            format!("amix=inputs={pistes_audio}:normalize=0[son]"),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "[son]".into(),
        ]);
    }
    match video {
        None => a.extend(["-c:v".into(), "copy".into()]),
        Some((encodeur, kbps, hauteur)) => {
            if let Some(h) = hauteur {
                a.extend(["-vf".into(), format!("scale=-2:{h}")]);
            }
            a.extend([
                "-c:v".into(),
                encodeur.into(),
                "-b:v".into(),
                format!("{kbps}k"),
                "-maxrate".into(),
                format!("{kbps}k"),
                "-bufsize".into(),
                format!("{}k", kbps * 2),
                "-pix_fmt".into(),
                "yuv420p".into(),
            ]);
        }
    }
    a.extend(
        [
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
            "-y",
        ]
        .map(String::from),
    );
    a.push(sortie.to_string_lossy().into_owned());
    a
}

pub fn creer_vignette(ffmpeg: &Path, dossier: &Path, clip: &Path) -> Result<PathBuf, String> {
    creer_annexes(dossier).map_err(|e| e.to_string())?;
    let sortie = annexe(dossier, clip, "jpg");
    let resultat = estompe_plateforme::commande_d_arriere_plan(ffmpeg)
        .args(["-hide_banner", "-nostdin"])
        .args(arguments_vignette(clip, &sortie))
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if resultat.status.success() && sortie.is_file() {
        Ok(sortie)
    } else {
        let _ = fs::remove_file(&sortie);
        Err(String::from_utf8_lossy(&resultat.stderr)
            .lines()
            .last()
            .unwrap_or("échec de ffmpeg")
            .to_string())
    }
}

pub fn arguments_vignette(source: &Path, sortie: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-i"].map(String::from).into();
    a.push(source.to_string_lossy().into_owned());
    a.extend(
        [
            "-frames:v",
            "1",
            "-vf",
            "scale=480:-2",
            "-q:v",
            "5",
            "-f",
            "mjpeg",
            "-y",
        ]
        .map(String::from),
    );
    a.push(sortie.to_string_lossy().into_owned());
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(dossier: &Path, nom: &str, taille: usize) {
        fs::write(dossier.join(nom), vec![0u8; taille]).unwrap();
    }

    #[test]
    fn nom_de_fichier_propre() {
        let date = Horodatage {
            annee: 2026,
            mois: 3,
            jour: 7,
            heure: 9,
            minute: 5,
            seconde: 2,
        };
        assert_eq!(
            nom_clip("Counter-Strike 2", &date),
            "Counter-Strike 2 2026-03-07 09-05-02"
        );
        assert_eq!(
            nom_clip(r#"Jeu: le <retour> / "2" | a\b?*"#, &date),
            "Jeu le retour 2 ab 2026-03-07 09-05-02"
        );
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
        assert_eq!(
            Horodatage::depuis_epoque(1_709_251_199).iso(),
            "2024-02-29T23:59:59"
        );
        assert_eq!(
            Horodatage::depuis_epoque(1_794_441_600).pour_nom(),
            "2026-11-12 00-00-00"
        );
        assert_eq!(Horodatage::depuis_epoque(-1).iso(), "1969-12-31T23:59:59");
    }

    #[test]
    fn collisions_de_noms() {
        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        assert_eq!(chemin_libre(d, "Jeu 1"), d.join("Jeu 1.mp4"));
        clip(d, "Jeu 1.mp4", 1);
        assert_eq!(chemin_libre(d, "Jeu 1"), d.join("Jeu 1 (2).mp4"));
        clip(d, "Jeu 1 (2).mp4", 1);
        assert_eq!(chemin_libre(d, "Jeu 1"), d.join("Jeu 1 (3).mp4"));
    }

    #[test]
    fn dossier_par_defaut_ou_choisi() {
        assert_eq!(
            dossier_effectif(None, Some(PathBuf::from("/v"))),
            Ok(PathBuf::from("/v/Estompe"))
        );
        assert_eq!(
            dossier_effectif(Some("/clips"), None),
            Ok(PathBuf::from("/clips"))
        );
        assert!(dossier_effectif(None, None).is_err());
    }

    #[test]
    fn liste_et_utilisation_du_dossier() {
        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        clip(d, "a.mp4", 10);
        clip(d, "notes.txt", 100);
        creer_annexes(d).unwrap();
        fs::write(annexe(d, &d.join("a.mp4"), "json"), b"{}").unwrap();
        assert_eq!(utilise(d), 12);
        let clips = lister(d);
        assert_eq!(clips.len(), 1);
        assert_eq!((clips[0].nom.as_str(), clips[0].jeu.as_ref()), ("a", None));
        assert!(clips[0].date.ends_with('Z'));
    }

    fn chemin(dossier: &Path, nom: &str, taille: usize) -> PathBuf {
        clip(dossier, nom, taille);
        dossier.join(nom)
    }

    #[test]
    fn jamais_hors_du_dossier_des_clips() {
        let racine = tempfile::tempdir().unwrap();
        let racine = racine.path();
        let d = racine.join("clips");
        fs::create_dir_all(d.join("sous")).unwrap();
        let dehors = chemin(racine, "dehors.mp4", 3);
        let dedans = chemin(&d, "a.mp4", 3);
        let profond = chemin(&d.join("sous"), "b.mp4", 3);
        let texte = chemin(&d, "notes.txt", 3);
        assert_eq!(dans_dossier(&d, &dedans), Ok(d.join("a.mp4")));
        let detour = d.join("sous").join("..").join("..").join("dehors.mp4");
        for refuse in [&dehors, &profond, &texte, &detour, &d] {
            assert!(dans_dossier(&d, refuse).is_err(), "{}", refuse.display());
            assert!(supprimer(&d, &[dedans.clone(), refuse.clone()]).is_err());
            assert!(renommer(&d, refuse, "x").is_err());
        }
        assert!(dedans.exists() && dehors.exists());
    }

    #[test]
    fn liste_renomme_et_supprime_avec_les_annexes() {
        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        let a = chemin(d, "Jeu 2026-01-01 10-00-00.mp4", 10);
        clip(d, "copie.mp4", 5);
        creer_annexes(d).unwrap();
        let info = Annexe {
            jeu: "Jeu".into(),
            date: "2026-01-01T10:00:00".into(),
            duree_s: 30.5,
            codec: Codec::Hevc,
            largeur: 2560,
            hauteur: 1440,
            ips: 144,
            pistes_audio: 2,
            origine: None,
        };
        fs::write(annexe(d, &a, "json"), serde_json::to_vec(&info).unwrap()).unwrap();
        fs::write(annexe(d, &a, "jpg"), b"jpg").unwrap();
        for derive in ["montage.json", "apercu.mp4", "piste1.m4a"] {
            fs::write(annexe(d, &a, derive), b"x").unwrap();
        }
        let clips = lister(d);
        assert_eq!(clips.len(), 2);
        let c = clips.iter().find(|c| c.nom.starts_with("Jeu")).unwrap();
        assert_eq!(
            (
                c.jeu.as_deref(),
                c.duree_s,
                c.codec,
                c.pistes_audio,
                c.taille_octets
            ),
            (Some("Jeu"), 30.5, Some(Codec::Hevc), 2, 10)
        );
        assert!(c
            .vignette
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .ends_with(".jpg"));
        let nu = clips.iter().find(|c| c.nom == "copie").unwrap();
        assert!(nu.jeu.is_none() && nu.vignette.is_none() && nu.date.ends_with('Z'));

        assert!(renommer(d, &a, "copie").is_err());
        assert!(renommer(d, &a, " ?? ").is_err());
        let b = renommer(d, &a, "Beau: tir.mp4").unwrap();
        assert_eq!(b, d.join("Beau tir.mp4"));
        assert_eq!(lire_annexe(d, &b), Some(info));
        assert!(annexe(d, &b, "jpg").is_file() && !annexe(d, &a, "jpg").exists());
        assert!(
            annexe(d, &b, "montage.json").is_file()
                && annexe(d, &b, "piste1.m4a").is_file()
                && !annexe(d, &a, "apercu.mp4").exists()
        );
        supprimer(d, std::slice::from_ref(&b)).unwrap();
        assert!(!b.exists() && !annexe(d, &b, "json").exists() && !annexe(d, &b, "jpg").exists());
        assert!(extensions().iter().all(|e| !annexe(d, &b, e).exists()));
        assert_eq!(lister(d).len(), 1);
    }

    #[test]
    fn quota_et_plus_anciens() {
        let fichier = |n: &str, taille: u64, date: i64| (PathBuf::from(n), taille, date);
        let clips = [
            fichier("a", 40, 1),
            fichier("b", 30, 2),
            fichier("c", 30, 3),
        ];
        let noms = |v: Vec<PathBuf>| {
            v.iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert!(plus_anciens(&clips, 100, 101, &[]).is_empty());
        assert_eq!(noms(plus_anciens(&clips, 100, 100, &[])), ["a"]);
        assert_eq!(noms(plus_anciens(&clips, 100, 50, &[])), ["a", "b"]);
        assert_eq!(noms(plus_anciens(&clips, 100, 1, &[])), ["a", "b", "c"]);
        assert_eq!(
            noms(plus_anciens(&clips, 100, 100, &[PathBuf::from("a")])),
            ["b"]
        );
        assert_eq!(
            noms(plus_anciens(&clips, 100, 50, &[PathBuf::from("b")])),
            ["a", "c"]
        );

        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        clip(d, "a.mp4", 600);
        clip(d, "b.mp4", 500);
        creer_annexes(d).unwrap();
        fs::write(annexe(d, &d.join("a.mp4"), "jpg"), vec![0u8; 100]).unwrap();
        let s = stockage(d, 1000, &[]);
        assert_eq!(
            (s.utilise_octets, s.depasse, s.plus_anciens.len()),
            (1200, true, 1)
        );
        assert_eq!(lister(d).len(), 2);
        assert!(!stockage(d, 5000, &[]).depasse);
        assert_eq!(stockage(&d.join("absent"), 10, &[]).utilise_octets, 0);
    }

    fn vieillir(chemin: &Path, secondes: u64) {
        let date = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + secondes);
        fs::File::options()
            .write(true)
            .open(chemin)
            .unwrap()
            .set_modified(date)
            .unwrap();
    }

    #[test]
    fn quota_depasse_purge_les_plus_anciens_non_proteges() {
        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        let ancien = chemin(d, "ancien.mp4", 400);
        let envoye = chemin(d, "envoye.mp4", 400);
        let moyen = chemin(d, "moyen.mp4", 400);
        let recent = chemin(d, "recent.mp4", 400);
        for (i, c) in [&ancien, &envoye, &moyen, &recent].into_iter().enumerate() {
            vieillir(c, i as u64 * 60);
        }
        let annexes = creer_annexes(d).unwrap();
        let partiel = annexes.join(format!("Jeu 2026-01-01 10-00-00.{PARTIEL}"));
        fs::write(&partiel, vec![0u8; 300]).unwrap();
        assert_eq!(utilise(d), 1900);
        let proteges = vec![envoye.clone()];
        let supprimes = purger(d, 1200, &proteges).unwrap();
        assert_eq!(supprimes, [ancien.clone(), moyen.clone()]);
        assert!(!ancien.exists() && !moyen.exists());
        assert!(envoye.exists() && recent.exists());
        assert!(partiel.exists(), "le clip en cours d'écriture reste");
        assert!(!stockage(d, 1200, &proteges).depasse);
        assert!(purger(d, 1200, &proteges).unwrap().is_empty());
        assert_eq!(
            purger(d, 1, &[envoye.clone(), recent.clone()]).unwrap(),
            Vec::<PathBuf>::new()
        );
        assert!(partiel.exists() && envoye.exists() && recent.exists());
    }

    #[test]
    fn reencodage_pour_tenir_dans_une_taille() {
        assert_eq!(debit_pour_taille(25_000_000, 30.0, 128), Some(6205));
        assert_eq!(debit_pour_taille(25_000_000, 300.0, 128), Some(505));
        assert_eq!(debit_pour_taille(8_000_000, 300.0, 128), None);
        let kbps = debit_pour_taille(100_000_000, 120.0, 128).unwrap();
        assert!((u64::from(kbps) + 128) * 125 * 120 <= 100_000_000);
        assert_eq!(hauteur_pour_debit(505, 1440), Some(480));
        assert_eq!(hauteur_pour_debit(2000, 1080), Some(720));
        assert_eq!(hauteur_pour_debit(2000, 720), None);
        assert_eq!(hauteur_pour_debit(8000, 2160), None);
        let a = arguments_envoi(
            Path::new("in.mp4"),
            Path::new("out.mp4"),
            Some(("h264_amf", 505, Some(480))),
            2,
        );
        assert!(
            a.windows(2).any(|w| w == ["-c:v", "h264_amf"])
                && a.windows(2).any(|w| w == ["-b:v", "505k"])
        );
        assert!(
            a.windows(2).any(|w| w == ["-vf", "scale=-2:480"])
                && a.contains(&"amix=inputs=2:normalize=0[son]".to_string())
        );
        assert!(a.windows(2).any(|w| w == ["-progress", "pipe:1"]));
        let copie = arguments_envoi(Path::new("i"), Path::new("o"), None, 1);
        assert!(
            !copie.contains(&"-filter_complex".to_string())
                && copie.windows(2).any(|w| w == ["-c:v", "copy"])
                && !copie.contains(&"-b:v".to_string())
        );
        assert_eq!(
            arguments_vignette(Path::new("i.mp4"), Path::new("v.jpg"))
                .last()
                .unwrap(),
            "v.jpg"
        );
    }

    #[test]
    fn vignette_tiree_du_clip_par_ffmpeg() {
        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        let source = d.join("Jeu 2026-01-01 10-00-00.mp4");
        let fabrique = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-f", "lavfi", "-i"])
            .arg("testsrc=size=320x180:rate=10:duration=1")
            .args(["-c:v", "mpeg4", "-y"])
            .arg(&source)
            .output();
        if !fabrique.is_ok_and(|o| o.status.success()) {
            eprintln!("ffmpeg absent : vignette non testée");
            return;
        }
        let vignette = creer_vignette(Path::new("ffmpeg"), d, &source).unwrap();
        assert_eq!(vignette, annexe(d, &source, "jpg"));
        assert_eq!(&fs::read(&vignette).unwrap()[..2], &[0xFF, 0xD8]);
        assert_eq!(lister(d)[0].vignette.as_ref(), Some(&vignette));
        let faux = chemin(d, "faux.mp4", 10);
        assert!(creer_vignette(Path::new("ffmpeg"), d, &faux).is_err());
        assert!(!annexe(d, &faux, "jpg").exists());
    }
}
