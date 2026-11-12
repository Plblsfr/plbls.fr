use nnnoiseless::DenoiseState;

pub const FREQUENCE: u32 = 48_000;
const TRAME: usize = DenoiseState::FRAME_SIZE;
const PLEINE_ECHELLE: f32 = 32_767.0;

fn normaliser(libelle: &str) -> String {
    let mut l = libelle.trim().to_lowercase();
    for prefixe in ["par défaut - ", "default - ", "communications - ", "communication - "] {
        if let Some(reste) = l.strip_prefix(prefixe) {
            l = reste.to_string();
        }
    }
    if let Some((debut, fin)) = l.rsplit_once(" (") {
        let code = fin.trim_end_matches(')');
        if code.len() == 9 && code.chars().enumerate().all(|(i, c)| if i == 4 { c == ':' } else { c.is_ascii_hexdigit() }) {
            l = debut.to_string();
        }
    }
    l.trim().to_string()
}

pub fn choisir(peripheriques: &[String], voulu: Option<&str>) -> Option<usize> {
    let voulu = normaliser(voulu?);
    if voulu.is_empty() {
        return None;
    }
    let noms: Vec<String> = peripheriques.iter().map(|p| normaliser(p)).collect();
    noms.iter().position(|n| *n == voulu).or_else(|| noms.iter().position(|n| !n.is_empty() && (n.contains(&voulu) || voulu.contains(n.as_str()))))
}

pub struct Traitement {
    debruiteur: Option<Box<DenoiseState<'static>>>,
    gain: f32,
    attente: Vec<f32>,
}

impl Traitement {
    pub fn nouveau(reduction: &str, gain: f32) -> Self {
        Self { debruiteur: (reduction != "off").then(DenoiseState::new), gain, attente: Vec::new() }
    }

    pub fn traiter(&mut self, entree: &[f32]) -> Vec<i16> {
        let fixer = |v: f32, gain: f32| (v * gain).clamp(-PLEINE_ECHELLE, PLEINE_ECHELLE) as i16;
        let Some(debruiteur) = self.debruiteur.as_mut() else {
            return entree.iter().map(|v| fixer(v * PLEINE_ECHELLE, self.gain)).collect();
        };
        self.attente.extend(entree.iter().map(|v| v * PLEINE_ECHELLE));
        let mut sortie = Vec::with_capacity(self.attente.len());
        let mut trame = [0f32; TRAME];
        let complet = self.attente.len() / TRAME * TRAME;
        for bloc in self.attente[..complet].chunks_exact(TRAME) {
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
    fn retrouve_le_micro_de_la_webview() {
        let p: Vec<String> = ["Microphone (Realtek(R) Audio)", "Microphone (HyperX QuadCast)", "Casque (Arctis 7 Chat)"].map(String::from).into();
        assert_eq!(choisir(&p, Some("Microphone (HyperX QuadCast) (0951:16df)")), Some(1));
        assert_eq!(choisir(&p, Some("Par défaut - Microphone (Realtek(R) Audio)")), Some(0));
        assert_eq!(choisir(&p, Some("Communications - Casque (Arctis 7 Chat)")), Some(2));
        assert_eq!(choisir(&p, Some("Default - microphone (hyperx quadcast)")), Some(1));
        assert_eq!(choisir(&p, Some("Arctis 7 Chat")), Some(2));
        assert_eq!(choisir(&p, Some("Webcam C920")), None);
        assert_eq!(choisir(&p, Some("  ")), None);
        assert_eq!(choisir(&p, None), None);
    }

    #[test]
    fn gain_borne_sans_reduction() {
        let mut t = Traitement::nouveau("off", 2.0);
        assert_eq!(t.traiter(&[0.0, 0.25, -0.25, 0.9, -1.0]), [0, 16383, -16383, 32767, -32767]);
        assert_eq!(Traitement::nouveau("off", 0.0).traiter(&[0.5; 7]), [0; 7]);
    }

    #[test]
    fn reduction_par_trames_entieres() {
        let mut t = Traitement::nouveau("strong", 1.0);
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
        let brut = energie(&Traitement::nouveau("off", 1.0).traiter(&bruit));
        let filtre = energie(&Traitement::nouveau("standard", 1.0).traiter(&bruit));
        assert!(filtre < brut * 0.7, "{filtre} {brut}");
    }
}
