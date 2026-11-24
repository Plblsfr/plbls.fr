use std::future::Future;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::appels::Envoi;
use estompe_client::creation::message_d_erreur;
use estompe_client::format::date_heure_complete;
use estompe_client::http::{Client, ErreurApi};
use estompe_client::journal::Journal;
use estompe_client::route::Route;
use estompe_client::signalements::{
    ecran_du_salon, formater_signalement, informations_techniques, journal_recent, libelle,
    modification, requete, resume_admin, resume_mes_signalements, statut, ton, vocal_actif,
    Brouillon, Cible, Environnement, Filtres, Signalement, CATEGORIES, MAX_CAPTURES, PLATEFORMES,
    STATUTS,
};
use estompe_client::vocal::Phase;
use estompe_plateforme::{capturer_fenetre_d_estompe, CaptureEcran};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use slint::{
    ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel,
};

use super::admin::Decalage;
use super::Pont;
use crate::confirmation::{self, Demande};
use crate::menu::{self, Ancre, Entree};
use crate::messagerie::pont::SystemeFichiers;
use crate::{
    CaptureSignalement, ChoixSignalement, CleValeurSignalement, DonneesSignalements,
    InfoSignalement, LigneSignalement, Principale,
};

pub const TITRE_FENETRE: &str = "Estompe natif (beta)";
pub const LARGEUR_ENVOYEE: u32 = 1440;
pub const QUALITE_JPEG: u8 = 80;
pub const HAUTEUR_APERCU: u32 = 108;
pub const ATTENTE_AFFICHAGE: Duration = Duration::from_millis(150);
pub const MESSAGE_ENVOYE: &str = "Signalement envoyé, merci !";

type Pixels = SharedPixelBuffer<Rgba8Pixel>;

#[derive(Clone)]
struct Piece {
    nom: String,
    type_mime: &'static str,
    octets: Vec<u8>,
    apercu: Pixels,
    id: Option<String>,
}

#[derive(Clone)]
struct Formulaire {
    cible: Cible,
    categorie: String,
    informations: Vec<(String, String, bool)>,
    pieces: Vec<Piece>,
    erreur: String,
    en_cours: bool,
    masquee: bool,
}

#[derive(Default)]
struct EtatSignalements {
    formulaire: Option<Formulaire>,
    generation: u64,
    mes: Option<Vec<Signalement>>,
    erreur_mes: String,
    info_mes: String,
    filtres: [usize; 3],
    liste: Vec<Signalement>,
    detail: Option<Signalement>,
    captures: Vec<(String, Pixels)>,
    statut: String,
    origine: Option<String>,
    erreur_admin: String,
    info_admin: String,
    en_cours_admin: bool,
    generation_admin: u64,
}

#[derive(Clone)]
pub struct PontSignalements {
    interne: Arc<Mutex<EtatSignalements>>,
    client: Client,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    decalage: Decalage,
    capture: Arc<Mutex<Option<Arc<dyn CaptureEcran>>>>,
    fichiers: Arc<Mutex<Option<SystemeFichiers>>>,
    journal: Arc<Mutex<Option<Arc<Journal>>>>,
    dossier: Arc<Mutex<Option<PathBuf>>>,
}

fn modele<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(lignes))
}

fn choix(liste: &[(&str, &str)]) -> ModelRc<ChoixSignalement> {
    modele(
        liste
            .iter()
            .map(|(id, l)| ChoixSignalement {
                id: (*id).into(),
                libelle: (*l).into(),
            })
            .collect(),
    )
}

fn pixels(image: &RgbaImage) -> Pixels {
    SharedPixelBuffer::clone_from_slice(image.as_raw(), image.width(), image.height())
}

fn apercu(image: &DynamicImage) -> Pixels {
    let reduite = if image.height() > HAUTEUR_APERCU {
        image.resize(u32::MAX, HAUTEUR_APERCU, FilterType::Triangle)
    } else {
        image.clone()
    };
    pixels(&reduite.to_rgba8())
}

pub fn jpeg_de_la_capture(capture: &estompe_plateforme::Image) -> Option<(Vec<u8>, DynamicImage)> {
    let rgba = RgbaImage::from_raw(capture.largeur, capture.hauteur, capture.rgba.clone())?;
    let mut image = DynamicImage::ImageRgba8(rgba);
    if image.width() > LARGEUR_ENVOYEE {
        image = image.resize(LARGEUR_ENVOYEE, u32::MAX, FilterType::Triangle);
    }
    let mut octets = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut octets), QUALITE_JPEG)
        .encode_image(&image.to_rgb8())
        .ok()?;
    Some((octets, image))
}

