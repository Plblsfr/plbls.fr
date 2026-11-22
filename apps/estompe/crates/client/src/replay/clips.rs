use std::cmp::Ordering;

use super::affichage::taille;
use super::galerie::{debit_pour_taille, hauteur_pour_debit, Annexe};
use super::types::{Clip, Codec, Stockage};
use crate::video::debit_kbps;

pub const SANS_JEU: &str = "Autres";
pub const PRESQUE_PLEIN: u32 = 90;
pub const AUDIO_KBPS: u32 = 128;
const DEBIT_MIN_KBPS: f64 = 300.0;
const MOIS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JeuClips {
    pub jeu: String,
    pub nombre: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Envoi {
    Direct,
    H264,
    Leger,
    Refus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanEnvoi {
    Original,
    Convertir {
        copie: bool,
        kbps: u32,
        hauteur: Option<u32>,
        pistes: u32,
        duree_s: f64,
    },
}

pub fn jeu_de(clip: &Clip) -> String {
    clip.jeu
        .as_deref()
        .map(str::trim)
        .filter(|j| !j.is_empty())
        .unwrap_or(SANS_JEU)
        .to_string()
}

fn champs(date: &str) -> Option<[i64; 6]> {
    let date = date.trim_end_matches('Z');
    let (jour, heure) = date.split_once('T')?;
    let mut j = jour.splitn(3, '-').map(|x| x.parse::<i64>().ok());
    let mut h = heure
        .splitn(3, ':')
        .map(|x| x.split('.').next().and_then(|x| x.parse::<i64>().ok()));
    Some([
        j.next()??,
        j.next()??,
        j.next()??,
        h.next()??,
        h.next()??,
        h.next().flatten().unwrap_or(0),
    ])
}

fn instant(clip: &Clip) -> [i64; 6] {
    champs(&clip.date).unwrap_or_default()
}

fn comparer_noms(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b))
}

pub fn trier(clips: &[Clip], jeu: Option<&str>) -> Vec<Clip> {
    let mut liste: Vec<Clip> = clips
        .iter()
        .filter(|c| jeu.is_none_or(|j| jeu_de(c) == j))
        .cloned()
        .collect();
    liste.sort_by(|a, b| {
        instant(b)
            .cmp(&instant(a))
            .then_with(|| comparer_noms(&a.nom, &b.nom))
    });
    liste
}

pub fn jeux_de(clips: &[Clip]) -> Vec<JeuClips> {
    let mut jeux: Vec<JeuClips> = Vec::new();
    for clip in clips {
        let jeu = jeu_de(clip);
        match jeux.iter_mut().find(|j| j.jeu == jeu) {
            Some(j) => j.nombre += 1,
            None => jeux.push(JeuClips { jeu, nombre: 1 }),
        }
    }
    jeux.sort_by(|a, b| {
        (a.jeu == SANS_JEU)
            .cmp(&(b.jeu == SANS_JEU))
            .then_with(|| comparer_noms(&a.jeu, &b.jeu))
    });
    jeux
}

pub fn date_clip(iso: &str) -> String {
    let Some([annee, mois, jour, heure, minute, _]) = champs(iso) else {
        return String::new();
    };
    let Some(nom) = usize::try_from(mois - 1).ok().and_then(|m| MOIS.get(m)) else {
        return String::new();
    };
    format!("{jour} {nom} {annee} à {heure:02}:{minute:02}")
}

pub fn details_clip(clip: &Clip) -> String {
    let mut morceaux = vec![taille(clip.taille_octets)];
    if clip.hauteur > 0 {
        morceaux.push(format!("{}p", clip.hauteur));
    }
    if clip.ips > 0 {
        morceaux.push(format!("{} i/s", clip.ips));
    }
    if clip.codec == Some(Codec::Hevc) {
        morceaux.push("HEVC".into());
    }
    if clip.pistes_audio > 1 {
        morceaux.push("piste micro".into());
    }
    morceaux.join(" · ")
}

pub fn part_utilisee(s: &Stockage) -> u32 {
    if s.quota_octets == 0 {
        return 0;
    }
    ((s.utilise_octets as f64 / s.quota_octets as f64) * 100.0)
        .round()
        .min(100.0) as u32
}

