use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use tokio::sync::mpsc;

use super::video::{DemandeVideo, FluxVideo, SourceVideo};

pub const SUFFIXE_PARTAGE: &str = "#screen";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JetonSalle {
    pub url: String,
    pub jeton: String,
    pub salle: String,
    pub max_participants: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Qualite {
    Excellente,
    Bonne,
    Faible,
    Perdue,
    #[default]
    Inconnue,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfoParticipant {
    pub identite: String,
    pub nom: String,
    pub attributs: BTreeMap<String, String>,
    pub qualite: Qualite,
    pub local: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceAudio {
    Micro,
    Partage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaisonDeconnexion {
    Volontaire,
    IdentiteDupliquee,
    Retire,
    SalleFermee,
    Reseau,
    Inconnue,
}

impl RaisonDeconnexion {
    pub fn definitive(self) -> bool {
        matches!(
            self,
            Self::Volontaire | Self::IdentiteDupliquee | Self::Retire | Self::SalleFermee
        )
    }
}

pub type FluxTrames = BoxStream<'static, Vec<i16>>;

pub enum EvenementSalle {
    Arrivee(InfoParticipant),
    Depart(String),
    Attributs {
        identite: String,
        attributs: BTreeMap<String, String>,
    },
    Qualite {
        identite: String,
        qualite: Qualite,
    },
    Parole(Vec<String>),
    Permissions {
        publier_micro: bool,
        recevoir: bool,
    },
    Reconnexion,
    Reconnecte,
    Deconnecte(RaisonDeconnexion),
    PisteAudio {
        identite: String,
        source: SourceAudio,
        trames: FluxTrames,
    },
    PisteRetiree {
        identite: String,
        source: SourceAudio,
    },
    VideoPubliee {
        identite: String,
        source: SourceVideo,
    },
    VideoRetiree {
        identite: String,
        source: SourceVideo,
    },
    PisteVideo {
        identite: String,
        source: SourceVideo,
        images: FluxVideo,
    },
    PisteVideoRetiree {
        identite: String,
        source: SourceVideo,
    },
}

impl fmt::Debug for EvenementSalle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrivee(p) => f.debug_tuple("Arrivee").field(p).finish(),
            Self::Depart(id) => f.debug_tuple("Depart").field(id).finish(),
            Self::Attributs {
                identite,
                attributs,
            } => f
                .debug_struct("Attributs")
                .field("identite", identite)
                .field("attributs", attributs)
                .finish(),
            Self::Qualite { identite, qualite } => f
                .debug_struct("Qualite")
                .field("identite", identite)
                .field("qualite", qualite)
                .finish(),
            Self::Parole(ids) => f.debug_tuple("Parole").field(ids).finish(),
            Self::Permissions {
                publier_micro,
                recevoir,
            } => f
                .debug_struct("Permissions")
                .field("publier_micro", publier_micro)
                .field("recevoir", recevoir)
                .finish(),
            Self::Reconnexion => f.write_str("Reconnexion"),
            Self::Reconnecte => f.write_str("Reconnecte"),
            Self::Deconnecte(r) => f.debug_tuple("Deconnecte").field(r).finish(),
            Self::PisteAudio {
                identite, source, ..
            } => f
                .debug_struct("PisteAudio")
                .field("identite", identite)
                .field("source", source)
                .finish(),
            Self::PisteRetiree { identite, source } => f
                .debug_struct("PisteRetiree")
                .field("identite", identite)
                .field("source", source)
                .finish(),
            Self::VideoPubliee { identite, source } => f
                .debug_struct("VideoPubliee")
                .field("identite", identite)
                .field("source", source)
                .finish(),
            Self::VideoRetiree { identite, source } => f
                .debug_struct("VideoRetiree")
                .field("identite", identite)
                .field("source", source)
                .finish(),
            Self::PisteVideo {
                identite, source, ..
            } => f
                .debug_struct("PisteVideo")
                .field("identite", identite)
                .field("source", source)
                .finish(),
            Self::PisteVideoRetiree { identite, source } => f
                .debug_struct("PisteVideoRetiree")
                .field("identite", identite)
                .field("source", source)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurSalle {
    Connexion(String),
    Publication(String),
    Fermee,
}

impl fmt::Display for ErreurSalle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connexion(e) => write!(f, "connexion à la salle impossible : {e}"),
            Self::Publication(e) => write!(f, "publication impossible : {e}"),
            Self::Fermee => f.write_str("salle fermée"),
        }
    }
}

pub trait SortieMicro: Send + Sync {
    fn envoyer(&self, trame: &[i16]);
    fn couper(&self, coupe: bool);
}

pub struct Connexion {
    pub salle: Arc<dyn Salle>,
    pub participants: Vec<InfoParticipant>,
    pub evenements: mpsc::UnboundedReceiver<EvenementSalle>,
}

pub trait Salle: Send + Sync {
    fn identite(&self) -> String;
    fn definir_attributs(
        &self,
        attributs: BTreeMap<String, String>,
    ) -> BoxFuture<'_, Result<(), ErreurSalle>>;
    fn publier_micro(&self) -> BoxFuture<'_, Result<Arc<dyn SortieMicro>, ErreurSalle>>;
    fn regler_video(&self, identite: &str, source: SourceVideo, demande: Option<DemandeVideo>);
    fn fermer(&self) -> BoxFuture<'_, ()>;
}

pub trait Connecteur: Send + Sync {
    fn connecter(&self, jeton: JetonSalle) -> BoxFuture<'static, Result<Connexion, ErreurSalle>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnecteurIndisponible;

impl Connecteur for ConnecteurIndisponible {
    fn connecter(&self, _: JetonSalle) -> BoxFuture<'static, Result<Connexion, ErreurSalle>> {
        Box::pin(async {
            Err(ErreurSalle::Connexion(
                "vocal absent de cette version".to_string(),
            ))
        })
    }
}

pub fn proprietaire(identite: &str) -> &str {
    identite.strip_suffix(SUFFIXE_PARTAGE).unwrap_or(identite)
}

pub fn est_partage(identite: &str) -> bool {
    identite.ends_with(SUFFIXE_PARTAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_participant_du_partage_appartient_a_son_proprietaire() {
        assert_eq!(proprietaire("u1#screen"), "u1");
        assert_eq!(proprietaire("u1"), "u1");
        assert!(est_partage("u1#screen") && !est_partage("u1"));
    }

    #[test]
    fn seules_les_coupures_du_reseau_se_reconnectent() {
        assert!(RaisonDeconnexion::IdentiteDupliquee.definitive());
        assert!(RaisonDeconnexion::Retire.definitive());
        assert!(!RaisonDeconnexion::Reseau.definitive());
        assert!(!RaisonDeconnexion::Inconnue.definitive());
    }
}
