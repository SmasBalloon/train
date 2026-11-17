//! Simulation pure : aucun accès au terminal, au disque ou à l'horloge.

mod actions;
mod crew;
mod kinematics;
mod modifiers;
mod offline;
mod tick;
mod world;

pub use actions::{ActionError, OrderOutcome, UnlockStatus};
pub use crew::{CrewMember, CrewRole, TrainCrew};
pub use kinematics::Profile;
pub use modifiers::{LineProps, Modifiers};
pub use offline::{OFFLINE_STEP, OfflineReport, catch_up};
pub use tick::{
    MAX_STEP, SimEvent, advance, advance_with_step, allowed_speed, condition_factor,
    demand_per_hour, station_capacity, trip_score, water_wait_s,
};
pub use world::{
    ActiveResearch, Dir, LineState, Prestige, SAVE_VERSION, StationState, Train, TrainOrder,
    TrainState, Trip, World, hour_minute,
};
