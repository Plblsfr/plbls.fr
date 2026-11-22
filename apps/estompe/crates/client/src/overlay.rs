use estompe_plateforme::ZoneEcran;
use serde::{Deserialize, Serialize};

pub const MARGE: f64 = 16.0;
pub const ECART: u32 = 8;
pub const LARGEUR_TEXTE: u32 = 220;
pub const OPACITE_MIN: u8 = 30;
pub const OPACITE_MAX: u8 = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuandOverlay {
    #[default]
    Jeux,
    Toujours,
    Jamais,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TailleOverlay {
    #[serde(rename = "sm")]
    Petite,
    #[default]
    #[serde(rename = "md")]
    Moyenne,
    #[serde(rename = "lg")]
    Grande,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoinOverlay {
    #[serde(rename = "tl")]
    HautGauche,
    #[default]
    #[serde(rename = "ml")]
    CentreGauche,
    #[serde(rename = "bl")]
    BasGauche,
    #[serde(rename = "tr")]
    HautDroite,
    #[serde(rename = "mr")]
    CentreDroite,
    #[serde(rename = "br")]
    BasDroite,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeOverlay {
    #[default]
    Tous,
    Parlent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesOverlay {
    pub quand: QuandOverlay,
    pub taille: TailleOverlay,
    pub coin: CoinOverlay,
    pub mode: ModeOverlay,
    pub opacite: u8,
}

impl Default for ReglagesOverlay {
    fn default() -> Self {
        Self {
            quand: QuandOverlay::default(),
            taille: TailleOverlay::default(),
            coin: CoinOverlay::default(),
            mode: ModeOverlay::default(),
            opacite: OPACITE_MAX,
        }
    }
}

impl ReglagesOverlay {
    pub fn borne(self) -> Self {
        Self {
            opacite: self.opacite.clamp(OPACITE_MIN, OPACITE_MAX),
            ..self
        }
    }

    pub fn alpha(&self) -> u8 {
        let opacite = u32::from(self.opacite.clamp(OPACITE_MIN, OPACITE_MAX));
        ((opacite * 255 + 50) / 100) as u8
    }
}

impl TailleOverlay {
    pub const TOUTES: [TailleOverlay; 3] = [Self::Petite, Self::Moyenne, Self::Grande];

    pub fn avatar(self) -> u32 {
        match self {
            Self::Petite => 28,
            Self::Moyenne => 40,
            Self::Grande => 56,
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Petite => "Petit",
            Self::Moyenne => "Moyen",
            Self::Grande => "Grand",
        }
    }
}

impl CoinOverlay {
    pub const TOUS: [CoinOverlay; 6] = [
        Self::HautGauche,
        Self::HautDroite,
        Self::CentreGauche,
        Self::CentreDroite,
        Self::BasGauche,
        Self::BasDroite,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::HautGauche => "tl",
            Self::CentreGauche => "ml",
            Self::BasGauche => "bl",
            Self::HautDroite => "tr",
            Self::CentreDroite => "mr",
            Self::BasDroite => "br",
        }
    }

    pub fn de_code(code: &str) -> Option<Self> {
        Self::TOUS.into_iter().find(|c| c.code() == code)
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::HautGauche => "Haut gauche",
            Self::CentreGauche => "Centre gauche",
            Self::BasGauche => "Bas gauche",
            Self::HautDroite => "Haut droite",
            Self::CentreDroite => "Centre droit",
            Self::BasDroite => "Bas droite",
        }
    }

    fn a_gauche(self) -> bool {
        matches!(
            self,
            Self::HautGauche | Self::CentreGauche | Self::BasGauche
        )
    }
}

pub fn retenus<T>(items: Vec<T>, mode: ModeOverlay, parle: impl Fn(&T) -> bool) -> Vec<T> {
    match mode {
        ModeOverlay::Tous => items,
        ModeOverlay::Parlent => items.into_iter().filter(|i| parle(i)).collect(),
    }
}

pub fn visible(quand: QuandOverlay, en_jeu: bool) -> bool {
    match quand {
        QuandOverlay::Toujours => true,
        QuandOverlay::Jeux => en_jeu,
        QuandOverlay::Jamais => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TailleLogique {
    pub largeur: f64,
    pub hauteur: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionLogique {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
    pub echelle: f32,
}

pub fn taille_fenetre(taille: TailleOverlay, lignes: usize) -> TailleLogique {
    let avatar = taille.avatar();
    TailleLogique {
        largeur: f64::from(avatar + LARGEUR_TEXTE),
        hauteur: (lignes as f64 * f64::from(avatar + ECART)).max(1.0),
    }
}

pub fn position(
    coin: CoinOverlay,
    ecran: &ZoneEcran,
    echelle: f64,
    fenetre: TailleLogique,
) -> PositionLogique {
    let (x, y) = (f64::from(ecran.x), f64::from(ecran.y));
    let (l, h) = (f64::from(ecran.largeur), f64::from(ecran.hauteur));
    let gauche = x / echelle + MARGE;
    let droite = (x + l) / echelle - fenetre.largeur - MARGE;
    let haut = y / echelle + MARGE;
    let bas = (y + h) / echelle - fenetre.hauteur - MARGE;
    let milieu = (y + h / 2.0) / echelle - fenetre.hauteur / 2.0;
    PositionLogique {
        x: if coin.a_gauche() { gauche } else { droite },
        y: match coin {
            CoinOverlay::HautGauche | CoinOverlay::HautDroite => haut,
            CoinOverlay::CentreGauche | CoinOverlay::CentreDroite => milieu,
            CoinOverlay::BasGauche | CoinOverlay::BasDroite => bas,
        },
    }
}

pub fn placement(position: PositionLogique, fenetre: TailleLogique, echelle: f64) -> Placement {
    Placement {
        x: (position.x * echelle).round() as i32,
        y: (position.y * echelle).round() as i32,
        largeur: (fenetre.largeur * echelle).round().max(1.0) as u32,
        hauteur: (fenetre.hauteur * echelle).round().max(1.0) as u32,
        echelle: echelle as f32,
    }
}

pub fn placer(
    coin: CoinOverlay,
    ecran: &ZoneEcran,
    echelle: f64,
    fenetre: TailleLogique,
) -> Placement {
    placement(position(coin, ecran, echelle, fenetre), fenetre, echelle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ecran(x: i32, y: i32, largeur: u32, hauteur: u32) -> ZoneEcran {
        ZoneEcran {
            x,
            y,
            largeur,
            hauteur,
        }
    }

    #[test]
    fn n_affiche_que_ceux_qui_parlent_en_mode_parlent() {
        let liste = vec![("a", true), ("b", false)];
        assert_eq!(retenus(liste.clone(), ModeOverlay::Tous, |p| p.1).len(), 2);
        assert_eq!(
            retenus(liste, ModeOverlay::Parlent, |p| p.1),
            vec![("a", true)]
        );
    }

    #[test]
    fn place_la_fenetre_dans_le_coin_choisi_en_pixels_logiques() {
        let mon = ecran(1920, 0, 3840, 2160);
        let win = taille_fenetre(TailleOverlay::Moyenne, 2);
        assert_eq!(
            position(CoinOverlay::HautGauche, &mon, 2.0, win),
            PositionLogique { x: 976.0, y: 16.0 }
        );
        let br = position(CoinOverlay::BasDroite, &mon, 2.0, win);
        assert_eq!(br.x, 960.0 + 1920.0 - win.largeur - 16.0);
        assert_eq!(br.y, 1080.0 - win.hauteur - 16.0);
    }

    #[test]
    fn donne_a_l_overlay_sa_place_et_sa_taille_en_pixels_physiques() {
        let mon = ecran(1920, 0, 3840, 2160);
        let win = taille_fenetre(TailleOverlay::Moyenne, 2);
        assert_eq!(
            placer(CoinOverlay::HautGauche, &mon, 1.25, win),
            Placement {
                x: 1940,
                y: 20,
                largeur: 325,
                hauteur: 120,
                echelle: 1.25
            }
        );
    }

    #[test]
    fn centre_la_fenetre_verticalement_au_milieu_de_l_ecran() {
        let mon = ecran(0, 0, 1920, 1080);
        let win = TailleLogique {
            largeur: 100.0,
            hauteur: 200.0,
        };
        assert_eq!(
            position(CoinOverlay::CentreGauche, &mon, 1.0, win),
            PositionLogique { x: 16.0, y: 440.0 }
        );
        assert_eq!(
            position(CoinOverlay::CentreDroite, &mon, 1.0, win),
            PositionLogique {
                x: 1804.0,
                y: 440.0
            }
        );
    }

    #[test]
    fn affiche_selon_le_mode_et_le_jeu_au_premier_plan() {
        assert!(visible(QuandOverlay::Jeux, true));
        assert!(!visible(QuandOverlay::Jeux, false));
        assert!(visible(QuandOverlay::Toujours, false));
        assert!(!visible(QuandOverlay::Jamais, true));
    }

    #[test]
    fn memes_tailles_d_avatar_et_de_fenetre_que_l_app_windows() {
        assert_eq!(
            TailleOverlay::TOUTES.map(TailleOverlay::avatar),
            [28, 40, 56]
        );
        assert_eq!(
            taille_fenetre(TailleOverlay::Grande, 3),
            TailleLogique {
                largeur: 276.0,
                hauteur: 192.0
            }
        );
        assert_eq!(taille_fenetre(TailleOverlay::Petite, 0).hauteur, 1.0);
    }

    #[test]
    fn les_coins_se_lisent_par_leur_code() {
        for coin in CoinOverlay::TOUS {
            assert_eq!(CoinOverlay::de_code(coin.code()), Some(coin));
        }
        assert_eq!(CoinOverlay::de_code("xx"), None);
        assert_eq!(CoinOverlay::default(), CoinOverlay::CentreGauche);
    }

    #[test]
    fn l_opacite_est_bornee_et_convertie_en_alpha() {
        let mut r = ReglagesOverlay::default();
        assert_eq!(r.alpha(), 255);
        r.opacite = 5;
        assert_eq!(r.borne().opacite, OPACITE_MIN);
        assert_eq!(r.alpha(), 77);
        r.opacite = 250;
        assert_eq!(r.borne().opacite, OPACITE_MAX);
        r.opacite = 50;
        assert_eq!(r.alpha(), 128);
    }

    #[test]
    fn les_reglages_se_relisent_avec_les_noms_du_front() {
        let r: ReglagesOverlay = serde_json::from_str(
            r#"{"quand":"toujours","taille":"lg","coin":"br","mode":"parlent","opacite":70}"#,
        )
        .unwrap();
        assert_eq!(
            r,
            ReglagesOverlay {
                quand: QuandOverlay::Toujours,
                taille: TailleOverlay::Grande,
                coin: CoinOverlay::BasDroite,
                mode: ModeOverlay::Parlent,
                opacite: 70,
            }
        );
        let defaut: ReglagesOverlay = serde_json::from_str("{}").unwrap();
        assert_eq!(defaut, ReglagesOverlay::default());
    }
}
