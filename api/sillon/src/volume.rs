use std::{path::Path, process::Stdio, time::Duration};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::catalogue::FichierPiste;

pub const CIBLE: f64 = -18.0;
pub const PIC_MIN: f64 = -144.0;
const DUREE_MAX: Duration = Duration::from_secs(1800);

#[derive(Clone, Debug, PartialEq)]
pub struct Mesure {
    pub niveau: f64,
    pub pic: f64,
    pub plage: f64,
    pub echantillons: i64,
    pub frequence: i32,
}

impl Mesure {
    pub fn duree(&self) -> f64 {
        self.echantillons as f64 / f64::from(self.frequence.max(1))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gains {
    pub niveau: Option<f64>,
    pub piste: Option<f64>,
    pub pic_piste: Option<f64>,
    pub album: Option<f64>,
    pub pic_album: Option<f64>,
}

fn valeur(ligne: &str, cle: &str) -> Option<f64> {
    let reste = ligne.trim().strip_prefix(cle)?;
    let v = reste.split_whitespace().next()?;
    match v {
        "-inf" => Some(f64::NEG_INFINITY),
        _ => v.parse().ok(),
    }
}

pub fn lire_resume(stderr: &str) -> Option<(f64, f64, f64)> {
    let resume = &stderr[stderr.rfind("Summary:")?..];
    let mut niveau = None;
    let mut plage = None;
    let mut pic = None;
    for l in resume.lines() {
        niveau = niveau.or_else(|| valeur(l, "I:"));
        plage = plage.or_else(|| valeur(l, "LRA:"));
        pic = pic.or_else(|| valeur(l, "Peak:"));
    }
    let borne = |v: f64| if v.is_finite() { v } else { PIC_MIN };
    Some((
        borne(niveau?).max(-70.0),
        borne(pic?).max(PIC_MIN),
        borne(plage?),
    ))
}

pub fn frequence_de_sortie(stderr: &str) -> Option<i32> {
    let sortie = &stderr[stderr.find("Output #0")?..];
    let avant = &sortie[..sortie.find(" Hz")?];
    avant.rsplit([' ', ',']).next()?.parse().ok()
}

pub async fn mesurer(ffmpeg: &str, chemin: &Path) -> Result<Mesure, String> {
    let mut c = tokio::process::Command::new(ffmpeg);
    c.args(["-nostdin", "-hide_banner", "-nostats", "-i"])
        .arg(chemin)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "ebur128=peak=true:framelog=quiet",
            "-f",
            "u8",
            "-ac",
            "1",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut enfant = c.spawn().map_err(|e| format!("ffmpeg : {e}"))?;
    let mut sortie = enfant.stdout.take().ok_or("ffmpeg sans sortie")?;
    let mut erreurs = enfant.stderr.take().ok_or("ffmpeg sans sortie")?;
    let travail = async {
        let compter = async {
            let mut n: i64 = 0;
            let mut tampon = vec![0u8; 1 << 16];
            loop {
                match sortie.read(&mut tampon).await {
                    Ok(0) => return Ok(n),
                    Ok(k) => n += k as i64,
                    Err(e) => return Err(e.to_string()),
                }
            }
        };
        let journal = async {
            let mut t = Vec::new();
            erreurs.read_to_end(&mut t).await.map(|_| t)
        };
        let (n, t) = tokio::join!(compter, journal);
        let statut = enfant.wait().await.map_err(|e| e.to_string())?;
        Ok::<_, String>((n?, t.map_err(|e| e.to_string())?, statut))
    };
    let (echantillons, journal, statut) = tokio::time::timeout(DUREE_MAX, travail)
        .await
        .map_err(|_| "mesure trop longue".to_string())??;
    let journal = String::from_utf8_lossy(&journal);
    if !statut.success() {
        return Err("mesure refusée par ffmpeg".into());
    }
    let (niveau, pic, plage) = lire_resume(&journal).ok_or("résumé ebur128 absent")?;
    let frequence = frequence_de_sortie(&journal).ok_or("fréquence de sortie absente")?;
    Ok(Mesure {
        niveau,
        pic,
        plage,
        echantillons,
        frequence,
    })
}

pub async fn enregistrer(
    pool: &PgPool,
    piste: Uuid,
    contenu_le: DateTime<Utc>,
    m: &Mesure,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM mesures WHERE piste_id = $1 AND contenu_le <> $2")
        .bind(piste)
        .bind(contenu_le)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO mesures (piste_id, contenu_le, niveau, pic, plage, echantillons, frequence)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (piste_id, contenu_le) DO UPDATE SET niveau = EXCLUDED.niveau, pic = EXCLUDED.pic,
           plage = EXCLUDED.plage, echantillons = EXCLUDED.echantillons, frequence = EXCLUDED.frequence, mesure_le = now()",
    )
    .bind(piste)
    .bind(contenu_le)
    .bind(m.niveau)
    .bind(m.pic)
    .bind(m.plage)
    .bind(m.echantillons)
    .bind(m.frequence)
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn lire(
    pool: &PgPool,
    piste: Uuid,
    contenu_le: DateTime<Utc>,
) -> Result<Option<Mesure>, sqlx::Error> {
    let l = sqlx::query(
        "SELECT niveau, pic, plage, echantillons, frequence FROM mesures WHERE piste_id = $1 AND contenu_le = $2",
    )
    .bind(piste)
    .bind(contenu_le)
    .fetch_optional(pool)
    .await?;
    Ok(l.map(|l| Mesure {
        niveau: l.get(0),
        pic: l.get(1),
        plage: l.get(2),
        echantillons: l.get(3),
        frequence: l.get(4),
    }))
}

pub async fn mesure_de(
    pool: &PgPool,
    ffmpeg: &str,
    f: &FichierPiste,
) -> Result<Option<Mesure>, sqlx::Error> {
    if let Some(m) = lire(pool, f.piste_id, f.contenu_le).await? {
        return Ok(Some(m));
    }
    match mesurer(ffmpeg, Path::new(&f.chemin)).await {
        Ok(m) => {
            enregistrer(pool, f.piste_id, f.contenu_le, &m).await?;
            Ok(Some(m))
        }
        Err(e) => {
            tracing::warn!(
                "mesure du niveau de {} : {e}",
                crate::filtre::nom_de_fichier(&f.chemin)
            );
            Ok(None)
        }
    }
}

const FICHIER_COURANT: &str = "JOIN LATERAL (SELECT contenu_le FROM fichiers WHERE piste_id = p.id
    ORDER BY taille DESC, id LIMIT 1) f ON true";

pub async fn non_mesurees_de_l_album(pool: &PgPool, piste: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT p.id FROM pistes p {FICHIER_COURANT}
         WHERE p.album_id = (SELECT album_id FROM pistes WHERE id = $1)
           AND NOT EXISTS (SELECT 1 FROM mesures m WHERE m.piste_id = p.id AND m.contenu_le = f.contenu_le)
         ORDER BY p.disque, p.numero, p.id"
    )))
    .bind(piste)
    .fetch_all(pool)
    .await
}

