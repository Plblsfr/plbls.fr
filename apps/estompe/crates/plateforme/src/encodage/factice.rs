use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use super::{
    CapaciteEncodeur, CodecVideo, EncodageMateriel, EncodeurAudio, EncodeurVideo, ImageNv12,
    MaterielEncodage, Paquet, ParametresAudio, ParametresVideo, RecepteurPaquets, SignalArret,
    UNITE_TEMPS,
};
use crate::{Erreur, Resultat};

pub const PARAMETRES_FACTICES: ParametresVideo = ParametresVideo {
    codec: CodecVideo::H264,
    largeur: 320,
    hauteur: 240,
    ips: 30,
    debit_kbps: 2000,
    images_par_groupe: 30,
};

const SPS: [u8; 10] = [0x67, 0x64, 0x00, 0x28, 0xAC, 0xD9, 0x40, 0x78, 0x02, 0x27];
const PPS: [u8; 4] = [0x68, 0xEB, 0xE3, 0xCB];
const BLOC_AUDIO: usize = 1024;

struct EtatFactice {
    materiel: Option<MaterielEncodage>,
    refus: Option<String>,
    muet: bool,
    videos: Vec<ParametresVideo>,
    audios: Vec<ParametresAudio>,
    arrets: Vec<SignalArret>,
}

#[derive(Clone)]
pub struct EncodageFactice {
    etat: Arc<Mutex<EtatFactice>>,
    images: Arc<AtomicU64>,
}

fn capacite(nom: &str, mb_par_seconde: u64, cote: u32) -> Option<CapaciteEncodeur> {
    Some(CapaciteEncodeur {
        nom: nom.into(),
        mb_par_seconde,
        largeur_max: cote,
        hauteur_max: cote,
    })
}

impl Default for EncodageFactice {
    fn default() -> Self {
        Self::avec(Some(MaterielEncodage {
            carte: "Carte factice".into(),
            h264: capacite("H.264 factice", 2_073_600, 4096),
            hevc: capacite("HEVC factice", 16_711_680, 8192),
            ecran_hz: 144,
            ram_libre: 16 << 30,
        }))
    }
}

impl EncodageFactice {
    pub fn nouveau() -> Self {
        Self::default()
    }

    pub fn avec(materiel: Option<MaterielEncodage>) -> Self {
        Self {
            etat: Arc::new(Mutex::new(EtatFactice {
                materiel,
                refus: None,
                muet: false,
                videos: Vec::new(),
                audios: Vec::new(),
                arrets: Vec::new(),
            })),
            images: Arc::default(),
        }
    }

    fn etat(&self) -> MutexGuard<'_, EtatFactice> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn refuser(&self, refus: Option<&str>) {
        self.etat().refus = refus.map(str::to_string);
    }

    pub fn rendre_muet(&self, muet: bool) {
        self.etat().muet = muet;
    }

    pub fn videos(&self) -> Vec<ParametresVideo> {
        self.etat().videos.clone()
    }

    pub fn audios(&self) -> Vec<ParametresAudio> {
        self.etat().audios.clone()
    }

    pub fn images(&self) -> u64 {
        self.images.load(Ordering::Relaxed)
    }

    pub fn arreter(&self, raison: &str) {
        let arrets = std::mem::take(&mut self.etat().arrets);
        for arret in arrets {
            arret(raison.to_string());
        }
    }
}

pub fn image_factice(cle: bool, valeur: u8) -> Vec<u8> {
    let mut donnees = Vec::new();
    if cle {
        for jeu in [&SPS[..], &PPS] {
            donnees.extend([0, 0, 0, 1]);
            donnees.extend(jeu);
        }
    }
    donnees.extend([0, 0, 1, if cle { 0x65 } else { 0x41 }]);
    donnees.extend([valeur | 1; 24]);
    donnees
}

struct VideoFactice {
    parametres: ParametresVideo,
    sortie: RecepteurPaquets,
    muet: bool,
    soumises: u64,
    encodees: u64,
    total: Arc<AtomicU64>,
}

impl EncodeurVideo for VideoFactice {
    fn nom(&self) -> &str {
        "encodeur factice"
    }

    fn soumettre(&mut self, image: ImageNv12, pts: i64, duree: i64) -> Resultat<bool> {
        if image.largeur != self.parametres.largeur || image.hauteur != self.parametres.hauteur {
            return Err(Erreur::Echec("taille d'image inattendue".into()));
        }
        let cle = self
            .soumises
            .is_multiple_of(u64::from(self.parametres.images_par_groupe.max(1)));
        self.soumises += 1;
        self.total.fetch_add(1, Ordering::Relaxed);
        if self.muet {
            return Ok(true);
        }
        self.encodees += 1;
        let valeur = image.donnees.first().copied().unwrap_or(0);
        (self.sortie)(Paquet {
            pts,
            duree,
            cle,
            donnees: image_factice(cle, valeur),
        });
        Ok(true)
    }

    fn encodees(&self) -> u64 {
        self.encodees
    }
}

struct AudioFactice {
    parametres: ParametresAudio,
    sortie: RecepteurPaquets,
    attente: usize,
    blocs: u64,
}

