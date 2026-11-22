use super::edition::clips_de;
use super::{borner, Cadre, Montage, Sources, CADENCES, HAUTEURS};

pub type Boite = (u32, u32, u32, u32);

const CADRE_MIN: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub toile: (u32, u32),
    pub cadre: Boite,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
}

fn pair(x: f64) -> u32 {
    ((x / 2.0).round() as u32 * 2).max(2)
}

pub fn toile(largeur: u32, hauteur: u32) -> (u32, u32) {
    ((largeur & !1).max(2), (hauteur & !1).max(2))
}

pub fn inscrire(largeur: u32, hauteur: u32, toile: (u32, u32)) -> Boite {
    let echelle = (f64::from(toile.0) / f64::from(largeur.max(1)))
        .min(f64::from(toile.1) / f64::from(hauteur.max(1)));
    let (l, h) = (
        pair(f64::from(largeur) * echelle).min(toile.0),
        pair(f64::from(hauteur) * echelle).min(toile.1),
    );
    (l, h, (toile.0 - l) / 4 * 2, (toile.1 - h) / 4 * 2)
}

pub fn cadre_pixels(c: &Cadre, toile: (u32, u32)) -> Boite {
    let (l, h) = (
        pair(c.l * f64::from(toile.0)).min(toile.0),
        pair(c.h * f64::from(toile.1)).min(toile.1),
    );
    let coin = |f: f64, total: u32, taille: u32| {
        ((f * f64::from(total) / 2.0).round().max(0.0) as u32 * 2).min(total - taille)
    };
    (l, h, coin(c.x, toile.0, l), coin(c.y, toile.1, h))
}

