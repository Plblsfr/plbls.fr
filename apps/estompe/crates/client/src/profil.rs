use crate::store::Etat;

pub const ZOOM_MAX: f32 = 4.0;
pub const PAS_ZOOM: f32 = 0.05;
pub const COTE_ENVOYE: u32 = 256;
pub const COTE_APERCU: f32 = 240.0;
pub const LIMITE_PNG: usize = 200 * 1024;
pub const PAS_CLAVIER: f32 = 12.0;
pub const NUANCES_BANNIERE: [&str; 10] = [
    "#d0688a", "#b0413e", "#f2a65a", "#c9a227", "#3d8c5a", "#2a8c8c", "#2f5d8a", "#7a5ea8",
    "#6b6560", "#1f2a44",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recadrage {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for Recadrage {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Carre {
    pub x: f32,
    pub y: f32,
    pub cote: f32,
}

pub fn carre(largeur: u32, hauteur: u32, r: Recadrage) -> Carre {
    let (l, h) = (largeur as f32, hauteur as f32);
    let cote = l.min(h) / r.zoom.max(1.0);
    Carre {
        x: (l - cote) / 2.0 + r.x * (l - cote) / 2.0,
        y: (h - cote) / 2.0 + r.y * (h - cote) / 2.0,
        cote,
    }
}

pub fn zoomer(r: Recadrage, zoom: f32) -> Recadrage {
    Recadrage {
        zoom: if zoom.is_finite() {
            zoom.clamp(1.0, ZOOM_MAX)
        } else {
            1.0
        },
        ..r
    }
}

pub fn deplacer(r: Recadrage, largeur: u32, hauteur: u32, dx: f32, dy: f32) -> Recadrage {
    let c = carre(largeur, hauteur, r);
    let k = 2.0 * c.cote / COTE_APERCU;
    let pas = |d: f32, total: u32| {
        let d = if d.is_finite() { d } else { 0.0 };
        d * k / (total as f32 - c.cote).max(1.0)
    };
    Recadrage {
        x: (r.x - pas(dx, largeur)).clamp(-1.0, 1.0),
        y: (r.y - pas(dy, hauteur)).clamp(-1.0, 1.0),
        ..r
    }
}

pub fn zoom_lisible(zoom: f32) -> String {
    format!("{:.1}×", zoom).replace('.', ",")
}

pub fn banniere_valide(couleur: &str) -> bool {
    couleur.is_empty()
        || (couleur.len() == 7
            && couleur.starts_with('#')
            && couleur[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn ma_banniere(etat: &Etat) -> String {
    let moi = etat.moi();
    etat.ids_serveurs_ordonnes()
        .iter()
        .find_map(|s| etat.membre(s, moi))
        .and_then(|m| m.banner_color.clone())
        .unwrap_or_default()
        .to_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoServeur {
    pub serveur: String,
    pub nom: String,
    pub version: i64,
}

pub fn photos_par_serveur(etat: &Etat) -> Vec<PhotoServeur> {
    let moi = etat.moi();
    etat.serveurs_ordonnes()
        .into_iter()
        .filter_map(|s| {
            let serveur = s.id.clone()?;
            let version = etat.version_avatar_du_serveur(moi, Some(&serveur));
            Some(PhotoServeur {
                nom: s.name.clone().unwrap_or_default(),
                serveur,
                version,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roles::{Detail, Membre};
    use crate::store::Etat;
    use crate::types::{GetMeReponse, Server};

    fn proche(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn le_carre_suit_le_zoom_et_le_decalage_comme_le_front() {
        assert_eq!(
            carre(400, 200, Recadrage::default()),
            Carre {
                x: 100.0,
                y: 0.0,
                cote: 200.0
            }
        );
        let c = carre(
            400,
            200,
            Recadrage {
                zoom: 2.0,
                x: 1.0,
                y: -1.0,
            },
        );
        assert_eq!((c.x, c.y, c.cote), (300.0, 0.0, 100.0));
        let c = carre(
            400,
            200,
            Recadrage {
                zoom: 0.2,
                x: -1.0,
                y: 0.0,
            },
        );
        assert_eq!((c.x, c.y, c.cote), (0.0, 0.0, 200.0));
    }

    #[test]
    fn glisser_deplace_l_image_dans_le_sens_du_pointeur_sans_sortir() {
        let r = Recadrage {
            zoom: 2.0,
            ..Recadrage::default()
        };
        let d = deplacer(r, 400, 400, 30.0, -15.0);
        assert!(proche(d.x, -2.0 * 200.0 * 30.0 / (COTE_APERCU * 200.0)));
        assert!(proche(d.y, 2.0 * 200.0 * 15.0 / (COTE_APERCU * 200.0)));
        assert!(d.x < 0.0 && d.y > 0.0, "{d:?}");
        let loin = deplacer(r, 400, 400, -10_000.0, 10_000.0);
        assert_eq!((loin.x, loin.y), (1.0, -1.0));
        let plein = deplacer(Recadrage::default(), 300, 300, 50.0, 50.0);
        assert_eq!((plein.x, plein.y), (-1.0, -1.0));
        assert_eq!(deplacer(r, 400, 400, f32::NAN, 0.0), r);
    }

    #[test]
    fn le_zoom_reste_entre_un_et_quatre() {
        let r = Recadrage::default();
        assert_eq!(zoomer(r, 0.5).zoom, 1.0);
        assert_eq!(zoomer(r, 9.0).zoom, ZOOM_MAX);
        assert_eq!(zoomer(r, f32::NAN).zoom, 1.0);
        assert_eq!(zoom_lisible(2.25), "2,2×");
        assert_eq!(zoom_lisible(1.0), "1,0×");
    }

    #[test]
    fn la_banniere_est_une_couleur_hexadecimale_ou_vide() {
        assert!(banniere_valide(""));
        assert!(banniere_valide("#A0b1c2"));
        assert!(!banniere_valide("#abc"));
        assert!(!banniere_valide("abcdef0"));
        assert!(!banniere_valide("#gg0000"));
        assert!(NUANCES_BANNIERE.iter().all(|c| banniere_valide(c)));
    }

    fn etat() -> Etat {
        let mut etat = Etat {
            compte: GetMeReponse {
                id: Some("me".into()),
                pseudo: Some("moi".into()),
                ..GetMeReponse::default()
            },
            ..Etat::default()
        };
        let serveur = |id: &str, nom: &str| Server {
            id: Some(id.into()),
            name: Some(nom.into()),
            ..Server::default()
        };
        etat.serveurs = vec![serveur("s1", "Atelier"), serveur("s2", "Club")];
        etat.details.insert(
            "s2".into(),
            Detail {
                members: Some(vec![Membre {
                    id: Some("me".into()),
                    banner_color: Some("#AA00FF".into()),
                    ..Membre::default()
                }]),
                ..Detail::default()
            },
        );
        etat.avatars_serveur
            .insert("s2".into(), [("me".to_string(), 3)].into());
        etat
    }

    #[test]
    fn ma_banniere_et_mes_photos_par_serveur_se_lisent_dans_l_etat() {
        let etat = etat();
        assert_eq!(ma_banniere(&etat), "#aa00ff");
        assert_eq!(
            photos_par_serveur(&etat),
            [
                PhotoServeur {
                    serveur: "s1".into(),
                    nom: "Atelier".into(),
                    version: 0
                },
                PhotoServeur {
                    serveur: "s2".into(),
                    nom: "Club".into(),
                    version: 3
                },
            ]
        );
        assert_eq!(ma_banniere(&Etat::default()), "");
    }
}
