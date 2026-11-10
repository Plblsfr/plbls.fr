use std::{process::Stdio, sync::Mutex, time::Duration};

use crate::config::{Config, ModeMateriel};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtatMateriel {
    pub demande: ModeMateriel,
    pub effectif: ModeMateriel,
    pub message: Option<String>,
}

pub struct Materiel(Mutex<EtatMateriel>);

pub fn encodeur(mode: ModeMateriel) -> &'static str {
    match mode {
        ModeMateriel::Cpu => "libx264",
        ModeMateriel::Vaapi => "h264_vaapi",
        ModeMateriel::Qsv => "h264_qsv",
        ModeMateriel::Nvenc => "h264_nvenc",
    }
}

pub fn essai(mode: ModeMateriel, peripherique: &str) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let (avant, filtre): (Vec<&str>, &str) = match mode {
        ModeMateriel::Cpu => (vec![], "format=yuv420p"),
        ModeMateriel::Vaapi => (vec!["-vaapi_device", peripherique], "format=nv12,hwupload"),
        ModeMateriel::Qsv => (
            vec!["-init_hw_device", "qsv=hw", "-filter_hw_device", "hw"],
            "format=nv12,hwupload=extra_hw_frames=16",
        ),
        ModeMateriel::Nvenc => (vec![], "format=yuv420p"),
    };
    a.extend(avant.into_iter().map(String::from));
    a.extend(
        [
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:d=0.2",
            "-vf",
            filtre,
            "-c:v",
            encodeur(mode),
            "-frames:v",
            "3",
            "-f",
            "null",
            "-",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    a
}

impl Materiel {
    pub fn nouveau(demande: ModeMateriel) -> Self {
        Self(Mutex::new(EtatMateriel {
            demande,
            effectif: ModeMateriel::Cpu,
            message: (demande != ModeMateriel::Cpu).then(|| "vérification en cours".to_string()),
        }))
    }

    pub fn etat(&self) -> EtatMateriel {
        self.0.lock().unwrap().clone()
    }

    pub fn mode(&self) -> ModeMateriel {
        self.0.lock().unwrap().effectif
    }

    pub fn signaler(&self, raison: &str) {
        let mut e = self.0.lock().unwrap();
        tracing::warn!("transcodage {} : {raison}", e.effectif.code());
        e.message = Some(format!("{} : {raison}", e.effectif.code()));
    }

    pub async fn verifier(&self, cfg: &Config) {
        let demande = self.etat().demande;
        if demande == ModeMateriel::Cpu {
            return;
        }
        let resultat = verifier_mode(cfg, demande).await;
        let mut e = self.0.lock().unwrap();
        match resultat {
            Ok(()) => {
                tracing::info!("transcodage matériel {} disponible", demande.code());
                e.effectif = demande;
                e.message = None;
            }
            Err(raison) => {
                tracing::error!(
                    "transcodage {} inutilisable : {raison} ; repli sur le CPU",
                    demande.code()
                );
                e.effectif = ModeMateriel::Cpu;
                e.message = Some(format!(
                    "{} inutilisable : {raison} ; repli sur le CPU",
                    demande.code()
                ));
            }
        }
    }
}

async fn verifier_mode(cfg: &Config, mode: ModeMateriel) -> Result<(), String> {
    if matches!(mode, ModeMateriel::Vaapi | ModeMateriel::Qsv)
        && tokio::fs::metadata(&cfg.peripherique).await.is_err()
    {
        return Err(format!("périphérique {} absent", cfg.peripherique));
    }
    let sortie = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(&cfg.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "ffmpeg ne répond pas".to_string())?
    .map_err(|e| format!("ffmpeg introuvable ({e})"))?;
    if !String::from_utf8_lossy(&sortie.stdout).contains(encodeur(mode)) {
        return Err(format!("ffmpeg sans l'encodeur {}", encodeur(mode)));
    }
    let statut = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(&cfg.ffmpeg)
            .args(essai(mode, &cfg.peripherique))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| "essai d'encodage trop long".to_string())?
    .map_err(|e| format!("ffmpeg : {e}"))?;
    if statut.success() {
        Ok(())
    } else {
        Err("essai d'encodage refusé par le matériel".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lignes_d_essai() {
        let l = essai(ModeMateriel::Vaapi, "/dev/dri/renderD129").join(" ");
        assert!(l.contains("-vaapi_device /dev/dri/renderD129"), "{l}");
        assert!(
            l.contains("-vf format=nv12,hwupload -c:v h264_vaapi"),
            "{l}"
        );
        assert!(
            essai(ModeMateriel::Qsv, "x")
                .join(" ")
                .contains("-c:v h264_qsv")
        );
        assert!(
            essai(ModeMateriel::Nvenc, "x")
                .join(" ")
                .contains("-c:v h264_nvenc")
        );
    }

    #[tokio::test]
    async fn mode_inutilisable_replie_sur_le_cpu() {
        let mut cfg = Config::pour_tests("postgres://x/y");
        cfg.peripherique = "/dev/absent-entracte".into();
        for mode in [ModeMateriel::Vaapi, ModeMateriel::Qsv] {
            let m = Materiel::nouveau(mode);
            m.verifier(&cfg).await;
            let e = m.etat();
            assert_eq!(e.effectif, ModeMateriel::Cpu);
            assert!(e.message.unwrap().contains("absent"));
        }
        cfg.ffmpeg = "/chemin/absent/ffmpeg".into();
        let m = Materiel::nouveau(ModeMateriel::Nvenc);
        m.verifier(&cfg).await;
        assert_eq!(m.mode(), ModeMateriel::Cpu);
        assert!(m.etat().message.unwrap().contains("ffmpeg"));
    }
}
