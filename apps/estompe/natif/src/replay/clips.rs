use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_client::replay::affichage::{duree_clip, taille};
use estompe_client::replay::automate::Etat as EtatMoteur;
use estompe_client::replay::clips::{
    self, date_clip, decision_envoi, details_clip, jeu_de, jeux_de, part_utilisee, plan_envoi,
    presque_plein, taille_liberee, Envoi, PlanEnvoi,
};
use estompe_client::replay::galerie;
use estompe_client::replay::types::{
    Clip, Codec, EtatReplay, EvenementReplay, ReglagesReplay, Stockage,
};
use estompe_client::store::messages::Brouillon;
use estompe_client::store::{CleConversation, Etat, GenreSalon, Store};
use estompe_client::televersement::{Progression, SourceEnvoi};
use estompe_client::video::{
    ffmpeg_du_systeme, Conversions, Convertisseur, ErreurConversion, FichierTemporaire,
};
use estompe_plateforme::libelle_touche;
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use super::ServiceReplay;
use crate::confirmation::{self, Demande, Saisie};
use crate::coquille::SourcePont;
use crate::messagerie::pont::SystemeFichiers;
use crate::{
    DestinationClip, DonneesClips, DonneesJeux, DonneesReplay, JeuClips, LigneClip, Principale,
    TravailClip,
};

pub const TYPE_MP4: &str = "video/mp4";
pub const SANS_ENCODEUR: &str = "aucun encodeur matériel H.264 disponible";
pub const LIMITE_PAR_DEFAUT: u64 = 100 * 1024 * 1024;

