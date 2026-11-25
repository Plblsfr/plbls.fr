use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::vocal::RecepteurVideo;

use super::etat::Evenement;
use super::image::ImageLecteur;
use super::moteur::{MoteurLecture, Rapport, Source};
use super::pistes::{GenrePiste, Piste};
use super::reglages::{borner_vitesse, position_bornee};

#[derive(Debug, Clone, PartialEq)]
pub enum CommandeMoteur {
    Charger(Source),
    Arreter,
    Pause(bool),
    Aller(f64),
    Volume(u8, bool),
    Vitesse(f64),
    Audio(Option<i64>),
    SousTitres(Option<i64>),
    Video(bool),
    Sortie(String),
    Zone(u32, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaFactice {
    pub duree: Option<f64>,
    pub pistes: Vec<Piste>,
    pub taille: (u32, u32),
}

fn piste(id: i64, genre: GenrePiste, langue: Option<&str>) -> Piste {
    Piste {
        id,
        genre,
        titre: None,
        langue: langue.map(str::to_string),
        codec: None,
        defaut: id == 1,
        forcee: false,
        externe: false,
        image_fixe: false,
        selectionnee: false,
    }
}

impl MediaFactice {
    pub fn video(duree: f64) -> Self {
        Self {
            duree: Some(duree),
            pistes: vec![
                piste(1, GenrePiste::Video, None),
                piste(1, GenrePiste::Audio, Some("fre")),
                piste(2, GenrePiste::Audio, Some("eng")),
                piste(1, GenrePiste::SousTitres, Some("fre")),
            ],
            taille: (1280, 720),
        }
    }

    pub fn son(duree: f64) -> Self {
        Self {
            duree: Some(duree),
            pistes: vec![piste(1, GenrePiste::Audio, None)],
            taille: (0, 0),
        }
    }

    pub fn direct() -> Self {
        Self {
            duree: None,
            ..Self::video(0.0)
        }
    }
}

#[derive(Default)]
struct Interne {
    commandes: Vec<CommandeMoteur>,
    medias: HashMap<String, Result<MediaFactice, String>>,
    charge: Option<MediaFactice>,
    position: f64,
    pause: bool,
    vitesse: f64,
    muet_reponses: bool,
}

#[derive(Clone)]
pub struct MoteurFactice {
    rapport: Rapport,
    interne: Arc<Mutex<Interne>>,
    images: Arc<RecepteurVideo>,
}

impl MoteurFactice {
    pub fn nouveau(rapport: Rapport) -> Self {
        Self {
            rapport,
            interne: Arc::new(Mutex::new(Interne {
                pause: true,
                vitesse: 1.0,
                ..Interne::default()
            })),
            images: RecepteurVideo::nouveau(),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn noter(
        &self,
        commande: CommandeMoteur,
        reponse: impl FnOnce(&mut Interne) -> Vec<Evenement>,
    ) {
        let evenements = {
            let mut interne = self.verrou();
            interne.commandes.push(commande);
            let evenements = reponse(&mut interne);
            if interne.muet_reponses {
                Vec::new()
            } else {
                evenements
            }
        };
        for evenement in evenements {
            (self.rapport)(evenement);
        }
    }

    pub fn ajouter_media(&self, adresse: &str, media: MediaFactice) {
        self.verrou().medias.insert(adresse.to_string(), Ok(media));
    }

    pub fn ajouter_echec(&self, adresse: &str, message: &str) {
        self.verrou()
            .medias
            .insert(adresse.to_string(), Err(message.to_string()));
    }

    pub fn suspendre_reponses(&self, suspendre: bool) {
        self.verrou().muet_reponses = suspendre;
    }

    pub fn emettre(&self, evenement: Evenement) {
        (self.rapport)(evenement);
    }

    pub fn commandes(&self) -> Vec<CommandeMoteur> {
        self.verrou().commandes.clone()
    }

    pub fn vider_commandes(&self) -> Vec<CommandeMoteur> {
        std::mem::take(&mut self.verrou().commandes)
    }

    pub fn position(&self) -> f64 {
        self.verrou().position
    }

    pub fn pousser_image(&self, image: ImageLecteur) {
        self.images.pousser(Arc::new(image));
    }

    pub fn avancer(&self, secondes: f64) {
        let evenements = {
            let mut i = self.verrou();
            let Some(media) = i.charge.clone() else {
                return;
            };
            if i.pause || i.muet_reponses {
                return;
            }
            i.position = position_bornee(i.position + secondes * i.vitesse, media.duree);
            let mut ev = vec![Evenement::Position(i.position)];
            if media.duree.is_some_and(|d| i.position >= d) {
                ev.push(Evenement::Fin(true));
            }
            ev
        };
        for evenement in evenements {
            (self.rapport)(evenement);
        }
    }
}

impl MoteurLecture for MoteurFactice {
    fn charger(&self, source: &Source) {
        let source = source.clone();
        self.noter(CommandeMoteur::Charger(source.clone()), |i| {
            let mut ev = vec![Evenement::Arrete, Evenement::Chargement];
            match i.medias.get(&source.adresse).cloned() {
                Some(Ok(media)) => {
                    i.position = position_bornee(source.depart, media.duree);
                    i.pause = source.en_pause;
                    let audio = media
                        .pistes
                        .iter()
                        .find(|p| p.genre == GenrePiste::Audio)
                        .map(|p| p.id);
                    ev.extend([
                        Evenement::Duree(media.duree),
                        Evenement::Pistes(media.pistes.clone()),
                        Evenement::PisteAudio(audio),
                        Evenement::SousTitres(None),
                        Evenement::LargeurVideo(media.taille.0),
                        Evenement::HauteurVideo(media.taille.1),
                        Evenement::Position(i.position),
                        Evenement::EnPause(i.pause),
                        Evenement::Fin(false),
                        Evenement::Charge,
                    ]);
                    i.charge = Some(media);
                }
                autre => {
                    i.charge = None;
                    let message = match autre {
                        Some(Err(m)) => m,
                        _ => format!("{} introuvable", source.adresse),
                    };
                    ev.extend([Evenement::Erreur(message), Evenement::Arrete]);
                }
            }
            ev
        });
    }

    fn arreter(&self) {
        self.noter(CommandeMoteur::Arreter, |i| {
            i.charge = None;
            vec![Evenement::Arrete]
        });
    }

    fn mettre_en_pause(&self, pause: bool) {
        self.noter(CommandeMoteur::Pause(pause), |i| {
            i.pause = pause;
            vec![Evenement::EnPause(pause)]
        });
    }

    fn aller(&self, position: f64) {
        self.noter(CommandeMoteur::Aller(position), |i| {
            let Some(media) = &i.charge else {
                return Vec::new();
            };
            i.position = position_bornee(position, media.duree);
            vec![
                Evenement::Recherche(true),
                Evenement::Position(i.position),
                Evenement::Fin(false),
                Evenement::Recherche(false),
            ]
        });
    }

    fn regler_volume(&self, volume: u8, muet: bool) {
        self.noter(CommandeMoteur::Volume(volume, muet), |_| {
            vec![Evenement::Volume(f64::from(volume)), Evenement::Muet(muet)]
        });
    }

    fn regler_vitesse(&self, vitesse: f64) {
        self.noter(CommandeMoteur::Vitesse(vitesse), |i| {
            i.vitesse = borner_vitesse(vitesse);
            vec![Evenement::Vitesse(i.vitesse)]
        });
    }

    fn choisir_audio(&self, piste: Option<i64>) {
        self.noter(CommandeMoteur::Audio(piste), |_| {
            vec![Evenement::PisteAudio(piste)]
        });
    }

    fn choisir_sous_titres(&self, piste: Option<i64>) {
        self.noter(CommandeMoteur::SousTitres(piste), |_| {
            vec![Evenement::SousTitres(piste)]
        });
    }

    fn activer_video(&self, active: bool) {
        self.noter(CommandeMoteur::Video(active), |_| Vec::new());
    }

    fn choisir_sortie(&self, peripherique: &str) {
        self.noter(CommandeMoteur::Sortie(peripherique.to_string()), |_| {
            Vec::new()
        });
    }

    fn definir_zone(&self, zone: (u32, u32)) {
        self.noter(CommandeMoteur::Zone(zone.0, zone.1), |_| Vec::new());
    }

    fn images(&self) -> Arc<RecepteurVideo> {
        self.images.clone()
    }
}
