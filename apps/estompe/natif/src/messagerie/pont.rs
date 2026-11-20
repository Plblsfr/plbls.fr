use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::appels::Badge;
use estompe_client::lru::Lru;
use estompe_client::markdown::{analyser, texte_brut};
use estompe_client::mention::Mentionnables;
use estompe_client::store::messages::{
    auteur, ChangementMessages, Conversation, EtatEnvoi, PREFIXE_LOCAL,
};
use estompe_client::store::{Changement, CleConversation, Store};
use estompe_client::types::LinkPreview;
use estompe_client::video::Conversions;
use estompe_plateforme::{Fichiers, Indisponible, OuvertureUrl, PressePapiers};
use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};

use super::actions::DroitsConversation;
use super::annexes::{carte_affichee, chemin_libre, taille_de_vignette, CarteAffichee};
use super::defilement::{Alignement, Defilement};
use super::emoji;
use super::gifs::Gifs;
use super::lignes::{
    donnees_message, libelle_vu_par, lignes_de_conversation, mentionnables, premier_non_lu,
    utilisateur_mentionne, ContexteLignes, DonneesAnnexes, DonneesLecteur, DonneesLigne,
    DonneesMessage, DonneesPiece, GenreLigne, GenrePiece, Votants,
};
use super::medias::{CleImage, Decodee, Format, Medias, Progres, Source, SourceMedias};
use super::rendu::{
    analyser_cible, blocs_affiches, BlocAffiche, Cible, GenreBlocAffiche, GenreSegmentAffiche,
    Palette,
};
use super::saisie::{executer, indice_de, Action, Mode, Saisie};
use super::selecteur::{Cible as CibleSelecteur, Selecteur};
use crate::coquille::avatars::{CleAvatar, Profils};
use crate::coquille::modeles::{synchroniser, Operation};
use crate::coquille::pont::{badge_slint, couleur_slint, vec_de};
use crate::entree::Futur;
use crate::{
    AnnexesMessage, BlocTexte, CarteLien, DonneesMessagerie, DroitsLigne, EpingleMessage,
    EtatConversation, EtatEpingles, EtatTelechargement, GenreBloc, GenreCarte, GenreLigneMessage,
    GenrePieceJointe, GenreSegment, LecteurMessage, LigneMessage, OptionSondage, PieceJointe,
    Principale, ReactionMessage, ReponseMessage, SegmentTexte, SondageMessage, Theme, Visionneuse,
};
use estompe_client::autocompletion::{
    autocompletion, convertir_code, GenreProposition, Proposition,
};

pub type Fuseau = Arc<dyn Fn(i64) -> i32 + Send + Sync>;
pub type Navigateur = Arc<dyn OuvertureUrl + Send + Sync>;
pub type HorlogeMs = Arc<dyn Fn() -> i64 + Send + Sync>;
pub type SystemeFichiers = Arc<dyn Fichiers + Send + Sync>;
pub type Presse = Arc<dyn PressePapiers + Send + Sync>;
pub type DossierParDefaut = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

pub const ZOOM_MAX: f32 = 8.0;
pub const TEXTE_LECTURE_SEULE_ESTOMPE: &str =
    "Conversation en lecture seule : Estompe y publie ses nouveautés.";
pub const TEXTE_SANS_DROIT_D_ECRIRE: &str = "Tu n'as pas la permission d'écrire dans ce salon.";
pub const PAS_DE_PROGRESSION: u8 = 1;

pub const CAPACITE_RENDUS: usize = 512;
pub const DUREE_MISE_EN_EVIDENCE: Duration = Duration::from_millis(1500);
pub const CLE_DEBUT: &str = "debut";
pub const CLE_CHARGEMENT_HAUT: &str = "chargement-haut";
pub const CLE_CHARGEMENT_BAS: &str = "chargement-bas";
pub const LARGEUR_PAR_DEFAUT: f64 = 800.0;
pub const VUE_PAR_DEFAUT: f64 = 600.0;

const LARGEUR_CARACTERE: f64 = 7.4;
const HAUTEUR_LIGNE_TEXTE: f64 = 23.0;
const GOUTTIERE: f64 = 96.0;