impl EncodeurAudio for AudioFactice {
    fn soumettre(&mut self, pcm: &[i16]) -> Resultat<()> {
        self.attente += pcm.len();
        let par_bloc = BLOC_AUDIO * usize::from(self.parametres.canaux.max(1));
        let temps = |n: u64| {
            (n as i128 * BLOC_AUDIO as i128 * UNITE_TEMPS as i128
                / i128::from(self.parametres.frequence.max(1))) as i64
        };
        while self.attente >= par_bloc {
            self.attente -= par_bloc;
            let pts = temps(self.blocs);
            let duree = temps(self.blocs + 1) - pts;
            self.blocs += 1;
            (self.sortie)(Paquet {
                pts,
                duree,
                cle: true,
                donnees: vec![0x21; 8],
            });
        }
        Ok(())
    }
}

impl EncodageMateriel for EncodageFactice {
    fn disponible(&self) -> bool {
        self.etat().materiel.is_some()
    }

    fn materiel(&self, _: Option<u64>) -> Resultat<MaterielEncodage> {
        self.etat()
            .materiel
            .clone()
            .ok_or(Erreur::Indisponible("encodage vidéo matériel"))
    }

    fn video(
        &self,
        parametres: ParametresVideo,
        sortie: RecepteurPaquets,
        arret: SignalArret,
    ) -> Resultat<Box<dyn EncodeurVideo>> {
        let mut etat = self.etat();
        if etat.materiel.is_none() {
            return Err(Erreur::Indisponible("encodage vidéo matériel"));
        }
        if let Some(refus) = &etat.refus {
            return Err(Erreur::Echec(refus.clone()));
        }
        etat.videos.push(parametres);
        etat.arrets.push(arret);
        Ok(Box::new(VideoFactice {
            parametres,
            sortie,
            muet: etat.muet,
            soumises: 0,
            encodees: 0,
            total: self.images.clone(),
        }))
    }

    fn audio(
        &self,
        parametres: ParametresAudio,
        sortie: RecepteurPaquets,
    ) -> Resultat<Box<dyn EncodeurAudio>> {
        let mut etat = self.etat();
        if etat.materiel.is_none() {
            return Err(Erreur::Indisponible("encodage vidéo matériel"));
        }
        etat.audios.push(parametres);
        Ok(Box::new(AudioFactice {
            parametres,
            sortie,
            attente: 0,
            blocs: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn image(p: &ParametresVideo) -> ImageNv12 {
        ImageNv12 {
            largeur: p.largeur,
            hauteur: p.hauteur,
            donnees: vec![16; (p.largeur * p.hauteur * 3 / 2) as usize],
        }
    }

    #[test]
    fn une_image_cle_par_groupe_avec_ses_parametres() {
        let encodage = EncodageFactice::nouveau();
        let (tx, rx) = mpsc::channel();
        let mut video = encodage
            .video(
                PARAMETRES_FACTICES,
                Box::new(move |p| tx.send(p).unwrap()),
                Box::new(|_| {}),
            )
            .unwrap();
        for n in 0..61 {
            assert!(video
                .soumettre(image(&PARAMETRES_FACTICES), n * 1000, 1000)
                .unwrap());
        }
        let paquets: Vec<Paquet> = rx.try_iter().collect();
        assert_eq!(paquets.len(), 61);
        let cles: Vec<i64> = paquets.iter().filter(|p| p.cle).map(|p| p.pts).collect();
        assert_eq!(cles, [0, 30_000, 60_000]);
        assert!(paquets[0].donnees.windows(SPS.len()).any(|f| f == SPS));
        assert_eq!((video.encodees(), encodage.images()), (61, 61));
        let mauvaise = ImageNv12 {
            largeur: 2,
            hauteur: 2,
            donnees: vec![0; 6],
        };
        assert!(video.soumettre(mauvaise, 0, 0).is_err());
    }

    #[test]
    fn muet_refus_et_arret() {
        let encodage = EncodageFactice::nouveau();
        encodage.rendre_muet(true);
        let (tx, rx) = mpsc::channel();
        let (arret, arrete) = mpsc::channel();
        let mut video = encodage
            .video(
                PARAMETRES_FACTICES,
                Box::new(move |p| tx.send(p).unwrap()),
                Box::new(move |r| arret.send(r).unwrap()),
            )
            .unwrap();
        assert!(video.soumettre(image(&PARAMETRES_FACTICES), 0, 1).unwrap());
        assert_eq!((video.encodees(), rx.try_iter().count()), (0, 0));
        encodage.arreter("pilote");
        assert_eq!(arrete.try_recv().as_deref(), Ok("pilote"));
        encodage.refuser(Some("occupé"));
        assert!(encodage
            .video(PARAMETRES_FACTICES, Box::new(|_| {}), Box::new(|_| {}))
            .is_err());
        assert_eq!(encodage.videos().len(), 1);
        let sans = EncodageFactice::avec(None);
        assert!(!sans.disponible() && sans.materiel(None).is_err());
    }

    #[test]
    fn l_audio_sort_par_blocs_de_1024() {
        let encodage = EncodageFactice::nouveau();
        let (tx, rx) = mpsc::channel();
        let mut audio = encodage
            .audio(
                ParametresAudio {
                    frequence: 48_000,
                    canaux: 2,
                    debit_kbps: 160,
                },
                Box::new(move |p| tx.send(p).unwrap()),
            )
            .unwrap();
        audio.soumettre(&[0; 2048 * 2 + 10]).unwrap();
        let paquets: Vec<Paquet> = rx.try_iter().collect();
        assert_eq!(paquets.len(), 2);
        assert_eq!(paquets[1].pts, 1024 * UNITE_TEMPS / 48_000);
        assert_eq!(encodage.audios().len(), 1);
    }
}
