use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use estompe_client::lru::Lru;
use estompe_client::message_vocal::{
    forme_d_onde, niveau, Audio, Encodeur, Lecture, Reechantillonneur, BARRES, DUREE_MAX_MS,
    ECHANTILLONS_PAR_NIVEAU, TAUX,
};
use estompe_client::store::CleConversation;
use estompe_plateforme::{Erreur, FluxSon, Indisponible, Micro, SortieSon};

pub type MicroPartage = Arc<dyn Micro + Send + Sync>;
pub type SortiePartagee = Arc<dyn SortieSon + Send + Sync>;
pub type NoterEcoutes = Arc<dyn Fn(Vec<String>) + Send + Sync>;

pub const APERCU: &str = "apercu";
pub const AUDIOS_GARDES: usize = 8;
pub const NIVEAUX_AFFICHES: usize = 40;
pub const MICRO_REFUSE: &str =
    "Accès au micro refusé. Autorise Estompe à utiliser le micro dans les réglages de confidentialité du système.";
pub const MICRO_ABSENT: &str =
    "Aucun micro disponible : branche un micro ou choisis-en un dans les réglages du système.";
pub const MICRO_INUTILISABLE: &str = "Le micro est inutilisable.";
pub const ECHEC_D_ENVOI: &str = "Le message vocal n'a pas pu être envoyé";
pub const ILLISIBLE: &str = "Message vocal illisible sur cet appareil";

