//! Rattrapage du temps passé hors du jeu.

use crate::data::{BonusKind, GameData};

use super::tick::{SimEvent, advance_with_step};
use super::world::World;

/// Pas de simulation du rattrapage, en secondes de jeu.
pub const OFFLINE_STEP: f64 = 60.0;

/// Résumé de ce qui s'est passé pendant l'absence du joueur.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OfflineReport {
    /// Durée réelle de l'absence, en secondes.
    pub away_s: f64,
    /// Part de l'absence réellement simulée, en secondes réelles.
    pub simulated_s: f64,
    pub trips: u64,
    pub carried: u64,
    pub revenue: f64,
    /// Variation de trésorerie, entretien déduit.
    pub net: f64,
    /// Identifiants des recherches terminées entre-temps.
    pub research_done: Vec<String>,
    pub track_worn: bool,
    /// Vrai si des trains sont restés à quai faute de départs automatiques.
    pub idle_trains: bool,
    /// Vrai si un stock de combustible s'est vidé (les trains concernés se sont arrêtés).
    pub fuel_out: bool,
    /// Vrai si un événement historique attend une décision.
    pub event_waiting: bool,
}

/// Fait avancer la partie comme si elle avait tourné pendant `away_s`
/// secondes réelles, dans les limites fixées par l'équilibrage (durée
/// maximale et efficacité), avec la même simulation qu'en jeu.
pub fn catch_up(world: &mut World, data: &GameData, away_s: f64) -> OfflineReport {
    let b = &data.balance;
    let away_s = if away_s.is_finite() {
        away_s.max(0.0)
    } else {
        0.0
    };
    // Les équipes de nuit (bonus de prestige) améliorent l'efficacité hors ligne.
    let efficiency =
        (b.offline_efficiency + world.prestige.total(data, BonusKind::OfflineEfficiency)).min(1.0);
    let simulated_s = away_s.min(b.offline_max_h * 3600.0) * efficiency;
    let mut report = OfflineReport {
        away_s,
        simulated_s,
        ..OfflineReport::default()
    };
    let money_before = world.money;

    // Par tranches d'une heure de jeu, pour ne pas garder tous les événements en mémoire.
    let mut remaining = simulated_s * b.time_scale;
    while remaining > 1e-9 {
        let chunk = remaining.min(3600.0);
        for event in advance_with_step(world, data, chunk, OFFLINE_STEP) {
            match event {
                SimEvent::Arrived {
                    passengers,
                    revenue,
                    ..
                } => {
                    report.trips += 1;
                    report.carried += passengers as u64;
                    report.revenue += revenue;
                }
                SimEvent::ResearchDone { id } => report.research_done.push(id),
                SimEvent::TrackWorn { .. } => report.track_worn = true,
                SimEvent::FuelOut { .. } => report.fuel_out = true,
                SimEvent::EventReady { .. }
                | SimEvent::Departed { .. }
                | SimEvent::Repaired { .. }
                | SimEvent::FuelBought { .. }
                | SimEvent::TrainSold { .. }
                | SimEvent::TrainingDone { .. }
                | SimEvent::TrainReplaced { .. } => {}
            }
        }
        remaining -= chunk;
    }

    report.net = world.money - money_before;
    report.event_waiting = world.event_pending.is_some();
    report.idle_trains =
        report.trips == 0 && simulated_s > 0.0 && world.trains.iter().any(|t| t.is_docked());
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (World, GameData) {
        let mut data = GameData::embedded().unwrap();
        data.events.clear();
        // Le conducteur d'essai, sans compétence, ne coûte rien : les comptes restent ronds.
        data.crew.base_wage = 0.0;
        let mut world = World::new(&data);
        world.test_driver(1);
        (world, data)
    }

    #[test]
    fn automated_network_earns_while_away() {
        let (mut world, data) = setup();
        world.research_done.insert("hand_signals".into());
        let before = world.clone();
        let report = catch_up(&mut world, &data, 3600.0);

        assert_eq!(report.away_s, 3600.0);
        assert_eq!(report.simulated_s, 1800.0, "efficacité hors ligne de 50 %");
        // 1800 s réelles à l'échelle 60.
        assert!((world.clock - before.clock - 108_000.0).abs() < 1e-6);
        assert!(report.trips > 20 && report.carried > 500, "{report:?}");
        assert!(report.net > 0.0 && report.revenue > report.net);
        assert!((world.money - before.money - report.net).abs() < 1e-9);
        assert_eq!(world.trips, report.trips);
        assert!(!report.idle_trains);
    }

    #[test]
    fn manual_network_stays_at_the_platform() {
        let (mut world, data) = setup();
        let report = catch_up(&mut world, &data, 3600.0);
        assert_eq!((report.trips, report.net), (0, 0.0));
        assert!(report.idle_trains);
    }

    #[test]
    fn absence_is_capped() {
        let (mut a, data) = setup();
        a.research_done.insert("hand_signals".into());
        let mut b = a.clone();
        let short = catch_up(&mut a, &data, 8.0 * 3600.0);
        let long = catch_up(&mut b, &data, 30.0 * 24.0 * 3600.0);
        assert_eq!(long.away_s, 30.0 * 24.0 * 3600.0);
        assert_eq!(long.simulated_s, short.simulated_s);
        assert_eq!(a, b);
    }

    #[test]
    fn research_finishes_while_away() {
        let (mut world, data) = setup();
        world.money = 1000.0;
        world.start_research(&data, "hand_signals").unwrap();
        let report = catch_up(&mut world, &data, 600.0);
        assert_eq!(report.research_done, vec!["hand_signals".to_string()]);
        assert!(world.research_done.contains("hand_signals"));
    }

    #[test]
    fn empty_coal_stock_and_pending_events_are_reported() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.test_driver(1);
        world.research_done.insert("hand_signals".into());
        world.era = "steam".into();
        world.fuel.insert("coal".into(), 1.0);
        let report = catch_up(&mut world, &data, 3600.0);
        assert!(
            report.fuel_out,
            "une tonne de charbon ne tient pas une heure d'absence"
        );
        // Sans charbon, le train finit son trajet puis reste à quai.
        assert!(world.trains.iter().all(|t| t.is_docked()));
        let trips = world.trips;
        catch_up(&mut world, &data, 3600.0);
        assert_eq!(world.trips, trips, "sans charbon, plus aucun trajet");
        world.passengers_carried = 250;
        let report = catch_up(&mut world, &data, 60.0);
        assert!(
            report.event_waiting,
            "le concours de Rainhill attend le retour du joueur"
        );
        assert_eq!(world.event_pending.as_deref(), Some("rainhill_trials"));
    }

    #[test]
    fn no_time_away_changes_nothing() {
        let (mut world, data) = setup();
        let before = world.clone();
        for away in [0.0, -50.0, f64::NAN] {
            let report = catch_up(&mut world, &data, away);
            assert_eq!(report.simulated_s, 0.0);
            assert!(!report.idle_trains);
        }
        assert_eq!(world, before);
    }
}

