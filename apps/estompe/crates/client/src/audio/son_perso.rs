use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{FiltreFichiers, Resultat, SystemeAudio};

use super::format::{Reechantillonneur, SourceLecture, FREQUENCE};
use super::peripheriques::Lecture;
use super::tonalite::{SonVocal, VOLUME_MAXIMAL};
use crate::message_vocal;

pub const TAILLE_MAX: u64 = 150 << 10;
pub const MESSAGE_REFUS: &str = "Choisis un fichier audio de 150 Ko au plus.";
pub const TITRE_DIALOGUE: &str = "Choisir un son";
pub const FILTRE_AUDIO: FiltreFichiers = FiltreFichiers {
    nom: "Fichiers audio",
    extensions: &[
        "wav", "mp3", "ogg", "oga", "opus", "webm", "weba", "m4a", "aac", "flac",
    ],
};
const DOSSIER: &str = "sons";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurSon {
    TropGros,
    Illisible,
}

impl ErreurSon {
    pub fn message(&self) -> &'static str {
        MESSAGE_REFUS
    }
}

fn mot16(o: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(o.get(i..i + 2)?.try_into().ok()?))
}

fn mot32(o: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(o.get(i..i + 4)?.try_into().ok()?))
}

fn wav(octets: &[u8]) -> Option<(Vec<f32>, u32)> {
    if octets.get(0..4)? != b"RIFF" || octets.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut position = 12usize;
    let mut format = None;
    let mut donnees = None;
    while position + 8 <= octets.len() {
        let taille = mot32(octets, position + 4)? as usize;
        let debut = position + 8;
        let fin = debut.saturating_add(taille).min(octets.len());
        let corps = &octets[debut..fin];
        match &octets[position..position + 4] {
            b"fmt " => {
                let etiquette = match mot16(corps, 0)? {
                    0xFFFE => mot16(corps, 24)?,
                    autre => autre,
                };
                format = Some((
                    etiquette,
                    mot16(corps, 2)?,
                    mot32(corps, 4)?,
                    mot16(corps, 14)?,
                ));
            }
            b"data" => donnees = Some(corps),
            _ => {}
        }
        position = debut.saturating_add(taille).saturating_add(taille & 1);
    }
    let (etiquette, canaux, frequence, bits) = format?;
    let donnees = donnees?;
    if canaux == 0 || frequence == 0 {
        return None;
    }
    let lire: fn(&[u8]) -> f32 = match (etiquette, bits) {
        (1, 8) => |o| (f32::from(o[0]) - 128.0) / 128.0,
        (1, 16) => |o| f32::from(i16::from_le_bytes([o[0], o[1]])) / 32_768.0,
        (1, 24) => |o| (i32::from_le_bytes([0, o[0], o[1], o[2]]) >> 8) as f32 / 8_388_608.0,
        (1, 32) => |o| i32::from_le_bytes([o[0], o[1], o[2], o[3]]) as f32 / 2_147_483_648.0,
        (3, 32) => |o| f32::from_le_bytes([o[0], o[1], o[2], o[3]]),
        _ => return None,
    };
    let largeur = usize::from(bits / 8);
    let echantillons = donnees
        .chunks_exact(largeur * usize::from(canaux))
        .map(|trame| trame.chunks_exact(largeur).map(lire).sum::<f32>() / f32::from(canaux))
        .collect();
    Some((echantillons, frequence))
}

fn a_48_khz(echantillons: Vec<f32>, frequence: u32) -> Vec<f32> {
    if frequence == FREQUENCE {
        return echantillons;
    }
    let mut sortie = Vec::new();
    Reechantillonneur::nouveau(frequence, FREQUENCE).traiter(&echantillons, &mut sortie);
    sortie
}

pub fn decoder(octets: &[u8], ffmpeg: Option<&Path>) -> Result<Vec<f32>, ErreurSon> {
    if octets.len() as u64 > TAILLE_MAX {
        return Err(ErreurSon::TropGros);
    }
    let echantillons = if let Some((echantillons, frequence)) = wav(octets) {
        a_48_khz(echantillons, frequence)
    } else if let Ok(audio) = message_vocal::decoder(octets) {
        audio.echantillons
    } else if let Some(ffmpeg) = ffmpeg {
        message_vocal::decoder_avec_ffmpeg(ffmpeg, octets)
            .map_err(|_| ErreurSon::Illisible)?
            .echantillons
    } else {
        return Err(ErreurSon::Illisible);
    };
    if echantillons.is_empty() || echantillons.iter().any(|e| !e.is_finite()) {
        return Err(ErreurSon::Illisible);
    }
    Ok(echantillons)
}