pub fn proportions(r: &str) -> Option<(f64, f64)> {
    let (a, b) = r.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

pub fn dimensions_sortie(cadre: (u32, u32), rapport: &str, hauteur: Option<u32>) -> (u32, u32) {
    let (l, h) = (f64::from(cadre.0), f64::from(cadre.1));
    let voulue = hauteur.map_or(h, |x| f64::from(x).min(h));
    match proportions(rapport) {
        Some((a, b)) => {
            let k = (((l / a).min(h / b).min(voulue / b) / 2.0).floor() * 2.0).max(2.0);
            ((a * k) as u32, (b * k) as u32)
        }
        None => {
            let haut = pair(voulue).min(cadre.1);
            (
                if haut == cadre.1 {
                    cadre.0
                } else {
                    pair(l * f64::from(haut) / h).min(cadre.0)
                },
                haut,
            )
        }
    }
}

pub fn borner_cadre(c: &Cadre, t: (u32, u32)) -> Cadre {
    let r = proportions(&c.rapport).map(|(a, b)| a / b / (f64::from(t.0) / f64::from(t.1)));
    let (mut l, mut h) = (
        borner(c.l, CADRE_MIN, 1.0, 1.0),
        borner(c.h, CADRE_MIN, 1.0, 1.0),
    );
    if let Some(r) = r {
        l = l.min(1.0).min(r);
        h = l / r;
    }
    Cadre {
        x: borner(c.x, 0.0, 1.0 - l, 0.0),
        y: borner(c.y, 0.0, 1.0 - h, 0.0),
        l,
        h,
        rapport: c.rapport.clone(),
    }
}

pub fn cadre_au(rapport: &str, c: &Cadre, t: (u32, u32)) -> Cadre {
    let plein = borner_cadre(
        &Cadre {
            rapport: rapport.into(),
            ..Cadre::default()
        },
        t,
    );
    if proportions(rapport).is_none() {
        return Cadre {
            rapport: rapport.into(),
            ..c.clone()
        };
    }
    borner_cadre(
        &Cadre {
            x: c.x + c.l / 2.0 - plein.l / 2.0,
            y: c.y + c.h / 2.0 - plein.h / 2.0,
            ..plein
        },
        t,
    )
}

pub fn recadrer(m: &Montage, c: &Cadre, t: (u32, u32)) -> Montage {
    Montage {
        cadre: borner_cadre(c, t),
        ..m.clone()
    }
}

pub fn format_de(m: &Montage, sources: &Sources) -> Format {
    let premier = &sources[&m.segments[0].source];
    let t = toile(premier.largeur, premier.hauteur);
    let cadre = cadre_pixels(&m.cadre, t);
    let (largeur, hauteur) =
        dimensions_sortie((cadre.0, cadre.1), &m.cadre.rapport, m.sortie.hauteur);
    let max = clips_de(m)
        .iter()
        .map(|c| sources[c].ips)
        .max()
        .unwrap_or(premier.ips);
    Format {
        toile: t,
        cadre,
        largeur,
        hauteur,
        ips: max.min(m.sortie.ips.unwrap_or(premier.ips)),
    }
}

pub fn hauteurs_possibles(f: &Format) -> Vec<u32> {
    HAUTEURS.into_iter().filter(|h| *h < f.cadre.1).collect()
}

pub fn cadences_possibles(m: &Montage, sources: &Sources) -> Vec<u32> {
    let max = clips_de(m)
        .iter()
        .map(|c| sources[c].ips)
        .max()
        .unwrap_or(0);
    let mut liste = vec![sources[&m.segments[0].source].ips];
    liste.extend(CADENCES.into_iter().filter(|c| *c <= max));
    liste.sort_unstable_by(|a, b| b.cmp(a));
    liste.dedup();
    liste
}

#[cfg(test)]
mod tests {
    use super::super::essais::*;
    use super::super::*;
    use super::*;

    fn cadre(x: f64, y: f64, l: f64, h: f64, rapport: &str) -> Cadre {
        Cadre {
            x,
            y,
            l,
            h,
            rapport: rapport.into(),
        }
    }

    #[test]
    fn inscrit_un_clip_dans_la_toile_sans_deformation() {
        assert_eq!(
            (toile(1920, 1080), toile(1921, 1081), toile(1, 1)),
            ((1920, 1080), (1920, 1080), (2, 2))
        );
        assert_eq!(inscrire(1920, 1080, (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(inscrire(2560, 1440, (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(inscrire(1280, 720, (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(inscrire(1000, 1000, (1920, 1080)), (1080, 1080, 420, 0));
        assert_eq!(inscrire(640, 360, (320, 240)), (320, 180, 0, 30));
        assert_eq!(inscrire(1080, 1920, (1920, 1080)), (608, 1080, 656, 0));
        assert_eq!(inscrire(2560, 1080, (1920, 1080)), (1920, 810, 0, 134));
    }

    #[test]
    fn convertit_le_cadre_en_pixels_pairs_dans_la_toile() {
        assert_eq!(
            cadre_pixels(&Cadre::default(), (1920, 1080)),
            (1920, 1080, 0, 0)
        );
        assert_eq!(
            cadre_pixels(
                &cadre(0.341_796_875, 0.0, 0.316_406_25, 1.0, "libre"),
                (1920, 1080)
            ),
            (608, 1080, 656, 0)
        );
        assert_eq!(
            cadre_pixels(&cadre(0.25, 0.25, 0.5, 0.5, "libre"), (1920, 1080)),
            (960, 540, 480, 270)
        );
        assert_eq!(
            cadre_pixels(&cadre(0.9, 0.9, 0.5, 0.5, "libre"), (1920, 1080)),
            (960, 540, 960, 540)
        );
        assert_eq!(
            cadre_pixels(&cadre(0.1, 0.2, 0.333, 0.333, "libre"), (1280, 720)),
            (426, 240, 128, 144)
        );
        assert_eq!(
            cadre_pixels(&cadre(0.0, 0.0, 0.0001, 0.0001, "libre"), (1280, 720)),
            (2, 2, 0, 0)
        );
    }

    #[test]
    fn deduit_la_resolution_de_sortie_du_cadre_sans_agrandissement() {
        assert_eq!(dimensions_sortie((1920, 1080), "libre", None), (1920, 1080));
        assert_eq!(
            dimensions_sortie((1920, 1080), "libre", Some(720)),
            (1280, 720)
        );
        assert_eq!(
            dimensions_sortie((1920, 1080), "libre", Some(2160)),
            (1920, 1080)
        );
        assert_eq!(
            dimensions_sortie((426, 240), "libre", Some(121)),
            (216, 122)
        );
        assert_eq!(dimensions_sortie((1920, 1080), "16:9", None), (1920, 1080));
        assert_eq!(
            dimensions_sortie((1920, 1080), "16:9", Some(720)),
            (1280, 720)
        );
        assert_eq!(dimensions_sortie((608, 1080), "9:16", None), (594, 1056));
        assert_eq!(
            dimensions_sortie((608, 1080), "9:16", Some(720)),
            (396, 704)
        );
        assert_eq!(dimensions_sortie((1080, 1080), "1:1", None), (1080, 1080));
        assert_eq!(dimensions_sortie((864, 1080), "4:5", None), (864, 1080));
        assert_eq!(dimensions_sortie((136, 240), "9:16", None), (126, 224));
    }

    #[test]
    fn borne_le_cadre_et_tient_les_rapports_proposes() {
        let t = (1920, 1080);
        let plein = cadre(0.0, 0.0, 1.0, 1.0, "libre");
        assert_eq!(
            cadre_au("9:16", &plein, t),
            cadre(0.341_796_875, 0.0, 0.316_406_25, 1.0, "9:16")
        );
        assert_eq!(
            cadre_au("16:9", &plein, t),
            cadre(0.0, 0.0, 1.0, 1.0, "16:9")
        );
        assert_eq!(
            cadre_au("1:1", &plein, t),
            cadre(0.21875, 0.0, 0.5625, 1.0, "1:1")
        );
        let portrait = cadre_au("4:5", &cadre(0.5, 0.0, 0.5, 1.0, "libre"), t);
        let r = |x: f64| (x * 1e9).round() / 1e9;
        assert_eq!(
            (
                r(portrait.l),
                r(portrait.x),
                r(portrait.h),
                r(portrait.y),
                portrait.rapport.as_str()
            ),
            (0.45, 0.525, 1.0, 0.0, "4:5")
        );
        assert_eq!(
            cadre_au("libre", &cadre(0.1, 0.1, 0.5, 0.5, "1:1"), t),
            cadre(0.1, 0.1, 0.5, 0.5, "libre")
        );
        assert_eq!(
            cadre_au("16:9", &plein, (1080, 1920)),
            cadre(0.0, 0.341_796_875, 1.0, 0.316_406_25, "16:9")
        );
        assert_eq!(
            borner_cadre(&cadre(0.9, -1.0, 0.5, 9.0, "libre"), t),
            cadre(0.5, 0.0, 0.5, 1.0, "libre")
        );
        assert_eq!(
            borner_cadre(&cadre(0.0, 0.0, 0.0, f64::NAN, "libre"), t),
            cadre(0.0, 0.0, 0.05, 1.0, "libre")
        );
        let v = borner_cadre(&cadre(0.9, 0.5, 0.2, 0.9, "9:16"), t);
        assert_eq!(
            (v.l, v.h, v.x, v.y),
            (0.2, 0.2 / 0.316_406_25, 0.8, 1.0 - 0.2 / 0.316_406_25)
        );
        assert!(proche(v.l * 1920.0 / (v.h * 1080.0), 9.0 / 16.0));
        let m = initial(&s(), None);
        assert_eq!(recadrer(&m, &plein, t), m);
        assert_eq!(
            recadrer(&m, &cadre(0.25, 0.25, 0.5, 0.5, "libre"), t).cadre,
            cadre(0.25, 0.25, 0.5, 0.5, "libre")
        );
    }

    #[test]
    fn donne_le_format_de_sortie_du_montage_et_les_reglages_possibles() {
        let m = tous();
        let src = src();
        let f = |toile, cadre, largeur, hauteur, ips| Format {
            toile,
            cadre,
            largeur,
            hauteur,
            ips,
        };
        assert_eq!(
            format_de(&m, &src),
            f((1920, 1080), (1920, 1080, 0, 0), 1920, 1080, 60)
        );
        let t = (1920, 1080);
        let vertical = regler_sortie(
            &recadrer(&m, &cadre_au("9:16", &m.cadre, t), t),
            ReglageSortie {
                hauteur: Some(Some(720)),
                ips: Some(Some(144)),
            },
        );
        assert_eq!(
            format_de(&vertical, &src),
            f((1920, 1080), (608, 1080, 656, 0), 396, 704, 144)
        );
        assert_eq!(
            format_de(&retirer_clip(&vertical, "Hevc.mp4"), &src).ips,
            60
        );
        assert_eq!(
            format_de(&deplacer_clip(&m, "Hevc.mp4", 0), &src),
            f((2560, 1440), (2560, 1440, 0, 0), 2560, 1440, 144)
        );
        assert_eq!(hauteurs_possibles(&format_de(&m, &src)), [720, 480]);
        assert_eq!(
            hauteurs_possibles(&format_de(&deplacer_clip(&m, "Hevc.mp4", 0), &src)),
            [1080, 720, 480]
        );
        assert_eq!(cadences_possibles(&m, &src), [144, 120, 60, 30]);
        assert_eq!(cadences_possibles(&initial(&c(), None), &src), [30]);
        assert_eq!(
            regler_sortie(
                &m,
                ReglageSortie {
                    hauteur: Some(None),
                    ips: None
                }
            ),
            m
        );
        assert_eq!(
            regler_sortie(
                &m,
                ReglageSortie {
                    hauteur: None,
                    ips: Some(Some(30))
                }
            )
            .sortie,
            Sortie {
                hauteur: None,
                ips: Some(30)
            }
        );
    }
}
