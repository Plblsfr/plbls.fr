use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_plateforme::{
    DemandeCamera, ErreurCamera, FluxCamera, FormatPixels, ImageCamera, InfoCamera, RappelCamera,
    SystemeCamera,
};
use serde::{Deserialize, Serialize};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

use super::partage::Degradation;
use super::video::{ImageI420, Mire, RecepteurVideo};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QualiteCamera {
    #[serde(rename = "360p")]
    Q360,
    #[default]
    #[serde(rename = "720p")]
    Q720,
    #[serde(rename = "1080p")]
    Q1080,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatVideo {
    pub largeur: u32,
    pub hauteur: u32,
    pub images_par_seconde: u32,
    pub debit: u64,
    pub simulcast: bool,
    pub degradation: Degradation,
    pub ecran: bool,
}

impl QualiteCamera {
    pub const TOUTES: [QualiteCamera; 3] = [Self::Q360, Self::Q720, Self::Q1080];

    pub fn format(self) -> FormatVideo {
        let (largeur, hauteur, debit) = match self {
            Self::Q360 => (640, 360, 700_000),
            Self::Q720 => (1280, 720, 2_200_000),
            Self::Q1080 => (1920, 1080, 4_000_000),
        };
        FormatVideo {
            largeur,
            hauteur,
            images_par_seconde: 30,
            debit,
            simulcast: true,
            degradation: Degradation::Auto,
            ecran: false,
        }
    }

    pub fn demande(self) -> DemandeCamera {
        let f = self.format();
        DemandeCamera {
            largeur: f.largeur,
            hauteur: f.hauteur,
            images_par_seconde: f.images_par_seconde,
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Q360 => "360p (connexion faible)",
            Self::Q720 => "720p (recommandé)",
            Self::Q1080 => "1080p (connexion rapide)",
        }
    }

    pub fn debit_affiche(self) -> String {
        let mbits = self.format().debit as f64 / 1e6;
        let texte = format!("{mbits:.1}").replace('.', ",").replace(",0", "");
        format!("Débit montant : environ {texte} Mb/s.")
    }
}

pub fn message_camera(erreur: &ErreurCamera) -> String {
    match erreur {
        ErreurCamera::Absente => "Aucune caméra trouvée.",
        ErreurCamera::Introuvable => "Caméra introuvable : elle a peut-être été débranchée.",
        ErreurCamera::Occupee => "Caméra déjà utilisée par une autre application.",
        ErreurCamera::Refusee => "Accès à la caméra refusé.",
        ErreurCamera::Debranchee => "Caméra débranchée.",
        ErreurCamera::Autre(_) => "Caméra indisponible.",
    }
    .to_string()
}

fn decoder_mjpeg(image: &ImageCamera<'_>) -> Option<ImageI420> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decodeur =
        zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(image.donnees), options);
    let rgb = decodeur.decode().ok()?;
    let (largeur, hauteur) = decodeur.dimensions()?;
    let (largeur, hauteur) = (largeur as u32, hauteur as u32);
    (rgb.len() >= (largeur * hauteur * 3) as usize)
        .then(|| ImageI420::depuis_rgb(largeur, hauteur, &rgb, largeur as usize * 3))
}

pub fn image_i420(image: &ImageCamera<'_>) -> Option<ImageI420> {
    let (l, h) = (image.largeur, image.hauteur);
    if l == 0 || h == 0 {
        return None;
    }
    let pixels = (l * h) as usize;
    let d = image.donnees;
    match image.format {
        FormatPixels::Mjpeg => decoder_mjpeg(image),
        FormatPixels::Yuyv if d.len() >= pixels * 2 => {
            Some(ImageI420::depuis_yuyv(l, h, d, l as usize * 2))
        }
        FormatPixels::Nv12 if d.len() >= pixels + pixels / 2 => Some(ImageI420::depuis_nv12(
            l,
            h,
            &d[..pixels],
            l as usize,
            &d[pixels..],
            l.div_ceil(2) as usize * 2,
        )),
        FormatPixels::Gris if d.len() >= pixels => {
            Some(ImageI420::depuis_gris(l, h, d, l as usize))
        }
        FormatPixels::Rgb if d.len() >= pixels * 3 => {
            Some(ImageI420::depuis_rgb(l, h, d, l as usize * 3))
        }
        FormatPixels::Bgr if d.len() >= pixels * 3 => {
            let rgb: Vec<u8> = d[..pixels * 3]
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect();
            Some(ImageI420::depuis_rgb(l, h, &rgb, l as usize * 3))
        }
        _ => None,
    }
}

