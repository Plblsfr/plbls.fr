use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use estompe_client::fichiers::{
    changer_extension, dimensions_a_envoyer, garder_la_version_compressee, type_mime_du_nom,
    Dimensions, MIMES_IMAGE,
};
use estompe_client::televersement::SourceEnvoi;
use estompe_client::video::{a_convertir, codec_mp4, FichierTemporaire};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{imageops::FilterType, DynamicImage, ImageEncoder, RgbaImage};

use super::medias::{decoder_original, reduire, Format};
use super::saisie::{SourcePiece, Vignette};

pub const NOM_DE_CAPTURE: &str = "capture.png";
pub const LECTURE_IMAGE_MAX: u64 = 64 * 1024 * 1024;
pub const QUALITE_JPEG: u8 = 82;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ajout {
    Chemin(PathBuf),
    Image(estompe_plateforme::Image),
}

impl Ajout {
    pub fn nom(&self) -> String {
        match self {
            Self::Chemin(chemin) => chemin
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| chemin.display().to_string()),
            Self::Image(_) => NOM_DE_CAPTURE.to_string(),
        }
    }
}

pub struct Preparee {
    pub source: SourcePiece,
    pub taille: u64,
    pub vignette: Option<Vignette>,
}

fn en_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut sortie = Vec::new();
    PngEncoder::new(&mut sortie)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(sortie)
}

