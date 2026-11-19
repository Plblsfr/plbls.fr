pub mod appels;
pub mod config;
mod genere;
pub mod http;
pub mod journal;
pub mod preferences;
#[cfg(test)]
mod serveur_factice;

pub use genere::types;
