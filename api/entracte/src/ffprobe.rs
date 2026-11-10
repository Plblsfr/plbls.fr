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