pub fn compresser(
    image: &RgbaImage,
    mime: &str,
    cible: Dimensions,
) -> Option<(Vec<u8>, &'static str, &'static str)> {
    let reduite = if (cible.largeur, cible.hauteur) == image.dimensions() {
        image.clone()
    } else {
        image::imageops::resize(image, cible.largeur, cible.hauteur, FilterType::CatmullRom)
    };
    let mut sortie = Vec::new();
    if mime == "image/jpeg" {
        let rgb = DynamicImage::ImageRgba8(reduite).into_rgb8();
        JpegEncoder::new_with_quality(Cursor::new(&mut sortie), QUALITE_JPEG)
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .ok()?;
        Some((sortie, "jpg", "image/jpeg"))
    } else {
        WebPEncoder::new_lossless(&mut sortie)
            .write_image(
                reduite.as_raw(),
                reduite.width(),
                reduite.height(),
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
        Some((sortie, "webp", "image/webp"))
    }
}

fn convertir_la_video(
    preparee: &mut Preparee,
    convertir: &dyn Fn(&Path) -> Option<FichierTemporaire>,
) {
    let SourceEnvoi::Chemin(chemin) = &preparee.source.source else {
        return;
    };
    let codec = std::fs::File::open(chemin)
        .and_then(|mut f| codec_mp4(&mut f))
        .ok()
        .flatten();
    if !a_convertir(&preparee.source.type_mime, codec.as_deref()) {
        return;
    }
    let Some(sortie) = convertir(chemin) else {
        return;
    };
    let Ok(taille) = std::fs::metadata(sortie.chemin()).map(|m| m.len()) else {
        return;
    };
    preparee.taille = taille;
    preparee.source.source = SourceEnvoi::Chemin(sortie.chemin().to_path_buf());
    preparee.source.temporaire = Some(Arc::new(sortie));
}

pub fn preparer(
    ajout: Ajout,
    convertir: &dyn Fn(&Path) -> Option<FichierTemporaire>,
) -> Result<Preparee, String> {
    let nom = ajout.nom();
    let source = match ajout {
        Ajout::Chemin(chemin) => {
            if !chemin.is_file() {
                return Err(format!("« {nom} » n'est pas un fichier."));
            }
            SourceEnvoi::Chemin(chemin)
        }
        Ajout::Image(image) => {
            let rgba = RgbaImage::from_raw(image.largeur, image.hauteur, image.rgba)
                .ok_or_else(|| "Image du presse-papiers illisible.".to_string())?;
            SourceEnvoi::Octets(en_png(&rgba)?.into())
        }
    };
    let type_mime = type_mime_du_nom(&nom);
    let taille = source
        .taille()
        .map_err(|e| format!("« {nom} » est illisible : {e}"))?;
    let mut preparee = Preparee {
        source: SourcePiece {
            nom: nom.clone(),
            type_mime: type_mime.to_string(),
            source: source.clone(),
            temporaire: None,
        },
        taille,
        vignette: None,
    };
    convertir_la_video(&mut preparee, convertir);
    if !MIMES_IMAGE.contains(&type_mime) || taille > LECTURE_IMAGE_MAX {
        return Ok(preparee);
    }
    let octets = match &source {
        SourceEnvoi::Chemin(chemin) => std::fs::read(chemin).map_err(|e| e.to_string())?,
        SourceEnvoi::Octets(octets) => octets.to_vec(),
    };
    let Ok(image) = decoder_original(&octets) else {
        return Ok(preparee);
    };
    let reduite = reduire(DynamicImage::ImageRgba8(image.clone()), Format::Piece);
    preparee.vignette = Some(Vignette::clone_from_slice(
        reduite.as_raw(),
        reduite.width(),
        reduite.height(),
    ));
    let compressee = dimensions_a_envoyer(type_mime, taille, image.width(), image.height())
        .and_then(|cible| compresser(&image, type_mime, cible))
        .filter(|(o, _, _)| garder_la_version_compressee(o.len() as u64, taille));
    if let Some((octets, extension, mime)) = compressee {
        preparee.taille = octets.len() as u64;
        preparee.source = SourcePiece {
            nom: changer_extension(&nom, extension),
            type_mime: mime.to_string(),
            source: SourceEnvoi::Octets(octets.into()),
            temporaire: None,
        };
    }
    Ok(preparee)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sans_conversion(_: &Path) -> Option<FichierTemporaire> {
        panic!("aucune conversion attendue")
    }

    fn degrade(largeur: u32, hauteur: u32) -> RgbaImage {
        RgbaImage::from_fn(largeur, hauteur, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8, 255])
        })
    }

    #[test]
    fn un_fichier_ordinaire_part_tel_quel_et_sans_vignette() {
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("notes.txt");
        std::fs::write(&chemin, "bonjour").unwrap();
        let p = preparer(Ajout::Chemin(chemin.clone()), &sans_conversion).unwrap();
        assert_eq!(p.source.nom, "notes.txt");
        assert_eq!(p.source.type_mime, "text/plain");
        assert_eq!(p.source.source, SourceEnvoi::Chemin(chemin));
        assert_eq!(p.taille, 7);
        assert!(p.vignette.is_none());
        assert!(preparer(
            Ajout::Chemin(dossier.path().to_path_buf()),
            &sans_conversion
        )
        .is_err());
    }

    #[test]
    fn une_capture_collee_devient_un_png_avec_sa_vignette() {
        let image = degrade(40, 20);
        let p = preparer(
            Ajout::Image(estompe_plateforme::Image {
                largeur: 40,
                hauteur: 20,
                rgba: image.as_raw().clone(),
            }),
            &sans_conversion,
        )
        .unwrap();
        assert_eq!(p.source.nom, NOM_DE_CAPTURE);
        assert_eq!(p.source.type_mime, "image/png");
        let SourceEnvoi::Octets(octets) = &p.source.source else {
            panic!("capture en mémoire attendue");
        };
        assert_eq!(decoder_original(octets).unwrap().dimensions(), (40, 20));
        let v = p.vignette.unwrap();
        assert_eq!((v.width(), v.height()), (20, 20));
    }

    #[test]
    fn une_grande_image_est_reduite_avant_l_envoi_comme_le_front() {
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("grande.png");
        let image = RgbaImage::from_pixel(4000, 2000, image::Rgba([200, 30, 90, 255]));
        std::fs::write(&chemin, en_png(&image).unwrap()).unwrap();
        let p = preparer(Ajout::Chemin(chemin), &sans_conversion).unwrap();
        assert_eq!(p.source.nom, "grande.webp");
        assert_eq!(p.source.type_mime, "image/webp");
        let SourceEnvoi::Octets(octets) = &p.source.source else {
            panic!("version réduite attendue");
        };
        assert_eq!(decoder_original(octets).unwrap().dimensions(), (1920, 960));
    }

    #[test]
    fn une_petite_image_ou_un_gif_partent_tels_quels() {
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("petite.png");
        std::fs::write(&chemin, en_png(&degrade(64, 64)).unwrap()).unwrap();
        let p = preparer(Ajout::Chemin(chemin.clone()), &sans_conversion).unwrap();
        assert_eq!(p.source.source, SourceEnvoi::Chemin(chemin));
        assert!(p.vignette.is_some());
        let jpeg = compresser(
            &degrade(64, 64),
            "image/jpeg",
            Dimensions {
                largeur: 32,
                hauteur: 32,
            },
        )
        .unwrap();
        assert_eq!((jpeg.1, jpeg.2), ("jpg", "image/jpeg"));
        assert_eq!(decoder_original(&jpeg.0).unwrap().dimensions(), (32, 32));
    }

    fn mp4(codec: &[u8; 4]) -> Vec<u8> {
        let boite = |nom: &[u8], contenu: &[u8]| {
            [&((contenu.len() + 8) as u32).to_be_bytes(), nom, contenu].concat()
        };
        let stsd = boite(b"stsd", &[[0u8; 12].as_slice(), codec].concat());
        let trak = boite(b"trak", &[b"hdlrvide".as_slice(), &stsd].concat());
        [
            boite(b"ftyp", b"isom"),
            boite(b"moov", &trak),
            boite(b"mdat", &[0u8; 64]),
        ]
        .concat()
    }

    #[test]
    fn une_video_deja_en_h264_part_sans_reencodage() {
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("clip.mp4");
        std::fs::write(&chemin, mp4(b"avc1")).unwrap();
        let p = preparer(Ajout::Chemin(chemin.clone()), &sans_conversion).unwrap();
        assert_eq!(p.source.source, SourceEnvoi::Chemin(chemin));
        assert_eq!(p.source.type_mime, "video/mp4");
        assert!(p.source.temporaire.is_none());
        let webm = dossier.path().join("clip.webm");
        std::fs::write(&webm, b"webm").unwrap();
        let p = preparer(Ajout::Chemin(webm.clone()), &sans_conversion).unwrap();
        assert_eq!(p.source.source, SourceEnvoi::Chemin(webm));
        println!("317 C2 : une vidéo H.264 (ou WebM) part telle quelle");
    }

    #[test]
    fn une_video_hevc_part_convertie_et_l_original_sans_encodeur() {
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("clip.mp4");
        std::fs::write(&chemin, mp4(b"hvc1")).unwrap();
        let converti = dossier.path().join("estompe-h264-test.mp4");
        let appels = std::sync::Mutex::new(Vec::new());
        let p = preparer(Ajout::Chemin(chemin.clone()), &|source: &Path| {
            appels.lock().unwrap().push(source.to_path_buf());
            std::fs::write(&converti, b"converti").unwrap();
            Some(FichierTemporaire::nouveau(converti.clone()))
        })
        .unwrap();
        assert_eq!(
            appels.lock().unwrap().as_slice(),
            std::slice::from_ref(&chemin)
        );
        assert_eq!(p.source.nom, "clip.mp4");
        assert_eq!(p.source.type_mime, "video/mp4");
        assert_eq!(p.source.source, SourceEnvoi::Chemin(converti.clone()));
        assert_eq!(p.taille, 8);
        drop(p);
        assert!(
            !converti.exists(),
            "fichier converti supprimé avec la pièce"
        );
        assert!(chemin.is_file());
        let p = preparer(Ajout::Chemin(chemin.clone()), &|_: &Path| None).unwrap();
        assert_eq!(p.source.source, SourceEnvoi::Chemin(chemin));
    }
}
