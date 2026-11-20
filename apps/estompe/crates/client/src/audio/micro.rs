use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nnnoiseless::DenoiseState;
use serde::{Deserialize, Serialize};

use super::format::{vers_i16, ECHANTILLONS_PAR_TRAME};

pub const GAIN_MICRO_MAXIMAL: u16 = 300;
pub const SEUIL_MINIMAL: i32 = -75;
pub const SEUIL_MAXIMAL: i32 = -15;
pub const SILENCE_DB: f32 = -100.0;
pub const MAINTIEN_VOIX: Duration = Duration::from_millis(400);
const PLEINE_ECHELLE: f32 = i16::MAX as f32;
const OUVERTURE_PORTE: f32 = 1.0 / 48.0;
const FERMETURE_PORTE: f32 = 1.0 / 480.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeBruit {
    Aucun,
    Standard,
    #[default]
    Renforce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesMicro {
    pub bruit: ModeBruit,
    pub gain: u16,
    pub detection_voix: bool,
    pub seuil: i32,
}

impl Default for ReglagesMicro {
    fn default() -> Self {
        Self {
            bruit: ModeBruit::Renforce,
            gain: 100,
            detection_voix: false,
            seuil: -45,
        }
    }
}

impl ReglagesMicro {
    pub fn borne(self) -> Self {
        Self {
            gain: self.gain.min(GAIN_MICRO_MAXIMAL),
            seuil: self.seuil.clamp(SEUIL_MINIMAL, SEUIL_MAXIMAL),
            ..self
        }
    }
}

pub fn niveau_db(echantillons: &[f32]) -> f32 {
    if echantillons.is_empty() {
        return SILENCE_DB;
    }
    let somme: f32 = echantillons.iter().map(|e| e * e).sum();
    let rms = (somme / echantillons.len() as f32).sqrt();
    if rms < 1e-5 {
        SILENCE_DB
    } else {
        (20.0 * rms.log10()).max(SILENCE_DB)
    }
}

pub fn porte_ouverte(ptt: bool, detection_voix: bool, touche: bool, voix: bool) -> bool {
    if ptt {
        touche
    } else if detection_voix {
        voix
    } else {
        true
    }
}

#[derive(Debug, Clone)]
pub struct DetectionVoix {
    maintien: u32,
    restant: u32,
}

impl Default for DetectionVoix {
    fn default() -> Self {
        let trame_ms = (ECHANTILLONS_PAR_TRAME * 1000 / super::FREQUENCE as usize) as u128;
        Self::nouvelle((MAINTIEN_VOIX.as_millis() / trame_ms) as u32)
    }
}

impl DetectionVoix {
    pub fn nouvelle(maintien_en_trames: u32) -> Self {
        Self {
            maintien: maintien_en_trames.max(1),
            restant: 0,
        }
    }

    pub fn mesurer(&mut self, niveau: f32, seuil: f32) -> bool {
        if niveau > seuil {
            self.restant = self.maintien;
        } else {
            self.restant = self.restant.saturating_sub(1);
        }
        self.restant > 0
    }

    pub fn active(&self) -> bool {
        self.restant > 0
    }

    pub fn reinitialiser(&mut self) {
        self.restant = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NiveauMicro {
    pub db: f32,
    pub voix: bool,
}

impl Default for NiveauMicro {
    fn default() -> Self {
        Self {
            db: SILENCE_DB,
            voix: false,
        }
    }
}

#[derive(Debug)]
pub struct MesureMicro {
    niveau: AtomicU32,
    voix: AtomicBool,
}

impl Default for MesureMicro {
    fn default() -> Self {
        Self {
            niveau: AtomicU32::new(SILENCE_DB.to_bits()),
            voix: AtomicBool::new(false),
        }
    }
}

impl MesureMicro {
    pub fn publier(&self, niveau: NiveauMicro) {
        self.niveau.store(niveau.db.to_bits(), Ordering::Relaxed);
        self.voix.store(niveau.voix, Ordering::Relaxed);
    }

    pub fn lire(&self) -> NiveauMicro {
        NiveauMicro {
            db: f32::from_bits(self.niveau.load(Ordering::Relaxed)),
            voix: self.voix.load(Ordering::Relaxed),
        }
    }

    pub fn reinitialiser(&self) {
        self.publier(NiveauMicro::default());
    }
}

pub trait MoteurApm: Send {
    fn capter(&mut self, trame: &mut [i16]);
    fn jouer(&mut self, trame: &mut [i16]);
}

pub struct Apm {
    actif: AtomicBool,
    moteur: Mutex<Box<dyn MoteurApm>>,
}

impl Apm {
    pub fn nouveau(moteur: Box<dyn MoteurApm>) -> Arc<Self> {
        Arc::new(Self {
            actif: AtomicBool::new(false),
            moteur: Mutex::new(moteur),
        })
    }

    pub fn activer(&self, actif: bool) {
        self.actif.store(actif, Ordering::Relaxed);
    }

    pub fn actif(&self) -> bool {
        self.actif.load(Ordering::Relaxed)
    }

    pub fn capter(&self, trame: &mut [i16]) {
        if !self.actif() || trame.len() != ECHANTILLONS_PAR_TRAME {
            return;
        }
        if let Ok(mut moteur) = self.moteur.lock() {
            moteur.capter(trame);
        }
    }

    pub fn jouer(&self, trame: &[f32]) {
        if !self.actif() || trame.len() != ECHANTILLONS_PAR_TRAME {
            return;
        }
        let mut reference = [0i16; ECHANTILLONS_PAR_TRAME];
        for (r, e) in reference.iter_mut().zip(trame) {
            *r = vers_i16(*e);
        }
        if let Ok(mut moteur) = self.moteur.lock() {
            moteur.jouer(&mut reference);
        }
    }
}

pub struct ChaineMicro {
    reglages: ReglagesMicro,
    apm: Option<Arc<Apm>>,
    debruiteur: Option<Box<DenoiseState<'static>>>,
    signal: [f32; ECHANTILLONS_PAR_TRAME],
    debruite: [f32; ECHANTILLONS_PAR_TRAME],
    sortie: [i16; ECHANTILLONS_PAR_TRAME],
    detection: DetectionVoix,
    porte: f32,
    mesure: Arc<MesureMicro>,
}

impl ChaineMicro {
    pub fn nouvelle(
        reglages: ReglagesMicro,
        apm: Option<Arc<Apm>>,
        mesure: Arc<MesureMicro>,
    ) -> Self {
        let mut chaine = Self {
            reglages: ReglagesMicro::default(),
            apm,
            debruiteur: None,
            signal: [0.0; ECHANTILLONS_PAR_TRAME],
            debruite: [0.0; ECHANTILLONS_PAR_TRAME],
            sortie: [0; ECHANTILLONS_PAR_TRAME],
            detection: DetectionVoix::default(),
            porte: 1.0,
            mesure,
        };
        chaine.appliquer(reglages);
        chaine
    }

    pub fn reglages(&self) -> ReglagesMicro {
        self.reglages
    }

    pub fn appliquer(&mut self, reglages: ReglagesMicro) {
        let reglages = reglages.borne();
        if reglages.bruit == ModeBruit::Renforce {
            if self.debruiteur.is_none() {
                self.debruiteur = Some(DenoiseState::new());
            }
        } else {
            self.debruiteur = None;
        }
        if let Some(apm) = &self.apm {
            apm.activer(reglages.bruit != ModeBruit::Aucun);
        }
        if !reglages.detection_voix {
            self.detection.reinitialiser();
        }
        self.reglages = reglages;
    }

    pub fn mesure(&self) -> Arc<MesureMicro> {
        self.mesure.clone()
    }

    pub fn traiter(&mut self, trame: &[i16]) -> &[i16] {
        let n = trame.len().min(ECHANTILLONS_PAR_TRAME);
        let complete = n == ECHANTILLONS_PAR_TRAME;
        self.sortie[..n].copy_from_slice(&trame[..n]);
        if let Some(apm) = &self.apm {
            if self.reglages.bruit != ModeBruit::Aucun {
                apm.capter(&mut self.sortie[..n]);
            }
        }
        for (s, e) in self.signal.iter_mut().zip(&self.sortie[..n]) {
            *s = f32::from(*e);
        }
        if let (Some(debruiteur), true) = (self.debruiteur.as_mut(), complete) {
            debruiteur.process_frame(&mut self.debruite, &self.signal);
            self.signal.copy_from_slice(&self.debruite);
        }
        for s in &mut self.signal[..n] {
            *s /= PLEINE_ECHELLE;
        }
        let db = niveau_db(&self.signal[..n]);
        let voix = self.detection.mesurer(db, self.reglages.seuil as f32);
        self.mesure.publier(NiveauMicro { db, voix });
        let cible = if porte_ouverte(false, self.reglages.detection_voix, false, voix) {
            1.0
        } else {
            0.0
        };
        let gain = f32::from(self.reglages.gain) / 100.0;
        for (s, e) in self.sortie[..n].iter_mut().zip(&self.signal[..n]) {
            self.porte = if cible > self.porte {
                (self.porte + OUVERTURE_PORTE).min(cible)
            } else {
                (self.porte - FERMETURE_PORTE).max(cible)
            };
            *s = vers_i16(e * gain * self.porte);
        }
        &self.sortie[..n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::Reechantillonneur;
    use crate::audio::FREQUENCE;

    fn sinus(amplitude: f32, frequence: f32, longueur: usize) -> Vec<f32> {
        (0..longueur)
            .map(|i| {
                amplitude
                    * (2.0 * std::f32::consts::PI * frequence * i as f32 / FREQUENCE as f32).sin()
            })
            .collect()
    }

    fn en_i16(signal: &[f32]) -> Vec<i16> {
        signal.iter().map(|e| vers_i16(*e)).collect()
    }

    fn chaine(reglages: ReglagesMicro) -> ChaineMicro {
        ChaineMicro::nouvelle(reglages, None, Arc::new(MesureMicro::default()))
    }

    fn sans_bruit() -> ReglagesMicro {
        ReglagesMicro {
            bruit: ModeBruit::Aucun,
            ..ReglagesMicro::default()
        }
    }

    #[test]
    fn niveau_du_silence() {
        assert_eq!(niveau_db(&[]), SILENCE_DB);
        assert_eq!(niveau_db(&[0.0; 512]), SILENCE_DB);
    }

    #[test]
    fn niveau_d_une_sinusoide_pleine_echelle_vers_moins_3_dbfs() {
        assert!((niveau_db(&sinus(1.0, 440.0, 4_800)) + 3.01).abs() < 0.05);
    }

    #[test]
    fn une_amplitude_dix_fois_plus_faible_vaut_20_db_de_moins() {
        let fort = sinus(0.5, 440.0, 4_800);
        let faible: Vec<f32> = fort.iter().map(|e| e / 10.0).collect();
        assert!((niveau_db(&fort) - niveau_db(&faible) - 20.0).abs() < 0.05);
    }

    #[test]
    fn la_porte_suit_le_push_to_talk_puis_la_detection_de_voix() {
        assert!(porte_ouverte(true, true, true, false));
        assert!(!porte_ouverte(true, false, false, true));
        assert!(porte_ouverte(false, true, false, true));
        assert!(!porte_ouverte(false, true, true, false));
        assert!(porte_ouverte(false, false, false, false));
    }

    #[test]
    fn la_detection_de_voix_maintient_400_ms_apres_le_dernier_depassement() {
        let mut d = DetectionVoix::default();
        assert!(!d.mesurer(-50.0, -45.0));
        assert!(!d.mesurer(-45.0, -45.0), "le seuil lui-même ne suffit pas");
        assert!(d.mesurer(-30.0, -45.0));
        for _ in 0..39 {
            assert!(d.mesurer(-80.0, -45.0), "pas de coupure entre deux mots");
        }
        assert!(!d.mesurer(-80.0, -45.0));
        assert!(d.mesurer(-44.0, -45.0));
        d.reinitialiser();
        assert!(!d.active());
    }

    #[test]
    fn le_gain_multiplie_le_signal_et_borne_sans_deborder() {
        let mut c = chaine(ReglagesMicro {
            gain: 200,
            ..sans_bruit()
        });
        let trame = en_i16(&[0.25; ECHANTILLONS_PAR_TRAME]);
        assert!(c.traiter(&trame).iter().all(|e| *e == vers_i16(0.5)));
        c.appliquer(ReglagesMicro {
            gain: 1_000,
            ..sans_bruit()
        });
        assert_eq!(c.reglages().gain, GAIN_MICRO_MAXIMAL);
        let fort = en_i16(&[0.5; ECHANTILLONS_PAR_TRAME]);
        assert!(c.traiter(&fort).iter().all(|e| *e == i16::MAX));
        c.appliquer(ReglagesMicro {
            gain: 0,
            ..sans_bruit()
        });
        assert!(c.traiter(&fort).iter().all(|e| *e == 0));
    }

    #[test]
    fn le_niveau_est_mesure_avant_le_gain_et_publie() {
        let mesure = Arc::new(MesureMicro::default());
        let mut c = ChaineMicro::nouvelle(
            ReglagesMicro {
                gain: 300,
                ..sans_bruit()
            },
            None,
            mesure.clone(),
        );
        assert_eq!(mesure.lire(), NiveauMicro::default());
        c.traiter(&en_i16(&sinus(0.1, 440.0, ECHANTILLONS_PAR_TRAME)));
        let n = mesure.lire();
        assert!((n.db + 23.0).abs() < 0.2, "{n:?}");
        mesure.reinitialiser();
        assert_eq!(mesure.lire().db, SILENCE_DB);
    }

    #[test]
    fn porte_fermee_le_micro_envoie_du_silence_mais_reste_mesure() {
        let mesure = Arc::new(MesureMicro::default());
        let mut c = ChaineMicro::nouvelle(
            ReglagesMicro {
                detection_voix: true,
                seuil: -30,
                ..sans_bruit()
            },
            None,
            mesure.clone(),
        );
        let faible = en_i16(&sinus(0.01, 440.0, ECHANTILLONS_PAR_TRAME));
        c.traiter(&faible);
        let sortie = c.traiter(&faible).to_vec();
        assert!(sortie.iter().all(|e| *e == 0));
        assert!(!mesure.lire().voix && mesure.lire().db > -45.0);
        let fort = en_i16(&sinus(0.5, 440.0, ECHANTILLONS_PAR_TRAME));
        let ouverte = c.traiter(&fort).to_vec();
        assert!(mesure.lire().voix);
        let attendu = en_i16(&sinus(0.5, 440.0, ECHANTILLONS_PAR_TRAME));
        assert!(ouverte[48..]
            .iter()
            .zip(&attendu[48..])
            .all(|(a, b)| (a - b).abs() <= 1));
        for _ in 0..40 {
            c.traiter(&faible);
        }
        assert!(c.traiter(&faible).iter().all(|e| *e == 0));
        c.appliquer(sans_bruit());
        let libre = c.traiter(&faible).to_vec();
        assert!(libre.iter().any(|e| *e != 0));
    }

    #[test]
    fn la_suppression_de_bruit_ne_s_applique_qu_en_mode_renforce() {
        let mut c = chaine(sans_bruit());
        assert!(c.debruiteur.is_none());
        c.appliquer(ReglagesMicro::default());
        assert!(c.debruiteur.is_some());
        let trame = [0i16; ECHANTILLONS_PAR_TRAME - 1];
        assert_eq!(c.traiter(&trame).len(), ECHANTILLONS_PAR_TRAME - 1);
    }

    struct MoteurFactice {
        captes: Arc<AtomicU32>,
        joues: Arc<AtomicU32>,
    }

    impl MoteurApm for MoteurFactice {
        fn capter(&mut self, trame: &mut [i16]) {
            self.captes.fetch_add(1, Ordering::SeqCst);
            trame.fill(0);
        }

        fn jouer(&mut self, _trame: &mut [i16]) {
            self.joues.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn l_apm_ne_sert_que_si_un_traitement_du_bruit_est_choisi() {
        let captes = Arc::new(AtomicU32::new(0));
        let joues = Arc::new(AtomicU32::new(0));
        let apm = Apm::nouveau(Box::new(MoteurFactice {
            captes: captes.clone(),
            joues: joues.clone(),
        }));
        let mut c = ChaineMicro::nouvelle(
            sans_bruit(),
            Some(apm.clone()),
            Arc::new(MesureMicro::default()),
        );
        let trame = en_i16(&[0.5; ECHANTILLONS_PAR_TRAME]);
        assert!(c.traiter(&trame).iter().all(|e| *e != 0));
        apm.jouer(&[0.0; ECHANTILLONS_PAR_TRAME]);
        assert_eq!(
            (captes.load(Ordering::SeqCst), joues.load(Ordering::SeqCst)),
            (0, 0)
        );
        c.appliquer(ReglagesMicro {
            bruit: ModeBruit::Standard,
            ..ReglagesMicro::default()
        });
        assert!(c.traiter(&trame).iter().all(|e| *e == 0));
        apm.jouer(&[0.0; ECHANTILLONS_PAR_TRAME]);
        apm.jouer(&[0.0; 10]);
        assert_eq!(
            (captes.load(Ordering::SeqCst), joues.load(Ordering::SeqCst)),
            (1, 1)
        );
    }

    #[test]
    fn les_reglages_sont_bornes_et_relus_depuis_le_json() {
        let r = ReglagesMicro {
            gain: 900,
            seuil: -200,
            ..ReglagesMicro::default()
        }
        .borne();
        assert_eq!((r.gain, r.seuil), (GAIN_MICRO_MAXIMAL, SEUIL_MINIMAL));
        let json = serde_json::to_string(&ReglagesMicro::default()).unwrap();
        assert_eq!(
            json,
            r#"{"bruit":"renforce","gain":100,"detection_voix":false,"seuil":-45}"#
        );
        let partiel: ReglagesMicro = serde_json::from_str(r#"{"bruit":"aucun"}"#).unwrap();
        assert_eq!(partiel, sans_bruit());
    }

    fn lire_wav(octets: &[u8]) -> (u32, Vec<f32>) {
        assert_eq!(&octets[..4], b"RIFF");
        let mut position = 12;
        let mut frequence = 0;
        while position + 8 <= octets.len() {
            let id = &octets[position..position + 4];
            let taille =
                u32::from_le_bytes(octets[position + 4..position + 8].try_into().unwrap()) as usize;
            let corps = &octets[position + 8..(position + 8 + taille).min(octets.len())];
            if id == b"fmt " {
                assert_eq!(u16::from_le_bytes([corps[0], corps[1]]), 1);
                assert_eq!(u16::from_le_bytes([corps[2], corps[3]]), 1);
                frequence = u32::from_le_bytes(corps[4..8].try_into().unwrap());
            } else if id == b"data" {
                let echantillons = corps
                    .chunks_exact(2)
                    .map(|o| f32::from(i16::from_le_bytes([o[0], o[1]])) / PLEINE_ECHELLE)
                    .collect();
                return (frequence, echantillons);
            }
            position += 8 + taille + taille % 2;
        }
        panic!("fichier WAV sans données");
    }

    struct Alea(u64);

    impl Alea {
        fn suivant(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
        }
    }

    fn bruit_de_clavier(longueur: usize) -> Vec<f32> {
        let mut alea = Alea(0x5eed_cafe_f00d);
        let mut rose = [0.0f32; 3];
        let mut clic = 0.0f32;
        (0..longueur)
            .map(|i| {
                let blanc = alea.suivant();
                rose[0] = 0.997 * rose[0] + 0.029 * blanc;
                rose[1] = 0.985 * rose[1] + 0.032 * blanc;
                rose[2] = 0.950 * rose[2] + 0.048 * blanc;
                let fond = (rose[0] + rose[1] + rose[2] + 0.02 * blanc) * 0.025;
                if i % 7_919 == 0 {
                    clic = 0.06;
                }
                clic *= 0.97;
                fond + clic * alea.suivant()
            })
            .collect()
    }

    fn energie_db(signal: &[f32]) -> f32 {
        niveau_db(signal)
    }

    fn voix_et_bruit() -> (Vec<f32>, Vec<f32>) {
        let (frequence, voix16) = lire_wav(include_bytes!("../../tests/donnees/voix-16k.wav"));
        let mut voix = Vec::new();
        Reechantillonneur::nouveau(frequence, FREQUENCE).traiter(&voix16, &mut voix);
        voix.truncate(voix.len() / ECHANTILLONS_PAR_TRAME * ECHANTILLONS_PAR_TRAME);
        let bruit = bruit_de_clavier(voix.len());
        (voix, bruit)
    }

    fn traiter_tout(c: &mut ChaineMicro, signal: &[f32]) -> Vec<f32> {
        let mut sortie = Vec::with_capacity(signal.len());
        for trame in signal.chunks_exact(ECHANTILLONS_PAR_TRAME) {
            let traitee = c.traiter(&en_i16(trame));
            sortie.extend(traitee.iter().map(|e| f32::from(*e) / PLEINE_ECHELLE));
        }
        sortie
    }

    fn trames_de_parole(voix: &[f32]) -> Vec<bool> {
        voix.chunks_exact(ECHANTILLONS_PAR_TRAME)
            .map(|t| energie_db(t) > -50.0)
            .collect()
    }

    fn hors_parole(parole: &[bool], signal: &[f32], marge: usize) -> Vec<f32> {
        let mut extrait = Vec::new();
        for (i, trame) in signal.chunks_exact(ECHANTILLONS_PAR_TRAME).enumerate() {
            let debut = i.saturating_sub(marge);
            let fin = (i + marge + 1).min(parole.len());
            if i >= 30 && !parole[debut..fin].iter().any(|p| *p) {
                extrait.extend_from_slice(trame);
            }
        }
        extrait
    }

    fn attaques(parole: &[bool]) -> Vec<usize> {
        (15..parole.len())
            .filter(|&i| parole[i] && parole[i - 15..i].iter().all(|p| !p))
            .collect()
    }

    #[test]
    fn le_mode_renforce_baisse_le_bruit_de_15_db_hors_parole_sans_couper_les_attaques() {
        let (voix, bruit) = voix_et_bruit();
        let melange: Vec<f32> = voix.iter().zip(&bruit).map(|(v, b)| v + b).collect();
        let parole = trames_de_parole(&voix);
        let debuts = attaques(&parole);
        assert!(debuts.len() >= 3, "{debuts:?}");
        let mut c = chaine(ReglagesMicro::default());
        let sortie = traiter_tout(&mut c, &melange);
        let avant = energie_db(&hors_parole(&parole, &melange, 5));
        let apres = energie_db(&hors_parole(&parole, &sortie, 5));
        assert!(
            avant - apres >= 15.0,
            "bruit hors parole : {avant:.1} dB avant, {apres:.1} dB après"
        );
        let retard = 1;
        for debut in debuts {
            let fenetre = |s: &[f32], decalage: usize| {
                let a = (debut + decalage) * ECHANTILLONS_PAR_TRAME;
                energie_db(&s[a..a + 5 * ECHANTILLONS_PAR_TRAME])
            };
            let propre = fenetre(&voix, 0);
            let traite = fenetre(&sortie, retard);
            assert!(
                traite >= propre - 6.0,
                "attaque à la trame {debut} : {propre:.1} dB attendus, {traite:.1} dB obtenus"
            );
        }
    }

    #[test]
    fn la_detection_de_voix_ouvre_la_porte_des_la_premiere_trame_de_parole() {
        let (voix, bruit) = voix_et_bruit();
        let melange: Vec<f32> = voix.iter().zip(&bruit).map(|(v, b)| v + b).collect();
        let parole = trames_de_parole(&voix);
        let mesure = Arc::new(MesureMicro::default());
        let mut c = ChaineMicro::nouvelle(
            ReglagesMicro {
                detection_voix: true,
                ..ReglagesMicro::default()
            },
            None,
            mesure.clone(),
        );
        let mut ouvertes = Vec::new();
        for trame in melange.chunks_exact(ECHANTILLONS_PAR_TRAME) {
            c.traiter(&en_i16(trame));
            ouvertes.push(mesure.lire().voix);
        }
        for debut in attaques(&parole) {
            assert!(
                ouvertes[debut..=debut + 2].iter().any(|o| *o),
                "porte fermée sur l'attaque de la trame {debut}"
            );
        }
        let fausses = ouvertes[..30].iter().filter(|o| **o).count();
        assert_eq!(fausses, 0, "le bruit seul ouvre la porte");
    }

    #[test]
    #[ignore = "mesure du coût, à lancer en release : cargo test --release -- --ignored --nocapture cout"]
    fn mesure_le_cout_du_traitement_du_micro() {
        let (voix, bruit) = voix_et_bruit();
        let melange: Vec<i16> = en_i16(
            &voix
                .iter()
                .zip(&bruit)
                .map(|(v, b)| v + b)
                .collect::<Vec<_>>(),
        );
        let mut c = chaine(ReglagesMicro {
            detection_voix: true,
            ..ReglagesMicro::default()
        });
        let tours = 20;
        let debut = std::time::Instant::now();
        for _ in 0..tours {
            for trame in melange.chunks_exact(ECHANTILLONS_PAR_TRAME) {
                std::hint::black_box(c.traiter(trame));
            }
        }
        let calcul = debut.elapsed().as_secs_f64();
        let audio = (tours * melange.len()) as f64 / f64::from(FREQUENCE);
        println!(
            "chaîne du micro (renforcé, détection de voix) : {:.2} % d'un cœur ({calcul:.3} s pour {audio:.1} s de son)",
            100.0 * calcul / audio
        );
    }
}
