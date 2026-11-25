use crate::vocal::TrameVideo;

pub const ALIGNEMENT_PIXELS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLecteur {
    largeur: u32,
    hauteur: u32,
    pas: usize,
    pixels: Vec<u32>,
}

impl ImageLecteur {
    pub fn nouvelle(largeur: u32, hauteur: u32) -> Self {
        let pas = (largeur as usize).div_ceil(ALIGNEMENT_PIXELS) * ALIGNEMENT_PIXELS;
        Self {
            largeur,
            hauteur,
            pas,
            pixels: vec![0; pas * hauteur as usize],
        }
    }

    pub fn depuis_rgb0(largeur: u32, hauteur: u32, octets: &[u8]) -> Self {
        let mut image = Self::nouvelle(largeur, hauteur);
        for y in 0..hauteur as usize {
            for x in 0..largeur as usize {
                let i = (y * largeur as usize + x) * 4;
                if let Some(p) = octets.get(i..i + 4) {
                    image.pixels[y * image.pas + x] = u32::from_ne_bytes([p[0], p[1], p[2], p[3]]);
                }
            }
        }
        image
    }

    pub fn pas_en_octets(&self) -> usize {
        self.pas * 4
    }

    pub fn tampon(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    fn rgba(&self, x: usize, y: usize) -> [u8; 4] {
        let [r, g, b, _] = self.pixels[y * self.pas + x].to_ne_bytes();
        [r, g, b, 255]
    }
}

impl TrameVideo for ImageLecteur {
    fn largeur(&self) -> u32 {
        self.largeur
    }

    fn hauteur(&self) -> u32 {
        self.hauteur
    }

    fn vers_rgba(&self, largeur: u32, hauteur: u32, sortie: &mut [u8]) {
        if self.largeur == 0 || self.hauteur == 0 || largeur == 0 || hauteur == 0 {
            return;
        }
        let (l, h) = (largeur as usize, hauteur as usize);
        let (sl, sh) = (self.largeur as usize, self.hauteur as usize);
        for (y, ligne) in sortie.chunks_exact_mut(l * 4).take(h).enumerate() {
            let sy = y * sh / h;
            for (x, pixel) in ligne.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                *pixel = self.rgba(x * sl / l, sy);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn damier() -> ImageLecteur {
        let mut octets = Vec::new();
        for y in 0..2u8 {
            for x in 0..3u8 {
                octets.extend_from_slice(&[x * 10, y * 10, 7, 0]);
            }
        }
        ImageLecteur::depuis_rgb0(3, 2, &octets)
    }

    #[test]
    fn le_pas_est_aligne_sur_64_octets() {
        let image = ImageLecteur::nouvelle(100, 3);
        assert_eq!(image.pas_en_octets(), 448);
        assert_eq!(image.pixels.len(), 112 * 3);
        assert_eq!(ImageLecteur::nouvelle(16, 1).pas_en_octets(), 64);
    }

    #[test]
    fn conversion_en_rgba_opaque_a_la_meme_taille_ou_reduite() {
        let image = damier();
        let mut sortie = vec![0; 3 * 2 * 4];
        image.vers_rgba(3, 2, &mut sortie);
        assert_eq!(&sortie[..8], &[0, 0, 7, 255, 10, 0, 7, 255]);
        assert_eq!(&sortie[20..24], &[20, 10, 7, 255]);
        let mut reduite = vec![0; 4];
        image.vers_rgba(1, 1, &mut reduite);
        assert_eq!(reduite, [0, 0, 7, 255]);
        let mut agrandie = vec![0; 6 * 4 * 4];
        image.vers_rgba(6, 4, &mut agrandie);
        assert_eq!(
            &agrandie[(3 * 6 + 5) * 4..(3 * 6 + 6) * 4],
            &[20, 10, 7, 255]
        );
    }
}
