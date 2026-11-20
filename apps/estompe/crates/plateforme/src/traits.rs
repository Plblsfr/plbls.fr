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
}

pub trait Notifications {
    fn disponible(&self) -> bool;
    fn notifier(&self, notification: &Notification) -> Resultat<()>;
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