fn type_d_image(octets: &[u8]) -> Option<&'static str> {
    match image::guess_format(octets).ok()? {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::Gif => Some("image/gif"),
        image::ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

impl PontSignalements {
    pub fn nouveau(
        client: Client,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        decalage: Decalage,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(EtatSignalements::default())),
            client,
            fenetre,
            runtime,
            decalage,
            capture: Arc::new(Mutex::new(None)),
            fichiers: Arc::new(Mutex::new(None)),
            journal: Arc::new(Mutex::new(None)),
            dossier: Arc::new(Mutex::new(None)),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, EtatSignalements> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn definir_capture(&self, capture: Option<Arc<dyn CaptureEcran>>) {
        *self.capture.lock().unwrap_or_else(|e| e.into_inner()) = capture;
    }

    pub fn definir_fichiers(&self, fichiers: SystemeFichiers) {
        *self.fichiers.lock().unwrap_or_else(|e| e.into_inner()) = Some(fichiers);
    }

    pub fn definir_journal(&self, journal: Option<Arc<Journal>>) {
        *self.journal.lock().unwrap_or_else(|e| e.into_inner()) = journal;
    }

    pub fn definir_dossier(&self, dossier: Option<PathBuf>) {
        *self.dossier.lock().unwrap_or_else(|e| e.into_inner()) = dossier;
    }

    fn capture(&self) -> Option<Arc<dyn CaptureEcran>> {
        self.capture
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|c| c.disponible())
    }

    fn journal(&self) -> Option<Arc<Journal>> {
        self.journal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn dossier(&self) -> Option<PathBuf> {
        self.dossier
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn journal_disponible(&self) -> bool {
        self.journal()
            .is_some_and(|j| j.est_actif() && !j.lire().trim().is_empty())
    }

    pub fn ouverte(&self) -> bool {
        self.verrou().formulaire.is_some()
    }

    pub fn ouvrir(
        &self,
        f: &Principale,
        cible: Cible,
        environnement: &Environnement,
        capturer: bool,
    ) {
        if self.ouverte() {
            return;
        }
        let brouillon = cible.brouillon().unwrap_or_else(|| {
            self.dossier()
                .map(|d| Brouillon::charger(&d))
                .unwrap_or_default()
        });
        let informations = informations_techniques(&cible, environnement)
            .into_iter()
            .map(|(k, v)| (k, v, true))
            .collect();
        let g = f.global::<DonneesSignalements>();
        g.set_titre(brouillon.title.as_str().into());
        g.set_description(brouillon.description.as_str().into());
        g.set_avec_journal(false);
        {
            let mut e = self.verrou();
            e.generation += 1;
            e.formulaire = Some(Formulaire {
                cible,
                categorie: brouillon.category,
                informations,
                pieces: Vec::new(),
                erreur: String::new(),
                en_cours: false,
                masquee: capturer,
            });
        }
        self.projeter_fenetre(f);
        if capturer {
            self.prendre_capture();
        }
    }

    fn prendre_capture(&self) {
        let Some(capture) = self.capture() else {
            if let Some(f) = self.fenetre.upgrade() {
                self.capture_prise(&f, None);
            }
            return;
        };
        let pont = self.clone();
        let generation = self.verrou().generation;
        slint::Timer::single_shot(ATTENTE_AFFICHAGE, move || {
            let p = pont.clone();
            let _ = std::thread::Builder::new()
                .name("estompe-capture-signalement".into())
                .spawn(move || {
                    let image = capturer_fenetre_d_estompe(capture.as_ref(), TITRE_FENETRE)
                        .ok()
                        .and_then(|i| jpeg_de_la_capture(&i));
                    let q = p.clone();
                    let _ = p.fenetre.upgrade_in_event_loop(move |f| {
                        if q.verrou().generation == generation {
                            q.capture_prise(&f, image);
                        }
                    });
                });
        });
    }

    fn capture_prise(&self, f: &Principale, image: Option<(Vec<u8>, DynamicImage)>) {
        {
            let mut e = self.verrou();
            let Some(fo) = e.formulaire.as_mut() else {
                return;
            };
            fo.masquee = false;
            if let Some((octets, image)) = image {
                if fo.pieces.len() < MAX_CAPTURES {
                    fo.pieces.push(Piece {
                        nom: "capture.jpg".into(),
                        type_mime: "image/jpeg",
                        octets,
                        apercu: apercu(&image),
                        id: None,
                    });
                }
            }
        }
        self.projeter_fenetre(f);
    }

    pub fn ajouter_capture(&self, f: &Principale, capture: &estompe_plateforme::Image) {
        self.capture_prise(f, jpeg_de_la_capture(capture));
    }

    pub fn capturer(&self, f: &Principale) {
        {
            let mut e = self.verrou();
            let Some(fo) = e.formulaire.as_mut() else {
                return;
            };
            if fo.pieces.len() >= MAX_CAPTURES || fo.en_cours {
                return;
            }
            fo.masquee = true;
        }
        self.projeter_fenetre(f);
        self.prendre_capture();
    }

    pub fn ajouter_images(&self, _f: &Principale) {
        let Some(fichiers) = self
            .fichiers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return;
        };
        if !fichiers.disponible() {
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-images-signalement".into())
            .spawn(move || {
                let Ok(chemins) = fichiers.choisir_fichiers(None) else {
                    return;
                };
                let pieces: Vec<Piece> = chemins
                    .iter()
                    .filter_map(|chemin| {
                        let octets = std::fs::read(chemin).ok()?;
                        let type_mime = type_d_image(&octets)?;
                        let image = image::load_from_memory(&octets).ok()?;
                        Some(Piece {
                            nom: chemin
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "image".into()),
                            type_mime,
                            apercu: apercu(&image),
                            octets,
                            id: None,
                        })
                    })
                    .collect();
                let p = pont.clone();
                let _ = pont
                    .fenetre
                    .upgrade_in_event_loop(move |f| p.images_choisies(&f, pieces));
            });
    }

    fn images_choisies(&self, f: &Principale, pieces: Vec<Piece>) {
        {
            let mut e = self.verrou();
            let Some(fo) = e.formulaire.as_mut() else {
                return;
            };
            for piece in pieces {
                if fo.pieces.len() >= MAX_CAPTURES {
                    fo.erreur = format!("{MAX_CAPTURES} images au maximum.");
                    break;
                }
                fo.pieces.push(piece);
            }
        }
        self.projeter_fenetre(f);
    }

    pub fn retirer_capture(&self, f: &Principale, i: usize) {
        if let Some(fo) = self.verrou().formulaire.as_mut() {
            if i < fo.pieces.len() {
                fo.pieces.remove(i);
            }
        }
        self.projeter_fenetre(f);
    }

    pub fn basculer_information(&self, f: &Principale, i: usize) {
        if let Some(info) = self
            .verrou()
            .formulaire
            .as_mut()
            .and_then(|fo| fo.informations.get_mut(i))
        {
            info.2 = !info.2;
        }
        self.projeter_fenetre(f);
    }

    pub fn choisir_categorie(&self, f: &Principale, id: &str) {
        if let Some(fo) = self.verrou().formulaire.as_mut() {
            fo.categorie = id.to_string();
        }
        self.modifie(f);
    }

    fn brouillon(&self, f: &Principale) -> Brouillon {
        let g = f.global::<DonneesSignalements>();
        Brouillon {
            title: g.get_titre().to_string(),
            description: g.get_description().to_string(),
            category: self
                .verrou()
                .formulaire
                .as_ref()
                .map(|fo| fo.categorie.clone())
                .unwrap_or_default(),
        }
    }

    pub fn modifie(&self, f: &Principale) {
        let bug = self
            .verrou()
            .formulaire
            .as_ref()
            .is_some_and(|fo| fo.cible == Cible::Bug);
        if bug {
            if let Some(dossier) = self.dossier() {
                self.brouillon(f).enregistrer(&dossier);
            }
        }
        self.projeter_fenetre(f);
    }

    pub fn fermer(&self, f: &Principale) {
        {
            let mut e = self.verrou();
            e.generation += 1;
            e.formulaire = None;
        }
        self.projeter_fenetre(f);
    }

    pub fn envoyer(&self, f: &Principale) {
        let brouillon = self.brouillon(f);
        if !brouillon.envoyable() {
            return;
        }
        let logs = (f.global::<DonneesSignalements>().get_avec_journal()
            && self.journal_disponible())
        .then(|| {
            self.journal()
                .map(|j| journal_recent(&j.lire(), (self.decalage)()))
                .unwrap_or_default()
        });
        let (pieces, informations, generation) = {
            let mut e = self.verrou();
            let generation = e.generation;
            let Some(fo) = e.formulaire.as_mut() else {
                return;
            };
            if fo.en_cours {
                return;
            }
            fo.en_cours = true;
            fo.erreur.clear();
            (fo.pieces.clone(), fo.informations.clone(), generation)
        };
        self.projeter_fenetre(f);
        let client = self.client.clone();
        let pont = self.clone();
        self.runtime.spawn(async move {
            let mut ids = Vec::new();
            let mut erreur = None;
            for (i, piece) in pieces.iter().enumerate() {
                if let Some(id) = &piece.id {
                    ids.push(id.clone());
                    continue;
                }
                let envoi = Envoi {
                    nom: piece.nom.clone(),
                    type_mime: piece.type_mime.into(),
                    contenu: piece.octets.clone().into(),
                    vocal: None,
                };
                match client.envoyer_fichier(&envoi).await {
                    Ok(a) => {
                        let id = a.id.unwrap_or_default();
                        if let Some(fo) = pont.verrou().formulaire.as_mut() {
                            if let Some(p) = fo.pieces.get_mut(i) {
                                p.id = Some(id.clone());
                            }
                        }
                        ids.push(id);
                    }
                    Err(e) => {
                        erreur = Some(e);
                        break;
                    }
                }
            }
            let resultat = match erreur {
                Some(e) => Err(e),
                None => {
                    client
                        .envoyer_signalement(&requete(&brouillon, &informations, logs, ids))
                        .await
                }
            };
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.verrou().generation != generation {
                    return;
                }
                p.envoi_termine(&f, resultat);
            });
        });
    }

    fn envoi_termine(&self, f: &Principale, resultat: Result<(), ErreurApi>) {
        match resultat {
            Ok(()) => {
                let bug = self
                    .verrou()
                    .formulaire
                    .as_ref()
                    .is_some_and(|fo| fo.cible == Cible::Bug);
                if bug {
                    if let Some(dossier) = self.dossier() {
                        Brouillon::default().enregistrer(&dossier);
                    }
                }
                self.fermer(f);
                self.verrou().info_mes = MESSAGE_ENVOYE.into();
                if self.verrou().mes.is_some() {
                    self.charger_mes(f);
                }
                self.projeter_mes(f);
            }
            Err(e) => {
                if let Some(fo) = self.verrou().formulaire.as_mut() {
                    fo.en_cours = false;
                    fo.erreur = format!(
                        "{} Ton texte est conservé : tu peux réessayer.",
                        message_d_erreur(&e)
                    );
                }
                self.projeter_fenetre(f);
            }
        }
    }

    fn projeter_fenetre(&self, f: &Principale) {
        let journal = self.journal_disponible();
        let g = f.global::<DonneesSignalements>();
        let e = self.verrou();
        let Some(fo) = &e.formulaire else {
            g.set_ouverte(false);
            g.set_masquee(false);
            return;
        };
        g.set_titre_fenetre(fo.cible.titre_de_la_fenetre().into());
        g.set_brouillon(fo.cible == Cible::Bug);
        g.set_categorie(fo.categorie.as_str().into());
        g.set_categories(choix(&CATEGORIES));
        g.set_max_captures(MAX_CAPTURES as i32);
        g.set_capture_disponible(self.capture().is_some());
        g.set_captures(modele(
            fo.pieces
                .iter()
                .map(|p| CaptureSignalement {
                    image: Image::from_rgba8(p.apercu.clone()),
                    nom: p.nom.as_str().into(),
                })
                .collect(),
        ));
        g.set_informations(modele(
            fo.informations
                .iter()
                .map(|(cle, valeur, incluse)| InfoSignalement {
                    cle: cle.as_str().into(),
                    valeur: valeur.as_str().into(),
                    incluse: *incluse,
                })
                .collect(),
        ));
        g.set_informations_incluses(fo.informations.iter().filter(|i| i.2).count() as i32);
        g.set_journal_disponible(journal);
        if !journal {
            g.set_avec_journal(false);
        }
        g.set_erreur(fo.erreur.as_str().into());
        g.set_en_cours(fo.en_cours);
        let brouillon = Brouillon {
            title: g.get_titre().to_string(),
            description: g.get_description().to_string(),
            category: String::new(),
        };
        g.set_envoyable(brouillon.envoyable());
        g.set_masquee(fo.masquee);
        g.set_ouverte(true);
    }

    fn lancer<T: Send + 'static>(
        &self,
        admin: bool,
        tache: impl Future<Output = Result<T, ErreurApi>> + Send + 'static,
        suite: impl FnOnce(&PontSignalements, &Principale, T) + Send + 'static,
    ) {
        let generation = self.verrou().generation_admin;
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = tache.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.verrou().generation_admin != generation {
                    return;
                }
                {
                    let mut e = p.verrou();
                    if admin {
                        e.en_cours_admin = false;
                    }
                }
                match resultat {
                    Ok(valeur) => suite(&p, &f, valeur),
                    Err(erreur) => {
                        let message = message_d_erreur(&erreur);
                        let mut e = p.verrou();
                        if admin {
                            e.erreur_admin = message;
                        } else {
                            e.erreur_mes = message;
                        }
                    }
                }
                p.projeter_mes(&f);
                p.projeter_admin(&f);
            });
        });
    }

    pub fn charger_mes(&self, _f: &Principale) {
        let client = self.client.clone();
        self.lancer(
            false,
            async move { client.mes_signalements().await },
            |p, _, l| {
                p.verrou().mes = Some(l);
            },
        );
    }

    pub fn installer_mes(&self, f: &Principale, liste: Vec<Signalement>) {
        {
            let mut e = self.verrou();
            e.generation_admin += 1;
            e.mes = Some(liste);
            e.erreur_mes.clear();
        }
        self.projeter_mes(f);
    }

    fn projeter_mes(&self, f: &Principale) {
        let decalage = (self.decalage)();
        let g = f.global::<DonneesSignalements>();
        let e = self.verrou();
        g.set_mes_charges(e.mes.is_some());
        g.set_erreur_mes(e.erreur_mes.as_str().into());
        g.set_info_mes(e.info_mes.as_str().into());
        g.set_mes(modele(
            e.mes
                .iter()
                .flatten()
                .map(|s| LigneSignalement {
                    id: s.id.as_str().into(),
                    titre: s.title.as_str().into(),
                    resume: resume_mes_signalements(s, decalage).into(),
                    statut: statut(&s.status).into(),
                    ton: ton(&s.status).into(),
                    ..LigneSignalement::default()
                })
                .collect(),
        ));
    }

    fn filtres(&self, f: &Principale) -> Filtres {
        let g = f.global::<DonneesSignalements>();
        let [s, c, p] = self.verrou().filtres;
        let id = |liste: &[(&'static str, &'static str)], i: usize| {
            i.checked_sub(1)
                .and_then(|i| liste.get(i))
                .map(|(id, _)| (*id).to_string())
                .unwrap_or_default()
        };
        Filtres {
            statut: id(&STATUTS, s),
            categorie: id(&CATEGORIES[1..], c),
            plateforme: id(&PLATEFORMES, p),
            auteur: g.get_filtre_auteur().to_string(),
            recherche: g.get_filtre_recherche().to_string(),
        }
    }

    pub fn charger_admin(&self, f: &Principale) {
        {
            let mut e = self.verrou();
            e.erreur_admin.clear();
            e.info_admin.clear();
        }
        self.recharger_liste(f);
        self.projeter_admin(f);
    }

    fn recharger_liste(&self, f: &Principale) {
        let client = self.client.clone();
        let filtres = self.filtres(f);
        self.lancer(
            true,
            async move { client.signalements_admin(&filtres).await },
            |p, _, l| p.verrou().liste = l,
        );
    }

    pub fn choisir_filtre(&self, f: &Principale, nom: &str, i: usize) {
        {
            let mut e = self.verrou();
            let n = match nom {
                "statut" => 0,
                "categorie" => 1,
                _ => 2,
            };
            e.filtres[n] = i;
        }
        self.recharger_liste(f);
        self.projeter_admin(f);
    }

    pub fn ouvrir_filtre(&self, f: &Principale, nom: &str, ancre: Ancre) {
        let (liste, tous, titre): (&[(&str, &str)], &str, &str) = match nom {
            "statut" => (&STATUTS, "Tous statuts", "Statut"),
            "categorie" => (&CATEGORIES[1..], "Toutes catégories", "Catégorie"),
            _ => (&PLATEFORMES, "Toutes plateformes", "Plateforme"),
        };
        let n = match nom {
            "statut" => 0,
            "categorie" => 1,
            _ => 2,
        };
        let actuel = self.verrou().filtres[n];
        let entrees: Vec<Entree> = std::iter::once(tous)
            .chain(liste.iter().map(|(_, l)| *l))
            .enumerate()
            .map(|(i, l)| Entree::nouvelle(&i.to_string(), l, "filtre").cochee(i == actuel))
            .collect();
        let pont = self.clone();
        let nom = nom.to_string();
        menu::ouvrir(f, titre, ancre, entrees, move |f, id| {
            if let Ok(i) = id.parse() {
                pont.choisir_filtre(f, &nom, i);
            }
            false
        });
    }

    pub fn ouvrir_signalement(&self, f: &Principale, i: usize) {
        let Some(id) = self.verrou().liste.get(i).map(|s| s.id.clone()) else {
            return;
        };
        self.afficher(f, &id);
    }

    fn afficher(&self, _f: &Principale, id: &str) {
        let client = self.client.clone();
        let id = id.to_string();
        self.lancer(
            true,
            async move {
                let s = client.signalement_admin(&id).await?;
                let mut captures = Vec::new();
                for piece in s.attachments.iter().filter(|a| !a.expired) {
                    if let Ok(octets) = client.contenu_de_fichier(&piece.id).await {
                        if let Ok(image) = image::load_from_memory(&octets) {
                            captures.push((piece.id.clone(), pixels(&image.to_rgba8())));
                        }
                    }
                }
                Ok((s, captures))
            },
            |p, f, (s, captures)| {
                f.global::<DonneesSignalements>()
                    .set_notes(s.notes.as_str().into());
                {
                    let mut e = p.verrou();
                    e.erreur_admin.clear();
                    e.info_admin.clear();
                    e.statut = s.status.clone();
                    e.origine = s.duplicate_of.clone().filter(|o| !o.is_empty());
                    e.captures = captures;
                    e.detail = Some(s);
                }
                p.recharger_liste(f);
            },
        );
    }

    pub fn installer_admin(
        &self,
        f: &Principale,
        liste: Vec<Signalement>,
        detail: Option<(Signalement, Vec<RgbaImage>)>,
    ) {
        {
            let mut e = self.verrou();
            e.generation_admin += 1;
            e.en_cours_admin = false;
            e.erreur_admin.clear();
            e.liste = liste;
            match detail {
                Some((s, images)) => {
                    f.global::<DonneesSignalements>()
                        .set_notes(s.notes.as_str().into());
                    e.statut = s.status.clone();
                    e.origine = s.duplicate_of.clone();
                    e.captures = images.iter().map(|i| (String::new(), pixels(i))).collect();
                    e.detail = Some(s);
                }
                None => e.detail = None,
            }
        }
        self.projeter_admin(f);
    }

    pub fn fermer_signalement(&self, f: &Principale) {
        {
            let mut e = self.verrou();
            e.detail = None;
            e.captures.clear();
        }
        self.projeter_admin(f);
    }

    pub fn choisir_statut(&self, f: &Principale, statut: &str) {
        self.verrou().statut = statut.to_string();
        self.projeter_admin(f);
    }

    pub fn ouvrir_origines(&self, f: &Principale, ancre: Ancre) {
        let entrees: Vec<Entree> = {
            let e = self.verrou();
            let actuel = e.detail.as_ref().map(|d| d.id.clone()).unwrap_or_default();
            e.liste
                .iter()
                .filter(|s| s.id != actuel)
                .map(|s| {
                    Entree::nouvelle(&s.id, &format!("{} ({})", s.title, s.author), "drapeau")
                        .cochee(e.origine.as_deref() == Some(s.id.as_str()))
                })
                .collect()
        };
        let pont = self.clone();
        menu::ouvrir(f, "Signalement d'origine", ancre, entrees, move |f, id| {
            pont.verrou().origine = Some(id.to_string());
            pont.projeter_admin(f);
            false
        });
    }

    pub fn voir_original(&self, f: &Principale) {
        let origine = self
            .verrou()
            .detail
            .as_ref()
            .and_then(|d| d.duplicate_of.clone())
            .filter(|o| !o.is_empty());
        if let Some(id) = origine {
            self.afficher(f, &id);
        }
    }

    pub fn enregistrer(&self, f: &Principale) {
        let (id, statut, origine) = {
            let e = self.verrou();
            let Some(d) = &e.detail else {
                return;
            };
            if e.en_cours_admin {
                return;
            }
            (
                d.id.clone(),
                e.statut.clone(),
                e.origine.clone().unwrap_or_default(),
            )
        };
        let notes = f.global::<DonneesSignalements>().get_notes().to_string();
        let corps = match modification(&statut, &origine, &notes) {
            Ok(corps) => corps,
            Err(message) => {
                self.verrou().erreur_admin = message.into();
                self.projeter_admin(f);
                return;
            }
        };
        {
            let mut e = self.verrou();
            e.en_cours_admin = true;
            e.erreur_admin.clear();
            e.info_admin.clear();
        }
        self.projeter_admin(f);
        let client = self.client.clone();
        self.lancer(
            true,
            async move { client.modifier_signalement(&id, &corps).await },
            |p, f, s| {
                {
                    let mut e = p.verrou();
                    e.info_admin = "Signalement mis à jour.".into();
                    if let Some(d) = e.detail.as_mut() {
                        d.status = s.status;
                        d.duplicate_of = s.duplicate_of;
                        d.notes = s.notes;
                    }
                }
                p.recharger_liste(f);
            },
        );
    }

    pub fn supprimer(&self, f: &Principale) {
        let Some((id, titre)) = self
            .verrou()
            .detail
            .as_ref()
            .map(|d| (d.id.clone(), d.title.clone()))
        else {
            return;
        };
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le signalement".into(),
                message: format!("Supprimer définitivement le signalement « {titre} » ?"),
                action: "Supprimer le signalement".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let client = pont.client.clone();
                pont.lancer(
                    true,
                    async move { client.supprimer_signalement(&id).await },
                    |p, f, ()| {
                        p.fermer_signalement(f);
                        p.recharger_liste(f);
                    },
                );
                pont.projeter_admin(f);
            },
        );
    }

    pub fn copier(&self, f: &Principale) {
        let texte = self
            .verrou()
            .detail
            .as_ref()
            .map(|d| formater_signalement(d, (self.decalage)()));
        if let Some(texte) = texte {
            menu::copier_texte(f, &texte);
            self.verrou().info_admin = "Signalement copié.".into();
            self.projeter_admin(f);
        }
    }

    pub fn telecharger_logs(&self, _f: &Principale) {
        let Some((id, logs)) = self
            .verrou()
            .detail
            .as_ref()
            .map(|d| (d.id.clone(), d.logs.clone()))
        else {
            return;
        };
        let Some(fichiers) = self
            .fichiers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return;
        };
        if !fichiers.disponible() {
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-logs-signalement".into())
            .spawn(move || {
                let Ok(Some(dossier)) = fichiers.choisir_dossier(None) else {
                    return;
                };
                let chemin = dossier.join(format!("signalement-{id}.log"));
                let message = match std::fs::write(&chemin, logs) {
                    Ok(()) => (true, format!("Logs enregistrés : {}", chemin.display())),
                    Err(e) => (false, format!("Enregistrement impossible : {e}")),
                };
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    {
                        let mut e = p.verrou();
                        if message.0 {
                            e.info_admin = message.1;
                        } else {
                            e.erreur_admin = message.1;
                        }
                    }
                    p.projeter_admin(&f);
                });
            });
    }

    fn projeter_admin(&self, f: &Principale) {
        let decalage = (self.decalage)();
        let g = f.global::<DonneesSignalements>();
        let e = self.verrou();
        g.set_statuts(choix(&STATUTS));
        g.set_categories(choix(&CATEGORIES[1..]));
        g.set_plateformes(choix(&PLATEFORMES));
        g.set_filtre_statut(e.filtres[0] as i32);
        g.set_filtre_categorie(e.filtres[1] as i32);
        g.set_filtre_plateforme(e.filtres[2] as i32);
        g.set_erreur_admin(e.erreur_admin.as_str().into());
        g.set_info_admin(e.info_admin.as_str().into());
        g.set_en_cours_admin(e.en_cours_admin);
        let actuel = e.detail.as_ref().map(|d| d.id.as_str()).unwrap_or_default();
        g.set_liste(modele(
            e.liste
                .iter()
                .map(|s| LigneSignalement {
                    id: s.id.as_str().into(),
                    titre: s.title.as_str().into(),
                    resume: resume_admin(s, decalage).into(),
                    statut: statut(&s.status).into(),
                    ton: ton(&s.status).into(),
                    plateforme: libelle(&PLATEFORMES, &s.platform).into(),
                    nouveau: !s.seen,
                    actif: s.id == actuel,
                })
                .collect(),
        ));
        let Some(d) = &e.detail else {
            g.set_detail_ouvert(false);
            return;
        };
        g.set_detail_ouvert(true);
        g.set_detail_titre(d.title.as_str().into());
        g.set_detail_meta(
            format!(
                "{} · {} · {} · {}",
                d.author,
                date_heure_complete(&d.created_at, decalage).unwrap_or_default(),
                libelle(&CATEGORIES, &d.category),
                libelle(&PLATEFORMES, &d.platform)
            )
            .into(),
        );
        g.set_detail_description(d.description.as_str().into());
        g.set_detail_captures(modele(
            e.captures
                .iter()
                .map(|(_, p)| Image::from_rgba8(p.clone()))
                .collect(),
        ));
        g.set_detail_tech(modele(
            d.tech
                .iter()
                .map(|(k, v)| CleValeurSignalement {
                    cle: k.as_str().into(),
                    valeur: v.as_str().into(),
                })
                .collect(),
        ));
        g.set_detail_logs(d.logs.as_str().into());
        g.set_statut_choisi(e.statut.as_str().into());
        g.set_origine(
            e.origine
                .as_deref()
                .map(|o| {
                    e.liste
                        .iter()
                        .find(|s| s.id == o)
                        .map_or_else(|| o.to_string(), |s| format!("{} ({})", s.title, s.author))
                })
                .unwrap_or_default()
                .into(),
        );
        g.set_a_original(d.duplicate_of.as_deref().is_some_and(|o| !o.is_empty()));
    }

    pub fn oublier(&self, f: &Principale) {
        {
            let mut e = self.verrou();
            e.generation += 1;
            e.generation_admin += 1;
            *e = EtatSignalements {
                generation: e.generation,
                generation_admin: e.generation_admin,
                ..EtatSignalements::default()
            };
        }
        let g = f.global::<DonneesSignalements>();
        g.set_ouverte(false);
        g.set_masquee(false);
        g.set_detail_ouvert(false);
        g.set_mes(modele(Vec::new()));
        g.set_liste(modele(Vec::new()));
        g.set_titre(SharedString::new());
        g.set_description(SharedString::new());
    }
}

