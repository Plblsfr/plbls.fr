use std::collections::VecDeque;
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use estompe_plateforme::commande_d_arriere_plan;

use crate::video::FichierTemporaire;

mod opus;
mod webm;

pub use opus::{Decodeur, Encodeur};

pub const TAUX: u32 = 48_000;
pub const DEBIT: i32 = 32_000;
pub const DUREE_MAX_MS: u64 = 5 * 60 * 1000;
pub const BARRES: usize = 40;
pub const ECOUTES_MAX: usize = 500;
pub const ECHANTILLONS_PAR_NIVEAU: usize = TAUX as usize / 10;
pub const NOM_DU_FICHIER: &str = "message-vocal.ogg";
pub const TYPE_MIME: &str = "audio/ogg";
pub const VITESSES: [f32; 3] = [1.0, 1.5, 2.0];
pub const LIBELLES_VITESSE: [&str; 3] = ["1×", "1,5×", "2×"];

const DEMI_TRAME: usize = 512;
const TRAME_ETIREMENT: usize = 2 * DEMI_TRAME;
const TOLERANCE: isize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurVocal {
    FormatInconnu,
    Illisible,
    Codec(String),
}

impl fmt::Display for ErreurVocal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FormatInconnu => f.write_str("format de message vocal inconnu"),
            Self::Illisible => f.write_str("message vocal illisible"),
            Self::Codec(raison) => f.write_str(raison),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    pub echantillons: Vec<f32>,
}

impl Audio {
    pub fn duree_ms(&self) -> u64 {
        self.echantillons.len() as u64 * 1000 / u64::from(TAUX)
    }
}

pub fn niveau(echantillons: &[f32]) -> u8 {
    let somme: f64 = echantillons.iter().map(|e| f64::from(*e).powi(2)).sum();
    let db = 10.0 * (somme / echantillons.len().max(1) as f64 + 1e-10).log10();
    ((db + 60.0) * 100.0 / 60.0).clamp(0.0, 100.0).round() as u8
}

pub fn forme_d_onde(niveaux: &[u8], barres: usize) -> Vec<u8> {
    if niveaux.is_empty() {
        return vec![0];
    }
    let n = barres.min(niveaux.len());
    (0..n)
        .map(|i| {
            let de = i * niveaux.len() / n;
            let a = ((i + 1) * niveaux.len() / n).max(de + 1);
            let tranche = &niveaux[de..a];
            (tranche.iter().map(|v| f64::from(*v)).sum::<f64>() / tranche.len() as f64).round()
                as u8
        })
        .collect()
}

pub fn marquer_ecoute(ecoutes: &mut Vec<String>, id: &str) -> bool {
    if ecoutes.last().is_some_and(|e| e == id) {
        return false;
    }
    ecoutes.retain(|e| e != id);
    ecoutes.push(id.to_string());
    let surplus = ecoutes.len().saturating_sub(ECOUTES_MAX);
    ecoutes.drain(..surplus);
    true
}

pub fn horloge(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, ms / 1000 % 60)
}

pub fn decoder(octets: &[u8]) -> Result<Audio, ErreurVocal> {
    let (paquets, preskip, fin) = if octets.starts_with(b"OggS") {
        let (paquets, granule) = opus::paquets_ogg(octets)?;
        let preskip = paquets
            .first()
            .and_then(|t| opus::preskip_de_la_tete(t))
            .ok_or(ErreurVocal::FormatInconnu)?;
        let audio = paquets.into_iter().skip(2).collect();
        (audio, preskip, granule.map(|g| g as usize))
    } else if octets.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let piste = webm::paquets_webm(octets)?;
        (piste.paquets, piste.preskip, None)
    } else {
        return Err(ErreurVocal::FormatInconnu);
    };
    let mut decodeur = Decodeur::nouveau()?;
    let mut echantillons = Vec::new();
    for paquet in &paquets {
        decodeur.decoder(paquet, &mut echantillons);
    }
    if let Some(fin) = fin {
        echantillons.truncate(fin);
    }
    let debut = preskip.min(echantillons.len());
    echantillons.drain(..debut);
    Ok(Audio { echantillons })
}

