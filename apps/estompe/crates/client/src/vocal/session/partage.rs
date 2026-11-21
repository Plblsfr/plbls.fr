use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{
    EvenementCapture, FluxCapture, Image, OptionsCapture, RecepteurCapture, SourceCapture,
};
use tokio::time::Instant;

use super::super::partage::{
    message_source_perdue, palier_inferieur, ImagePartage, PartageLocal, QualitePartage,
    ReglagesPartage, MESSAGE_PARTAGE_IMPOSSIBLE,
};
use super::super::salle::SortieVideo;
use super::super::video::{CleVideo, RecepteurVideo, SourceVideo};
use super::{Acteur, AvisVocal, Commande};

pub const PERIODE_ADAPTATION: Duration = Duration::from_secs(3);
pub const COUPS_AVANT_REDUCTION: u32 = 3;
pub const MESSAGE_PUBLICATION_PARTAGE: &str = "Impossible de publier le partage dans le salon.";

pub trait Incrustation: Send + Sync {
    fn incruster(&self, image: &mut Image);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalPartage {
    Reduite(bool),
    Perdue(Option<String>),
}

#[derive(Default)]
pub struct RoutePartage {
    sortie: Mutex<Option<Arc<dyn SortieVideo>>>,
    recepteur: Mutex<Option<Arc<RecepteurVideo>>>,
    incrustation: Mutex<Option<Arc<dyn Incrustation>>>,
    hauteur: AtomicU32,
}

impl RoutePartage {
    pub fn envoyer(&self, mut image: Image) {
        if let Some(incrustation) = self.incrustation.lock().ok().and_then(|i| i.clone()) {
            incrustation.incruster(&mut image);
        }
        if let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) {
            sortie.envoyer_rgba(&image, self.hauteur.load(Ordering::Relaxed));
        }
        if let Some(recepteur) = self.recepteur.lock().ok().and_then(|r| r.clone()) {
            recepteur.pousser(Arc::new(ImagePartage(image)));
        }
    }

    pub fn definir_incrustation(&self, incrustation: Option<Arc<dyn Incrustation>>) {
        if let Ok(mut i) = self.incrustation.lock() {
            *i = incrustation;
        }
    }

    fn brancher(&self, sortie: Option<Arc<dyn SortieVideo>>) {
        if let Ok(mut s) = self.sortie.lock() {
            *s = sortie;
        }
    }

    fn afficher(&self, recepteur: Option<Arc<RecepteurVideo>>) {
        let ancien = match self.recepteur.lock() {
            Ok(mut r) => std::mem::replace(&mut *r, recepteur),
            Err(_) => None,
        };
        if let Some(ancien) = ancien {
            ancien.effacer();
        }
    }

    fn definir_hauteur(&self, hauteur: u32) {
        self.hauteur.store(hauteur, Ordering::Relaxed);
    }
}

impl Acteur {
    fn cle_partage_locale(&self) -> Option<CleVideo> {
        self.etat.identite.as_deref().map(CleVideo::partage)
    }

    fn qualite_pour(&self, source: &SourceCapture) -> QualitePartage {
        self.etat
            .reglages_partage
            .qualite(self.etat.plafond_partage.avec_source(Some(source.hauteur)))
    }

    async fn ouvrir_partage(
        &mut self,
        source: &SourceCapture,
        ips: u32,
    ) -> Result<FluxCapture, String> {
        let generation = self.generation_partage + 1;
        let route = self.route_partage.clone();
        let retour = self.retour.clone();
        let signal =
            move |s: SignalPartage| retour.send(Commande::SignalPartage(generation, s)).is_ok();
        let recepteur: RecepteurCapture = Box::new(move |evenement| match evenement {
            EvenementCapture::Trame(image) => {
                route.envoyer(image);
                true
            }
            EvenementCapture::Reduite => signal(SignalPartage::Reduite(true)),
            EvenementCapture::Restauree => signal(SignalPartage::Reduite(false)),
            EvenementCapture::Fermee => {
                signal(SignalPartage::Perdue(None));
                false
            }
            EvenementCapture::Echec(raison) => {
                signal(SignalPartage::Perdue(Some(raison)));
                false
            }
        });
        let ecrans = self.ecrans.clone();
        let id = source.id.clone();
        let ouverture = tokio::task::spawn_blocking(move || {
            ecrans.demarrer(&id, OptionsCapture { ips, curseur: true }, recepteur)
        })
        .await;
        match ouverture {
            Ok(Ok(flux)) => {
                self.generation_partage = generation;
                Ok(flux)
            }
            _ => Err(MESSAGE_PARTAGE_IMPOSSIBLE.to_string()),
        }
    }

    async fn publier_partage(&mut self, qualite: QualitePartage) -> bool {
        let Some(salle) = self.salle.clone() else {
            return false;
        };
        let format = qualite.format(self.etat.reglages_partage.fluidite);
        match salle.publier_video(SourceVideo::Partage, format).await {
            Ok(sortie) => {
                if let Some(cle) = self.cle_partage_locale() {
                    let piste = sortie.piste();
                    if !piste.is_empty() {
                        self.etat.pistes.insert(cle, piste);
                    }
                }
                self.route_partage.brancher(Some(sortie));
                true
            }
            Err(_) => false,
        }
    }

