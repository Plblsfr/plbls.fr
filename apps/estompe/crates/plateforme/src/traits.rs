use crate::Resultat;

pub trait Coffre {
    fn disponible(&self) -> bool;
    fn lire(&self, cle: &str) -> Resultat<Option<String>>;
    fn ecrire(&self, cle: &str, valeur: &str) -> Resultat<()>;
    fn effacer(&self, cle: &str) -> Resultat<()>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notification {
    pub titre: String,
    pub corps: String,
    pub etiquette: String,
    pub actions: bool,
    pub reponse: bool,
    pub silencieuse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionNotification {
    Ouvrir,
    Repondre(String),
    MarquerLu,
}

pub type RecepteurActions = Box<dyn Fn(String, ActionNotification) + Send + Sync>;

pub trait Notifications: Send + Sync {
    fn disponible(&self) -> bool;
    fn accepte_les_actions(&self) -> bool;
    fn accepte_la_reponse(&self) -> bool;
    fn ecouter(&self, actions: RecepteurActions);
    fn notifier(&self, notification: &Notification) -> Resultat<()>;
    fn retirer(&self, etiquette: &str) -> Resultat<()>;
}

pub trait Pastille {
    fn disponible(&self) -> bool;
    fn attacher(&self, fenetre: raw_window_handle::RawWindowHandle) -> Resultat<()>;
    fn definir(&self, image: Option<&Image>, description: &str) -> Resultat<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvenementZone {
    Clic,
    Ouvrir,
    Quitter,
}

pub type RecepteurZone = Box<dyn Fn(EvenementZone) + Send + Sync>;

pub trait ZoneNotification {
    fn disponible(&self) -> bool;
    fn presente(&self) -> bool;
    fn installer(&self, icone: &Image, info: &str, evenements: RecepteurZone) -> Resultat<()>;
    fn definir_icone(&self, icone: &Image) -> Resultat<()>;
    fn retirer(&self);
}

pub trait Installation: Send + Sync {
    fn disponible(&self) -> bool;
    fn cible(&self) -> Option<&'static str>;
    fn lancer(&self, installeur: &std::path::Path) -> Resultat<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatTouche {
    Appui,
    Relache,
}

pub type ActionRaccourci = Box<dyn Fn(EtatTouche) + Send + Sync>;

pub trait RaccourcisGlobaux: Send + Sync {
    fn disponible(&self) -> bool;
    fn enregistrer(&self, touche: &str, action: ActionRaccourci) -> Resultat<()>;
    fn retirer(&self, touche: &str) -> Resultat<()>;
}

pub trait OuvertureUrl {
    fn disponible(&self) -> bool;
    fn ouvrir(&self, url: &str) -> Resultat<()>;
}

pub trait Fichiers {
    fn disponible(&self) -> bool;
    fn choisir_dossier(
        &self,
        depart: Option<&std::path::Path>,
    ) -> Resultat<Option<std::path::PathBuf>>;
    fn choisir_fichiers(
        &self,
        depart: Option<&std::path::Path>,
    ) -> Resultat<Vec<std::path::PathBuf>>;
    fn ouvrir(&self, chemin: &std::path::Path) -> Resultat<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContenuPressePapiers {
    Image(Image),
    Fichiers(Vec<std::path::PathBuf>),
}

pub trait PressePapiers {
    fn disponible(&self) -> bool;
    fn copier_image(&self, image: &Image) -> Resultat<()>;
    fn lire_pieces(&self) -> Resultat<Option<ContenuPressePapiers>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeSource {
    Ecran,
    Fenetre,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCapture {
    pub id: String,
    pub nom: String,
    pub genre: TypeSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Image {
    pub largeur: u32,
    pub hauteur: u32,
    pub rgba: Vec<u8>,
}

pub trait CaptureEcran {
    fn disponible(&self) -> bool;
    fn sources(&self) -> Resultat<Vec<SourceCapture>>;
    fn capturer(&self, source: &str) -> Resultat<Image>;
}

pub type RecepteurSon = Box<dyn FnMut(&[f32], u32) + Send>;

pub trait CaptureSon {
    fn disponible(&self) -> bool;
    fn demarrer(&self, source: Option<&str>, echantillons: RecepteurSon) -> Resultat<()>;
    fn arreter(&self) -> Resultat<()>;
}

pub trait Overlay {
    fn disponible(&self) -> bool;
    fn afficher(&self, image: &Image, x: i32, y: i32) -> Resultat<()>;
    fn masquer(&self) -> Resultat<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationPremierPlan {
    pub executable: String,
    pub titre: String,
    pub plein_ecran: bool,
}

pub trait PremierPlan {
    fn disponible(&self) -> bool;
    fn application(&self) -> Resultat<Option<ApplicationPremierPlan>>;
}

pub trait DemarrageSession {
    fn disponible(&self) -> bool;
    fn actif(&self) -> Resultat<bool>;
    fn definir(&self, actif: bool) -> Resultat<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensAudio {
    Entree,
    Sortie,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheriqueAudio {
    pub id: String,
    pub nom: String,
    pub defaut: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatAudio {
    pub frequence: u32,
    pub canaux: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurFluxAudio {
    Debranche,
    Echec(String),
}

pub type RecepteurAudio = Box<dyn FnMut(&[f32]) + Send>;
pub type FournisseurAudio = Box<dyn FnMut(&mut [f32]) + Send>;
pub type SignalErreurAudio = Box<dyn FnMut(ErreurFluxAudio) + Send>;

pub trait FluxAudio: Send {
    fn format(&self) -> FormatAudio;
    fn peripherique(&self) -> &str;
}

pub trait SystemeAudio: Send + Sync {
    fn disponible(&self) -> bool;
    fn peripheriques(&self, sens: SensAudio) -> Resultat<Vec<PeripheriqueAudio>>;
    fn ouvrir_entree(
        &self,
        id: Option<&str>,
        echantillons: RecepteurAudio,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>>;
    fn ouvrir_sortie(
        &self,
        id: Option<&str>,
        echantillons: FournisseurAudio,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompteursProcessus {
    pub prive: u64,
    pub ws: u64,
    pub pic_ws: u64,
    pub cpu_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfosMachine {
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub coeurs: usize,
    pub ram_mo: Option<u64>,
    pub gpu: Vec<String>,
}

pub trait MesureProcessus: Send + Sync {
    fn disponible(&self) -> bool;
    fn compteurs(&self) -> Resultat<CompteursProcessus>;
    fn machine(&self) -> InfosMachine;
}

pub type ProducteurSon = Box<dyn FnMut(&mut [f32], u32) + Send>;

pub struct FluxSon(#[allow(dead_code)] Box<dyn Send>);

impl FluxSon {
    pub fn nouveau(garde: impl Send + 'static) -> Self {
        Self(Box::new(garde))
    }
}

pub trait Micro {
    fn disponible(&self) -> bool;
    fn ecouter(&self, echantillons: RecepteurSon) -> Resultat<FluxSon>;
}

pub trait SortieSon {
    fn disponible(&self) -> bool;
    fn jouer(&self, producteur: ProducteurSon) -> Resultat<FluxSon>;
}
