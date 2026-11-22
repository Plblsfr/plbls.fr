use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::types::{Clip, Codec};

pub const ANNEXES: &str = ".estompe";
pub const SOUS_DOSSIER: &str = "Estompe";
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
}