pub type OuvertureEditeur = Arc<dyn Fn(&Principale, &Clip) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub cle: CleConversation,
    pub nom: String,
    pub ou: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VueLigne {
    pub chemin: PathBuf,
    pub nom: String,
    pub infos: String,
    pub details: String,
    pub duree: String,
    pub origine: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VueGalerie {
    pub visible: bool,
    pub titre: String,
    pub tous: bool,
    pub total: usize,
    pub lignes: Vec<VueLigne>,
    pub jeux: Vec<(String, usize, bool)>,
    pub stockage_lu: bool,
    pub espace: String,
    pub part: f32,
    pub depasse: bool,
    pub presque: bool,
    pub alerte: String,
    pub liberer: String,
    pub vide_titre: String,
    pub vide_texte: String,
    pub vide_reglages: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VueEnvoi {
    pub clip: String,
    pub refus: bool,
    pub alerte_titre: String,
    pub alerte: String,
    pub note: String,
    pub action: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErreurEnvoi {
    Annule,
    TropGros,
    Echec(String),
}

impl ErreurEnvoi {
    pub fn message(&self, nom: &str) -> String {
        match self {
            Self::Annule => format!("Envoi de « {nom} » annulé."),
            Self::TropGros => {
                format!("Envoi de « {nom} » impossible : le clip réencodé dépasse encore la limite d'envoi.")
            }
            Self::Echec(raison) => format!("Envoi de « {nom} » impossible : {raison}."),
        }
    }
}

pub fn destinations(etat: &Etat) -> Vec<Destination> {
    let mut liste = Vec::new();
    for serveur in etat.serveurs_ordonnes() {
        let id = serveur.id.as_deref().unwrap_or_default();
        let nom_serveur = serveur.name.clone().unwrap_or_default();
        for salon in etat.salons(id) {
            let Some(salon_id) = salon.id.clone() else {
                continue;
            };
            let cle = CleConversation::Salon(salon_id);
            if salon.kind == Some(GenreSalon::Text)
                && etat.peut_dans(Some(&cle), "send_messages")
                && etat.peut_dans(Some(&cle), "attach_files")
            {
                liste.push(Destination {
                    cle,
                    nom: format!("# {}", salon.name.as_deref().unwrap_or_default()),
                    ou: nom_serveur.clone(),
                });
            }
        }
    }
    for prive in &etat.messages_prives {
        let Some(id) = prive.id.clone() else {
            continue;
        };
        liste.push(Destination {
            cle: CleConversation::Prive(id),
            nom: prive
                .other
                .as_ref()
                .and_then(|p| p.pseudo.clone())
                .unwrap_or_default(),
            ou: "Message privé".into(),
        });
    }
    liste
}

pub fn filtrer_destinations<'a>(liste: &'a [Destination], filtre: &str) -> Vec<&'a Destination> {
    let filtre = filtre.trim().to_lowercase();
    liste
        .iter()
        .filter(|d| {
            filtre.is_empty()
                || format!("{} {}", d.nom, d.ou)
                    .to_lowercase()
                    .contains(&filtre)
        })
        .collect()
}

pub fn texte_vide(
    filtre: Option<&str>,
    reglages: &ReglagesReplay,
    etat: &EtatReplay,
) -> (String, String, bool) {
    let titre = format!(
        "Aucun clip{}",
        if filtre.is_some() { " pour ce jeu" } else { "" }
    );
    if let EtatMoteur::Indisponible(raison) = &etat.etat {
        return (
            titre,
            format!("L'instant replay est indisponible : {raison}."),
            false,
        );
    }
    if reglages.actif {
        let texte = format!(
            "En jeu, appuie sur {} pour enregistrer les dernières secondes.",
            libelle_touche(&reglages.touche)
        );
        (titre, texte, false)
    } else {
        (titre, "L'instant replay est désactivé.".into(), true)
    }
}

pub fn vue(
    clips: &[Clip],
    stockage: Option<&Stockage>,
    filtre: Option<&str>,
    reglages: &ReglagesReplay,
    etat: &EtatReplay,
) -> VueGalerie {
    let jeux = jeux_de(clips);
    let filtre = filtre.filter(|f| jeux.iter().any(|j| j.jeu == *f));
    let lignes = clips::trier(clips, filtre)
        .into_iter()
        .map(|c| {
            let infos = [jeu_de(&c), date_clip(&c.date)]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            VueLigne {
                infos,
                details: details_clip(&c),
                duree: if c.duree_s > 0.0 {
                    duree_clip(c.duree_s)
                } else {
                    String::new()
                },
                origine: c.origine.clone().unwrap_or_default(),
                nom: c.nom,
                chemin: c.chemin,
            }
        })
        .collect();
    let (vide_titre, vide_texte, vide_reglages) = texte_vide(filtre, reglages, etat);
    let depasse = stockage.is_some_and(|s| s.depasse);
    let presque = stockage.is_some_and(presque_plein);
    let part = stockage.map_or(0, part_utilisee);
    let anciens = stockage.map_or(0, |s| s.plus_anciens.len());
    VueGalerie {
        visible: reglages.actif || !clips.is_empty(),
        titre: filtre.unwrap_or("Tous les clips").to_string(),
        tous: filtre.is_none(),
        total: clips.len(),
        lignes,
        jeux: jeux
            .into_iter()
            .map(|j| {
                let choisi = filtre == Some(j.jeu.as_str());
                (j.jeu, j.nombre, choisi)
            })
            .collect(),
        stockage_lu: stockage.is_some(),
        espace: stockage
            .map(|s| {
                format!(
                    "{} sur {}",
                    taille(s.utilise_octets),
                    taille(s.quota_octets)
                )
            })
            .unwrap_or_default(),
        part: part as f32 / 100.0,
        depasse,
        presque,
        alerte: if depasse {
            "Quota du dossier atteint : plus aucun clip n'est enregistré. Rien n'est supprimé sans ta confirmation.".into()
        } else if presque {
            format!("Quota presque atteint ({part} %) : supprime d'anciens clips ou augmente le quota pour continuer à enregistrer.")
        } else {
            String::new()
        },
        liberer: if depasse && anciens > 0 {
            "Supprimer les plus anciens…".into()
        } else {
            String::new()
        },
        vide_titre,
        vide_texte,
        vide_reglages,
    }
}

pub fn vue_envoi(clip: &Clip, limite: u64, leger: bool) -> VueEnvoi {
    let mode = if leger {
        Envoi::Leger
    } else {
        decision_envoi(clip, limite)
    };
    let limite = taille(limite);
    let mut v = VueEnvoi {
        clip: format!("{} · {}", clip.nom, taille(clip.taille_octets)),
        refus: mode == Envoi::Refus,
        alerte_titre: String::new(),
        alerte: String::new(),
        note: String::new(),
        action: "Envoyer".into(),
    };
    match mode {
        Envoi::Refus => {
            v.alerte_titre = "Clip trop long pour être envoyé".into();
            v.alerte = format!("Ce clip dépasse la limite d'envoi ({limite}) et, même réencodé, il ne tiendrait pas dedans avec une qualité lisible. Enregistre un clip plus court.");
        }
        Envoi::Leger => {
            v.alerte_titre = format!("Clip plus gros que la limite d'envoi ({limite})");
            v.alerte = "Il peut être réencodé plus léger, en H.264, pour tenir dans la limite : la qualité de la copie envoyée baisse, le clip d'origine n'est pas modifié.".into();
            v.action = "Réencoder et envoyer".into();
        }
        Envoi::H264 => {
            v.note = format!(
                "{} Le clip d'origine n'est pas modifié.",
                if clip.codec == Some(Codec::H264) {
                    "Les pistes du jeu et du micro sont réunies en une seule avant l'envoi."
                } else {
                    "Le clip est converti en H.264 avant l'envoi, pour être lisible sur le web et sur Android."
                }
            );
        }
        Envoi::Direct => {}
    }
    v
}

pub fn preparer_fichier(
    convertisseur: &Convertisseur,
    dossier: &Path,
    clip: &Path,
    taille_max: Option<u64>,
    annule: &AtomicBool,
    progres: &(dyn Fn(f32) + Sync),
) -> Result<Option<FichierTemporaire>, ErreurEnvoi> {
    let echec = ErreurEnvoi::Echec;
    let annexe = galerie::lire_annexe(dossier, clip);
    let taille_clip = std::fs::metadata(clip)
        .map_err(|e| echec(e.to_string()))?
        .len();
    let plan = plan_envoi(
        annexe.as_ref(),
        taille_clip,
        || convertisseur.duree(clip),
        taille_max,
    )
    .map_err(echec)?;
    let PlanEnvoi::Convertir {
        copie,
        kbps,
        hauteur,
        pistes,
        duree_s,
    } = plan
    else {
        return Ok(None);
    };
    let encodeurs: Vec<Option<&str>> = if copie {
        vec![None]
    } else {
        convertisseur
            .encodeurs
            .iter()
            .map(|e| Some(e.as_str()))
            .collect()
    };
    let sortie = convertisseur
        .sortie_temporaire()
        .map_err(|e| echec(e.to_string()))?;
    let mut derniere = SANS_ENCODEUR.to_string();
    for encodeur in encodeurs {
        let arguments = galerie::arguments_envoi(
            clip,
            sortie.chemin(),
            encodeur.map(|e| (e, kbps, hauteur)),
            pistes,
        );
        match convertisseur.executer(&arguments, duree_s, annule, progres) {
            Ok(()) => {
                let obtenu = std::fs::metadata(sortie.chemin()).map_or(0, |m| m.len());
                if taille_max.is_some_and(|m| obtenu > m) {
                    return Err(ErreurEnvoi::TropGros);
                }
                return Ok(Some(sortie));
            }
            Err(ErreurConversion::Annulee) => return Err(ErreurEnvoi::Annule),
            Err(ErreurConversion::Echec(e)) => derniere = e,
        }
    }
    Err(ErreurEnvoi::Echec(derniere))
}

#[derive(Clone)]
pub struct OutilsGalerie {
    pub service: Arc<ServiceReplay>,
    pub fichiers: SystemeFichiers,
    pub conversions: Arc<Conversions>,
    pub ffmpeg: Option<PathBuf>,
    pub messagerie: Option<SourcePont>,
}

struct Travail {
    id: u64,
    chemin: PathBuf,
    libelle: String,
    progression: Option<f32>,
    annule: Arc<AtomicBool>,
}

struct TacheEnvoi<'a> {
    clip: &'a Clip,
    cle: &'a CleConversation,
    store: &'a Store,
    taille_max: Option<u64>,
    limite: u64,
    id: u64,
    annule: Arc<AtomicBool>,
}

struct EnvoiOuvert {
    clip: Clip,
    destinations: Vec<Destination>,
    filtre: String,
    choisie: Option<CleConversation>,
    leger: bool,
    limite: u64,
}

#[derive(Default)]
struct EtatGalerie {
    clips: Vec<Clip>,
    stockage: Option<Stockage>,
    filtre: Option<String>,
    vignettes: HashMap<PathBuf, SharedPixelBuffer<Rgba8Pixel>>,
    tentees: HashSet<PathBuf>,
    travaux: Vec<Travail>,
    proteges: Vec<(u64, PathBuf)>,
    envoi: Option<EnvoiOuvert>,
    erreur: String,
    editeur: Option<OuvertureEditeur>,
}

