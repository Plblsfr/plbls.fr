use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::stream::{self, StreamExt};
use tokio::sync::mpsc;

use crate::http::{ErreurApi, Refus};

use super::camera::FormatVideo;
use super::partage::PlafondPartage;
use super::salle::{
    Connecteur, Connexion, ErreurSalle, EvenementSalle, FluxTrames, InfoParticipant, JetonSalle,
    Limitation, RaisonDeconnexion, Salle, SortieMicro, SortieVideo, SourceAudio,
};
use super::session::ApiVocale;
use super::video::{DemandeVideo, FluxVideo, ImageI420, ImageVideo, SourceVideo};

type CleFactice = (String, String, SourceVideo);
type SonsPartage = BTreeMap<String, mpsc::UnboundedSender<Vec<i16>>>;

pub fn piste_factice(identite: &str, source: SourceVideo) -> String {
    match source {
        SourceVideo::Camera => format!("TR_{identite}_camera"),
        SourceVideo::Partage => format!("TR_{identite}_ecran"),
    }
}

#[derive(Default)]
struct PublicationFactice {
    abonnes: BTreeMap<String, (mpsc::UnboundedSender<ImageVideo>, DemandeVideo)>,
}

fn flux_video(reception: mpsc::UnboundedReceiver<ImageVideo>) -> FluxVideo {
    stream::unfold(reception, |mut r| async move {
        r.recv().await.map(|image| (image, r))
    })
    .boxed()
}

fn flux(reception: mpsc::UnboundedReceiver<Vec<i16>>) -> FluxTrames {
    stream::unfold(reception, |mut r| async move {
        r.recv().await.map(|trame| (trame, r))
    })
    .boxed()
}

#[derive(Default)]
struct Membre {
    evenements: Option<mpsc::UnboundedSender<EvenementSalle>>,
    attributs: BTreeMap<String, String>,
    publie: bool,
}

#[derive(Default)]
struct EtatServeur {
    salles: BTreeMap<String, BTreeMap<String, Membre>>,
    abonnes: BTreeMap<(String, String), Vec<mpsc::UnboundedSender<Vec<i16>>>>,
    hors_ligne: bool,
    trames: BTreeMap<String, usize>,
    coupes: BTreeMap<String, bool>,
    connexions: usize,
    videos: BTreeMap<CleFactice, PublicationFactice>,
    images: BTreeMap<String, usize>,
    formats: BTreeMap<CleFactice, FormatVideo>,
    publiees: BTreeMap<CleFactice, usize>,
    sons_partage: BTreeMap<(String, String), SonsPartage>,
    dimensions: BTreeMap<CleFactice, (u32, u32)>,
    trames_partage: BTreeMap<String, usize>,
    limitations: BTreeMap<String, Limitation>,
}

#[derive(Clone, Default)]
pub struct ServeurVocalFactice {
    etat: Arc<Mutex<EtatServeur>>,
}

fn info(identite: &str, attributs: &BTreeMap<String, String>, local: bool) -> InfoParticipant {
    InfoParticipant {
        identite: identite.to_string(),
        nom: identite.to_string(),
        attributs: attributs.clone(),
        local,
        ..InfoParticipant::default()
    }
}

impl ServeurVocalFactice {
    pub fn nouveau() -> Self {
        Self::default()
    }

    fn etat(&self) -> MutexGuard<'_, EtatServeur> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn participants(&self, salle: &str) -> BTreeSet<String> {
        self.etat()
            .salles
            .get(salle)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn attributs(&self, salle: &str, identite: &str) -> BTreeMap<String, String> {
        self.etat()
            .salles
            .get(salle)
            .and_then(|m| m.get(identite))
            .map(|m| m.attributs.clone())
            .unwrap_or_default()
    }

    pub fn trames_recues(&self, identite: &str) -> usize {
        self.etat().trames.get(identite).copied().unwrap_or(0)
    }

