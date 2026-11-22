use serde_json::Value;

use super::plan::duree_finale;
use super::{arrondi, borner, Montage, Texte, HABILLAGES, POLICES, TEXTES_MAX, TEXTE_MAX};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReglageTexte {
    pub contenu: Option<String>,
    pub police: Option<String>,
    pub taille: Option<f64>,
    pub couleur: Option<String>,
    pub habillage: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub debut: Option<f64>,
    pub fin: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LigneTexte {
    pub texte: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Disposition {
    pub corps: f64,
    pub encre: String,
    pub contour: Option<(String, f64)>,
    pub fond: Option<(String, f64, [f64; 4])>,
    pub lignes: Vec<LigneTexte>,
    pub boite: [f64; 4],
}

fn nettoyer(contenu: &str) -> String {
    contenu
        .chars()
        .filter(|c| *c != '\r' && (*c == '\n' || !c.is_control()))
        .take(TEXTE_MAX)
        .collect()
}

pub fn texte_neuf(m: &Montage, debut: f64) -> Texte {
    let finale = duree_finale(m);
    let d = debut.min(finale - 0.5).max(0.0);
    Texte {
        contenu: "Texte".into(),
        police: POLICES[0].into(),
        taille: 0.08,
        couleur: "#ffffff".into(),
        habillage: "contour".into(),
        x: 0.5,
        y: 0.82,
        debut: d,
        fin: (d + 0.1).max(finale.min(d + 3.0)),
    }
}

fn couleur_valide(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

pub fn borner_texte(t: &Texte, finale: f64) -> Texte {
    let debut = borner(t.debut, 0.0, (finale - 0.1).max(0.0), 0.0);
    Texte {
        contenu: nettoyer(&t.contenu),
        police: if POLICES.contains(&t.police.as_str()) {
            t.police.clone()
        } else {
            POLICES[0].into()
        },
        taille: borner(t.taille, 0.02, 0.4, 0.08),
        couleur: if couleur_valide(&t.couleur) {
            t.couleur.to_lowercase()
        } else {
            "#ffffff".into()
        },
        habillage: if HABILLAGES.iter().any(|(h, _)| *h == t.habillage) {
            t.habillage.clone()
        } else {
            "contour".into()
        },
        x: borner(t.x, 0.0, 1.0, 0.5),
        y: borner(t.y, 0.0, 1.0, 0.5),
        debut,
        fin: (debut + 0.1).max(borner(t.fin, 0.0, finale.max(debut + 0.1), debut + 3.0)),
    }
}

pub fn texte_de_valeur(v: &Value) -> Texte {
    let chaine = |cle: &str| {
        v.get(cle)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let nombre = |cle: &str| v.get(cle).and_then(Value::as_f64).unwrap_or(f64::NAN);
    Texte {
        contenu: chaine("contenu"),
        police: chaine("police"),
        taille: nombre("taille"),
        couleur: chaine("couleur"),
        habillage: chaine("habillage"),
        x: nombre("x"),
        y: nombre("y"),
        debut: nombre("debut"),
        fin: nombre("fin"),
    }
}

pub fn ajouter_texte(m: &Montage, debut: f64) -> Montage {
    if m.textes.len() >= TEXTES_MAX {
        return m.clone();
    }
    let mut apres = m.clone();
    apres.textes.push(texte_neuf(m, debut));
    apres
}

pub fn regler_texte(m: &Montage, i: usize, r: ReglageTexte) -> Montage {
    let Some(avant) = m.textes.get(i) else {
        return m.clone();
    };
    let brut = Texte {
        contenu: r.contenu.unwrap_or_else(|| avant.contenu.clone()),
        police: r.police.unwrap_or_else(|| avant.police.clone()),
        taille: r.taille.unwrap_or(avant.taille),
        couleur: r.couleur.unwrap_or_else(|| avant.couleur.clone()),
        habillage: r.habillage.unwrap_or_else(|| avant.habillage.clone()),
        x: r.x.unwrap_or(avant.x),
        y: r.y.unwrap_or(avant.y),
        debut: r.debut.unwrap_or(avant.debut),
        fin: r.fin.unwrap_or(avant.fin),
    };
    let apres = borner_texte(&brut, duree_finale(m));
    if apres.contenu.trim().is_empty() || apres == *avant {
        return m.clone();
    }
    let mut nouveau = m.clone();
    nouveau.textes[i] = apres;
    nouveau
}

pub fn retirer_texte(m: &Montage, i: usize) -> Montage {
    let mut apres = m.clone();
    if i < apres.textes.len() {
        apres.textes.remove(i);
    }
    apres
}

pub fn textes_a(m: &Montage, t: f64) -> Vec<usize> {
    m.textes
        .iter()
        .enumerate()
        .filter(|(_, x)| t >= x.debut && t < x.fin)
        .map(|(i, _)| i)
        .collect()
}

pub fn corps(taille: f64, largeur: u32, hauteur: u32) -> f64 {
    arrondi(taille * f64::from(largeur.min(hauteur))).max(1.0)
}

fn contraste(couleur: &str) -> String {
    let n = u32::from_str_radix(couleur.get(1..).unwrap_or_default(), 16).unwrap_or(0);
    let luminance =
        f64::from(n >> 16) * 0.299 + f64::from((n >> 8) & 255) * 0.587 + f64::from(n & 255) * 0.114;
    if luminance > 140.0 {
        "#000000"
    } else {
        "#ffffff"
    }
    .into()
}

pub fn disposer_texte(
    t: &Texte,
    largeur: u32,
    hauteur: u32,
    mesure: impl Fn(&str, f64) -> f64,
) -> Disposition {
    let px = corps(t.taille, largeur, hauteur);
    let (lt, ht) = (f64::from(largeur), f64::from(hauteur));
    let lignes: Vec<&str> = t.contenu.split('\n').collect();
    let interligne = px * 1.25;
    let marge = match t.habillage.as_str() {
        "aucun" => 0.0,
        "fond" => px * 0.35,
        _ => px * 0.1,
    };
    let l = lignes
        .iter()
        .map(|x| mesure(x, px))
        .fold(f64::MIN, f64::max)
        + marge * 2.0;
    let h = lignes.len() as f64 * interligne + marge * 2.0;
    let centrer = |f: f64, total: f64, taille: f64| {
        if taille >= total {
            total / 2.0
        } else {
            (f * total).max(taille / 2.0).min(total - taille / 2.0)
        }
    };
    let (cx, cy) = (centrer(t.x, lt, l), centrer(t.y, ht, h));
    let n = lignes.len() as f64;
    Disposition {
        corps: px,
        encre: t.couleur.clone(),
        contour: (t.habillage == "contour").then(|| (contraste(&t.couleur), px * 0.16)),
        fond: (t.habillage == "fond").then(|| {
            (
                contraste(&t.couleur),
                0.72,
                [cx - l / 2.0, cy - h / 2.0, l, h],
            )
        }),
        lignes: lignes
            .iter()
            .enumerate()
            .map(|(i, ligne)| LigneTexte {
                texte: ligne.to_string(),
                x: cx,
                y: cy + (i as f64 - (n - 1.0) / 2.0) * interligne,
            })
            .collect(),
        boite: [(cx - l / 2.0) / lt, (cy - h / 2.0) / ht, l / lt, h / ht],
    }
}

#[cfg(test)]
mod tests {
    use super::super::essais::*;
    use super::super::*;
    use super::*;

    fn t_base() -> Texte {
        Texte {
            contenu: "Bien joué 🎉".into(),
            police: "Bricolage Grotesque".into(),
            taille: 0.1,
            couleur: "#ffffff".into(),
            habillage: "contour".into(),
            x: 0.5,
            y: 0.8,
            debut: 2.0,
            fin: 5.0,
        }
    }

    fn mesure(texte: &str, px: f64) -> f64 {
        texte.chars().count() as f64 * px * 0.5
    }

    #[test]
    fn pose_des_blocs_sur_un_intervalle_plusieurs_a_la_fois() {
        let m = ajouter_texte(&ajouter_texte(&initial(&s(), None), 2.0), 9.9);
        let vus: Vec<(f64, f64, &str, &str, &str)> = m
            .textes
            .iter()
            .map(|t| {
                (
                    t.debut,
                    t.fin,
                    t.contenu.as_str(),
                    t.police.as_str(),
                    t.habillage.as_str(),
                )
            })
            .collect();
        assert_eq!(
            vus,
            [
                (2.0, 5.0, "Texte", "Bricolage Grotesque", "contour"),
                (9.5, 10.0, "Texte", "Bricolage Grotesque", "contour")
            ]
        );
        let r = regler_texte(
            &m,
            0,
            ReglageTexte {
                debut: Some(2.0),
                fin: Some(5.0),
                contenu: Some("Bien joué 🎉\r\n\u{7}ligne 2".into()),
                ..ReglageTexte::default()
            },
        );
        let r = regler_texte(
            &r,
            1,
            ReglageTexte {
                debut: Some(4.0),
                fin: Some(99.0),
                x: Some(3.0),
                y: Some(-1.0),
                taille: Some(9.0),
                couleur: Some("rouge".into()),
                police: Some("Comic Sans MS".into()),
                ..ReglageTexte::default()
            },
        );
        assert_eq!(r.textes[0].contenu, "Bien joué 🎉\nligne 2");
        assert_eq!(
            r.textes[1],
            Texte {
                debut: 4.0,
                fin: 10.0,
                x: 1.0,
                y: 0.0,
                taille: 0.4,
                ..m.textes[1].clone()
            }
        );
        assert_eq!(
            [
                textes_a(&r, 1.99),
                textes_a(&r, 2.0),
                textes_a(&r, 4.5),
                textes_a(&r, 5.0),
                textes_a(&r, 10.0)
            ],
            [vec![], vec![0], vec![0, 1], vec![1], vec![]]
        );
        let contenu = |c: &str| ReglageTexte {
            contenu: Some(c.into()),
            ..ReglageTexte::default()
        };
        assert_eq!(regler_texte(&r, 0, contenu("  ")), r);
        assert_eq!(
            regler_texte(
                &r,
                0,
                ReglageTexte {
                    x: Some(0.5),
                    ..ReglageTexte::default()
                }
            ),
            r
        );
        assert_eq!(
            regler_texte(
                &r,
                7,
                ReglageTexte {
                    x: Some(0.1),
                    ..ReglageTexte::default()
                }
            ),
            r
        );
        assert_eq!(retirer_texte(&r, 7), r);
        assert_eq!(
            regler_texte(&r, 0, contenu(&"é".repeat(500))).textes[0]
                .contenu
                .chars()
                .count(),
            200
        );
        assert!(proche(
            regler_texte(
                &r,
                0,
                ReglageTexte {
                    fin: Some(1.0),
                    ..ReglageTexte::default()
                }
            )
            .textes[0]
                .fin,
            2.1
        ));
        assert_eq!(retirer_texte(&r, 0).textes, [r.textes[1].clone()]);
        assert_eq!(
            (0..60)
                .fold(initial(&s(), None), |x, _| ajouter_texte(&x, 0.0))
                .textes
                .len(),
            50
        );
    }

    #[test]
    fn place_le_texte_dans_le_cadre_final_quelle_que_soit_la_resolution() {
        let t = t_base();
        let b = disposer_texte(&t, 1920, 1080, mesure);
        let p = disposer_texte(&t, 640, 360, mesure);
        for k in 0..4 {
            assert!(proche(p.boite[k], b.boite[k]));
        }
        assert_eq!(
            [
                corps(0.1, 1920, 1080),
                corps(0.1, 1080, 1920),
                corps(0.1, 594, 1056),
                corps(0.0, 10, 10)
            ],
            [108.0, 108.0, 59.0, 1.0]
        );
        assert_eq!((b.corps, p.corps), (108.0, 36.0));
        assert_eq!(b.contour, Some(("#000000".into(), 108.0 * 0.16)));
        assert_eq!((b.encre.as_str(), b.fond.clone()), ("#ffffff", None));
        assert_eq!(
            b.lignes,
            [LigneTexte {
                texte: "Bien joué 🎉".into(),
                x: 960.0,
                y: 864.0
            }]
        );
        assert_eq!((p.lignes[0].x, p.lignes[0].y), (320.0, 288.0));
        let centre = [b.boite[0] + b.boite[2] / 2.0, b.boite[1] + b.boite[3] / 2.0]
            .map(|x| (x * 1e9).round() / 1e9);
        assert_eq!(centre, [0.5, 0.8]);
        assert!(proche(b.boite[2], (11.0 * 54.0 + 21.6) / 1920.0));
        let haut_bas = Texte {
            x: 0.99,
            y: 1.0,
            habillage: "fond".into(),
            couleur: "#101010".into(),
            contenu: "Haut\nBas".into(),
            ..t.clone()
        };
        let v = disposer_texte(&haut_bas, 594, 1056, mesure);
        assert!(
            proche(v.boite[0] + v.boite[2], 1.0)
                && proche(v.boite[1] + v.boite[3], 1.0)
                && v.boite[0] >= 0.0
        );
        assert!(v.contour.is_none() && v.lignes.len() == 2);
        let (fond, opacite, _) = v.fond.clone().unwrap();
        assert_eq!((fond.as_str(), opacite), ("#ffffff", 0.72));
        assert!(proche(v.lignes[1].y - v.lignes[0].y, 59.0 * 1.25));
        let large = disposer_texte(
            &Texte {
                contenu: "x".repeat(100),
                habillage: "aucun".into(),
                x: 0.0,
                ..t
            },
            594,
            1056,
            mesure,
        );
        assert_eq!(
            (large.lignes[0].x, large.lignes[0].y),
            (297.0, 0.8 * 1056.0)
        );
        assert!(large.boite[2] > 1.0);
    }
}
