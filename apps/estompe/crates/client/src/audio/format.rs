use std::collections::VecDeque;

use estompe_plateforme::FormatAudio;

pub const FREQUENCE: u32 = 48_000;
pub const ECHANTILLONS_PAR_TRAME: usize = 480;

pub fn vers_i16(echantillon: f32) -> i16 {
    (echantillon.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

pub fn depuis_i16(echantillon: i16) -> f32 {
    echantillon as f32 / i16::MAX as f32
}

pub fn vers_mono(entrelace: &[f32], canaux: u16, sortie: &mut Vec<f32>) {
    let canaux = usize::from(canaux.max(1));
    sortie.extend(
        entrelace
            .chunks_exact(canaux)
            .map(|image| image.iter().sum::<f32>() / canaux as f32),
    );
}

pub fn etaler(mono: &[f32], canaux: u16, sortie: &mut [f32]) {
    let canaux = usize::from(canaux.max(1));
    for (image, echantillon) in sortie.chunks_exact_mut(canaux).zip(mono) {
        image.fill(*echantillon);
    }
}

fn pgcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        pgcd(b, a % b)
    }
}

#[derive(Debug, Clone)]
pub struct Reechantillonneur {
    de: u64,
    vers: u64,
    phase: u64,
    precedent: Option<f32>,
}

impl Reechantillonneur {
    pub fn nouveau(de: u32, vers: u32) -> Self {
        let d = pgcd(de.max(1), vers.max(1));
        Self {
            de: u64::from(de.max(1) / d),
            vers: u64::from(vers.max(1) / d),
            phase: 0,
            precedent: None,
        }
    }

