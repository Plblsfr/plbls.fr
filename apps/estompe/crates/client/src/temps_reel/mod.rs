pub mod battement;
pub mod connexion;
pub mod evenements;
pub mod reseau;
pub mod transport;

#[cfg(test)]
mod faux_transport;
#[cfg(test)]
mod serveur_ws_factice;

pub use connexion::{
    adresse_avec_ticket, adresse_ws, FluxTempsReel, OptionsTempsReel, Signal, SourceTicket,
    TempsReel,
};
pub use evenements::{decoder, Evenement, Trame};
pub use reseau::{EtatReseau, StatutConnexion};
