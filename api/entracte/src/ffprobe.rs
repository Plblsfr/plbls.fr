use std::{path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::filtre;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Video {
    pub index: i64,
    pub codec: String,
    pub largeur: i64,
    pub hauteur: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub index: i64,
    pub codec: String,
    pub langue: Option<String>,
    pub titre: Option<String>,
    pub canaux: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SousTitre {
    pub index: Option<i64>,
    pub codec: String,
    pub langue: Option<String>,
    pub titre: Option<String>,
    pub externe: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Analyse {
    pub conteneur: String,
    pub duree: Option<f64>,
    pub video: Option<Video>,
    pub audio: Vec<Audio>,
    pub sous_titres: Vec<SousTitre>,
}

impl Analyse {
    pub fn depuis(v: &Value) -> Self {
        let tags = |s: &Value, k: &str| {
            s["tags"][k]
                .as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty() && *t != "und")
                .map(String::from)
        };
        let flux = v["streams"].as_array().cloned().unwrap_or_default();
        let de_type = |t: &'static str| flux.iter().filter(move |s| s["codec_type"] == t);
        let video = de_type("video")
            .find(|s| s["disposition"]["attached_pic"].as_i64().unwrap_or(0) == 0)
            .map(|s| Video {
                index: s["index"].as_i64().unwrap_or(0),
                codec: s["codec_name"].as_str().unwrap_or_default().to_string(),
                largeur: s["width"].as_i64().unwrap_or(0),
                hauteur: s["height"].as_i64().unwrap_or(0),
            });
        Analyse {
            conteneur: v["format"]["format_name"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            duree: v["format"]["duration"]
                .as_str()
                .and_then(|d| d.parse::<f64>().ok())
                .filter(|d| d.is_finite() && *d > 0.0),
            video,
            audio: de_type("audio")
                .map(|s| Audio {
                    index: s["index"].as_i64().unwrap_or(0),
                    codec: s["codec_name"].as_str().unwrap_or_default().to_string(),
                    langue: tags(s, "language"),
                    titre: tags(s, "title"),
                    canaux: s["channels"].as_i64().unwrap_or(2),
                })
                .collect(),
            sous_titres: de_type("subtitle")
                .filter(|s| {
                    filtre::sous_titre_autorise(s["codec_name"].as_str().unwrap_or_default(), None)
                })
                .map(|s| SousTitre {
                    index: s["index"].as_i64(),
                    codec: s["codec_name"].as_str().unwrap_or_default().to_string(),
                    langue: tags(s, "language"),
                    titre: tags(s, "title"),
                    externe: None,
                })
                .collect(),
        }
    }
}

pub async fn analyser(ffprobe: &str, chemin: &Path) -> Result<Analyse, String> {
    let mut c = tokio::process::Command::new(ffprobe);
    c.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(chemin)
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    let sortie = tokio::time::timeout(Duration::from_secs(120), c.output())
        .await
        .map_err(|_| "ffprobe trop long".to_string())?
        .map_err(|e| format!("ffprobe : {e}"))?;
    if !sortie.status.success() {
        return Err("ffprobe a échoué".into());
    }
    let v: Value = serde_json::from_slice(&sortie.stdout).map_err(|e| format!("ffprobe : {e}"))?;
    Ok(Analyse::depuis(&v))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Depart {
    pub image_cle: f64,
    pub viser: f64,
}

const MARGE: f64 = 0.5;
const VOISINAGE: f64 = 1.0;

pub fn image_cle(sortie: &str) -> Option<f64> {
    let (pts, drapeaux) = sortie.lines().next()?.trim().split_once(',')?;
    drapeaux
        .starts_with('K')
        .then(|| pts.parse::<f64>().ok())
        .flatten()
        .filter(|t| t.is_finite())
}

pub fn viser(cle: f64, borne: f64) -> f64 {
    cle + ((borne - cle) / 2.0).clamp(0.0, MARGE)
}

async fn sonder(ffprobe: &str, chemin: &Path, arguments: &[&str]) -> Option<String> {
    let mut c = tokio::process::Command::new(ffprobe);
    c.args(["-v", "error"])
        .args(arguments)
        .args(["-of", "csv=p=0"])
        .arg(chemin)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let sortie = tokio::time::timeout(Duration::from_secs(20), c.output())
        .await
        .ok()?
        .ok()?;
    Some(String::from_utf8_lossy(&sortie.stdout).into_owned())
}

async fn image_cle_avant(ffprobe: &str, chemin: &Path, index: i64, t: f64) -> Option<f64> {
    let (flux, intervalle) = (index.to_string(), format!("{t:.6}%+#1"));
    let arguments = [
        "-select_streams",
        flux.as_str(),
        "-read_intervals",
        intervalle.as_str(),
        "-show_entries",
        "packet=pts_time,flags",
    ];
    image_cle(&sonder(ffprobe, chemin, &arguments).await?)
}

pub async fn depart_de_copie(
    ffprobe: &str,
    chemin: &Path,
    index: i64,
    debut: f64,
) -> Option<Depart> {
    let origine = sonder(ffprobe, chemin, &["-show_entries", "format=start_time"])
        .await?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|t| t.is_finite())
        .unwrap_or(0.0);
    let au_debut = Depart {
        image_cle: 0.0,
        viser: 0.0,
    };
    let mut borne = debut + origine;
    let mut cle = image_cle_avant(ffprobe, chemin, index, borne).await?;
    if cle > borne {
        return None;
    }
    if borne - cle < VOISINAGE {
        match image_cle_avant(ffprobe, chemin, index, cle - MARGE).await {
            Some(avant) if avant < cle => (borne, cle) = (cle, avant),
            _ => return Some(au_debut),
        }
    }
    if cle - origine < 0.001 {
        return Some(au_debut);
    }
    Some(Depart {
        image_cle: cle - origine,
        viser: viser(cle, borne) - origine,
    })
}

pub async fn sous_titres_externes(video: &Path) -> Vec<SousTitre> {
    let (Some(dossier), Some(base)) = (video.parent(), video.file_stem().and_then(|s| s.to_str()))
    else {
        return Vec::new();
    };
    let Ok(mut lecture) = tokio::fs::read_dir(dossier).await else {
        return Vec::new();
    };
    let mut trouves = Vec::new();
    while let Ok(Some(e)) = lecture.next_entry().await {
        let chemin = e.path();
        let Some(nom) = chemin.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(reste) = nom.strip_prefix(base) else {
            continue;
        };
        if !reste.starts_with('.') || trouves.len() >= 20 {
            continue;
        }
        let morceaux: Vec<&str> = reste[1..].split('.').collect();
        let extension = morceaux
            .last()
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        let texte = chemin.to_string_lossy().to_string();
        if !filtre::sous_titre_autorise(&extension, Some(&format!("/x/sous-titre.{extension}")))
            || !e.file_type().await.is_ok_and(|t| t.is_file())
        {
            continue;
        }
        let langue = (morceaux.len() > 1)
            .then(|| morceaux[0].to_string())
            .filter(|l| l.len() <= 8 && l.chars().all(|c| c.is_ascii_alphabetic() || c == '-'));
        trouves.push(SousTitre {
            index: None,
            codec: extension,
            langue,
            titre: None,
            externe: Some(texte),
        });
    }
    trouves.sort_by(|a, b| a.externe.cmp(&b.externe));
    trouves
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lecture_du_json_de_ffprobe() {
        let a = Analyse::depuis(&json!({
            "format": {"format_name": "matroska,webm", "duration": "5400.5"},
            "streams": [
                {"index": 0, "codec_type": "video", "codec_name": "mjpeg", "disposition": {"attached_pic": 1}},
                {"index": 1, "codec_type": "video", "codec_name": "hevc", "width": 3840, "height": 2160, "disposition": {"attached_pic": 0}},
                {"index": 2, "codec_type": "audio", "codec_name": "eac3", "channels": 6, "tags": {"language": "fre", "title": "VF"}},
                {"index": 3, "codec_type": "audio", "codec_name": "aac", "tags": {"language": "und"}},
                {"index": 4, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"language": "fre"}},
                {"index": 5, "codec_type": "subtitle", "codec_name": "hdmv_pgs_subtitle"}
            ]
        }));
        assert_eq!(a.conteneur, "matroska,webm");
        assert_eq!(a.duree, Some(5400.5));
        assert_eq!(a.video.as_ref().unwrap().index, 1);
        assert_eq!(a.video.as_ref().unwrap().hauteur, 2160);
        assert_eq!(a.audio.len(), 2);
        assert_eq!(a.audio[0].langue.as_deref(), Some("fre"));
        assert_eq!(a.audio[1].langue, None);
        assert_eq!(a.audio[1].canaux, 2);
        assert_eq!(a.sous_titres.len(), 1);
    }

    #[test]
    fn image_cle_lue_dans_la_sortie_de_ffprobe() {
        assert_eq!(image_cle("300.000000,K__\n"), Some(300.0));
        assert_eq!(image_cle("12.512000,K_\r\n13.0,__\n"), Some(12.512));
        assert_eq!(image_cle("300.041000,___\n"), None);
        assert_eq!(image_cle("N/A,K__\n"), None);
        assert_eq!(image_cle(""), None);
    }

    #[test]
    fn position_visee_a_l_ecart_des_images_cles() {
        assert_eq!(viser(300.0, 307.0), 300.5);
        assert_eq!(viser(300.0, 300.6), 300.3);
        assert_eq!(viser(300.0, 300.0), 300.0);
    }

    #[tokio::test]
    async fn depart_d_une_copie_sur_une_image_cle() {
        let present = |outil: &str| {
            std::process::Command::new(outil)
                .arg("-version")
                .output()
                .is_ok_and(|s| s.status.success())
        };
        if !present("ffmpeg") || !present("ffprobe") {
            eprintln!("ignoré : ffmpeg ou ffprobe absent");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let film = d.path().join("film.mkv");
        let ok = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc=size=160x90:rate=24"])
            .args(["-t", "12", "-c:v", "libx264", "-preset", "ultrafast"])
            .args(["-g", "48", "-keyint_min", "48", "-sc_threshold", "0"])
            .arg(&film)
            .status()
            .unwrap()
            .success();
        assert!(ok, "génération du film de test");
        let depart = |debut: f64| depart_de_copie("ffprobe", &film, 0, debut);
        let attendu = |image_cle: f64, viser: f64| Some(Depart { image_cle, viser });
        assert_eq!(depart(5.3).await, attendu(4.0, 4.5));
        assert_eq!(depart(7.9).await, attendu(6.0, 6.5));
        assert_eq!(
            depart(4.2).await,
            attendu(2.0, 2.5),
            "trop près de l'image clé"
        );
        assert_eq!(depart(4.0).await, attendu(2.0, 2.5));
        assert_eq!(depart(0.4).await, attendu(0.0, 0.0));
        assert_eq!(depart(2.1).await, attendu(0.0, 0.0));
        for (debut, image) in [(5.3, 4.0), (4.2, 2.0), (7.9, 6.0)] {
            let vise = depart(debut).await.unwrap().viser;
            let extrait = d.path().join("extrait.nut");
            let ok = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                .args(["-ss", &format!("{vise:.6}"), "-i"])
                .arg(&film)
                .args(["-c", "copy", "-copyts", "-frames:v", "1"])
                .arg(&extrait)
                .status()
                .unwrap()
                .success();
            assert!(ok, "extraction à {vise}");
            let lu = sonder(
                "ffprobe",
                &extrait,
                &["-show_entries", "packet=pts_time,flags"],
            )
            .await;
            assert_eq!(image_cle(&lu.unwrap()), Some(image), "ffmpeg -ss {vise}");
        }
    }

    #[tokio::test]
    async fn sous_titres_a_cote_de_la_video() {
        let d = tempfile::tempdir().unwrap();
        for nom in [
            "Film.mkv",
            "Film.fr.srt",
            "Film.en.forced.ass",
            "Film.exe",
            "Film.srt.exe",
            "Autre.srt",
            "Film.vtt",
        ] {
            std::fs::write(d.path().join(nom), b"x").unwrap();
        }
        let s = sous_titres_externes(&d.path().join("Film.mkv")).await;
        let noms: Vec<String> = s
            .iter()
            .map(|x| {
                Path::new(x.externe.as_ref().unwrap())
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert_eq!(noms, ["Film.en.forced.ass", "Film.fr.srt", "Film.vtt"]);
        assert_eq!(s[1].langue.as_deref(), Some("fr"));
        assert_eq!(s[2].langue, None);
    }
}
