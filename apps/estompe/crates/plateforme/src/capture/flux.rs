use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::cadence::{Cadence, IPS_MAX};
use crate::traits::{EvenementCapture, Image, OptionsCapture, RecepteurCapture};
use crate::{Erreur, Resultat};

pub(crate) const PAS_MAX: Duration = Duration::from_millis(50);
pub(crate) const ATTENTE_REDUITE: Duration = Duration::from_millis(250);
pub(crate) const ECHECS_MAX: u32 = 10;

#[derive(Debug)]
pub(crate) struct Commande {
    arret: AtomicBool,
    ips: AtomicU32,
    curseur: AtomicBool,
}

impl Commande {
    pub(crate) fn nouvelle(options: OptionsCapture) -> Self {
        Self {
            arret: AtomicBool::new(false),
            ips: AtomicU32::new(options.ips.clamp(1, IPS_MAX)),
            curseur: AtomicBool::new(options.curseur),
        }
    }

    pub(crate) fn arretee(&self) -> bool {
        self.arret.load(Ordering::Relaxed)
    }

    pub(crate) fn ips(&self) -> u32 {
        self.ips.load(Ordering::Relaxed)
    }

    pub(crate) fn curseur(&self) -> bool {
        self.curseur.load(Ordering::Relaxed)
    }
}

pub(crate) struct Rythme {
    ips: u32,
    cadence: Cadence,
}

impl Rythme {
    pub(crate) fn nouveau(commande: &Commande) -> Self {
        let ips = commande.ips();
        Self {
            ips,
            cadence: Cadence::nouvelle(ips, Instant::now()),
        }
    }

    fn suivre(&mut self, commande: &Commande, t: Instant) {
        let ips = commande.ips();
        if ips != self.ips {
            self.ips = ips;
            self.cadence = Cadence::nouvelle(ips, t);
        }
    }

    pub(crate) fn attente(&mut self, commande: &Commande) -> Duration {
        let t = Instant::now();
        self.suivre(commande, t);
        self.cadence.attente(t)
    }

    pub(crate) fn due(&mut self, commande: &Commande) -> bool {
        let t = Instant::now();
        self.suivre(commande, t);
        self.cadence.due(t)
    }
}

pub struct FluxCapture {
    source: String,
    commande: Arc<Commande>,
    fil: Option<JoinHandle<()>>,
}

impl FluxCapture {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn ips(&self) -> u32 {
        self.commande.ips()
    }

    pub fn definir_cadence(&self, ips: u32) {
        self.commande
            .ips
            .store(ips.clamp(1, IPS_MAX), Ordering::Relaxed);
    }

    pub fn definir_curseur(&self, curseur: bool) {
        self.commande.curseur.store(curseur, Ordering::Relaxed);
    }

    pub fn actif(&self) -> bool {
        self.fil.as_ref().is_some_and(|fil| !fil.is_finished())
    }

    pub fn arreter(self) {}

    pub(crate) fn commande(&self) -> Arc<Commande> {
        self.commande.clone()
    }
}

impl Drop for FluxCapture {
    fn drop(&mut self) {
        self.commande.arret.store(true, Ordering::Relaxed);
        if let Some(fil) = self.fil.take() {
            if fil.thread().id() != std::thread::current().id() {
                let _ = fil.join();
            }
        }
    }
}

impl std::fmt::Debug for FluxCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FluxCapture")
            .field("source", &self.source)
            .field("ips", &self.ips())
            .finish()
    }
}