impl Pont {
    pub fn signalements(&self) -> &PontSignalements {
        self.admin().signalements()
    }

    fn environnement_du_signalement(&self) -> (String, String) {
        let route = self.route();
        let vocal = self.vocal().etat();
        self.store.lire(|e| {
            let ecran = match &route {
                Route::Salon { serveur, salon, .. } => {
                    let nom_serveur = e
                        .serveur(serveur)
                        .and_then(|s| s.name.clone())
                        .unwrap_or_else(|| "?".into());
                    let nom_salon = salon
                        .as_deref()
                        .and_then(|s| e.salon(s))
                        .and_then(|(_, s)| s.name.clone())
                        .unwrap_or_else(|| "?".into());
                    ecran_du_salon(&nom_serveur, &nom_salon)
                }
                Route::MessagePrive { .. } => "message privé".into(),
                Route::Accueil => "accueil".into(),
                Route::Invitation { .. } => "invitation".into(),
                Route::Admin { .. } => "administration".into(),
                Route::Films { .. } => "films".into(),
                Route::Musique { .. } => "musique".into(),
                Route::Regie { .. } => "régie".into(),
                Route::Stats { .. } => "statistiques".into(),
                Route::Espace { espace, .. } => format!("espace « {espace} »"),
            };
            let salon = vocal
                .salon
                .as_deref()
                .and_then(|s| e.salon(s))
                .and_then(|(_, s)| s.name.clone());
            let vocal = match (vocal.phase, salon) {
                (Phase::Inactif, _) | (_, None) => "inactif".to_string(),
                (phase, Some(nom)) => vocal_actif(&format!("{phase:?}").to_lowercase(), &nom),
            };
            (ecran, vocal)
        })
    }

    pub fn signaler(&self, f: &Principale, cible: Cible) {
        let (ecran, vocal) = self.environnement_du_signalement();
        let systeme = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let environnement = Environnement {
            version: crate::VERSION,
            systeme: &systeme,
            ecran: &ecran,
            vocal: &vocal,
        };
        self.signalements().ouvrir(f, cible, &environnement, true);
    }
}
