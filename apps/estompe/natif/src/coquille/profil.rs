use std::io::Cursor;
use std::path::Path;

use estompe_client::profil::{carre, Recadrage, COTE_ENVOYE, LIMITE_PNG};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageEncoder, RgbaImage};

pub const COTE_SOURCE_MAX: u32 = 2048;
pub const LECTURE_MAX: u64 = 32 * 1024 * 1024;
pub const QUALITE_JPEG: u8 = 85;
pub const EXTENSIONS_IMAGES: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

pub fn ouvrir_image(octets: &[u8]) -> Option<RgbaImage> {
    let image = image::load_from_memory(octets).ok()?;
    if image.width() == 0 || image.height() == 0 {
        return None;
    }
    let image = if image.width().max(image.height()) > COTE_SOURCE_MAX {
        image.resize(COTE_SOURCE_MAX, COTE_SOURCE_MAX, FilterType::Triangle)
    } else {
        image
    };
    Some(image.into_rgba8())
}

pub fn lire_image(chemin: &Path) -> Option<RgbaImage> {
    let taille = std::fs::metadata(chemin).ok()?.len();
    if taille > LECTURE_MAX {
        return None;
    }
    ouvrir_image(&std::fs::read(chemin).ok()?)
}

pub fn decoupe(largeur: u32, hauteur: u32, r: Recadrage) -> (u32, u32, u32) {
    let c = carre(largeur, hauteur, r);
    let cote = (c.cote.round() as u32).clamp(1, largeur.min(hauteur).max(1));
    let x = (c.x.round().max(0.0) as u32).min(largeur.saturating_sub(cote));
    let y = (c.y.round().max(0.0) as u32).min(hauteur.saturating_sub(cote));
    (x, y, cote)
}

pub fn recadrer(image: &RgbaImage, r: Recadrage, cote: u32) -> RgbaImage {
    let (x, y, c) = decoupe(image.width(), image.height(), r);
    let morceau = image::imageops::crop_imm(image, x, y, c, c).to_image();
    if c == cote {
        morceau
    } else {
        image::imageops::resize(&morceau, cote, cote, FilterType::CatmullRom)
    }
}

pub fn encoder(image: &RgbaImage) -> Result<(&'static str, Vec<u8>), String> {
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    if png.len() <= LIMITE_PNG {
        return Ok(("image/png", png));
    }
    let rgb = DynamicImage::ImageRgba8(image.clone()).into_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), QUALITE_JPEG)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    Ok(("image/jpeg", jpeg))
}

pub fn avatar_recadre(image: &RgbaImage, r: Recadrage) -> Result<(&'static str, Vec<u8>), String> {
    encoder(&recadrer(image, r, COTE_ENVOYE))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUGE: [u8; 4] = [220, 30, 40, 255];
    const VERT: [u8; 4] = [30, 200, 60, 255];
    const BLEU: [u8; 4] = [30, 60, 220, 255];
    const JAUNE: [u8; 4] = [240, 210, 20, 255];

    fn quadrants() -> RgbaImage {
        RgbaImage::from_fn(800, 400, |x, y| {
            image::Rgba(match (x < 400, y < 200) {
                (true, true) => ROUGE,
                (false, true) => VERT,
                (true, false) => BLEU,
                (false, false) => JAUNE,
            })
        })
    }

    fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        image.get_pixel(x, y).0
    }

    #[test]
    fn le_recadrage_centre_garde_le_carre_du_milieu() {
        let sortie = recadrer(&quadrants(), Recadrage::default(), COTE_ENVOYE);
        assert_eq!(sortie.dimensions(), (256, 256));
        assert_eq!(pixel(&sortie, 10, 10), ROUGE);
        assert_eq!(pixel(&sortie, 245, 10), VERT);
        assert_eq!(pixel(&sortie, 10, 245), BLEU);
        assert_eq!(pixel(&sortie, 245, 245), JAUNE);
    }

    #[test]
    fn zoom_et_decalage_donnent_le_pixel_attendu() {
        let image = quadrants();
        let r = Recadrage {
            zoom: 2.0,
            x: 1.0,
            y: -1.0,
        };
        assert_eq!(decoupe(800, 400, r), (600, 0, 200));
        let sortie = recadrer(&image, r, COTE_ENVOYE);
        assert!(sortie.pixels().all(|p| p.0 == VERT), "coin haut droit");
        let r = Recadrage {
            zoom: 4.0,
            x: -1.0,
            y: 1.0,
        };
        assert_eq!(decoupe(800, 400, r), (0, 300, 100));
        assert_eq!(pixel(&recadrer(&image, r, 64), 32, 32), BLEU);
        let r = Recadrage {
            zoom: 2.0,
            x: 0.0,
            y: 0.0,
        };
        let sortie = recadrer(&image, r, 200);
        assert_eq!(decoupe(800, 400, r), (300, 100, 200));
        assert_eq!(pixel(&sortie, 50, 50), ROUGE);
        assert_eq!(pixel(&sortie, 150, 50), VERT);
        assert_eq!(pixel(&sortie, 50, 150), BLEU);
        assert_eq!(pixel(&sortie, 150, 150), JAUNE);
    }

    #[test]
    fn une_petite_image_est_agrandie_et_une_grande_reduite_a_l_ouverture() {
        let petite = RgbaImage::from_pixel(40, 30, image::Rgba(ROUGE));
        let sortie = recadrer(&petite, Recadrage::default(), COTE_ENVOYE);
        assert_eq!(sortie.dimensions(), (256, 256));
        assert_eq!(pixel(&sortie, 128, 128), ROUGE);
        let mut octets = Cursor::new(Vec::new());
        RgbaImage::from_pixel(3000, 1000, image::Rgba(BLEU))
            .write_to(&mut octets, image::ImageFormat::Png)
            .unwrap();
        let ouverte = ouvrir_image(&octets.into_inner()).unwrap();
        assert_eq!(ouverte.width(), 2048);
        assert!(
            (682..=683).contains(&ouverte.height()),
            "{}",
            ouverte.height()
        );
        assert!(ouvrir_image(b"pas une image").is_none());
    }

    #[test]
    fn l_avatar_part_en_png_sauf_s_il_reste_trop_lourd() {
        let (mime, octets) = avatar_recadre(&quadrants(), Recadrage::default()).unwrap();
        assert_eq!(mime, "image/png");
        let relue = image::load_from_memory(&octets).unwrap().into_rgba8();
        assert_eq!(relue.dimensions(), (256, 256));
        assert_eq!(pixel(&relue, 245, 245), JAUNE);
        let mut graine = 7u32;
        let bruit = RgbaImage::from_fn(256, 256, |_, _| {
            graine = graine.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let o = graine.to_le_bytes();
            image::Rgba([o[1], o[2], o[3], 255])
        });
        let (mime, octets) = encoder(&bruit).unwrap();
        assert_eq!(mime, "image/jpeg");
        assert!(octets.len() < LIMITE_PNG, "{}", octets.len());
    }
}
