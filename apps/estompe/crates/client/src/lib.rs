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
pub mod version;
pub use genere::types;
