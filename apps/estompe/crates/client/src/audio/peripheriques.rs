use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use estompe_plateforme::{
    Erreur, ErreurFluxAudio, FluxAudio, PeripheriqueAudio, Resultat, SensAudio, SignalErreurAudio,
    SystemeAudio,
};

use super::format::{ConversionEntree, ConversionSortie, SourceLecture};

pub type RenduTrame = Box<dyn FnMut(&[i16]) + Send>;

type Ouvreur = Box<
    dyn FnMut(&dyn SystemeAudio, Option<&str>, SignalErreurAudio) -> Resultat<Box<dyn FluxAudio>>
        + Send,
>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bascule {
    Choix(String),
    Repli(String),
    Retour(String),
    Perdu(String),
}

pub fn resoudre<'a>(
    liste: &'a [PeripheriqueAudio],
    choisi: Option<&str>,
) -> Option<&'a PeripheriqueAudio> {
    choisi
        .and_then(|id| liste.iter().find(|p| p.id == id))
        .or_else(|| liste.iter().find(|p| p.defaut))
        .or_else(|| liste.first())
}

struct Suivi {
    systeme: Arc<dyn SystemeAudio>,
    sens: SensAudio,
    choisi: Option<String>,
    flux: Option<Box<dyn FluxAudio>>,
    perdu: Arc<AtomicBool>,
    ouvreur: Ouvreur,
}

impl Suivi {
    fn nouveau(
        systeme: Arc<dyn SystemeAudio>,
        sens: SensAudio,
        choisi: Option<String>,
        ouvreur: Ouvreur,
    ) -> Self {
        Self {
            systeme,
            sens,
            choisi,
            flux: None,
            perdu: Arc::new(AtomicBool::new(false)),
            ouvreur,
        }
    }

    fn ouvrir(&mut self) -> Resultat<String> {
        self.flux = None;
        let liste = self.systeme.peripheriques(self.sens).unwrap_or_default();
        let present = self
            .choisi
            .clone()
            .filter(|id| liste.iter().any(|p| &p.id == id));
        let mut derniere = Erreur::Refuse("aucun périphérique audio".to_string());
        let essais = match present {
            Some(id) => vec![Some(id), None],
            None => vec![None],
        };
        for essai in essais {
            let perdu = Arc::new(AtomicBool::new(false));
            let signal = perdu.clone();
            let erreur: SignalErreurAudio = Box::new(move |e: ErreurFluxAudio| {
                if matches!(e, ErreurFluxAudio::Debranche | ErreurFluxAudio::Echec(_)) {
                    signal.store(true, Ordering::SeqCst);
                }
            });
            match (self.ouvreur)(self.systeme.as_ref(), essai.as_deref(), erreur) {
                Ok(flux) => {
                    let id = flux.peripherique().to_string();
                    self.flux = Some(flux);
                    self.perdu = perdu;
                    return Ok(id);
                }
                Err(e) => derniere = e,
            }
        }
        Err(derniere)
    }

    fn peripherique(&self) -> Option<&str> {
        self.flux.as_ref().map(|f| f.peripherique())
    }

    fn en_repli(&self) -> bool {
        match (&self.choisi, self.peripherique()) {
            (Some(choisi), Some(actuel)) => choisi != actuel,
            (Some(_), None) => true,
            _ => false,
        }
    }

    fn choisir(&mut self, id: Option<String>) -> Resultat<Bascule> {
        self.choisi = id;
        let ouvert = self.ouvrir()?;
        Ok(if self.en_repli() {
            Bascule::Repli(ouvert)
        } else {
            Bascule::Choix(ouvert)
        })
    }

    fn surveiller(&mut self) -> Option<Bascule> {
        let perdu = self.perdu.load(Ordering::SeqCst) || self.flux.is_none();
        if perdu {
            return Some(match self.ouvrir() {
                Ok(id) if self.en_repli() => Bascule::Repli(id),
                Ok(id) => Bascule::Retour(id),
                Err(e) => Bascule::Perdu(e.to_string()),
            });
        }
        if !self.en_repli() {
            return None;
        }
        let choisi = self.choisi.clone()?;
        let liste = self.systeme.peripheriques(self.sens).ok()?;
        if !liste.iter().any(|p| p.id == choisi) {
            return None;
        }
        match self.ouvrir() {
            Ok(id) if !self.en_repli() => Some(Bascule::Retour(id)),
            Ok(id) => Some(Bascule::Repli(id)),
            Err(e) => Some(Bascule::Perdu(e.to_string())),
        }
    }
}