#[cfg(test)]
mod balance {
    use super::*;
    use crate::sim::advance_with_step;

    /// Sonde d'équilibrage : `cargo test balance_probe -- --ignored --nocapture`.
    /// Pour chaque machine, une flotte complète tourne deux jours de jeu avec
    /// toutes les recherches et améliorations de son époque.
    #[test]
    #[ignore]
    fn balance_probe() {
        let mut data = GameData::embedded().unwrap();
        data.events.clear();
        println!(
            "{:<30} {:>4} {:>6} {:>7} {:>6} {:>12} {:>12} {:>9}",
            "machine", "n", "km/h", "trajets", "charge", "net €/s", "prix", "retour"
        );
        for (rank, era) in data.eras.iter().enumerate() {
            let research: Vec<&str> = data
                .research
                .iter()
                .filter(|r| data.era_index(&r.era).unwrap() <= rank)
                .map(|r| r.id.as_str())
                .collect();
            let research_cost: f64 = data
                .research
                .iter()
                .filter(|r| r.era == era.id)
                .map(|r| r.cost)
                .sum();
            let upgrade_cost: f64 = data
                .upgrades
                .iter()
                .filter(|u| u.era == era.id)
                .map(|u| u.cost_per_km * 19.0)
                .sum();
            println!(
                "── {} : entrée {}, recherches {}, améliorations {}",
                era.name.fr, era.unlock.cost, research_cost, upgrade_cost
            );
            for loco in data.locomotives.iter().filter(|l| {
                data.era_index(&l.era).unwrap() <= rank
                    && data.era_index(&l.era).unwrap() + 1 >= rank
            }) {
                let mut world = World::new(&data);
                world.trains.clear();
                world.era = era.id.clone();
                world.research_done = research.iter().map(|r| r.to_string()).collect();
                let line = world.lines.get_mut("stockton_darlington").unwrap();
                line.upgrades = data
                    .upgrades
                    .iter()
                    .filter(|u| data.era_index(&u.era).unwrap() <= rank)
                    .map(|u| u.id.clone())
                    .collect();
                let n = world.line_capacity(&data, "stockton_darlington");
                for _ in 0..n {
                    world.spawn_train(&loco.id, "stockton_darlington");
                }
                let m = world.modifiers(&data);
                let speed = crate::sim::allowed_speed(
                    &data,
                    &m,
                    loco,
                    world.lines.get("stockton_darlington"),
                );
                // Mise en régime, puis mesure sur deux jours, voie entretenue.
                advance_with_step(&mut world, &data, 24.0 * 3600.0, 30.0);
                let (money, trips, carried) = (world.money, world.trips, world.passengers_carried);
                for _ in 0..48 {
                    world
                        .lines
                        .get_mut("stockton_darlington")
                        .unwrap()
                        .condition = 1.0;
                    advance_with_step(&mut world, &data, 3600.0, 30.0);
                }
                let real_s = 48.0 * 3600.0 / data.balance.time_scale;
                let net = (world.money - money) / real_s;
                let trips = world.trips - trips;
                let load = (world.passengers_carried - carried) as f64
                    / trips.max(1) as f64
                    / m.capacity_of(loco) as f64;
                println!(
                    "{:<30} {:>4} {:>6.0} {:>7} {:>5.0}% {:>12.1} {:>12.0} {:>7.0} s",
                    loco.name,
                    n,
                    speed,
                    trips,
                    load * 100.0,
                    net,
                    loco.cost,
                    loco.cost * n as f64 / net.max(1e-9)
                );
            }
        }
    }

