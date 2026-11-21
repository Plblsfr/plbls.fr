use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{Resultat, SystemeAudio};
use serde::{Deserialize, Serialize};

use super::format::{SourceLecture, FREQUENCE};
use super::peripheriques::Lecture;

pub const PAS_S: f32 = 0.1;
pub const QUEUE_S: f32 = 0.02;
pub const NIVEAU_PLEIN: f32 = 0.2;
pub const VOLUME_PAR_DEFAUT: u16 = 60;
pub const VOLUME_MAXIMAL: u16 = 100;
pub const NIVEAU: f32 = NIVEAU_PLEIN * VOLUME_PAR_DEFAUT as f32 / 100.0;
const PLANCHER: f32 = 0.0001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonVocal {
    Arrivee,
    Depart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChoixSon {
    #[serde(rename = "off")]
    Aucun,
    #[serde(rename = "rise")]
    Montee,
    #[serde(rename = "fall")]
    Descente,
    #[serde(rename = "chime")]
    Carillon,
    #[serde(rename = "pop")]
    Pop,
    #[serde(rename = "double")]
    DoubleBip,
}

impl ChoixSon {
    pub const PROPOSES: [ChoixSon; 6] = [
        Self::Montee,
        Self::Descente,
        Self::Carillon,
        Self::Pop,
        Self::DoubleBip,
        Self::Aucun,
    ];

    pub fn notes(self) -> &'static [f32] {
        match self {
            Self::Aucun => &[],
            Self::Montee => &[660.0, 880.0],
            Self::Descente => &[880.0, 660.0],
            Self::Carillon => &[784.0, 988.0, 1319.0],
            Self::Pop => &[520.0],
            Self::DoubleBip => &[700.0, 700.0],
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Aucun => "Aucun",
            Self::Montee => "Montée",
            Self::Descente => "Descente",
            Self::Carillon => "Carillon",
            Self::Pop => "Pop",
            Self::DoubleBip => "Double bip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesSons {
    pub arrivee: ChoixSon,
    pub depart: ChoixSon,
    pub volume: u16,
}

impl Default for ReglagesSons {
    fn default() -> Self {
        Self {
            arrivee: ChoixSon::Montee,
            depart: ChoixSon::Descente,
            volume: VOLUME_PAR_DEFAUT,
        }
    }
}

impl ReglagesSons {
    pub fn borne(self) -> Self {
        Self {
            volume: self.volume.min(VOLUME_MAXIMAL),
            ..self
        }
    }

    pub fn choix(&self, son: SonVocal) -> ChoixSon {
        match son {
            SonVocal::Arrivee => self.arrivee,
            SonVocal::Depart => self.depart,
        }
    }

    pub fn niveau(&self) -> f32 {
        NIVEAU_PLEIN * f32::from(self.volume.min(VOLUME_MAXIMAL)) / 100.0
    }
}

pub struct Tonalite {
    echantillons: Vec<f32>,
    position: usize,
}

impl Tonalite {
    pub fn nouvelle(notes: &[f32], niveau: f32) -> Self {
        let frequence = FREQUENCE as f32;
        let duree = if notes.is_empty() {
            0.0
        } else {
            notes.len() as f32 * PAS_S + QUEUE_S
        };
        let mut echantillons = vec![0.0f32; (duree * frequence).ceil() as usize];
        let niveau = niveau.max(PLANCHER);
        let decroissance = (PLANCHER / niveau).ln() / PAS_S;
        for (i, note) in notes.iter().enumerate() {
            let debut = (i as f32 * PAS_S * frequence) as usize;
            let longueur = ((PAS_S + QUEUE_S) * frequence) as usize;
            for n in 0..longueur {
                let Some(e) = echantillons.get_mut(debut + n) else {
                    break;
                };
                let t = n as f32 / frequence;
                let gain = niveau * (decroissance * t.min(PAS_S)).exp();
                *e += gain * (2.0 * std::f32::consts::PI * note * t).sin();
            }
        }
        Self {
            echantillons,
            position: 0,
        }
    }

    pub fn duree(&self) -> Duration {
        Duration::from_secs_f32(self.echantillons.len() as f32 / FREQUENCE as f32)
    }

    pub fn echantillons(&self) -> &[f32] {
        &self.echantillons
    }

    pub fn terminee(&self) -> bool {
        self.position >= self.echantillons.len()
    }
}

impl SourceLecture for Tonalite {
    fn lire(&mut self, sortie: &mut [f32]) {
        for e in sortie.iter_mut() {
            *e = self.echantillons.get(self.position).copied().unwrap_or(0.0);
            self.position = self.position.saturating_add(1);
        }
    }
}

pub fn jouer(
    systeme: Arc<dyn SystemeAudio>,
    sortie: Option<String>,
    choix: ChoixSon,
    volume: u16,
) -> Resultat<Option<(Lecture, Duration)>> {
    let reglages = ReglagesSons {
        volume,
        ..ReglagesSons::default()
    };
    if choix == ChoixSon::Aucun || reglages.niveau() <= 0.0 {
        return Ok(None);
    }
    let tonalite = Tonalite::nouvelle(choix.notes(), reglages.niveau());
    let duree = tonalite.duree();
    let source: Arc<Mutex<dyn SourceLecture>> = Arc::new(Mutex::new(tonalite));
    Lecture::ouvrir(systeme, sortie, source).map(|lecture| Some((lecture, duree)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::factice::SystemeFactice;
    use crate::audio::{FormatAudio, SensAudio};

    #[test]
    fn deux_notes_comme_le_front_qui_s_eteignent() {
        let t = Tonalite::nouvelle(ChoixSon::Montee.notes(), NIVEAU);
        assert_eq!(
            t.echantillons().len(),
            (0.22 * FREQUENCE as f32).ceil() as usize
        );
        let crete = t.echantillons().iter().fold(0.0f32, |m, e| m.max(e.abs()));
        assert!(
            crete > NIVEAU * 0.9 && crete <= NIVEAU * 1.01,
            "crête {crete}"
        );
        let fin = &t.echantillons()[t.echantillons().len() - 48..];
        assert!(fin.iter().all(|e| e.abs() < 0.001), "la note s'éteint");
        assert_eq!(ChoixSon::Descente.notes(), [880.0, 660.0]);
    }

    #[test]
    fn joue_sur_la_sortie_choisie_puis_se_tait() {
        let s = SystemeFactice::nouveau();
        s.brancher(
            SensAudio::Sortie,
            "hp",
            "Haut-parleurs",
            FormatAudio {
                frequence: 48_000,
                canaux: 2,
            },
        );
        let (lecture, duree) = jouer(
            Arc::new(s.clone()),
            Some("hp".into()),
            ChoixSon::Descente,
            VOLUME_PAR_DEFAUT,
        )
        .unwrap()
        .unwrap();
        assert!(duree >= Duration::from_millis(200));
        let joue = s.tirer("hp", 4_800).unwrap();
        assert!(joue.iter().any(|e| e.abs() > 0.05));
        let _ = s.tirer("hp", 48_000).unwrap();
        let apres = s.tirer("hp", 960).unwrap();
        assert!(apres.iter().all(|e| *e == 0.0));
        drop(lecture);
        assert!(s.flux_ouverts().is_empty());
    }

    #[test]
    fn les_sons_proposes_sont_ceux_du_front_et_aucun_ne_joue_rien() {
        let ids: Vec<String> = ChoixSon::PROPOSES
            .iter()
            .map(|c| {
                serde_json::to_value(c)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(ids, ["rise", "fall", "chime", "pop", "double", "off"]);
        assert_eq!(
            Tonalite::nouvelle(ChoixSon::Carillon.notes(), NIVEAU)
                .echantillons()
                .len(),
            ((3.0 * PAS_S + QUEUE_S) * FREQUENCE as f32).ceil() as usize
        );
        let s = SystemeFactice::nouveau();
        assert!(jouer(Arc::new(s.clone()), None, ChoixSon::Aucun, 60)
            .unwrap()
            .is_none());
        assert!(jouer(Arc::new(s.clone()), None, ChoixSon::Pop, 0)
            .unwrap()
            .is_none());
        assert!(s.flux_ouverts().is_empty());
    }

    #[test]
    fn le_volume_des_sons_regle_la_crete_et_se_borne() {
        let crete = |volume: u16| {
            let niveau = ReglagesSons {
                volume,
                ..ReglagesSons::default()
            }
            .niveau();
            Tonalite::nouvelle(ChoixSon::Pop.notes(), niveau)
                .echantillons()
                .iter()
                .fold(0.0f32, |m, e| m.max(e.abs()))
        };
        assert!((crete(100) / crete(50) - 2.0).abs() < 0.02);
        let lu: ReglagesSons =
            serde_json::from_str(r#"{"arrivee": "chime", "volume": 400}"#).unwrap();
        assert_eq!(
            lu.borne(),
            ReglagesSons {
                arrivee: ChoixSon::Carillon,
                depart: ChoixSon::Descente,
                volume: 100,
            }
        );
        assert_eq!(lu.choix(SonVocal::Arrivee), ChoixSon::Carillon);
    }
}