pub struct Capture {
    suivi: Suivi,
}

impl Capture {
    pub fn ouvrir(
        systeme: Arc<dyn SystemeAudio>,
        choisi: Option<String>,
        rendu: RenduTrame,
    ) -> Resultat<Self> {
        let rendu = Arc::new(Mutex::new(rendu));
        let ouvreur: Ouvreur = Box::new(move |systeme, id, erreur| {
            let rendu = rendu.clone();
            let mut conversion: Option<ConversionEntree> = None;
            let format = Arc::new(Mutex::new(None));
            let format_flux = format.clone();
            let flux = systeme.ouvrir_entree(
                id,
                Box::new(move |echantillons| {
                    if conversion.is_none() {
                        conversion = format_flux
                            .lock()
                            .ok()
                            .and_then(|f| *f)
                            .map(ConversionEntree::nouvelle);
                    }
                    let (Some(conversion), Ok(mut rendu)) = (conversion.as_mut(), rendu.lock())
                    else {
                        return;
                    };
                    conversion.pousser(echantillons, |trame| rendu(trame));
                }),
                erreur,
            )?;
            if let Ok(mut f) = format.lock() {
                *f = Some(flux.format());
            }
            Ok(flux)
        });
        let mut suivi = Suivi::nouveau(systeme, SensAudio::Entree, choisi, ouvreur);
        suivi.ouvrir()?;
        Ok(Self { suivi })
    }

    pub fn peripherique(&self) -> Option<&str> {
        self.suivi.peripherique()
    }

    pub fn en_repli(&self) -> bool {
        self.suivi.en_repli()
    }

    pub fn choisir(&mut self, id: Option<String>) -> Resultat<Bascule> {
        self.suivi.choisir(id)
    }

    pub fn surveiller(&mut self) -> Option<Bascule> {
        self.suivi.surveiller()
    }
}

pub struct Lecture {
    suivi: Suivi,
}

impl Lecture {
    pub fn ouvrir(
        systeme: Arc<dyn SystemeAudio>,
        choisi: Option<String>,
        source: Arc<Mutex<dyn SourceLecture>>,
    ) -> Resultat<Self> {
        let ouvreur: Ouvreur = Box::new(move |systeme, id, erreur| {
            let source = source.clone();
            let mut conversion: Option<ConversionSortie> = None;
            let format = Arc::new(Mutex::new(None));
            let format_flux = format.clone();
            let flux = systeme.ouvrir_sortie(
                id,
                Box::new(move |sortie| {
                    if conversion.is_none() {
                        conversion = format_flux
                            .lock()
                            .ok()
                            .and_then(|f| *f)
                            .map(ConversionSortie::nouvelle);
                    }
                    let (Some(conversion), Ok(mut source)) = (conversion.as_mut(), source.lock())
                    else {
                        sortie.fill(0.0);
                        return;
                    };
                    conversion.remplir(&mut *source, sortie);
                }),
                erreur,
            )?;
            if let Ok(mut f) = format.lock() {
                *f = Some(flux.format());
            }
            Ok(flux)
        });
        let mut suivi = Suivi::nouveau(systeme, SensAudio::Sortie, choisi, ouvreur);
        suivi.ouvrir()?;
        Ok(Self { suivi })
    }

    pub fn peripherique(&self) -> Option<&str> {
        self.suivi.peripherique()
    }

    pub fn en_repli(&self) -> bool {
        self.suivi.en_repli()
    }

    pub fn choisir(&mut self, id: Option<String>) -> Resultat<Bascule> {
        self.suivi.choisir(id)
    }

    pub fn surveiller(&mut self) -> Option<Bascule> {
        self.suivi.surveiller()
    }
}