thread_local! {
    static ANIMATION: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static REVEIL_SONDAGE: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static MINUTERIE: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static PLANIFIE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PROPRIETAIRE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

mod actions;
mod gifs;
mod sondages;
mod televersements;

pub use actions::TITRE_MENU_MESSAGE;

static IDENTIFIANTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn liberer_modele() {
    PROPRIETAIRE.with(|p| p.set(0));
}

pub fn fuseau_local() -> Fuseau {
    Arc::new(|ms| {
        use chrono::{Local, Offset, TimeZone};
        Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|d| d.offset().fix().local_minus_utc() / 60)
            .unwrap_or(0)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CleRendu {
    id: String,
    contenu: String,
    modifie: bool,
    reveles: BTreeSet<u32>,
    palette: Palette,
    generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendus {
    pub blocs: Vec<BlocAffiche>,
    pub texte_brut: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Telechargement {
    #[default]
    Aucun,
    EnCours,
    Termine,
    Echec,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatPiece {
    pub telechargement: Telechargement,
    pub progression: u8,
    pub chemin: Option<PathBuf>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PieceAffichee {
    donnees: DonneesPiece,
    largeur: u32,
    hauteur: u32,
    chargee: bool,
    echec: bool,
    index_image: i32,
    etat: EtatPiece,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EmojiAffiche {
    nom: String,
    image: Option<CleImage>,
    chargee: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MediasLigne {
    pieces: Vec<PieceAffichee>,
    carte: CarteAffichee,
    carte_chargee: bool,
    emojis: Vec<EmojiAffiche>,
}

impl Default for MediasLigne {
    fn default() -> Self {
        Self {
            pieces: Vec::new(),
            carte: CarteAffichee::Aucune,
            carte_chargee: false,
            emojis: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Emplacement {
    Piece(usize),
    Emoji(String),
}

#[derive(Debug, Clone)]
struct EtatAnimation {
    image: CleImage,
    indice: usize,
    restant: Duration,
}

#[derive(Debug, Clone, PartialEq)]
struct EtatVisionneuse {
    message: String,
    index: usize,
    zoom: f32,
    note: String,
}

#[derive(Debug, Clone, PartialEq)]
struct LigneFenetre {
    donnees: DonneesLigne,
    rendus: Option<Arc<Rendus>>,
    portrait: bool,
    portraits_lecteurs: usize,
    badges: Vec<Badge>,
    medias: MediasLigne,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Proprietes {
    active: bool,
    haut: f32,
    bas: f32,
    defilement: f32,
    etat: Option<EtatConversation>,
    bouton_bas: bool,
    nouveaux: i32,
    saisie: String,
    lecture_seule: bool,
    lecture_seule_texte: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statistiques {
    pub projections: usize,
    pub conversions: usize,
    pub rendus_calcules: usize,
    pub reveils_animation: usize,
    pub images_animees: usize,
}

struct Interne {
    cle: Option<CleConversation>,
    serveur: Option<String>,
    non_lus_ouverture: i64,
    nouveau_depuis: Option<String>,
    positionnee: bool,
    mis_en_evidence: Option<String>,
    supprimes: HashSet<String>,
    reveles: HashMap<String, BTreeSet<u32>>,
    lignes: Vec<DonneesLigne>,
    index: HashMap<String, usize>,
    defilement: Defilement,
    ombre_fenetre: Vec<LigneFenetre>,
    rendus: Lru<CleRendu, Arc<Rendus>>,
    mentionnables: Option<Arc<Mentionnables>>,
    generation: u64,
    palette: Palette,
    en_bas_signale: Option<bool>,
    chargement_haut: bool,
    chargement_bas: bool,
    echec_page: bool,
    debut_atteint: bool,
    fin_atteinte: bool,
    chargee: bool,
    erreur: bool,
    focus: bool,
    nouveaux: usize,
    saisie: String,
    lecture_seule: bool,
    lecture_seule_texte: String,
    compte_estompe: bool,
    avec_portrait: HashSet<String>,
    demandes_avatars: HashSet<CleAvatar>,
    echecs_avatars: HashSet<CleAvatar>,
    demandes_badges: HashSet<String>,
    operations: Vec<(Operation, String)>,
    proprietes: Proprietes,
    statistiques: Statistiques,
    pieces: HashMap<String, EtatPiece>,
    demandes_images: HashSet<CleImage>,
    demandes_apercus: HashSet<String>,
    emojis_demandes: bool,
    animations: HashMap<(String, Emplacement), EtatAnimation>,
    visionneuse: Option<EtatVisionneuse>,
    apercus: bool,
    dossier: Option<PathBuf>,
    reveil_sondage: Option<i64>,
    votants: HashMap<String, Votants>,
    creation_sondage: Option<sondages::CreationEnCours>,
    minuterie_active: bool,
    epingles_visibles: bool,
    etat_epingles: EtatEpingles,
}

impl Interne {
    fn nouveau() -> Self {
        let mut defilement = Defilement::default();
        defilement.redimensionner(LARGEUR_PAR_DEFAUT, VUE_PAR_DEFAUT);
        Self {
            cle: None,
            serveur: None,
            non_lus_ouverture: 0,
            nouveau_depuis: None,
            positionnee: false,
            mis_en_evidence: None,
            supprimes: HashSet::new(),
            reveles: HashMap::new(),
            lignes: Vec::new(),
            index: HashMap::new(),
            defilement,
            ombre_fenetre: Vec::new(),
            rendus: Lru::nouveau(CAPACITE_RENDUS),
            mentionnables: None,
            generation: 0,
            palette: Palette::default(),
            en_bas_signale: None,
            chargement_haut: false,
            chargement_bas: false,
            echec_page: false,
            debut_atteint: false,
            fin_atteinte: false,
            chargee: false,
            erreur: false,
            focus: true,
            nouveaux: 0,
            saisie: String::new(),
            lecture_seule: false,
            lecture_seule_texte: String::new(),
            compte_estompe: false,
            avec_portrait: HashSet::new(),
            demandes_avatars: HashSet::new(),
            echecs_avatars: HashSet::new(),
            demandes_badges: HashSet::new(),
            operations: Vec::new(),
            proprietes: Proprietes::default(),
            statistiques: Statistiques::default(),
            pieces: HashMap::new(),
            demandes_images: HashSet::new(),
            demandes_apercus: HashSet::new(),
            emojis_demandes: false,
            animations: HashMap::new(),
            visionneuse: None,
            apercus: true,
            dossier: None,
            reveil_sondage: None,
            votants: HashMap::new(),
            creation_sondage: None,
            minuterie_active: false,
            epingles_visibles: false,
            etat_epingles: EtatEpingles::Chargement,
        }
    }

    fn reinitialiser(&mut self) {
        let palette = self.palette.clone();
        let focus = self.focus;
        let generation = self.generation + 1;
        let (largeur, vue) = (self.defilement.largeur(), self.defilement.vue());
        let proprietes = std::mem::take(&mut self.proprietes);
        let statistiques = std::mem::take(&mut self.statistiques);
        let rendus = std::mem::replace(&mut self.rendus, Lru::nouveau(0));
        let echecs = std::mem::take(&mut self.echecs_avatars);
        let pieces = std::mem::take(&mut self.pieces);
        let (emojis_demandes, apercus, dossier) =
            (self.emojis_demandes, self.apercus, self.dossier.take());
        let epingles_visibles = self.epingles_visibles;
        *self = Self::nouveau();
        self.epingles_visibles = epingles_visibles;
        self.pieces = pieces;
        self.emojis_demandes = emojis_demandes;
        self.apercus = apercus;
        self.dossier = dossier;
        self.palette = palette;
        self.focus = focus;
        self.generation = generation;
        self.defilement.redimensionner(largeur, vue);
        self.proprietes = proprietes;
        self.statistiques = statistiques;
        self.rendus = rendus;
        self.echecs_avatars = echecs;
    }
}

pub fn estimer_hauteur(ligne: &DonneesLigne, largeur: f64) -> f64 {
    match ligne.genre {
        GenreLigne::Jour => 40.0,
        GenreLigne::Nouveau => 32.0,
        GenreLigne::Message => ligne
            .message
            .as_ref()
            .map_or(40.0, |m| estimer_message(m, largeur)),
    }
}

fn estimer_message(m: &DonneesMessage, largeur: f64) -> f64 {
    let colonne = (largeur - GOUTTIERE).max(120.0);
    let par_ligne = (colonne / LARGEUR_CARACTERE).max(10.0);
    let lignes: f64 = m
        .contenu
        .split('\n')
        .map(|l| (l.chars().count() as f64 / par_ligne).ceil().max(1.0))
        .sum();
    let mut h = if m.suite { 4.0 } else { 36.0 };
    if !m.contenu.is_empty() {
        h += lignes * HAUTEUR_LIGNE_TEXTE;
    }
    if m.reponse.is_some() {
        h += 24.0;
    }
    if let Some(sondage) = &m.annexes.sondage {
        let votants = if sondage.votants_ouverts { 20.0 } else { 0.0 };
        h += 110.0 + (46.0 + votants) * sondage.options.len() as f64;
    }
    if m.annexes.images().next().is_some() {
        h += 196.0;
    }
    if m.annexes
        .pieces
        .iter()
        .any(|p| p.genre != GenrePiece::Image)
    {
        h += 44.0;
    }
    if m.annexes.lien.is_some() {
        h += 90.0;
    }
    if !m.annexes.reactions.is_empty() || !m.annexes.lecteurs.is_empty() {
        h += 34.0;
    }
    h.max(if m.suite { 26.0 } else { 58.0 })
}

pub fn libelle_fichiers(nombre: usize) -> String {
    match nombre {
        0 => String::new(),
        1 => "1 fichier".to_string(),
        n => format!("{n} fichiers"),
    }
}

pub fn texte_de_saisie(noms: &[String]) -> String {
    match noms {
        [] => String::new(),
        [a] => format!("{a} est en train d'écrire…"),
        [a, b] => format!("{a} et {b} écrivent…"),
        _ => "Plusieurs personnes écrivent…".to_string(),
    }
}

fn hexa(couleur: slint::Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        couleur.red(),
        couleur.green(),
        couleur.blue()
    )
}

pub fn palette_du_theme(f: &Principale) -> Palette {
    let theme = f.global::<Theme>();
    Palette {
        mention: hexa(theme.get_rose_texte()),
        masque: hexa(theme.get_encre()),
        discret: hexa(theme.get_gris()),
    }
}

fn pseudo_ligne(cle: &str) -> DonneesLigne {
    DonneesLigne {
        cle: cle.to_string(),
        genre: GenreLigne::Jour,
        libelle: String::new(),
        message: None,
    }
}

fn genre_pseudo(cle: &str) -> Option<GenreLigneMessage> {
    match cle {
        CLE_DEBUT => Some(GenreLigneMessage::Debut),
        CLE_CHARGEMENT_HAUT | CLE_CHARGEMENT_BAS => Some(GenreLigneMessage::Chargement),
        _ => None,
    }
}

fn hauteur_pseudo(cle: &str) -> f64 {
    if cle == CLE_DEBUT {
        130.0
    } else {
        48.0
    }
}

struct Images<'a> {
    medias: &'a Medias,
    emojis: &'a [EmojiAffiche],
    animations: &'a HashMap<(String, Emplacement), EtatAnimation>,
    cle_ligne: &'a str,
}

impl Images<'_> {
    fn decodee(&self, cle: &CleImage, emplacement: Emplacement) -> Option<slint::Image> {
        let image = self.medias.image(cle)?;
        let indice = self
            .animations
            .get(&(self.cle_ligne.to_string(), emplacement))
            .map_or(0, |a| a.indice);
        Some(image_de(&image, indice))
    }

    fn emoji_serveur(&self, nom: &str) -> slint::Image {
        self.emojis
            .iter()
            .find(|e| e.nom == nom)
            .and_then(|e| e.image.as_ref())
            .and_then(|cle| self.decodee(cle, Emplacement::Emoji(nom.to_string())))
            .unwrap_or_default()
    }
}

fn image_de(image: &Decodee, indice: usize) -> slint::Image {
    match image {
        Decodee::Fixe(i) => Image::from_rgba8(i.clone()),
        Decodee::Animee(a) => Image::from_rgba8(a.images[indice % a.images.len()].0.clone()),
    }
}

fn bloc_slint(bloc: &BlocAffiche, images: Option<&Images>) -> BlocTexte {
    let segments: Vec<SegmentTexte> = bloc
        .segments
        .iter()
        .map(|s| {
            let (genre, image) = match s.genre {
                GenreSegmentAffiche::Texte => (GenreSegment::Texte, Image::default()),
                GenreSegmentAffiche::Code => (GenreSegment::Code, Image::default()),
                GenreSegmentAffiche::Masque => (GenreSegment::Masque, Image::default()),
                GenreSegmentAffiche::Emoji => (
                    GenreSegment::Emoji,
                    emoji::image(&s.texte).unwrap_or_default(),
                ),
                GenreSegmentAffiche::EmojiServeur => (
                    GenreSegment::Emoji,
                    images
                        .map(|i| i.emoji_serveur(&s.cible))
                        .unwrap_or_default(),
                ),
            };
            SegmentTexte {
                genre,
                texte: if s.balisage.is_empty() {
                    slint::StyledText::default()
                } else {
                    slint::StyledText::from_markdown(&s.balisage)
                        .unwrap_or_else(|_| slint::StyledText::from_plain_text(&s.texte))
                },
                brut: s.texte.as_str().into(),
                image,
                espace: s.espace,
                joint: s.joint,
                cible: s.cible.as_str().into(),
            }
        })
        .collect();
    BlocTexte {
        genre: match bloc.genre {
            GenreBlocAffiche::Paragraphe => GenreBloc::Paragraphe,
            GenreBlocAffiche::Titre => GenreBloc::Titre,
            GenreBlocAffiche::Code => GenreBloc::Code,
        },
        texte: slint::StyledText::from_markdown(&bloc.balisage)
            .unwrap_or_else(|_| slint::StyledText::from_plain_text(&bloc.balisage)),
        code: bloc.code.as_str().into(),
        niveau: i32::from(bloc.niveau),
        citation: i32::from(bloc.citation),
        retrait: i32::from(bloc.retrait),
        puce: bloc.puce.as_str().into(),
        en_segments: bloc.en_segments(),
        segments: ModelRc::new(VecModel::from(segments)),
        grand: bloc.grand,
        espace_avant: bloc.espace_avant,
    }
}

fn piece_slint(p: &PieceAffichee, images: &Images, i: usize) -> PieceJointe {
    let image = if p.chargee {
        images
            .decodee(&cle_vignette(&p.donnees.id), Emplacement::Piece(i))
            .unwrap_or_default()
    } else {
        Image::default()
    };
    PieceJointe {
        id: p.donnees.id.as_str().into(),
        genre: match p.donnees.genre {
            GenrePiece::Image => GenrePieceJointe::Image,
            GenrePiece::Fichier => GenrePieceJointe::Fichier,
            GenrePiece::Media => GenrePieceJointe::Media,
            GenrePiece::Expiree => GenrePieceJointe::Expiree,
        },
        nom: p.donnees.nom.as_str().into(),
        taille: p.donnees.taille.as_str().into(),
        largeur: p.largeur as f32,
        hauteur: p.hauteur as f32,
        image,
        chargee: p.chargee,
        echec: p.echec,
        anime: p.donnees.anime,
        vocal: p.donnees.vocal,
        index_image: p.index_image,
        telechargement: match p.etat.telechargement {
            Telechargement::Aucun => EtatTelechargement::Aucun,
            Telechargement::EnCours => EtatTelechargement::EnCours,
            Telechargement::Termine => EtatTelechargement::Termine,
            Telechargement::Echec => EtatTelechargement::Echec,
        },
        progression: f32::from(p.etat.progression) / 100.0,
        note: p.etat.note.as_str().into(),
    }
}

fn carte_slint(m: &MediasLigne, medias: &Medias) -> CarteLien {
    let image = if m.carte_chargee {
        m.carte
            .image()
            .and_then(|cle| medias.image(&cle))
            .map(|i| image_de(&i, 0))
    } else {
        None
    };
    let avec_image = image.is_some();
    let image = image.unwrap_or_default();
    match &m.carte {
        CarteAffichee::Aucune => CarteLien::default(),
        CarteAffichee::Lien {
            url,
            titre,
            description,
            site,
            ..
        } => CarteLien {
            genre: GenreCarte::Lien,
            url: url.as_str().into(),
            titre: titre.as_str().into(),
            description: description.as_str().into(),
            site: site.as_str().into(),
            image,
            avec_image,
        },
        CarteAffichee::Youtube {
            url, titre, site, ..
        } => CarteLien {
            genre: GenreCarte::Youtube,
            url: url.as_str().into(),
            titre: titre.as_str().into(),
            description: SharedString::default(),
            site: site.as_str().into(),
            image,
            avec_image,
        },
        CarteAffichee::Sillon {
            titre,
            site,
            artiste,
        } => CarteLien {
            genre: GenreCarte::Sillon,
            url: SharedString::default(),
            titre: titre.as_str().into(),
            description: artiste.as_str().into(),
            site: site.as_str().into(),
            image,
            avec_image: false,
        },
    }
}

pub const LECTEURS_EN_APERCU: usize = 5;

fn lecteur_slint(l: &DonneesLecteur, profils: &Profils) -> LecteurMessage {
    LecteurMessage {
        utilisateur: l.utilisateur.as_str().into(),
        pseudo: l.pseudo.as_str().into(),
        initiales: l.initiales.as_str().into(),
        teinte: l.teinte,
        portrait: l
            .avatar
            .as_ref()
            .and_then(|c| profils.avatar(c))
            .map(Image::from_rgba8)
            .unwrap_or_default(),
        heure: l.heure.as_str().into(),
    }
}

fn annexes_slint(
    a: &DonneesAnnexes,
    m: &MediasLigne,
    images: &Images,
    profils: &Profils,
) -> AnnexesMessage {
    let lecteurs: Vec<LecteurMessage> = a
        .lecteurs
        .iter()
        .map(|l| lecteur_slint(l, profils))
        .collect();
    let apercu: Vec<LecteurMessage> = lecteurs.iter().take(LECTEURS_EN_APERCU).cloned().collect();
    let reactions: Vec<ReactionMessage> = a
        .reactions
        .iter()
        .map(|r| ReactionMessage {
            emoji: r.emoji.as_str().into(),
            image: emoji::image(&r.emoji).unwrap_or_default(),
            nombre: r.nombre.clamp(0, i64::from(i32::MAX)) as i32,
            moi: r.moi,
            titre: r.titre.as_str().into(),
            infobulle: infobulle_de_reaction(&r.titre, &r.emoji).into(),
        })
        .collect();
    let pieces: Vec<PieceJointe> = m
        .pieces
        .iter()
        .enumerate()
        .map(|(i, p)| piece_slint(p, images, i))
        .collect();
    let sondage = a
        .sondage
        .as_ref()
        .map(|s| SondageMessage {
            present: true,
            auteur: s.auteur.as_str().into(),
            auteur_bot: s.auteur_bot,
            clos: s.clos,
            options: ModelRc::new(VecModel::from(
                s.options
                    .iter()
                    .map(|o| OptionSondage {
                        id: o.id.as_str().into(),
                        libelle: o.libelle.as_str().into(),
                        pourcentage: o.pourcentage.clamp(0, 100) as i32,
                        votes: o.votes.clamp(0, i64::from(i32::MAX)) as i32,
                        moi: o.moi,
                        gagnant: o.gagnant,
                        votants: o.votants.as_deref().unwrap_or_default().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
            pied: s.pied.as_str().into(),
            total: s.total.clamp(0, i64::from(i32::MAX)) as i32,
            votable: s.votable,
            gerer: s.gerer,
            votants_ouverts: s.votants_ouverts,
        })
        .unwrap_or_default();
    AnnexesMessage {
        images: a.images().count().min(i32::MAX as usize) as i32,
        pieces_jointes: ModelRc::new(VecModel::from(pieces)),
        sondage,
        reactions: ModelRc::new(VecModel::from(reactions)),
        lecteurs_en_plus: lecteurs
            .len()
            .saturating_sub(LECTEURS_EN_APERCU)
            .min(i32::MAX as usize) as i32,
        vu_par: libelle_vu_par(&a.lecteurs).into(),
        lecteurs: ModelRc::new(VecModel::from(lecteurs)),
        apercu_lecteurs: ModelRc::new(VecModel::from(apercu)),
        carte: carte_slint(m, images.medias),
        resume: a.resume().into(),
    }
}

pub fn libelle_d_envoi(etat: Option<&EtatEnvoi>) -> String {
    match etat {
        None => String::new(),
        Some(EtatEnvoi::EnCours) => "Envoi…".to_string(),
        Some(EtatEnvoi::EnAttente) => "En attente du réseau…".to_string(),
        Some(EtatEnvoi::Echec(raison)) => format!("Non envoyé · {}", raison.libelle()),
    }
}

pub fn infobulle_de_reaction(titre: &str, emoji: &str) -> String {
    titre
        .strip_suffix(emoji)
        .and_then(|t| t.strip_suffix(" avec "))
        .unwrap_or(titre)
        .to_string()
}

fn cle_vignette(id: &str) -> CleImage {
    CleImage {
        source: Source::Fichier(id.to_string()),
        format: Format::Vignette,
    }
}

fn ligne_slint(
    l: &LigneFenetre,
    profils: &Profils,
    medias: &Medias,
    animations: &HashMap<(String, Emplacement), EtatAnimation>,
) -> LigneMessage {
    let d = &l.donnees;
    if let Some(genre) = genre_pseudo(&d.cle) {
        return LigneMessage {
            cle: d.cle.as_str().into(),
            genre,
            ..LigneMessage::default()
        };
    }
    let Some(m) = &d.message else {
        return LigneMessage {
            cle: d.cle.as_str().into(),
            genre: match d.genre {
                GenreLigne::Nouveau => GenreLigneMessage::Nouveau,
                _ => GenreLigneMessage::Jour,
            },
            libelle: d.libelle.as_str().into(),
            ..LigneMessage::default()
        };
    };
    let images = Images {
        medias,
        emojis: &l.medias.emojis,
        animations,
        cle_ligne: &d.cle,
    };
    let couleur = couleur_slint(m.couleur.as_deref());
    let portrait = if l.portrait {
        m.avatar
            .as_ref()
            .and_then(|c| profils.avatar(c))
            .map(Image::from_rgba8)
            .unwrap_or_default()
    } else {
        Image::default()
    };
    let blocs: Vec<BlocTexte> = l
        .rendus
        .as_ref()
        .map(|r| {
            r.blocs
                .iter()
                .map(|b| bloc_slint(b, Some(&images)))
                .collect()
        })
        .unwrap_or_default();
    LigneMessage {
        cle: d.cle.as_str().into(),
        genre: GenreLigneMessage::Message,
        libelle: SharedString::default(),
        id: m.id.as_str().into(),
        suite: m.suite,
        utilisateur: m.utilisateur.as_str().into(),
        auteur: m.auteur.as_str().into(),
        initiales: m.initiales.as_str().into(),
        teinte: m.teinte,
        portrait,
        a_couleur: couleur.is_some(),
        couleur: couleur.unwrap_or_default(),
        bot: m.bot,
        badge_1: l.badges.first().map(badge_slint).unwrap_or_default(),
        badge_2: l.badges.get(1).map(badge_slint).unwrap_or_default(),
        nombre_badges: l.badges.len().min(i32::MAX as usize) as i32,
        heure: m.heure.as_str().into(),
        date_complete: m.date_complete.as_str().into(),
        systeme: m.systeme,
        epingle: m.epingle,
        modifie: m.modifie,
        date_modification: m.date_modification.as_str().into(),
        mention: m.mention,
        mis_en_evidence: m.mis_en_evidence,
        reponse: m
            .reponse
            .as_ref()
            .map(|r| ReponseMessage {
                presente: true,
                id: r.id.as_str().into(),
                auteur: r.auteur.as_str().into(),
                texte: r.texte.as_str().into(),
                supprimee: r.supprimee,
            })
            .unwrap_or_default(),
        blocs: ModelRc::new(VecModel::from(blocs)),
        texte_brut: l
            .rendus
            .as_ref()
            .map(|r| r.texte_brut.as_str())
            .unwrap_or_default()
            .into(),
        annexes: annexes_slint(&m.annexes, &l.medias, &images, profils),
        etat_envoi: libelle_d_envoi(m.envoi.as_ref()).into(),
        envoi_echec: matches!(m.envoi, Some(EtatEnvoi::Echec(_))),
        droits: DroitsLigne {
            repondre: m.droits.repondre,
            modifier: m.droits.modifier,
            epingler: m.droits.epingler,
            supprimer: m.droits.supprimer,
            reagir: m.droits.reagir,
            menu: m.droits.menu,
        },
    }
}

fn cles_des_operations(
    anciennes: Vec<String>,
    nouvelles: &[LigneFenetre],
    operations: &[Operation],
) -> Vec<(Operation, String)> {
    let mut cles = anciennes;
    operations
        .iter()
        .map(|operation| {
            let cle = match *operation {
                Operation::Retirer(i) => cles.remove(i),
                Operation::Inserer(i) => {
                    cles.insert(i, nouvelles[i].donnees.cle.clone());
                    nouvelles[i].donnees.cle.clone()
                }
                Operation::Remplacer(i) => nouvelles[i].donnees.cle.clone(),
            };
            (*operation, cle)
        })
        .collect()
}

#[derive(Clone)]
pub struct PontMessagerie {
    identifiant: u64,
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    profils: Arc<Profils>,
    fuseau: Arc<Mutex<Fuseau>>,
    navigateur: Arc<Mutex<Navigateur>>,
    medias: Arc<Mutex<Arc<Medias>>>,
    horloge: Arc<Mutex<HorlogeMs>>,
    fichiers: Arc<Mutex<SystemeFichiers>>,
    presse_papiers: Arc<Mutex<Presse>>,
    conversions: Arc<Mutex<Arc<Conversions>>>,
    dossier_par_defaut: Arc<Mutex<DossierParDefaut>>,
    saisie: Saisie,
    selecteur: Selecteur,
    gifs: Gifs,
}

struct SourceIndisponible;

impl SourceMedias for SourceIndisponible {
    fn octets(&self, _: &Source) -> Futur<Result<Vec<u8>, estompe_client::http::ErreurApi>> {
        Box::pin(async { Err(estompe_client::http::ErreurApi::DelaiDepasse) })
    }
    fn emojis(
        &self,
    ) -> Futur<
        Result<Vec<estompe_client::appels::EmojiPersonnalise>, estompe_client::http::ErreurApi>,
    > {
        Box::pin(async { Err(estompe_client::http::ErreurApi::DelaiDepasse) })
    }
    fn apercu(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> Futur<Result<Option<LinkPreview>, estompe_client::http::ErreurApi>> {
        Box::pin(async { Err(estompe_client::http::ErreurApi::DelaiDepasse) })
    }
    fn telecharger(
        &self,
        _: &str,
        _: PathBuf,
        _: Progres,
    ) -> Futur<Result<u64, estompe_client::http::ErreurApi>> {
        Box::pin(async { Err(estompe_client::http::ErreurApi::DelaiDepasse) })
    }
}

fn verrou_de<T: Clone>(m: &Mutex<T>) -> T {
    m.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn maintenant_ms() -> HorlogeMs {
    Arc::new(|| chrono::Utc::now().timestamp_millis())
}

impl PontMessagerie {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        profils: Arc<Profils>,
    ) -> Self {
        Self {
            identifiant: IDENTIFIANTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            interne: Arc::new(Mutex::new(Interne::nouveau())),
            store,
            fenetre,
            runtime,
            profils,
            fuseau: Arc::new(Mutex::new(fuseau_local())),
            navigateur: Arc::new(Mutex::new(Arc::new(Indisponible))),
            medias: Arc::new(Mutex::new(Medias::nouveaux(
                Arc::new(SourceIndisponible),
                None,
            ))),
            horloge: Arc::new(Mutex::new(maintenant_ms())),
            fichiers: Arc::new(Mutex::new(Arc::new(Indisponible))),
            presse_papiers: Arc::new(Mutex::new(Arc::new(Indisponible))),
            conversions: Arc::new(Mutex::new(Arc::new(Conversions::aucune()))),
            dossier_par_defaut: Arc::new(Mutex::new(Arc::new(
                estompe_plateforme::dossier_telechargements,
            ))),
            saisie: Saisie::default(),
            selecteur: Selecteur::default(),
            gifs: Gifs::default(),
        }
    }

    fn raison_lecture_seule(&self) -> Option<&'static str> {
        let (cle, estompe) = {
            let i = self.verrou();
            (i.cle.clone()?, i.compte_estompe)
        };
        if estompe {
            return Some(TEXTE_LECTURE_SEULE_ESTOMPE);
        }
        (!self
            .store
            .lire(|etat| etat.peut_dans(Some(&cle), "send_messages")))
        .then_some(TEXTE_SANS_DROIT_D_ECRIRE)
    }

    fn droits_conversation(&self) -> DroitsConversation {
        let Some(cle) = self.conversation() else {
            return DroitsConversation::default();
        };
        let ecrire = self.raison_lecture_seule().is_none();
        self.store.lire(|etat| DroitsConversation {
            ecrire,
            gerer: etat.peut_dans(Some(&cle), "manage_messages"),
            reagir: etat.peut_dans(Some(&cle), "add_reactions"),
        })
    }

    pub fn saisie(&self) -> &Saisie {
        &self.saisie
    }

    pub fn conversations_en_brouillon(&self) -> HashSet<String> {
        self.saisie.conversations_en_brouillon()
    }

    fn indice_de_saisie(&self, cle: Option<&CleConversation>) -> String {
        let Some(cle) = cle else {
            return String::new();
        };
        self.store.lire(|etat| match cle {
            CleConversation::Salon(id) => {
                let nom = etat
                    .details
                    .values()
                    .flat_map(|d| d.channels.iter().flatten())
                    .find(|s| s.id.as_deref() == Some(id.as_str()))
                    .and_then(|s| s.name.clone())
                    .unwrap_or_default();
                indice_de(&nom, false)
            }
            CleConversation::Prive(id) => {
                let nom = etat
                    .messages_prives
                    .iter()
                    .find(|d| d.id.as_deref() == Some(id.as_str()))
                    .and_then(|d| d.other.as_ref())
                    .and_then(|o| o.pseudo.clone())
                    .unwrap_or_default();
                indice_de(&nom, true)
            }
        })
    }

    pub fn selecteur(&self) -> &Selecteur {
        &self.selecteur
    }

    fn image_de_proposition(&self, p: &Proposition) -> Image {
        match p.genre {
            GenreProposition::Emoji => emoji::image(&p.emoji).unwrap_or_default(),
            GenreProposition::EmojiServeur => self
                .medias()
                .image(&CleImage {
                    source: Source::Emoji(p.emoji.clone()),
                    format: Format::Emoji,
                })
                .map(|d| Image::from_rgba8(d.premiere().clone()))
                .unwrap_or_default(),
            _ => Image::default(),
        }
    }

    fn proposer(&self, f: &Principale) {
        let auto = match self.saisie.cle() {
            Some(cle) => {
                let emojis = self.medias().emojis().unwrap_or_default();
                let texte = self.saisie.texte();
                let curseur = f.global::<crate::DonneesSaisie>().get_curseur().max(0) as usize;
                self.store
                    .lire(|etat| autocompletion(etat, &cle, &emojis, &texte, curseur))
            }
            None => None,
        };
        self.saisie
            .proposer(f, auto, |p| self.image_de_proposition(p));
    }

    pub fn saisie_modifiee(&self, f: &Principale, texte: &str) -> bool {
        let curseur = f.global::<crate::DonneesSaisie>().get_curseur().max(0) as usize;
        let emojis = self.medias().emojis().unwrap_or_default();
        let (texte, curseur) = match convertir_code(&emojis, texte, curseur) {
            Some(r) => {
                self.saisie.placer(f, &r.texte, r.curseur);
                (r.texte, r.curseur)
            }
            None => (texte.to_string(), curseur),
        };
        self.saisie.definir_curseur(curseur);
        let change = self.saisie.modifie(f, &texte, &self.runtime);
        self.proposer(f);
        if matches!(self.saisie.mode(), Mode::Modification { .. }) {
            return change;
        }
        if let Some(cle) = self.saisie.cle() {
            let store = self.store.clone();
            self.runtime
                .spawn(async move { store.signaler_saisie(&cle, &texte).await });
        }
        change
    }

    pub fn curseur_deplace(&self, f: &Principale, texte: &str, curseur: i32) {
        if self.saisie.texte() != texte {
            return;
        }
        self.saisie.definir_curseur(curseur.max(0) as usize);
        self.proposer(f);
    }

    pub fn naviguer_propositions(&self, f: &Principale, pas: i32) {
        self.saisie.naviguer(f, pas);
    }

    pub fn fermer_propositions(&self, f: &Principale) {
        self.saisie.proposer(f, None, |_| Image::default());
    }

    fn remplacer_saisie(&self, f: &Principale, remplacement: Option<(String, usize)>) -> bool {
        let Some((texte, curseur)) = remplacement else {
            return false;
        };
        self.saisie.placer(f, &texte, curseur);
        let change = self.saisie.modifie(f, &texte, &self.runtime);
        self.fermer_propositions(f);
        change
    }

    pub fn choisir_proposition(&self, f: &Principale, index: i32) -> bool {
        let choix = usize::try_from(index)
            .ok()
            .and_then(|i| self.saisie.choisir(i));
        self.remplacer_saisie(f, choix)
    }

    pub fn ouvrir_selecteur(&self, f: &Principale, cible: CibleSelecteur) {
        if self.selecteur.ouvert() {
            self.fermer_selecteur(f);
            return;
        }
        if self.gifs.ouvert() {
            self.gifs.fermer(f);
            f.global::<crate::DonneesSaisie>().set_gifs_ouvert(false);
        }
        let medias = self.medias();
        let emojis = medias.emojis().unwrap_or_default();
        if medias.emojis().is_none() {
            self.charger_emojis(true);
        }
        self.fermer_propositions(f);
        self.selecteur.ouvrir(f, cible, emojis);
        f.global::<crate::DonneesSaisie>()
            .set_selecteur_ouvert(true);
        self.charger_images_du_selecteur(f);
    }

    fn charger_images_du_selecteur(&self, f: &Principale) {
        let medias = self.medias();
        let emojis = medias.emojis().unwrap_or_default();
        self.selecteur.emojis_serveur(f, emojis.clone());
        for e in emojis {
            if self.selecteur.a_l_image(&e.id) {
                continue;
            }
            let cle = CleImage {
                source: Source::Emoji(e.id.clone()),
                format: Format::Emoji,
            };
            if let Some(d) = medias.image(&cle) {
                self.selecteur
                    .installer_image(f, &e.id, d.premiere().clone());
                continue;
            }
            let medias = medias.clone();
            let selecteur = self.selecteur.clone();
            let faible = self.fenetre.clone();
            self.runtime.spawn(async move {
                if let Some(d) = medias.charger_image(cle).await {
                    let image = d.premiere().clone();
                    let _ = faible.upgrade_in_event_loop(move |f| {
                        selecteur.installer_image(&f, &e.id, image)
                    });
                }
            });
        }
    }

    pub fn fermer_selecteur(&self, f: &Principale) {
        let saisie = self.selecteur.cible() == CibleSelecteur::Saisie;
        self.selecteur.fermer(f);
        f.global::<crate::DonneesSaisie>()
            .set_selecteur_ouvert(false);
        if saisie {
            self.saisie.demander_focus(f);
        }
    }

    pub fn choisir_emoji(&self, f: &Principale, index: Option<usize>, garder: bool) -> bool {
        let Some(choix) = self.selecteur.choisir(f, index, garder) else {
            return false;
        };
        if !garder {
            f.global::<crate::DonneesSaisie>()
                .set_selecteur_ouvert(false);
        }
        match choix.cible {
            CibleSelecteur::Saisie => self.inserer_dans_la_saisie(f, &choix.insertion, !garder),
            CibleSelecteur::Reaction(message) => {
                self.reagir(&message, &choix.insertion);
                false
            }
        }
    }

    pub fn inserer_dans_la_saisie(&self, f: &Principale, texte: &str, focus: bool) -> bool {
        let g = f.global::<crate::DonneesSaisie>();
        self.saisie.definir_curseur(g.get_curseur().max(0) as usize);
        let insere = self.saisie.inserer(texte, g.get_ancre().max(0) as usize);
        let change = self.remplacer_saisie(f, insere);
        if focus {
            self.saisie.demander_focus(f);
        }
        change
    }

    pub fn ecriture_permise(&self) -> bool {
        !self.verrou().lecture_seule
    }

    pub fn envoyer_saisie(&self, f: &Principale, texte: &str) -> bool {
        if self.verrou().lecture_seule {
            return false;
        }
        let Some(action) = self.saisie.envoyer(f, texte) else {
            return false;
        };
        let envoi = matches!(action, Action::Envoyer(..));
        if envoi {
            self.store.oublier_saisie();
            {
                let mut i = self.verrou();
                i.nouveau_depuis = None;
                i.defilement.aller_en_bas();
            }
        }
        self.lancer(action);
        envoi
    }

    fn lancer(&self, action: Action) {
        let futur = executer(&self.store, action);
        let faible = self.fenetre.clone();
        let saisie = self.saisie.clone();
        self.runtime.spawn(async move {
            if let Err(erreur) = futur.await {
                let _ = faible.upgrade_in_event_loop(move |f| saisie.signaler_erreur(&f, &erreur));
            }
        });
    }

    pub fn annuler_bandeau(&self, f: &Principale) {
        self.saisie.annuler(f);
    }

    pub fn fleche_haut(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            return;
        };
        if let Some(message) = self.store.dernier_de_moi(&cle) {
            self.saisie.modifier(f, &message);
            self.vignettes_d_edition(f);
        }
    }

    pub fn repondre_a(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        if let Some(message) = self
            .store
            .conversation(&cle)
            .and_then(|c| c.message(id).cloned())
        {
            self.saisie.repondre(f, &message);
        }
    }

    pub fn modifier_message(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        if let Some(message) = self
            .store
            .conversation(&cle)
            .and_then(|c| c.message(id).cloned())
        {
            self.saisie.modifier(f, &message);
            self.vignettes_d_edition(f);
        }
    }

    pub fn supprimer_message(&self, id: &str) {
        if let Some(cle) = self.conversation() {
            self.lancer(Action::Supprimer {
                cle,
                id: id.to_string(),
            });
        }
    }

    pub fn reessayer_envoi(&self, id: &str) {
        let Some(n) = id.strip_prefix(PREFIXE_LOCAL).and_then(|n| n.parse().ok()) else {
            return;
        };
        let store = self.store.clone();
        self.runtime.spawn(async move { store.reessayer(n).await });
    }

    pub fn abandonner_envoi(&self, id: &str) {
        if let Some(n) = id.strip_prefix(PREFIXE_LOCAL).and_then(|n| n.parse().ok()) {
            self.store.abandonner(n);
        }
    }

    pub fn definir_medias(&self, medias: Arc<Medias>) {
        *self.medias.lock().unwrap_or_else(|e| e.into_inner()) = medias;
    }

    pub fn medias(&self) -> Arc<Medias> {
        verrou_de(&self.medias)
    }

    pub fn definir_horloge(&self, horloge: HorlogeMs) {
        *self.horloge.lock().unwrap_or_else(|e| e.into_inner()) = horloge;
    }

    pub fn definir_fichiers(&self, fichiers: SystemeFichiers) {
        *self.fichiers.lock().unwrap_or_else(|e| e.into_inner()) = fichiers;
    }

    pub fn definir_presse_papiers(&self, presse: Presse) {
        *self
            .presse_papiers
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = presse;
    }

    pub fn definir_conversions(&self, conversions: Arc<Conversions>) {
        *self.conversions.lock().unwrap_or_else(|e| e.into_inner()) = conversions;
    }

    pub fn definir_dossier_par_defaut(&self, dossier: DossierParDefaut) {
        *self
            .dossier_par_defaut
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = dossier;
    }

    pub fn definir_apercus(&self, apercus: bool) {
        self.verrou().apercus = apercus;
    }

    fn maintenant(&self) -> i64 {
        verrou_de(&self.horloge)()
    }

    pub fn etat_piece(&self, id: &str) -> EtatPiece {
        self.verrou().pieces.get(id).cloned().unwrap_or_default()
    }

    pub fn animations_actives(&self) -> usize {
        self.verrou().animations.len()
    }

    pub fn minuterie_d_animation(&self) -> bool {
        self.verrou().minuterie_active
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn definir_fuseau(&self, fuseau: Fuseau) {
        *self.fuseau.lock().unwrap_or_else(|e| e.into_inner()) = fuseau;
    }

    pub fn definir_navigateur(&self, navigateur: Navigateur) {
        *self.navigateur.lock().unwrap_or_else(|e| e.into_inner()) = navigateur;
    }

    pub fn conversation(&self) -> Option<CleConversation> {
        self.verrou().cle.clone()
    }

    pub fn operations(&self) -> Vec<(Operation, String)> {
        self.verrou().operations.clone()
    }

    pub fn statistiques(&self) -> Statistiques {
        self.verrou().statistiques.clone()
    }

    pub fn nombre_de_lignes(&self) -> usize {
        self.verrou().lignes.len()
    }

    pub fn rendus_en_cache(&self) -> usize {
        self.verrou().rendus.len()
    }

    pub fn position(&self) -> f64 {
        self.verrou().defilement.position()
    }

    pub fn colle(&self) -> bool {
        self.verrou().defilement.colle()
    }

    pub fn chargement_en_cours(&self) -> bool {
        let i = self.verrou();
        i.chargement_haut || i.chargement_bas
    }

    pub fn fin_atteinte(&self) -> bool {
        self.verrou().fin_atteinte
    }

    pub fn en_bas(&self) -> bool {
        self.verrou().defilement.en_bas()
    }

    pub fn mesures(&self) -> usize {
        self.verrou().defilement.mesures()
    }

    pub fn nouveau_depuis(&self) -> Option<String> {
        self.verrou().nouveau_depuis.clone()
    }

    pub fn cles(&self) -> Vec<String> {
        self.verrou().lignes.iter().map(|l| l.cle.clone()).collect()
    }

    pub fn ouvrir(
        &self,
        f: &Principale,
        cle: Option<CleConversation>,
        serveur: Option<String>,
        non_lus: i64,
    ) {
        {
            let mut i = self.verrou();
            if i.cle == cle && PROPRIETAIRE.with(|p| p.get()) == self.identifiant {
                return;
            }
            i.reinitialiser();
            i.palette = palette_du_theme(f);
            i.cle = cle.clone();
            i.serveur = serveur;
            i.non_lus_ouverture = non_lus;
            i.compte_estompe = match &cle {
                Some(CleConversation::Prive(id)) => self.store.lire(|etat| {
                    etat.messages_prives.iter().any(|d| {
                        d.id.as_deref() == Some(id.as_str())
                            && d.other
                                .as_ref()
                                .and_then(|o| o.id.as_deref())
                                .is_some_and(estompe_client::store::messages::est_compte_estompe)
                    })
                }),
                _ => false,
            };
        }
        let g = f.global::<DonneesMessagerie>();
        if g.get_reactions_rapides().row_count() == 0 {
            self.installer_reactions_rapides(f);
        }
        vec_de(&g.get_lignes()).set_vec(Vec::new());
        g.set_defilement(0.0);
        g.set_visionneuse(Visionneuse::default());
        PROPRIETAIRE.with(|p| p.set(self.identifiant));
        ANIMATION.with(|a| *a.borrow_mut() = None);
        let indice = self.indice_de_saisie(cle.as_ref());
        let ancienne = self.saisie.cle();
        self.saisie.ouvrir(f, cle.clone(), indice);
        if ancienne != cle {
            self.selecteur.fermer(f);
            f.global::<crate::DonneesSaisie>()
                .set_selecteur_ouvert(false);
            self.gifs.fermer(f);
            f.global::<crate::DonneesSaisie>().set_gifs_ouvert(false);
        }
        self.medias().noter_fil_interface();
        self.charger_emojis(false);
        self.reprojeter(f);
        if self.verrou().epingles_visibles {
            self.charger_epingles(f);
        } else {
            self.projeter_epingles(f);
        }
    }

    pub fn montrer_epingles(&self, f: &Principale, visibles: bool) {
        let avant = std::mem::replace(&mut self.verrou().epingles_visibles, visibles);
        if visibles && !avant {
            self.charger_epingles(f);
        } else if !visibles && avant {
            self.store.oublier_epingles();
            self.projeter_epingles(f);
        }
    }

    pub fn epingles_visibles(&self) -> bool {
        self.verrou().epingles_visibles
    }

    pub fn charger_epingles(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            self.projeter_epingles(f);
            return;
        };
        self.verrou().etat_epingles = EtatEpingles::Chargement;
        self.projeter_epingles(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let reussi = pont.store.charger_epingles(&cle).await.is_ok();
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    if i.cle.as_ref() != Some(&cle) || !i.epingles_visibles {
                        return;
                    }
                    i.etat_epingles = if reussi {
                        EtatEpingles::Pret
                    } else {
                        EtatEpingles::Erreur
                    };
                }
                p.projeter_epingles(&f);
            });
        });
    }

    pub fn voir_epingle(&self, id: &str) {
        self.reponse_cliquee(id);
    }

    fn projeter_epingles(&self, f: &Principale) {
        let (cle, serveur, etat, visibles) = {
            let i = self.verrou();
            (
                i.cle.clone(),
                i.serveur.clone(),
                i.etat_epingles,
                i.epingles_visibles,
            )
        };
        let messages = cle
            .as_ref()
            .filter(|_| visibles)
            .and_then(|c| self.store.epingles(c))
            .unwrap_or_default();
        let decalage = {
            let fuseau = self
                .fuseau
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            fuseau(chrono::Utc::now().timestamp_millis())
        };
        let (supprimes, apercus) = {
            let i = self.verrou();
            (i.supprimes.clone(), i.apercus)
        };
        let aucun = BTreeMap::new();
        let maintenant = self.maintenant();
        let droits = self.droits_conversation();
        let donnees: Vec<DonneesMessage> = self.store.lire(|etat| {
            let contexte = ContexteLignes {
                etat,
                serveur: serveur.as_deref(),
                nouveau_depuis: None,
                mis_en_evidence: None,
                supprimes: &supprimes,
                decalage_minutes: decalage,
                maintenant_ms: maintenant,
                apercus,
                lecteurs: &aucun,
                envois: &[],
                droits,
                votants: &HashMap::new(),
            };
            messages
                .iter()
                .map(|m| donnees_message(m, false, &contexte))
                .collect()
        });
        let epingles: Vec<EpingleMessage> = donnees
            .iter()
            .map(|m| {
                let rendus = self.rendus_de(m);
                let couleur = couleur_slint(m.couleur.as_deref());
                EpingleMessage {
                    id: m.id.as_str().into(),
                    auteur: m.auteur.as_str().into(),
                    a_couleur: couleur.is_some(),
                    couleur: couleur.unwrap_or_default(),
                    date: m.date_complete.as_str().into(),
                    blocs: ModelRc::new(VecModel::from(
                        rendus
                            .blocs
                            .iter()
                            .map(|b| bloc_slint(b, None))
                            .collect::<Vec<_>>(),
                    )),
                    fichiers: libelle_fichiers(m.annexes.pieces.len()).into(),
                    desepingler: m.droits.epingler,
                }
            })
            .collect();
        let g = f.global::<DonneesMessagerie>();
        g.set_epingles(ModelRc::new(VecModel::from(epingles)));
        g.set_etat_epingles(if cle.is_some() {
            etat
        } else {
            EtatEpingles::Pret
        });
    }

    pub fn fermer(&self, f: &Principale) {
        self.ouvrir(f, None, None, 0);
    }

    pub fn etat_chargement(&self, f: &Principale, cle: &CleConversation, reussi: bool) {
        {
            let mut i = self.verrou();
            if i.cle.as_ref() != Some(cle) {
                return;
            }
            i.erreur = !reussi && !i.chargee;
        }
        self.rafraichir(f);
    }

    pub fn appliquer(&self, f: &Principale, changements: &[Changement]) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let serveur = self.verrou().serveur.clone();
        let mut reprojeter = false;
        let mut modifies: Vec<String> = Vec::new();
        let mut evidence: Option<String> = None;
        let mut saisie = false;
        let mut epingles = false;
        for changement in changements {
            match changement {
                Changement::Messages(m) => match m {
                    ChangementMessages::AjoutesEnFin {
                        conversation,
                        nombre,
                    } if *conversation == cle => {
                        self.noter_ajouts(&cle, *nombre);
                        reprojeter = true;
                    }
                    ChangementMessages::Supprime {
                        conversation,
                        message,
                    } if *conversation == cle => {
                        self.verrou().supprimes.insert(message.clone());
                        reprojeter = true;
                    }
                    ChangementMessages::Modifie {
                        conversation,
                        message,
                    } if *conversation == cle => modifies.push(message.clone()),
                    ChangementMessages::MiseEnEvidence {
                        conversation,
                        message,
                    } if *conversation == cle => evidence = Some(message.clone()),
                    ChangementMessages::Saisie { conversation } if *conversation == cle => {
                        saisie = true
                    }
                    ChangementMessages::AjoutesEnTete { conversation, .. }
                    | ChangementMessages::Insere { conversation, .. }
                    | ChangementMessages::RetiresEnTete { conversation, .. }
                    | ChangementMessages::RetiresEnFin { conversation, .. }
                    | ChangementMessages::Bornes { conversation }
                    | ChangementMessages::Rechargee { conversation }
                    | ChangementMessages::Lectures { conversation }
                    | ChangementMessages::Envois { conversation }
                        if *conversation == cle =>
                    {
                        reprojeter = true
                    }
                    ChangementMessages::Epingles { conversation } if *conversation == cle => {
                        epingles = true
                    }
                    _ => {}
                },
                Changement::Roles { serveur: s }
                | Changement::Membre { serveur: s, .. }
                | Changement::MembreAjoute { serveur: s, .. }
                | Changement::MembreRetire { serveur: s, .. }
                    if serveur.as_deref() == Some(s.as_str()) =>
                {
                    let mut i = self.verrou();
                    i.mentionnables = None;
                    i.generation += 1;
                    reprojeter = true;
                }
                Changement::Avatar { .. } => reprojeter = true,
                Changement::Emojis { .. } => self.charger_emojis(true),
                _ => {}
            }
        }
        if saisie {
            self.verrou().saisie = texte_de_saisie(&self.store.saisies(&cle));
        }
        if reprojeter {
            self.reprojeter(f);
        } else {
            for id in &modifies {
                self.modifier_ligne(&cle, id);
            }
            if !modifies.is_empty() || saisie {
                self.rafraichir(f);
            }
        }
        if let Some(id) = evidence {
            self.mettre_en_evidence(f, &id);
        }
        if epingles {
            self.projeter_epingles(f);
        }
    }

    fn noter_ajouts(&self, cle: &CleConversation, nombre: usize) {
        let Some(conversation) = self.store.conversation(cle) else {
            return;
        };
        let moi = self.store.lire(|e| e.moi().to_string());
        let mut i = self.verrou();
        if !i.positionnee {
            return;
        }
        let nouveaux: Vec<String> = conversation
            .messages
            .iter()
            .rev()
            .take(nombre)
            .filter(|m| auteur(m) != moi)
            .map(|m| estompe_client::store::messages::id_message(m).to_string())
            .collect();
        if nouveaux.is_empty() {
            if conversation
                .messages
                .iter()
                .rev()
                .take(nombre)
                .any(|m| auteur(m) == moi)
            {
                i.nouveau_depuis = None;
                i.defilement.aller_en_bas();
            }
            return;
        }
        let lecture = !i.defilement.colle() || !i.focus;
        if lecture && i.nouveau_depuis.is_none() {
            i.nouveau_depuis = nouveaux.last().cloned();
        }
        if !i.defilement.colle() {
            i.nouveaux += nouveaux.len();
        }
    }

    fn contexte_et_mentions(&self) -> Option<(CleConversation, Option<String>)> {
        let i = self.verrou();
        Some((i.cle.clone()?, i.serveur.clone()))
    }

    fn reprojeter(&self, f: &Principale) {
        let Some((cle, serveur)) = self.contexte_et_mentions() else {
            self.rafraichir(f);
            return;
        };
        let conversation = self.store.conversation(&cle).filter(|c| c.chargee);
        let decalage = {
            let fuseau = self
                .fuseau
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            fuseau(chrono::Utc::now().timestamp_millis())
        };
        let moi = self.store.lire(|e| e.moi().to_string());
        {
            let mut i = self.verrou();
            i.statistiques.projections += 1;
            if let Some(c) = &conversation {
                if !i.positionnee && i.nouveau_depuis.is_none() {
                    i.nouveau_depuis = premier_non_lu(&c.messages, i.non_lus_ouverture, &moi);
                }
                i.chargee = true;
                i.erreur = false;
                i.debut_atteint = c.debut_atteint;
                i.fin_atteinte = c.fin_atteinte;
            }
        }
        let lignes = match &conversation {
            Some(c) => self.lignes_de(c, serveur.as_deref(), decalage),
            None => Vec::new(),
        };
        let mut a_positionner = false;
        {
            let mut i = self.verrou();
            let largeur = i.defilement.largeur();
            let mut toutes: Vec<DonneesLigne> = Vec::with_capacity(lignes.len() + 2);
            if conversation.is_some() {
                toutes.push(pseudo_ligne(if i.debut_atteint {
                    CLE_DEBUT
                } else {
                    CLE_CHARGEMENT_HAUT
                }));
            }
            toutes.extend(lignes);
            if conversation.is_some() && !i.fin_atteinte {
                toutes.push(pseudo_ligne(CLE_CHARGEMENT_BAS));
            }
            let estimations: Vec<(String, f64)> = toutes
                .iter()
                .map(|l| {
                    let h = if genre_pseudo(&l.cle).is_some() {
                        hauteur_pseudo(&l.cle)
                    } else {
                        estimer_hauteur(l, largeur)
                    };
                    (l.cle.clone(), h)
                })
                .collect();
            i.index = toutes
                .iter()
                .enumerate()
                .map(|(n, l)| (l.cle.clone(), n))
                .collect();
            i.lignes = toutes;
            i.defilement.remplacer_lignes(estimations);
            if conversation.is_some() && !i.positionnee {
                i.positionnee = true;
                a_positionner = true;
            }
        }
        if a_positionner {
            let mut i = self.verrou();
            match i.nouveau_depuis.clone() {
                Some(id) => {
                    i.defilement.aller_a(&format!("new-{id}"), Alignement::Haut);
                }
                None => i.defilement.aller_en_bas(),
            }
        }
        self.rafraichir(f);
    }

    fn lignes_de(
        &self,
        c: &Conversation,
        serveur: Option<&str>,
        decalage: i32,
    ) -> Vec<DonneesLigne> {
        let i = self.verrou();
        let nouveau = i.nouveau_depuis.clone();
        let evidence = i.mis_en_evidence.clone();
        let supprimes = i.supprimes.clone();
        let apercus = i.apercus;
        let cle = i.cle.clone();
        let votants = i.votants.clone();
        drop(i);
        let lecteurs = cle
            .as_ref()
            .map(|c| self.store.lecteurs(c))
            .unwrap_or_default();
        let envois = cle.map(|c| self.store.envois(&c)).unwrap_or_default();
        let messages: std::borrow::Cow<[estompe_client::types::Message]> = if envois.is_empty() {
            std::borrow::Cow::Borrowed(&c.messages)
        } else {
            let mut tous = c.messages.clone();
            tous.extend(envois.iter().map(|e| e.message()));
            std::borrow::Cow::Owned(tous)
        };
        let maintenant = self.maintenant();
        let droits = self.droits_conversation();
        self.store.lire(|etat| {
            lignes_de_conversation(
                &messages,
                &ContexteLignes {
                    etat,
                    serveur,
                    nouveau_depuis: nouveau.as_deref(),
                    mis_en_evidence: evidence.as_deref(),
                    supprimes: &supprimes,
                    decalage_minutes: decalage,
                    maintenant_ms: maintenant,
                    apercus,
                    lecteurs: &lecteurs,
                    envois: &envois,
                    droits,
                    votants: &votants,
                },
            )
        })
    }

    fn modifier_ligne(&self, cle: &CleConversation, id: &str) {
        let Some(conversation) = self.store.conversation(cle) else {
            return;
        };
        let Some(message) = conversation.message(id).cloned() else {
            return;
        };
        let decalage = {
            let fuseau = self
                .fuseau
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            fuseau(chrono::Utc::now().timestamp_millis())
        };
        let (serveur, evidence, supprimes, indice, suite, apercus, votants) = {
            let i = self.verrou();
            let Some(&n) = i.index.get(id) else {
                return;
            };
            let suite = i.lignes[n].message.as_ref().is_some_and(|m| m.suite);
            (
                i.serveur.clone(),
                i.mis_en_evidence.clone(),
                i.supprimes.clone(),
                n,
                suite,
                i.apercus,
                i.votants.clone(),
            )
        };
        let maintenant = self.maintenant();
        let lecteurs = self.store.lecteurs(cle);
        let droits = self.droits_conversation();
        let donnees = self.store.lire(|etat| {
            donnees_message(
                &message,
                suite,
                &ContexteLignes {
                    etat,
                    serveur: serveur.as_deref(),
                    nouveau_depuis: None,
                    mis_en_evidence: evidence.as_deref(),
                    supprimes: &supprimes,
                    decalage_minutes: decalage,
                    maintenant_ms: maintenant,
                    apercus,
                    lecteurs: &lecteurs,
                    envois: &[],
                    droits,
                    votants: &votants,
                },
            )
        });
        let mut i = self.verrou();
        let largeur = i.defilement.largeur();
        if let Some(ligne) = i.lignes.get_mut(indice) {
            ligne.message = Some(donnees);
            let estimation = estimer_hauteur(ligne, largeur);
            let cle_ligne = ligne.cle.clone();
            i.defilement.estimer(&cle_ligne, estimation);
        }
    }

    fn mentionnables(&self) -> Arc<Mentionnables> {
        if let Some(m) = self.verrou().mentionnables.clone() {
            return m;
        }
        let cle = self.conversation();
        let noms: Vec<String> = self
            .medias()
            .emojis()
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.name)
            .collect();
        let m = Arc::new(match &cle {
            Some(cle) => self.store.lire(|etat| mentionnables(etat, cle, &noms)),
            None => Mentionnables::default(),
        });
        self.verrou().mentionnables = Some(m.clone());
        m
    }

    fn rendus_de(&self, m: &DonneesMessage) -> Arc<Rendus> {
        let (cle, trouve) = {
            let mut i = self.verrou();
            let cle = CleRendu {
                id: m.id.clone(),
                contenu: m.contenu.clone(),
                modifie: m.modifie,
                reveles: i.reveles.get(&m.id).cloned().unwrap_or_default(),
                palette: i.palette.clone(),
                generation: i.generation,
            };
            let trouve = i.rendus.lire(&cle).cloned();
            (cle, trouve)
        };
        if let Some(r) = trouve {
            return r;
        }
        let contexte = self.mentionnables();
        let rendu = analyser(&m.contenu, &*contexte);
        let rendus = Arc::new(Rendus {
            blocs: if m.contenu.is_empty() && !m.modifie {
                Vec::new()
            } else {
                blocs_affiches(&rendu, &cle.reveles, &cle.palette, m.modifie)
            },
            texte_brut: texte_brut(&rendu.blocs),
        });
        let mut i = self.verrou();
        i.statistiques.rendus_calcules += 1;
        i.rendus.inserer(cle, rendus.clone());
        rendus
    }

    fn rafraichir(&self, f: &Principale) {
        if PROPRIETAIRE.with(|p| p.get()) != self.identifiant {
            return;
        }
        let g = f.global::<DonneesMessagerie>();
        let raison = self.raison_lecture_seule();
        let joindre = self.pieces_possibles();
        let sondage = self.sondage_possible();
        let s = f.global::<crate::DonneesSaisie>();
        if s.get_joindre_possible() != joindre {
            s.set_joindre_possible(joindre);
        }
        if s.get_sondage_possible() != sondage {
            s.set_sondage_possible(sondage);
        }
        let gifs = self.gifs_possibles();
        if s.get_gifs_possibles() != gifs {
            s.set_gifs_possibles(gifs);
        }
        let (fenetre, lignes_fenetre) = {
            let mut i = self.verrou();
            i.lecture_seule = raison.is_some();
            i.lecture_seule_texte = raison.unwrap_or_default().to_string();
            let fenetre = i.defilement.fenetre();
            let fin = fenetre.fin.min(i.lignes.len());
            let debut = fenetre.debut.min(fin);
            (fenetre, i.lignes[debut..fin].to_vec())
        };
        let mut nouvelles: Vec<LigneFenetre> = Vec::with_capacity(lignes_fenetre.len());
        let mut avatars = Vec::new();
        let mut badges = Vec::new();
        let mut demandes = Demandes::default();
        let medias = self.medias();
        let serveur = self.verrou().serveur.clone();
        for donnees in lignes_fenetre {
            let medias_ligne = match &donnees.message {
                Some(m) => {
                    let rendus = self.rendus_de(m);
                    self.medias_de_ligne(&medias, m, &rendus, serveur.as_deref(), &mut demandes)
                }
                None => MediasLigne::default(),
            };
            let (rendus, portrait, portraits_lecteurs, badges_ligne) = match &donnees.message {
                Some(m) => {
                    let rendus = self.rendus_de(m);
                    let mut present = |c: &CleAvatar| {
                        let present = self.profils.avatar(c).is_some();
                        if !present {
                            let mut i = self.verrou();
                            if !i.echecs_avatars.contains(c) && i.demandes_avatars.insert(c.clone())
                            {
                                avatars.push(c.clone());
                            }
                        }
                        present
                    };
                    let portrait = m.avatar.as_ref().is_some_and(&mut present);
                    let portraits_lecteurs = m
                        .annexes
                        .lecteurs
                        .iter()
                        .filter_map(|l| l.avatar.as_ref())
                        .filter(|c| present(c))
                        .count();
                    let badges_ligne = match self.profils.badges(&m.utilisateur) {
                        Some(b) => b,
                        None => {
                            if !m.suite
                                && !m.systeme
                                && self.verrou().demandes_badges.insert(m.utilisateur.clone())
                            {
                                badges.push(m.utilisateur.clone());
                            }
                            Vec::new()
                        }
                    };
                    (
                        Some(rendus),
                        portrait,
                        portraits_lecteurs,
                        if m.suite { Vec::new() } else { badges_ligne },
                    )
                }
                None => (None, false, 0, Vec::new()),
            };
            nouvelles.push(LigneFenetre {
                donnees,
                rendus,
                portrait,
                portraits_lecteurs,
                badges: badges_ligne,
                medias: medias_ligne,
            });
        }
        let modele_rc = g.get_lignes();
        let modele = vec_de(&modele_rc);
        let profils = self.profils.clone();
        let mut i = self.verrou();
        let anciennes: Vec<String> = i
            .ombre_fenetre
            .iter()
            .map(|l| l.donnees.cle.clone())
            .collect();
        let faites = {
            let Interne {
                ombre_fenetre,
                animations,
                ..
            } = &mut *i;
            synchroniser(
                modele,
                ombre_fenetre,
                nouvelles,
                |l| l.donnees.cle.clone(),
                |l| ligne_slint(l, &profils, &medias, animations),
            )
        };
        if !faites.is_empty() {
            i.statistiques.conversions += faites
                .iter()
                .filter(|o| !matches!(o, Operation::Retirer(_)))
                .count();
            let ops = cles_des_operations(anciennes, &i.ombre_fenetre, &faites);
            i.operations = ops;
        }
        i.avec_portrait = i
            .ombre_fenetre
            .iter()
            .filter(|l| l.portrait)
            .map(|l| l.donnees.cle.clone())
            .collect();
        let en_bas = i.defilement.en_bas();
        if en_bas && i.fin_atteinte {
            i.nouveaux = 0;
        }
        let proprietes = Proprietes {
            active: i.cle.is_some(),
            haut: fenetre.haut as f32,
            bas: fenetre.bas as f32,
            defilement: -(i.defilement.position() as f32),
            etat: Some(if i.chargee {
                EtatConversation::Prete
            } else if i.erreur {
                EtatConversation::Erreur
            } else {
                EtatConversation::Chargement
            }),
            bouton_bas: i.chargee && (!en_bas || !i.fin_atteinte),
            nouveaux: i.nouveaux.min(i32::MAX as usize) as i32,
            saisie: i.saisie.clone(),
            lecture_seule: i.lecture_seule,
            lecture_seule_texte: i.lecture_seule_texte.clone(),
        };
        let ancienne = std::mem::replace(&mut i.proprietes, proprietes.clone());
        let charger_haut = i.chargee
            && !i.debut_atteint
            && !i.chargement_haut
            && !i.echec_page
            && i.defilement.pres_du_haut();
        if charger_haut {
            i.chargement_haut = true;
        }
        let charger_bas = i.chargee
            && !i.fin_atteinte
            && !i.chargement_bas
            && !i.echec_page
            && i.defilement.pres_du_bas();
        if charger_bas {
            i.chargement_bas = true;
        }
        let lecture = i.chargee && i.fin_atteinte && en_bas;
        let signaler = i.cle.is_some() && i.chargee && i.en_bas_signale != Some(lecture);
        if signaler {
            i.en_bas_signale = Some(lecture);
        }
        let ancre = i
            .defilement
            .ancre()
            .and_then(|a| i.index.get(&a.cle).copied())
            .and_then(|n| i.lignes.get(n))
            .and_then(|l| l.message.as_ref())
            .map(|m| m.id.clone());
        let cle = i.cle.clone();
        drop(i);
        if ancienne.active != proprietes.active {
            g.set_active(proprietes.active);
        }
        if ancienne.haut != proprietes.haut {
            g.set_haut(proprietes.haut);
        }
        if ancienne.bas != proprietes.bas {
            g.set_bas(proprietes.bas);
        }
        if (g.get_defilement() - proprietes.defilement).abs() >= 0.5 {
            g.set_defilement(proprietes.defilement);
        }
        if ancienne.etat != proprietes.etat {
            g.set_etat(proprietes.etat.unwrap_or(EtatConversation::Chargement));
        }
        if ancienne.bouton_bas != proprietes.bouton_bas {
            g.set_bouton_bas(proprietes.bouton_bas);
        }
        if ancienne.nouveaux != proprietes.nouveaux {
            g.set_nouveaux(proprietes.nouveaux);
        }
        if ancienne.saisie != proprietes.saisie {
            g.set_saisie(proprietes.saisie.as_str().into());
        }
        if ancienne.lecture_seule != proprietes.lecture_seule
            || ancienne.lecture_seule_texte != proprietes.lecture_seule_texte
        {
            g.set_lecture_seule(proprietes.lecture_seule);
            g.set_lecture_seule_texte(proprietes.lecture_seule_texte.as_str().into());
        }
        let Some(cle) = cle else {
            return;
        };
        if charger_haut {
            self.charger_page(cle.clone(), true);
        }
        if charger_bas {
            self.charger_page(cle.clone(), false);
        }
        if signaler {
            let store = self.store.clone();
            self.runtime
                .spawn(async move { store.signaler_lecture(lecture, ancre).await });
        }
        for avatar in avatars {
            self.telecharger_avatar(avatar);
        }
        for utilisateur in badges {
            self.telecharger_badges(utilisateur);
        }
        for image in demandes.images {
            self.charger_image(image);
        }
        for (lien, message) in demandes.apercus {
            self.charger_apercu(lien, message);
        }
        if demandes.emojis {
            self.charger_emojis(false);
        }
        self.suivre_animations(f);
        self.planifier_sondages();
        self.rafraichir_visionneuse(f);
    }

    fn charger_page(&self, cle: CleConversation, haut: bool) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = if haut {
                pont.store.charger_plus_anciens(&cle).await
            } else {
                pont.store.charger_plus_recents(&cle).await
            };
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    if i.cle.as_ref() != Some(&cle) {
                        return;
                    }
                    if haut {
                        i.chargement_haut = false;
                    } else {
                        i.chargement_bas = false;
                    }
                    i.echec_page = resultat.is_err();
                }
                p.reprojeter(&f);
            });
        });
    }

    fn telecharger_avatar(&self, cle: CleAvatar) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let recu = pont.profils.charger_avatar(cle.clone()).await.is_some();
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    i.demandes_avatars.remove(&cle);
                    if !recu {
                        i.echecs_avatars.insert(cle);
                    }
                }
                p.rafraichir(&f);
            });
        });
    }

    fn telecharger_badges(&self, utilisateur: String) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            pont.profils.charger_badges(utilisateur.clone()).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.verrou().demandes_badges.remove(&utilisateur);
                p.rafraichir(&f);
            });
        });
    }

    fn planifier(&self) {
        if PLANIFIE.with(|p| p.replace(true)) {
            return;
        }
        let pont = self.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            PLANIFIE.with(|p| p.set(false));
            if let Some(f) = pont.fenetre.upgrade() {
                pont.rafraichir(&f);
            }
        });
    }

    pub fn defilee(&self, f: &Principale, position: f32, vue: f32, largeur: f32) {
        let remesurer = {
            let mut i = self.verrou();
            let largeur_avant = i.defilement.largeur();
            let echo = (position + i.proprietes.defilement).abs() < 0.5;
            let position = f64::from(position);
            if !echo && (position - i.defilement.position()).abs() >= 0.5 {
                i.defilement.defiler(position);
                i.echec_page = false;
            }
            if vue > 0.0 && largeur > 0.0 {
                i.defilement
                    .redimensionner(f64::from(largeur), f64::from(vue));
            }
            (i.defilement.largeur() - largeur_avant).abs() >= 0.5
        };
        if remesurer {
            let g = f.global::<DonneesMessagerie>();
            g.set_mesure(g.get_mesure().wrapping_add(1));
        }
        self.planifier();
    }

    pub fn ligne_mesuree(&self, _f: &Principale, cle: &str, hauteur: f32) {
        let change = self.verrou().defilement.mesurer(cle, f64::from(hauteur));
        if change {
            self.planifier();
        }
    }

    pub fn lien_clique(&self, f: &Principale, id: &str, cible: &str) -> Option<String> {
        match analyser_cible(cible)? {
            Cible::Lien(url) => {
                let navigateur = self
                    .navigateur
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let _ = navigateur.ouvrir(&url);
                None
            }
            Cible::Mention(nom) => {
                let cle = self.conversation()?;
                self.store
                    .lire(|etat| utilisateur_mentionne(etat, &cle, &nom))
            }
            Cible::Masque(n) => {
                self.verrou()
                    .reveles
                    .entry(id.to_string())
                    .or_default()
                    .insert(n);
                self.rafraichir(f);
                None
            }
        }
    }

    pub fn reponse_cliquee(&self, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let store = self.store.clone();
        let id = id.to_string();
        self.runtime.spawn(async move {
            let _ = store.sauter_a(&cle, &id).await;
        });
    }

    fn mettre_en_evidence(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let precedent = {
            let mut i = self.verrou();
            i.defilement.aller_a(id, Alignement::Centre);
            i.mis_en_evidence.replace(id.to_string())
        };
        if let Some(precedent) = precedent.filter(|p| p != id) {
            self.modifier_ligne(&cle, &precedent);
        }
        self.modifier_ligne(&cle, id);
        self.rafraichir(f);
        let pont = self.clone();
        let id = id.to_string();
        let minuterie = slint::Timer::default();
        minuterie.start(
            slint::TimerMode::SingleShot,
            DUREE_MISE_EN_EVIDENCE,
            move || {
                if let Some(f) = pont.fenetre.upgrade() {
                    pont.fin_mise_en_evidence(&f, &id);
                }
            },
        );
        MINUTERIE.with(|m| *m.borrow_mut() = Some(minuterie));
    }

    pub fn fin_mise_en_evidence(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        {
            let mut i = self.verrou();
            if i.mis_en_evidence.as_deref() != Some(id) {
                return;
            }
            i.mis_en_evidence = None;
        }
        self.modifier_ligne(&cle, id);
        self.rafraichir(f);
    }

    pub fn mis_en_evidence(&self) -> Option<String> {
        self.verrou().mis_en_evidence.clone()
    }

    pub fn aller_en_bas(&self, f: &Principale) {
        {
            let mut i = self.verrou();
            i.nouveaux = 0;
            i.nouveau_depuis = None;
            i.echec_page = false;
            i.defilement.aller_en_bas();
        }
        self.reprojeter(f);
    }

    pub fn reessayer(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            return;
        };
        self.verrou().erreur = false;
        self.rafraichir(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let reussi = pont.store.charger_conversation(&cle).await.is_ok();
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.etat_chargement(&f, &cle, reussi));
        });
    }

    pub fn theme_change(&self, f: &Principale) {
        let palette = palette_du_theme(f);
        {
            let mut i = self.verrou();
            if i.palette == palette {
                return;
            }
            i.palette = palette;
        }
        self.rafraichir(f);
    }

    pub fn fenetre_active(&self, actif: bool) {
        self.verrou().focus = actif;
        let store = self.store.clone();
        self.runtime
            .spawn(async move { store.definir_focus(actif).await });
    }

    pub fn expirer_saisies(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let texte = texte_de_saisie(&self.store.saisies(&cle));
        let change = {
            let mut i = self.verrou();
            let change = i.saisie != texte;
            i.saisie = texte;
            change
        };
        if change {
            self.rafraichir(f);
        }
    }

    fn medias_de_ligne(
        &self,
        medias: &Medias,
        m: &DonneesMessage,
        rendus: &Rendus,
        serveur: Option<&str>,
        d: &mut Demandes,
    ) -> MediasLigne {
        let mut i = self.verrou();
        let mut index_image = 0;
        let mut pieces = Vec::with_capacity(m.annexes.pieces.len());
        for p in &m.annexes.pieces {
            let mut affichee = PieceAffichee {
                donnees: p.clone(),
                largeur: 0,
                hauteur: 0,
                chargee: false,
                echec: false,
                index_image: -1,
                etat: i.pieces.get(&p.id).cloned().unwrap_or_default(),
            };
            if p.genre == GenrePiece::Image {
                let cle = cle_vignette(&p.id);
                affichee.index_image = index_image;
                index_image += 1;
                match medias.image(&cle) {
                    Some(image) => {
                        (affichee.largeur, affichee.hauteur) = image.dimensions();
                        affichee.chargee = true;
                    }
                    None => {
                        affichee.echec = medias.en_echec(&cle);
                        (affichee.largeur, affichee.hauteur) =
                            taille_de_vignette(medias.dimensions(&cle.source));
                        if !affichee.echec && i.demandes_images.insert(cle.clone()) {
                            d.images.push(cle);
                        }
                    }
                }
            }
            pieces.push(affichee);
        }
        let mut emojis: Vec<EmojiAffiche> = Vec::new();
        for segment in rendus.blocs.iter().flat_map(|b| &b.segments) {
            if segment.genre != GenreSegmentAffiche::EmojiServeur
                || emojis.iter().any(|e| e.nom == segment.cible)
            {
                continue;
            }
            let image = medias.emoji(&segment.cible, serveur).map(|e| CleImage {
                source: Source::Emoji(e.id),
                format: Format::Emoji,
            });
            let chargee = image.as_ref().is_some_and(|c| medias.image(c).is_some());
            if let Some(c) = &image {
                if !chargee && !medias.en_echec(c) && i.demandes_images.insert(c.clone()) {
                    d.images.push(c.clone());
                }
            }
            emojis.push(EmojiAffiche {
                nom: segment.cible.clone(),
                image,
                chargee,
            });
        }
        if medias.emojis().is_none() && !i.emojis_demandes {
            d.emojis = true;
        }
        let carte = match &m.annexes.lien {
            Some(lien) => match medias.apercu(lien, Some(&m.id)) {
                Some(apercu) => carte_affichee(lien, apercu.as_ref()),
                None => {
                    if i.demandes_apercus
                        .insert(estompe_client::apercu_liens::cle_de_cache(
                            Some(&m.id),
                            lien,
                        ))
                    {
                        d.apercus.push((lien.clone(), m.id.clone()));
                    }
                    carte_affichee(lien, None)
                }
            },
            None => CarteAffichee::Aucune,
        };
        let carte_chargee = match carte.image() {
            Some(cle) => {
                let chargee = medias.image(&cle).is_some();
                if !chargee && !medias.en_echec(&cle) && i.demandes_images.insert(cle.clone()) {
                    d.images.push(cle);
                }
                chargee
            }
            None => false,
        };
        MediasLigne {
            pieces,
            carte,
            carte_chargee,
            emojis,
        }
    }

    fn charger_image(&self, cle: CleImage) {
        let pont = self.clone();
        let medias = self.medias();
        self.runtime.spawn(async move {
            medias.charger_image(cle.clone()).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.verrou().demandes_images.remove(&cle);
                p.rafraichir(&f);
            });
        });
    }

    fn charger_apercu(&self, lien: String, message: String) {
        let pont = self.clone();
        let medias = self.medias();
        self.runtime.spawn(async move {
            medias.charger_apercu(&lien, Some(&message)).await;
            let p = pont.clone();
            let _ =
                pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.verrou().demandes_apercus.remove(
                        &estompe_client::apercu_liens::cle_de_cache(Some(&message), &lien),
                    );
                    p.rafraichir(&f);
                });
        });
    }

    fn charger_emojis(&self, force: bool) {
        {
            let mut i = self.verrou();
            if i.emojis_demandes && !force {
                return;
            }
            i.emojis_demandes = true;
        }
        let pont = self.clone();
        let medias = self.medias();
        self.runtime.spawn(async move {
            if !medias.charger_emojis().await {
                return;
            }
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    i.mentionnables = None;
                    i.generation += 1;
                }
                p.reprojeter(&f);
                if p.selecteur.ouvert() {
                    p.charger_images_du_selecteur(&f);
                }
            });
        });
    }

    fn suivre_animations(&self, f: &Principale) {
        let _ = f;
        let medias = self.medias();
        let prochain = {
            let mut garde = self.verrou();
            let i = &mut *garde;
            let mut actives = HashMap::new();
            for l in &i.ombre_fenetre {
                if !i.defilement.visible(&l.donnees.cle) {
                    continue;
                }
                let mut cibles: Vec<(Emplacement, CleImage)> = l
                    .medias
                    .pieces
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.chargee && p.donnees.anime)
                    .map(|(n, p)| (Emplacement::Piece(n), cle_vignette(&p.donnees.id)))
                    .collect();
                cibles.extend(
                    l.medias
                        .emojis
                        .iter()
                        .filter(|e| e.chargee)
                        .filter_map(|e| {
                            e.image
                                .clone()
                                .map(|c| (Emplacement::Emoji(e.nom.clone()), c))
                        }),
                );
                for (emplacement, cle) in cibles {
                    let Some(Decodee::Animee(a)) = medias.image(&cle) else {
                        continue;
                    };
                    let k = (l.donnees.cle.clone(), emplacement);
                    let etat = i.animations.get(&k).cloned().unwrap_or(EtatAnimation {
                        image: cle,
                        indice: 0,
                        restant: a.images[0].1,
                    });
                    actives.insert(k, etat);
                }
            }
            i.animations = actives;
            i.animations.values().map(|a| a.restant).min()
        };
        match prochain {
            Some(d) if !self.minuterie_d_animation() => self.armer_animation(d),
            Some(_) => {}
            None => self.arreter_animation(),
        }
    }

    fn arreter_animation(&self) {
        self.verrou().minuterie_active = false;
        ANIMATION.with(|a| {
            if let Some(t) = a.borrow().as_ref() {
                t.stop();
            }
        });
    }

    fn armer_animation(&self, d: Duration) {
        self.verrou().minuterie_active = true;
        let pont = self.clone();
        ANIMATION.with(|a| {
            let mut a = a.borrow_mut();
            let minuterie = a.get_or_insert_with(slint::Timer::default);
            minuterie.start(slint::TimerMode::SingleShot, d, move || {
                if let Some(f) = pont.fenetre.upgrade() {
                    pont.avancer_animations(&f, d);
                }
            });
        });
    }

    fn avancer_animations(&self, f: &Principale, ecoule: Duration) {
        let medias = self.medias();
        let mut changements: Vec<(usize, Emplacement, slint::Image)> = Vec::new();
        let prochain = {
            let mut garde = self.verrou();
            let i = &mut *garde;
            i.statistiques.reveils_animation += 1;
            for ((cle_ligne, emplacement), etat) in i.animations.iter_mut() {
                let Some(Decodee::Animee(a)) = medias.image(&etat.image) else {
                    continue;
                };
                let mut reste = ecoule;
                let mut change = false;
                while reste >= etat.restant {
                    reste -= etat.restant;
                    etat.indice = (etat.indice + 1) % a.images.len();
                    etat.restant = a.images[etat.indice].1;
                    change = true;
                }
                etat.restant -= reste;
                if change {
                    if let Some(rang) = i
                        .ombre_fenetre
                        .iter()
                        .position(|l| l.donnees.cle == *cle_ligne)
                    {
                        changements.push((
                            rang,
                            emplacement.clone(),
                            Image::from_rgba8(a.images[etat.indice].0.clone()),
                        ));
                    }
                }
            }
            i.statistiques.images_animees += changements.len();
            i.animations.values().map(|a| a.restant).min()
        };
        let modele = f.global::<DonneesMessagerie>().get_lignes();
        for (rang, emplacement, image) in changements {
            remplacer_image(&modele, rang, &emplacement, image);
        }
        match prochain {
            Some(d) => self.armer_animation(d),
            None => self.arreter_animation(),
        }
    }

    fn planifier_sondages(&self) {
        let maintenant = self.maintenant();
        let prochain = {
            let mut i = self.verrou();
            let prochain = i
                .ombre_fenetre
                .iter()
                .filter_map(|l| l.donnees.message.as_ref()?.annexes.sondage.as_ref()?.fin_ms)
                .filter(|fin| *fin > maintenant)
                .min();
            if i.reveil_sondage == prochain {
                return;
            }
            i.reveil_sondage = prochain;
            prochain
        };
        let Some(fin) = prochain else {
            REVEIL_SONDAGE.with(|r| *r.borrow_mut() = None);
            return;
        };
        let delai = (fin - maintenant + 50).min(i64::from(i32::MAX));
        let pont = self.clone();
        let minuterie = slint::Timer::default();
        minuterie.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(delai.max(0) as u64),
            move || {
                if let Some(f) = pont.fenetre.upgrade() {
                    pont.verrou().reveil_sondage = None;
                    pont.reprojeter(&f);
                }
            },
        );
        REVEIL_SONDAGE.with(|r| *r.borrow_mut() = Some(minuterie));
    }

    fn images_du_message(&self, message: &str) -> Vec<DonneesPiece> {
        let i = self.verrou();
        i.index
            .get(message)
            .and_then(|&n| i.lignes.get(n))
            .and_then(|l| l.message.as_ref())
            .map(|m| m.annexes.images().cloned().collect())
            .unwrap_or_default()
    }

    fn piece_du_message(&self, message: &str, piece: &str) -> Option<DonneesPiece> {
        let i = self.verrou();
        i.index
            .get(message)
            .and_then(|&n| i.lignes.get(n))
            .and_then(|l| l.message.as_ref())
            .and_then(|m| m.annexes.pieces.iter().find(|p| p.id == piece).cloned())
    }

    pub fn image_cliquee(&self, f: &Principale, message: &str, index: i32) {
        self.verrou().visionneuse = Some(EtatVisionneuse {
            message: message.to_string(),
            index: index.max(0) as usize,
            zoom: 1.0,
            note: String::new(),
        });
        self.rafraichir_visionneuse(f);
    }

    pub fn visionneuse_voisine(&self, f: &Principale, sens: i32) {
        let Some(message) = self
            .verrou()
            .visionneuse
            .as_ref()
            .map(|v| v.message.clone())
        else {
            return;
        };
        let total = self.images_du_message(&message).len();
        if let Some(v) = self.verrou().visionneuse.as_mut() {
            let index = (v.index as i64 + i64::from(sens)).clamp(0, total.max(1) as i64 - 1);
            if index as usize != v.index {
                v.index = index as usize;
                v.zoom = 1.0;
                v.note.clear();
            }
        }
        self.rafraichir_visionneuse(f);
    }

    pub fn visionneuse_zoom(&self, f: &Principale, facteur: f32) {
        if let Some(v) = self.verrou().visionneuse.as_mut() {
            v.zoom = if facteur <= 0.0 {
                1.0
            } else {
                (v.zoom * facteur).clamp(1.0, ZOOM_MAX)
            };
        }
        self.rafraichir_visionneuse(f);
    }

    pub fn fermer_visionneuse(&self, f: &Principale) {
        self.verrou().visionneuse = None;
        self.rafraichir_visionneuse(f);
    }

    fn piece_visionnee(&self) -> Option<(EtatVisionneuse, DonneesPiece, usize)> {
        let v = self.verrou().visionneuse.clone()?;
        let images = self.images_du_message(&v.message);
        let total = images.len();
        let piece = images.into_iter().nth(v.index)?;
        Some((v, piece, total))
    }

    fn noter_visionneuse(&self, note: &str) {
        if let Some(v) = self.verrou().visionneuse.as_mut() {
            v.note = note.to_string();
        }
    }

    pub fn visionneuse_copier(&self) {
        let Some((v, piece, _)) = self.piece_visionnee() else {
            return;
        };
        let presse = verrou_de(&self.presse_papiers);
        if !presse.disponible() {
            return;
        }
        let pont = self.clone();
        let medias = self.medias();
        self.runtime.spawn(async move {
            let note = match medias.octets_de(&Source::Fichier(piece.id.clone())).await {
                Some(octets) => {
                    let copie = tokio::task::spawn_blocking(move || {
                        let image = super::medias::decoder_original(&octets)?;
                        presse
                            .copier_image(&estompe_plateforme::Image {
                                largeur: image.width(),
                                hauteur: image.height(),
                                rgba: image.into_raw(),
                            })
                            .map_err(|e| e.to_string())
                    })
                    .await;
                    match copie {
                        Ok(Ok(())) => "Image copiée".to_string(),
                        _ => "Copie impossible".to_string(),
                    }
                }
                None => "Image indisponible".to_string(),
            };
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.verrou().visionneuse.as_ref().map(|e| &e.message) == Some(&v.message) {
                    p.noter_visionneuse(&note);
                    p.rafraichir_visionneuse(&f);
                }
            });
        });
    }

    pub fn visionneuse_enregistrer(&self, f: &Principale) {
        if let Some((v, piece, _)) = self.piece_visionnee() {
            self.telecharger_piece(f, &v.message, &piece.id);
        }
    }

    fn rafraichir_visionneuse(&self, f: &Principale) {
        let g = f.global::<DonneesMessagerie>();
        let Some((v, piece, total)) = self.piece_visionnee() else {
            let fermee = Visionneuse::default();
            if g.get_visionneuse() != fermee {
                g.set_visionneuse(fermee);
            }
            return;
        };
        let medias = self.medias();
        let cle = CleImage {
            source: Source::Fichier(piece.id.clone()),
            format: Format::Visionneuse,
        };
        let image = medias.image(&cle);
        let echec = image.is_none() && medias.en_echec(&cle);
        if image.is_none() && !echec && self.verrou().demandes_images.insert(cle.clone()) {
            self.charger_image(cle);
        }
        let (largeur, hauteur) = match &image {
            Some(i) => i.dimensions(),
            None => medias
                .dimensions(&Source::Fichier(piece.id.clone()))
                .map(|(l, h)| super::medias::ajuster(l, h, Format::Visionneuse.boite()))
                .unwrap_or((0, 0)),
        };
        let etat = self.etat_piece(&piece.id);
        let note = if !v.note.is_empty() {
            v.note.clone()
        } else {
            etat.note.clone()
        };
        let nouvelle = Visionneuse {
            ouverte: true,
            message: v.message.as_str().into(),
            index: v.index.min(i32::MAX as usize) as i32,
            total: total.min(i32::MAX as usize) as i32,
            nom: piece.nom.as_str().into(),
            chargee: image.is_some(),
            image: image.map(|i| image_de(&i, 0)).unwrap_or_default(),
            largeur: largeur as f32,
            hauteur: hauteur as f32,
            echec,
            zoom: v.zoom,
            copie_possible: verrou_de(&self.presse_papiers).disponible(),
            note: note.as_str().into(),
        };
        if g.get_visionneuse() != nouvelle {
            g.set_visionneuse(nouvelle);
        }
    }

    fn noter_piece(&self, id: &str, modifier: impl FnOnce(&mut EtatPiece)) {
        let mut i = self.verrou();
        modifier(i.pieces.entry(id.to_string()).or_default());
    }

    fn rafraichir_plus_tard(&self) {
        let p = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| p.rafraichir(&f));
    }

    pub fn telecharger_piece(&self, f: &Principale, message: &str, id: &str) {
        let Some(piece) = self.piece_du_message(message, id) else {
            return;
        };
        if self.etat_piece(id).telechargement == Telechargement::EnCours {
            return;
        }
        let fichiers = verrou_de(&self.fichiers);
        let defaut = verrou_de(&self.dossier_par_defaut);
        let dernier = self.verrou().dossier.clone();
        let source = self.medias().source().clone();
        let pont = self.clone();
        let runtime = self.runtime.clone();
        let _ = f;
        let lancer = move || {
            let par_defaut = defaut();
            let choisi = if fichiers.disponible() {
                match fichiers.choisir_dossier(dernier.as_deref().or(par_defaut.as_deref())) {
                    Ok(Some(dossier)) => Some(dossier),
                    Ok(None) => return,
                    Err(_) => par_defaut,
                }
            } else {
                par_defaut
            };
            let Some(dossier) = choisi else {
                pont.noter_piece(&piece.id, |e| {
                    e.telechargement = Telechargement::Echec;
                    e.note = "Aucun dossier de téléchargement".into();
                });
                pont.rafraichir_plus_tard();
                return;
            };
            if std::fs::create_dir_all(&dossier).is_err() {
                pont.noter_piece(&piece.id, |e| {
                    e.telechargement = Telechargement::Echec;
                    e.note = format!("Dossier inaccessible : {}", dossier.display());
                });
                pont.rafraichir_plus_tard();
                return;
            }
            pont.verrou().dossier = Some(dossier.clone());
            let destination = chemin_libre(&dossier, &piece.nom);
            pont.noter_piece(&piece.id, |e| {
                *e = EtatPiece {
                    telechargement: Telechargement::EnCours,
                    ..EtatPiece::default()
                };
            });
            pont.rafraichir_plus_tard();
            let suivi = pont.clone();
            let id = piece.id.clone();
            let progres: Progres = Box::new(move |recus, total| {
                let Some(total) = total.filter(|t| *t > 0) else {
                    return;
                };
                let pourcentage = (recus.saturating_mul(100) / total).min(100) as u8;
                let change = {
                    let mut i = suivi.verrou();
                    let e = i.pieces.entry(id.clone()).or_default();
                    let change = pourcentage >= e.progression.saturating_add(PAS_DE_PROGRESSION);
                    if change {
                        e.progression = pourcentage;
                    }
                    change
                };
                if change {
                    suivi.rafraichir_plus_tard();
                }
            });
            let fin = pont.clone();
            runtime.spawn(async move {
                let resultat = source
                    .telecharger(&piece.id, destination.clone(), progres)
                    .await;
                fin.noter_piece(&piece.id, |e| match resultat {
                    Ok(_) => {
                        *e = EtatPiece {
                            telechargement: Telechargement::Termine,
                            progression: 100,
                            note: format!(
                                "Enregistré dans {}",
                                destination.parent().unwrap_or(&dossier).display()
                            ),
                            chemin: Some(destination.clone()),
                        }
                    }
                    Err(erreur) => {
                        *e = EtatPiece {
                            telechargement: Telechargement::Echec,
                            note: format!("Échec du téléchargement : {erreur}"),
                            ..EtatPiece::default()
                        }
                    }
                });
                fin.rafraichir_plus_tard();
            });
        };
        let _ = std::thread::Builder::new()
            .name("estompe-enregistrer".into())
            .spawn(lancer);
    }

    pub fn noter_telechargement(
        &self,
        id: &str,
        telechargement: Telechargement,
        progression: u8,
        note: &str,
    ) {
        self.noter_piece(id, |e| {
            e.telechargement = telechargement;
            e.progression = progression.min(100);
            e.note = note.to_string();
        });
    }

    pub fn rafraichir_maintenant(&self, f: &Principale) {
        self.rafraichir(f);
    }

    pub fn ouvrir_fichier(&self, f: &Principale, id: &str) {
        let Some(chemin) = self.etat_piece(id).chemin else {
            return;
        };
        if let Err(erreur) = verrou_de(&self.fichiers).ouvrir(&chemin) {
            self.noter_piece(id, |e| e.note = format!("Ouverture impossible : {erreur}"));
            self.rafraichir(f);
        }
    }

    pub fn carte_cliquee(&self, url: &str) {
        if let Some(url) = estompe_client::markdown::lien_ouvrable(url) {
            let _ = verrou_de(&self.navigateur).ouvrir(&url);
        }
    }
}

