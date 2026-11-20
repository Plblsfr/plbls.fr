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

pub trait RaccourcisGlobaux {
    fn disponible(&self) -> bool;
    fn enregistrer(&self, raccourci: &str, action: Box<dyn Fn() + Send>) -> Resultat<()>;
    fn retirer(&self, raccourci: &str) -> Resultat<()>;
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
    fn ouvrir(&self, chemin: &std::path::Path) -> Resultat<()>;
}

pub trait PressePapiers {
    fn disponible(&self) -> bool;
    fn copier_image(&self, image: &Image) -> Resultat<()>;
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