    pub fn micro_coupe(&self, identite: &str) -> Option<bool> {
        self.etat().coupes.get(identite).copied()
    }

    pub fn connexions(&self) -> usize {
        self.etat().connexions
    }

    pub fn publier_video(&self, salle: &str, identite: &str, source: SourceVideo) {
        self.etat()
            .videos
            .entry((salle.to_string(), identite.to_string(), source))
            .or_default();
        self.diffuser(salle, identite, || EvenementSalle::VideoPubliee {
            identite: identite.to_string(),
            source,
            piste: piste_factice(identite, source),
        });
    }

    pub fn publier_son_partage(&self, salle: &str, identite: &str) {
        self.etat()
            .sons_partage
            .entry((salle.to_string(), identite.to_string()))
            .or_default();
        self.diffuser(salle, identite, || EvenementSalle::AudioPubliee {
            identite: identite.to_string(),
            source: SourceAudio::Partage,
        });
    }

    pub fn retirer_son_partage(&self, salle: &str, identite: &str) {
        let abonnes = self
            .etat()
            .sons_partage
            .remove(&(salle.to_string(), identite.to_string()))
            .unwrap_or_default();
        for abonne in abonnes.into_keys() {
            self.envoyer(
                salle,
                &abonne,
                EvenementSalle::PisteRetiree {
                    identite: identite.to_string(),
                    source: SourceAudio::Partage,
                },
            );
        }
        self.diffuser(salle, identite, || EvenementSalle::AudioRetiree {
            identite: identite.to_string(),
            source: SourceAudio::Partage,
        });
    }

