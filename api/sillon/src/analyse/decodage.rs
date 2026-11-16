use std::{path::Path, process::Stdio, time::Duration};

use tokio::{io::AsyncReadExt, process::Command, sync::mpsc};

use super::descripteurs::FREQUENCE;

const DUREE_MAX: Duration = Duration::from_secs(1800);
const BLOCS_EN_ATTENTE: usize = 8;

fn commande(programme: &str, prefixe: &[&str], chemin: &Path) -> Command {
    let mut c = Command::new(programme);
    c.args(prefixe)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error"])
        .args(["-threads", "1", "-i"])
        .arg(chemin)
        .args(["-map", "0:a:0", "-vn", "-ac", "1", "-ar"])
        .arg(FREQUENCE.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    c
}

pub async fn decoder<T: Send + 'static>(
    ffmpeg: &str,
    chemin: &Path,
    consommer: impl FnOnce(mpsc::Receiver<Vec<f32>>) -> T + Send + 'static,
) -> Result<T, String> {
    let mut enfant = match commande("nice", &["-n", "19", ffmpeg], chemin).spawn() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => commande(ffmpeg, &[], chemin).spawn(),
        r => r,
    }
    .map_err(|e| format!("ffmpeg : {e}"))?;
    let mut sortie = enfant.stdout.take().ok_or("ffmpeg sans sortie")?;
    let (tx, rx) = mpsc::channel(BLOCS_EN_ATTENTE);
    let calcul = tokio::task::spawn_blocking(move || consommer(rx));
    let lecture = async move {
        let mut tampon = vec![0u8; 1 << 16];
        let mut reste = 0;
        loop {
            let lus = sortie
                .read(&mut tampon[reste..])
                .await
                .map_err(|e| e.to_string())?;
            if lus == 0 {
                break;
            }
            let total = reste + lus;
            let entiers = total - total % 4;
            let bloc = tampon[..entiers]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|o| f32::from_le_bytes(*o))
                .map(|e| if e.is_finite() { e } else { 0.0 })
                .collect();
            tampon.copy_within(entiers..total, 0);
            reste = total - entiers;
            if tx.send(bloc).await.is_err() {
                break;
            }
        }
        drop(tx);
        enfant.wait().await.map_err(|e| e.to_string())
    };
    let statut = tokio::time::timeout(DUREE_MAX, lecture)
        .await
        .map_err(|_| "décodage trop long".to_string())??;
    let resultat = calcul.await.map_err(|_| "calcul interrompu".to_string())?;
    if !statut.success() {
        return Err("décodage refusé par ffmpeg".into());
    }
    Ok(resultat)
}
