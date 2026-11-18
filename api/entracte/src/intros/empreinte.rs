use std::{path::Path, process::Stdio, time::Duration};

use tokio::process::Command;

use crate::config::Intro;

const ATTENTE: Duration = Duration::from_secs(900);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outil {
    Ffmpeg,
    Fpcalc,
}

impl Outil {
    pub fn nom(self) -> &'static str {
        match self {
            Outil::Ffmpeg => "muxer chromaprint de ffmpeg",
            Outil::Fpcalc => "fpcalc",
        }
    }
}

pub fn muxer_present(sortie: &str) -> bool {
    sortie.lines().any(|l| {
        let mut mots = l.split_whitespace();
        mots.next().is_some_and(|d| d.contains('E')) && mots.next() == Some("chromaprint")
    })
}

async fn sortie_de(mut c: Command, attente: Duration) -> Result<Vec<u8>, String> {
    c.stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let s = tokio::time::timeout(attente, c.output())
        .await
        .map_err(|_| "délai dépassé".to_string())?
        .map_err(|e| e.kind().to_string())?;
    if !s.status.success() {
        return Err("échec de la commande".into());
    }
    Ok(s.stdout)
}

pub async fn outil(ffmpeg: &str, fpcalc: &str) -> Option<Outil> {
    let mut c = Command::new(ffmpeg);
    c.args(["-hide_banner", "-muxers"]);
    if let Ok(s) = sortie_de(c, Duration::from_secs(20)).await
        && muxer_present(&String::from_utf8_lossy(&s))
    {
        return Some(Outil::Ffmpeg);
    }
    let mut c = Command::new(fpcalc);
    c.arg("-version");
    sortie_de(c, Duration::from_secs(20))
        .await
        .is_ok()
        .then_some(Outil::Fpcalc)
}

pub fn points_bruts(octets: &[u8]) -> Vec<u32> {
    octets
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect()
}

pub fn points_fpcalc(sortie: &str) -> Option<Vec<u32>> {
    sortie
        .lines()
        .find_map(|l| l.trim().strip_prefix("FINGERPRINT="))?
        .split(',')
        .map(|p| p.trim().parse::<u32>().ok())
        .collect()
}

pub fn en_octets(points: &[u32]) -> Vec<u8> {
    points.iter().flat_map(|p| p.to_le_bytes()).collect()
}

pub fn depuis_octets(octets: &[u8]) -> Vec<u32> {
    octets
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}

fn commande(programme: &str, nice: u8, avec_nice: bool) -> Command {
    if avec_nice && nice > 0 {
        let mut c = Command::new("nice");
        c.args(["-n", &nice.to_string(), programme]);
        c
    } else {
        Command::new(programme)
    }
}

fn arguments(c: &mut Command, outil: Outil, duree: u32, chemin: &Path) {
    let duree = duree.to_string();
    match outil {
        Outil::Ffmpeg => {
            c.args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-threads",
                "1",
                "-t",
                &duree,
                "-i",
            ])
            .arg(chemin)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-sn",
                "-dn",
                "-ac",
                "1",
                "-ar",
                "11025",
                "-threads",
                "1",
                "-filter_threads",
                "1",
                "-f",
                "chromaprint",
                "-fp_format",
                "raw",
                "-",
            ]);
        }
        Outil::Fpcalc => {
            c.args(["-raw", "-length", &duree]).arg(chemin);
        }
    }
}

pub async fn extraire(
    ffmpeg: &str,
    cfg: &Intro,
    outil: Outil,
    chemin: &Path,
) -> Result<Vec<u32>, String> {
    let programme = match outil {
        Outil::Ffmpeg => ffmpeg,
        Outil::Fpcalc => cfg.fpcalc.as_str(),
    };
    let lancer = |avec_nice: bool| {
        let mut c = commande(programme, cfg.nice, avec_nice);
        arguments(&mut c, outil, cfg.analyse, chemin);
        sortie_de(c, ATTENTE)
    };
    let brut = match lancer(true).await {
        Ok(b) => b,
        Err(_) if cfg.nice > 0 => lancer(false).await?,
        Err(e) => return Err(e),
    };
    let points = match outil {
        Outil::Ffmpeg => points_bruts(&brut),
        Outil::Fpcalc => points_fpcalc(&String::from_utf8_lossy(&brut))
            .ok_or_else(|| "sortie de fpcalc illisible".to_string())?,
    };
    if points.is_empty() {
        return Err("aucune piste audio exploitable".into());
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muxer_reconnu_dans_la_liste_de_ffmpeg() {
        let avec = " E caf             Apple CAF\n  E chromaprint     Chromaprint\n  E crc             CRC testing\n";
        assert!(muxer_present(avec));
        assert!(!muxer_present(
            "  E caf             Apple CAF\n D  chromaprint     x\n"
        ));
        assert!(!muxer_present(""));
    }

    #[test]
    fn sorties_lues() {
        assert_eq!(
            points_fpcalc("DURATION=600\nFINGERPRINT=1,2,4294967295\n"),
            Some(vec![1, 2, u32::MAX])
        );
        assert_eq!(points_fpcalc("DURATION=600\n"), None);
        assert_eq!(points_fpcalc("FINGERPRINT=1,x\n"), None);
        let points = vec![0, 7, u32::MAX, 123_456];
        assert_eq!(depuis_octets(&en_octets(&points)), points);
        assert_eq!(
            points_bruts(&[1u32.to_ne_bytes(), 9u32.to_ne_bytes()].concat()),
            vec![1, 9]
        );
        assert_eq!(points_bruts(&[1, 2, 3]), Vec::<u32>::new());
    }
}