    pub fn abonnes_son_partage(&self, salle: &str, identite: &str) -> BTreeSet<String> {
        self.etat()
            .sons_partage
            .get(&(salle.to_string(), identite.to_string()))
            .map(|a| a.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn pousser_son_partage(&self, salle: &str, identite: &str, trame: &[i16]) -> usize {
        let mut etat = self.etat();
        let Some(abonnes) = etat
            .sons_partage
            .get_mut(&(salle.to_string(), identite.to_string()))
        else {
            return 0;
        };
        abonnes.retain(|_, envoi| envoi.send(trame.to_vec()).is_ok());
        abonnes.len()
    }

    fn regler_son_partage(&self, salle: &str, abonne: &str, identite: &str, recevoir: bool) {
        let evenement = {
            let mut etat = self.etat();
            let Some(abonnes) = etat
                .sons_partage
                .get_mut(&(salle.to_string(), identite.to_string()))
            else {
                return;
            };
            match (recevoir, abonnes.contains_key(abonne)) {
                (true, false) => {
                    let (envoi, reception) = mpsc::unbounded_channel();
                    abonnes.insert(abonne.to_string(), envoi);
                    Some(EvenementSalle::PisteAudio {
                        identite: identite.to_string(),
                        source: SourceAudio::Partage,
                        trames: flux(reception),
                    })
                }
                (false, true) => {
                    abonnes.remove(abonne);
                    Some(EvenementSalle::PisteRetiree {
                        identite: identite.to_string(),
                        source: SourceAudio::Partage,
                    })
                }
                _ => None,
            }
        };
        if let Some(evenement) = evenement {
            self.envoyer(salle, abonne, evenement);
        }
    }

    pub fn retirer_video(&self, salle: &str, identite: &str, source: SourceVideo) {
        let publication =
            self.etat()
                .videos
                .remove(&(salle.to_string(), identite.to_string(), source));
        for abonne in publication
            .map(|p| p.abonnes)
            .unwrap_or_default()
            .into_keys()
        {
            self.envoyer(
                salle,
                &abonne,
                EvenementSalle::PisteVideoRetiree {
                    identite: identite.to_string(),
                    source,
                },
            );
        }
        self.diffuser(salle, identite, || EvenementSalle::VideoRetiree {
            identite: identite.to_string(),
            source,
        });
    }

    pub fn pousser_image(
        &self,
        salle: &str,
        identite: &str,
        source: SourceVideo,
        image: ImageVideo,
    ) -> usize {
        let mut etat = self.etat();
        let EtatServeur { videos, images, .. } = &mut *etat;
        let Some(publication) = videos.get_mut(&(salle.to_string(), identite.to_string(), source))
        else {
            return 0;
        };
        publication
            .abonnes
            .retain(|_, (envoi, _)| envoi.send(image.clone()).is_ok());
        for abonne in publication.abonnes.keys() {
            *images.entry(abonne.clone()).or_default() += 1;
        }
        publication.abonnes.len()
    }

    pub fn abonnes_video(
        &self,
        salle: &str,
        identite: &str,
        source: SourceVideo,
    ) -> BTreeMap<String, DemandeVideo> {
        self.etat()
            .videos
            .get(&(salle.to_string(), identite.to_string(), source))
            .map(|p| {
                p.abonnes
                    .iter()
                    .map(|(a, (_, d))| (a.clone(), *d))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn video_publiee(&self, salle: &str, identite: &str, source: SourceVideo) -> bool {
        self.etat()
            .videos
            .contains_key(&(salle.to_string(), identite.to_string(), source))
    }

    pub fn format_publie(
        &self,
        salle: &str,
        identite: &str,
        source: SourceVideo,
    ) -> Option<FormatVideo> {
        self.etat()
            .formats
            .get(&(salle.to_string(), identite.to_string(), source))
            .copied()
    }

    pub fn images_publiees(&self, salle: &str, identite: &str, source: SourceVideo) -> usize {
        self.etat()
            .publiees
            .get(&(salle.to_string(), identite.to_string(), source))
            .copied()
            .unwrap_or(0)
    }

    pub fn dimensions_publiees(
        &self,
        salle: &str,
        identite: &str,
        source: SourceVideo,
    ) -> Option<(u32, u32)> {
        self.etat()
            .dimensions
            .get(&(salle.to_string(), identite.to_string(), source))
            .copied()
    }

    pub fn trames_son_partage(&self, identite: &str) -> usize {
        self.etat()
            .trames_partage
            .get(identite)
            .copied()
            .unwrap_or(0)
    }

    pub fn son_partage_publie(&self, salle: &str, identite: &str) -> bool {
        self.etat()
            .sons_partage
            .contains_key(&(salle.to_string(), identite.to_string()))
    }

    pub fn limiter(&self, identite: &str, limitation: Limitation) {
        self.etat()
            .limitations
            .insert(identite.to_string(), limitation);
    }

    pub fn images_envoyees(&self, abonne: &str) -> usize {
        self.etat().images.get(abonne).copied().unwrap_or(0)
    }

    fn regler_abonnement(
        &self,
        salle: &str,
        abonne: &str,
        identite: &str,
        source: SourceVideo,
        demande: Option<DemandeVideo>,
    ) {
        let evenement = {
            let mut etat = self.etat();
            let Some(publication) =
                etat.videos
                    .get_mut(&(salle.to_string(), identite.to_string(), source))
            else {
                return;
            };
            match (demande, publication.abonnes.get_mut(abonne)) {
                (Some(d), Some((_, actuelle))) => {
                    *actuelle = d;
                    None
                }
                (Some(d), None) => {
                    let (envoi, reception) = mpsc::unbounded_channel();
                    publication.abonnes.insert(abonne.to_string(), (envoi, d));
                    Some(EvenementSalle::PisteVideo {
                        identite: identite.to_string(),
                        source,
                        images: flux_video(reception),
                    })
                }
                (None, Some(_)) => {
                    publication.abonnes.remove(abonne);
                    Some(EvenementSalle::PisteVideoRetiree {
                        identite: identite.to_string(),
                        source,
                    })
                }
                (None, None) => None,
            }
        };
        if let Some(evenement) = evenement {
            self.envoyer(salle, abonne, evenement);
        }
    }

    pub fn hors_ligne(&self, hors_ligne: bool) {
        self.etat().hors_ligne = hors_ligne;
    }

    pub fn envoyer(&self, salle: &str, identite: &str, evenement: EvenementSalle) -> bool {
        self.etat()
            .salles
            .get(salle)
            .and_then(|m| m.get(identite))
            .and_then(|m| m.evenements.as_ref())
            .is_some_and(|e| e.send(evenement).is_ok())
    }

    pub fn couper(&self, salle: &str, identite: &str, raison: RaisonDeconnexion) {
        let retire = {
            let mut etat = self.etat();
            if raison == RaisonDeconnexion::Reseau {
                etat.hors_ligne = true;
            }
            etat.salles.get_mut(salle).and_then(|m| m.remove(identite))
        };
        if let Some(membre) = retire {
            if let Some(e) = membre.evenements {
                let _ = e.send(EvenementSalle::Deconnecte(raison));
            }
            self.quitter_videos(salle, identite);
            self.diffuser(salle, identite, || {
                EvenementSalle::Depart(identite.to_string())
            });
        }
    }

    fn quitter_videos(&self, salle: &str, identite: &str) {
        let son = {
            let mut etat = self.etat();
            for ((s, _), abonnes) in etat.sons_partage.iter_mut() {
                if s == salle {
                    abonnes.remove(identite);
                }
            }
            etat.sons_partage
                .contains_key(&(salle.to_string(), identite.to_string()))
        };
        if son {
            self.retirer_son_partage(salle, identite);
        }
        let publiees: Vec<SourceVideo> = {
            let mut etat = self.etat();
            for ((s, _, _), p) in etat.videos.iter_mut() {
                if s == salle {
                    p.abonnes.remove(identite);
                }
            }
            etat.videos
                .keys()
                .filter(|(s, i, _)| s == salle && i == identite)
                .map(|(_, _, source)| *source)
                .collect()
        };
        for source in publiees {
            self.retirer_video(salle, identite, source);
        }
    }

    fn diffuser(&self, salle: &str, sauf: &str, evenement: impl Fn() -> EvenementSalle) {
        let etat = self.etat();
        if let Some(membres) = etat.salles.get(salle) {
            for (identite, membre) in membres {
                if identite != sauf {
                    if let Some(e) = &membre.evenements {
                        let _ = e.send(evenement());
                    }
                }
            }
        }
    }

    fn abonner(&self, salle: &str, publicateur: &str) -> EvenementSalle {
        let (envoi, reception) = mpsc::unbounded_channel();
        self.etat()
            .abonnes
            .entry((salle.to_string(), publicateur.to_string()))
            .or_default()
            .push(envoi);
        EvenementSalle::PisteAudio {
            identite: publicateur.to_string(),
            source: SourceAudio::Micro,
            trames: flux(reception),
        }
    }

    fn connecter_membre(&self, jeton: &JetonSalle) -> Result<Connexion, ErreurSalle> {
        let (identite, salle) = jeton
            .jeton
            .split_once('@')
            .map(|(i, s)| (i.to_string(), s.to_string()))
            .ok_or_else(|| ErreurSalle::Connexion("jeton invalide".into()))?;
        let (envoi, evenements) = mpsc::unbounded_channel();
        let (participants, publicateurs, videos, sons) = {
            let mut etat = self.etat();
            if etat.hors_ligne {
                return Err(ErreurSalle::Connexion("réseau coupé".into()));
            }
            etat.connexions += 1;
            let membres = etat.salles.entry(salle.clone()).or_default();
            membres.insert(
                identite.clone(),
                Membre {
                    evenements: Some(envoi.clone()),
                    ..Membre::default()
                },
            );
            let participants: Vec<_> = membres
                .iter()
                .map(|(i, m)| info(i, &m.attributs, i == &identite))
                .collect();
            let publicateurs: Vec<_> = membres
                .iter()
                .filter(|(i, m)| m.publie && *i != &identite)
                .map(|(i, _)| i.clone())
                .collect();
            let videos: Vec<(String, SourceVideo)> = etat
                .videos
                .keys()
                .filter(|(s, i, _)| s == &salle && i != &identite)
                .map(|(_, i, source)| (i.clone(), *source))
                .collect();
            let sons: Vec<String> = etat
                .sons_partage
                .keys()
                .filter(|(s, i)| s == &salle && i != &identite)
                .map(|(_, i)| i.clone())
                .collect();
            (participants, publicateurs, videos, sons)
        };
        self.diffuser(&salle, &identite, || {
            EvenementSalle::Arrivee(info(&identite, &BTreeMap::new(), false))
        });
        for publicateur in publicateurs {
            let _ = envoi.send(self.abonner(&salle, &publicateur));
        }
        for (identite, source) in videos {
            let piste = piste_factice(&identite, source);
            let _ = envoi.send(EvenementSalle::VideoPubliee {
                identite,
                source,
                piste,
            });
        }
        for identite in sons {
            let _ = envoi.send(EvenementSalle::AudioPubliee {
                identite,
                source: SourceAudio::Partage,
            });
        }
        Ok(Connexion {
            salle: Arc::new(SalleFactice {
                serveur: self.clone(),
                salle,
                identite,
                fermee: AtomicBool::new(false),
            }),
            participants,
            evenements,
        })
    }
}

struct SortieVideoFactice {
    serveur: ServeurVocalFactice,
    salle: String,
    identite: String,
    source: SourceVideo,
}

impl SortieVideo for SortieVideoFactice {
    fn envoyer(&self, image: &ImageI420) {
        let cle = (self.salle.clone(), self.identite.clone(), self.source);
        if !self.serveur.etat().videos.contains_key(&cle) {
            return;
        }
        {
            let mut etat = self.serveur.etat();
            etat.dimensions
                .insert(cle.clone(), (image.largeur, image.hauteur));
            *etat.publiees.entry(cle).or_default() += 1;
        }
        self.serveur.pousser_image(
            &self.salle,
            &self.identite,
            self.source,
            Arc::new(image.clone()),
        );
    }

    fn piste(&self) -> String {
        piste_factice(&self.identite, self.source)
    }
}

struct SortieSonPartageFactice {
    serveur: ServeurVocalFactice,
    salle: String,
    identite: String,
}

impl SortieMicro for SortieSonPartageFactice {
    fn envoyer(&self, trame: &[i16]) {
        *self
            .serveur
            .etat()
            .trames_partage
            .entry(self.identite.clone())
            .or_default() += 1;
        self.serveur
            .pousser_son_partage(&self.salle, &self.identite, trame);
    }

    fn couper(&self, _coupe: bool) {}
}

struct SalleFactice {
    serveur: ServeurVocalFactice,
    salle: String,
    identite: String,
    fermee: AtomicBool,
}

struct SortieFactice {
    serveur: ServeurVocalFactice,
    salle: String,
    identite: String,
}

impl SortieMicro for SortieFactice {
    fn envoyer(&self, trame: &[i16]) {
        let mut etat = self.serveur.etat();
        *etat.trames.entry(self.identite.clone()).or_default() += 1;
        if let Some(abonnes) = etat
            .abonnes
            .get_mut(&(self.salle.clone(), self.identite.clone()))
        {
            abonnes.retain(|a| a.send(trame.to_vec()).is_ok());
        }
    }

    fn couper(&self, coupe: bool) {
        self.serveur
            .etat()
            .coupes
            .insert(self.identite.clone(), coupe);
    }
}

impl Salle for SalleFactice {
    fn identite(&self) -> String {
        self.identite.clone()
    }

    fn definir_attributs(
        &self,
        attributs: BTreeMap<String, String>,
    ) -> BoxFuture<'_, Result<(), ErreurSalle>> {
        Box::pin(async move {
            if self.fermee.load(Ordering::SeqCst) {
                return Err(ErreurSalle::Fermee);
            }
            {
                let mut etat = self.serveur.etat();
                let membre = etat
                    .salles
                    .get_mut(&self.salle)
                    .and_then(|m| m.get_mut(&self.identite))
                    .ok_or(ErreurSalle::Fermee)?;
                membre.attributs = attributs.clone();
            }
            self.serveur
                .diffuser(&self.salle, "", || EvenementSalle::Attributs {
                    identite: self.identite.clone(),
                    attributs: attributs.clone(),
                });
            Ok(())
        })
    }

    fn publier_micro(&self) -> BoxFuture<'_, Result<Arc<dyn SortieMicro>, ErreurSalle>> {
        Box::pin(async move {
            let abonnes: Vec<String> = {
                let mut etat = self.serveur.etat();
                let membres = etat
                    .salles
                    .get_mut(&self.salle)
                    .ok_or(ErreurSalle::Fermee)?;
                membres
                    .get_mut(&self.identite)
                    .ok_or(ErreurSalle::Fermee)?
                    .publie = true;
                membres
                    .keys()
                    .filter(|i| *i != &self.identite)
                    .cloned()
                    .collect()
            };
            for abonne in abonnes {
                let piste = self.serveur.abonner(&self.salle, &self.identite);
                self.serveur.envoyer(&self.salle, &abonne, piste);
            }
            let sortie: Arc<dyn SortieMicro> = Arc::new(SortieFactice {
                serveur: self.serveur.clone(),
                salle: self.salle.clone(),
                identite: self.identite.clone(),
            });
            Ok(sortie)
        })
    }

    fn regler_video(&self, identite: &str, source: SourceVideo, demande: Option<DemandeVideo>) {
        if self.fermee.load(Ordering::SeqCst) {
            return;
        }
        self.serveur
            .regler_abonnement(&self.salle, &self.identite, identite, source, demande);
    }

    fn regler_audio(&self, identite: &str, source: SourceAudio, recevoir: bool) {
        if self.fermee.load(Ordering::SeqCst) || source != SourceAudio::Partage {
            return;
        }
        self.serveur
            .regler_son_partage(&self.salle, &self.identite, identite, recevoir);
    }

    fn publier_video(
        &self,
        source: SourceVideo,
        format: FormatVideo,
    ) -> BoxFuture<'_, Result<Arc<dyn SortieVideo>, ErreurSalle>> {
        Box::pin(async move {
            if self.fermee.load(Ordering::SeqCst) {
                return Err(ErreurSalle::Fermee);
            }
            self.serveur
                .etat()
                .formats
                .insert((self.salle.clone(), self.identite.clone(), source), format);
            self.serveur
                .publier_video(&self.salle, &self.identite, source);
            let sortie: Arc<dyn SortieVideo> = Arc::new(SortieVideoFactice {
                serveur: self.serveur.clone(),
                salle: self.salle.clone(),
                identite: self.identite.clone(),
                source,
            });
            Ok(sortie)
        })
    }

    fn retirer_video(&self, source: SourceVideo) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.serveur
                .retirer_video(&self.salle, &self.identite, source);
        })
    }

    fn publier_son_partage(&self) -> BoxFuture<'_, Result<Arc<dyn SortieMicro>, ErreurSalle>> {
        Box::pin(async move {
            if self.fermee.load(Ordering::SeqCst) {
                return Err(ErreurSalle::Fermee);
            }
            self.serveur
                .publier_son_partage(&self.salle, &self.identite);
            let sortie: Arc<dyn SortieMicro> = Arc::new(SortieSonPartageFactice {
                serveur: self.serveur.clone(),
                salle: self.salle.clone(),
                identite: self.identite.clone(),
            });
            Ok(sortie)
        })
    }

    fn retirer_son_partage(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.serveur
                .retirer_son_partage(&self.salle, &self.identite);
        })
    }

    fn limitation_video(&self, _source: SourceVideo) -> BoxFuture<'_, Limitation> {
        Box::pin(async move {
            self.serveur
                .etat()
                .limitations
                .get(&self.identite)
                .copied()
                .unwrap_or_default()
        })
    }

    fn fermer(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if self.fermee.swap(true, Ordering::SeqCst) {
                return;
            }
            self.serveur.quitter_videos(&self.salle, &self.identite);
            let present = self
                .serveur
                .etat()
                .salles
                .get_mut(&self.salle)
                .and_then(|m| m.remove(&self.identite))
                .is_some();
            if present {
                self.serveur.diffuser(&self.salle, &self.identite, || {
                    EvenementSalle::Depart(self.identite.clone())
                });
            }
        })
    }
}

