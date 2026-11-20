use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::audio::{Apm, SourceLecture, TamponLecture, FREQUENCE};

use super::salle::SourceAudio;

pub const VOLUME_MAXIMAL: u16 = 300;
pub const VOLUME_GENERAL_MAXIMAL: u16 = 100;
pub const VOLUME_PAR_DEFAUT: u16 = 100;
pub const PLAFOND: f32 = 0.98;
pub const RELACHEMENT_LIMITEUR_MS: f32 = 100.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ecoute {
    pub volumes: BTreeMap<String, u16>,
    pub volumes_partage: BTreeMap<String, u16>,
    pub muets: BTreeSet<String>,
    pub muets_partage: BTreeSet<String>,
    pub general: u16,
}

impl Default for Ecoute {
    fn default() -> Self {
        Self {
            volumes: BTreeMap::new(),
            volumes_partage: BTreeMap::new(),
            muets: BTreeSet::new(),
            muets_partage: BTreeSet::new(),
            general: VOLUME_PAR_DEFAUT,
        }
    }
}

impl Ecoute {
    fn volumes_de(&self, source: SourceAudio) -> &BTreeMap<String, u16> {
        match source {
            SourceAudio::Micro => &self.volumes,
            SourceAudio::Partage => &self.volumes_partage,
        }
    }

    fn muets_de(&self, source: SourceAudio) -> &BTreeSet<String> {
        match source {
            SourceAudio::Micro => &self.muets,
            SourceAudio::Partage => &self.muets_partage,
        }
    }

    pub fn volume(&self, identite: &str, source: SourceAudio) -> u16 {
        self.volumes_de(source)
            .get(identite)
            .copied()
            .unwrap_or(VOLUME_PAR_DEFAUT)
    }

    pub fn muet(&self, identite: &str, source: SourceAudio) -> bool {
        self.muets_de(source).contains(identite)
    }

    pub fn definir_volume(&mut self, identite: &str, source: SourceAudio, pourcent: u16) {
        let volumes = match source {
            SourceAudio::Micro => &mut self.volumes,
            SourceAudio::Partage => &mut self.volumes_partage,
        };
        let pourcent = pourcent.min(VOLUME_MAXIMAL);
        if pourcent == VOLUME_PAR_DEFAUT {
            volumes.remove(identite);
        } else {
            volumes.insert(identite.to_string(), pourcent);
        }
    }

    pub fn definir_muet(&mut self, identite: &str, source: SourceAudio, muet: bool) {
        let muets = match source {
            SourceAudio::Micro => &mut self.muets,
            SourceAudio::Partage => &mut self.muets_partage,
        };
        if muet {
            muets.insert(identite.to_string());
        } else {
            muets.remove(identite);
        }
    }

    pub fn definir_general(&mut self, pourcent: u16) {
        self.general = pourcent.min(VOLUME_GENERAL_MAXIMAL);
    }

    pub fn gain(&self, sourd: bool, identite: &str, source: SourceAudio) -> f32 {
        if sourd || self.muet(identite, source) {
            return 0.0;
        }
        f32::from(self.volume(identite, source)) / 100.0 * f32::from(self.general) / 100.0
    }