pub type SortieImages = Arc<dyn Fn(Arc<ImageI420>) + Send + Sync>;
pub type SignalCamera = Arc<dyn Fn(ErreurCamera) + Send + Sync>;

pub struct CaptureCamera {
    flux: Box<dyn FluxCamera>,
}

impl CaptureCamera {
    pub fn ouvrir(
        systeme: &dyn SystemeCamera,
        id: Option<&str>,
        qualite: QualiteCamera,
        sortie: SortieImages,
        perte: SignalCamera,
    ) -> Result<Self, ErreurCamera> {
        let rappel: RappelCamera = Box::new(move |resultat| match resultat {
            Ok(image) => {
                if let Some(i420) = image_i420(&image) {
                    sortie(Arc::new(i420));
                }
            }
            Err(e) => perte(e),
        });
        let flux = systeme.ouvrir(id, qualite.demande(), rappel)?;
        Ok(Self { flux })
    }

    pub fn format(&self) -> DemandeCamera {
        self.flux.format()
    }
}

pub struct ApercuCamera {
    _capture: CaptureCamera,
    erreur: Arc<Mutex<Option<ErreurCamera>>>,
}

impl ApercuCamera {
    pub fn ouvrir(
        systeme: &dyn SystemeCamera,
        id: Option<&str>,
        qualite: QualiteCamera,
        recepteur: Arc<RecepteurVideo>,
        alerte: Option<SignalCamera>,
    ) -> Result<Self, ErreurCamera> {
        let erreur = Arc::new(Mutex::new(None));
        let capture = {
            let erreur = erreur.clone();
            let recepteur_perte = recepteur.clone();
            CaptureCamera::ouvrir(
                systeme,
                id,
                qualite,
                Arc::new(move |image| recepteur.pousser(image)),
                Arc::new(move |e| {
                    if let Ok(mut erreur) = erreur.lock() {
                        *erreur = Some(e.clone());
                    }
                    recepteur_perte.effacer();
                    if let Some(alerte) = &alerte {
                        alerte(e);
                    }
                }),
            )?
        };
        Ok(Self {
            _capture: capture,
            erreur,
        })
    }

    pub fn erreur(&self) -> Option<ErreurCamera> {
        self.erreur.lock().ok().and_then(|e| e.clone())
    }
}

struct CameraFactice {
    nom: String,
    occupee: bool,
}

struct FluxOuvert {
    id: String,
    demande: DemandeCamera,
    rappel: RappelCamera,
    ouvert: Arc<AtomicBool>,
}

#[derive(Default)]
struct EtatFactice {
    cameras: BTreeMap<String, CameraFactice>,
    ouverts: Vec<FluxOuvert>,
    refus: Option<ErreurCamera>,
    numero: u64,
}

#[derive(Clone, Default)]
pub struct CamerasFactices {
    etat: Arc<Mutex<EtatFactice>>,
}

struct FluxFactice {
    demande: DemandeCamera,
    ouvert: Arc<AtomicBool>,
}

impl FluxCamera for FluxFactice {
    fn format(&self) -> DemandeCamera {
        self.demande
    }
}

impl Drop for FluxFactice {
    fn drop(&mut self) {
        self.ouvert.store(false, Ordering::SeqCst);
    }
}

fn yuyv_de(image: &ImageI420) -> Vec<u8> {
    let (l, h) = (image.largeur as usize, image.hauteur as usize);
    let cl = image.largeur_chroma() as usize;
    let mut sortie = Vec::with_capacity(l * h * 2);
    for y in 0..h {
        for x in (0..l).step_by(2) {
            let c = (y / 2) * cl + x / 2;
            sortie.extend([
                image.y[y * l + x],
                image.u[c],
                image.y[y * l + (x + 1).min(l - 1)],
                image.v[c],
            ]);
        }
    }
    sortie
}

impl CamerasFactices {
    pub fn nouvelles() -> Self {
        Self::default()
    }

