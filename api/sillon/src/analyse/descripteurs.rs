use std::f32::consts::PI;

pub const FREQUENCE: usize = 22_050;
pub const FENETRE: usize = 2048;
pub const PAS: usize = 512;
pub const CLASSES: usize = 12;
pub const COEFS: usize = 12;
pub const DIMENSION: usize = 7 + CLASSES + 2 * COEFS;
const BANDES: usize = 26;
const SILENCE: f32 = 1e-4;
const PLANCHER: f32 = 1e-8;
const TEMPO_MIN: f32 = 50.0;
const TEMPO_MAX: f32 = 220.0;
const TEMPO_CENTRE: f32 = 120.0;
const PULSATION_MIN: f32 = 0.1;
const ATTAQUE_MIN: f32 = 0.05;
const PLANCHER_ATTAQUES: f32 = 1e-5;
const PORTE: f32 = 20.0;
const SEUIL_PASSAGES: f32 = 0.1;
const CHROMA_MIN: f32 = 130.0;
const CHROMA_MAX: f32 = 4000.0;
const DO: f32 = 261.625_58;
const MAJEUR: [f32; CLASSES] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINEUR: [f32; CLASSES] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

pub struct Fft {
    inverses: Vec<usize>,
    cos: Vec<f32>,
    sin: Vec<f32>,
}

