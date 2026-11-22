use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::{
    cible_du_son, EvenementCapture, FluxCapture, FormatAudio, Image, OptionsCapture,
    RecepteurCapture, SourceCapture,
};
use tokio::time::Instant;

use super::super::incrustation::{Compositeur, Piece};
use super::super::partage::{
    message_source_perdue, palier_inferieur, ImagePartage, PartageLocal, QualitePartage,
    ReglagesPartage, MESSAGE_PARTAGE_IMPOSSIBLE,
};
use super::super::salle::{SortieMicro, SortieVideo};
use super::super::video::{CleVideo, RecepteurVideo, SourceVideo};
use super::{Acteur, AvisVocal, Commande};
use crate::audio::ConversionEntree;

pub const PERIODE_ADAPTATION: Duration = Duration::from_secs(3);
pub const COUPS_AVANT_REDUCTION: u32 = 3;
pub const MESSAGE_PUBLICATION_PARTAGE: &str = "Impossible de publier le partage dans le salon.";
pub const MESSAGE_SON_PARTAGE: &str = "Partage sans le son : capture du son impossible.";
pub const PERIODE_RELANCE: Duration = Duration::from_millis(33);
pub const INACTIVITE_RELANCE: Duration = Duration::from_millis(45);

pub struct Relance {
    arret: Arc<AtomicBool>,
    fil: Option<std::thread::JoinHandle<()>>,
}

impl Relance {
    pub fn demarrer(route: Arc<RoutePartage>) -> Option<Self> {
        let arret = Arc::new(AtomicBool::new(false));
        let drapeau = arret.clone();
        let fil = std::thread::Builder::new()
            .name("estompe-incrustation".into())
            .spawn(move || {
                while !drapeau.load(Ordering::Acquire) {
                    std::thread::sleep(PERIODE_RELANCE);
                    route.relancer(INACTIVITE_RELANCE);
                }
            })
            .ok()?;
        Some(Self {
            arret,
            fil: Some(fil),
        })
    }
}

impl Drop for Relance {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Release);
        if let Some(fil) = self.fil.take() {
            let _ = fil.join();
        }
    }
}

#[derive(Default)]
pub struct RouteSonPartage {
    sortie: Mutex<Option<Arc<dyn SortieMicro>>>,
    conversion: Mutex<Option<(FormatAudio, ConversionEntree)>>,
}