    fn etat(&self) -> MutexGuard<'_, EtatFactice> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn brancher(&self, id: &str, nom: &str) {
        self.etat().cameras.insert(
            id.to_string(),
            CameraFactice {
                nom: nom.to_string(),
                occupee: false,
            },
        );
    }

    pub fn occuper(&self, id: &str, occupee: bool) {
        if let Some(c) = self.etat().cameras.get_mut(id) {
            c.occupee = occupee;
        }
    }

    pub fn refuser(&self, refus: Option<ErreurCamera>) {
        self.etat().refus = refus;
    }

    pub fn debrancher(&self, id: &str) {
        let partis: Vec<FluxOuvert> = {
            let mut etat = self.etat();
            etat.cameras.remove(id);
            let (partis, restants) = std::mem::take(&mut etat.ouverts)
                .into_iter()
                .partition(|f| f.id == id);
            etat.ouverts = restants;
            partis
        };
        for mut flux in partis {
            if flux.ouvert.load(Ordering::SeqCst) {
                (flux.rappel)(Err(ErreurCamera::Debranchee));
            }
        }
    }

    pub fn ouvertes(&self) -> Vec<String> {
        let mut etat = self.etat();
        etat.ouverts.retain(|f| f.ouvert.load(Ordering::SeqCst));
        etat.ouverts.iter().map(|f| f.id.clone()).collect()
    }

    pub fn pousser(&self, images: usize) -> usize {
        let mut etat = self.etat();
        etat.ouverts.retain(|f| f.ouvert.load(Ordering::SeqCst));
        let mut envoyees = 0;
        for _ in 0..images {
            etat.numero += 1;
            let numero = etat.numero;
            for flux in etat.ouverts.iter_mut() {
                let mire = Mire::nouvelle(flux.demande.largeur, flux.demande.hauteur);
                let yuyv = yuyv_de(&mire.image(numero));
                (flux.rappel)(Ok(ImageCamera {
                    largeur: flux.demande.largeur,
                    hauteur: flux.demande.hauteur,
                    format: FormatPixels::Yuyv,
                    donnees: &yuyv,
                }));
                envoyees += 1;
            }
        }
        envoyees
    }
}

impl SystemeCamera for CamerasFactices {
    fn cameras(&self) -> Result<Vec<InfoCamera>, ErreurCamera> {
        Ok(self
            .etat()
            .cameras
            .iter()
            .map(|(id, c)| InfoCamera {
                id: id.clone(),
                nom: c.nom.clone(),
            })
            .collect())
    }

