use crate::data::{Balance, GameData};

use super::modifiers::Modifiers;
use super::world::{INCOME_TAU, TrainState, World};

/// Pas de simulation maximal, en secondes de jeu.
pub const MAX_STEP: f64 = 10.0;

/// Ce qui s'est passé pendant une avance de la simulation.
#[derive(Debug, Clone, PartialEq)]
pub enum SimEvent {
    Departed {
        train: u32,
        station: String,
    },
    Arrived {
        train: u32,
        station: String,
        passengers: u32,
        revenue: f64,
    },
    ResearchDone {
        id: String,
    },
    TrackWorn {
        line: String,
    },
}

/// Facteur de vitesse imposé par l'état de la voie.
pub fn condition_factor(condition: f64, b: &Balance) -> f64 {
    if condition >= b.wear_slow_threshold {
        1.0
    } else {
        let t = (condition / b.wear_slow_threshold).clamp(0.0, 1.0);
        b.wear_min_speed_factor + (1.0 - b.wear_min_speed_factor) * t
    }
}

/// Avance la simulation de `dt` secondes de jeu, par pas d'au plus `MAX_STEP`.
pub fn advance(world: &mut World, data: &GameData, dt: f64) -> Vec<SimEvent> {
    let mut events = Vec::new();
    let mut remaining = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    while remaining > 1e-9 {
        let h = remaining.min(MAX_STEP);
        step(world, data, h, &mut events);
        remaining -= h;
    }
    events
}

