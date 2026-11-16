use std::collections::BTreeSet;

use crate::data::{Effect, GameData, Locomotive};

/// Effets cumulés des recherches terminées.
#[derive(Debug, Clone, PartialEq)]
pub struct Modifiers {
    pub speed: f64,
    pub capacity: f64,
    pub fare: f64,
    pub demand: f64,
    pub wear: f64,
    pub auto_dispatch: bool,
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers {
            speed: 1.0,
            capacity: 1.0,
            fare: 1.0,
            demand: 1.0,
            wear: 1.0,
            auto_dispatch: false,
        }
    }
}

impl Modifiers {
    pub fn compute(done: &BTreeSet<String>, data: &GameData) -> Modifiers {
        let mut m = Modifiers::default();
        for effect in done
            .iter()
            .filter_map(|id| data.research(id))
            .flat_map(|r| &r.effects)
        {
            match effect {
                Effect::SpeedMult { value } => m.speed *= value,
                Effect::CapacityMult { value } => m.capacity *= value,
                Effect::FareMult { value } => m.fare *= value,
                Effect::DemandMult { value } => m.demand *= value,
                Effect::TrackWearMult { value } => m.wear *= value,
                Effect::AutoDispatch => m.auto_dispatch = true,
            }
        }
        m
    }

    /// Capacité effective d'une locomotive, au moins une place.
    pub fn capacity_of(&self, loco: &Locomotive) -> u32 {
        ((loco.capacity as f64 * self.capacity).round() as u32).max(1)
    }
}
