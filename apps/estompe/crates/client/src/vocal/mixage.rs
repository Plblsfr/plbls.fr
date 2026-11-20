use std::collections::BTreeMap;

use crate::audio::{SourceLecture, TamponLecture};

use super::salle::SourceAudio;

pub const GAIN_MAXIMAL: f32 = 3.0;

#[derive(Debug, Default)]
pub struct Mixeur {
    pistes: BTreeMap<(String, SourceAudio), TamponLecture>,
    gains: BTreeMap<(String, SourceAudio), f32>,
    muet: bool,
}

impl Mixeur {
    pub fn ajouter(&mut self, identite: &str, source: SourceAudio) {
        self.pistes
            .entry((identite.to_string(), source))
            .or_default()
            .vider();
    }

    pub fn retirer(&mut self, identite: &str, source: SourceAudio) {
        self.pistes.remove(&(identite.to_string(), source));
    }

    pub fn vider(&mut self) {
        self.pistes.clear();
    }

    pub fn pousser(&mut self, identite: &str, source: SourceAudio, trame: &[i16]) {
        if let Some(piste) = self.pistes.get_mut(&(identite.to_string(), source)) {
            piste.pousser(trame);
        }
    }

    pub fn definir_gain(&mut self, identite: &str, source: SourceAudio, gain: f32) {
        self.gains.insert(
            (identite.to_string(), source),
            gain.clamp(0.0, GAIN_MAXIMAL),
        );
    }

    pub fn gain(&self, identite: &str, source: SourceAudio) -> f32 {
        self.gains
            .get(&(identite.to_string(), source))
            .copied()
            .unwrap_or(1.0)
    }

    pub fn definir_muet(&mut self, muet: bool) {
        self.muet = muet;
    }

    pub fn pistes(&self) -> Vec<(String, SourceAudio)> {
        self.pistes.keys().cloned().collect()
    }
}

impl SourceLecture for Mixeur {
    fn lire(&mut self, sortie: &mut [f32]) {
        sortie.fill(0.0);
        for ((identite, source), piste) in &mut self.pistes {
            let gain = if self.muet {
                0.0
            } else {
                self.gains
                    .get(&(identite.clone(), *source))
                    .copied()
                    .unwrap_or(1.0)
            };
            piste.ajouter_a(sortie, gain);
        }
        for echantillon in sortie.iter_mut() {
            *echantillon = echantillon.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::ECHANTILLONS_PAR_TRAME;

    fn lire(m: &mut Mixeur) -> Vec<f32> {
        let mut sortie = vec![0.0; ECHANTILLONS_PAR_TRAME];
        m.lire(&mut sortie);
        sortie
    }

    #[test]
    fn mixe_les_participants_avec_leur_gain_et_coupe_tout_en_sourdine() {
        let mut m = Mixeur::default();
        m.ajouter("a", SourceAudio::Micro);
        m.ajouter("b", SourceAudio::Micro);
        m.definir_gain("b", SourceAudio::Micro, 2.0);
        for _ in 0..2 {
            m.pousser("a", SourceAudio::Micro, &[3_277; ECHANTILLONS_PAR_TRAME]);
            m.pousser("b", SourceAudio::Micro, &[3_277; ECHANTILLONS_PAR_TRAME]);
            m.pousser(
                "inconnu",
                SourceAudio::Micro,
                &[i16::MAX; ECHANTILLONS_PAR_TRAME],
            );
        }
        assert!((lire(&mut m)[0] - 0.3).abs() < 1e-3);
        m.definir_muet(true);
        assert!(lire(&mut m).iter().all(|e| *e == 0.0));
        m.definir_gain("a", SourceAudio::Micro, 10.0);
        assert_eq!(m.gain("a", SourceAudio::Micro), GAIN_MAXIMAL);
        m.retirer("a", SourceAudio::Micro);
        assert_eq!(m.pistes(), vec![("b".to_string(), SourceAudio::Micro)]);
    }

    #[test]
    fn le_mixage_ecrete_au_lieu_de_deborder() {
        let mut m = Mixeur::default();
        m.ajouter("a", SourceAudio::Micro);
        m.definir_gain("a", SourceAudio::Micro, 3.0);
        for _ in 0..2 {
            m.pousser("a", SourceAudio::Micro, &[i16::MAX; ECHANTILLONS_PAR_TRAME]);
        }
        assert!(lire(&mut m).iter().all(|e| *e == 1.0));
    }
}