pub fn lire_fichier(
    chemin: &Path,
    ffmpeg: Option<&Path>,
) -> Result<(Vec<u8>, Vec<f32>), ErreurSon> {
    let taille = std::fs::metadata(chemin)
        .map_err(|_| ErreurSon::Illisible)?
        .len();
    if taille > TAILLE_MAX {
        return Err(ErreurSon::TropGros);
    }
    let octets = std::fs::read(chemin).map_err(|_| ErreurSon::Illisible)?;
    let echantillons = decoder(&octets, ffmpeg)?;
    Ok((octets, echantillons))
}

#[derive(Debug, Clone)]
pub struct SonsPersonnalises {
    dossier: PathBuf,
}

impl SonsPersonnalises {
    pub fn dans(dossier_donnees: &Path) -> Self {
        Self {
            dossier: dossier_donnees.join(DOSSIER),
        }
    }

    pub fn chemin(&self, son: SonVocal) -> PathBuf {
        self.dossier.join(match son {
            SonVocal::Arrivee => "arrivee",
            SonVocal::Depart => "depart",
        })
    }

    pub fn existe(&self, son: SonVocal) -> bool {
        self.chemin(son).is_file()
    }

    pub fn lire(&self, son: SonVocal) -> Option<Vec<u8>> {
        std::fs::read(self.chemin(son)).ok()
    }

    pub fn enregistrer(&self, son: SonVocal, octets: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(&self.dossier)?;
        let chemin = self.chemin(son);
        let provisoire = chemin.with_extension("tmp");
        std::fs::write(&provisoire, octets)?;
        std::fs::rename(&provisoire, &chemin)
    }
}

pub struct Extrait {
    echantillons: Arc<[f32]>,
    gain: f32,
    position: usize,
}

impl Extrait {
    pub fn nouveau(echantillons: Arc<[f32]>, volume: u16) -> Self {
        Self {
            echantillons,
            gain: f32::from(volume.min(VOLUME_MAXIMAL)) / 100.0,
            position: 0,
        }
    }

    pub fn duree(&self) -> Duration {
        Duration::from_micros(self.echantillons.len() as u64 * 1_000_000 / u64::from(FREQUENCE))
    }
}

impl SourceLecture for Extrait {
    fn lire(&mut self, sortie: &mut [f32]) {
        for e in sortie.iter_mut() {
            *e = self
                .echantillons
                .get(self.position)
                .map_or(0.0, |v| v * self.gain);
            self.position = self.position.saturating_add(1);
        }
    }
}