impl Connecteur for ServeurVocalFactice {
    fn connecter(&self, jeton: JetonSalle) -> BoxFuture<'static, Result<Connexion, ErreurSalle>> {
        let serveur = self.clone();
        Box::pin(async move { serveur.connecter_membre(&jeton) })
    }
}

pub struct ApiVocaleFactice {
    pub identite: String,
    pub hors_ligne: AtomicBool,
    pub refus: Mutex<BTreeMap<String, (u16, String)>>,
    pub appels: AtomicUsize,
    delai: Mutex<Duration>,
    plafond: Mutex<Option<PlafondPartage>>,
}

impl ApiVocaleFactice {
    pub fn nouvelle(identite: &str) -> Self {
        Self {
            identite: identite.to_string(),
            hors_ligne: AtomicBool::new(false),
            refus: Mutex::new(BTreeMap::new()),
            appels: AtomicUsize::new(0),
            delai: Mutex::new(Duration::ZERO),
            plafond: Mutex::new(None),
        }
    }

    pub fn plafonner(&self, plafond: Option<PlafondPartage>) {
        if let Ok(mut p) = self.plafond.lock() {
            *p = plafond;
        }
    }

    pub fn retarder(&self, delai: Duration) {
        if let Ok(mut d) = self.delai.lock() {
            *d = delai;
        }
    }

