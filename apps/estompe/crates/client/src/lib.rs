pub mod appels;
pub mod auth;
pub mod config;
mod genere;
pub mod http;
pub mod journal;
pub mod preferences;
#[cfg(test)]
mod serveur_factice;

pub mod derniere_vue;
pub mod ordre_serveurs;
pub mod roles;
pub mod route;
pub mod sections;
pub mod store;
pub mod temps_reel;
pub mod version;
pub mod apercu_liens;
pub mod emoji;
pub mod fichiers;
pub mod format;
pub mod lru;
pub mod sondage;
pub mod virtualisation;
pub use genere::types;