pub fn message_du_micro(erreur: &Erreur) -> &'static str {
    match erreur {
        Erreur::Refuse(_) => MICRO_REFUSE,
        Erreur::Indisponible(_) => MICRO_ABSENT,
        Erreur::Echec(_) => MICRO_INUTILISABLE,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prise {
    pub fichier: Vec<u8>,
    pub duree_ms: u64,
    pub forme: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suivi {
    pub niveaux: Vec<u8>,
    pub echantillons: u64,
}

impl Suivi {
    pub fn duree_ms(&self) -> u64 {
        self.echantillons * 1000 / u64::from(TAUX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Niveau,
    Limite,
    Fin(Result<Prise, String>),
}

pub fn ecouter(
    micro: &dyn Micro,
    suivi: Arc<Mutex<Suivi>>,
    signaler: impl Fn(Signal) + Send + 'static,
) -> Result<FluxSon, Erreur> {
    let (envoi, reception) = mpsc::channel::<(Vec<f32>, u32)>();
    let flux = micro.ecouter(Box::new(move |echantillons, taux| {
        let _ = envoi.send((echantillons.to_vec(), taux));
    }))?;
    std::thread::Builder::new()
        .name("estompe-vocal".into())
        .spawn(move || encoder(reception, suivi, signaler))
        .map_err(|e| Erreur::Echec(e.to_string()))?;
    Ok(flux)
}

fn encoder(
    reception: mpsc::Receiver<(Vec<f32>, u32)>,
    suivi: Arc<Mutex<Suivi>>,
    signaler: impl Fn(Signal),
) {
    let mut encodeur = match Encodeur::nouveau() {
        Ok(e) => e,
        Err(e) => return signaler(Signal::Fin(Err(e.to_string()))),
    };
    let maximum = u64::from(TAUX) * DUREE_MAX_MS / 1000;
    let mut reechantillonneur: Option<(u32, Reechantillonneur)> = None;
    let mut bloc = Vec::new();
    let mut converti = Vec::new();
    let mut limite = false;
    while let Ok((echantillons, taux)) = reception.recv() {
        if limite {
            continue;
        }
        if reechantillonneur.as_ref().map(|(t, _)| *t) != Some(taux) {
            reechantillonneur = Some((taux, Reechantillonneur::nouveau(taux, TAUX)));
        }
        converti.clear();
        if let Some((_, r)) = reechantillonneur.as_mut() {
            r.traiter(&echantillons, &mut converti);
        }
        converti.truncate(maximum.saturating_sub(encodeur.echantillons()) as usize);
        encodeur.pousser(&converti);
        bloc.extend_from_slice(&converti);
        let mut nouveaux = false;
        {
            let mut s = suivi.lock().unwrap_or_else(|e| e.into_inner());
            while bloc.len() >= ECHANTILLONS_PAR_NIVEAU {
                s.niveaux.push(niveau(&bloc[..ECHANTILLONS_PAR_NIVEAU]));
                bloc.drain(..ECHANTILLONS_PAR_NIVEAU);
                nouveaux = true;
            }
            s.echantillons = encodeur.echantillons();
        }
        if encodeur.echantillons() >= maximum {
            limite = true;
            signaler(Signal::Limite);
        } else if nouveaux {
            signaler(Signal::Niveau);
        }
    }
    let (duree_ms, forme) = {
        let mut s = suivi.lock().unwrap_or_else(|e| e.into_inner());
        if !bloc.is_empty() {
            s.niveaux.push(niveau(&bloc));
        }
        (s.duree_ms().max(1), forme_d_onde(&s.niveaux, BARRES))
    };
    signaler(Signal::Fin(Ok(Prise {
        fichier: encodeur.terminer(),
        duree_ms,
        forme,
    })));
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Etape {
    #[default]
    Aucune,
    Ouverture,
    Enregistrement,
    Finalisation {
        envoyer: bool,
    },
    Ecoute {
        echec: bool,
    },
    Envoi,
}

#[derive(Default)]
pub struct Enregistreur {
    pub etape: Etape,
    pub generation: u64,
    pub cle: Option<CleConversation>,
    pub flux: Option<FluxSon>,
    pub suivi: Arc<Mutex<Suivi>>,
    pub prise: Option<Arc<Prise>>,
}

impl Enregistreur {
    pub fn commencer(&mut self, cle: CleConversation) -> u64 {
        *self = Self {
            etape: Etape::Ouverture,
            generation: self.generation + 1,
            cle: Some(cle),
            ..Self::default()
        };
        self.generation
    }

    pub fn abandonner(&mut self) {
        *self = Self {
            generation: self.generation + 1,
            ..Self::default()
        };
    }

    pub fn niveaux_affiches(&self) -> Vec<f32> {
        let s = self.suivi.lock().unwrap_or_else(|e| e.into_inner());
        let debut = s.niveaux.len().saturating_sub(NIVEAUX_AFFICHES);
        s.niveaux[debut..]
            .iter()
            .map(|n| f32::from(*n) / 100.0)
            .collect()
    }

    pub fn duree_ms(&self) -> u64 {
        match &self.prise {
            Some(p) => p.duree_ms,
            None => self
                .suivi
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .duree_ms(),
        }
    }
}

pub struct Courant {
    pub piece: String,
    pub lecture: Option<Arc<Mutex<Lecture>>>,
    pub flux: Option<FluxSon>,
    pub jouer: bool,
    pub position_demandee: Option<f32>,
}

impl Courant {
    pub fn nouveau(piece: &str, jouer: bool, position_demandee: Option<f32>) -> Self {
        Self {
            piece: piece.to_string(),
            lecture: None,
            flux: None,
            jouer,
            position_demandee,
        }
    }
}

pub struct Lecteur {
    pub courant: Option<Courant>,
    pub vitesse: usize,
    pub audios: Lru<String, Arc<Audio>>,
}

impl Default for Lecteur {
    fn default() -> Self {
        Self {
            courant: None,
            vitesse: 0,
            audios: Lru::nouveau(AUDIOS_GARDES),
        }
    }
}

impl Lecteur {
    pub fn piece(&self) -> Option<&str> {
        self.courant.as_ref().map(|c| c.piece.as_str())
    }

    pub fn en_lecture(&self) -> bool {
        self.courant.as_ref().is_some_and(|c| c.flux.is_some())
    }

    pub fn chargement(&self) -> bool {
        self.courant
            .as_ref()
            .is_some_and(|c| c.jouer && c.lecture.is_none())
    }

    pub fn position(&self) -> Option<(u64, u64, bool)> {
        let c = self.courant.as_ref()?;
        let l = c
            .lecture
            .as_ref()?
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        Some((l.position_ms(), l.duree_ms(), l.finie()))
    }

    pub fn arreter(&mut self) {
        self.courant = None;
    }
}

#[derive(Clone)]
pub struct Vocal {
    pub enregistreur: Arc<Mutex<Enregistreur>>,
    pub lecteur: Arc<Mutex<Lecteur>>,
    pub micro: Arc<Mutex<MicroPartage>>,
    pub sortie: Arc<Mutex<SortiePartagee>>,
    ecoutes: Arc<Mutex<(Vec<String>, Option<NoterEcoutes>)>>,
}

impl Default for Vocal {
    fn default() -> Self {
        Self {
            enregistreur: Arc::default(),
            lecteur: Arc::default(),
            micro: Arc::new(Mutex::new(Arc::new(Indisponible))),
            sortie: Arc::new(Mutex::new(Arc::new(Indisponible))),
            ecoutes: Arc::default(),
        }
    }
}

impl Vocal {
    pub fn enregistreur(&self) -> std::sync::MutexGuard<'_, Enregistreur> {
        self.enregistreur.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn lecteur(&self) -> std::sync::MutexGuard<'_, Lecteur> {
        self.lecteur.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn definir_ecoutes(&self, ecoutes: Vec<String>, noter: NoterEcoutes) {
        *self.ecoutes.lock().unwrap_or_else(|e| e.into_inner()) = (ecoutes, Some(noter));
    }

    pub fn ecoute(&self, piece: &str) -> bool {
        self.ecoutes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .iter()
            .any(|e| e == piece)
    }

    pub fn marquer(&self, piece: &str) -> bool {
        let (liste, noter) = {
            let mut e = self.ecoutes.lock().unwrap_or_else(|e| e.into_inner());
            if e.0.iter().any(|x| x == piece) {
                return false;
            }
            estompe_client::message_vocal::marquer_ecoute(&mut e.0, piece);
            (e.0.clone(), e.1.clone())
        };
        if let Some(noter) = noter {
            noter(liste);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use estompe_client::message_vocal::decoder;
    use estompe_plateforme::RecepteurSon;

    use super::*;

    #[derive(Default)]
    struct MicroDeTest(Arc<Mutex<Option<RecepteurSon>>>);

    struct Garde(Arc<Mutex<Option<RecepteurSon>>>);

    impl Drop for Garde {
        fn drop(&mut self) {
            self.0.lock().unwrap().take();
        }
    }

    impl Micro for MicroDeTest {
        fn disponible(&self) -> bool {
            true
        }
        fn ecouter(&self, recepteur: RecepteurSon) -> Result<FluxSon, Erreur> {
            *self.0.lock().unwrap() = Some(recepteur);
            Ok(FluxSon::nouveau(Garde(self.0.clone())))
        }
    }

    fn sinus(taux: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / taux as f32).sin())
            .collect()
    }

    #[test]
    fn le_micro_a_44_1_khz_donne_un_ogg_opus_avec_sa_duree_et_sa_forme() {
        let micro = MicroDeTest::default();
        let suivi = Arc::new(Mutex::new(Suivi::default()));
        let (envoi, signaux) = mpsc::channel();
        let flux = ecouter(&micro, suivi.clone(), move |s| {
            let _ = envoi.send(s);
        })
        .unwrap();
        let signal = sinus(44_100, 44_100);
        for bloc in signal.chunks(441) {
            (micro.0.lock().unwrap().as_mut().unwrap())(bloc, 44_100);
        }
        drop(flux);
        let prise = loop {
            match signaux.recv().unwrap() {
                Signal::Fin(prise) => break prise.unwrap(),
                Signal::Niveau => {}
                Signal::Limite => panic!("limite atteinte trop tôt"),
            }
        };
        assert!(prise.duree_ms.abs_diff(1000) <= 2, "{}", prise.duree_ms);
        assert_eq!(prise.forme.len(), 10);
        assert!(
            prise.forme.iter().all(|n| (80..=95).contains(n)),
            "{:?}",
            prise.forme
        );
        let audio = decoder(&prise.fichier).unwrap();
        assert!(audio.duree_ms().abs_diff(1000) <= 2);
        assert_eq!(suivi.lock().unwrap().niveaux.len(), 10);
    }

    #[test]
    fn l_enregistrement_s_arrete_a_la_duree_maximale_du_front() {
        let micro = MicroDeTest::default();
        let suivi = Arc::new(Mutex::new(Suivi::default()));
        let (envoi, signaux) = mpsc::channel();
        let flux = ecouter(&micro, suivi.clone(), move |s| {
            let _ = envoi.send(s);
        })
        .unwrap();
        let seconde = vec![0.0f32; TAUX as usize];
        for _ in 0..DUREE_MAX_MS / 1000 + 2 {
            (micro.0.lock().unwrap().as_mut().unwrap())(&seconde, TAUX);
        }
        while signaux.recv().unwrap() != Signal::Limite {}
        drop(flux);
        let prise = loop {
            if let Signal::Fin(prise) = signaux.recv().unwrap() {
                break prise.unwrap();
            }
        };
        assert_eq!(prise.duree_ms, DUREE_MAX_MS);
        assert_eq!(prise.forme.len(), BARRES);
    }

    #[test]
    fn un_refus_du_micro_donne_un_message_clair() {
        assert_eq!(
            message_du_micro(&Erreur::Refuse("E_ACCESSDENIED".into())),
            MICRO_REFUSE
        );
        assert_eq!(
            message_du_micro(&Erreur::Indisponible("micro")),
            MICRO_ABSENT
        );
        assert_eq!(
            message_du_micro(&Erreur::Echec("x".into())),
            MICRO_INUTILISABLE
        );
    }
}
