mod disque;
mod factice;
#[cfg(windows)]
mod windows;

use serde::{Deserialize, Serialize};

use crate::Resultat;

pub use disque::{disque_plein, masquer, ERREUR_DISQUE_PLEIN};
pub use factice::{EncodageFactice, PARAMETRES_FACTICES};

pub const UNITE_TEMPS: i64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodecVideo {
    H264,
    Hevc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapaciteEncodeur {
    pub nom: String,
    pub mb_par_seconde: u64,
    pub largeur_max: u32,
    pub hauteur_max: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterielEncodage {
    pub carte: String,
    pub h264: Option<CapaciteEncodeur>,
    pub hevc: Option<CapaciteEncodeur>,
    pub ecran_hz: u32,
    pub ram_libre: u64,
}

impl MaterielEncodage {
    pub fn capacite(&self, codec: CodecVideo) -> Option<&CapaciteEncodeur> {
        match codec {
            CodecVideo::H264 => self.h264.as_ref(),
            CodecVideo::Hevc => self.hevc.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParametresVideo {
    pub codec: CodecVideo,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub debit_kbps: u32,
    pub images_par_groupe: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParametresAudio {
    pub frequence: u32,
    pub canaux: u16,
    pub debit_kbps: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageNv12 {
    pub largeur: u32,
    pub hauteur: u32,
    pub donnees: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paquet {
    pub pts: i64,
    pub duree: i64,
    pub cle: bool,
    pub donnees: Vec<u8>,
}

pub type RecepteurPaquets = Box<dyn FnMut(Paquet) + Send>;
pub type SignalArret = Box<dyn FnOnce(String) + Send>;

pub trait EncodeurVideo: Send {
    fn nom(&self) -> &str;
    fn soumettre(&mut self, image: ImageNv12, pts: i64, duree: i64) -> Resultat<bool>;
    fn encodees(&self) -> u64;
}

pub trait EncodeurAudio: Send {
    fn soumettre(&mut self, pcm: &[i16]) -> Resultat<()>;
}

pub trait EncodageMateriel: Send + Sync {
    fn disponible(&self) -> bool;
    fn materiel(&self, fenetre: Option<u64>) -> Resultat<MaterielEncodage>;
    fn video(
        &self,
        parametres: ParametresVideo,
        sortie: RecepteurPaquets,
        arret: SignalArret,
    ) -> Resultat<Box<dyn EncodeurVideo>>;
    fn audio(
        &self,
        parametres: ParametresAudio,
        sortie: RecepteurPaquets,
    ) -> Resultat<Box<dyn EncodeurAudio>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EncodageSysteme;

#[cfg(windows)]
impl EncodageMateriel for EncodageSysteme {
    fn disponible(&self) -> bool {
        true
    }
    fn materiel(&self, fenetre: Option<u64>) -> Resultat<MaterielEncodage> {
        windows::materiel(fenetre)
    }
    fn video(
        &self,
        parametres: ParametresVideo,
        sortie: RecepteurPaquets,
        arret: SignalArret,
    ) -> Resultat<Box<dyn EncodeurVideo>> {
        windows::video(parametres, sortie, arret)
    }
    fn audio(
        &self,
        parametres: ParametresAudio,
        sortie: RecepteurPaquets,
    ) -> Resultat<Box<dyn EncodeurAudio>> {
        windows::audio(parametres, sortie)
    }
}

#[cfg(not(windows))]
impl EncodageMateriel for EncodageSysteme {
    fn disponible(&self) -> bool {
        false
    }
    fn materiel(&self, _: Option<u64>) -> Resultat<MaterielEncodage> {
        Err(crate::Erreur::Indisponible("encodage vidéo matériel"))
    }
    fn video(
        &self,
        _: ParametresVideo,
        _: RecepteurPaquets,
        _: SignalArret,
    ) -> Resultat<Box<dyn EncodeurVideo>> {
        Err(crate::Erreur::Indisponible("encodage vidéo matériel"))
    }
    fn audio(&self, _: ParametresAudio, _: RecepteurPaquets) -> Resultat<Box<dyn EncodeurAudio>> {
        Err(crate::Erreur::Indisponible("encodage vidéo matériel"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_encodage_materiel_n_existe_que_sous_windows() {
        assert_eq!(EncodageSysteme.disponible(), cfg!(windows));
        if !cfg!(windows) {
            assert!(EncodageSysteme.materiel(None).is_err());
            assert!(EncodageSysteme
                .video(PARAMETRES_FACTICES, Box::new(|_| {}), Box::new(|_| {}))
                .is_err());
        }
    }

    #[test]
    fn le_codec_s_ecrit_en_minuscules() {
        assert_eq!(
            serde_json::to_string(&CodecVideo::Hevc).unwrap(),
            r#""hevc""#
        );
        let m = MaterielEncodage::default();
        assert!(m.capacite(CodecVideo::H264).is_none());
    }
}
