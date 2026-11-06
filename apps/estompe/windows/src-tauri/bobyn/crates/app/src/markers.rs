//! Marqueurs posés pendant l'enregistrement (EST-130) : horodatage sur l'horloge du buffer, découpage en chapitres du replay
//! et fichier annexe `{replay}.markers.json` lu par la bibliothèque d'Estompe.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Marqueur posé : heure du buffer (µs, même horloge que les paquets) et heure locale affichée dans son nom.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub pts_us: i64,
    pub clock: String,
}

/// Marqueur d'un replay : `t` en secondes depuis son début.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Marker {
    pub t: f64,
    pub name: String,
}

/// Oublie les marqueurs sortis du tampon (plus anciens que sa plus ancienne image).
pub fn prune(marks: &mut Vec<Mark>, oldest: i64) {
    marks.retain(|m| m.pts_us >= oldest);
}

/// Marqueurs d'un replay dont la vidéo couvre `[first, end)`, nommés « Marqueur N (heure) » dans l'ordre.
pub fn timeline(marks: &[Mark], first: i64, end: i64) -> Vec<Marker> {
    marks
        .iter()
        .filter(|m| m.pts_us >= first && m.pts_us < end)
        .enumerate()
        .map(|(i, m)| Marker { t: (m.pts_us - first) as f64 / 1e6, name: format!("Marqueur {} ({})", i + 1, m.clock) })
        .collect()
}

/// Fichier annexe d'un replay : `a.mp4` → `a.markers.json`.
pub fn sidecar(video: &Path) -> PathBuf {
    video.with_extension("markers.json")
}

/// Fichier absent ou illisible : aucun marqueur.
pub fn read(video: &Path) -> Vec<Marker> {
    std::fs::read_to_string(sidecar(video)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Liste vide : le fichier annexe est supprimé.
pub fn write(video: &Path, markers: &[Marker]) -> std::io::Result<()> {
    let path = sidecar(video);
    if markers.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::write(path, serde_json::to_string_pretty(markers).map_err(std::io::Error::other)?)
}

/// Marqueurs d'un extrait `[start, end]` : ceux qu'il contient, recalés sur son début.
pub fn clip(markers: &[Marker], start: f64, end: f64) -> Vec<Marker> {
    markers.iter().filter(|m| m.t >= start && m.t <= end).map(|m| Marker { t: m.t - start, name: m.name.clone() }).collect()
}

/// Marqueurs reçus de l'interface : triés, noms nettoyés, au plus 255 (limite des chapitres MP4).
pub fn sanitize(markers: Vec<Marker>) -> Result<Vec<Marker>, String> {
    if markers.len() > 255 {
        return Err("255 marqueurs au plus".into());
    }
    let mut out = Vec::with_capacity(markers.len());
    for m in markers {
        if !m.t.is_finite() || m.t < 0.0 {
            return Err("instant de marqueur invalide".into());
        }
        let name: String = m.name.chars().filter(|c| !c.is_control()).take(100).collect();
        out.push(Marker { t: m.t, name: name.trim().to_string() });
    }
    out.sort_by(|a, b| a.t.total_cmp(&b.t));
    Ok(out)
}

/// Métadonnées ffmpeg (`-f ffmetadata`) des chapitres d'un extrait de `len` secondes.
pub fn ffmetadata(markers: &[Marker], len: f64) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('=', "\\=").replace(';', "\\;").replace('#', "\\#").replace('\n', " ");
    let mut out = String::from(";FFMETADATA1\n");
    for (i, m) in markers.iter().enumerate() {
        let start = (m.t * 1000.0) as i64;
        let end = markers.get(i + 1).map_or((len * 1000.0) as i64, |n| (n.t * 1000.0) as i64).max(start + 1);
        out.push_str(&format!("[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={end}\ntitle={}\n", esc(&m.name)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(s: i64) -> Mark {
        Mark { pts_us: s * 1_000_000, clock: format!("20:00:{s:02}") }
    }

    #[test]
    fn horodatage_et_marqueurs_sortis_du_tampon() {
        let mut marks = vec![mark(5), mark(20), mark(40)];
        // le tampon commence à 10 s : le marqueur de 5 s est oublié
        prune(&mut marks, 10_000_000);
        assert_eq!(marks, vec![mark(20), mark(40)]);
        // un replay des 30 dernières secondes (de 15 s à 45 s) les garde, recalés sur son début
        let t = timeline(&marks, 15_000_000, 45_000_000);
        assert_eq!(t, vec![Marker { t: 5.0, name: "Marqueur 1 (20:00:20)".into() }, Marker { t: 25.0, name: "Marqueur 2 (20:00:40)".into() }]);
        // un replay qui commence après le premier marqueur ne le contient pas, et la numérotation repart de 1
        let t = timeline(&marks, 30_000_000, 45_000_000);
        assert_eq!(t.iter().map(|m| (m.t, m.name.as_str())).collect::<Vec<_>>(), vec![(10.0, "Marqueur 1 (20:00:40)")]);
        assert!(timeline(&marks, 41_000_000, 50_000_000).is_empty());
    }

    #[test]
    fn extrait_et_nettoyage() {
        let m = |t: f64, n: &str| Marker { t, name: n.into() };
        let all = [m(2.0, "a"), m(12.0, "b"), m(30.0, "c")];
        assert_eq!(clip(&all, 10.0, 20.0), vec![m(2.0, "b")]);
        assert_eq!(sanitize(vec![m(5.0, "  x\n"), m(1.0, "y")]).unwrap(), vec![m(1.0, "y"), m(5.0, "x")]);
        assert!(sanitize(vec![m(-1.0, "a")]).is_err() && sanitize(vec![m(f64::NAN, "a")]).is_err());
        assert!(sanitize(vec![m(1.0, "a"); 256]).is_err());
    }

    #[test]
    fn fichier_annexe_et_metadonnees() {
        let dir = std::env::temp_dir().join(format!("estompe-markers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("a.mp4");
        assert!(read(&video).is_empty());
        let list = vec![Marker { t: 1.5, name: "x=y".into() }, Marker { t: 4.0, name: "z".into() }];
        write(&video, &list).unwrap();
        assert_eq!(sidecar(&video).file_name().unwrap(), "a.markers.json");
        assert_eq!(read(&video), list);
        write(&video, &[]).unwrap();
        assert!(read(&video).is_empty() && !sidecar(&video).exists());
        let meta = ffmetadata(&list, 10.0);
        assert!(meta.starts_with(";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=1500\nEND=4000\ntitle=x\\=y\n"));
        assert!(meta.ends_with("START=4000\nEND=10000\ntitle=z\n"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
