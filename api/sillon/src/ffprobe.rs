use std::{path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Analyse {
    pub conteneur: String,
    pub codec: Option<String>,
    pub debit: Option<i32>,
    pub frequence: Option<i32>,
    pub canaux: Option<i32>,
    pub bits: Option<i32>,
    pub duree: Option<f64>,
}

fn nombre(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        .filter(|n| *n > 0)
}

fn entier(v: &Value) -> Option<i32> {
    nombre(v).and_then(|n| i32::try_from(n).ok())
}

fn duree(v: &Value) -> Option<f64> {
    v.as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0)
}

impl Analyse {
    pub fn depuis(v: &Value) -> Self {
        let audio = v["streams"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["codec_type"] == "audio");
        let format = &v["format"];
        let conteneur = format["format_name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let Some(a) = audio else {
            return Analyse {
                conteneur,
                duree: duree(&format["duration"]),
                ..Analyse::default()
            };
        };
        Analyse {
            conteneur,
            codec: a["codec_name"]
                .as_str()
                .filter(|c| !c.is_empty())
                .map(String::from),
            debit: entier(&a["bit_rate"]).or_else(|| entier(&format["bit_rate"])),
            frequence: entier(&a["sample_rate"]),
            canaux: entier(&a["channels"]),
            bits: entier(&a["bits_per_raw_sample"]).or_else(|| entier(&a["bits_per_sample"])),
            duree: duree(&a["duration"]).or_else(|| duree(&format["duration"])),
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
    let sortie = tokio::time::timeout(Duration::from_secs(60), c.output())
        .await
        .map_err(|_| "ffprobe trop long".to_string())?
        .map_err(|e| format!("ffprobe : {e}"))?;
    if !sortie.status.success() {
        return Err("ffprobe a échoué".into());
    }
    let v: Value = serde_json::from_slice(&sortie.stdout).map_err(|e| format!("ffprobe : {e}"))?;
    Ok(Analyse::depuis(&v))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn lecture_du_json_de_ffprobe() {
        let a = Analyse::depuis(&json!({
            "format": {"format_name": "mp3", "duration": "181.5", "bit_rate": "320000"},
            "streams": [
                {"index": 0, "codec_type": "video", "codec_name": "mjpeg"},
                {"index": 1, "codec_type": "audio", "codec_name": "mp3", "sample_rate": "44100", "channels": 2}
            ]
        }));
        assert_eq!(
            a,
            Analyse {
                conteneur: "mp3".into(),
                codec: Some("mp3".into()),
                debit: Some(320_000),
                frequence: Some(44_100),
                canaux: Some(2),
                bits: None,
                duree: Some(181.5),
            }
        );
        let flac = Analyse::depuis(&json!({
            "format": {"format_name": "flac", "duration": "N/A"},
            "streams": [{"codec_type": "audio", "codec_name": "flac", "sample_rate": "96000",
                "channels": 2, "bits_per_raw_sample": "24", "duration": "3.5"}]
        }));
        assert_eq!((flac.bits, flac.duree), (Some(24), Some(3.5)));
        let vide = Analyse::depuis(&json!({"format": {"format_name": "png_pipe"}, "streams": [{"codec_type": "video"}]}));
        assert_eq!(vide.codec, None);
    }
}