pub async fn gains(pool: &PgPool, piste: Uuid, m: Option<&Mesure>) -> Result<Gains, sqlx::Error> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT 10 * log(sum(m.echantillons::float8 / m.frequence * power(10, m.niveau / 10))
                          / nullif(sum(m.echantillons::float8 / m.frequence), 0)),
                max(m.pic)
         FROM pistes p {FICHIER_COURANT}
         JOIN mesures m ON m.piste_id = p.id AND m.contenu_le = f.contenu_le
         WHERE p.album_id = (SELECT album_id FROM pistes WHERE id = $1)"
    )))
    .bind(piste)
    .fetch_one(pool)
    .await?;
    let album: Option<f64> = l.get(0);
    let arrondi = |v: f64| (v * 100.0).round() / 100.0;
    Ok(Gains {
        niveau: m.map(|m| arrondi(m.niveau)),
        piste: m.map(|m| arrondi(CIBLE - m.niveau)),
        pic_piste: m.map(|m| arrondi(m.pic)),
        album: album.filter(|a| a.is_finite()).map(|a| arrondi(CIBLE - a)),
        pic_album: l.get::<Option<f64>, _>(1).map(arrondi),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOURNAL: &str = "Input #0, flac, from 'a.flac':
  Stream #0:0: Audio: flac, 22050 Hz, mono, s16
Output #0, u8, to 'pipe:1':
  Stream #0:0: Audio: pcm_u8, 22050 Hz, mono, u8, 176 kb/s
[Parsed_ebur128_0 @ 0x1] Summary:

  Integrated loudness:
    I:         -21.7 LUFS
    Threshold: -31.7 LUFS

  Loudness range:
    LRA:         1.5 LU

  True peak:
    Peak:      -18.1 dBFS
";

    #[test]
    fn lecture_du_resume_ebur128() {
        assert_eq!(lire_resume(JOURNAL), Some((-21.7, -18.1, 1.5)));
        assert_eq!(frequence_de_sortie(JOURNAL), Some(22050));
        let silence = JOURNAL
            .replace("-21.7 LUFS", "-70.0 LUFS")
            .replace("-18.1 dBFS", "-inf dBFS");
        assert_eq!(lire_resume(&silence), Some((-70.0, PIC_MIN, 1.5)));
        assert_eq!(lire_resume("rien"), None);
        assert_eq!(frequence_de_sortie("Output #0, u8"), None);
    }
}