pub fn decoder_avec_ffmpeg(ffmpeg: &Path, octets: &[u8]) -> Result<Audio, ErreurVocal> {
    let mut hasard = [0u8; 8];
    let _ = getrandom::fill(&mut hasard);
    let entree = FichierTemporaire::nouveau(
        std::env::temp_dir().join(format!("estompe-vocal-{}", u64::from_le_bytes(hasard))),
    );
    std::fs::write(entree.chemin(), octets).map_err(|e| ErreurVocal::Codec(e.to_string()))?;
    let mut processus = commande_d_arriere_plan(ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(entree.chemin())
        .args(["-f", "f32le", "-ac", "1", "-ar", "48000", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ErreurVocal::FormatInconnu)?;
    let mut brut = Vec::new();
    if let Some(sortie) = processus.stdout.as_mut() {
        let _ = sortie.read_to_end(&mut brut);
    }
    let statut = processus
        .wait()
        .map_err(|e| ErreurVocal::Codec(e.to_string()))?;
    if !statut.success() || brut.is_empty() {
        return Err(ErreurVocal::Illisible);
    }
    Ok(Audio {
        echantillons: brut
            .as_chunks::<4>()
            .0
            .iter()
            .map(|o| f32::from_le_bytes(*o))
            .collect(),
    })
}

#[derive(Debug, Clone)]
pub struct Reechantillonneur {
    pas: f64,
    position: f64,
    precedent: f32,
}

impl Reechantillonneur {
    pub fn nouveau(de: u32, vers: u32) -> Self {
        Self {
            pas: f64::from(de.max(1)) / f64::from(vers.max(1)),
            position: 0.0,
            precedent: 0.0,
        }
    }

    pub fn pousser(&mut self, entree: &[f32], sortie: &mut Vec<f32>) {
        if entree.is_empty() {
            return;
        }
        if self.pas == 1.0 {
            sortie.extend_from_slice(entree);
            return;
        }
        let n = entree.len() as f64;
        while self.position < n - 1.0 {
            let i = self.position.floor();
            let fraction = (self.position - i) as f32;
            let a = if i < 0.0 {
                self.precedent
            } else {
                entree[i as usize]
            };
            let b = entree[(i + 1.0) as usize];
            sortie.push(a + (b - a) * fraction);
            self.position += self.pas;
        }
        self.position -= n;
        self.precedent = entree[entree.len() - 1];
    }
}

pub fn indice_de_vitesse(vitesse: usize) -> usize {
    vitesse % VITESSES.len()
}

pub struct Lecture {
    audio: Arc<Audio>,
    position: f64,
    vitesse: f32,
    recouvrement: Vec<f32>,
    precedent: Option<usize>,
    reechantillonneur: Option<(u32, Reechantillonneur)>,
    tampon: VecDeque<f32>,
    finie: bool,
}

impl Lecture {
    pub fn nouvelle(audio: Arc<Audio>, vitesse: f32) -> Self {
        Self {
            audio,
            position: 0.0,
            vitesse,
            recouvrement: vec![0.0; DEMI_TRAME],
            precedent: None,
            reechantillonneur: None,
            tampon: VecDeque::new(),
            finie: false,
        }
    }

    pub fn duree_ms(&self) -> u64 {
        self.audio.duree_ms()
    }

    pub fn position_ms(&self) -> u64 {
        ((self.position.min(self.audio.echantillons.len() as f64)) * 1000.0 / f64::from(TAUX))
            as u64
    }

    pub fn finie(&self) -> bool {
        self.finie
    }

    pub fn definir_vitesse(&mut self, vitesse: f32) {
        self.vitesse = vitesse;
    }

    pub fn placer(&mut self, fraction: f32) {
        self.position = f64::from(fraction.clamp(0.0, 1.0)) * self.audio.echantillons.len() as f64;
        self.precedent = None;
        self.recouvrement.iter_mut().for_each(|e| *e = 0.0);
        self.tampon.clear();
        self.finie = false;
    }

    pub fn remplir(&mut self, sortie: &mut [f32], taux: u32) {
        if self.reechantillonneur.as_ref().map(|(t, _)| *t) != Some(taux) {
            self.reechantillonneur = Some((taux, Reechantillonneur::nouveau(TAUX, taux)));
        }
        let mut morceau = Vec::with_capacity(DEMI_TRAME);
        let mut converti = Vec::new();
        while self.tampon.len() < sortie.len() && !self.finie {
            morceau.clear();
            if !self.produire(&mut morceau) {
                self.finie = true;
            }
            converti.clear();
            if let Some((_, r)) = self.reechantillonneur.as_mut() {
                r.pousser(&morceau, &mut converti);
            }
            self.tampon.extend(converti.iter().copied());
        }
        for s in sortie.iter_mut() {
            *s = self.tampon.pop_front().unwrap_or(0.0);
        }
    }

    fn produire(&mut self, sortie: &mut Vec<f32>) -> bool {
        let source = &self.audio.echantillons;
        let debut = self.position as usize;
        if debut >= source.len() {
            return false;
        }
        if self.vitesse == 1.0 {
            let fin = (debut + DEMI_TRAME).min(source.len());
            sortie.extend_from_slice(&source[debut..fin]);
            self.position = fin as f64;
            self.precedent = None;
            return true;
        }
        let nominal = self.position as isize;
        let debut = match self.precedent {
            Some(precedent) => meilleur_depart(source, precedent + DEMI_TRAME, nominal),
            None => nominal.max(0) as usize,
        };
        for k in 0..TRAME_ETIREMENT {
            let poids =
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * k as f32 / TRAME_ETIREMENT as f32).cos();
            let e = source.get(debut + k).copied().unwrap_or(0.0) * poids;
            if k < DEMI_TRAME {
                sortie.push(self.recouvrement[k] + e);
            } else {
                self.recouvrement[k - DEMI_TRAME] = e;
            }
        }
        self.precedent = Some(debut);
        self.position += DEMI_TRAME as f64 * f64::from(self.vitesse);
        true
    }
}

fn meilleur_depart(source: &[f32], naturel: usize, nominal: isize) -> usize {
    let echantillon = |i: usize| source.get(i).copied().unwrap_or(0.0);
    let mut meilleur = (f32::MIN, nominal.max(0) as usize);
    for decalage in -TOLERANCE..=TOLERANCE {
        let candidat = nominal + decalage;
        if candidat < 0 {
            continue;
        }
        let candidat = candidat as usize;
        let score: f32 = (0..DEMI_TRAME)
            .step_by(2)
            .map(|k| echantillon(naturel + k) * echantillon(candidat + k))
            .sum();
        if score > meilleur.0 {
            meilleur = (score, candidat);
        }
    }
    meilleur.1
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    const CHROME: &[u8] = include_bytes!("essais/chrome.webm");
    const FIREFOX: &[u8] = include_bytes!("essais/firefox.ogg");
    const SAFARI: &[u8] = include_bytes!("essais/safari.m4a");
    const NATIF: &[u8] = include_bytes!("essais/natif.ogg");

    fn sinus(frequence: f32, taux: u32, duree_ms: u64, amplitude: f32) -> Vec<f32> {
        let n = (u64::from(taux) * duree_ms / 1000) as usize;
        (0..n)
            .map(|i| {
                amplitude * (2.0 * std::f32::consts::PI * frequence * i as f32 / taux as f32).sin()
            })
            .collect()
    }

    fn frequence(echantillons: &[f32], taux: u32) -> f32 {
        let milieu = &echantillons[echantillons.len() / 8..echantillons.len() * 7 / 8];
        let passages = milieu
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        passages as f32 * taux as f32 / milieu.len() as f32
    }

    fn efficace(echantillons: &[f32]) -> f32 {
        (echantillons.iter().map(|e| e * e).sum::<f32>() / echantillons.len().max(1) as f32).sqrt()
    }

    fn message_natif(duree_ms: u64) -> Vec<u8> {
        let mut encodeur = Encodeur::nouveau().unwrap();
        for bloc in sinus(440.0, TAUX, duree_ms, 0.5).chunks(441) {
            encodeur.pousser(bloc);
        }
        assert_eq!(encodeur.echantillons(), u64::from(TAUX) * duree_ms / 1000);
        encodeur.terminer()
    }

    fn ffmpeg() -> Option<&'static str> {
        if Command::new("ffmpeg").arg("-version").output().is_ok() {
            return Some("ffmpeg");
        }
        assert!(
            std::env::var_os("ESTOMPE_FFMPEG_EXIGE").is_none(),
            "ffmpeg exigé (ESTOMPE_FFMPEG_EXIGE)"
        );
        println!("ignoré : ffmpeg indisponible");
        None
    }

    fn verifier_le_la(audio: &Audio, duree_ms: u64) {
        assert!(
            audio.duree_ms().abs_diff(duree_ms) <= 50,
            "durée {} ms au lieu de {duree_ms}",
            audio.duree_ms()
        );
        let f = frequence(&audio.echantillons, TAUX);
        assert!((f - 440.0).abs() < 6.0, "fréquence {f}");
        let e = efficace(&audio.echantillons);
        assert!((0.25..0.45).contains(&e), "valeur efficace {e}");
    }

    #[test]
    fn niveau_et_forme_d_onde_comme_le_front() {
        assert_eq!(niveau(&[0.0; 256]), 0);
        assert_eq!(niveau(&[1.0; 256]), 100);
        assert_eq!(forme_d_onde(&[10, 30, 50, 70], 2), vec![20, 60]);
        assert_eq!(forme_d_onde(&[40, 40], BARRES), vec![40, 40]);
        assert_eq!(forme_d_onde(&[], BARRES), vec![0]);
        assert_eq!(horloge(0), "0:00");
        let mut ecoutes: Vec<String> = (0..ECOUTES_MAX).map(|i| format!("f{i}")).collect();
        assert!(marquer_ecoute(&mut ecoutes, "f3"));
        assert!(!marquer_ecoute(&mut ecoutes, "f3"));
        assert!(marquer_ecoute(&mut ecoutes, "nouveau"));
        assert_eq!(ecoutes.len(), ECOUTES_MAX);
        assert_eq!(ecoutes[ECOUTES_MAX - 2..], ["f3", "nouveau"]);
        assert_eq!(ecoutes[0], "f1");
        assert_eq!(horloge(65_400), "1:05");
    }

    #[test]
    fn un_signal_synthetique_encode_en_ogg_opus_se_decode_a_l_identique() {
        let octets = message_natif(1500);
        assert!(octets.starts_with(b"OggS\0\x02"));
        assert!(
            octets.len() < 1500 * DEBIT as usize / 8000 * 13 / 10,
            "{} octets",
            octets.len()
        );
        let audio = decoder(&octets).unwrap();
        assert_eq!(audio.echantillons.len(), 72_000);
        verifier_le_la(&audio, 1500);
    }

    #[test]
    fn les_formats_du_front_se_decodent_sans_peripherique() {
        verifier_le_la(&decoder(CHROME).unwrap(), 1500);
        verifier_le_la(&decoder(FIREFOX).unwrap(), 1500);
        verifier_le_la(&decoder(NATIF).unwrap(), 1500);
        assert_eq!(decoder(SAFARI), Err(ErreurVocal::FormatInconnu));
        assert_eq!(decoder(b"OggS\0\0"), Err(ErreurVocal::FormatInconnu));
        assert!(decoder(b"<script>").is_err());
    }

    #[test]
    fn un_message_du_front_en_aac_se_lit_par_ffmpeg() {
        let Some(ffmpeg) = ffmpeg() else {
            return;
        };
        verifier_le_la(
            &decoder_avec_ffmpeg(Path::new(ffmpeg), SAFARI).unwrap(),
            1500,
        );
    }

    #[test]
    fn un_message_natif_est_lu_par_un_autre_decodeur() {
        let Some(ffmpeg) = ffmpeg() else {
            return;
        };
        for octets in [message_natif(1500), NATIF.to_vec()] {
            verifier_le_la(
                &decoder_avec_ffmpeg(Path::new(ffmpeg), &octets).unwrap(),
                1500,
            );
        }
    }

    #[test]
    fn le_reechantillonnage_garde_la_hauteur_et_la_duree() {
        let source = sinus(440.0, 44_100, 1000, 0.5);
        let mut r = Reechantillonneur::nouveau(44_100, TAUX);
        let mut sortie = Vec::new();
        for bloc in source.chunks(500) {
            r.pousser(bloc, &mut sortie);
        }
        assert!(sortie.len().abs_diff(48_000) <= 2, "{}", sortie.len());
        assert!((frequence(&sortie, TAUX) - 440.0).abs() < 3.0);
    }

    #[test]
    fn la_lecture_acceleree_garde_la_hauteur_et_raccourcit_la_duree() {
        let audio = Arc::new(Audio {
            echantillons: sinus(440.0, TAUX, 2000, 0.5),
        });
        for (vitesse, attendu) in [(1.0, 96_000.0), (1.5, 64_000.0), (2.0, 48_000.0)] {
            let mut lecture = Lecture::nouvelle(audio.clone(), vitesse);
            let mut sortie = Vec::new();
            let mut bloc = [0f32; 480];
            while !lecture.finie() {
                lecture.remplir(&mut bloc, TAUX);
                sortie.extend_from_slice(&bloc);
            }
            let utile = sortie.iter().rposition(|e| *e != 0.0).unwrap_or(0) + 1;
            assert!(
                (utile as f32 - attendu).abs() < 1500.0,
                "{vitesse} : {utile}"
            );
            assert!(
                (frequence(&sortie[..utile], TAUX) - 440.0).abs() < 8.0,
                "{vitesse}"
            );
            assert_eq!(lecture.position_ms(), 2000);
        }
    }

    #[test]
    fn la_lecture_se_place_et_suit_la_sortie_de_l_appareil() {
        let audio = Arc::new(Audio {
            echantillons: sinus(440.0, TAUX, 1000, 0.5),
        });
        let mut lecture = Lecture::nouvelle(audio, 1.0);
        lecture.placer(0.5);
        assert_eq!(lecture.position_ms(), 500);
        let mut bloc = vec![0f32; 44_100 / 10];
        lecture.remplir(&mut bloc, 44_100);
        assert!(
            lecture.position_ms().abs_diff(600) <= 12,
            "{}",
            lecture.position_ms()
        );
        assert!(!lecture.finie());
    }
}