pub fn presque_plein(s: &Stockage) -> bool {
    !s.depasse && part_utilisee(s) >= PRESQUE_PLEIN
}

pub fn taille_liberee(clips: &[Clip], chemins: &[std::path::PathBuf]) -> u64 {
    clips
        .iter()
        .filter(|c| chemins.contains(&c.chemin))
        .map(|c| c.taille_octets)
        .sum()
}

pub fn decision_envoi(clip: &Clip, limite: u64) -> Envoi {
    if clip.taille_octets > limite {
        let debit =
            limite as f64 * 8.0 / 1000.0 / clip.duree_s.max(1.0) * 0.95 - f64::from(AUDIO_KBPS);
        return if clip.duree_s > 0.0 && debit < DEBIT_MIN_KBPS {
            Envoi::Refus
        } else {
            Envoi::Leger
        };
    }
    if clip.codec == Some(Codec::H264) && clip.pistes_audio <= 1 {
        Envoi::Direct
    } else {
        Envoi::H264
    }
}

pub fn plan_envoi(
    annexe: Option<&Annexe>,
    taille_octets: u64,
    duree_lue: impl FnOnce() -> f64,
    taille_max: Option<u64>,
) -> Result<PlanEnvoi, String> {
    let pistes = annexe.map_or(1, |a| a.pistes_audio);
    let tient = taille_max.is_none_or(|m| taille_octets <= m);
    let copie = tient && annexe.is_some_and(|a| a.codec == Codec::H264);
    if copie && pistes <= 1 {
        return Ok(PlanEnvoi::Original);
    }
    let duree_s = annexe
        .map(|a| a.duree_s)
        .filter(|d| *d > 0.0)
        .unwrap_or_else(duree_lue);
    let actuel = debit_kbps(taille_octets, duree_s);
    let kbps = match taille_max {
        Some(m) => debit_pour_taille(m, duree_s, AUDIO_KBPS)
            .ok_or("clip trop long pour tenir dans la limite d'envoi")?
            .min(actuel),
        None => actuel,
    };
    Ok(PlanEnvoi::Convertir {
        copie,
        kbps,
        hauteur: annexe
            .filter(|_| !copie)
            .and_then(|a| hauteur_pour_debit(kbps, a.hauteur)),
        pistes,
        duree_s,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn clip(nom: &str, jeu: Option<&str>, date: &str) -> Clip {
        Clip {
            chemin: PathBuf::from(format!("/c/{nom}.mp4")),
            nom: nom.into(),
            jeu: jeu.map(String::from),
            date: date.into(),
            duree_s: 30.0,
            taille_octets: 10 * 1024 * 1024,
            vignette: None,
            codec: Some(Codec::H264),
            largeur: 1920,
            hauteur: 1080,
            ips: 60,
            pistes_audio: 1,
            origine: None,
        }
    }

    fn annexe(codec: Codec, pistes: u32) -> Annexe {
        Annexe {
            jeu: "Jeu".into(),
            date: "2026-01-01T10:00:00".into(),
            duree_s: 120.0,
            codec,
            largeur: 2560,
            hauteur: 1440,
            ips: 60,
            pistes_audio: pistes,
            origine: None,
        }
    }

    #[test]
    fn tri_par_date_puis_par_nom_et_filtre_par_jeu() {
        let clips = [
            clip("b", Some("Valorant"), "2026-03-07T09:05:02"),
            clip("a", Some("Valorant"), "2026-03-07T09:05:02"),
            clip("c", Some("CS2"), "2026-03-08T10:00:00"),
            clip("d", None, "2026-01-01T00:00:00Z"),
            clip("e", Some("  "), "illisible"),
        ];
        let noms = |v: Vec<Clip>| v.into_iter().map(|c| c.nom).collect::<Vec<_>>();
        assert_eq!(noms(trier(&clips, None)), ["c", "a", "b", "d", "e"]);
        assert_eq!(noms(trier(&clips, Some("Valorant"))), ["a", "b"]);
        assert_eq!(noms(trier(&clips, Some(SANS_JEU))), ["d", "e"]);
        assert_eq!(
            jeux_de(&clips),
            [
                JeuClips {
                    jeu: "CS2".into(),
                    nombre: 1
                },
                JeuClips {
                    jeu: "Valorant".into(),
                    nombre: 2
                },
                JeuClips {
                    jeu: SANS_JEU.into(),
                    nombre: 2
                },
            ]
        );
    }

    #[test]
    fn dates_details_et_stockage() {
        assert_eq!(date_clip("2026-03-07T09:05:02"), "7 mars 2026 à 09:05");
        assert_eq!(
            date_clip("2026-12-25T23:59:00Z"),
            "25 décembre 2026 à 23:59"
        );
        assert_eq!(date_clip("n'importe quoi"), "");
        assert_eq!(date_clip("2026-13-01T00:00:00"), "");
        let mut c = clip("a", None, "");
        assert_eq!(details_clip(&c), "10 Mo · 1080p · 60 i/s");
        c.codec = Some(Codec::Hevc);
        c.pistes_audio = 2;
        assert_eq!(
            details_clip(&c),
            "10 Mo · 1080p · 60 i/s · HEVC · piste micro"
        );
        let s = |utilise, quota, depasse| Stockage {
            utilise_octets: utilise,
            quota_octets: quota,
            depasse,
            plus_anciens: Vec::new(),
        };
        assert_eq!(part_utilisee(&s(5, 10, false)), 50);
        assert_eq!(part_utilisee(&s(15, 10, true)), 100);
        assert_eq!(part_utilisee(&s(1, 0, true)), 0);
        assert!(presque_plein(&s(92, 100, false)));
        assert!(!presque_plein(&s(89, 100, false)));
        assert!(!presque_plein(&s(100, 100, true)));
        let clips = [clip("a", None, ""), clip("b", None, "")];
        assert_eq!(
            taille_liberee(&clips, &[PathBuf::from("/c/b.mp4")]),
            10 * 1024 * 1024
        );
    }

    #[test]
    fn decision_d_envoi_comme_l_app_tauri() {
        let limite = 100 * 1024 * 1024;
        let mut c = clip("a", None, "");
        assert_eq!(decision_envoi(&c, limite), Envoi::Direct);
        c.pistes_audio = 2;
        assert_eq!(decision_envoi(&c, limite), Envoi::H264);
        c.pistes_audio = 1;
        c.codec = None;
        assert_eq!(decision_envoi(&c, limite), Envoi::H264);
        c.taille_octets = limite + 1;
        assert_eq!(decision_envoi(&c, limite), Envoi::Leger);
        c.duree_s = 4000.0;
        assert_eq!(decision_envoi(&c, limite), Envoi::Refus);
        c.duree_s = 0.0;
        assert_eq!(decision_envoi(&c, limite), Envoi::Leger);
    }

    #[test]
    fn plan_de_conversion_avant_l_envoi() {
        let h264 = annexe(Codec::H264, 1);
        assert_eq!(
            plan_envoi(Some(&h264), 50_000_000, || 0.0, None),
            Ok(PlanEnvoi::Original)
        );
        let micro = annexe(Codec::H264, 2);
        assert_eq!(
            plan_envoi(Some(&micro), 50_000_000, || 0.0, None),
            Ok(PlanEnvoi::Convertir {
                copie: true,
                kbps: debit_kbps(50_000_000, 120.0),
                hauteur: None,
                pistes: 2,
                duree_s: 120.0,
            })
        );
        let hevc = annexe(Codec::Hevc, 1);
        let Ok(PlanEnvoi::Convertir {
            copie,
            kbps,
            hauteur,
            ..
        }) = plan_envoi(Some(&hevc), 200_000_000, || 0.0, Some(25_000_000))
        else {
            panic!("conversion attendue");
        };
        assert!(!copie && kbps == 1455 && hauteur == Some(720));
        assert_eq!(
            plan_envoi(Some(&hevc), 200_000_000, || 0.0, Some(1_000_000)),
            Err("clip trop long pour tenir dans la limite d'envoi".into())
        );
        let Ok(PlanEnvoi::Convertir {
            duree_s, pistes, ..
        }) = plan_envoi(None, 10_000_000, || 42.0, None)
        else {
            panic!("conversion attendue");
        };
        assert_eq!((duree_s, pistes), (42.0, 1));
    }
}
