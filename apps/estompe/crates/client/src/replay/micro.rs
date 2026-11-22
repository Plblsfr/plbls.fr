use nnnoiseless::DenoiseState;

pub const FREQUENCE: u32 = 48_000;
const TRAME: usize = DenoiseState::FRAME_SIZE;
const PLEINE_ECHELLE: f32 = 32_767.0;

pub struct Traitement {
    debruiteur: Option<Box<DenoiseState<'static>>>,
    gain: f32,
    attente: Vec<f32>,
}

impl Traitement {
    pub fn nouveau(debruiter: bool, gain: f32) -> Self {
        Self {
            debruiteur: debruiter.then(DenoiseState::new),
            gain: if gain.is_finite() {
                gain.clamp(0.0, 3.0)
            } else {
                1.0
            },
            attente: Vec::new(),
        }
    }

    pub fn traiter(&mut self, entree: &[f32]) -> Vec<i16> {
        let fixer = |v: f32, gain: f32| (v * gain).clamp(-PLEINE_ECHELLE, PLEINE_ECHELLE) as i16;
        let Some(debruiteur) = self.debruiteur.as_mut() else {
            return entree
                .iter()
                .map(|v| fixer(v * PLEINE_ECHELLE, self.gain))
                .collect();
        };
        self.attente
            .extend(entree.iter().map(|v| v * PLEINE_ECHELLE));
        let mut sortie = Vec::with_capacity(self.attente.len());
        let mut trame = [0f32; TRAME];
        let (blocs, _) = self.attente.as_chunks::<TRAME>();
        let complet = blocs.len() * TRAME;
        for bloc in blocs {
            debruiteur.process_frame(&mut trame, bloc);
            sortie.extend(trame.iter().map(|v| fixer(*v, self.gain)));
        }
        self.attente.drain(..complet);
        sortie
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_borne_sans_reduction() {
        let mut t = Traitement::nouveau(false, 2.0);
        assert_eq!(
            t.traiter(&[0.0, 0.25, -0.25, 0.9, -1.0]),
            [0, 16383, -16383, 32767, -32767]
        );
        assert_eq!(Traitement::nouveau(false, 0.0).traiter(&[0.5; 7]), [0; 7]);
        assert_eq!(
            Traitement::nouveau(false, f32::NAN).traiter(&[0.5]),
            [16383]
        );
    }

    #[test]
    fn reduction_par_trames_entieres() {
        let mut t = Traitement::nouveau(true, 1.0);
        assert!(t.traiter(&[0.0; 300]).is_empty());
        assert_eq!(t.traiter(&[0.0; 300]).len(), 480);
        assert_eq!(t.traiter(&[0.0; 1320]).len(), 1440);
        let mut graine = 0x2545_F491u32;
        let mut bruit = Vec::with_capacity(96_000);
        for _ in 0..96_000 {
            graine ^= graine << 13;
            graine ^= graine >> 17;
            graine ^= graine << 5;
            bruit.push((graine as f32 / u32::MAX as f32 - 0.5) * 0.2);
        }
        let energie = |s: &[i16]| s[48_000..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
        let brut = energie(&Traitement::nouveau(false, 1.0).traiter(&bruit));
        let filtre = energie(&Traitement::nouveau(true, 1.0).traiter(&bruit));
        assert!(filtre < brut * 0.7, "{filtre} {brut}");
    }
}