    pub fn traiter(&mut self, entree: &[f32], sortie: &mut Vec<f32>) {
        for &x in entree {
            let Some(precedent) = self.precedent else {
                self.precedent = Some(x);
                continue;
            };
            while self.phase < self.vers {
                let t = self.phase as f32 / self.vers as f32;
                sortie.push(precedent + (x - precedent) * t);
                self.phase += self.de;
            }
            self.phase -= self.vers;
            self.precedent = Some(x);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConversionEntree {
    canaux: u16,
    reechantillonneur: Reechantillonneur,
    mono: Vec<f32>,
    reechantillonne: Vec<f32>,
    trame: Vec<i16>,
}

impl ConversionEntree {
    pub fn nouvelle(format: FormatAudio) -> Self {
        Self {
            canaux: format.canaux,
            reechantillonneur: Reechantillonneur::nouveau(format.frequence, FREQUENCE),
            mono: Vec::new(),
            reechantillonne: Vec::new(),
            trame: Vec::with_capacity(ECHANTILLONS_PAR_TRAME),
        }
    }

    pub fn pousser(&mut self, entrelace: &[f32], mut rendu: impl FnMut(&[i16])) {
        self.mono.clear();
        vers_mono(entrelace, self.canaux, &mut self.mono);
        self.reechantillonne.clear();
        self.reechantillonneur
            .traiter(&self.mono, &mut self.reechantillonne);
        for &echantillon in &self.reechantillonne {
            self.trame.push(vers_i16(echantillon));
            if self.trame.len() == ECHANTILLONS_PAR_TRAME {
                rendu(&self.trame);
                self.trame.clear();
            }
        }
    }
}

pub trait SourceLecture: Send {
    fn lire(&mut self, sortie: &mut [f32]);
}

#[derive(Debug, Clone)]
pub struct ConversionSortie {
    canaux: u16,
    reechantillonneur: Reechantillonneur,
    trame: Vec<f32>,
    reechantillonne: Vec<f32>,
    file: VecDeque<f32>,
    mono: Vec<f32>,
}

impl ConversionSortie {
    pub fn nouvelle(format: FormatAudio) -> Self {
        Self {
            canaux: format.canaux,
            reechantillonneur: Reechantillonneur::nouveau(FREQUENCE, format.frequence),
            trame: vec![0.0; ECHANTILLONS_PAR_TRAME],
            reechantillonne: Vec::new(),
            file: VecDeque::new(),
            mono: Vec::new(),
        }
    }

    pub fn remplir(&mut self, source: &mut dyn SourceLecture, entrelace: &mut [f32]) {
        let canaux = usize::from(self.canaux.max(1));
        let images = entrelace.len() / canaux;
        while self.file.len() < images {
            self.trame.fill(0.0);
            source.lire(&mut self.trame);
            self.reechantillonne.clear();
            self.reechantillonneur
                .traiter(&self.trame, &mut self.reechantillonne);
            self.file.extend(self.reechantillonne.iter().copied());
        }
        self.mono.clear();
        self.mono.extend(self.file.drain(..images));
        etaler(&self.mono, self.canaux, entrelace);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compteurs {
    pub recus: u64,
    pub lus: u64,
    pub perdus: u64,
    pub silences: u64,
}

#[derive(Debug, Clone)]
pub struct TamponLecture {
    file: VecDeque<f32>,
    amorce: usize,
    maximum: usize,
    demarre: bool,
    compteurs: Compteurs,
}

pub const AMORCE_LECTURE: usize = 2 * ECHANTILLONS_PAR_TRAME;
pub const MAXIMUM_LECTURE: usize = 20 * ECHANTILLONS_PAR_TRAME;

impl Default for TamponLecture {
    fn default() -> Self {
        Self::nouveau(AMORCE_LECTURE, MAXIMUM_LECTURE)
    }
}

impl TamponLecture {
    pub fn nouveau(amorce: usize, maximum: usize) -> Self {
        Self {
            file: VecDeque::new(),
            amorce,
            maximum: maximum.max(amorce),
            demarre: false,
            compteurs: Compteurs::default(),
        }
    }

    pub fn pousser(&mut self, trame: &[i16]) {
        self.compteurs.recus += trame.len() as u64;
        self.file.extend(trame.iter().map(|e| depuis_i16(*e)));
        if self.file.len() > self.maximum {
            let surplus = self.file.len() - self.maximum;
            self.file.drain(..surplus);
            self.compteurs.perdus += surplus as u64;
        }
    }

    pub fn longueur(&self) -> usize {
        self.file.len()
    }

    pub fn compteurs(&self) -> &Compteurs {
        &self.compteurs
    }

    pub fn vider(&mut self) {
        self.file.clear();
        self.demarre = false;
    }

    pub fn ajouter_a(&mut self, sortie: &mut [f32], gain: f32) {
        if !self.demarre && self.file.len() >= self.amorce {
            self.demarre = true;
        }
        if !self.demarre {
            self.compteurs.silences += sortie.len() as u64;
            return;
        }
        let disponibles = self.file.len().min(sortie.len());
        for (s, e) in sortie.iter_mut().zip(self.file.drain(..disponibles)) {
            *s += e * gain;
        }
        self.compteurs.lus += disponibles as u64;
        if disponibles < sortie.len() {
            self.compteurs.silences += (sortie.len() - disponibles) as u64;
            self.demarre = false;
        }
    }
}

impl SourceLecture for TamponLecture {
    fn lire(&mut self, sortie: &mut [f32]) {
        sortie.fill(0.0);
        self.ajouter_a(sortie, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convertit_en_mono_et_etale_sur_les_canaux() {
        let mut mono = Vec::new();
        vers_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, -1.0], 2, &mut mono);
        assert_eq!(mono, vec![0.5, 0.5, -1.0]);
        let mut stereo = vec![0.0; 6];
        etaler(&mono, 2, &mut stereo);
        assert_eq!(stereo, vec![0.5, 0.5, 0.5, 0.5, -1.0, -1.0]);
        assert_eq!(vers_i16(2.0), i16::MAX);
        assert_eq!(vers_i16(-2.0), -i16::MAX);
        assert_eq!(depuis_i16(i16::MAX), 1.0);
    }

    #[test]
    fn le_reechantillonnage_garde_le_nombre_d_echantillons_sans_derive() {
        for (de, vers) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (16_000, 48_000),
            (48_000, 48_000),
        ] {
            let mut r = Reechantillonneur::nouveau(de, vers);
            let mut sortie = Vec::new();
            let morceau = vec![0.25; (de / 100) as usize];
            for _ in 0..6_000 {
                r.traiter(&morceau, &mut sortie);
            }
            let attendu = u64::from(vers) * 60;
            assert!(
                (sortie.len() as u64).abs_diff(attendu) <= u64::from(vers / de) + 1,
                "{de} → {vers} : {} au lieu de {attendu}",
                sortie.len()
            );
            assert!(sortie.iter().all(|e| (*e - 0.25).abs() < 1e-6));
        }
    }

    #[test]
    fn le_reechantillonnage_suit_une_sinusoide() {
        let mut r = Reechantillonneur::nouveau(44_100, 48_000);
        let entree: Vec<f32> = (0..44_100)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin())
            .collect();
        let mut sortie = Vec::new();
        r.traiter(&entree, &mut sortie);
        for (i, e) in sortie.iter().enumerate() {
            let t = i as f32 / 48_000.0;
            let attendu = (t * 440.0 * std::f32::consts::TAU).sin();
            assert!((e - attendu).abs() < 0.01, "{i} : {e} au lieu de {attendu}");
        }
    }

    #[test]
    fn decoupe_l_entree_en_trames_de_10_ms_a_48_khz_mono() {
        let mut conversion = ConversionEntree::nouvelle(FormatAudio {
            frequence: 48_000,
            canaux: 2,
        });
        let mut trames = Vec::new();
        conversion.pousser(&vec![0.5; 2 * 1_000], |t| trames.push(t.to_vec()));
        assert_eq!(trames.len(), 2);
        assert!(trames.iter().all(|t| t.len() == ECHANTILLONS_PAR_TRAME));
        assert_eq!(trames[1][0], vers_i16(0.5));
    }

    #[test]
    fn le_tampon_s_amorce_borne_sa_latence_et_compte_les_pertes() {
        let mut tampon = TamponLecture::nouveau(960, 1_440);
        let mut sortie = vec![0.0; 480];
        tampon.pousser(&[100; 480]);
        tampon.lire(&mut sortie);
        assert!(sortie.iter().all(|e| *e == 0.0));
        tampon.pousser(&[100; 480]);
        tampon.lire(&mut sortie);
        assert!(sortie.iter().all(|e| *e > 0.0));
        for _ in 0..4 {
            tampon.pousser(&[100; 480]);
        }
        assert_eq!(tampon.longueur(), 1_440);
        assert_eq!(tampon.compteurs().perdus, 960);
        assert_eq!(tampon.compteurs().silences, 480);
    }
}
