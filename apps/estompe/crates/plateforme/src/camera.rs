use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoCamera {
    pub id: String,
    pub nom: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatPixels {
    Mjpeg,
    Yuyv,
    Nv12,
    Gris,
    Rgb,
    Bgr,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageCamera<'a> {
    pub largeur: u32,
    pub hauteur: u32,
    pub format: FormatPixels,
    pub donnees: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemandeCamera {
    pub largeur: u32,
    pub hauteur: u32,
    pub images_par_seconde: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurCamera {
    Absente,
    Introuvable,
    Occupee,
    Refusee,
    Debranchee,
    Autre(String),
}

impl fmt::Display for ErreurCamera {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absente => f.write_str("aucune caméra"),
            Self::Introuvable => f.write_str("caméra introuvable"),
            Self::Occupee => f.write_str("caméra occupée"),
            Self::Refusee => f.write_str("accès à la caméra refusé"),
            Self::Debranchee => f.write_str("caméra débranchée"),
            Self::Autre(e) => write!(f, "caméra indisponible : {e}"),
        }
    }
}

pub type RappelCamera = Box<dyn FnMut(Result<ImageCamera<'_>, ErreurCamera>) + Send>;

pub trait FluxCamera: Send {
    fn format(&self) -> DemandeCamera;
}

pub trait SystemeCamera: Send + Sync {
    fn cameras(&self) -> Result<Vec<InfoCamera>, ErreurCamera>;
    fn ouvrir(
        &self,
        id: Option<&str>,
        demande: DemandeCamera,
        rappel: RappelCamera,
    ) -> Result<Box<dyn FluxCamera>, ErreurCamera>;
}

pub fn erreur_de_texte(texte: &str) -> ErreurCamera {
    let t = texte.to_lowercase();
    if t.contains("busy") || t.contains("in use") || t.contains("0xc00d3704") {
        ErreurCamera::Occupee
    } else if t.contains("permission") || t.contains("denied") || t.contains("0x80070005") {
        ErreurCamera::Refusee
    } else if t.contains("no such device") || t.contains("disconnected") || t.contains("0xc00d36b2")
    {
        ErreurCamera::Debranchee
    } else if t.contains("no such") || t.contains("not found") || t.contains("introuvable") {
        ErreurCamera::Introuvable
    } else {
        ErreurCamera::Autre(texte.to_string())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CamerasIndisponibles;

impl SystemeCamera for CamerasIndisponibles {
    fn cameras(&self) -> Result<Vec<InfoCamera>, ErreurCamera> {
        Ok(Vec::new())
    }

    fn ouvrir(
        &self,
        _: Option<&str>,
        _: DemandeCamera,
        _: RappelCamera,
    ) -> Result<Box<dyn FluxCamera>, ErreurCamera> {
        Err(ErreurCamera::Absente)
    }
}

#[cfg(any(target_os = "linux", windows))]
pub use systeme::CamerasSysteme;

#[cfg(not(any(target_os = "linux", windows)))]
pub type CamerasSysteme = CamerasIndisponibles;

#[cfg(any(target_os = "linux", windows))]
mod systeme {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{
        ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
    };
    use nokhwa::{Camera, NokhwaError};

    use super::{
        erreur_de_texte, DemandeCamera, ErreurCamera, FluxCamera, FormatPixels, ImageCamera,
        InfoCamera, RappelCamera, SystemeCamera,
    };

    const OUVERTURE: Duration = Duration::from_secs(8);
    const ECHECS_AVANT_DEBRANCHEE: u32 = 10;

    #[derive(Debug, Clone, Copy, Default)]
    pub struct CamerasSysteme;

    fn erreur(e: &NokhwaError) -> ErreurCamera {
        erreur_de_texte(&e.to_string())
    }

    fn format_pixels(f: FrameFormat) -> FormatPixels {
        match f {
            FrameFormat::MJPEG => FormatPixels::Mjpeg,
            FrameFormat::YUYV => FormatPixels::Yuyv,
            FrameFormat::NV12 => FormatPixels::Nv12,
            FrameFormat::GRAY => FormatPixels::Gris,
            FrameFormat::RAWRGB => FormatPixels::Rgb,
            FrameFormat::RAWBGR => FormatPixels::Bgr,
        }
    }

    fn indice(id: &str) -> CameraIndex {
        match id.parse::<u32>() {
            Ok(n) => CameraIndex::Index(n),
            Err(_) => CameraIndex::String(id.to_string()),
        }
    }

    fn lister() -> Result<Vec<InfoCamera>, ErreurCamera> {
        Ok(nokhwa::query(ApiBackend::Auto)
            .map_err(|e| erreur(&e))?
            .into_iter()
            .map(|c| InfoCamera {
                id: c.index().to_string(),
                nom: c.human_name(),
            })
            .collect())
    }

    struct FluxSysteme {
        arret: Arc<AtomicBool>,
        format: DemandeCamera,
        fil: Option<JoinHandle<()>>,
    }

    impl FluxCamera for FluxSysteme {
        fn format(&self) -> DemandeCamera {
            self.format
        }
    }

    impl Drop for FluxSysteme {
        fn drop(&mut self) {
            self.arret.store(true, Ordering::SeqCst);
            if let Some(fil) = self.fil.take() {
                let _ = fil.join();
            }
        }
    }

    fn ouvrir_camera(id: Option<&str>, demande: DemandeCamera) -> Result<Camera, ErreurCamera> {
        let index = match id {
            Some(id) => indice(id),
            None => lister()?
                .first()
                .map(|c| indice(&c.id))
                .ok_or(ErreurCamera::Absente)?,
        };
        let voulu = CameraFormat::new_from(
            demande.largeur,
            demande.hauteur,
            FrameFormat::MJPEG,
            demande.images_par_seconde,
        );
        let requete = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(voulu));
        let mut camera = Camera::new(index, requete).map_err(|e| erreur(&e))?;
        camera.open_stream().map_err(|e| erreur(&e))?;
        Ok(camera)
    }

    impl SystemeCamera for CamerasSysteme {
        fn cameras(&self) -> Result<Vec<InfoCamera>, ErreurCamera> {
            lister()
        }

        fn ouvrir(
            &self,
            id: Option<&str>,
            demande: DemandeCamera,
            mut rappel: RappelCamera,
        ) -> Result<Box<dyn FluxCamera>, ErreurCamera> {
            let arret = Arc::new(AtomicBool::new(false));
            let (envoi, reception) = mpsc::channel();
            let id = id.map(str::to_string);
            let fil = {
                let arret = arret.clone();
                std::thread::Builder::new()
                    .name("camera".into())
                    .spawn(move || {
                        let mut camera = match ouvrir_camera(id.as_deref(), demande) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = envoi.send(Err(e));
                                return;
                            }
                        };
                        let format = camera.camera_format();
                        let _ = envoi.send(Ok(DemandeCamera {
                            largeur: format.width(),
                            hauteur: format.height(),
                            images_par_seconde: format.frame_rate(),
                        }));
                        let mut echecs = 0;
                        while !arret.load(Ordering::SeqCst) {
                            let (largeur, hauteur) =
                                (camera.resolution().width(), camera.resolution().height());
                            let format = format_pixels(camera.frame_format());
                            match camera.frame_raw() {
                                Ok(donnees) => {
                                    echecs = 0;
                                    rappel(Ok(ImageCamera {
                                        largeur,
                                        hauteur,
                                        format,
                                        donnees: &donnees,
                                    }));
                                }
                                Err(e) => {
                                    echecs += 1;
                                    let cause = erreur(&e);
                                    if echecs >= ECHECS_AVANT_DEBRANCHEE
                                        || cause == ErreurCamera::Debranchee
                                    {
                                        if !arret.load(Ordering::SeqCst) {
                                            rappel(Err(ErreurCamera::Debranchee));
                                        }
                                        break;
                                    }
                                    std::thread::sleep(Duration::from_millis(20));
                                }
                            }
                        }
                        let _ = camera.stop_stream();
                    })
                    .map_err(|e| ErreurCamera::Autre(e.to_string()))?
            };
            match reception.recv_timeout(OUVERTURE) {
                Ok(Ok(format)) => Ok(Box::new(FluxSysteme {
                    arret,
                    format,
                    fil: Some(fil),
                })),
                Ok(Err(e)) => {
                    let _ = fil.join();
                    Err(e)
                }
                Err(_) => {
                    arret.store(true, Ordering::SeqCst);
                    Err(ErreurCamera::Autre("ouverture trop longue".into()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_messages_du_systeme_deviennent_une_cause() {
        assert_eq!(
            erreur_de_texte("Could not open device stream: Device or resource busy (os error 16)"),
            ErreurCamera::Occupee
        );
        assert_eq!(
            erreur_de_texte("Could not open device /dev/video0: Permission denied"),
            ErreurCamera::Refusee
        );
        assert_eq!(
            erreur_de_texte("Could not open device 3: No such file or directory"),
            ErreurCamera::Introuvable
        );
        assert_eq!(
            erreur_de_texte("Could not capture frame: No such device"),
            ErreurCamera::Debranchee
        );
        assert!(matches!(erreur_de_texte("bizarre"), ErreurCamera::Autre(_)));
    }

    #[test]
    fn sans_camera_l_ouverture_est_refusee() {
        let systeme = CamerasIndisponibles;
        assert!(systeme.cameras().unwrap().is_empty());
        let ouverture = systeme.ouvrir(
            None,
            DemandeCamera {
                largeur: 1280,
                hauteur: 720,
                images_par_seconde: 30,
            },
            Box::new(|_| {}),
        );
        assert_eq!(ouverture.err(), Some(ErreurCamera::Absente));
    }
}
