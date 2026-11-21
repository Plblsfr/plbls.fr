use estompe_plateforme::Image;

const ICONE: &[u8] = include_bytes!("../../icones/estompe.png");
pub const COTE_ICONE: u32 = 64;
pub const INFO_ZONE: &str = "Estompe natif (bêta)";

pub fn icone_de_l_app() -> Image {
    let Ok(image) = image::load_from_memory(ICONE) else {
        return Image::default();
    };
    let image = image
        .resize_exact(
            COTE_ICONE,
            COTE_ICONE,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    Image {
        largeur: image.width(),
        hauteur: image.height(),
        rgba: image.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn l_icone_de_la_zone_est_decodee() {
        let icone = super::icone_de_l_app();
        assert_eq!((icone.largeur, icone.hauteur), (64, 64));
        assert_eq!(icone.rgba.len(), 64 * 64 * 4);
        assert!(icone.rgba.chunks(4).any(|p| p[3] > 0));
    }
}