    fn ouvrir(
        &self,
        id: Option<&str>,
        demande: DemandeCamera,
        rappel: RappelCamera,
    ) -> Result<Box<dyn FluxCamera>, ErreurCamera> {
        let mut etat = self.etat();
        if let Some(refus) = etat.refus.clone() {
            return Err(refus);
        }
        let id = match id {
            Some(id) => id.to_string(),
            None => etat
                .cameras
                .keys()
                .next()
                .cloned()
                .ok_or(ErreurCamera::Absente)?,
        };
        let camera = etat.cameras.get(&id).ok_or(ErreurCamera::Introuvable)?;
        if camera.occupee {
            return Err(ErreurCamera::Occupee);
        }
        let ouvert = Arc::new(AtomicBool::new(true));
        etat.ouverts.push(FluxOuvert {
            id,
            demande,
            rappel,
            ouvert: ouvert.clone(),
        });
        Ok(Box::new(FluxFactice { demande, ouvert }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn les_qualites_reprennent_celles_du_front() {
        assert_eq!(QualiteCamera::default(), QualiteCamera::Q720);
        let f = QualiteCamera::Q1080.format();
        assert_eq!(
            (f.largeur, f.hauteur, f.images_par_seconde),
            (1920, 1080, 30)
        );
        assert_eq!(f.debit, 4_000_000);
        assert!(f.simulcast);
        assert_eq!(
            QualiteCamera::Q360.debit_affiche(),
            "Débit montant : environ 0,7 Mb/s."
        );
        assert_eq!(
            QualiteCamera::Q1080.debit_affiche(),
            "Débit montant : environ 4 Mb/s."
        );
        assert_eq!(
            serde_json::to_string(&QualiteCamera::Q360).unwrap(),
            "\"360p\""
        );
    }

    #[test]
    fn une_image_mjpeg_est_decodee() {
        let jpeg = include_bytes!("../../tests/donnees/mire.jpg");
        let image = image_i420(&ImageCamera {
            largeur: 64,
            hauteur: 32,
            format: FormatPixels::Mjpeg,
            donnees: jpeg,
        })
        .unwrap();
        assert_eq!((image.largeur, image.hauteur), (64, 32));
        let [r, g, b] = image.pixel(4, 16);
        assert!(r > 150 && g < 90 && b < 90, "{r} {g} {b}");
        let [r, g, b] = image.pixel(60, 16);
        assert!(b > 150 && r < 90 && g < 90, "{r} {g} {b}");
        assert!(image_i420(&ImageCamera {
            largeur: 64,
            hauteur: 32,
            format: FormatPixels::Mjpeg,
            donnees: b"pas une image",
        })
        .is_none());
    }

    #[test]
    fn une_image_trop_courte_est_ignoree() {
        assert!(image_i420(&ImageCamera {
            largeur: 8,
            hauteur: 8,
            format: FormatPixels::Yuyv,
            donnees: &[0; 10],
        })
        .is_none());
        let bgr = [10u8, 20, 200].repeat(4);
        let image = image_i420(&ImageCamera {
            largeur: 2,
            hauteur: 2,
            format: FormatPixels::Bgr,
            donnees: &bgr,
        })
        .unwrap();
        let [r, _, b] = image.pixel(0, 0);
        assert!(r > 150 && b < 60);
    }

    #[test]
    fn l_apercu_recoit_les_images_et_signale_la_perte() {
        let cameras = CamerasFactices::nouvelles();
        let recepteur = RecepteurVideo::nouveau();
        assert_eq!(
            ApercuCamera::ouvrir(&cameras, None, QualiteCamera::Q360, recepteur.clone(), None)
                .err(),
            Some(ErreurCamera::Absente)
        );
        cameras.brancher("c1", "Caméra intégrée");
        cameras.brancher("c2", "Webcam USB");
        cameras.occuper("c2", true);
        assert_eq!(
            ApercuCamera::ouvrir(
                &cameras,
                Some("c2"),
                QualiteCamera::Q360,
                recepteur.clone(),
                None
            )
            .err(),
            Some(ErreurCamera::Occupee)
        );
        assert_eq!(
            ApercuCamera::ouvrir(
                &cameras,
                Some("c9"),
                QualiteCamera::Q360,
                recepteur.clone(),
                None
            )
            .err(),
            Some(ErreurCamera::Introuvable)
        );
        let apercu =
            ApercuCamera::ouvrir(&cameras, None, QualiteCamera::Q360, recepteur.clone(), None)
                .unwrap();
        assert_eq!(cameras.ouvertes(), vec!["c1".to_string()]);
        assert_eq!(cameras.pousser(2), 2);
        assert_eq!(recepteur.images_recues(), 2);
        let image = recepteur.derniere().unwrap();
        assert_eq!((image.largeur(), image.hauteur()), (640, 360));
        cameras.debrancher("c1");
        assert_eq!(apercu.erreur(), Some(ErreurCamera::Debranchee));
        assert!(recepteur.derniere().is_none());
        drop(apercu);
        assert!(cameras.ouvertes().is_empty());
    }

    #[test]
    fn fermer_la_capture_libere_la_camera() {
        let cameras = CamerasFactices::nouvelles();
        cameras.brancher("c1", "Caméra");
        let recues = Arc::new(AtomicUsize::new(0));
        let capture = {
            let recues = recues.clone();
            CaptureCamera::ouvrir(
                &cameras,
                Some("c1"),
                QualiteCamera::Q720,
                Arc::new(move |_| {
                    recues.fetch_add(1, Ordering::SeqCst);
                }),
                Arc::new(|_| {}),
            )
            .unwrap()
        };
        assert_eq!(capture.format().largeur, 1280);
        cameras.pousser(1);
        drop(capture);
        assert_eq!(cameras.pousser(1), 0);
        assert_eq!(recues.load(Ordering::SeqCst), 1);
        assert_eq!(
            message_camera(&ErreurCamera::Occupee),
            "Caméra déjà utilisée par une autre application."
        );
    }
}