    pub fn bornee(mut self) -> Self {
        for volume in self
            .volumes
            .values_mut()
            .chain(self.volumes_partage.values_mut())
        {
            *volume = (*volume).min(VOLUME_MAXIMAL);
        }
        self.general = self.general.min(VOLUME_GENERAL_MAXIMAL);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Limiteur {
    enveloppe: f32,
    relachement: f32,
}

impl Default for Limiteur {
    fn default() -> Self {
        Self {
            enveloppe: 0.0,
            relachement: (-1000.0 / (RELACHEMENT_LIMITEUR_MS * FREQUENCE as f32)).exp(),
        }
    }
}

impl Limiteur {
    pub fn traiter(&mut self, signal: &mut [f32]) {
        for echantillon in signal {
            let amplitude = echantillon.abs();
            self.enveloppe = if amplitude > self.enveloppe {
                amplitude
            } else {
                amplitude + (self.enveloppe - amplitude) * self.relachement
            };
            if self.enveloppe > PLAFOND {
                *echantillon *= PLAFOND / self.enveloppe;
            }
        }
    }
}

#[derive(Debug, Default)]
struct Piste {
    tampon: TamponLecture,
    gain: f32,
}

#[derive(Default)]
pub struct Mixeur {
    pistes: BTreeMap<(String, SourceAudio), Piste>,
    ecoute: Ecoute,
    sourd: bool,
    limiteur: Limiteur,
    apm: Option<Arc<Apm>>,
}

impl Mixeur {
    pub fn nouveau(ecoute: Ecoute, apm: Option<Arc<Apm>>) -> Self {
        Self {
            ecoute,
            apm,
            ..Self::default()
        }
    }

    fn recalculer(&mut self) {
        for ((identite, source), piste) in &mut self.pistes {
            piste.gain = self.ecoute.gain(self.sourd, identite, *source);
        }
    }

    pub fn ajouter(&mut self, identite: &str, source: SourceAudio) {
        let gain = self.ecoute.gain(self.sourd, identite, source);
        let piste = self
            .pistes
            .entry((identite.to_string(), source))
            .or_default();
        piste.tampon.vider();
        piste.gain = gain;
    }

    pub fn retirer(&mut self, identite: &str, source: SourceAudio) {
        self.pistes.remove(&(identite.to_string(), source));
    }

    pub fn vider(&mut self) {
        self.pistes.clear();
    }

    pub fn pousser(&mut self, identite: &str, source: SourceAudio, trame: &[i16]) {
        if let Some(piste) = self.pistes.get_mut(&(identite.to_string(), source)) {
            piste.tampon.pousser(trame);
        }
    }

    pub fn definir_ecoute(&mut self, ecoute: Ecoute) {
        self.ecoute = ecoute;
        self.recalculer();
    }

    pub fn definir_sourd(&mut self, sourd: bool) {
        self.sourd = sourd;
        self.recalculer();
    }

    pub fn gain(&self, identite: &str, source: SourceAudio) -> f32 {
        self.ecoute.gain(self.sourd, identite, source)
    }

    pub fn pistes(&self) -> Vec<(String, SourceAudio)> {
        self.pistes.keys().cloned().collect()
    }
}

impl SourceLecture for Mixeur {
    fn lire(&mut self, sortie: &mut [f32]) {
        sortie.fill(0.0);
        for piste in self.pistes.values_mut() {
            piste.tampon.ajouter_a(sortie, piste.gain);
        }
        self.limiteur.traiter(sortie);
        if let Some(apm) = &self.apm {
            apm.jouer(sortie);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::audio::{MoteurApm, ECHANTILLONS_PAR_TRAME};

    fn lire(m: &mut Mixeur) -> Vec<f32> {
        let mut sortie = vec![0.0; ECHANTILLONS_PAR_TRAME];
        m.lire(&mut sortie);
        sortie
    }

    fn ecoute(voix: &[(&str, u16)], muets: &[&str]) -> Ecoute {
        let mut e = Ecoute::default();
        for (identite, volume) in voix {
            e.definir_volume(identite, SourceAudio::Micro, *volume);
        }
        for identite in muets {
            e.definir_muet(identite, SourceAudio::Micro, true);
        }
        e
    }

    #[test]
    fn applique_a_chaque_piste_son_propre_volume() {
        let mut e = ecoute(&[("a", 50)], &[]);
        e.definir_volume("a", SourceAudio::Partage, 200);
        assert_eq!(e.gain(false, "a", SourceAudio::Micro), 0.5);
        assert_eq!(e.gain(false, "a", SourceAudio::Partage), 2.0);
    }

    #[test]
    fn couper_le_partage_laisse_la_voix() {
        let mut e = ecoute(&[("a", 50)], &[]);
        e.definir_volume("a", SourceAudio::Partage, 200);
        e.definir_muet("a", SourceAudio::Partage, true);
        assert_eq!(e.gain(false, "a", SourceAudio::Partage), 0.0);
        assert_eq!(e.gain(false, "a", SourceAudio::Micro), 0.5);
        assert_eq!(e.volume("a", SourceAudio::Partage), 200);
    }

    #[test]
    fn cent_pour_cent_par_defaut_sans_effet_sur_les_autres_et_la_sourdine_coupe_tout() {
        let e = ecoute(&[("a", 50)], &[]);
        assert_eq!(e.gain(false, "b", SourceAudio::Micro), 1.0);
        assert_eq!(e.gain(true, "a", SourceAudio::Micro), 0.0);
        assert_eq!(e.gain(true, "b", SourceAudio::Partage), 0.0);
    }

    #[test]
    fn le_muet_garde_le_volume_et_le_volume_general_s_applique_a_tous() {
        let mut e = ecoute(&[("a", 300)], &["a"]);
        assert_eq!(e.gain(false, "a", SourceAudio::Micro), 0.0);
        e.definir_muet("a", SourceAudio::Micro, false);
        e.definir_general(50);
        assert_eq!(e.gain(false, "a", SourceAudio::Micro), 1.5);
        assert_eq!(e.gain(false, "b", SourceAudio::Micro), 0.5);
        e.definir_volume("a", SourceAudio::Micro, 1_000);
        e.definir_general(250);
        assert_eq!(
            (e.volume("a", SourceAudio::Micro), e.general),
            (VOLUME_MAXIMAL, VOLUME_GENERAL_MAXIMAL)
        );
        e.definir_volume("a", SourceAudio::Micro, 100);
        assert!(e.volumes.is_empty());
    }

    #[test]
    fn l_ecoute_relue_est_bornee_et_tolere_les_champs_manquants() {
        let e: Ecoute =
            serde_json::from_str(r#"{"volumes":{"a":900},"muets":["b"],"general":400}"#).unwrap();
        let e = e.bornee();
        assert_eq!(e.volume("a", SourceAudio::Micro), VOLUME_MAXIMAL);
        assert!(e.muet("b", SourceAudio::Micro) && !e.muet("b", SourceAudio::Partage));
        assert_eq!(e.general, VOLUME_GENERAL_MAXIMAL);
        let vide: Ecoute = serde_json::from_str("{}").unwrap();
        assert_eq!(vide, Ecoute::default());
    }

    #[test]
    fn mixe_les_participants_avec_leur_gain_et_coupe_tout_en_sourdine() {
        let mut m = Mixeur::nouveau(ecoute(&[("b", 200)], &[]), None);
        m.ajouter("a", SourceAudio::Micro);
        m.ajouter("b", SourceAudio::Micro);
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
        m.definir_sourd(true);
        assert!(lire(&mut m).iter().all(|e| *e == 0.0));
        m.definir_sourd(false);
        m.definir_ecoute(ecoute(&[], &["a"]));
        assert_eq!(m.gain("a", SourceAudio::Micro), 0.0);
        m.retirer("a", SourceAudio::Micro);
        assert_eq!(m.pistes(), vec![("b".to_string(), SourceAudio::Micro)]);
    }

    #[test]
    fn le_volume_reste_quand_la_piste_part_puis_revient() {
        let mut m = Mixeur::nouveau(ecoute(&[("a", 40)], &[]), None);
        m.ajouter("a", SourceAudio::Micro);
        m.vider();
        m.ajouter("a", SourceAudio::Micro);
        for _ in 0..2 {
            m.pousser("a", SourceAudio::Micro, &[16_384; ECHANTILLONS_PAR_TRAME]);
        }
        assert!((lire(&mut m)[0] - 0.2).abs() < 1e-3);
    }

    fn sinus(longueur: usize) -> Vec<i16> {
        (0..longueur)
            .map(|i| {
                let t = 2.0 * std::f32::consts::PI * 440.0 * i as f32 / FREQUENCE as f32;
                (t.sin() * f32::from(i16::MAX)) as i16
            })
            .collect()
    }

    #[test]
    fn dix_pistes_a_pleine_echelle_sans_ecretage_dur() {
        let mut m = Mixeur::default();
        let trames = 50;
        let signal = sinus(trames * ECHANTILLONS_PAR_TRAME);
        let identites: Vec<String> = (0..10).map(|i| format!("p{i}")).collect();
        for identite in &identites {
            m.ajouter(identite, SourceAudio::Micro);
        }
        let mut sortie = Vec::new();
        for trame in signal.chunks_exact(ECHANTILLONS_PAR_TRAME) {
            for identite in &identites {
                m.pousser(identite, SourceAudio::Micro, trame);
            }
            sortie.extend(lire(&mut m));
        }
        let amorce = ECHANTILLONS_PAR_TRAME;
        let reference: Vec<f32> = signal[..signal.len() - amorce]
            .iter()
            .map(|e| f32::from(*e) / f32::from(i16::MAX))
            .collect();
        let utile = &sortie[amorce..];
        assert!(utile.iter().all(|e| e.abs() <= PLAFOND + 1e-6));
        let haut =
            |s: &mut dyn Iterator<Item = f32>| s.filter(|e| e.abs() >= 0.9 * PLAFOND).count();
        let sortie_haute = haut(&mut utile.iter().copied());
        let sinus_haut = haut(&mut reference.iter().map(|r| r * PLAFOND));
        assert!(
            sortie_haute <= sinus_haut + utile.len() / 20,
            "{sortie_haute} échantillons près du plafond contre {sinus_haut} pour le sinus seul : écrêtage dur"
        );
        let regime = &utile[10 * ECHANTILLONS_PAR_TRAME..];
        let attendu = &reference[10 * ECHANTILLONS_PAR_TRAME..];
        let ecart = regime
            .iter()
            .zip(attendu)
            .map(|(s, r)| (s - r * PLAFOND).abs())
            .fold(0.0f32, f32::max);
        assert!(ecart < 0.05, "forme du signal déformée : écart {ecart}");
        let crete = regime.iter().fold(0.0f32, |a, e| a.max(e.abs()));
        assert!(crete > 0.9, "volume écrasé : crête {crete}");
    }

    #[test]
    fn un_signal_sous_le_plafond_passe_sans_changement() {
        let mut m = Mixeur::default();
        m.ajouter("a", SourceAudio::Micro);
        m.ajouter("b", SourceAudio::Micro);
        for _ in 0..3 {
            m.pousser("a", SourceAudio::Micro, &[9_830; ECHANTILLONS_PAR_TRAME]);
            m.pousser("b", SourceAudio::Micro, &[9_830; ECHANTILLONS_PAR_TRAME]);
        }
        let attendu = 2.0 * 9_830.0 / f32::from(i16::MAX);
        assert!(lire(&mut m).iter().all(|e| *e == attendu));
    }

    #[test]
    fn le_mixage_n_ajoute_aucune_latence_au_tampon_de_reception() {
        let mut m = Mixeur::nouveau(ecoute(&[("a", 300)], &[]), None);
        m.ajouter("a", SourceAudio::Micro);
        m.ajouter("b", SourceAudio::Micro);
        let mut seul = TamponLecture::default();
        let mut impulsion = vec![0i16; ECHANTILLONS_PAR_TRAME];
        impulsion[123] = 8_000;
        let silence = vec![0i16; ECHANTILLONS_PAR_TRAME];
        let position = |s: &[f32]| s.iter().position(|e| *e != 0.0);
        let mut dans_le_mixeur = None;
        let mut dans_le_tampon = None;
        for i in 0..6 {
            let trame = if i == 1 { &impulsion } else { &silence };
            m.pousser("a", SourceAudio::Micro, trame);
            m.pousser("b", SourceAudio::Micro, &silence);
            seul.pousser(trame);
            let mut a = vec![0.0; ECHANTILLONS_PAR_TRAME];
            seul.lire(&mut a);
            let b = lire(&mut m);
            dans_le_tampon =
                dans_le_tampon.or(position(&a).map(|p| i * ECHANTILLONS_PAR_TRAME + p));
            dans_le_mixeur =
                dans_le_mixeur.or(position(&b).map(|p| i * ECHANTILLONS_PAR_TRAME + p));
        }
        assert!(dans_le_tampon.is_some());
        assert_eq!(dans_le_mixeur, dans_le_tampon);
    }

    struct Reference(Arc<AtomicU32>);

    impl MoteurApm for Reference {
        fn capter(&mut self, _trame: &mut [i16]) {}

        fn jouer(&mut self, trame: &mut [i16]) {
            self.0.store(
                trame
                    .iter()
                    .map(|e| e.unsigned_abs() as u32)
                    .max()
                    .unwrap_or(0),
                Ordering::SeqCst,
            );
        }
    }

    #[test]
    fn le_son_joue_sert_de_reference_a_l_annulation_d_echo() {
        let crete = Arc::new(AtomicU32::new(0));
        let apm = Apm::nouveau(Box::new(Reference(crete.clone())));
        apm.activer(true);
        let mut m = Mixeur::nouveau(Ecoute::default(), Some(apm));
        m.ajouter("a", SourceAudio::Micro);
        for _ in 0..2 {
            m.pousser("a", SourceAudio::Micro, &[16_384; ECHANTILLONS_PAR_TRAME]);
        }
        lire(&mut m);
        assert_eq!(crete.load(Ordering::SeqCst), 16_384);
    }
}
