use crate::traits::Image;

pub fn bgra_vers_rgba(source: &[u8], pas: usize, largeur: usize, hauteur: usize) -> Vec<u8> {
    let mut sortie = vec![0u8; largeur * hauteur * 4];
    if pas < largeur * 4 || largeur == 0 {
        return sortie;
    }
    for (ligne, cible) in source.chunks(pas).zip(sortie.chunks_exact_mut(largeur * 4)) {
        if ligne.len() < largeur * 4 {
            break;
        }
        for (s, c) in ligne[..largeur * 4]
            .chunks_exact(4)
            .zip(cible.chunks_exact_mut(4))
        {
            c[0] = s[2];
            c[1] = s[1];
            c[2] = s[0];
            c[3] = 255;
        }
    }
    sortie
}

fn complete(image: &Image) -> bool {
    image.largeur > 0
        && image.hauteur > 0
        && image.rgba.len() >= image.largeur as usize * image.hauteur as usize * 4
}

pub fn reduire(image: &Image, hauteur_max: u32) -> Image {
    if hauteur_max == 0 || image.hauteur <= hauteur_max || !complete(image) {
        return image.clone();
    }
    let (l, h) = (image.largeur as usize, image.hauteur as usize);
    let hauteur = hauteur_max as usize;
    let largeur = (l * hauteur / h).max(1);
    let mut rgba = vec![0u8; largeur * hauteur * 4];
    for y in 0..hauteur {
        let y0 = y * h / hauteur;
        let y1 = ((y + 1) * h / hauteur).max(y0 + 1);
        for x in 0..largeur {
            let x0 = x * l / largeur;
            let x1 = ((x + 1) * l / largeur).max(x0 + 1);
            let mut somme = [0u64; 4];
            for ligne in y0..y1 {
                let debut = (ligne * l + x0) * 4;
                for pixel in image.rgba[debut..debut + (x1 - x0) * 4].chunks_exact(4) {
                    for (s, p) in somme.iter_mut().zip(pixel) {
                        *s += u64::from(*p);
                    }
                }
            }
            let nombre = ((y1 - y0) * (x1 - x0)) as u64;
            let cible = (y * largeur + x) * 4;
            for (c, s) in rgba[cible..cible + 4].iter_mut().zip(somme) {
                *c = ((s + nombre / 2) / nombre) as u8;
            }
        }
    }
    Image {
        largeur: largeur as u32,
        hauteur: hauteur as u32,
        rgba,
    }
}

fn pixel_mut(image: &mut Image, x: i32, y: i32) -> Option<&mut [u8]> {
    if x < 0 || y < 0 || x as u32 >= image.largeur || y as u32 >= image.hauteur {
        return None;
    }
    let i = (y as usize * image.largeur as usize + x as usize) * 4;
    image.rgba.get_mut(i..i + 4)
}

pub fn melanger_curseur(
    image: &mut Image,
    x: i32,
    y: i32,
    taille: u32,
    sur_noir: &[u8],
    sur_blanc: &[u8],
) {
    let attendu = taille as usize * taille as usize * 4;
    if sur_noir.len() < attendu || sur_blanc.len() < attendu {
        return;
    }
    for cy in 0..taille {
        for cx in 0..taille {
            let i = ((cy * taille + cx) * 4) as usize;
            let (noir, blanc) = (&sur_noir[i..i + 3], &sur_blanc[i..i + 3]);
            let perdu: i32 = (0..3)
                .map(|c| i32::from(blanc[c]) - i32::from(noir[c]))
                .sum::<i32>()
                / 3;
            let alpha = (255 - perdu).clamp(0, 255) as u32;
            if alpha == 0 {
                continue;
            }
            let Some(cible) = pixel_mut(image, x + cx as i32, y + cy as i32) else {
                continue;
            };
            for (c, source) in [noir[2], noir[1], noir[0]].into_iter().enumerate() {
                cible[c] = ((u32::from(cible[c]) * (255 - alpha) / 255) + u32::from(source))
                    .min(255) as u8;
            }
        }
    }
}