pub struct Galerie {
    outils: OutilsGalerie,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    etat: Mutex<EtatGalerie>,
    compteur: AtomicU64,
    generation: AtomicU64,
}

pub struct Protection {
    galerie: Arc<Galerie>,
    id: u64,
}

impl Drop for Protection {
    fn drop(&mut self) {
        self.galerie
            .verrou()
            .proteges
            .retain(|(id, _)| *id != self.id);
    }
}

fn decoder(chemin: &Path) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let image = image::open(chemin).ok()?.to_rgba8();
    Some(SharedPixelBuffer::clone_from_slice(
        image.as_raw(),
        image.width(),
        image.height(),
    ))
}

impl Galerie {
    fn verrou(&self) -> MutexGuard<'_, EtatGalerie> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn suivant(&self) -> u64 {
        self.compteur.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn clips(&self) -> Vec<Clip> {
        self.verrou().clips.clone()
    }

    pub fn clip(&self, chemin: &Path) -> Option<Clip> {
        self.verrou()
            .clips
            .iter()
            .find(|c| c.chemin == chemin)
            .cloned()
    }

    pub fn proteges(&self) -> Vec<PathBuf> {
        self.verrou()
            .proteges
            .iter()
            .map(|(_, p)| p.clone())
            .collect()
    }

    pub fn est_protege(&self, chemin: &Path) -> bool {
        self.verrou().proteges.iter().any(|(_, p)| p == chemin)
    }

    pub fn proteger(self: &Arc<Self>, chemin: PathBuf) -> Protection {
        let id = self.suivant();
        self.verrou().proteges.push((id, chemin));
        Protection {
            galerie: self.clone(),
            id,
        }
    }

    pub fn definir_editeur(&self, f: &Principale, ouvrir: Option<OuvertureEditeur>) {
        f.global::<DonneesClips>()
            .set_modifier_disponible(ouvrir.is_some());
        self.verrou().editeur = ouvrir;
    }

    fn reglages(&self) -> ReglagesReplay {
        self.outils.service.reglages().replay
    }

    pub fn vue(&self) -> VueGalerie {
        let e = self.verrou();
        vue(
            &e.clips,
            e.stockage.as_ref(),
            e.filtre.as_deref(),
            &self.reglages(),
            &self.outils.service.actuel(),
        )
    }

    pub fn afficher(&self, f: &Principale) {
        let v = self.vue();
        let g = f.global::<DonneesClips>();
        let lignes: Vec<LigneClip> = {
            let e = self.verrou();
            let images: HashMap<&Path, &SharedPixelBuffer<Rgba8Pixel>> = e
                .clips
                .iter()
                .filter_map(|c| {
                    let image = e.vignettes.get(c.vignette.as_ref()?)?;
                    Some((c.chemin.as_path(), image))
                })
                .collect();
            v.lignes
                .iter()
                .map(|l| {
                    let vignette = images.get(l.chemin.as_path()).map(|i| (*i).clone());
                    LigneClip {
                        chemin: l.chemin.to_string_lossy().as_ref().into(),
                        nom: l.nom.as_str().into(),
                        infos: l.infos.as_str().into(),
                        details: l.details.as_str().into(),
                        duree: l.duree.as_str().into(),
                        origine: l.origine.as_str().into(),
                        a_vignette: vignette.is_some(),
                        vignette: vignette.map(Image::from_rgba8).unwrap_or_default(),
                    }
                })
                .collect()
        };
        g.set_visible(v.visible);
        g.set_titre(v.titre.as_str().into());
        g.set_tous(v.tous);
        g.set_total(v.total as i32);
        g.set_clips(ModelRc::new(VecModel::from(lignes)));
        g.set_jeux(ModelRc::new(VecModel::from(
            v.jeux
                .iter()
                .map(|(jeu, nombre, choisi)| JeuClips {
                    jeu: jeu.as_str().into(),
                    nombre: *nombre as i32,
                    choisi: *choisi,
                })
                .collect::<Vec<_>>(),
        )));
        g.set_stockage_lu(v.stockage_lu);
        g.set_espace(v.espace.as_str().into());
        g.set_part(v.part);
        g.set_depasse(v.depasse);
        g.set_presque(v.presque);
        g.set_alerte(v.alerte.as_str().into());
        g.set_liberer(v.liberer.as_str().into());
        g.set_vide_titre(v.vide_titre.as_str().into());
        g.set_vide_texte(v.vide_texte.as_str().into());
        g.set_vide_reglages(v.vide_reglages);
        g.set_envoi_disponible(self.outils.messagerie.is_some());
        self.afficher_travaux(f);
    }

    fn afficher_travaux(&self, f: &Principale) {
        let e = self.verrou();
        let g = f.global::<DonneesClips>();
        g.set_erreur(e.erreur.as_str().into());
        g.set_travaux(ModelRc::new(VecModel::from(
            e.travaux
                .iter()
                .map(|t| TravailClip {
                    id: t.id as i32,
                    libelle: t.libelle.as_str().into(),
                    progression: t.progression.unwrap_or(0.0),
                    a_progression: t.progression.is_some(),
                })
                .collect::<Vec<_>>(),
        )));
    }

    fn signaler(&self, f: &Principale, erreur: impl Into<String>) {
        self.verrou().erreur = erreur.into();
        self.afficher_travaux(f);
    }