    /// Sonde du réseau complet : `cargo test network_probe -- --ignored --nocapture`.
    /// À chaque époque, tout ce qui est constructible est construit et équipé.
    #[test]
    #[ignore]
    fn network_probe() {
        let mut data = GameData::embedded().unwrap();
        data.events.clear();
        for (rank, era) in data.eras.iter().enumerate() {
            let mut world = World::new(&data);
            world.trains.clear();
            world.money = 1e15;
            world.era = era.id.clone();
            world.research_done = data
                .research
                .iter()
                .filter(|r| data.era_index(&r.era).unwrap() <= rank)
                .map(|r| r.id.clone())
                .collect();
            let mut spent_lines = 0.0;
            let mut spent_upgrades = 0.0;
            for country in &data.countries {
                let _ = world.buy_concession(&data, &country.id);
            }
            // Plusieurs passes : une ligne en rend d'autres accessibles.
            for _ in 0..6 {
                for line in data.lines() {
                    if let Ok(cost) = world.build_line(&data, &line.id) {
                        spent_lines += cost;
                    }
                }
            }
            let built: Vec<&str> = data
                .lines()
                .filter(|l| world.is_built(&l.id))
                .map(|l| l.id.as_str())
                .collect();
            let mut spent_trains = 0.0;
            for upgrade in &data.upgrades {
                if let Ok(cost) = world.buy_upgrade(&data, &upgrade.id) {
                    spent_upgrades += cost;
                }
            }
            for line in &built {
                // La meilleure machine achetable : celle qui offre le plus de places-kilomètres.
                let m = world.modifiers(&data);
                let best = data
                    .locomotives
                    .iter()
                    .filter(|l| {
                        world.purchase_blocker(&data, l, line).is_none()
                            && !l.speculative
                            && l.id != "v150"
                    })
                    .max_by(|a, b| {
                        let score = |l: &crate::data::Locomotive| {
                            m.capacity_of(l) as f64
                                * crate::sim::allowed_speed(&data, &m, l, world.lines.get(*line))
                        };
                        score(a).total_cmp(&score(b))
                    });
                if let Some(loco) = best {
                    while world.buy_locomotive(&data, &loco.id, line).is_ok() {
                        spent_trains += loco.cost;
                    }
                }
            }
            for fuel in &data.fuels {
                world.fuel.insert(fuel.id.clone(), 0.0);
            }
            advance_with_step(&mut world, &data, 48.0 * 3600.0, 30.0);
            let (money, carried) = (world.money, world.passengers_carried);
            for _ in 0..48 {
                for line in world.lines.values_mut() {
                    line.condition = 1.0;
                }
                advance_with_step(&mut world, &data, 3600.0, 30.0);
            }
            let real_s = 48.0 * 3600.0 / data.balance.time_scale;
            let research: f64 = data
                .research
                .iter()
                .filter(|r| r.era == era.id)
                .map(|r| r.cost)
                .sum();
            println!(
                "{:<28} lignes {:>2} trains {:>3} | net {:>12.0} €/s | voy./h {:>8.0} | lignes {:>12.0} amél. {:>14.0} trains {:>13.0} rech. {:>12.0} entrée {:>11.0} | satisf. {:.2}",
                era.name.fr,
                built.len(),
                world.trains.len(),
                (world.money - money) / real_s,
                (world.passengers_carried - carried) as f64 / 48.0,
                spent_lines,
                spent_upgrades,
                spent_trains,
                research,
                era.unlock.cost,
                world.overall_satisfaction(&data),
            );
        }
    }
}
