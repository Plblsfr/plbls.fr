use estompe_plateforme::FormatAudio;

use super::micro::FREQUENCE;
use crate::audio::format::{vers_i16, Reechantillonneur};

pub struct Conversion {
    canaux: usize,
    frequence: u32,
    reechantillonneurs: Vec<Reechantillonneur>,
}

impl Conversion {
    pub fn nouvelle(canaux: usize) -> Self {
        let canaux = canaux.max(1);
        Self {
            canaux,
            frequence: FREQUENCE,
            reechantillonneurs: vec![Reechantillonneur::nouveau(FREQUENCE, FREQUENCE); canaux],
        }
    }

    pub fn canaux(&self) -> usize {
        self.canaux
    }

    pub fn convertir(&mut self, entrelace: &[f32], format: FormatAudio) -> Vec<f32> {
        let source = usize::from(format.canaux.max(1));
        if format.frequence != self.frequence {
            self.frequence = format.frequence;
            self.reechantillonneurs =
                vec![Reechantillonneur::nouveau(format.frequence, FREQUENCE); self.canaux];
        }
        let pistes: Vec<Vec<f32>> = (0..self.canaux)
            .map(|c| {
                let brut: Vec<f32> = entrelace
                    .chunks_exact(source)
                    .map(|trame| trame[c.min(source - 1)])
                    .collect();
                if self.frequence == FREQUENCE {
                    brut
                } else {
                    let mut sortie = Vec::with_capacity(brut.len() * 2);
                    self.reechantillonneurs[c].traiter(&brut, &mut sortie);
                    sortie
                }
            })
            .collect();
        let longueur = pistes.iter().map(Vec::len).min().unwrap_or(0);
        (0..longueur)
            .flat_map(|i| pistes.iter().map(move |p| p[i]))
            .collect()
    }
}

pub fn avec_silence(silence: u64, canaux: usize, pcm: &[f32]) -> Vec<i16> {
    let mut sortie = vec![0i16; silence as usize * canaux];
    sortie.extend(pcm.iter().map(|e| vers_i16(*e)));
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn met_en_stereo_a_48_khz() {
        let mut c = Conversion::nouvelle(2);
        let stereo = FormatAudio {
            frequence: 48_000,
            canaux: 2,
        };
        assert_eq!(
            c.convertir(&[0.1, 0.2, 0.3, 0.4], stereo),
            [0.1, 0.2, 0.3, 0.4]
        );
        let mono = FormatAudio {
            frequence: 48_000,
            canaux: 1,
        };
        assert_eq!(c.convertir(&[0.5, -0.5], mono), [0.5, 0.5, -0.5, -0.5]);
        let six = FormatAudio {
            frequence: 48_000,
            canaux: 6,
        };
        assert_eq!(
            c.convertir(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], six),
            [1.0, 2.0]
        );
        let lent = FormatAudio {
            frequence: 24_000,
            canaux: 2,
        };
        let sortie = c.convertir(&vec![0.25; 2 * 2400], lent);
        assert!(
            (sortie.len() as i64 - 2 * 4800).abs() <= 4,
            "{}",
            sortie.len()
        );
        assert!(sortie.iter().all(|e| (*e - 0.25).abs() < 1e-6));
        assert_eq!(c.canaux(), 2);
    }

    #[test]
    fn le_silence_precede_le_son() {
        assert_eq!(
            avec_silence(2, 2, &[1.0, -1.0]),
            [0, 0, 0, 0, 32767, -32767]
        );
        assert!(avec_silence(0, 1, &[]).is_empty());
    }
}