pub fn incruster_curseur(
    image: &mut Image,
    x: i32,
    y: i32,
    largeur: u32,
    hauteur: u32,
    argb_premultiplie: &[u32],
) {
    if argb_premultiplie.len() < largeur as usize * hauteur as usize {
        return;
    }
    for cy in 0..hauteur {
        for cx in 0..largeur {
            let p = argb_premultiplie[(cy * largeur + cx) as usize];
            let alpha = p >> 24;
            if alpha == 0 {
                continue;
            }
            let Some(cible) = pixel_mut(image, x + cx as i32, y + cy as i32) else {
                continue;
            };
            for (c, decalage) in [16u32, 8, 0].into_iter().enumerate() {
                let source = (p >> decalage) & 0xff;
                cible[c] = (source + u32::from(cible[c]) * (255 - alpha) / 255).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fond() -> Image {
        Image {
            largeur: 1,
            hauteur: 1,
            rgba: vec![100, 100, 100, 255],
        }
    }

    #[test]
    fn bgra_devient_rgba_sans_le_remplissage_de_ligne() {
        let source = [1, 2, 3, 0, 4, 5, 6, 0, 99, 99, 7, 8, 9, 0, 10, 11, 12, 0];
        assert_eq!(
            bgra_vers_rgba(&source, 10, 2, 2),
            vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
        );
    }

    #[test]
    fn bgra_refuse_un_pas_trop_court() {
        assert_eq!(bgra_vers_rgba(&[1, 2, 3, 4], 2, 1, 1), vec![0, 0, 0, 0]);
    }

    #[test]
    fn la_reduction_garde_les_proportions_et_moyenne_les_pixels() {
        let mut rgba = Vec::new();
        for y in 0..4u8 {
            for x in 0..8u8 {
                let v = if (x / 2 + y / 2) % 2 == 0 { 0 } else { 200 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let image = Image {
            largeur: 8,
            hauteur: 4,
            rgba,
        };
        let petite = reduire(&image, 2);
        assert_eq!((petite.largeur, petite.hauteur), (4, 2));
        assert_eq!(&petite.rgba[..8], &[0, 0, 0, 255, 200, 200, 200, 255]);
        let moyenne = reduire(&image, 1);
        assert_eq!((moyenne.largeur, moyenne.hauteur), (2, 1));
        assert_eq!(&moyenne.rgba[..4], &[100, 100, 100, 255]);
    }

    #[test]
    fn la_reduction_laisse_une_petite_image_ou_une_image_incomplete() {
        assert_eq!(reduire(&fond(), 10), fond());
        assert_eq!(reduire(&fond(), 0), fond());
        let incomplete = Image {
            largeur: 10,
            hauteur: 10,
            rgba: vec![0; 4],
        };
        assert_eq!(reduire(&incomplete, 2), incomplete);
    }

    #[test]
    fn la_reduction_d_un_ecran_4k_donne_une_miniature() {
        let image = Image {
            largeur: 3840,
            hauteur: 2160,
            rgba: vec![50; 3840 * 2160 * 4],
        };
        let miniature = reduire(&image, 180);
        assert_eq!((miniature.largeur, miniature.hauteur), (320, 180));
        assert!(miniature.rgba.iter().all(|c| *c == 50));
    }

    #[test]
    fn curseur_opaque_remplace_le_fond() {
        let mut image = fond();
        melanger_curseur(&mut image, 0, 0, 1, &[0, 0, 255, 0], &[0, 0, 255, 0]);
        assert_eq!(image.rgba, [255, 0, 0, 255]);
    }

    #[test]
    fn curseur_transparent_laisse_le_fond() {
        let mut image = fond();
        melanger_curseur(&mut image, 0, 0, 1, &[0, 0, 0, 0], &[255, 255, 255, 0]);
        assert_eq!(image.rgba, [100, 100, 100, 255]);
    }

    #[test]
    fn curseur_semi_transparent_se_melange() {
        let mut image = fond();
        melanger_curseur(
            &mut image,
            0,
            0,
            1,
            &[128, 128, 128, 0],
            &[255, 255, 255, 0],
        );
        assert_eq!(image.rgba, [177, 177, 177, 255]);
    }

    #[test]
    fn curseur_hors_image_est_ignore() {
        let mut image = fond();
        melanger_curseur(&mut image, -1, 0, 1, &[0, 0, 255, 0], &[0, 0, 255, 0]);
        melanger_curseur(&mut image, 1, 0, 1, &[0, 0, 255, 0], &[0, 0, 255, 0]);
        melanger_curseur(&mut image, 0, 0, 2, &[0; 4], &[0; 4]);
        assert_eq!(image.rgba, [100, 100, 100, 255]);
    }

    #[test]
    fn curseur_argb_premultiplie_se_superpose() {
        let mut image = Image {
            largeur: 2,
            hauteur: 1,
            rgba: vec![100, 100, 100, 255, 100, 100, 100, 255],
        };
        incruster_curseur(&mut image, 0, 0, 2, 1, &[0xffff_0000, 0x8080_8080]);
        assert_eq!(image.rgba, [255, 0, 0, 255, 177, 177, 177, 255]);
        incruster_curseur(&mut image, 1, 0, 2, 1, &[0x0000_0000, 0xff00_ff00]);
        assert_eq!(image.rgba, [255, 0, 0, 255, 177, 177, 177, 255]);
        incruster_curseur(&mut image, 0, 0, 2, 2, &[0xff00_ff00]);
        assert_eq!(&image.rgba[..4], &[255, 0, 0, 255]);
    }
}
