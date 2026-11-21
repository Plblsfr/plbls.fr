use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{Resultat, SystemeAudio};

use super::format::{SourceLecture, FREQUENCE};
use super::peripheriques::Lecture;

pub const PAS_S: f32 = 0.1;
pub const QUEUE_S: f32 = 0.02;
pub const NIVEAU: f32 = 0.2 * 0.6;
const PLANCHER: f32 = 0.0001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonVocal {
    Arrivee,
    Depart,
}

impl SonVocal {
    pub fn notes(self) -> &'static [f32] {
        match self {
            Self::Arrivee => &[660.0, 880.0],
            Self::Depart => &[880.0, 660.0],
        }
    }
}

pub struct Tonalite {
    echantillons: Vec<f32>,
    position: usize,
}

impl Tonalite {
    pub fn nouvelle(son: SonVocal) -> Self {
        let frequence = FREQUENCE as f32;
        let notes = son.notes();
        let duree = (notes.len() as f32 - 1.0) * PAS_S + PAS_S + QUEUE_S;
        let mut echantillons = vec![0.0f32; (duree * frequence).ceil() as usize];
        let decroissance = (PLANCHER / NIVEAU).ln() / PAS_S;
        for (i, note) in notes.iter().enumerate() {
            let debut = (i as f32 * PAS_S * frequence) as usize;
            let longueur = ((PAS_S + QUEUE_S) * frequence) as usize;
            for n in 0..longueur {
                let Some(e) = echantillons.get_mut(debut + n) else {
                    break;
                };
                let t = n as f32 / frequence;
                let gain = NIVEAU * (decroissance * t.min(PAS_S)).exp();
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
    son: SonVocal,
) -> Resultat<(Lecture, Duration)> {
    let tonalite = Tonalite::nouvelle(son);
    let duree = tonalite.duree();
    let source: Arc<Mutex<dyn SourceLecture>> = Arc::new(Mutex::new(tonalite));
    Lecture::ouvrir(systeme, sortie, source).map(|lecture| (lecture, duree))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::factice::SystemeFactice;
    use crate::audio::{FormatAudio, SensAudio};

    #[test]
    fn deux_notes_comme_le_front_qui_s_eteignent() {
        let t = Tonalite::nouvelle(SonVocal::Arrivee);
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
        assert_eq!(SonVocal::Depart.notes(), [880.0, 660.0]);
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
        let (lecture, duree) =
            jouer(Arc::new(s.clone()), Some("hp".into()), SonVocal::Depart).unwrap();
        assert!(duree >= Duration::from_millis(200));
        let joue = s.tirer("hp", 4_800).unwrap();
        assert!(joue.iter().any(|e| e.abs() > 0.05));
        let _ = s.tirer("hp", 48_000).unwrap();
        let apres = s.tirer("hp", 960).unwrap();
        assert!(apres.iter().all(|e| *e == 0.0));
        drop(lecture);
        assert!(s.flux_ouverts().is_empty());
    }
}