pub(crate) fn lancer(
    source: &str,
    options: OptionsCapture,
    mut recepteur: RecepteurCapture,
    corps: impl FnOnce(&Commande, &mut RecepteurCapture) + Send + 'static,
) -> Resultat<FluxCapture> {
    let commande = Arc::new(Commande::nouvelle(options));
    let partagee = commande.clone();
    let fil = std::thread::Builder::new()
        .name("estompe-capture".into())
        .spawn(move || corps(&partagee, &mut recepteur))
        .map_err(|e| Erreur::Echec(e.to_string()))?;
    Ok(FluxCapture {
        source: source.to_string(),
        commande,
        fil: Some(fil),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Releve {
    Image(Image),
    Reduite,
    Fermee,
    Echec(String),
}

#[derive(Default)]
pub(crate) struct Suivi {
    reduite: bool,
    echecs: u32,
}

impl Suivi {
    pub(crate) fn transmettre(
        &mut self,
        releve: Releve,
        recepteur: &mut RecepteurCapture,
    ) -> Option<Duration> {
        match releve {
            Releve::Image(image) => {
                self.echecs = 0;
                if std::mem::take(&mut self.reduite) && !recepteur(EvenementCapture::Restauree) {
                    return None;
                }
                recepteur(EvenementCapture::Trame(image)).then_some(Duration::ZERO)
            }
            Releve::Reduite => {
                self.echecs = 0;
                if !std::mem::replace(&mut self.reduite, true)
                    && !recepteur(EvenementCapture::Reduite)
                {
                    return None;
                }
                Some(ATTENTE_REDUITE)
            }
            Releve::Fermee => {
                recepteur(EvenementCapture::Fermee);
                None
            }
            Releve::Echec(raison) => {
                self.echecs += 1;
                if self.echecs >= ECHECS_MAX {
                    recepteur(EvenementCapture::Echec(raison));
                    return None;
                }
                Some(ATTENTE_REDUITE)
            }
        }
    }
}

pub(crate) fn sonder(
    commande: &Commande,
    recepteur: &mut RecepteurCapture,
    mut lire: impl FnMut(bool) -> Releve,
) {
    let mut rythme = Rythme::nouveau(commande);
    let mut suivi = Suivi::default();
    while !commande.arretee() {
        let attente = rythme.attente(commande);
        if !attente.is_zero() {
            std::thread::sleep(attente.min(PAS_MAX));
            continue;
        }
        if !rythme.due(commande) {
            continue;
        }
        match suivi.transmettre(lire(commande.curseur()), recepteur) {
            None => break,
            Some(pause) if !pause.is_zero() => std::thread::sleep(pause),
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::Mutex;

    fn image(valeur: u8) -> Image {
        Image {
            largeur: 1,
            hauteur: 1,
            rgba: vec![valeur, valeur, valeur, 255],
        }
    }

    fn recepteur() -> (RecepteurCapture, mpsc::Receiver<EvenementCapture>) {
        let (envoi, reception) = mpsc::channel();
        (
            Box::new(move |evenement| envoi.send(evenement).is_ok()),
            reception,
        )
    }

    fn compter_trames(reception: &mpsc::Receiver<EvenementCapture>) -> usize {
        reception
            .try_iter()
            .filter(|e| matches!(e, EvenementCapture::Trame(_)))
            .count()
    }

    #[test]
    fn le_flux_suit_la_cadence_demandee_puis_la_nouvelle() {
        let (recepteur, reception) = recepteur();
        let options = OptionsCapture {
            ips: 50,
            curseur: false,
        };
        let flux = lancer("ecran:0", options, recepteur, |commande, recepteur| {
            sonder(commande, recepteur, |_| Releve::Image(image(1)))
        })
        .unwrap();
        assert!(flux.actif());
        assert_eq!(flux.source(), "ecran:0");
        std::thread::sleep(Duration::from_millis(500));
        let a_50 = compter_trames(&reception);
        assert!(
            (18..=32).contains(&a_50),
            "{a_50} images en 500 ms à 50 i/s"
        );
        flux.definir_cadence(10);
        assert_eq!(flux.ips(), 10);
        std::thread::sleep(Duration::from_millis(60));
        let _ = compter_trames(&reception);
        std::thread::sleep(Duration::from_millis(500));
        let a_10 = compter_trames(&reception);
        assert!((3..=7).contains(&a_10), "{a_10} images en 500 ms à 10 i/s");
        flux.arreter();
        let _ = compter_trames(&reception);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(compter_trames(&reception), 0, "plus rien après l'arrêt");
    }

    #[test]
    fn le_curseur_se_change_pendant_le_flux() {
        let (recepteur, reception) = recepteur();
        let vus = Arc::new(Mutex::new(Vec::new()));
        let notes = vus.clone();
        let flux = lancer(
            "fenetre:1",
            OptionsCapture {
                ips: 100,
                curseur: true,
            },
            recepteur,
            move |commande, recepteur| {
                sonder(commande, recepteur, |curseur| {
                    notes.lock().unwrap().push(curseur);
                    Releve::Image(image(u8::from(curseur)))
                })
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        flux.definir_curseur(false);
        std::thread::sleep(Duration::from_millis(100));
        drop(flux);
        let vus = vus.lock().unwrap();
        assert_eq!(vus.first(), Some(&true));
        assert_eq!(vus.last(), Some(&false));
        assert!(reception.try_iter().count() >= 2);
    }

    #[test]
    fn une_fenetre_reduite_puis_fermee_est_signalee_une_fois() {
        let (recepteur, reception) = recepteur();
        let mut releves = vec![
            Releve::Image(image(1)),
            Releve::Reduite,
            Releve::Reduite,
            Releve::Reduite,
            Releve::Image(image(2)),
            Releve::Fermee,
        ]
        .into_iter();
        let flux = lancer(
            "fenetre:2",
            OptionsCapture {
                ips: 240,
                curseur: false,
            },
            recepteur,
            move |commande, recepteur| {
                sonder(commande, recepteur, |_| {
                    releves.next().unwrap_or(Releve::Fermee)
                })
            },
        )
        .unwrap();
        let evenements: Vec<_> = reception.iter().collect();
        assert_eq!(
            evenements,
            [
                EvenementCapture::Trame(image(1)),
                EvenementCapture::Reduite,
                EvenementCapture::Restauree,
                EvenementCapture::Trame(image(2)),
                EvenementCapture::Fermee,
            ]
        );
        assert!(!flux.actif());
    }

    #[test]
    fn des_echecs_repetes_arretent_le_flux_mais_pas_un_echec_passager() {
        let mut suivi = Suivi::default();
        let (mut recepteur, reception) = recepteur();
        for _ in 0..ECHECS_MAX - 1 {
            assert!(suivi
                .transmettre(Releve::Echec("occupé".into()), &mut recepteur)
                .is_some());
        }
        assert_eq!(
            suivi.transmettre(Releve::Image(image(3)), &mut recepteur),
            Some(Duration::ZERO)
        );
        for _ in 0..ECHECS_MAX - 1 {
            suivi.transmettre(Releve::Echec("occupé".into()), &mut recepteur);
        }
        assert_eq!(
            suivi.transmettre(Releve::Echec("perdu".into()), &mut recepteur),
            None
        );
        assert_eq!(
            reception.try_iter().collect::<Vec<_>>(),
            [
                EvenementCapture::Trame(image(3)),
                EvenementCapture::Echec("perdu".into())
            ]
        );
    }

    #[test]
    fn le_recepteur_peut_arreter_le_flux_et_le_lacher_depuis_son_fil() {
        let gardien: Arc<Mutex<Option<FluxCapture>>> = Arc::new(Mutex::new(None));
        let dans_le_fil = gardien.clone();
        let mut recues = 0;
        let flux = lancer(
            "ecran:1",
            OptionsCapture {
                ips: 200,
                curseur: false,
            },
            Box::new(move |_| {
                recues += 1;
                let lache = recues >= 3 && dans_le_fil.lock().unwrap().take().is_some();
                !lache
            }),
            |commande, recepteur| sonder(commande, recepteur, |_| Releve::Image(image(0))),
        )
        .unwrap();
        *gardien.lock().unwrap() = Some(flux);
        let depart = Instant::now();
        while gardien.lock().unwrap().is_some() {
            assert!(depart.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