    pub fn refuser(&self, salon: &str, statut: u16, code: &str) {
        if let Ok(mut refus) = self.refus.lock() {
            refus.insert(salon.to_string(), (statut, code.to_string()));
        }
    }
}

impl ApiVocale for ApiVocaleFactice {
    fn jeton(&self, salon: &str) -> BoxFuture<'_, Result<JetonSalle, ErreurApi>> {
        let salon = salon.to_string();
        Box::pin(async move {
            self.appels.fetch_add(1, Ordering::SeqCst);
            let delai = self.delai.lock().map(|d| *d).unwrap_or_default();
            if !delai.is_zero() {
                tokio::time::sleep(delai).await;
            }
            if self.hors_ligne.load(Ordering::SeqCst) {
                return Err(ErreurApi::Reseau("hors ligne".into()));
            }
            let refus = self.refus.lock().ok().and_then(|r| r.get(&salon).cloned());
            if let Some((statut, code)) = refus {
                let refus = Refus {
                    statut,
                    code,
                    message: String::new(),
                    donnees: Default::default(),
                };
                return Err(match statut {
                    403 => ErreurApi::Interdit(refus),
                    409 => ErreurApi::Conflit(refus),
                    500..=599 => ErreurApi::Serveur(refus),
                    _ => ErreurApi::Autre(refus),
                });
            }
            Ok(JetonSalle {
                url: "factice://vocal".to_string(),
                jeton: format!("{}@{salon}", self.identite),
                salle: salon,
                max_participants: Some(10),
                plafond_partage: self.plafond.lock().ok().and_then(|p| *p),
            })
        })
    }
}