    pub(super) async fn demarrer_partage(&mut self, source: SourceCapture) {
        if self.capture_partage.is_some() {
            return self.changer_source(source).await;
        }
        if self.salle.is_none() || self.etat.partage_demarrage {
            return;
        }
        self.etat.partage_demarrage = true;
        self.etat.partage_erreur = None;
        self.publier();
        let qualite = self.qualite_pour(&source);
        self.route_partage
            .definir_hauteur(qualite.resolution.hauteur());
        if let Some(cle) = self.cle_partage_locale() {
            self.route_partage
                .afficher(Some(self.videos.recepteur(&cle)));
        }
        let resultat = match self.ouvrir_partage(&source, qualite.ips).await {
            Ok(flux) => {
                if self.publier_partage(qualite).await {
                    self.capture_partage = Some(flux);
                    Ok(())
                } else {
                    Err(MESSAGE_PUBLICATION_PARTAGE.to_string())
                }
            }
            Err(message) => Err(message),
        };
        self.etat.partage_demarrage = false;
        match resultat {
            Ok(()) => {
                self.etat.partage = Some(PartageLocal {
                    source,
                    qualite,
                    reduite: false,
                    son: false,
                });
                self.programmer_adaptation();
            }
            Err(message) => {
                self.couper_partage();
                self.etat.partage_erreur = Some(message);
            }
        }
        self.publier();
    }

    pub(super) async fn changer_source(&mut self, source: SourceCapture) {
        let Some(partage) = self.etat.partage.clone() else {
            return;
        };
        if partage.source.id == source.id {
            return;
        }
        let qualite = self.qualite_pour(&source);
        match self.ouvrir_partage(&source, qualite.ips).await {
            Ok(flux) => {
                let ancien = self.capture_partage.replace(flux);
                self.route_partage
                    .definir_hauteur(qualite.resolution.hauteur());
                self.etat.partage_erreur = None;
                self.etat.partage = Some(PartageLocal {
                    source,
                    qualite,
                    reduite: false,
                    ..partage
                });
                self.coups = 0;
                drop(ancien);
            }
            Err(message) => self.etat.partage_erreur = Some(message),
        }
        self.publier();
    }

    pub(super) async fn regler_partage(&mut self, reglages: ReglagesPartage) {
        let reglages = reglages.borne();
        self.etat.reglages_partage = reglages;
        self.reglages.partage = reglages;
        if let Some(source) = self.etat.partage.as_ref().map(|p| p.source.clone()) {
            let qualite = self.qualite_pour(&source);
            self.appliquer_qualite(qualite);
            self.coups = 0;
            self.programmer_adaptation();
        }
        self.publier();
    }

    fn appliquer_qualite(&mut self, qualite: QualitePartage) {
        if let Some(flux) = &self.capture_partage {
            flux.definir_cadence(qualite.ips);
        }
        self.route_partage
            .definir_hauteur(qualite.resolution.hauteur());
        if let Some(p) = self.etat.partage.as_mut() {
            p.qualite = qualite;
        }
    }

    fn programmer_adaptation(&mut self) {
        self.adaptation = (self.etat.partage.is_some() && self.etat.reglages_partage.auto)
            .then(|| Instant::now() + PERIODE_ADAPTATION);
    }

    pub(super) async fn adapter_partage(&mut self) {
        self.adaptation = None;
        let (Some(salle), Some(partage)) = (self.salle.clone(), self.etat.partage.clone()) else {
            return;
        };
        if !self.etat.reglages_partage.auto {
            return;
        }
        let limitation = salle.limitation_video(SourceVideo::Partage).await;
        if self.etat.partage.is_none() {
            return;
        }
        self.coups = if limitation.freine() {
            self.coups + 1
        } else {
            0
        };
        if self.coups >= COUPS_AVANT_REDUCTION {
            self.coups = 0;
            if let Some(palier) =
                palier_inferieur(partage.qualite, self.etat.reglages_partage.fluidite)
            {
                self.appliquer_qualite(palier);
                self.signaler(AvisVocal::PartageReduit(palier));
                self.publier();
            }
        }
        self.programmer_adaptation();
    }

    pub(super) async fn signal_partage(&mut self, generation: u64, signal: SignalPartage) {
        if generation != self.generation_partage {
            return;
        }
        match signal {
            SignalPartage::Reduite(reduite) => {
                if let Some(p) = self.etat.partage.as_mut() {
                    p.reduite = reduite;
                }
                self.publier();
            }
            SignalPartage::Perdue(raison) => {
                let message = match (&raison, &self.etat.partage) {
                    (None, Some(p)) => message_source_perdue(&p.source),
                    _ => "Partage arrêté : la capture de l'écran a échoué.".to_string(),
                };
                self.arreter_partage().await;
                self.etat.partage_erreur = Some(message);
                self.publier();
            }
        }
    }

    pub(super) fn couper_partage(&mut self) {
        self.generation_partage += 1;
        self.capture_partage = None;
        self.route_partage.brancher(None);
        self.route_partage.afficher(None);
        self.adaptation = None;
        self.coups = 0;
        self.etat.partage = None;
        self.etat.partage_demarrage = false;
        if let Some(cle) = self.cle_partage_locale() {
            self.etat.pistes.remove(&cle);
        }
    }

    pub(super) fn suspendre_partage(&mut self) {
        self.route_partage.brancher(None);
        if let Some(cle) = self.cle_partage_locale() {
            self.etat.pistes.remove(&cle);
        }
    }

    pub(super) async fn republier_partage(&mut self) {
        let Some(partage) = self.etat.partage.clone() else {
            return;
        };
        if !self.publier_partage(partage.qualite).await {
            self.arreter_partage().await;
            self.etat.partage_erreur = Some(MESSAGE_PUBLICATION_PARTAGE.to_string());
        }
        self.publier();
    }

    pub(super) async fn arreter_partage(&mut self) {
        let publie = self.etat.partage.is_some();
        self.couper_partage();
        if let (true, Some(salle)) = (publie, self.salle.clone()) {
            salle.retirer_video(SourceVideo::Partage).await;
        }
        self.publier();
    }
}
