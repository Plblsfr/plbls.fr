use estompe_plateforme::Image;

pub const COTE_PASTILLE: u32 = 32;
pub const COULEUR_PASTILLE: [u8; 3] = [0xd9, 0x34, 0x2b];
const BLANC: [u8; 3] = [0xff, 0xff, 0xff];

const GLYPHES: [(char, [u8; 5]); 11] = [
    ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
    ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
    ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
    ('3', [0b111, 0b001, 0b111, 0b001, 0b111]),
    ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
    ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
    ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
    ('7', [0b111, 0b001, 0b010, 0b010, 0b010]),
    ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
    ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
    ('+', [0b000, 0b010, 0b111, 0b010, 0b000]),
];

pub fn texte_pastille(mentions: usize) -> Option<String> {
    match mentions {
        0 => None,
        1..=9 => Some(mentions.to_string()),
        _ => Some("9+".to_string()),
    }
}

pub fn description_pastille(mentions: usize) -> String {
    match mentions {
        0 => String::new(),
        1 => "1 mention".to_string(),
        n => format!("{n} mentions"),
    }
}

fn glyphe(caractere: char) -> [u8; 5] {
    GLYPHES
        .iter()
        .find(|(c, _)| *c == caractere)
        .map(|(_, g)| *g)
        .unwrap_or([0; 5])
}

fn poser(image: &mut Image, x: u32, y: u32, couleur: [u8; 3], alpha: u8) {
    if x >= image.largeur || y >= image.hauteur || alpha == 0 {
        return;
    }
    let i = ((y * image.largeur + x) * 4) as usize;
    let a = u32::from(alpha);
    let fond_a = u32::from(image.rgba[i + 3]);
    let sortie_a = a + fond_a * (255 - a) / 255;
    if sortie_a == 0 {
        return;
    }
    for (canal, composante) in couleur.iter().enumerate() {
        let dessus = u32::from(*composante) * a;
        let dessous = u32::from(image.rgba[i + canal]) * fond_a * (255 - a) / 255;
        image.rgba[i + canal] = ((dessus + dessous) / sortie_a) as u8;
    }
    image.rgba[i + 3] = sortie_a as u8;
}

pub fn dessiner_pastille(mentions: usize, cote: u32) -> Option<Image> {
    let texte = texte_pastille(mentions)?;
    let cote = cote.max(8);
    let mut image = Image {
        largeur: cote,
        hauteur: cote,
        rgba: vec![0; (cote * cote * 4) as usize],
    };
    let rayon = cote as f32 / 2.0;
    for y in 0..cote {
        for x in 0..cote {
            let (dx, dy) = (x as f32 + 0.5 - rayon, y as f32 + 0.5 - rayon);
            let couverture = (rayon - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            poser(
                &mut image,
                x,
                y,
                COULEUR_PASTILLE,
                (couverture * 255.0) as u8,
            );
        }
    }
    let echelle = (cote * 3 / 32).max(1);
    let caracteres: Vec<char> = texte.chars().collect();
    let largeur = (caracteres.len() as u32 * 4 - 1) * echelle;
    let hauteur = 5 * echelle;
    let (x0, y0) = ((cote - largeur) / 2, (cote - hauteur) / 2);
    for (rang, caractere) in caracteres.iter().enumerate() {
        let lignes = glyphe(*caractere);
        for (ligne, motif) in lignes.iter().enumerate() {
            for colonne in 0..3u32 {
                if motif & (0b100 >> colonne) == 0 {
                    continue;
                }
                let gx = x0 + (rang as u32 * 4 + colonne) * echelle;
                let gy = y0 + ligne as u32 * echelle;
                for py in 0..echelle {
                    for px in 0..echelle {
                        poser(&mut image, gx + px, gy + py, BLANC, 255);
                    }
                }
            }
        }
    }
    Some(image)
}

pub fn image_pastille(mentions: usize) -> Option<Image> {
    dessiner_pastille(mentions, COTE_PASTILLE)
}

pub fn icone_avec_pastille(icone: &Image, mentions: usize) -> Image {
    let mut sortie = icone.clone();
    let cote = icone.largeur.min(icone.hauteur) * 9 / 16;
    let Some(pastille) = dessiner_pastille(mentions, cote) else {
        return sortie;
    };
    if sortie.rgba.len() != (sortie.largeur * sortie.hauteur * 4) as usize {
        return sortie;
    }
    let (ox, oy) = (
        icone.largeur - pastille.largeur,
        icone.hauteur - pastille.hauteur,
    );
    for y in 0..pastille.hauteur {
        for x in 0..pastille.largeur {
            let i = ((y * pastille.largeur + x) * 4) as usize;
            let p = &pastille.rgba[i..i + 4];
            poser(&mut sortie, ox + x, oy + y, [p[0], p[1], p[2]], p[3]);
        }
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.largeur + x) * 4) as usize;
        image.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn aucune_pastille_sans_mention() {
        assert_eq!(image_pastille(0), None);
        assert_eq!(texte_pastille(0), None);
        assert_eq!(description_pastille(0), "");
    }

    #[test]
    fn la_pastille_est_un_disque_rouge_avec_un_chiffre_blanc() {
        let image = image_pastille(1).unwrap();
        assert_eq!((image.largeur, image.hauteur), (32, 32));
        assert_eq!(pixel(&image, 0, 0)[3], 0, "coin transparent");
        assert_eq!(pixel(&image, 4, 16), [0xd9, 0x34, 0x2b, 0xff], "disque");
        assert_eq!(
            pixel(&image, 16, 9),
            [0xff, 0xff, 0xff, 0xff],
            "trait du « 1 »"
        );
        assert_eq!(description_pastille(1), "1 mention");
    }

    #[test]
    fn au_dela_de_neuf_la_pastille_dit_neuf_plus() {
        assert_eq!(texte_pastille(9).as_deref(), Some("9"));
        assert_eq!(texte_pastille(10).as_deref(), Some("9+"));
        assert_eq!(description_pastille(12), "12 mentions");
        assert_ne!(image_pastille(10), image_pastille(9));
        assert_eq!(image_pastille(10), image_pastille(250));
    }

    #[test]
    fn l_icone_avec_pastille_garde_l_icone_hors_du_disque() {
        let icone = Image {
            largeur: 64,
            hauteur: 64,
            rgba: [10, 20, 30, 255].repeat(64 * 64),
        };
        assert_eq!(icone_avec_pastille(&icone, 0), icone);
        let marquee = icone_avec_pastille(&icone, 3);
        assert_eq!(pixel(&marquee, 2, 2), [10, 20, 30, 255]);
        assert_eq!(pixel(&marquee, 30, 30), [10, 20, 30, 255]);
        assert_eq!(pixel(&marquee, 63 - 4, 63 - 17), [0xd9, 0x34, 0x2b, 0xff]);
    }
}
