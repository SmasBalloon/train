//! Simulation pure : aucun accès au terminal, au disque ou à l'horloge.

mod actions;
mod modifiers;
mod tick;
mod world;

pub use actions::ActionError;
pub use modifiers::Modifiers;
pub use tick::{MAX_STEP, SimEvent, advance, condition_factor};
pub use world::{
    ActiveResearch, Dir, LineState, SAVE_VERSION, StationState, Train, TrainState, World,
};
