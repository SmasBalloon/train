//! Rainhill : jeu incrémental de réseau ferroviaire dans le terminal.
//!
//! - `data` : données de jeu chargées depuis des fichiers TOML ;
//! - `sim`  : simulation pure et déterministe (aucune entrée/sortie) ;
//! - `save` : sauvegarde locale ;
//! - `ui`   : interface terminal ;
//! - `daemon`, `notify` : simulation en arrière-plan et notifications de bureau.

pub mod daemon;
pub mod data;
pub mod notify;
pub mod save;
pub mod sim;
pub mod ui;
