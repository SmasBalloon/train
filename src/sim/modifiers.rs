use std::collections::BTreeSet;

use crate::data::{Balance, Effect, GameData, Locomotive, UpgradeEffect};

use super::world::LineState;

/// Effets cumulés des recherches terminées.
#[derive(Debug, Clone, PartialEq)]
pub struct Modifiers {
    pub speed: f64,
    pub capacity: f64,
    pub fare: f64,
    pub demand: f64,
    pub wear: f64,
    pub fuel: f64,
    /// Masse des voitures : plus lourd, donc moins d'accélération.
    pub weight: f64,
    /// Niveau de confort offert aux voyageurs, de 0 à 1.
    pub comfort: f64,
    /// Décélération de freinage ajoutée par la recherche, en m/s².
    pub brake: f64,
    /// Vitesse maximale autorisée par la signalisation, en km/h.
    pub signal_speed: f64,
    /// Trains supplémentaires admis sur chaque ligne.
    pub line_capacity: u32,
    pub auto_dispatch: bool,
    pub no_water_stop: bool,
    /// Accès aux pays dont l'écartement diffère de celui du pays de départ.
    pub variable_gauge: bool,
    /// Vitesse des recherches (bonus de prestige).
    pub research_speed: f64,
    /// Coût des lignes et des améliorations (bonus de prestige).
    pub build_cost: f64,
}

impl Modifiers {
    pub fn compute(done: &BTreeSet<String>, data: &GameData) -> Modifiers {
        let mut m = Modifiers {
            speed: 1.0,
            capacity: 1.0,
            fare: 1.0,
            demand: 1.0,
            wear: 1.0,
            fuel: 1.0,
            weight: 1.0,
            comfort: data.balance.base_comfort,
            brake: 0.0,
            signal_speed: data.balance.base_signal_speed_kmh,
            line_capacity: 0,
            auto_dispatch: false,
            no_water_stop: false,
            variable_gauge: false,
            research_speed: 1.0,
            build_cost: 1.0,
        };
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
                Effect::FuelMult { value } => m.fuel *= value,
                Effect::WeightMult { value } => m.weight *= value,
                Effect::Comfort { value } => m.comfort += value,
                Effect::BrakeAdd { value } => m.brake += value,
                Effect::SignalSpeed { value } => m.signal_speed = m.signal_speed.max(*value),
                Effect::LineCapacity { value } => m.line_capacity += value,
                Effect::AutoDispatch => m.auto_dispatch = true,
                Effect::NoWaterStop => m.no_water_stop = true,
                Effect::VariableGauge => m.variable_gauge = true,
            }
        }
        m.comfort = m.comfort.clamp(0.0, 1.0);
        m
    }

    /// Capacité effective d'une locomotive, au moins une place.
    pub fn capacity_of(&self, loco: &Locomotive) -> u32 {
        ((loco.capacity as f64 * self.capacity).round() as u32).max(1)
    }

    /// Accélération d'un train, en m/s² : puissance rapportée à la masse.
    pub fn accel_of(&self, loco: &Locomotive, b: &Balance) -> f64 {
        let mass = loco.weight_t + self.capacity_of(loco) as f64 * b.unit_weight_t * self.weight;
        (b.accel_k * loco.power_kw / mass).clamp(0.005, b.accel_max)
    }

    /// Décélération de freinage, en m/s².
    pub fn decel(&self, b: &Balance) -> f64 {
        (b.base_decel + self.brake).min(b.decel_max)
    }
}

/// Caractéristiques d'une ligne compte tenu de ses améliorations.
#[derive(Debug, Clone, PartialEq)]
pub struct LineProps {
    base_speed: f64,
    /// Vitesses autorisées, avec les tractions auxquelles elles sont réservées.
    speeds: Vec<(f64, Vec<String>)>,
    /// Multiplicateur d'usure de la voie.
    pub wear: f64,
}

impl LineProps {
    pub fn of(state: &LineState, data: &GameData) -> LineProps {
        let mut props = LineProps {
            base_speed: data.balance.base_line_speed_kmh,
            speeds: Vec::new(),
            wear: 1.0,
        };
        for effect in state
            .upgrades
            .iter()
            .filter_map(|id| data.upgrade(id))
            .flat_map(|u| &u.effects)
        {
            match effect {
                UpgradeEffect::LineSpeed { value, only } => {
                    props.speeds.push((*value, only.clone()))
                }
                UpgradeEffect::WearMult { value } => props.wear *= value,
            }
        }
        props
    }

    /// Vitesse autorisée sur la ligne pour un type de traction, en km/h.
    pub fn speed_limit(&self, traction: &str) -> f64 {
        self.speeds
            .iter()
            .filter(|(_, only)| only.is_empty() || only.iter().any(|t| t == traction))
            .map(|(speed, _)| *speed)
            .fold(self.base_speed, f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::World;

    #[test]
    fn research_effects_accumulate() {
        let data = GameData::embedded().unwrap();
        let b = &data.balance;
        let none = Modifiers::compute(&BTreeSet::new(), &data);
        assert_eq!(
            (none.signal_speed, none.line_capacity, none.brake),
            (50.0, 0, 0.0)
        );
        assert_eq!(none.decel(b), 0.12);

        let done: BTreeSet<String> = [
            "hand_signals",
            "electric_telegraph",
            "hand_brakes",
            "covered_carriages",
            "variable_expansion",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let m = Modifiers::compute(&done, &data);
        assert_eq!(
            m.signal_speed, 100.0,
            "la meilleure signalisation fixe la vitesse"
        );
        assert_eq!(m.line_capacity, 1);
        assert!((m.decel(b) - 0.20).abs() < 1e-9);
        assert!((m.fuel - 0.85).abs() < 1e-9);
        assert!(m.auto_dispatch && !m.no_water_stop);

        // Des voitures plus lourdes font un train qui démarre moins vite.
        let loco = data.locomotive("locomotion_1").unwrap();
        assert!(m.accel_of(loco, b) < none.accel_of(loco, b));
    }

    #[test]
    fn heavier_locomotives_carry_more_but_accelerate_slower_per_tonne() {
        let data = GameData::embedded().unwrap();
        let m = Modifiers::compute(&BTreeSet::new(), &data);
        let accel = |id: &str| m.accel_of(data.locomotive(id).unwrap(), &data.balance);
        // Même signalisation, même voie : la machine légère démarre plus vite.
        assert!(accel("bb_9004") > accel("cc_7107"));
        assert!(accel("tgv_sud_est") > accel("shinkansen_0"));
        for loco in &data.locomotives {
            let a = m.accel_of(loco, &data.balance);
            assert!((0.005..=1.0).contains(&a), "{} : {a}", loco.id);
        }
    }

    #[test]
    fn upgrades_raise_the_line_speed_per_traction() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        let state = world.lines.get_mut("stockton_darlington").unwrap();
        assert_eq!(LineProps::of(state, &data).speed_limit("steam"), 80.0);

        state
            .upgrades
            .extend(["steel_rails".to_string(), "maglev_guideway".to_string()]);
        let props = LineProps::of(state, &data);
        assert_eq!(props.speed_limit("steam"), 140.0);
        assert_eq!(
            props.speed_limit("maglev"),
            650.0,
            "la voie de guidage est réservée au maglev"
        );
        assert!((props.wear - 0.7).abs() < 1e-9);
    }
}
