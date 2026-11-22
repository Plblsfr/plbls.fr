use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use super::flux::{lancer, Commande, Releve, Suivi, ECHECS_MAX};
use super::{source_inconnue, FluxCapture};
use crate::traits::{CaptureEcran, Image, OptionsCapture, RecepteurCapture, SourceCapture};
use crate::{Erreur, Resultat};

struct FluxOuvert {
    source: String,
    envoi: mpsc::Sender<Releve>,
    commande: Arc<Commande>,
}

#[derive(Default)]
struct EtatFactice {
    sources: Vec<SourceCapture>,
    ouverts: Vec<FluxOuvert>,
    refus: Option<String>,
    numero: u8,
}

#[derive(Clone, Default)]
pub struct EcransFactices {
    etat: Arc<Mutex<EtatFactice>>,
}

pub fn image_unie(largeur: u32, hauteur: u32, rgba: [u8; 4]) -> Image {
    Image {
        largeur,
        hauteur,
        rgba: rgba.repeat((largeur * hauteur) as usize),
    }
}

impl EcransFactices {
    pub fn nouveaux() -> Self {
        Self::default()
    }

    fn etat(&self) -> MutexGuard<'_, EtatFactice> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn brancher(&self, source: SourceCapture) {
        let mut etat = self.etat();
        etat.sources.retain(|s| s.id != source.id);
        etat.sources.push(source);
    }

    pub fn refuser(&self, refus: Option<&str>) {
        self.etat().refus = refus.map(str::to_string);
    }

    fn envoyer(&self, source: &str, releve: impl Fn() -> Releve) -> usize {
        let mut etat = self.etat();
        etat.ouverts.retain(|f| !f.commande.arretee());
        etat.ouverts
            .iter()
            .filter(|f| f.source == source)
            .filter(|f| f.envoi.send(releve()).is_ok())
            .count()
    }

    pub fn retirer(&self, source: &str) {
        self.etat().sources.retain(|s| s.id != source);
        self.envoyer(source, || Releve::Fermee);
    }

    pub fn reduire(&self, source: &str) {
        self.envoyer(source, || Releve::Reduite);
    }

    pub fn echouer(&self, source: &str, raison: &str) {
        for _ in 0..ECHECS_MAX {
            self.envoyer(source, || Releve::Echec(raison.to_string()));
        }
    }

    pub fn pousser(&self, source: &str) -> usize {
        let (largeur, hauteur, valeur) = {
            let mut etat = self.etat();
            etat.numero = etat.numero.wrapping_add(1);
            let numero = etat.numero;
            let Some(s) = etat.sources.iter().find(|s| s.id == source) else {
                return 0;
            };
            (s.largeur, s.hauteur, numero)
        };
        self.envoyer(source, || {
            Releve::Image(image_unie(
                largeur,
                hauteur,
                [valeur, 128, 255 - valeur, 255],
            ))
        })
    }

    pub fn ouverts(&self) -> Vec<(String, u32)> {
        let mut etat = self.etat();
        etat.ouverts.retain(|f| !f.commande.arretee());
        etat.ouverts
            .iter()
            .map(|f| (f.source.clone(), f.commande.ips()))
            .collect()
    }
}

impl CaptureEcran for EcransFactices {
    fn disponible(&self) -> bool {
        true
    }

    fn sources(&self) -> Resultat<Vec<SourceCapture>> {
        let etat = self.etat();
        match &etat.refus {
            Some(refus) => Err(Erreur::Echec(refus.clone())),
            None => Ok(etat.sources.clone()),
        }
    }

    fn capturer(&self, source: &str) -> Resultat<Image> {
        let etat = self.etat();
        let s = etat
            .sources
            .iter()
            .find(|s| s.id == source)
            .ok_or_else(|| source_inconnue(source))?;
        Ok(image_unie(s.largeur, s.hauteur, [40, 90, 160, 255]))
    }

    fn demarrer(
        &self,
        source: &str,
        options: OptionsCapture,
        recepteur: RecepteurCapture,
    ) -> Resultat<FluxCapture> {
        if !self.etat().sources.iter().any(|s| s.id == source) {
            return Err(source_inconnue(source));
        }
        let (envoi, reception) = mpsc::channel::<Releve>();
        let flux = lancer(source, options, recepteur, move |commande, recepteur| {
            let mut suivi = Suivi::default();
            while !commande.arretee() {
                match reception.recv_timeout(Duration::from_millis(20)) {
                    Ok(releve) => {
                        if suivi.transmettre(releve, recepteur).is_none() {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })?;
        self.etat().ouverts.push(FluxOuvert {
            source: source.to_string(),
            envoi,
            commande: flux.commande(),
        });
        Ok(flux)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{EvenementCapture, TypeSource};

    fn source(id: &str, genre: TypeSource) -> SourceCapture {
        SourceCapture {
            id: id.to_string(),
            nom: id.to_string(),
            application: String::new(),
            genre,
            largeur: 64,
            hauteur: 36,
            principale: false,
            processus: None,
            estompe: false,
        }
    }

    #[test]
    fn le_flux_factice_transmet_les_images_puis_la_fermeture() {
        let ecrans = EcransFactices::nouveaux();
        ecrans.brancher(source("fenetre:1", TypeSource::Fenetre));
        let (envoi, reception) = mpsc::channel();
        let flux = ecrans
            .demarrer(
                "fenetre:1",
                OptionsCapture {
                    ips: 15,
                    curseur: true,
                },
                Box::new(move |e| envoi.send(e).is_ok()),
            )
            .unwrap();
        assert_eq!(ecrans.ouverts(), vec![("fenetre:1".to_string(), 15)]);
        flux.definir_cadence(60);
        assert_eq!(ecrans.ouverts()[0].1, 60);
        assert_eq!(ecrans.pousser("fenetre:1"), 1);
        assert!(matches!(
            reception.recv_timeout(Duration::from_secs(5)),
            Ok(EvenementCapture::Trame(i)) if i.largeur == 64
        ));
        ecrans.retirer("fenetre:1");
        assert_eq!(
            reception.recv_timeout(Duration::from_secs(5)),
            Ok(EvenementCapture::Fermee)
        );
        assert!(ecrans.sources().unwrap().is_empty());
        drop(flux);
        assert!(ecrans.ouverts().is_empty());
    }
}
