use estompe_plateforme::ImageNv12;

fn luma(r: i32, v: i32, b: i32) -> u8 {
    (16 + ((47 * r + 157 * v + 16 * b + 128) >> 8)).clamp(16, 235) as u8
}

fn chroma(r: i32, v: i32, b: i32) -> (u8, u8) {
    let u = 128 + ((-26 * r - 86 * v + 112 * b + 128) >> 8);
    let w = 128 + ((112 * r - 102 * v - 10 * b + 128) >> 8);
    (u.clamp(16, 240) as u8, w.clamp(16, 240) as u8)
}

pub fn vers_nv12(rgba: &[u8], largeur: u32, hauteur: u32, cible: (u32, u32)) -> ImageNv12 {
    let (sl, sh) = (largeur.max(1) as usize, hauteur.max(1) as usize);
    let (cl, ch) = (
        (cible.0.max(2) & !1) as usize,
        (cible.1.max(2) & !1) as usize,
    );
    let mut donnees = vec![0u8; cl * ch * 3 / 2];
    if rgba.len() < sl * sh * 4 {
        return ImageNv12 {
            largeur: cl as u32,
            hauteur: ch as u32,
            donnees,
        };
    }
    let colonnes: Vec<usize> = (0..cl).map(|x| (x * sl / cl).min(sl - 1) * 4).collect();
    let pixel = |ligne: usize, x: usize| {
        let i = ligne + colonnes[x];
        (
            i32::from(rgba[i]),
            i32::from(rgba[i + 1]),
            i32::from(rgba[i + 2]),
        )
    };
    let (plan_y, plan_uv) = donnees.split_at_mut(cl * ch);
    for y in (0..ch).step_by(2) {
        let haut = (y * sh / ch).min(sh - 1) * sl * 4;
        let bas = ((y + 1) * sh / ch).min(sh - 1) * sl * 4;
        for x in (0..cl).step_by(2) {
            let quatre = [
                pixel(haut, x),
                pixel(haut, x + 1),
                pixel(bas, x),
                pixel(bas, x + 1),
            ];
            for (k, (r, v, b)) in quatre.iter().enumerate() {
                plan_y[(y + k / 2) * cl + x + k % 2] = luma(*r, *v, *b);
            }
            let somme = quatre
                .iter()
                .fold((0, 0, 0), |a, p| (a.0 + p.0, a.1 + p.1, a.2 + p.2));
            let (u, w) = chroma(somme.0 / 4, somme.1 / 4, somme.2 / 4);
            let i = y / 2 * cl + x;
            plan_uv[i] = u;
            plan_uv[i + 1] = w;
        }
    }
    ImageNv12 {
        largeur: cl as u32,
        hauteur: ch as u32,
        donnees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unie(largeur: u32, hauteur: u32, couleur: [u8; 4]) -> Vec<u8> {
        couleur.repeat((largeur * hauteur) as usize)
    }

    #[test]
    fn couleurs_de_reference_bt709_limitees() {
        let blanc = vers_nv12(&unie(4, 4, [255, 255, 255, 255]), 4, 4, (4, 4));
        assert_eq!(blanc.donnees.len(), 24);
        assert!(blanc.donnees[..16].iter().all(|y| *y == 235));
        assert!(blanc.donnees[16..].iter().all(|c| *c == 128));
        let noir = vers_nv12(&unie(4, 4, [0, 0, 0, 255]), 4, 4, (4, 4));
        assert!(noir.donnees[..16].iter().all(|y| *y == 16));
        let rouge = vers_nv12(&unie(2, 2, [255, 0, 0, 255]), 2, 2, (2, 2));
        assert_eq!(rouge.donnees, [63, 63, 63, 63, 102, 240]);
    }

    #[test]
    fn reduit_a_la_taille_voulue() {
        let mut rgba = unie(8, 4, [0, 0, 0, 255]);
        for ligne in 0..4 {
            for x in 4..8 {
                rgba[(ligne * 8 + x) * 4..][..3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let image = vers_nv12(&rgba, 8, 4, (4, 2));
        assert_eq!(
            (image.largeur, image.hauteur, image.donnees.len()),
            (4, 2, 12)
        );
        assert_eq!(&image.donnees[..8], [16, 16, 235, 235, 16, 16, 235, 235]);
        let impaire = vers_nv12(&rgba, 8, 4, (5, 3));
        assert_eq!((impaire.largeur, impaire.hauteur), (4, 2));
        let courte = vers_nv12(&[0; 3], 8, 4, (4, 2));
        assert_eq!(courte.donnees.len(), 12);
    }
}
