use estompe_plateforme::{CapaciteEncodeur, MaterielEncodage};

use super::types::{
    Codec, Qualite, ReglagesReplay as Reglages, Resolution, CADENCES, DUREE_MAX_S, DUREE_MIN_S,
};

pub const RESOLUTIONS: [Resolution; 5] = [
    Resolution::R720,
    Resolution::R1080,
    Resolution::R1440,
    Resolution::R4k,
    Resolution::Native,
];
pub const AUDIO_JEU_KBPS: u32 = 160;
pub const AUDIO_MICRO_KBPS: u32 = 96;
pub const GOP_S: u32 = 1;
pub const MB_S_H264: u64 = 2_073_600;
pub const MB_S_HEVC: u64 = 16_711_680;
const DEBITS_KBPS: [[u32; 4]; 4] = [
    [5000, 8000, 12000, 14000],
    [8000, 12000, 18000, 20000],
    [12000, 25000, 35000, 40000],
    [25000, 45000, 70000, 80000],
];
const PIXELS: [u64; 4] = [1280 * 720, 1920 * 1080, 2560 * 1440, 3840 * 2160];
pub const NON_PRIS_EN_CHARGE: &str = "non pris en charge par la carte graphique";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoixResolution {
    pub id: Resolution,
    pub disponible: bool,
    pub raison: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoixCadence {
    pub ips: u32,
    pub disponible: bool,
    pub raison: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub resolutions: Vec<ChoixResolution>,
    pub cadences: Vec<ChoixCadence>,
    pub duree_max_s: u32,
    pub raison_duree: Option<String>,
    pub codec: Codec,
    pub debit_kbps: u32,
    pub memoire_octets: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Materiel {
    pub ecran_hz: u32,
    pub source: (u32, u32),
    pub h264: Option<CapaciteEncodeur>,
    pub hevc: Option<CapaciteEncodeur>,
    pub ram_libre: u64,
}

impl Materiel {
    pub fn de(m: &MaterielEncodage, source: (u32, u32)) -> Self {
        Self {
            ecran_hz: m.ecran_hz.max(1),
            source,
            h264: m.h264.clone(),
            hevc: m.hevc.clone(),
            ram_libre: m.ram_libre,
        }
    }

    pub fn encodeur(&self, codec: Codec) -> Option<&CapaciteEncodeur> {
        match codec {
            Codec::H264 => self.h264.as_ref(),
            Codec::Hevc => self.hevc.as_ref(),
        }
    }
}

pub fn audio_kbps(r: &Reglages) -> u32 {
    AUDIO_JEU_KBPS + if r.micro { AUDIO_MICRO_KBPS } else { 0 }
}

pub fn hauteur_cible(r: Resolution) -> Option<u32> {
    match r {
        Resolution::R720 => Some(720),
        Resolution::R1080 => Some(1080),
        Resolution::R1440 => Some(1440),
        Resolution::R4k => Some(2160),
        Resolution::Native => None,
    }
}

pub fn dimensions(r: Resolution, source: (u32, u32)) -> (u32, u32) {
    let (sl, sh) = (source.0.max(2), source.1.max(2));
    let h = hauteur_cible(r).map_or(sh, |c| c.min(sh));
    let l = (sl as u64 * h as u64 / sh as u64) as u32;
    ((l & !1).max(2), (h & !1).max(2))
}

fn macroblocs(dim: (u32, u32)) -> u64 {
    dim.0.div_ceil(16) as u64 * dim.1.div_ceil(16) as u64
}

fn accepte(e: &CapaciteEncodeur, dim: (u32, u32), ips: u32) -> bool {
    dim.0 <= e.largeur_max
        && dim.1 <= e.hauteur_max
        && macroblocs(dim) * ips as u64 <= e.mb_par_seconde
}

pub fn codec(dim: (u32, u32), ips: u32, m: &Materiel) -> Option<Codec> {
    [Codec::H264, Codec::Hevc]
        .into_iter()
        .find(|c| m.encodeur(*c).is_some_and(|e| accepte(e, dim, ips)))
}

fn facteur(q: Qualite) -> u64 {
    match q {
        Qualite::Haute => 100,
        Qualite::Moyenne => 65,
        Qualite::Basse => 40,
    }
}

pub fn debit_kbps(dim: (u32, u32), ips: u32, q: Qualite) -> u32 {
    let colonne = CADENCES
        .iter()
        .position(|c| *c >= ips)
        .unwrap_or(CADENCES.len() - 1);
    let pixels = dim.0 as u64 * dim.1 as u64;
    let table = |i: usize| DEBITS_KBPS[i][colonne] as u64;
    let haut = match PIXELS.iter().position(|p| *p >= pixels) {
        Some(0) => table(0) * pixels / PIXELS[0],
        None => table(3) * pixels / PIXELS[3],
        Some(i) => {
            table(i - 1)
                + (table(i) - table(i - 1)) * (pixels - PIXELS[i - 1]) / (PIXELS[i] - PIXELS[i - 1])
        }
    };
    (haut * facteur(q) / 100).max(1000) as u32
}

pub fn memoire_octets(debit_total_kbps: u32, duree_s: u32) -> u64 {
    debit_total_kbps as u64 * 125 * (duree_s + 2 * GOP_S) as u64 * 115 / 100
}

pub fn duree_max_s(debit_total_kbps: u32, ram_libre: u64) -> u32 {
    let par_seconde = debit_total_kbps as u64 * 125 * 115 / 100;
    ((ram_libre / 4 / par_seconde.max(1)).saturating_sub(2 * GOP_S as u64)).min(DUREE_MAX_S as u64)
        as u32
}

fn raison_resolution(r: Resolution, ips: u32, m: &Materiel) -> Option<String> {
    if hauteur_cible(r).is_some_and(|h| h > m.source.1) {
        return Some(format!(
            "plus grand que l'image du jeu ({} × {})",
            m.source.0, m.source.1
        ));
    }
    let dim = dimensions(r, m.source);
    (codec(dim, ips, m).is_none() && codec(dim, CADENCES[0], m).is_none())
        .then(|| NON_PRIS_EN_CHARGE.to_string())
}

fn raison_cadence(ips: u32, dim: (u32, u32), m: &Materiel) -> Option<String> {
    if ips > m.ecran_hz + 1 {
        return Some(format!("écran en {} Hz", m.ecran_hz));
    }
    codec(dim, ips, m)
        .is_none()
        .then(|| NON_PRIS_EN_CHARGE.to_string())
}

pub fn effectifs(r: &Reglages, m: &Materiel) -> Result<((u32, u32), u32, Codec), String> {
    let resolution = if raison_resolution(r.resolution, r.ips, m).is_none() {
        r.resolution
    } else {
        Resolution::Native
    };
    let dim = dimensions(resolution, m.source);
    let ips = CADENCES
        .iter()
        .rev()
        .copied()
        .find(|c| *c <= r.ips && raison_cadence(*c, dim, m).is_none());
    let ips = ips.ok_or_else(|| NON_PRIS_EN_CHARGE.to_string())?;
    let codec = codec(dim, ips, m).ok_or_else(|| NON_PRIS_EN_CHARGE.to_string())?;
    Ok((dim, ips, codec))
}

pub fn options(r: &Reglages, m: &Materiel) -> Options {
    let dim = dimensions(r.resolution, m.source);
    let resolutions = RESOLUTIONS.iter().map(|id| {
        let raison = raison_resolution(*id, r.ips, m);
        ChoixResolution {
            id: *id,
            disponible: raison.is_none(),
            raison,
        }
    });
    let cadences = CADENCES.iter().map(|ips| {
        let raison = raison_cadence(*ips, dim, m);
        ChoixCadence {
            ips: *ips,
            disponible: raison.is_none(),
            raison,
        }
    });
    let (dim, ips, codec) = effectifs(r, m).unwrap_or((dim, r.ips, Codec::H264));
    let debit = debit_kbps(dim, ips, r.qualite);
    let total = debit + audio_kbps(r);
    let duree_max = duree_max_s(total, m.ram_libre);
    let duree_max = if duree_max < DUREE_MIN_S {
        0
    } else {
        duree_max
    };
    let libre = m.ram_libre as f64 / (1024.0 * 1024.0 * 1024.0);
    Options {
        resolutions: resolutions.collect(),
        cadences: cadences.collect(),
        duree_max_s: duree_max,
        raison_duree: (duree_max < DUREE_MAX_S)
            .then(|| format!("mémoire libre insuffisante ({libre:.1} Go libres)")),
        codec,
        debit_kbps: debit,
        memoire_octets: memoire_octets(total, r.duree_s.min(duree_max.max(DUREE_MIN_S))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encodeur(mb: u64, cote: u32) -> Option<CapaciteEncodeur> {
        Some(CapaciteEncodeur {
            nom: "test".into(),
            mb_par_seconde: mb,
            largeur_max: cote,
            hauteur_max: cote,
        })
    }

    fn materiel(hz: u32, source: (u32, u32)) -> Materiel {
        Materiel {
            ecran_hz: hz,
            source,
            h264: encodeur(MB_S_H264, 4096),
            hevc: encodeur(MB_S_HEVC, 8192),
            ram_libre: 16 << 30,
        }
    }

    #[test]
    fn materiel_lu_de_la_plateforme() {
        let m = MaterielEncodage {
            carte: "c".into(),
            h264: encodeur(MB_S_H264, 4096),
            hevc: None,
            ecran_hz: 0,
            ram_libre: 5,
        };
        let lu = Materiel::de(&m, (800, 600));
        assert_eq!((lu.ecran_hz, lu.source, lu.ram_libre), (1, (800, 600), 5));
        assert!(lu.encodeur(Codec::H264).is_some() && lu.encodeur(Codec::Hevc).is_none());
        assert_eq!(
            audio_kbps(&Reglages {
                micro: true,
                ..Default::default()
            }),
            256
        );
    }

    #[test]
    fn reduit_sans_agrandir_ni_deformer() {
        assert_eq!(dimensions(Resolution::R1080, (2560, 1440)), (1920, 1080));
        assert_eq!(dimensions(Resolution::R4k, (1920, 1080)), (1920, 1080));
        assert_eq!(dimensions(Resolution::R1080, (3440, 1440)), (2580, 1080));
        assert_eq!(dimensions(Resolution::Native, (1367, 769)), (1366, 768));
        assert_eq!(dimensions(Resolution::R720, (1280, 1024)), (900, 720));
    }

    #[test]
    fn table_des_debits_et_qualites() {
        assert_eq!(debit_kbps((1920, 1080), 60, Qualite::Haute), 12000);
        assert_eq!(debit_kbps((2560, 1440), 60, Qualite::Haute), 25000);
        assert_eq!(debit_kbps((3840, 2160), 144, Qualite::Haute), 80000);
        assert_eq!(debit_kbps((1280, 720), 30, Qualite::Haute), 5000);
        assert_eq!(debit_kbps((1920, 1080), 60, Qualite::Moyenne), 7800);
        assert_eq!(debit_kbps((1920, 1080), 60, Qualite::Basse), 4800);
        let ultra = debit_kbps((3440, 1440), 60, Qualite::Haute);
        assert!(ultra > 25000 && ultra < 45000);
        assert!(debit_kbps((640, 360), 30, Qualite::Haute) >= 1000);
    }

    #[test]
    fn hevc_au_dela_de_h264() {
        let m = materiel(144, (3840, 2160));
        assert_eq!(codec((1920, 1080), 144, &m), Some(Codec::H264));
        assert_eq!(codec((3840, 2160), 60, &m), Some(Codec::H264));
        assert_eq!(codec((3840, 2160), 120, &m), Some(Codec::Hevc));
        assert_eq!(codec((2560, 1440), 144, &m), Some(Codec::H264));
        let limite = Materiel {
            h264: encodeur(1_000_000, 4096),
            ..m.clone()
        };
        assert_eq!(codec((2560, 1440), 144, &limite), Some(Codec::Hevc));
        let sans_hevc = Materiel { hevc: None, ..m };
        assert_eq!(codec((3840, 2160), 144, &sans_hevc), None);
    }

    #[test]
    fn choix_hors_de_portee_grises_avec_raison() {
        let m = Materiel {
            hevc: None,
            ..materiel(60, (2560, 1440))
        };
        let o = options(&Reglages::default(), &m);
        let resolution = |id| o.resolutions.iter().find(|r| r.id == id).unwrap();
        assert!(
            resolution(Resolution::R1440).disponible && resolution(Resolution::Native).disponible
        );
        assert_eq!(
            resolution(Resolution::R4k).raison.as_deref(),
            Some("plus grand que l'image du jeu (2560 × 1440)")
        );
        let cadence = |ips| o.cadences.iter().find(|c| c.ips == ips).unwrap();
        assert!(cadence(60).disponible);
        assert_eq!(cadence(120).raison.as_deref(), Some("écran en 60 Hz"));
        assert_eq!((o.codec, o.debit_kbps), (Codec::H264, 12000));

        let m = Materiel {
            hevc: None,
            ..materiel(144, (3840, 2160))
        };
        let o = options(
            &Reglages {
                resolution: Resolution::R4k,
                ..Default::default()
            },
            &m,
        );
        assert_eq!(o.cadences[2].raison.as_deref(), Some(NON_PRIS_EN_CHARGE));
        assert!(o.cadences[1].disponible);
        let m = Materiel {
            h264: encodeur(MB_S_H264, 1920),
            hevc: None,
            ..materiel(144, (3840, 2160))
        };
        assert_eq!(
            options(&Reglages::default(), &m).resolutions[3]
                .raison
                .as_deref(),
            Some(NON_PRIS_EN_CHARGE)
        );
        assert!(options(&Reglages::default(), &materiel(59, (1920, 1080))).cadences[1].disponible);
    }

    #[test]
    fn reglages_effectifs_plafonnes() {
        let m = materiel(60, (1920, 1080));
        let r = Reglages {
            resolution: Resolution::R4k,
            ips: 144,
            ..Default::default()
        };
        assert_eq!(effectifs(&r, &m), Ok(((1920, 1080), 60, Codec::H264)));
        let m = materiel(144, (3840, 2160));
        assert_eq!(effectifs(&r, &m), Ok(((3840, 2160), 144, Codec::Hevc)));
        assert!(effectifs(
            &r,
            &Materiel {
                h264: None,
                hevc: None,
                ..m
            }
        )
        .is_err());
    }

    #[test]
    fn duree_maximale_selon_la_ram() {
        assert_eq!(duree_max_s(25160, 16 << 30), 300);
        let limite = duree_max_s(80256, 4 << 30);
        assert!((80..100).contains(&limite), "{limite}");
        assert!(memoire_octets(80256, limite) <= (4 << 30) / 4);
        let m = Materiel {
            ram_libre: 4 << 30,
            ..materiel(144, (3840, 2160))
        };
        let o = options(
            &Reglages {
                resolution: Resolution::R4k,
                ips: 144,
                micro: true,
                ..Default::default()
            },
            &m,
        );
        assert_eq!(o.duree_max_s, limite);
        assert_eq!(
            o.raison_duree.as_deref(),
            Some("mémoire libre insuffisante (4.0 Go libres)")
        );
        let o = options(
            &Reglages::default(),
            &Materiel {
                ram_libre: 64 << 20,
                ..m
            },
        );
        assert_eq!(o.duree_max_s, 0);
        assert!(options(&Reglages::default(), &materiel(60, (1920, 1080)))
            .raison_duree
            .is_none());
        let minute_1440p = memoire_octets(25160, 60) / (1024 * 1024);
        assert!((190..240).contains(&minute_1440p), "{minute_1440p}");
    }
}
