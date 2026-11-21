use estompe_plateforme::{Image, SourceCapture, TypeSource};
use serde::{Deserialize, Serialize};

use super::camera::FormatVideo;
use super::video::{correspondance, rgb_vers_yuv, ImageI420, TrameVideo};

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum ResolutionPartage {
    #[serde(rename = "720p")]
    R720,
    #[default]
    #[serde(rename = "1080p")]
    R1080,
    #[serde(rename = "1440p")]
    R1440,
    #[serde(rename = "2160p")]
    R2160,
}

pub const CADENCES_PARTAGE: [u32; 3] = [30, 60, 120];

impl ResolutionPartage {
    pub const TOUTES: [ResolutionPartage; 4] = [Self::R720, Self::R1080, Self::R1440, Self::R2160];

    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Self::R720 => (1280, 720),
            Self::R1080 => (1920, 1080),
            Self::R1440 => (2560, 1440),
            Self::R2160 => (3840, 2160),
        }
    }

    pub fn hauteur(self) -> u32 {
        self.dimensions().1
    }

    pub fn debit_de_base(self) -> u64 {
        match self {
            Self::R720 => 1_500_000,
            Self::R1080 => 3_000_000,
            Self::R1440 => 7_000_000,
            Self::R2160 => 12_000_000,
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::R720 => "720p",
            Self::R1080 => "1080p",
            Self::R1440 => "1440p",
            Self::R2160 => "4K",
        }
    }

    fn rang(self) -> usize {
        Self::TOUTES.iter().position(|r| *r == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QualitePartage {
    pub resolution: ResolutionPartage,
    pub ips: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlafondPartage {
    pub ips: u32,
    pub hauteur: u32,
}

impl Default for PlafondPartage {
    fn default() -> Self {
        Self {
            ips: 60,
            hauteur: 2160,
        }
    }
}

impl PlafondPartage {
    pub fn avec_source(self, hauteur_source: Option<u32>) -> Self {
        Self {
            ips: self.ips,
            hauteur: hauteur_source
                .filter(|h| *h > 0)
                .map_or(self.hauteur, |h| self.hauteur.min(h)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesPartage {
    pub resolution: ResolutionPartage,
    pub ips: u32,
    pub fluidite: bool,
    pub auto: bool,
    pub son: bool,
}

impl Default for ReglagesPartage {
    fn default() -> Self {
        Self {
            resolution: ResolutionPartage::R1080,
            ips: 30,
            fluidite: true,
            auto: true,
            son: true,
        }
    }
}

impl ReglagesPartage {
    pub fn qualite(self, plafond: PlafondPartage) -> QualitePartage {
        borner(self.resolution, self.ips, plafond)
    }

    pub fn borne(self) -> Self {
        Self {
            ips: if CADENCES_PARTAGE.contains(&self.ips) {
                self.ips
            } else {
                30
            },
            ..self
        }
    }

    pub fn appliquer(&mut self, prereglage: &Prereglage) {
        self.resolution = prereglage.resolution;
        self.ips = prereglage.ips;
        self.fluidite = prereglage.fluidite;
    }

    pub fn prereglage(self, plafond: PlafondPartage) -> Option<usize> {
        let q = self.qualite(plafond);
        PREREGLAGES.iter().position(|p| {
            p.resolution == q.resolution && p.ips == q.ips && p.fluidite == self.fluidite
        })
    }
}

pub fn debit_partage(resolution: ResolutionPartage, ips: u32) -> u64 {
    let facteur = match ips {
        0..=30 => 1.0,
        31..=60 => 1.7,
        _ => 2.5,
    };
    (resolution.debit_de_base() as f64 * facteur).round() as u64
}

pub fn borner(resolution: ResolutionPartage, ips: u32, plafond: PlafondPartage) -> QualitePartage {
    let possibles = ResolutionPartage::TOUTES
        .iter()
        .filter(|r| r.hauteur() <= plafond.hauteur)
        .count();
    let rang = resolution.rang().min(possibles.max(1) - 1);
    let cadences: Vec<u32> = CADENCES_PARTAGE
        .iter()
        .copied()
        .filter(|f| *f <= plafond.ips)
        .collect();
    QualitePartage {
        resolution: ResolutionPartage::TOUTES[rang],
        ips: if cadences.contains(&ips) {
            ips
        } else {
            cadences.last().copied().unwrap_or(30)
        },
    }
}

pub fn palier_inferieur(q: QualitePartage, fluidite: bool) -> Option<QualitePartage> {
    let rang = q.resolution.rang();
    if fluidite && rang > 0 {
        return Some(QualitePartage {
            resolution: ResolutionPartage::TOUTES[rang - 1],
            ips: q.ips,
        });
    }
    if q.ips > 30 {
        return Some(QualitePartage {
            resolution: q.resolution,
            ips: if q.ips == 120 { 60 } else { 30 },
        });
    }
    (rang > 0).then(|| QualitePartage {
        resolution: ResolutionPartage::TOUTES[rang - 1],
        ips: 30,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndiceContenu {
    Mouvement,
    Detail,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Degradation {
    #[default]
    Auto,
    GarderCadence,
    GarderResolution,
}

pub fn mode_partage(fluidite: bool) -> (IndiceContenu, Degradation) {
    if fluidite {
        (IndiceContenu::Mouvement, Degradation::GarderCadence)
    } else {
        (IndiceContenu::Detail, Degradation::GarderResolution)
    }
}

impl QualitePartage {
    pub fn format(self, fluidite: bool) -> FormatVideo {
        let (largeur, hauteur) = self.resolution.dimensions();
        let (indice, degradation) = mode_partage(fluidite);
        FormatVideo {
            largeur,
            hauteur,
            images_par_seconde: self.ips,
            debit: debit_partage(self.resolution, self.ips),
            simulcast: hauteur > 1080,
            degradation,
            ecran: indice == IndiceContenu::Detail,
        }
    }

    pub fn libelle(self) -> String {
        format!("{} · {} i/s", self.resolution.libelle(), self.ips)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prereglage {
    pub nom: &'static str,
    pub indice: &'static str,
    pub resolution: ResolutionPartage,
    pub ips: u32,
    pub fluidite: bool,
}

pub const PREREGLAGES: [Prereglage; 3] = [
    Prereglage {
        nom: "Fluide",
        indice: "Jeux et vidéos : mouvement fluide",
        resolution: ResolutionPartage::R1080,
        ips: 60,
        fluidite: true,
    },
    Prereglage {
        nom: "Net",
        indice: "Documents et code : image nette",
        resolution: ResolutionPartage::R1440,
        ips: 30,
        fluidite: false,
    },
    Prereglage {
        nom: "Économie de données",
        indice: "Connexion lente ou mobile",
        resolution: ResolutionPartage::R720,
        ips: 30,
        fluidite: true,
    },
];

pub fn mbits(bits: u64) -> String {
    let m = bits as f64 / 1e6;
    let texte = if bits < 10_000_000 {
        format!("{m:.1}")
    } else {
        format!("{m:.0}")
    };
    format!("{} Mb/s", texte.replace(".0", "").replace('.', ","))
}

pub fn note_resolution(
    resolution: ResolutionPartage,
    serveur: PlafondPartage,
    effectif: PlafondPartage,
) -> &'static str {
    if resolution.hauteur() <= effectif.hauteur {
        ""
    } else if resolution.hauteur() > serveur.hauteur {
        " (non proposé par le serveur)"
    } else {
        " (au-delà de ton écran)"
    }
}

pub fn resume_partage(reglages: ReglagesPartage, plafond: PlafondPartage) -> String {
    let q = reglages.qualite(plafond);
    let mut texte = format!(
        "Partage : {}, environ {} montants.",
        q.libelle(),
        mbits(debit_partage(q.resolution, q.ips))
    );
    texte.push(' ');
    texte.push_str(if reglages.auto {
        if reglages.fluidite {
            "Si la connexion ou l'ordinateur ne suit pas, la qualité baisse toute seule (résolution d'abord, puis images/s)."
        } else {
            "Si la connexion ou l'ordinateur ne suit pas, la qualité baisse toute seule (images/s d'abord, puis résolution)."
        }
    } else {
        "La qualité choisie est maintenue, même si la connexion est mauvaise."
    });
    if q.ips > 60 {
        texte.push_str(" Beaucoup de systèmes plafonnent la capture d'écran à 60 i/s.");
    }
    if q.resolution.hauteur() > 1080 {
        texte
            .push_str(" Au-delà de 1080p, chaque spectateur consomme aussi beaucoup côté serveur.");
    }
    texte
}

pub fn taille_reduite(largeur: u32, hauteur: u32, hauteur_max: u32) -> (u32, u32) {
    let (l, h) = if hauteur_max > 0 && hauteur > hauteur_max {
        (
            (u64::from(largeur) * u64::from(hauteur_max) / u64::from(hauteur.max(1))) as u32,
            hauteur_max,
        )
    } else {
        (largeur, hauteur)
    };
    ((l & !1).max(2), (h & !1).max(2))
}

pub fn i420_de_rgba(image: &Image, hauteur_max: u32) -> Option<ImageI420> {
    if image.largeur == 0
        || image.hauteur == 0
        || image.rgba.len() < (image.largeur * image.hauteur * 4) as usize
    {
        return None;
    }
    let (largeur, hauteur) = taille_reduite(image.largeur, image.hauteur, hauteur_max);
    let colonnes = correspondance(image.largeur, largeur);
    let rangees = correspondance(image.hauteur, hauteur);
    let mut sortie = ImageI420::noire(largeur, hauteur);
    let cl = sortie.largeur_chroma() as usize;
    let pas = image.largeur as usize * 4;
    for (y, sy) in rangees.iter().enumerate() {
        let ligne = &image.rgba[*sy as usize * pas..][..pas];
        for (x, sx) in colonnes.iter().enumerate() {
            let p = &ligne[*sx as usize * 4..][..3];
            let [luma, u, v] = rgb_vers_yuv(p[0], p[1], p[2]);
            sortie.y[y * largeur as usize + x] = luma;
            if y % 2 == 0 && x % 2 == 0 {
                sortie.u[(y / 2) * cl + x / 2] = u;
                sortie.v[(y / 2) * cl + x / 2] = v;
            }
        }
    }
    Some(sortie)
}

pub struct ImagePartage(pub Image);

impl TrameVideo for ImagePartage {
    fn largeur(&self) -> u32 {
        self.0.largeur
    }

    fn hauteur(&self) -> u32 {
        self.0.hauteur
    }

    fn vers_rgba(&self, largeur: u32, hauteur: u32, sortie: &mut [u8]) {
        let source = &self.0;
        if largeur == 0
            || hauteur == 0
            || source.largeur == 0
            || source.hauteur == 0
            || sortie.len() < (largeur * hauteur * 4) as usize
            || source.rgba.len() < (source.largeur * source.hauteur * 4) as usize
        {
            return;
        }
        let colonnes = correspondance(source.largeur, largeur);
        let rangees = correspondance(source.hauteur, hauteur);
        let pas = source.largeur as usize * 4;
        for (rangee, sy) in sortie.chunks_exact_mut(largeur as usize * 4).zip(rangees) {
            let ligne = &source.rgba[sy as usize * pas..][..pas];
            for (pixel, sx) in rangee.as_chunks_mut::<4>().0.iter_mut().zip(&colonnes) {
                let p = &ligne[*sx as usize * 4..][..4];
                *pixel = [p[0], p[1], p[2], 255];
            }
        }
    }
}

pub fn genre_affiche(source: &SourceCapture) -> &'static str {
    match source.genre {
        TypeSource::Ecran => "écran",
        TypeSource::Fenetre => "fenêtre",
    }
}

pub fn message_source_perdue(source: &SourceCapture) -> String {
    match source.genre {
        TypeSource::Fenetre => "Partage arrêté : la fenêtre partagée a été fermée.".to_string(),
        TypeSource::Ecran => "Partage arrêté : l'écran partagé n'est plus disponible.".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartageLocal {
    pub source: SourceCapture,
    pub qualite: QualitePartage,
    pub reduite: bool,
    pub son: bool,
}

pub const MESSAGE_PARTAGE_IMPOSSIBLE: &str = "Partage d'écran impossible.";

#[cfg(test)]
mod tests {
    use super::*;

    fn q(resolution: ResolutionPartage, ips: u32) -> QualitePartage {
        QualitePartage { resolution, ips }
    }

    fn plafond(ips: u32, hauteur: u32) -> PlafondPartage {
        PlafondPartage { ips, hauteur }
    }

    #[test]
    fn ramene_le_choix_aux_plafonds_du_serveur() {
        use ResolutionPartage::*;
        assert_eq!(borner(R2160, 120, plafond(60, 1440)), q(R1440, 60));
        assert_eq!(borner(R1080, 120, plafond(30, 2160)), q(R1080, 30));
        assert_eq!(borner(R2160, 120, plafond(120, 2160)), q(R2160, 120));
        assert_eq!(borner(R1080, 30, plafond(60, 480)), q(R720, 30));
        assert_eq!(
            PlafondPartage::default().avec_source(Some(1080)),
            plafond(60, 1080)
        );
        assert_eq!(
            PlafondPartage::default().avec_source(None),
            plafond(60, 2160)
        );
    }

    #[test]
    fn descend_d_abord_les_images_par_seconde_puis_la_resolution_puis_s_arrete() {
        use ResolutionPartage::*;
        assert_eq!(palier_inferieur(q(R2160, 120), false), Some(q(R2160, 60)));
        assert_eq!(palier_inferieur(q(R2160, 60), false), Some(q(R2160, 30)));
        assert_eq!(palier_inferieur(q(R2160, 30), false), Some(q(R1440, 30)));
        assert_eq!(palier_inferieur(q(R720, 30), false), None);
    }

    #[test]
    fn en_fluidite_descend_d_abord_la_resolution_puis_les_images_par_seconde() {
        use ResolutionPartage::*;
        assert_eq!(palier_inferieur(q(R1440, 60), true), Some(q(R1080, 60)));
        assert_eq!(palier_inferieur(q(R1080, 60), true), Some(q(R720, 60)));
        assert_eq!(palier_inferieur(q(R720, 60), true), Some(q(R720, 30)));
        assert_eq!(palier_inferieur(q(R720, 30), true), None);
    }

    #[test]
    fn la_fluidite_garde_les_images_par_seconde_la_nettete_garde_la_resolution() {
        assert_eq!(
            mode_partage(true),
            (IndiceContenu::Mouvement, Degradation::GarderCadence)
        );
        assert_eq!(
            mode_partage(false),
            (IndiceContenu::Detail, Degradation::GarderResolution)
        );
        let f = q(ResolutionPartage::R1440, 60).format(false);
        assert!(f.ecran && f.simulcast && f.degradation == Degradation::GarderResolution);
        assert!(!q(ResolutionPartage::R1080, 60).format(true).simulcast);
    }

    #[test]
    fn le_debit_croit_avec_les_images_par_seconde() {
        use ResolutionPartage::*;
        assert!(debit_partage(R1080, 60) > debit_partage(R1080, 30));
        assert!(debit_partage(R1080, 120) > debit_partage(R1080, 60));
        assert_eq!(debit_partage(R1080, 60), 5_100_000);
        assert_eq!(mbits(debit_partage(R1080, 60)), "5,1 Mb/s");
        assert_eq!(mbits(debit_partage(R2160, 30)), "12 Mb/s");
        assert_eq!(mbits(debit_partage(R1080, 30)), "3 Mb/s");
    }

    #[test]
    fn les_prereglages_et_les_notes_suivent_le_front() {
        let mut r = ReglagesPartage::default();
        assert_eq!(r.prereglage(PlafondPartage::default()), None);
        r.appliquer(&PREREGLAGES[0]);
        assert_eq!(r.prereglage(PlafondPartage::default()), Some(0));
        assert_eq!(r.prereglage(plafond(30, 2160)), None);
        let serveur = plafond(60, 1440);
        let effectif = serveur.avec_source(Some(1080));
        use ResolutionPartage::*;
        assert_eq!(note_resolution(R1080, serveur, effectif), "");
        assert_eq!(
            note_resolution(R1440, serveur, effectif),
            " (au-delà de ton écran)"
        );
        assert_eq!(
            note_resolution(R2160, serveur, effectif),
            " (non proposé par le serveur)"
        );
        assert!(resume_partage(r, PlafondPartage::default())
            .starts_with("Partage : 1080p · 60 i/s, environ 5,1 Mb/s montants."));
        let json = serde_json::to_string(&ReglagesPartage::default()).unwrap();
        assert!(json.contains("\"1080p\""), "{json}");
        assert_eq!(
            ReglagesPartage {
                ips: 45,
                ..ReglagesPartage::default()
            }
            .borne()
            .ips,
            30
        );
    }

    #[test]
    fn une_image_rgba_est_reduite_en_i420_aux_dimensions_paires() {
        assert_eq!(taille_reduite(3840, 2160, 1080), (1920, 1080));
        assert_eq!(taille_reduite(1921, 1081, 0), (1920, 1080));
        assert_eq!(taille_reduite(1000, 3000, 1080), (360, 1080));
        let image = Image {
            largeur: 8,
            hauteur: 4,
            rgba: [250u8, 10, 10, 255].repeat(32),
        };
        let i420 = i420_de_rgba(&image, 2).unwrap();
        assert_eq!((i420.largeur, i420.hauteur), (4, 2));
        let [r, g, b] = i420.pixel(1, 1);
        assert!(r > 200 && g < 60 && b < 60, "{r} {g} {b}");
        assert!(i420_de_rgba(&Image::default(), 0).is_none());
        let mut rgba = vec![0u8; 4 * 2 * 4];
        ImagePartage(image).vers_rgba(4, 2, &mut rgba);
        assert_eq!(&rgba[..4], &[250, 10, 10, 255]);
    }
}
