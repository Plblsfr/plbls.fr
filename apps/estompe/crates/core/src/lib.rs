//! Commandes Tauri communes à Windows et Android. Les commandes sont dans des sous-modules : à la racine, leurs macros `__cmd__*` entreraient en conflit.

pub mod callback;
pub mod commands;
pub mod debuglog;
