pub const MARGE_PAR_DEFAUT: f64 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fenetre {
    pub debut: usize,
    pub fin: usize,
    pub haut: f64,
    pub bas: f64,
}

pub fn fenetre_visible<K>(
    cles: &[K],
    hauteur: impl Fn(&K) -> f64,
    defilement: f64,
    zone_visible: f64,
    marge: f64,
) -> Fenetre {
    let bas_de_zone = defilement - marge;
    let haut_de_zone = defilement + zone_visible + marge;
    let mut y = 0.0;
    let mut debut: Option<usize> = None;
    let mut fin: Option<usize> = None;
    let mut haut = 0.0;
    let mut y_de_fin = 0.0;
    for (i, cle) in cles.iter().enumerate() {
        let h = hauteur(cle);
        if debut.is_none() && y + h > bas_de_zone {
            debut = Some(i);
            haut = y;
        }
        if fin.is_none() && y > haut_de_zone {
            fin = Some(i);
            y_de_fin = y;
        }
        y += h;
    }
    let debut = debut.unwrap_or_else(|| {
        haut = y - cles.last().map_or(0.0, &hauteur);
        cles.len().saturating_sub(1)
    });
    let fin = fin.unwrap_or_else(|| {
        y_de_fin = y;
        cles.len()
    });
    Fenetre {
        debut,
        fin,
        haut,
        bas: y - y_de_fin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cles() -> Vec<String> {
        (0..1000).map(|i| i.to_string()).collect()
    }

    fn h(_: &String) -> f64 {
        50.0
    }

    #[test]
    fn ne_garde_que_les_lignes_visibles_et_leur_marge() {
        let w = fenetre_visible(&cles(), h, 10_000.0, 600.0, 500.0);
        assert_eq!(w.debut, 190);
        assert_eq!(w.fin, 223);
        assert_eq!(w.haut, 190.0 * 50.0);
        assert_eq!(
            w.haut + (w.fin - w.debut) as f64 * 50.0 + w.bas,
            1000.0 * 50.0
        );
    }

    #[test]
    fn debut_de_liste() {
        let w = fenetre_visible(&cles(), h, 0.0, 600.0, 500.0);
        assert_eq!(w.debut, 0);
        assert_eq!(w.haut, 0.0);
        assert!(w.fin < 30);
    }

    #[test]
    fn fin_de_liste() {
        let w = fenetre_visible(&cles(), h, 1000.0 * 50.0 - 600.0, 600.0, 500.0);
        assert_eq!(w.fin, 1000);
        assert_eq!(w.bas, 0.0);
    }

    #[test]
    fn utilise_les_hauteurs_mesurees() {
        let haute = |k: &String| if k == "0" { 5000.0 } else { 50.0 };
        let w = fenetre_visible(&cles(), haute, 5200.0, 600.0, 100.0);
        assert_eq!(w.debut, 3);
        assert_eq!(w.haut, 5000.0 + 2.0 * 50.0);
    }

    #[test]
    fn liste_vide_ou_defilement_au_dela_de_la_fin() {
        let vide: Vec<String> = Vec::new();
        assert_eq!(
            fenetre_visible(&vide, h, 0.0, 600.0, MARGE_PAR_DEFAUT),
            Fenetre {
                debut: 0,
                fin: 0,
                haut: 0.0,
                bas: 0.0
            }
        );
        let trois = cles()[..3].to_vec();
        let w = fenetre_visible(&trois, h, 99_999.0, 600.0, MARGE_PAR_DEFAUT);
        assert_eq!(w.fin, 3);
        assert_eq!(w.debut, 2);
        assert_eq!(w.haut, 100.0);
        assert_eq!(w.bas, 0.0);
    }

    #[test]
    fn la_marge_par_defaut_est_celle_du_front() {
        assert_eq!(MARGE_PAR_DEFAUT, 1000.0);
        let w = fenetre_visible(&cles(), h, 10_000.0, 600.0, MARGE_PAR_DEFAUT);
        assert_eq!(w.debut, 180);
    }
}