impl Fft {
    pub fn nouveau(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 2);
        let decalage = usize::BITS - n.trailing_zeros();
        let angle = |k: usize| -2.0 * PI * k as f32 / n as f32;
        Self {
            inverses: (0..n).map(|i| i.reverse_bits() >> decalage).collect(),
            cos: (0..n / 2).map(|k| angle(k).cos()).collect(),
            sin: (0..n / 2).map(|k| angle(k).sin()).collect(),
        }
    }

    pub fn transformer(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.inverses.len();
        for (i, &j) in self.inverses.iter().enumerate() {
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut taille = 2;
        while taille <= n {
            let (demi, saut) = (taille / 2, n / taille);
            for debut in (0..n).step_by(taille) {
                for k in 0..demi {
                    let (c, s) = (self.cos[k * saut], self.sin[k * saut]);
                    let (a, b) = (debut + k, debut + k + demi);
                    let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            taille *= 2;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Descripteurs {
    pub duree: f32,
    pub tempo: Option<f32>,
    pub energie: f32,
    pub dynamique: f32,
    pub centroide: f32,
    pub etalement: f32,
    pub pente: f32,
    pub passages: f32,
    pub chroma: [f32; CLASSES],
    pub tonalite: u8,
    pub majeur: bool,
    pub timbre: [f32; COEFS],
    pub variances: [f32; COEFS],
}

impl Descripteurs {
    pub fn vecteur(&self) -> Vec<f32> {
        let mut v = vec![
            self.tempo.map_or(0.0, |t| (t / TEMPO_CENTRE).log2()),
            self.energie / 20.0,
            self.dynamique / 10.0,
            self.centroide / 2500.0,
            self.etalement / 2500.0,
            self.pente,
            self.passages * 5.0,
        ];
        v.extend(self.chroma.iter().map(|c| c * 4.0));
        v.extend(self.timbre.iter().map(|c| c / 10.0));
        v.extend(self.variances.iter().map(|c| c.sqrt() / 5.0));
        v
    }
}

fn mel(f: f32) -> f32 {
    2595.0 * (1.0 + f / 700.0).log10()
}

fn hertz(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

fn filtres() -> Vec<(usize, Vec<f32>)> {
    let pas = mel(FREQUENCE as f32 / 2.0) / (BANDES + 1) as f32;
    let bord = |i: usize| hertz(pas * i as f32) * FENETRE as f32 / FREQUENCE as f32;
    (0..BANDES)
        .map(|b| {
            let (bas, centre, haut) = (bord(b), bord(b + 1), bord(b + 2));
            let debut = bas.ceil() as usize;
            let poids = (debut..=(haut.floor() as usize).min(FENETRE / 2))
                .map(|k| {
                    let k = k as f32;
                    if k <= centre {
                        (k - bas) / (centre - bas)
                    } else {
                        (haut - k) / (haut - centre)
                    }
                })
                .collect();
            (debut, poids)
        })
        .collect()
}

fn classes() -> Vec<Option<usize>> {
    (0..=FENETRE / 2)
        .map(|k| {
            let f = k as f32 * FREQUENCE as f32 / FENETRE as f32;
            (CHROMA_MIN..=CHROMA_MAX)
                .contains(&f)
                .then(|| ((12.0 * (f / DO).log2()).round() as i32).rem_euclid(12) as usize)
        })
        .collect()
}

fn correlation(a: &[f32; CLASSES], b: &[f32; CLASSES], decalage: usize) -> f32 {
    let moyenne = |v: &[f32; CLASSES]| v.iter().sum::<f32>() / CLASSES as f32;
    let (ma, mb) = (moyenne(a), moyenne(b));
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for i in 0..CLASSES {
        let (x, y) = (a[(i + decalage) % CLASSES] - ma, b[i] - mb);
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa <= 0.0 || bb <= 0.0 {
        0.0
    } else {
        ab / (aa * bb).sqrt()
    }
}

pub fn tonalite(chroma: &[f32; CLASSES]) -> (u8, bool) {
    let mut meilleur = (0u8, true, f32::MIN);
    for t in 0..CLASSES {
        for (profil, majeur) in [(&MAJEUR, true), (&MINEUR, false)] {
            let c = correlation(chroma, profil, t);
            if c > meilleur.2 {
                meilleur = (t as u8, majeur, c);
            }
        }
    }
    (meilleur.0, meilleur.1)
}

pub fn tempo(attaques: &[f32]) -> Option<f32> {
    let cadence = FREQUENCE as f32 / PAS as f32;
    let (court, long) = (
        (60.0 * cadence / TEMPO_MAX).floor() as usize,
        (60.0 * cadence / TEMPO_MIN).ceil() as usize,
    );
    let n = attaques.len();
    if n < 4 * long {
        return None;
    }
    let moyenne = attaques.iter().sum::<f32>() / n as f32;
    let brut: Vec<f32> = attaques.iter().map(|a| a - moyenne).collect();
    let x: Vec<f32> = (0..n)
        .map(|i| {
            [1.0, 4.0, 6.0, 4.0, 1.0]
                .iter()
                .enumerate()
                .map(|(j, w)| w * brut[(i + j).saturating_sub(2).min(n - 1)])
                .sum::<f32>()
                / 16.0
        })
        .collect();
    let auto = |retard: usize| {
        x[..n - retard]
            .iter()
            .zip(&x[retard..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
            / (n - retard) as f32
    };
    let zero = auto(0);
    if zero.sqrt() < ATTAQUE_MIN {
        return None;
    }
    let valeurs: Vec<f32> = (court - 1..=long + 1).map(auto).collect();
    let poids = |retard: usize| {
        let octaves = (60.0 * cadence / retard as f32 / TEMPO_CENTRE).log2();
        (-0.5 * octaves * octaves).exp()
    };
    let i = (1..valeurs.len() - 1)
        .filter(|&i| valeurs[i] >= valeurs[i - 1] && valeurs[i] >= valeurs[i + 1])
        .max_by(|&a, &b| {
            (valeurs[a] * poids(court - 1 + a)).total_cmp(&(valeurs[b] * poids(court - 1 + b)))
        })?;
    if valeurs[i] / zero < PULSATION_MIN {
        return None;
    }
    let (a, b, c) = (valeurs[i - 1], valeurs[i], valeurs[i + 1]);
    let courbure = a - 2.0 * b + c;
    let fraction = if courbure < 0.0 {
        0.5 * (a - c) / courbure
    } else {
        0.0
    };
    Some(60.0 * cadence / ((court - 1 + i) as f32 + fraction))
}

pub struct Extracteur {
    fft: Fft,
    fenetre: Vec<f32>,
    filtres: Vec<(usize, Vec<f32>)>,
    cosinus: Vec<[f32; BANDES]>,
    classes: Vec<Option<usize>>,
    tampon: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    echantillons: u64,
    carres: f64,
    poids: f64,
    niveaux: Vec<f32>,
    passages: f64,
    centroide: f64,
    etalement: f64,
    pente: f64,
    chroma: [f64; CLASSES],
    timbre: [f64; COEFS],
    timbre2: [f64; COEFS],
    precedent: Option<[f32; BANDES]>,
    attaques: Vec<f32>,
}

fn decibels(efficace: f64) -> f32 {
    (20.0 * efficace.max(1e-5).log10()) as f32
}

impl Default for Extracteur {
    fn default() -> Self {
        Self::nouveau()
    }
}

impl Extracteur {
    pub fn nouveau() -> Self {
        Self {
            fft: Fft::nouveau(FENETRE),
            fenetre: (0..FENETRE)
                .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FENETRE as f32).cos())
                .collect(),
            filtres: filtres(),
            cosinus: (1..=COEFS)
                .map(|c| {
                    std::array::from_fn(|b| {
                        (PI * c as f32 * (b as f32 + 0.5) / BANDES as f32).cos()
                            * (2.0 / BANDES as f32).sqrt()
                    })
                })
                .collect(),
            classes: classes(),
            tampon: Vec::new(),
            re: vec![0.0; FENETRE],
            im: vec![0.0; FENETRE],
            echantillons: 0,
            carres: 0.0,
            poids: 0.0,
            niveaux: Vec::new(),
            passages: 0.0,
            centroide: 0.0,
            etalement: 0.0,
            pente: 0.0,
            chroma: [0.0; CLASSES],
            timbre: [0.0; COEFS],
            timbre2: [0.0; COEFS],
            precedent: None,
            attaques: Vec::new(),
        }
    }

    pub fn pousser(&mut self, bloc: &[f32]) {
        for &e in bloc {
            self.carres += f64::from(e) * f64::from(e);
        }
        self.echantillons += bloc.len() as u64;
        self.tampon.extend_from_slice(bloc);
        let mut debut = 0;
        while self.tampon.len() - debut >= FENETRE {
            self.trame(debut);
            debut += PAS;
        }
        self.tampon.drain(..debut);
    }

    fn trame(&mut self, debut: usize) {
        let t = &self.tampon[debut..debut + FENETRE];
        let efficace = (t.iter().map(|e| e * e).sum::<f32>() / FENETRE as f32).sqrt();
        self.niveaux.push(decibels(f64::from(efficace)));
        if efficace < SILENCE {
            self.precedent = None;
            self.attaques.push(0.0);
            return;
        }
        let (mut passages, mut signe) = (0u32, 0i8);
        for e in t.iter().filter(|e| e.abs() > SEUIL_PASSAGES * efficace) {
            let courant = if *e > 0.0 { 1 } else { -1 };
            if signe == -courant {
                passages += 1;
            }
            signe = courant;
        }
        for (i, e) in t.iter().enumerate() {
            self.re[i] = e * self.fenetre[i];
            self.im[i] = 0.0;
        }
        self.fft.transformer(&mut self.re, &mut self.im);
        let echelle = 16.0 / (FENETRE * FENETRE) as f32;
        for k in 0..=FENETRE / 2 {
            self.re[k] = (self.re[k] * self.re[k] + self.im[k] * self.im[k]) * echelle;
        }
        let puissances = &self.re[..=FENETRE / 2];
        let largeur = FREQUENCE as f32 / FENETRE as f32;
        let (mut somme, mut ponderee) = (0.0f32, 0.0f32);
        for (k, p) in puissances.iter().enumerate() {
            let m = p.sqrt();
            somme += m;
            ponderee += m * k as f32;
            if let Some(c) = self.classes[k] {
                self.chroma[c] += f64::from(*p);
            }
        }
        let centre = ponderee / somme.max(f32::MIN_POSITIVE);
        let ecart = puissances
            .iter()
            .enumerate()
            .map(|(k, p)| p.sqrt() * (k as f32 - centre) * (k as f32 - centre))
            .sum::<f32>()
            / somme.max(f32::MIN_POSITIVE);
        let mut bandes = [0.0f32; BANDES];
        let mut contours = [0.0f32; BANDES];
        for (b, (premier, poids)) in self.filtres.iter().enumerate() {
            let e: f32 = poids
                .iter()
                .zip(&puissances[*premier..])
                .map(|(w, p)| w * p)
                .sum();
            bandes[b] = (e + PLANCHER).ln();
            contours[b] = (e + PLANCHER_ATTAQUES).ln();
        }
        let milieu = (BANDES - 1) as f32 / 2.0;
        let moyenne = bandes.iter().sum::<f32>() / BANDES as f32;
        let (mut haut, mut bas) = (0.0f32, 0.0f32);
        for (b, e) in bandes.iter().enumerate() {
            haut += (b as f32 - milieu) * (e - moyenne);
            bas += (b as f32 - milieu) * (b as f32 - milieu);
        }
        let attaque = self.precedent.map_or(0.0, |p| {
            contours
                .iter()
                .zip(&p)
                .map(|(e, a)| (e - a).max(0.0))
                .sum::<f32>()
        });
        self.attaques.push(attaque);
        self.precedent = Some(contours);
        let w = f64::from(efficace);
        for (c, cosinus) in self.cosinus.iter().enumerate() {
            let v: f32 = cosinus.iter().zip(&bandes).map(|(w, e)| w * e).sum();
            self.timbre[c] += w * f64::from(v);
            self.timbre2[c] += w * f64::from(v) * f64::from(v);
        }
        self.passages += w * f64::from(passages) / (FENETRE - 1) as f64;
        self.centroide += w * f64::from(centre * largeur);
        self.etalement += w * f64::from(ecart.sqrt() * largeur);
        self.pente += w * f64::from(haut / bas);
        self.poids += w;
    }

    pub fn terminer(self) -> Option<Descripteurs> {
        if self.echantillons < FREQUENCE as u64 {
            return None;
        }
        let n = if self.poids > 0.0 { self.poids } else { 1.0 };
        let moyenne = |v: f64| (v / n) as f32;
        let energie = decibels((self.carres / self.echantillons as f64).sqrt());
        let (mut trames, mut somme, mut carres) = (0.0f64, 0.0f64, 0.0f64);
        for d in self.niveaux.iter().filter(|d| **d > energie - PORTE) {
            trames += 1.0;
            somme += f64::from(*d);
            carres += f64::from(*d) * f64::from(*d);
        }
        let dynamique = if trames > 0.0 {
            (carres / trames - (somme / trames).powi(2)).max(0.0).sqrt() as f32
        } else {
            0.0
        };
        let total: f64 = self.chroma.iter().sum();
        let chroma: [f32; CLASSES] = std::array::from_fn(|c| {
            if total > 0.0 {
                (self.chroma[c] / total) as f32
            } else {
                1.0 / CLASSES as f32
            }
        });
        let (tonalite, majeur) = tonalite(&chroma);
        Some(Descripteurs {
            duree: self.echantillons as f32 / FREQUENCE as f32,
            tempo: tempo(&self.attaques),
            energie,
            dynamique,
            centroide: moyenne(self.centroide),
            etalement: moyenne(self.etalement),
            pente: moyenne(self.pente),
            passages: moyenne(self.passages),
            chroma,
            tonalite,
            majeur,
            timbre: std::array::from_fn(|c| moyenne(self.timbre[c])),
            variances: std::array::from_fn(|c| {
                (self.timbre2[c] / n - (self.timbre[c] / n).powi(2)).max(0.0) as f32
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyser(signal: impl Fn(f32) -> f32, secondes: usize) -> Descripteurs {
        let mut e = Extracteur::nouveau();
        let s: Vec<f32> = (0..secondes * FREQUENCE)
            .map(|i| signal(i as f32 / FREQUENCE as f32))
            .collect();
        for bloc in s.chunks(1000) {
            e.pousser(bloc);
        }
        e.terminer().unwrap()
    }

    #[test]
    fn fft_egale_a_la_transformee_directe() {
        let n = 64;
        let x: Vec<f32> = (0..n).map(|i| ((i * 37 % 17) as f32 - 8.0) / 8.0).collect();
        let (mut re, mut im) = (x.clone(), vec![0.0; n]);
        Fft::nouveau(n).transformer(&mut re, &mut im);
        for k in 0..n {
            let (mut r, mut i) = (0.0f32, 0.0f32);
            for (t, v) in x.iter().enumerate() {
                let a = -2.0 * PI * (k * t) as f32 / n as f32;
                r += v * a.cos();
                i += v * a.sin();
            }
            assert!((re[k] - r).abs() < 1e-3 && (im[k] - i).abs() < 1e-3, "{k}");
        }
    }

    #[test]
    fn dimension_fixe_et_valeurs_finies() {
        let d = analyser(|t| 0.3 * (2.0 * PI * 440.0 * t).sin(), 3);
        let v = d.vecteur();
        assert_eq!(v.len(), DIMENSION);
        assert!(v.iter().all(|x| x.is_finite()), "{v:?}");
        assert!((d.chroma.iter().sum::<f32>() - 1.0).abs() < 1e-3);
        assert!((d.duree - 3.0).abs() < 0.01);
    }

    #[test]
    fn decoupage_des_blocs_sans_effet() {
        let s: Vec<f32> = (0..3 * FREQUENCE)
            .map(|i| 0.2 * (i as f32 * 0.05).sin() + 0.1 * (i as f32 * 0.9).sin())
            .collect();
        let par = |taille: usize| {
            let mut e = Extracteur::nouveau();
            for b in s.chunks(taille) {
                e.pousser(b);
            }
            e.terminer().unwrap()
        };
        assert_eq!(par(1), par(65_536));
        assert_eq!(par(511), par(4097));
    }

    #[test]
    fn silence_ou_signal_trop_court() {
        let mut e = Extracteur::nouveau();
        e.pousser(&vec![0.1; FREQUENCE - 1]);
        assert!(e.terminer().is_none());
        let d = analyser(|_| 0.0, 2);
        assert_eq!(d.tempo, None);
        assert_eq!(d.energie, -100.0);
        assert!(d.vecteur().iter().all(|x| x.is_finite()));
    }

    #[test]
    fn tonalite_d_un_accord() {
        let accord = |notes: [f32; 3]| {
            analyser(
                move |t| {
                    notes
                        .iter()
                        .map(|f| 0.2 * (2.0 * PI * f * t).sin())
                        .sum::<f32>()
                },
                3,
            )
        };
        let la_mineur = accord([440.0, 523.25, 659.25]);
        assert_eq!((la_mineur.tonalite, la_mineur.majeur), (9, false));
        let do_majeur = accord([523.25, 659.25, 783.99]);
        assert_eq!((do_majeur.tonalite, do_majeur.majeur), (0, true));
    }
}