    fn plus_tard(self: &Arc<Self>, action: impl FnOnce(&Arc<Self>, &Principale) + Send + 'static) {
        let galerie = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| action(&galerie, &f));
    }

    pub fn recharger(self: &Arc<Self>) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let galerie = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-clips".into())
            .spawn(move || galerie.relire(generation));
    }

    fn relire(self: &Arc<Self>, generation: u64) {
        let Ok(dossier) = self.outils.service.dossier() else {
            let mut e = self.verrou();
            e.clips.clear();
            e.stockage = None;
            drop(e);
            self.plus_tard(|g, f| g.afficher(f));
            return;
        };
        let quota = self.reglages().quota_octets;
        let clips = galerie::lister(&dossier);
        let stockage = galerie::stockage(&dossier, quota, &self.proteges());
        let a_decoder: Vec<PathBuf> = {
            let e = self.verrou();
            clips
                .iter()
                .filter_map(|c| c.vignette.clone())
                .filter(|v| !e.vignettes.contains_key(v))
                .collect()
        };
        let decodees: Vec<(PathBuf, SharedPixelBuffer<Rgba8Pixel>)> = a_decoder
            .into_iter()
            .filter_map(|v| decoder(&v).map(|d| (v, d)))
            .collect();
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        {
            let mut e = self.verrou();
            for (vignette, image) in decodees {
                e.vignettes.insert(vignette, image);
            }
            let gardees: HashSet<PathBuf> =
                clips.iter().filter_map(|c| c.vignette.clone()).collect();
            e.vignettes.retain(|v, _| gardees.contains(v));
            e.clips = clips.clone();
            e.stockage = Some(stockage);
        }
        self.plus_tard(|g, f| g.afficher(f));
        self.creer_vignettes(&dossier, &clips, generation);
    }

    fn creer_vignettes(self: &Arc<Self>, dossier: &Path, clips: &[Clip], generation: u64) {
        let Some(ffmpeg) = self.outils.ffmpeg.clone() else {
            return;
        };
        let manquants: Vec<PathBuf> = {
            let mut e = self.verrou();
            let liste: Vec<PathBuf> = clips
                .iter()
                .filter(|c| c.vignette.is_none() && !e.tentees.contains(&c.chemin))
                .map(|c| c.chemin.clone())
                .collect();
            e.tentees.extend(liste.iter().cloned());
            liste
        };
        for clip in manquants {
            if self.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let Ok(vignette) = galerie::creer_vignette(&ffmpeg, dossier, &clip) else {
                continue;
            };
            let Some(image) = decoder(&vignette) else {
                continue;
            };
            {
                let mut e = self.verrou();
                e.vignettes.insert(vignette.clone(), image);
                if let Some(c) = e.clips.iter_mut().find(|c| c.chemin == clip) {
                    c.vignette = Some(vignette);
                }
            }
            self.plus_tard(|g, f| g.afficher(f));
        }
    }

    pub fn ouvrir(self: &Arc<Self>, f: &Principale) {
        f.global::<DonneesReplay>().set_ouvert(false);
        f.global::<DonneesJeux>().set_ouvert(false);
        self.verrou().erreur.clear();
        self.afficher(f);
        f.global::<DonneesClips>().set_ouvert(true);
        self.recharger();
    }

    pub fn filtrer(&self, f: &Principale, jeu: &str) {
        self.verrou().filtre = (!jeu.is_empty()).then(|| jeu.to_string());
        f.global::<DonneesClips>().set_decalage(0.0);
        self.afficher(f);
    }

    fn sur_un_fil(self: &Arc<Self>, nom: &str, action: impl FnOnce(&Arc<Self>) + Send + 'static) {
        let galerie = self.clone();
        let _ = std::thread::Builder::new()
            .name(format!("estompe-clips-{nom}"))
            .spawn(move || action(&galerie));
    }

    pub fn lire(self: &Arc<Self>, chemin: &Path) {
        let chemin = chemin.to_path_buf();
        self.sur_un_fil("lecture", move |g| {
            if let Err(e) = g.outils.fichiers.ouvrir(&chemin) {
                let message = format!("Lecture impossible : {e}");
                g.plus_tard(move |g, f| g.signaler(f, message));
            }
        });
    }

    pub fn ouvrir_dossier(self: &Arc<Self>) {
        self.sur_un_fil("dossier", |g| {
            let resultat = g.outils.service.dossier().and_then(|d| {
                std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
                g.outils
                    .fichiers
                    .ouvrir_dossier(&d)
                    .map_err(|e| e.to_string())
            });
            if let Err(e) = resultat {
                let message = format!("Ouverture du dossier impossible : {e}");
                g.plus_tard(move |g, f| g.signaler(f, message));
            }
        });
    }

    fn apres(self: &Arc<Self>, resultat: Result<(), String>) {
        if let Err(e) = resultat {
            self.plus_tard(move |g, f| g.signaler(f, e));
        } else {
            self.plus_tard(|g, f| g.signaler(f, ""));
        }
        self.recharger();
    }

    pub fn renommer_en(self: &Arc<Self>, chemin: &Path, nom: String) {
        let chemin = chemin.to_path_buf();
        self.sur_un_fil("renommage", move |g| {
            let resultat = g
                .outils
                .service
                .dossier()
                .and_then(|d| galerie::renommer(&d, &chemin, &nom).map(|_| ()));
            g.apres(resultat.map_err(|e| format!("Renommage impossible : {e}.")));
        });
    }

    pub fn supprimer_definitivement(self: &Arc<Self>, chemins: Vec<PathBuf>) {
        self.sur_un_fil("suppression", move |g| {
            let resultat = g
                .outils
                .service
                .dossier()
                .and_then(|d| galerie::supprimer(&d, &chemins));
            g.apres(resultat.map_err(|e| format!("Suppression impossible : {e}.")));
        });
    }

    pub fn purger(self: &Arc<Self>) {
        self.sur_un_fil("purge", |g| {
            let quota = g.reglages().quota_octets;
            let proteges = g.proteges();
            let resultat = g
                .outils
                .service
                .dossier()
                .and_then(|d| galerie::purger(&d, quota, &proteges).map(|_| ()));
            g.apres(resultat.map_err(|e| format!("Suppression impossible : {e}.")));
        });
    }

    fn refuser_si_protege(&self, f: &Principale, chemin: &Path) -> bool {
        let protege = self.est_protege(chemin);
        if protege {
            self.signaler(f, "Ce clip est en cours d'utilisation (envoi ou montage) : réessaie quand il sera terminé.");
        }
        protege
    }

    pub fn demander_renommage(self: &Arc<Self>, f: &Principale, chemin: &Path) {
        let Some(clip) = self.clip(chemin) else {
            return;
        };
        if self.refuser_si_protege(f, chemin) {
            return;
        }
        let galerie = self.clone();
        confirmation::saisir(
            f,
            Demande {
                titre: "Renommer le clip".into(),
                action: "Renommer".into(),
                saisie: Some(Saisie {
                    libelle: "Nom du clip".into(),
                    valeur: clip.nom.clone(),
                    exiger: None,
                }),
                ..Demande::default()
            },
            move |_, nom| {
                if let Some(nom) = nom.filter(|n| !n.trim().is_empty() && n.trim() != clip.nom) {
                    galerie.renommer_en(&clip.chemin, nom);
                }
            },
        );
    }

    pub fn demander_suppression(self: &Arc<Self>, f: &Principale, chemin: &Path) {
        let Some(clip) = self.clip(chemin) else {
            return;
        };
        if self.refuser_si_protege(f, chemin) {
            return;
        }
        let galerie = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le clip".into(),
                message: format!(
                    "Supprimer définitivement « {} » ({}) ? Le fichier est effacé du disque.",
                    clip.nom,
                    taille(clip.taille_octets)
                ),
                action: "Supprimer".into(),
                destructif: true,
                ..Demande::default()
            },
            move |_, ok| {
                if ok {
                    galerie.supprimer_definitivement(vec![clip.chemin]);
                }
            },
        );
    }

    pub fn demander_purge(self: &Arc<Self>, f: &Principale) {
        let (n, octets) = {
            let e = self.verrou();
            let anciens = e
                .stockage
                .as_ref()
                .map(|s| s.plus_anciens.clone())
                .unwrap_or_default();
            (anciens.len(), taille_liberee(&e.clips, &anciens))
        };
        if n == 0 {
            return;
        }
        let galerie = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer les clips les plus anciens".into(),
                message: format!(
                    "Supprimer définitivement {} ({}) pour repasser sous le quota ? Un clip en cours d'envoi est gardé.",
                    if n > 1 {
                        format!("les {n} clips les plus anciens")
                    } else {
                        "le clip le plus ancien".into()
                    },
                    taille(octets)
                ),
                action: if n > 1 {
                    format!("Supprimer {n} clips")
                } else {
                    "Supprimer le clip".into()
                },
                destructif: true,
                ..Demande::default()
            },
            move |_, ok| {
                if ok {
                    galerie.purger();
                }
            },
        );
    }

    pub fn modifier(self: &Arc<Self>, f: &Principale, chemin: &Path) {
        let editeur = self.verrou().editeur.clone();
        if let (Some(ouvrir), Some(clip)) = (editeur, self.clip(chemin)) {
            ouvrir(f, &clip);
        }
    }

    fn store(&self) -> Option<Store> {
        self.outils
            .messagerie
            .as_ref()
            .and_then(|source| source())
            .map(|pont| pont.store().clone())
    }

    pub fn ouvrir_envoi(&self, f: &Principale, chemin: &Path, leger: bool) {
        let Some(clip) = self.clip(chemin) else {
            return;
        };
        let Some(store) = self.store() else {
            self.signaler(f, "Connecte-toi pour envoyer un clip dans un salon.");
            return;
        };
        let (destinations, limite) = store.lire(|etat| {
            (
                destinations(etat),
                etat.compte
                    .max_video_size
                    .and_then(|m| u64::try_from(m).ok())
                    .filter(|m| *m > 0)
                    .unwrap_or(LIMITE_PAR_DEFAUT),
            )
        });
        self.verrou().envoi = Some(EnvoiOuvert {
            clip,
            destinations,
            filtre: String::new(),
            choisie: None,
            leger,
            limite,
        });
        let g = f.global::<DonneesClips>();
        g.set_envoi_filtre("".into());
        g.set_envoi_erreur("".into());
        self.afficher_envoi(f);
        g.set_envoi_ouvert(true);
    }

    fn afficher_envoi(&self, f: &Principale) {
        let e = self.verrou();
        let Some(envoi) = &e.envoi else {
            return;
        };
        let v = vue_envoi(&envoi.clip, envoi.limite, envoi.leger);
        let g = f.global::<DonneesClips>();
        g.set_envoi_clip(v.clip.as_str().into());
        g.set_envoi_refus(v.refus);
        g.set_envoi_alerte_titre(v.alerte_titre.as_str().into());
        g.set_envoi_alerte(v.alerte.as_str().into());
        g.set_envoi_note(v.note.as_str().into());
        g.set_envoi_action(v.action.as_str().into());
        g.set_destination_choisie(envoi.choisie.is_some());
        g.set_destinations(ModelRc::new(VecModel::from(
            filtrer_destinations(&envoi.destinations, &envoi.filtre)
                .into_iter()
                .map(|d| DestinationClip {
                    cle: d.cle.to_string().into(),
                    nom: d.nom.as_str().into(),
                    ou: d.ou.as_str().into(),
                    choisie: envoi.choisie.as_ref() == Some(&d.cle),
                })
                .collect::<Vec<_>>(),
        )));
    }

    pub fn filtrer_destinations(&self, f: &Principale, texte: &str) {
        if let Some(envoi) = self.verrou().envoi.as_mut() {
            envoi.filtre = texte.to_string();
        }
        self.afficher_envoi(f);
    }

    pub fn choisir_destination(&self, f: &Principale, cle: &str) {
        if let Some(envoi) = self.verrou().envoi.as_mut() {
            envoi.choisie = CleConversation::analyser(cle);
        }
        self.afficher_envoi(f);
    }

    pub fn fermer_envoi(&self, f: &Principale) {
        self.verrou().envoi = None;
        f.global::<DonneesClips>().set_envoi_ouvert(false);
    }

    pub fn confirmer_envoi(self: &Arc<Self>, f: &Principale) {
        let envoi = self.verrou().envoi.take();
        let Some(envoi) = envoi else {
            return;
        };
        let (Some(cle), Some(store)) = (envoi.choisie.clone(), self.store()) else {
            self.verrou().envoi = Some(envoi);
            return;
        };
        f.global::<DonneesClips>().set_envoi_ouvert(false);
        let mode = if envoi.leger {
            Envoi::Leger
        } else {
            decision_envoi(&envoi.clip, envoi.limite)
        };
        if mode == Envoi::Refus {
            return;
        }
        self.lancer_envoi(f, envoi.clip, cle, store, mode, envoi.limite);
    }

    fn progression(&self, id: u64, libelle: Option<String>, part: Option<f32>) -> bool {
        let mut e = self.verrou();
        let Some(t) = e.travaux.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        let avant = (t.libelle.clone(), t.progression.map(|p| (p * 100.0) as u32));
        if let Some(l) = libelle {
            t.libelle = l;
        }
        t.progression = part;
        avant != (t.libelle.clone(), t.progression.map(|p| (p * 100.0) as u32))
    }

    fn lancer_envoi(
        self: &Arc<Self>,
        f: &Principale,
        clip: Clip,
        cle: CleConversation,
        store: Store,
        mode: Envoi,
        limite: u64,
    ) {
        let id = self.suivant();
        let annule = Arc::new(AtomicBool::new(false));
        let protection = self.proteger(clip.chemin.clone());
        {
            let mut e = self.verrou();
            e.erreur.clear();
            e.travaux.push(Travail {
                id,
                chemin: clip.chemin.clone(),
                libelle: format!("Envoi de « {} »", clip.nom),
                progression: None,
                annule: annule.clone(),
            });
        }
        self.afficher_travaux(f);
        let galerie = self.clone();
        let taille_max = (mode == Envoi::Leger).then_some(limite);
        self.runtime.spawn(async move {
            let _protection = protection;
            let resultat = galerie
                .envoyer(TacheEnvoi {
                    clip: &clip,
                    cle: &cle,
                    store: &store,
                    taille_max,
                    limite,
                    id,
                    annule,
                })
                .await;
            let nom = clip.nom.clone();
            galerie.verrou().travaux.retain(|t| t.id != id);
            galerie.plus_tard(move |g, f| match resultat {
                Ok(()) => g.signaler(f, ""),
                Err(ErreurEnvoi::TropGros) if mode != Envoi::Leger => {
                    g.afficher_travaux(f);
                    g.ouvrir_envoi(f, &clip.chemin, true);
                }
                Err(e) => g.signaler(f, e.message(&nom)),
            });
        });
    }

    async fn envoyer(self: &Arc<Self>, tache: TacheEnvoi<'_>) -> Result<(), ErreurEnvoi> {
        let TacheEnvoi {
            clip,
            cle,
            store,
            taille_max,
            limite,
            id,
            annule,
        } = tache;
        let dossier = self.outils.service.dossier().map_err(ErreurEnvoi::Echec)?;
        let convertisseur = self
            .outils
            .conversions
            .convertisseur()
            .cloned()
            .unwrap_or_else(|| Convertisseur {
                ffmpeg: self.outils.ffmpeg.clone().unwrap_or_else(ffmpeg_du_systeme),
                encodeurs: Vec::new(),
                dossier: self.outils.conversions.dossier().to_path_buf(),
            });
        let suivi = self.clone();
        let nom = clip.nom.clone();
        let chemin = clip.chemin.clone();
        let arret = annule.clone();
        let fichier = tokio::task::spawn_blocking(move || {
            let libelle = format!("Conversion de « {nom} »");
            preparer_fichier(
                &convertisseur,
                &dossier,
                &chemin,
                taille_max,
                &arret,
                &|p| {
                    if suivi.progression(id, Some(libelle.clone()), Some(p)) {
                        suivi.plus_tard(|g, f| g.afficher_travaux(f));
                    }
                },
            )
        })
        .await
        .map_err(|e| ErreurEnvoi::Echec(e.to_string()))??;
        let source = fichier
            .as_ref()
            .map_or_else(|| clip.chemin.clone(), |f| f.chemin().to_path_buf());
        if std::fs::metadata(&source).map_or(0, |m| m.len()) > limite {
            return Err(ErreurEnvoi::TropGros);
        }
        if annule.load(Ordering::SeqCst) {
            return Err(ErreurEnvoi::Annule);
        }
        self.progression(id, Some(format!("Envoi de « {} »", clip.nom)), Some(0.0));
        self.plus_tard(|g, f| g.afficher_travaux(f));
        let suivi = self.clone();
        let progres: Progression = Arc::new(move |envoyes, total| {
            let part = envoyes as f32 / total.max(1) as f32;
            if suivi.progression(id, None, Some(part)) {
                suivi.plus_tard(|g, f| g.afficher_travaux(f));
            }
        });
        let client = store.client().clone();
        let nom = format!("{}.mp4", clip.nom);
        let televersement =
            client.envoyer_piece(&nom, TYPE_MP4, SourceEnvoi::Chemin(source), progres);
        let piece = tokio::select! {
            resultat = televersement => resultat.map_err(|e| ErreurEnvoi::Echec(e.message().to_string()))?,
            () = attendre_l_annulation(&annule) => return Err(ErreurEnvoi::Annule),
        };
        drop(fichier);
        store
            .envoyer(
                cle,
                Brouillon {
                    pieces: vec![piece],
                    ..Brouillon::default()
                },
            )
            .await;
        Ok(())
    }

    pub fn annuler_travail(&self, id: u64) {
        if let Some(t) = self.verrou().travaux.iter().find(|t| t.id == id) {
            t.annule.store(true, Ordering::SeqCst);
        }
    }

    pub fn travaux_en_cours(&self) -> Vec<PathBuf> {
        self.verrou()
            .travaux
            .iter()
            .map(|t| t.chemin.clone())
            .collect()
    }
}