fn step(world: &mut World, data: &GameData, h: f64, events: &mut Vec<SimEvent>) {
    let b = &data.balance;
    let m = Modifiers::compute(&world.research_done, data);
    world.clock += h;

    // Les voyageurs arrivent dans les gares.
    for station in data.stations() {
        if let Some(state) = world.stations.get_mut(&station.id) {
            let per_hour = station.population as f64 / 1000.0 * b.demand_per_1000_per_h * m.demand;
            state.waiting = (state.waiting + per_hour * h / 3600.0).min(b.station_waiting_cap);
        }
    }

    // Recherche en cours.
    if let Some(active) = &mut world.research_active {
        active.progress_s += h;
        let finished = match data.research(&active.id) {
            Some(r) => active.progress_s >= r.duration_h * 3600.0,
            None => true,
        };
        if finished {
            let id = active.id.clone();
            world.research_active = None;
            if data.research(&id).is_some() {
                world.research_done.insert(id.clone());
                events.push(SimEvent::ResearchDone { id });
            }
        }
    }

    // Trains.
    let mut net = 0.0;
    let mut gross = 0.0;
    let mut carried = 0u64;
    for train in &mut world.trains {
        let (Some(loco), Some(line)) = (data.locomotive(&train.loco), data.line(&train.line))
        else {
            continue;
        };
        match &mut train.state {
            TrainState::AtStation { waited } => {
                *waited += h;
                let capacity = m.capacity_of(loco);
                let station_id = train.dir.origin(line);
                if let Some(station) = world.stations.get_mut(station_id) {
                    train.board_acc += b.boarding_per_min * h / 60.0;
                    let free = capacity.saturating_sub(train.passengers) as f64;
                    let boarding = (train.board_acc + 1e-9)
                        .floor()
                        .min(free)
                        .min(station.waiting.floor())
                        .max(0.0);
                    train.passengers += boarding as u32;
                    station.waiting -= boarding;
                    // Pas de réserve d'embarquement accumulée pendant qu'on attend des voyageurs.
                    train.board_acc = (train.board_acc - boarding).clamp(0.0, 1.0);
                }
                let dwell_ok = *waited >= b.min_dwell_min * 60.0;
                let full = train.passengers >= capacity;
                let waited_enough = *waited >= b.max_wait_min * 60.0 && train.passengers > 0;
                if m.auto_dispatch && dwell_ok && (full || waited_enough) {
                    train.state = TrainState::Moving;
                    events.push(SimEvent::Departed {
                        train: train.id,
                        station: station_id.to_string(),
                    });
                }
            }
            TrainState::Moving => {
                let condition = world.lines.get(&line.id).map_or(1.0, |l| l.condition);
                let speed = loco.top_speed_kmh * m.speed * condition_factor(condition, b);
                let wanted = speed * h / 3600.0;
                let remaining = match train.dir {
                    super::Dir::Outbound => line.length_km - train.pos_km,
                    super::Dir::Inbound => train.pos_km,
                }
                .max(0.0);
                let travelled = wanted.min(remaining);
                match train.dir {
                    super::Dir::Outbound => train.pos_km += travelled,
                    super::Dir::Inbound => train.pos_km -= travelled,
                }
                net -= loco.upkeep_per_h * h / 3600.0;

                if let Some(state) = world.lines.get_mut(&line.id) {
                    let before = state.condition;
                    let wear = travelled * loco.track_wear * m.wear * b.wear_per_train_km;
                    state.condition = (before - wear).max(0.0);
                    if before >= b.wear_slow_threshold && state.condition < b.wear_slow_threshold {
                        events.push(SimEvent::TrackWorn {
                            line: line.id.clone(),
                        });
                    }
                }

                if wanted >= remaining {
                    let station = train.dir.destination(line).to_string();
                    let revenue = train.passengers as f64 * b.fare_per_km * line.length_km * m.fare;
                    net += revenue;
                    gross += revenue;
                    carried += train.passengers as u64;
                    events.push(SimEvent::Arrived {
                        train: train.id,
                        station,
                        passengers: train.passengers,
                        revenue,
                    });
                    train.pos_km = match train.dir {
                        super::Dir::Outbound => line.length_km,
                        super::Dir::Inbound => 0.0,
                    };
                    train.dir = train.dir.flipped();
                    train.passengers = 0;
                    train.board_acc = 0.0;
                    train.state = TrainState::AtStation { waited: 0.0 };
                }
            }
        }
    }

    world.money += net;
    world.lifetime_income += gross;
    world.passengers_carried += carried;
    world.income_recent = world.income_recent * (-h / INCOME_TAU).exp() + net;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Dir;

    fn setup() -> (World, GameData) {
        let data = GameData::embedded().unwrap();
        (World::new(&data), data)
    }

    fn total_waiting(world: &World) -> f64 {
        world.stations.values().map(|s| s.waiting).sum()
    }

    #[test]
    fn passengers_accumulate_up_to_the_station_cap() {
        let (mut world, data) = setup();
        world.trains.clear();
        advance(&mut world, &data, 3600.0);
        // Stockton : 6000 habitants × 8 voyageurs par millier et par heure.
        assert!((world.stations["stockton"].waiting - 48.0).abs() < 1e-6);
        advance(&mut world, &data, 100.0 * 3600.0);
        assert_eq!(
            world.stations["stockton"].waiting,
            data.balance.station_waiting_cap
        );
    }

    #[test]
    fn docked_train_boards_waiting_passengers_up_to_capacity() {
        let (mut world, data) = setup();
        world.stations.get_mut("stockton").unwrap().waiting = 100.0;
        advance(&mut world, &data, 5.0 * 60.0);
        assert_eq!(
            world.trains[0].passengers, 20,
            "4 voyageurs par minute pendant 5 minutes"
        );
        advance(&mut world, &data, 3600.0);
        assert_eq!(world.trains[0].passengers, 40, "plafonné à la capacité");
        assert!(
            world.trains[0].is_docked(),
            "pas de départ automatique sans recherche"
        );
    }

    #[test]
    fn trip_takes_distance_over_speed_and_pays_the_fare() {
        let (mut world, data) = setup();
        world.stations.get_mut("stockton").unwrap().waiting = 100.0;
        advance(&mut world, &data, 600.0);
        assert_eq!(world.trains[0].passengers, 40);
        let money_before = world.money;
        world.dispatch(1).unwrap();

        // 19 km à 24 km/h : 2850 s de jeu.
        let events = advance(&mut world, &data, 2840.0);
        assert!(events.is_empty());
        assert_eq!(world.trains[0].state, TrainState::Moving);
        let events = advance(&mut world, &data, 20.0);
        let revenue = 40.0 * 0.10 * 19.0;
        assert_eq!(
            events,
            vec![SimEvent::Arrived {
                train: 1,
                station: "darlington".into(),
                passengers: 40,
                revenue
            }]
        );
        let train = &world.trains[0];
        assert!(train.is_docked());
        assert_eq!(train.dir, Dir::Inbound);
        assert_eq!(train.pos_km, 19.0);
        assert_eq!(world.passengers_carried, 40);

        // Recette moins le charbon brûlé pendant les 2850 s de trajet.
        let upkeep = 6.0 * 2850.0 / 3600.0;
        assert!(
            (world.money - (money_before + revenue - upkeep)).abs() < 0.02,
            "{}",
            world.money
        );
        assert!(world.income_per_game_second() > 0.0);
    }

    #[test]
    fn research_completes_after_its_duration() {
        let (mut world, data) = setup();
        world.money = 10_000.0;
        world.start_research(&data, "hand_signals").unwrap();
        let events = advance(&mut world, &data, 4.0 * 3600.0 - 20.0);
        assert!(!events.contains(&SimEvent::ResearchDone {
            id: "hand_signals".into()
        }));
        let events = advance(&mut world, &data, 20.0);
        assert!(events.contains(&SimEvent::ResearchDone {
            id: "hand_signals".into()
        }));
        assert!(world.research_done.contains("hand_signals"));
        assert!(world.research_active.is_none());
    }

    #[test]
    fn auto_dispatch_runs_the_line_without_input() {
        let (mut world, data) = setup();
        world.research_done.insert("hand_signals".into());
        let events = advance(&mut world, &data, 24.0 * 3600.0);
        let arrivals = events
            .iter()
            .filter(|e| matches!(e, SimEvent::Arrived { .. }))
            .count();
        assert!(
            arrivals >= 15,
            "seulement {arrivals} arrivées en une journée"
        );
        assert!(world.money > data.balance.start_money);
        assert!(world.passengers_carried > 300);
    }

    #[test]
    fn auto_dispatch_never_sends_an_empty_train() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        world.research_done.insert("hand_signals".into());
        let events = advance(&mut world, &data, 12.0 * 3600.0);
        assert!(events.is_empty());
        assert!(world.trains[0].is_docked());
    }

    #[test]
    fn worn_track_slows_trains_down() {
        let (mut fresh, data) = setup();
        let mut worn = fresh.clone();
        worn.lines.get_mut("stockton_darlington").unwrap().condition = 0.0;
        fresh.dispatch(1).unwrap();
        worn.dispatch(1).unwrap();
        advance(&mut fresh, &data, 600.0);
        advance(&mut worn, &data, 600.0);
        assert!((fresh.trains[0].pos_km - 4.0).abs() < 1e-9);
        assert!((worn.trains[0].pos_km - 2.0).abs() < 1e-9);
        assert_eq!(condition_factor(0.7, &data.balance), 1.0);
        assert!(condition_factor(0.35, &data.balance) < 1.0);
    }

    #[test]
    fn running_trains_wear_the_track_and_report_it_once() {
        let (mut world, data) = setup();
        world.research_done.insert("hand_signals".into());
        world
            .lines
            .get_mut("stockton_darlington")
            .unwrap()
            .condition = 0.7005;
        let events = advance(&mut world, &data, 6.0 * 3600.0);
        let worn = events
            .iter()
            .filter(|e| matches!(e, SimEvent::TrackWorn { .. }))
            .count();
        assert_eq!(worn, 1);
        assert!(world.lines["stockton_darlington"].condition < 0.7);
    }

    #[test]
    fn speed_and_fare_research_change_the_outcome() {
        let (mut world, data) = setup();
        world.research_done.insert("covered_carriages".into());
        world.stations.get_mut("stockton").unwrap().waiting = 100.0;
        advance(&mut world, &data, 600.0);
        world.dispatch(1).unwrap();
        // 5 % plus lent : toujours en route là où le train de base serait arrivé.
        let events = advance(&mut world, &data, 2860.0);
        assert!(events.is_empty());
        let events = advance(&mut world, &data, 200.0);
        match &events[0] {
            SimEvent::Arrived { revenue, .. } => {
                assert!((revenue - 40.0 * 0.10 * 19.0 * 1.2).abs() < 1e-9)
            }
            other => panic!("événement inattendu : {other:?}"),
        }
    }

    #[test]
    fn simulation_is_deterministic_and_independent_of_frame_slicing() {
        let (mut a, data) = setup();
        a.research_done.insert("hand_signals".into());
        let mut b = a.clone();
        advance(&mut a, &data, 5000.0);
        for _ in 0..500 {
            advance(&mut b, &data, 10.0);
        }
        assert_eq!(a, b);
    }

    #[test]
    fn passengers_are_conserved() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        world.research_done.insert("hand_signals".into());
        world.stations.get_mut("stockton").unwrap().waiting = 90.0;
        world.stations.get_mut("darlington").unwrap().waiting = 60.0;
        advance(&mut world, &data, 48.0 * 3600.0);
        let on_board: u32 = world.trains.iter().map(|t| t.passengers).sum();
        let total = total_waiting(&world) + on_board as f64 + world.passengers_carried as f64;
        assert!((total - 150.0).abs() < 1e-9, "{total}");
    }

    #[test]
    fn negative_or_invalid_time_does_nothing() {
        let (mut world, data) = setup();
        let before = world.clone();
        advance(&mut world, &data, -5.0);
        advance(&mut world, &data, f64::NAN);
        assert_eq!(world, before);
    }
}