fn remplacer_image(
    modele: &ModelRc<LigneMessage>,
    rang: usize,
    emplacement: &Emplacement,
    image: slint::Image,
) {
    let Some(ligne) = modele.row_data(rang) else {
        return;
    };
    match emplacement {
        Emplacement::Piece(n) => {
            let pieces = ligne.annexes.pieces_jointes;
            if let Some(vm) = pieces.as_any().downcast_ref::<VecModel<PieceJointe>>() {
                if let Some(mut piece) = vm.row_data(*n) {
                    piece.image = image;
                    vm.set_row_data(*n, piece);
                }
            }
        }
        Emplacement::Emoji(nom) => {
            let Some(blocs) = ligne.blocs.as_any().downcast_ref::<VecModel<BlocTexte>>() else {
                return;
            };
            for b in 0..blocs.row_count() {
                let Some(bloc) = blocs.row_data(b) else {
                    continue;
                };
                let Some(segments) = bloc
                    .segments
                    .as_any()
                    .downcast_ref::<VecModel<SegmentTexte>>()
                else {
                    continue;
                };
                for n in 0..segments.row_count() {
                    if let Some(mut segment) = segments.row_data(n) {
                        if segment.cible.as_str() == nom && segment.genre == GenreSegment::Emoji {
                            segment.image = image.clone();
                            segments.set_row_data(n, segment);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct Demandes {
    images: Vec<CleImage>,
    apercus: Vec<(String, String)>,
    emojis: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(contenu: &str, suite: bool) -> DonneesLigne {
        DonneesLigne {
            cle: "1".into(),
            genre: GenreLigne::Message,
            libelle: String::new(),
            message: Some(DonneesMessage {
                id: "1".into(),
                utilisateur: "u".into(),
                auteur: "u".into(),
                initiales: "U".into(),
                teinte: 1,
                couleur: None,
                avatar: None,
                bot: false,
                heure: String::new(),
                date_complete: String::new(),
                suite,
                systeme: false,
                epingle: false,
                modifie: false,
                date_modification: String::new(),
                mention: false,
                mis_en_evidence: false,
                reponse: None,
                contenu: contenu.into(),
                annexes: DonneesAnnexes::default(),
                envoi: None,
                droits: Default::default(),
            }),
        }
    }

    #[test]
    fn l_estimation_croit_avec_le_texte_et_tient_compte_du_regroupement() {
        let court = estimer_hauteur(&message("salut", false), 800.0);
        let suite = estimer_hauteur(&message("salut", true), 800.0);
        let long = estimer_hauteur(&message(&"mot ".repeat(400), false), 800.0);
        let etroit = estimer_hauteur(&message(&"mot ".repeat(400), false), 400.0);
        assert!(suite < court, "{suite} {court}");
        assert!(long > 5.0 * court, "{long} {court}");
        assert!(etroit > long);
        assert_eq!(estimer_hauteur(&pseudo_ligne("x"), 800.0), 40.0);
    }

    #[test]
    fn l_infobulle_d_une_reaction_ne_dessine_pas_l_emoji_en_texte() {
        assert_eq!(
            infobulle_de_reaction("Toi et bob ont réagi avec 👍", "👍"),
            "Toi et bob ont réagi"
        );
        assert_eq!(infobulle_de_reaction("bizarre", "👍"), "bizarre");
    }

    #[test]
    fn le_texte_de_saisie_suit_le_nombre_de_personnes() {
        assert_eq!(texte_de_saisie(&[]), "");
        assert_eq!(
            texte_de_saisie(&["carol".into()]),
            "carol est en train d'écrire…"
        );
        assert_eq!(
            texte_de_saisie(&["carol".into(), "dave".into()]),
            "carol et dave écrivent…"
        );
        assert_eq!(
            texte_de_saisie(&["a".into(), "b".into(), "c".into()]),
            "Plusieurs personnes écrivent…"
        );
    }
}
