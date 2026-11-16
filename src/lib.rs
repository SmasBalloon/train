//! Rainhill : jeu incrémental de réseau ferroviaire dans le terminal.
//!
//! - `data` : données de jeu chargées depuis des fichiers TOML ;
//! - `sim`  : simulation pure et déterministe (aucune entrée/sortie) ;
//! - `save` : sauvegarde locale ;
//! - `ui`   : interface terminal.

pub mod data;
pub mod save;
pub mod sim;
pub mod ui;