impl RouteSonPartage {
    pub fn pousser(&self, entrelace: &[f32], format: FormatAudio) {
        let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) else {
            return;
        };
        let Ok(mut conversion) = self.conversion.lock() else {
            return;
        };
        if conversion.as_ref().map(|(f, _)| *f) != Some(format) {
            *conversion = Some((format, ConversionEntree::nouvelle(format)));
        }
        if let Some((_, c)) = conversion.as_mut() {
            c.pousser(entrelace, |trame| sortie.envoyer(trame));
        }
    }

    fn brancher(&self, sortie: Option<Arc<dyn SortieMicro>>) {
        if let Ok(mut s) = self.sortie.lock() {
            *s = sortie;
        }
        if let Ok(mut c) = self.conversion.lock() {
            *c = None;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalPartage {
    Reduite(bool),
    Perdue(Option<String>),
}

struct DerniereImage {
    image: Arc<ImagePartage>,
    piece: Option<Piece>,
    quand: std::time::Instant,
}

#[derive(Default)]
pub struct RoutePartage {
    sortie: Mutex<Option<Arc<dyn SortieVideo>>>,
    recepteur: Mutex<Option<Arc<RecepteurVideo>>>,
    incrustation: Mutex<Option<Arc<Compositeur>>>,
    derniere: Mutex<Option<DerniereImage>>,
    emission: Mutex<()>,
    hauteur: AtomicU32,
}

impl RoutePartage {
    pub fn envoyer(&self, image: Image) {
        let _emission = self.emission.lock();
        self.emettre(image);
    }

    fn emettre(&self, mut image: Image) {
        let compositeur = self.incrustation.lock().ok().and_then(|i| i.clone());
        let piece = compositeur.as_ref().and_then(|c| c.incruster(&mut image));
        let image = Arc::new(ImagePartage(image));
        if let Some(sortie) = self.sortie.lock().ok().and_then(|s| s.clone()) {
            sortie.envoyer_rgba(&image.0, self.hauteur.load(Ordering::Relaxed));
        }
        if let Some(recepteur) = self.recepteur.lock().ok().and_then(|r| r.clone()) {
            recepteur.pousser(image.clone());
        }
        if let Ok(mut derniere) = self.derniere.lock() {
            *derniere = (compositeur.is_some() || piece.is_some()).then(|| DerniereImage {
                image,
                piece,
                quand: std::time::Instant::now(),
            });
        }
    }

    pub fn relancer(&self, inactivite: Duration) -> bool {
        let _emission = self.emission.lock();
        let camera = self
            .incrustation
            .lock()
            .ok()
            .and_then(|i| i.clone())
            .is_some_and(|c| c.a_une_camera());
        let base = match self.derniere.lock() {
            Ok(d) => match d.as_ref() {
                Some(d) if d.quand.elapsed() >= inactivite && (camera || d.piece.is_some()) => {
                    let mut image = d.image.0.clone();
                    if let Some(piece) = &d.piece {
                        piece.restaurer(&mut image);
                    }
                    image
                }
                _ => return false,
            },
            Err(_) => return false,
        };
        self.emettre(base);
        true
    }

    pub fn definir_incrustation(&self, incrustation: Option<Arc<Compositeur>>) {
        let retiree = incrustation.is_none();
        if let Ok(mut i) = self.incrustation.lock() {
            *i = incrustation;
        }
        if retiree {
            self.relancer(Duration::ZERO);
        }
    }

    fn oublier(&self) {
        if let Ok(mut d) = self.derniere.lock() {
            *d = None;
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
                self.suivre_incrustation();
                self.demarrer_son().await;
            }
            Err(message) => {
                self.couper_partage();
                self.etat.partage_erreur = Some(message);
            }
        }
        self.annoncer_incrustation().await;
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
                if partage.son {
                    self.arreter_son().await;
                    self.demarrer_son().await;
                }
            }
            Err(message) => self.etat.partage_erreur = Some(message),
        }
        self.publier();
    }

    pub(super) async fn regler_partage(&mut self, reglages: ReglagesPartage) {
        let reglages = reglages.borne();
        self.etat.reglages_partage = reglages;
        self.reglages.partage = reglages;
        if let Some(partage) = self.etat.partage.clone() {
            let qualite = self.qualite_pour(&partage.source);
            self.appliquer_qualite(qualite);
            self.coups = 0;
            self.programmer_adaptation();
            match (reglages.son, partage.son) {
                (true, false) => self.demarrer_son().await,
                (false, true) => self.arreter_son().await,
                _ => {}
            }
        }
        self.publier();
    }

    async fn demarrer_son(&mut self) {
        let (Some(salle), Some(partage)) = (self.salle.clone(), self.etat.partage.clone()) else {
            return;
        };
        if !self.etat.reglages_partage.son || !self.sons.disponible() || self.capture_son.is_some()
        {
            return;
        }
        let Some(cible) = cible_du_son(&partage.source, std::process::id()) else {
            return;
        };
        let Ok(sortie) = salle.publier_son_partage().await else {
            self.etat.partage_erreur = Some(MESSAGE_SON_PARTAGE.to_string());
            return;
        };
        self.route_son.brancher(Some(sortie));
        let route = self.route_son.clone();
        let sons = self.sons.clone();
        let ouverture = tokio::task::spawn_blocking(move || {
            sons.demarrer(
                cible,
                Box::new(move |echantillons, format| route.pousser(echantillons, format)),
            )
        })
        .await;
        match ouverture {
            Ok(Ok(flux)) => {
                self.capture_son = Some(flux);
                if let Some(p) = self.etat.partage.as_mut() {
                    p.son = true;
                }
            }
            _ => {
                self.route_son.brancher(None);
                salle.retirer_son_partage().await;
                self.etat.partage_erreur = Some(MESSAGE_SON_PARTAGE.to_string());
            }
        }
    }

    async fn arreter_son(&mut self) {
        let publie = self.etat.partage.as_ref().is_some_and(|p| p.son);
        self.capture_son = None;
        self.route_son.brancher(None);
        if let Some(p) = self.etat.partage.as_mut() {
            p.son = false;
        }
        if let (true, Some(salle)) = (publie, self.salle.clone()) {
            salle.retirer_son_partage().await;
        }
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
        self.capture_son = None;
        self.route_son.brancher(None);
        self.route_partage.brancher(None);
        self.route_partage.afficher(None);
        self.route_partage.oublier();
        self.adaptation = None;
        self.coups = 0;
        self.etat.partage = None;
        self.etat.partage_demarrage = false;
        if let Some(cle) = self.cle_partage_locale() {
            self.etat.pistes.remove(&cle);
        }
        self.suivre_incrustation();
    }

    pub(super) fn suspendre_partage(&mut self) {
        self.route_partage.brancher(None);
        self.route_son.brancher(None);
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
        } else if partage.son {
            match self.salle.clone() {
                Some(salle) => match salle.publier_son_partage().await {
                    Ok(sortie) => self.route_son.brancher(Some(sortie)),
                    Err(_) => self.arreter_son().await,
                },
                None => self.arreter_son().await,
            }
        }
        self.publier();
    }

    pub(super) async fn arreter_partage(&mut self) {
        let publie = self.etat.partage.is_some();
        let son = self.etat.partage.as_ref().is_some_and(|p| p.son);
        self.couper_partage();
        if let (true, Some(salle)) = (publie, self.salle.clone()) {
            if son {
                salle.retirer_son_partage().await;
            }
            salle.retirer_video(SourceVideo::Partage).await;
        }
        self.annoncer_incrustation().await;
        self.publier();
    }
}