pub fn jouer(
    systeme: Arc<dyn SystemeAudio>,
    sortie: Option<String>,
    echantillons: Arc<[f32]>,
    volume: u16,
) -> Resultat<Option<(Lecture, Duration)>> {
    if volume == 0 || echantillons.is_empty() {
        return Ok(None);
    }
    let extrait = Extrait::nouveau(echantillons, volume);
    let duree = extrait.duree();
    let source: Arc<Mutex<dyn SourceLecture>> = Arc::new(Mutex::new(extrait));
    Lecture::ouvrir(systeme, sortie, source).map(|lecture| Some((lecture, duree)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::factice::SystemeFactice;
    use crate::audio::{FormatAudio, SensAudio};
    use std::process::Command;

    fn sinus(frequence: u32, duree_ms: u32, amplitude: f32) -> Vec<f32> {
        (0..frequence * duree_ms / 1000)
            .map(|n| {
                amplitude * (2.0 * std::f32::consts::PI * 440.0 * n as f32 / frequence as f32).sin()
            })
            .collect()
    }

    fn fichier_wav(
        etiquette: u16,
        canaux: u16,
        frequence: u32,
        bits: u16,
        donnees: &[u8],
    ) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(b"RIFF");
        o.extend_from_slice(&(36 + donnees.len() as u32).to_le_bytes());
        o.extend_from_slice(b"WAVEfmt ");
        o.extend_from_slice(&16u32.to_le_bytes());
        o.extend_from_slice(&etiquette.to_le_bytes());
        o.extend_from_slice(&canaux.to_le_bytes());
        o.extend_from_slice(&frequence.to_le_bytes());
        let bloc = canaux * bits / 8;
        o.extend_from_slice(&(frequence * u32::from(bloc)).to_le_bytes());
        o.extend_from_slice(&bloc.to_le_bytes());
        o.extend_from_slice(&bits.to_le_bytes());
        o.extend_from_slice(b"LIST");
        o.extend_from_slice(&3u32.to_le_bytes());
        o.extend_from_slice(b"abc\0");
        o.extend_from_slice(b"data");
        o.extend_from_slice(&(donnees.len() as u32).to_le_bytes());
        o.extend_from_slice(donnees);
        o
    }

    fn wav_16(frequence: u32, canaux: u16, echantillons: &[f32]) -> Vec<u8> {
        let donnees: Vec<u8> = echantillons
            .iter()
            .flat_map(|e| {
                let v = (e * 32_767.0) as i16;
                std::iter::repeat_n(v, usize::from(canaux)).flat_map(i16::to_le_bytes)
            })
            .collect();
        fichier_wav(1, canaux, frequence, 16, &donnees)
    }

    fn crete(echantillons: &[f32]) -> f32 {
        echantillons.iter().fold(0.0f32, |m, e| m.max(e.abs()))
    }

    fn ffmpeg() -> Option<&'static str> {
        if Command::new("ffmpeg").arg("-version").output().is_ok() {
            return Some("ffmpeg");
        }
        assert!(
            std::env::var_os("ESTOMPE_FFMPEG_EXIGE").is_none(),
            "ffmpeg exigé (ESTOMPE_FFMPEG_EXIGE)"
        );
        println!("ignoré : ffmpeg indisponible");
        None
    }

    #[test]
    fn un_wav_se_decode_en_mono_a_48_khz_quelle_que_soit_sa_forme() {
        let source = sinus(44_100, 500, 0.5);
        let stereo = decoder(&wav_16(44_100, 2, &source), None).unwrap();
        assert!(
            (stereo.len() as i64 - 24_000).abs() <= 2,
            "{}",
            stereo.len()
        );
        assert!((crete(&stereo) - 0.5).abs() < 0.01);
        let flottant: Vec<u8> = sinus(48_000, 100, 0.25)
            .iter()
            .flat_map(|e| e.to_le_bytes())
            .collect();
        let lu = decoder(&fichier_wav(3, 1, 48_000, 32, &flottant), None).unwrap();
        assert_eq!(lu.len(), 4_800);
        assert!((crete(&lu) - 0.25).abs() < 0.001);
        let sur_8_bits: Vec<u8> = [0u8, 128, 255].repeat(100);
        let lu = decoder(&fichier_wav(1, 1, 48_000, 8, &sur_8_bits), None).unwrap();
        assert_eq!(lu[..3], [-1.0, 0.0, 127.0 / 128.0]);
        let sur_24_bits: Vec<u8> = [0x00u8, 0x00, 0x80, 0xFF, 0xFF, 0x7F].repeat(10);
        let lu = decoder(&fichier_wav(1, 1, 48_000, 24, &sur_24_bits), None).unwrap();
        assert_eq!(lu[0], -1.0);
        assert!((lu[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn un_fichier_trop_gros_vide_ou_qui_n_est_pas_du_son_est_refuse() {
        let gros = wav_16(48_000, 1, &vec![0.1; 80_000]);
        assert!(gros.len() as u64 > TAILLE_MAX);
        assert_eq!(decoder(&gros, None), Err(ErreurSon::TropGros));
        let juste = wav_16(48_000, 1, &vec![0.1; (TAILLE_MAX as usize - 60) / 2]);
        assert!(juste.len() as u64 <= TAILLE_MAX);
        assert!(decoder(&juste, None).is_ok());
        assert_eq!(
            decoder(b"pas du son du tout", None),
            Err(ErreurSon::Illisible)
        );
        assert_eq!(
            decoder(&fichier_wav(1, 1, 48_000, 16, &[]), None),
            Err(ErreurSon::Illisible)
        );
        assert_eq!(
            decoder(&fichier_wav(2, 1, 48_000, 4, &[1, 2, 3]), None),
            Err(ErreurSon::Illisible)
        );
        assert_eq!(ErreurSon::TropGros.message(), MESSAGE_REFUS);
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("gros.wav");
        std::fs::write(&chemin, &gros).unwrap();
        assert_eq!(
            lire_fichier(&chemin, None).unwrap_err(),
            ErreurSon::TropGros
        );
        assert_eq!(
            lire_fichier(&dossier.path().join("absent.wav"), None).unwrap_err(),
            ErreurSon::Illisible
        );
    }

    #[test]
    fn un_ogg_opus_se_decode_sans_ffmpeg() {
        let mut encodeur = message_vocal::Encodeur::nouveau().unwrap();
        encodeur.pousser(&sinus(48_000, 300, 0.5));
        let lu = decoder(&encodeur.terminer(), None).unwrap();
        assert!((lu.len() as i64 - 14_400).abs() < 960, "{}", lu.len());
        assert!(crete(&lu) > 0.3);
    }

    #[test]
    fn un_mp3_se_decode_par_ffmpeg() {
        let Some(ffmpeg) = ffmpeg() else {
            return;
        };
        let dossier = tempfile::tempdir().unwrap();
        let entree = dossier.path().join("son.wav");
        let sortie = dossier.path().join("son.mp3");
        std::fs::write(&entree, wav_16(44_100, 1, &sinus(44_100, 400, 0.5))).unwrap();
        let statut = Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(&entree)
            .arg(&sortie)
            .status()
            .unwrap();
        if !statut.success() {
            println!("ignoré : ffmpeg sans encodeur mp3");
            return;
        }
        let (octets, lu) = lire_fichier(&sortie, Some(Path::new(ffmpeg))).unwrap();
        assert!(octets.starts_with(b"ID3") || octets[0] == 0xFF);
        assert!((lu.len() as i64 - 19_200).abs() < 4_800, "{}", lu.len());
        assert!(crete(&lu) > 0.3);
        assert_eq!(
            lire_fichier(&sortie, None).unwrap_err(),
            ErreurSon::Illisible
        );
    }

    #[test]
    fn le_son_est_garde_sur_l_appareil_et_remplace_le_precedent() {
        let dossier = tempfile::tempdir().unwrap();
        let sons = SonsPersonnalises::dans(dossier.path());
        assert!(!sons.existe(SonVocal::Arrivee));
        assert!(sons.lire(SonVocal::Arrivee).is_none());
        sons.enregistrer(SonVocal::Arrivee, b"premier").unwrap();
        sons.enregistrer(SonVocal::Arrivee, b"second").unwrap();
        assert!(sons.existe(SonVocal::Arrivee));
        assert!(!sons.existe(SonVocal::Depart));
        assert_eq!(sons.lire(SonVocal::Arrivee).unwrap(), b"second");
        assert_eq!(
            sons.chemin(SonVocal::Depart),
            dossier.path().join("sons").join("depart")
        );
        assert_eq!(
            std::fs::read_dir(dossier.path().join("sons"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn le_fichier_joue_au_volume_des_sons_puis_se_tait() {
        let s = SystemeFactice::nouveau();
        s.brancher(
            SensAudio::Sortie,
            "hp",
            "Haut-parleurs",
            FormatAudio {
                frequence: 48_000,
                canaux: 1,
            },
        );
        let echantillons: Arc<[f32]> = vec![0.5; 4_800].into();
        let (lecture, duree) = jouer(Arc::new(s.clone()), None, echantillons.clone(), 60)
            .unwrap()
            .unwrap();
        assert_eq!(duree, Duration::from_millis(100));
        let joue = s.tirer("hp", 4_800).unwrap();
        assert!(
            joue.iter().all(|e| (e - 0.3).abs() < 1e-6),
            "{:?}",
            &joue[..4]
        );
        assert!(s.tirer("hp", 480).unwrap().iter().all(|e| *e == 0.0));
        drop(lecture);
        assert!(jouer(Arc::new(s.clone()), None, echantillons, 0)
            .unwrap()
            .is_none());
        assert!(s.flux_ouverts().is_empty());
    }
}