async fn attendre_l_annulation(annule: &AtomicBool) {
    while !annule.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

pub fn brancher(
    f: &Principale,
    outils: OutilsGalerie,
    runtime: &tokio::runtime::Handle,
) -> Arc<Galerie> {
    let mut evenements = outils.service.evenements();
    let g = Arc::new(Galerie {
        outils,
        fenetre: f.as_weak(),
        runtime: runtime.clone(),
        etat: Mutex::new(EtatGalerie::default()),
        compteur: AtomicU64::new(0),
        generation: AtomicU64::new(0),
    });
    g.afficher(f);
    g.recharger();
    let suivi = g.clone();
    runtime.spawn(async move {
        loop {
            match evenements.recv().await {
                Ok(EvenementReplay::Clip(_)) => suivi.recharger(),
                Ok(EvenementReplay::Erreur(e)) if e.code == "quota" => suivi.recharger(),
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => suivi.recharger(),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    let d = f.global::<DonneesClips>();
    macro_rules! rappel {
        ($methode:ident, |$g:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $g = g.clone();
            let faible = f.as_weak();
            d.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_ouvrir, |g, f| g.ouvrir(&f));
    rappel!(on_fermer, |_g, f| f
        .global::<DonneesClips>()
        .set_ouvert(false));
    rappel!(on_filtrer, |g, f, jeu| g.filtrer(&f, &jeu));
    rappel!(on_lire, |g, _f, chemin| g.lire(Path::new(chemin.as_str())));
    rappel!(on_ouvrir_dossier, |g, _f| g.ouvrir_dossier());
    rappel!(on_reglages, |_g, f| {
        f.global::<DonneesClips>().set_ouvert(false);
        f.global::<DonneesReplay>().invoke_ouvrir();
    });
    rappel!(on_renommer, |g, f, chemin| g
        .demander_renommage(&f, Path::new(chemin.as_str())));
    rappel!(on_supprimer, |g, f, chemin| g
        .demander_suppression(&f, Path::new(chemin.as_str())));
    rappel!(on_liberer_espace, |g, f| g.demander_purge(&f));
    rappel!(on_envoyer, |g, f, chemin| g.ouvrir_envoi(
        &f,
        Path::new(chemin.as_str()),
        false
    ));
    rappel!(on_modifier, |g, f, chemin| g
        .modifier(&f, Path::new(chemin.as_str())));
    rappel!(on_annuler_travail, |g, _f, id| g
        .annuler_travail(id.max(0) as u64));
    rappel!(on_filtrer_destinations, |g, f, texte| g
        .filtrer_destinations(&f, &texte));
    rappel!(on_choisir_destination, |g, f, cle| g
        .choisir_destination(&f, &cle));
    rappel!(on_confirmer_envoi, |g, f| g.confirmer_envoi(&f));
    rappel!(on_fermer_envoi, |g, f| g.fermer_envoi(&f));
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::replay::automate::Etat as Moteur;
    use estompe_client::replay::galerie::Annexe;
    use serde_json::json;

    fn clip(nom: &str, jeu: Option<&str>, date: &str) -> Clip {
        Clip {
            chemin: PathBuf::from(format!("/v/Estompe/{nom}.mp4")),
            nom: nom.into(),
            jeu: jeu.map(String::from),
            date: date.into(),
            duree_s: 75.4,
            taille_octets: 48 * 1024 * 1024,
            vignette: None,
            codec: Some(Codec::H264),
            largeur: 1920,
            hauteur: 1080,
            ips: 60,
            pistes_audio: 1,
            origine: None,
        }
    }

    fn stockage(utilise: u64, quota: u64, anciens: usize) -> Stockage {
        Stockage {
            utilise_octets: utilise,
            quota_octets: quota,
            depasse: utilise >= quota,
            plus_anciens: (0..anciens)
                .map(|i| PathBuf::from(format!("/v/Estompe/{i}.mp4")))
                .collect(),
        }
    }

    #[test]
    fn galerie_vide_selon_l_etat_du_replay() {
        let mut r = ReglagesReplay::default();
        let etat = EtatReplay::default();
        let v = vue(&[], None, None, &r, &etat);
        assert!(!v.visible && v.lignes.is_empty() && !v.stockage_lu);
        assert_eq!(
            (
                v.vide_titre.as_str(),
                v.vide_texte.as_str(),
                v.vide_reglages
            ),
            ("Aucun clip", "L'instant replay est désactivé.", true)
        );
        r.actif = true;
        let v = vue(&[], Some(&stockage(0, 20 << 30, 0)), None, &r, &etat);
        assert!(v.visible && v.stockage_lu && !v.vide_reglages);
        assert_eq!(
            v.vide_texte,
            "En jeu, appuie sur F10 pour enregistrer les dernières secondes."
        );
        assert_eq!(v.espace, "0 Ko sur 20 Go");
        let indisponible = EtatReplay {
            etat: Moteur::Indisponible(super::super::HORS_WINDOWS.into()),
            ..EtatReplay::default()
        };
        assert_eq!(
            vue(&[], None, None, &r, &indisponible).vide_texte,
            "L'instant replay est indisponible : instant replay disponible sous Windows uniquement."
        );
    }

    #[test]
    fn galerie_remplie_par_jeu_et_par_date() {
        let mut montage = clip("Montage 1", Some("Valorant"), "2026-03-09T20:00:00");
        montage.origine = Some("Valorant 2026-03-07 09-05-02".into());
        let clips = [
            clip(
                "Valorant 2026-03-07 09-05-02",
                Some("Valorant"),
                "2026-03-07T09:05:02",
            ),
            clip(
                "CS2 2026-03-08 21-14-00",
                Some("Counter-Strike 2"),
                "2026-03-08T21:14:00",
            ),
            montage,
            clip("copie", None, "2026-01-02T08:00:00Z"),
        ];
        let r = ReglagesReplay::default();
        let v = vue(&clips, None, None, &r, &EtatReplay::default());
        assert!(v.visible && v.tous && v.titre == "Tous les clips");
        assert_eq!(v.total, 4);
        let noms: Vec<&str> = v.lignes.iter().map(|l| l.nom.as_str()).collect();
        assert_eq!(
            noms,
            [
                "Montage 1",
                "CS2 2026-03-08 21-14-00",
                "Valorant 2026-03-07 09-05-02",
                "copie"
            ]
        );
        assert_eq!(v.lignes[1].infos, "Counter-Strike 2 · 8 mars 2026 à 21:14");
        assert_eq!(v.lignes[1].details, "48 Mo · 1080p · 60 i/s");
        assert_eq!(v.lignes[1].duree, "1:15");
        assert_eq!(v.lignes[0].origine, "Valorant 2026-03-07 09-05-02");
        assert_eq!(
            v.jeux,
            [
                ("Counter-Strike 2".to_string(), 1, false),
                ("Valorant".to_string(), 2, false),
                ("Autres".to_string(), 1, false),
            ]
        );
        let v = vue(&clips, None, Some("Valorant"), &r, &EtatReplay::default());
        assert!(!v.tous && v.titre == "Valorant" && v.lignes.len() == 2);
        assert!(v.jeux[1].2);
        let v = vue(&clips, None, Some("Disparu"), &r, &EtatReplay::default());
        assert!(v.tous && v.lignes.len() == 4);
    }

    #[test]
    fn quota_presque_atteint_puis_depasse() {
        let r = ReglagesReplay::default();
        let clips = [clip("a", None, "")];
        let go = 1u64 << 30;
        let v = vue(
            &clips,
            Some(&stockage(18 * go + go * 6 / 10, 20 * go, 0)),
            None,
            &r,
            &EtatReplay::default(),
        );
        assert!(v.presque && !v.depasse && v.liberer.is_empty());
        assert_eq!(v.espace, "19 Go sur 20 Go");
        assert_eq!((v.part * 100.0).round(), 93.0);
        assert!(v.alerte.starts_with("Quota presque atteint (93 %)"));
        let v = vue(
            &clips,
            Some(&stockage(21 * go, 20 * go, 2)),
            None,
            &r,
            &EtatReplay::default(),
        );
        assert!(v.depasse && !v.presque && v.part == 1.0);
        assert_eq!(v.liberer, "Supprimer les plus anciens…");
        assert!(v.alerte.starts_with("Quota du dossier atteint"));
        let v = vue(
            &clips,
            Some(&stockage(21 * go, 20 * go, 0)),
            None,
            &r,
            &EtatReplay::default(),
        );
        assert!(v.depasse && v.liberer.is_empty());
    }

    #[test]
    fn fenetre_d_envoi_selon_le_clip() {
        let limite = 100 * 1024 * 1024;
        let mut c = clip("a", None, "");
        let v = vue_envoi(&c, limite, false);
        assert_eq!(v.clip, "a · 48 Mo");
        assert!(!v.refus && v.alerte_titre.is_empty() && v.note.is_empty());
        assert_eq!(v.action, "Envoyer");
        c.pistes_audio = 2;
        assert!(vue_envoi(&c, limite, false)
            .note
            .starts_with("Les pistes du jeu et du micro"));
        c.codec = Some(Codec::Hevc);
        assert!(vue_envoi(&c, limite, false)
            .note
            .starts_with("Le clip est converti en H.264"));
        let leger = vue_envoi(&c, limite, true);
        assert_eq!(
            leger.alerte_titre,
            "Clip plus gros que la limite d'envoi (100 Mo)"
        );
        assert_eq!(leger.action, "Réencoder et envoyer");
        c.taille_octets = 2 * limite;
        c.duree_s = 5000.0;
        let refus = vue_envoi(&c, limite, false);
        assert!(refus.refus && refus.alerte_titre == "Clip trop long pour être envoyé");
    }

    #[test]
    fn destinations_ou_l_on_peut_joindre_un_fichier() {
        let mut etat = Etat {
            serveurs: vec![
                serde_json::from_value(json!({"id": "s1", "name": "Les copains"})).unwrap(),
            ],
            messages_prives: vec![serde_json::from_value(
                json!({"id": "d1", "other": {"id": "u2", "pseudo": "Bob"}}),
            )
            .unwrap()],
            ..Etat::default()
        };
        etat.details.insert(
            "s1".into(),
            serde_json::from_value(json!({
                "permissions": ["send_messages", "attach_files"],
                "channels": [
                    {"id": "c1", "name": "general", "kind": "text"},
                    {"id": "c2", "name": "annonces", "kind": "text", "permissions": ["send_messages"]},
                    {"id": "c3", "name": "Vocal", "kind": "voice"},
                    {"id": "c4", "name": "clips", "kind": "text"}
                ]
            }))
            .unwrap(),
        );
        let liste = destinations(&etat);
        let noms: Vec<(&str, &str)> = liste
            .iter()
            .map(|d| (d.nom.as_str(), d.ou.as_str()))
            .collect();
        assert_eq!(
            noms,
            [
                ("# general", "Les copains"),
                ("# clips", "Les copains"),
                ("Bob", "Message privé")
            ]
        );
        assert_eq!(liste[2].cle, CleConversation::Prive("d1".into()));
        let filtre: Vec<&str> = filtrer_destinations(&liste, " CLI ")
            .iter()
            .map(|d| d.nom.as_str())
            .collect();
        assert_eq!(filtre, ["# clips"]);
        assert_eq!(filtrer_destinations(&liste, "privé").len(), 1);
        assert_eq!(filtrer_destinations(&liste, "").len(), 3);
    }

    fn ffmpeg(arguments: &[&str]) -> Option<std::process::Output> {
        std::process::Command::new("ffmpeg")
            .arg("-nostdin")
            .args(arguments)
            .output()
            .ok()
    }

    fn convertisseur(dossier: &Path) -> Convertisseur {
        Convertisseur {
            ffmpeg: PathBuf::from("ffmpeg"),
            encodeurs: Vec::new(),
            dossier: dossier.join("conversions"),
        }
    }

    fn annexer(dossier: &Path, clip: &Path, codec: Codec, pistes: u32) {
        galerie::creer_annexes(dossier).unwrap();
        let a = Annexe {
            jeu: "Jeu".into(),
            date: "2026-01-01T10:00:00".into(),
            duree_s: 1.0,
            codec,
            largeur: 320,
            hauteur: 180,
            ips: 10,
            pistes_audio: pistes,
            origine: None,
        };
        std::fs::write(
            galerie::annexe(dossier, clip, "json"),
            serde_json::to_vec(&a).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn envoi_prepare_le_fichier_comme_l_app_tauri() {
        let d = tempfile::tempdir().unwrap();
        let d = d.path();
        let source = d.join("Jeu 2026-01-01 10-00-00.mp4");
        let fabrique = ffmpeg(&[
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x180:rate=10:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=1",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-y",
            source.to_str().unwrap(),
        ]);
        if !fabrique.is_some_and(|o| o.status.success()) {
            eprintln!("ffmpeg absent : préparation de l'envoi non testée");
            return;
        }
        let c = convertisseur(d);
        let annule = AtomicBool::new(false);
        let parts = Mutex::new(Vec::new());
        let suivre = |p: f32| parts.lock().unwrap().push(p);
        annexer(d, &source, Codec::H264, 1);
        assert_eq!(
            preparer_fichier(&c, d, &source, None, &annule, &suivre),
            Ok(None)
        );
        annexer(d, &source, Codec::H264, 2);
        let fichier = preparer_fichier(&c, d, &source, None, &annule, &suivre)
            .unwrap()
            .unwrap();
        let sortie = fichier.chemin().to_path_buf();
        assert!(sortie.starts_with(d.join("conversions")) && sortie.is_file());
        let lu = ffmpeg(&["-hide_banner", "-i", sortie.to_str().unwrap()]).unwrap();
        let description = String::from_utf8_lossy(&lu.stderr);
        assert_eq!(description.matches("Audio:").count(), 1, "{description}");
        assert!(parts.lock().unwrap().last().is_some_and(|p| *p >= 1.0));
        drop(fichier);
        assert!(!sortie.exists());
        assert_eq!(
            preparer_fichier(&c, d, &source, Some(1000), &annule, &suivre),
            Err(ErreurEnvoi::Echec(
                "clip trop long pour tenir dans la limite d'envoi".into()
            ))
        );
        annexer(d, &source, Codec::Hevc, 2);
        assert_eq!(
            preparer_fichier(&c, d, &source, None, &annule, &suivre),
            Err(ErreurEnvoi::Echec(SANS_ENCODEUR.into()))
        );
        annexer(d, &source, Codec::H264, 2);
        annule.store(true, Ordering::SeqCst);
        assert_eq!(
            preparer_fichier(&c, d, &source, None, &annule, &suivre),
            Err(ErreurEnvoi::Annule)
        );
    }

    #[test]
    fn messages_d_echec_de_l_envoi() {
        assert_eq!(ErreurEnvoi::Annule.message("a"), "Envoi de « a » annulé.");
        assert!(ErreurEnvoi::TropGros
            .message("a")
            .contains("dépasse encore la limite"));
        assert_eq!(
            ErreurEnvoi::Echec("réseau".into()).message("a"),
            "Envoi de « a » impossible : réseau."
        );
    }
}
