use std::collections::BTreeMap;

use crate::data::{
    Balance, Country, Era, GameData, GameEvent, Locomotive, StaffRole, Station, TrafficClass,
};

use super::crew::CrewRole;
use super::kinematics::Profile;
use super::modifiers::{LineProps, Modifiers};
use super::world::{Dir, INCOME_TAU, LineState, Train, TrainState, Trip, World};

/// Pas de simulation maximal en jeu interactif, en secondes de jeu.
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
        delay_min: f64,
    },
    ResearchDone {
        id: String,
    },
    TrackWorn {
        line: String,
    },
    /// Le stock d'un combustible vient de tomber à zéro : achats d'urgence.
    FuelOut {
        fuel: String,
    },
    /// Un événement historique attend une décision du joueur.
    EventReady {
        id: String,
    },
    /// Les brigades de la voie ont remis une ligne à neuf.
    Repaired {
        line: String,
        cost: f64,
    },
    /// Le service des approvisionnements a acheté du combustible.
    FuelBought {
        fuel: String,
        quantity: f64,
        cost: f64,
    },
    /// Un train a été revendu à son arrivée, comme prévu.
    TrainSold {
        train: u32,
        loco: String,
        refund: f64,
    },
    /// Une session de formation s'est terminée : inscrits ayant progressé, et
    /// ceux qui n'ont rien retenu.
    TrainingDone {
        role: CrewRole,
        skill: usize,
        passed: Vec<u32>,
        failed: Vec<u32>,
    },
    /// La machine d'un train a été remplacée à son arrivée, comme prévu.
    TrainReplaced {
        train: u32,
        old: String,
        new: String,
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

/// Lignes construites qui desservent une gare : pour chacune, sa demande de
/// base et son niveau de prix.
fn station_links(world: &World, data: &GameData, station_id: &str) -> Vec<(f64, f64)> {
    data.lines()
        .filter(|l| l.from == station_id || l.to == station_id)
        .filter_map(|l| {
            let state = world.lines.get(&l.id).filter(|s| s.built)?;
            Some((data.line_demand(l), state.fare_level))
        })
        .collect()
}

/// Nombre de voyageurs qu'une gare peut faire attendre : il grandit avec
/// l'époque et avec le trafic des lignes qui la desservent.
pub fn station_capacity(world: &World, data: &GameData, station_id: &str) -> f64 {
    let links = station_links(world, data, station_id);
    capacity_for(data, world.era(data), &links)
}

fn capacity_for(data: &GameData, era: &Era, links: &[(f64, f64)]) -> f64 {
    let traffic: f64 = links.iter().map(|(base, _)| base).sum();
    data.balance.station_waiting_cap.max(3.0 * traffic) * era.demand_mult
}

/// Demande d'une catégorie dans une gare, en voyageurs par heure de jeu.
///
/// Elle additionne les lignes construites qui desservent la gare, et dépend de
/// l'époque, de l'heure, du pays, du prix des billets de chaque ligne (demande
/// élastique) et de la satisfaction passée de cette clientèle.
pub fn demand_per_hour(
    world: &World,
    data: &GameData,
    m: &Modifiers,
    station: &Station,
    class: &TrafficClass,
) -> f64 {
    let links = station_links(world, data, &station.id);
    let Some(country) = data.country_of_station(&station.id) else {
        return 0.0;
    };
    let mood = world.satisfaction_of(&class.id);
    demand_from(
        world.era(data),
        m,
        country,
        class,
        &links,
        world.time_of_day().0,
        mood,
    )
}

fn demand_from(
    era: &Era,
    m: &Modifiers,
    country: &Country,
    class: &TrafficClass,
    links: &[(f64, f64)],
    hour: u32,
    satisfaction: f64,
) -> f64 {
    let lines: f64 = links
        .iter()
        .map(|(base, fare)| base * class.price_factor(*fare))
        .sum();
    lines
        * era.demand_mult
        * m.demand
        * class.share
        * class.hourly_mult(hour)
        * country.demand_mult(&class.id)
        * (0.5 + satisfaction)
}

/// Note d'un trajet pour une catégorie (0 à 1), ou `None` si rien ne lui importe.
pub fn trip_score(
    class: &TrafficClass,
    comfort: f64,
    punctuality: f64,
    crowding: f64,
) -> Option<f64> {
    let total = class.comfort_weight + class.punctuality_weight + class.crowding_weight;
    (total > 0.0).then(|| {
        (class.comfort_weight * comfort
            + class.punctuality_weight * punctuality
            + class.crowding_weight * crowding)
            / total
    })
}

/// Vitesse qu'une machine a le droit d'atteindre sur une ligne, en km/h :
/// la plus contraignante de ses propres limites, de la voie et de la signalisation.
pub fn allowed_speed(
    data: &GameData,
    m: &Modifiers,
    loco: &Locomotive,
    line: Option<&LineState>,
) -> f64 {
    let line_limit = match line {
        Some(state) => LineProps::of(state, data).speed_limit(&loco.traction),
        None => data.balance.base_line_speed_kmh,
    };
    (loco.top_speed_kmh * m.speed)
        .min(line_limit)
        .min(m.signal_speed)
}

/// Arrêt qu'une machine doit marquer en gare pour faire de l'eau, en secondes de jeu.
pub fn water_wait_s(era: &Era, m: &Modifiers, loco: &Locomotive) -> f64 {
    if loco.traction == "steam" && !m.no_water_stop {
        era.rules.water_stop_min * 60.0
    } else {
        0.0
    }
}

/// Profil de marche d'un train qui part maintenant de sa position vers le
/// bout de la ligne. `None` si sa machine ou sa ligne n'existent plus.
pub(crate) fn plan_trip(
    data: &GameData,
    m: &Modifiers,
    train: &Train,
    line_state: Option<&LineState>,
    clock: f64,
) -> Option<Trip> {
    let (loco, line) = (data.locomotive(&train.loco)?, data.line(&train.line)?);
    let b = &data.balance;
    let distance = match train.dir {
        Dir::Outbound => line.length_km - train.pos_km,
        Dir::Inbound => train.pos_km,
    }
    .max(0.0);
    let allowed = allowed_speed(data, m, loco, line_state);
    // Un bon conducteur démarre plus franchement et freine plus tard.
    let finesse = 1.0 + data.crew.effects.driving_accel * train.crew.driving();
    let (accel, decel) = (m.accel_of(loco, b) * finesse, m.decel(b) * finesse);
    let condition = line_state.map_or(1.0, |l| l.condition);
    Some(Trip {
        started: clock,
        origin_km: train.pos_km,
        nominal_s: Profile::new(distance, allowed, accel, decel).duration_s(),
        profile: Profile::new(
            distance,
            allowed * condition_factor(condition, b),
            accel,
            decel,
        ),
    })
}

/// Vrai si toutes les conditions de déclenchement d'un événement sont réunies.
fn event_ready(world: &World, data: &GameData, event: &GameEvent) -> bool {
    let t = &event.trigger;
    t.era.as_ref().is_none_or(|era| {
        data.era_index(era)
            .is_some_and(|i| world.era_index(data) >= i)
    }) && world.passengers_carried >= t.passengers
        && t.research.iter().all(|r| world.research_done.contains(r))
        && (t.owns.is_empty()
            || world
                .trains
                .iter()
                .any(|train| t.owns.contains(&train.loco)))
}

/// Rôles tenus par le personnel engagé.
fn roles(world: &World, data: &GameData) -> Vec<StaffRole> {
    world
        .staff
        .iter()
        .filter_map(|id| data.staff(id))
        .map(|s| s.role)
        .collect()
}

/// Tâches confiées au personnel : entretien de la voie et achats de combustible.
fn automate(world: &mut World, data: &GameData, h: f64, events: &mut Vec<SimEvent>) {
    let era = world.era(data);
    // Les achats se décident une fois par heure de jeu.
    let new_hour = (world.clock / 3600.0).floor() != ((world.clock - h) / 3600.0).floor();
    for role in roles(world, data) {
        match role {
            StaffRole::AutoRepair { threshold } => {
                for line in data.lines() {
                    let worn = world
                        .lines
                        .get(&line.id)
                        .is_some_and(|l| l.built && l.condition < threshold);
                    if worn && let Ok(cost) = world.repair_line(data, &line.id) {
                        events.push(SimEvent::Repaired {
                            line: line.id.clone(),
                            cost,
                        });
                    }
                }
            }
            StaffRole::AutoFuel {
                reserve_h,
                max_price_ratio,
            } if new_hour && era.rules.fuel_market => {
                for fuel in data.fuels.iter().filter(|f| f.stock) {
                    let burn = world.fuel_burn_per_hour(data, &fuel.id);
                    let stock = world.fuel_stock(&fuel.id);
                    let target = burn * reserve_h;
                    if burn <= 0.0 || stock >= target {
                        continue;
                    }
                    let price = fuel.price(world.clock, era);
                    let fair = price <= fuel.base_price * era.cost_mult * max_price_ratio;
                    let urgent = stock < burn * 2.0;
                    if !fair && !urgent {
                        continue;
                    }
                    // Urgence : de quoi tenir ; bon prix : toute la réserve.
                    let wanted = if fair {
                        target - stock
                    } else {
                        burn * 3.0 - stock
                    };
                    let affordable = (world.money / (fuel.lot * price)).floor().max(0.0);
                    let lots = (wanted / fuel.lot).ceil().min(affordable) as u32;
                    if lots > 0
                        && let Ok(cost) = world.buy_fuel(data, &fuel.id, lots)
                    {
                        events.push(SimEvent::FuelBought {
                            fuel: fuel.id.clone(),
                            quantity: fuel.lot * lots as f64,
                            cost,
                        });
                    }
                }
            }
            StaffRole::AutoFuel { .. } | StaffRole::AutoTimetable { .. } => {}
        }
    }
}

/// Avance la simulation de `dt` secondes de jeu, par pas d'au plus `MAX_STEP`.
pub fn advance(world: &mut World, data: &GameData, dt: f64) -> Vec<SimEvent> {
    advance_with_step(world, data, dt, MAX_STEP)
}

/// Comme `advance`, avec un pas choisi (plus grossier pour le rattrapage hors ligne).
pub fn advance_with_step(
    world: &mut World,
    data: &GameData,
    dt: f64,
    max_step: f64,
) -> Vec<SimEvent> {
    let mut events = Vec::new();
    let mut remaining = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let max_step = max_step.max(0.1);
    while remaining > 1e-9 {
        let h = remaining.min(max_step);
        step(world, data, h, &mut events);
        remaining -= h;
    }
    events
}

fn step(world: &mut World, data: &GameData, h: f64, events: &mut Vec<SimEvent>) {
    let b = &data.balance;
    let m = world.modifiers(data);
    let era = world.era(data);
    world.clock += h;
    world.refresh_candidates(data);
    for (session, outcome) in world.finish_trainings(data) {
        let (passed, failed): (Vec<_>, Vec<_>) = outcome.into_iter().partition(|(_, ok)| *ok);
        events.push(SimEvent::TrainingDone {
            role: session.role,
            skill: session.skill,
            passed: passed.into_iter().map(|(id, _)| id).collect(),
            failed: failed.into_iter().map(|(id, _)| id).collect(),
        });
    }
    world.sync_crew();
    let crew_fx = data.crew.effects;
    let mut driven: Vec<(u32, f64)> = Vec::new();
    // Les voyageurs arrivent dans les gares desservies, sans dépasser leur capacité
    // d'accueil. On parcourt les lignes une seule fois pour savoir qui dessert quoi.
    let mut links: BTreeMap<&str, Vec<(f64, f64)>> = BTreeMap::new();
    for line in data.lines() {
        if let Some(state) = world.lines.get(&line.id).filter(|s| s.built) {
            let link = (data.line_demand(line), state.fare_level);
            links.entry(&line.from).or_default().push(link);
            links.entry(&line.to).or_default().push(link);
        }
    }
    let hour = world.time_of_day().0;
    let mut station_caps: Vec<(&str, f64)> = Vec::with_capacity(links.len());
    for country in &data.countries {
        for station in &country.stations {
            let Some(links) = links.get(station.id.as_str()) else {
                continue;
            };
            let station_cap = capacity_for(data, era, links);
            station_caps.push((&station.id, station_cap));
            let arrivals: Vec<f64> = data
                .traffic
                .iter()
                .map(|class| {
                    let mood = world.satisfaction_of(&class.id);
                    demand_from(era, &m, country, class, links, hour, mood) * h / 3600.0
                })
                .collect();
            let Some(state) = world.stations.get_mut(&station.id) else {
                continue;
            };
            let room = (station_cap - state.total()).max(0.0);
            let wanted: f64 = arrivals.iter().sum();
            let scale = if wanted > room { room / wanted } else { 1.0 };
            for (class, add) in data.traffic.iter().zip(arrivals) {
                match state.waiting.get_mut(&class.id) {
                    Some(w) => *w += add * scale,
                    None => {
                        state.waiting.insert(class.id.clone(), add * scale);
                    }
                }
            }
        }
    }

    // Recherche en cours.
    if let Some(active) = &mut world.research_active {
        active.progress_s += h * m.research_speed;
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
    let crowd_factor = roles(world, data)
        .into_iter()
        .find_map(|role| match role {
            StaffRole::AutoTimetable { crowd_factor } => Some(crowd_factor),
            _ => None,
        })
        .unwrap_or(0.0);
    let clock = world.clock;
    // Combustibles épuisés : les trains qui en dépendent ne partent plus.
    let empty: Vec<String> = data
        .fuels
        .iter()
        .filter(|f| world.is_out_of_fuel(data, &f.id))
        .map(|f| f.id.clone())
        .collect();
    let mut net = 0.0;
    let mut gross = 0.0;
    let mut carried = 0u64;
    let mut trips = 0u64;
    for train in &mut world.trains {
        let (Some(loco), Some(line)) = (data.locomotive(&train.loco), data.line(&train.line))
        else {
            continue;
        };
        let dwell_s = world
            .lines
            .get(&line.id)
            .map_or(b.default_dwell_min, |l| l.dwell_min)
            * 60.0;
        match train.state {
            TrainState::AtStation { waited } => {
                let waited = waited + h;
                train.state = TrainState::AtStation { waited };
                let capacity = m.capacity_of(loco);
                let station_id = train.dir.origin(line);
                let mut count = train.load.values().sum::<u32>();
                let mut station_fill = 0.0;
                if let Some(station) = world.stations.get_mut(station_id) {
                    // Les grandes rames ont plus de portes : elles se remplissent aussi vite.
                    let rate = b
                        .boarding_per_min
                        .max(capacity as f64 / b.boarding_full_min)
                        * (1.0 + crew_fx.boarding_speed * train.crew.boarding());
                    train.board_acc += rate * h / 60.0;
                    let allowed = (train.board_acc + 1e-9).floor().max(0.0) as u32;
                    let mut boarded = 0;
                    while boarded < allowed && count < capacity {
                        // Un voyageur à la fois, pris dans la file la plus longue.
                        let Some((id, w)) = station
                            .waiting
                            .iter_mut()
                            .filter(|(_, w)| **w >= 1.0)
                            .max_by(|a, b| a.1.total_cmp(b.1))
                        else {
                            break;
                        };
                        *w -= 1.0;
                        match train.load.get_mut(id) {
                            Some(n) => *n += 1,
                            None => {
                                train.load.insert(id.clone(), 1);
                            }
                        }
                        boarded += 1;
                        count += 1;
                    }
                    // Pas de réserve d'embarquement accumulée pendant qu'on attend des voyageurs.
                    train.board_acc = (train.board_acc - boarded as f64).clamp(0.0, 1.0);
                    let station_cap = station_caps
                        .iter()
                        .find(|(id, _)| *id == station_id)
                        .map_or(b.station_waiting_cap, |(_, cap)| *cap);
                    station_fill = station.total() / station_cap;
                }
                if m.auto_dispatch
                    && train.crew.has_driver
                    && !empty.contains(&loco.fuel)
                    && train.order.is_none()
                    && waited >= water_wait_s(era, &m, loco)
                {
                    let due = clock >= train.sched_departure;
                    if count >= capacity || (due && count > 0) {
                        if let Some(trip) =
                            plan_trip(data, &m, train, world.lines.get(&line.id), clock)
                        {
                            train.depart(clock, capacity, station_fill, trip);
                            events.push(SimEvent::Departed {
                                train: train.id,
                                station: station_id.to_string(),
                            });
                        }
                    } else if due {
                        // Personne à bord : le départ est reporté au créneau suivant.
                        train.sched_departure += dwell_s.max(60.0);
                    }
                }
            }
            TrainState::Moving => {
                let trip = match train.trip {
                    Some(trip) => trip,
                    None => {
                        match plan_trip(data, &m, train, world.lines.get(&line.id), clock - h) {
                            Some(trip) => {
                                train.trip = Some(trip);
                                trip
                            }
                            None => continue,
                        }
                    }
                };
                let elapsed = clock - trip.started;
                let (done_km, _) = trip.profile.at(elapsed);
                let position = match train.dir {
                    Dir::Outbound => trip.origin_km + done_km,
                    Dir::Inbound => trip.origin_km - done_km,
                }
                .clamp(0.0, line.length_km);
                let travelled = (position - train.pos_km).abs();
                train.pos_km = position;
                driven.push((train.id, travelled));

                // Entretien et énergie.
                net -= loco.upkeep_per_h * era.cost_mult * h / 3600.0;
                if let Some(fuel) = data.fuel(&loco.fuel) {
                    let need = travelled
                        * loco.fuel_per_km
                        * m.fuel
                        * (1.0 - crew_fx.economy_fuel * train.crew.economy());
                    let price = fuel.price(clock, era);
                    if fuel.stock && era.rules.fuel_market {
                        let stock = world.fuel.entry(fuel.id.clone()).or_insert(0.0);
                        let used = need.min(*stock);
                        if used > 0.0 && used >= *stock {
                            events.push(SimEvent::FuelOut {
                                fuel: fuel.id.clone(),
                            });
                        }
                        *stock -= used;
                        // Stock vide en route : le train finit son trajet sur une
                        // livraison d'urgence, au prix fort, puis reste à quai.
                        net -= (need - used) * price * b.fuel_emergency_mult;
                    } else {
                        net -= need * price;
                    }
                }

                let fare_level = world.lines.get(&line.id).map_or(1.0, |l| l.fare_level);
                if let Some(state) = world.lines.get_mut(&line.id) {
                    let before = state.condition;
                    let upgrades = LineProps::of(state, data).wear;
                    let wear = travelled
                        * loco.track_wear
                        * m.wear
                        * upgrades
                        * b.wear_per_train_km
                        * (1.0 - crew_fx.care_wear * train.crew.care());
                    state.condition = (before - wear).max(0.0);
                    if before >= b.wear_slow_threshold && state.condition < b.wear_slow_threshold {
                        events.push(SimEvent::TrackWorn {
                            line: line.id.clone(),
                        });
                    }
                }

                if elapsed >= trip.profile.duration_s() {
                    // Retard : celui pris au départ, plus le temps perdu sur une voie usée.
                    let delay_s =
                        train.late_s + (trip.profile.duration_s() - trip.nominal_s).max(0.0);
                    let punctuality =
                        (1.0 - delay_s / 60.0 / b.delay_tolerance_min).clamp(0.0, 1.0);

                    // Le contrôleur joue sur le confort ressenti et sur la fraude.
                    let comfort =
                        (m.comfort + crew_fx.service_comfort * train.crew.service()).min(1.0);
                    let collected = 1.0 + crew_fx.control_revenue * train.crew.control();
                    let mut revenue = 0.0;
                    let mut passengers = 0;
                    for (class_id, &n) in &train.load {
                        let Some(class) = data.class(class_id) else {
                            continue;
                        };
                        passengers += n;
                        revenue += n as f64
                            * b.fare_per_km
                            * line.length_km
                            * era.fare_mult
                            * m.fare
                            * class.fare_mult
                            * fare_level
                            * collected;
                        if let (Some(score), Some(mood)) = (
                            trip_score(class, comfort, punctuality, train.crowding),
                            world.satisfaction.get_mut(class_id),
                        ) {
                            let weight = 1.0 - (-(n as f64) / b.satisfaction_inertia).exp();
                            *mood = (*mood + (score - *mood) * weight).clamp(0.0, 1.0);
                        }
                    }
                    net += revenue;
                    gross += revenue;
                    carried += passengers as u64;
                    trips += 1;
                    events.push(SimEvent::Arrived {
                        train: train.id,
                        station: train.dir.destination(line).to_string(),
                        passengers,
                        revenue,
                        delay_min: delay_s / 60.0,
                    });
                    train.pos_km = match train.dir {
                        Dir::Outbound => line.length_km,
                        Dir::Inbound => 0.0,
                    };
                    train.dir = train.dir.flipped();
                    train.load.clear();
                    train.board_acc = 0.0;
                    // Le bureau des horaires raccourcit l'arrêt quand les quais sont pleins.
                    let arrival = train.dir.origin(line);
                    let cap = station_caps
                        .iter()
                        .find(|(id, _)| *id == arrival)
                        .map(|(_, cap)| *cap);
                    let fill = match (world.stations.get(arrival), cap) {
                        (Some(station), Some(cap)) => (station.total() / cap).clamp(0.0, 1.0),
                        _ => 0.0,
                    };
                    let shorter = (1.0 - crowd_factor * fill).max(0.0);
                    let stop_s = (dwell_s * shorter).max(b.dwell_min_min * 60.0).min(dwell_s);
                    train.sched_departure = clock + stop_s;
                    train.late_s = 0.0;
                    train.trip = None;
                    train.state = TrainState::AtStation { waited: 0.0 };
                }
            }
        }
    }

    // Ventes et remplacements prévus : exécutés dès l'arrivée à quai.
    let ordered: Vec<u32> = world
        .trains
        .iter()
        .filter(|t| t.order.is_some() && t.is_docked())
        .map(|t| t.id)
        .collect();
    for train in ordered {
        match world.execute_order(data, train) {
            Some((loco, None, refund)) => events.push(SimEvent::TrainSold {
                train,
                loco,
                refund,
            }),
            Some((old, Some(new), _)) => events.push(SimEvent::TrainReplaced { train, old, new }),
            None => {}
        }
    }

    // Les équipages apprennent en roulant, et sont payés à l'heure.
    for (train, km) in driven {
        world.train_crew_experience(&data.crew, train, km);
    }
    net -= world.crew_wages_per_hour(data) * h / 3600.0;

    // Salaires du personnel engagé.
    let salaries: f64 = world
        .staff
        .iter()
        .filter_map(|id| data.staff(id))
        .map(|s| s.salary_per_h)
        .sum();
    net -= salaries * era.cost_mult * h / 3600.0;

    world.money += net;
    world.lifetime_income += gross;
    world.passengers_carried += carried;
    world.trips += trips;
    world.income_recent = world.income_recent * (-h / INCOME_TAU).exp() + net;

    automate(world, data, h, events);

    // Un événement historique à la fois attend la décision du joueur.
    if world.event_pending.is_none()
        && let Some(event) = data
            .events
            .iter()
            .find(|e| !world.events_done.contains(&e.id) && event_ready(world, data, e))
    {
        world.event_pending = Some(event.id.clone());
        events.push(SimEvent::EventReady {
            id: event.id.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "stockton_darlington";

    fn setup() -> (World, GameData) {
        let mut data = GameData::embedded().unwrap();
        data.events.clear();
        // Le conducteur d'essai, sans compétence, ne coûte rien : les comptes restent ronds.
        data.crew.base_wage = 0.0;
        let mut world = World::new(&data);
        world.test_driver(1);
        (world, data)
    }

    /// Durée du trajet complet de la Locomotion n°1 sur voie neuve, sans recherche.
    fn locomotion_trip_s(data: &GameData) -> f64 {
        let m = Modifiers::compute(&Default::default(), data);
        let loco = data.locomotive("locomotion_1").unwrap();
        Profile::new(19.0, 24.0, m.accel_of(loco, &data.balance), 0.12).duration_s()
    }

    fn enter_era(world: &mut World, era: &str) {
        world.era = era.to_string();
        world.era_started = world.clock;
    }

    /// Données sans variation horaire ni nationale : une seule catégorie neutre,
    /// et aucun événement pour interrompre la partie.
    fn flat_data() -> GameData {
        let mut data = GameData::embedded().unwrap();
        data.events.clear();
        data.crew.base_wage = 0.0;
        data.traffic.truncate(1);
        let class = &mut data.traffic[0];
        class.share = 1.0;
        class.fare_mult = 1.0;
        class.hourly = vec![1.0; 24];
        data.countries[0].demand.clear();
        data
    }

    fn flat() -> (World, GameData) {
        let data = flat_data();
        let mut world = World::new(&data);
        world.test_driver(1);
        (world, data)
    }

    fn fill(world: &mut World, station: &str, class: &str, n: f64) {
        world
            .stations
            .get_mut(station)
            .unwrap()
            .waiting
            .insert(class.into(), n);
    }

    fn total_waiting(world: &World) -> f64 {
        world.stations.values().map(|s| s.total()).sum()
    }

    #[test]
    fn passengers_accumulate_up_to_the_station_cap() {
        let (mut world, data) = flat();
        world.trains.clear();
        advance(&mut world, &data, 3600.0);
        // Demande de la ligne : √(6000 × 6500) / 1000 × 8 voyageurs par heure.
        assert!((world.stations["stockton"].total() - 49.96).abs() < 0.01);
        advance(&mut world, &data, 100.0 * 3600.0);
        assert!(
            (world.stations["stockton"].total() - data.balance.station_waiting_cap).abs() < 1e-6
        );
    }

    #[test]
    fn only_served_stations_get_passengers_and_hubs_get_more() {
        let (mut world, data) = flat();
        advance(&mut world, &data, 3600.0);
        assert_eq!(
            world.stations["york"].total(),
            0.0,
            "York n'est pas encore desservie"
        );
        let before = world.stations["darlington"].total();

        world.lines.get_mut("darlington_york").unwrap().built = true;
        let mut second = world.clone();
        advance(&mut second, &data, 3600.0);
        assert!(second.stations["york"].total() > 0.0);
        let gained = second.stations["darlington"].total() - before;
        assert!(
            gained > before * 1.2,
            "deux lignes à Darlington : {gained} contre {before}"
        );
        assert!(
            station_capacity(&second, &data, "darlington")
                > station_capacity(&world, &data, "stockton") - 1e-9
        );
    }

    #[test]
    fn long_lines_offer_fewer_but_longer_trips() {
        let data = GameData::embedded().unwrap();
        let demand = |id: &str| data.line_demand(data.line(id).unwrap());
        let km = |id: &str| data.line(id).unwrap().length_km;
        // Moins de voyageurs sur une longue ligne…
        assert!(demand("york_london") < demand("york_leeds"));
        // … mais les grandes villes offrent bien plus de voyageurs-kilomètres.
        assert!(
            demand("york_london") * km("york_london") > 2.0 * demand("stockton_darlington") * 19.0
        );
        assert!(
            demand("london_paris") * km("london_paris") > demand("york_london") * km("york_london")
        );
    }

    #[test]
    fn commuters_come_at_rush_hour_and_freight_all_night() {
        let (mut world, data) = setup();
        let m = world.modifiers(&data);
        let station = data.station("stockton").unwrap();
        let rate = |world: &World, class: &str| {
            demand_per_hour(world, &data, &m, station, data.class(class).unwrap())
        };

        world.clock = 8.0 * 3600.0;
        let (rush, freight_day) = (rate(&world, "commuter"), rate(&world, "freight"));
        world.clock = 3.0 * 3600.0;
        let (night, freight_night) = (rate(&world, "commuter"), rate(&world, "freight"));
        assert!(rush > 20.0 * night, "pointe {rush}, nuit {night}");
        assert_eq!(freight_day, freight_night);
        // Le pays pèse sur le fret : demande de la ligne × 0,20 de part × 1,3.
        let line = data.line_demand(data.line(LINE).unwrap());
        assert!(
            (line - 49.96).abs() < 0.01,
            "√(6000 × 6500) / 1000 × 8 : {line}"
        );
        assert!((freight_day - line * 0.20 * 1.3).abs() < 1e-9);
    }

    #[test]
    fn price_and_satisfaction_shift_demand() {
        let (mut world, data) = setup();
        let m = world.modifiers(&data);
        let station = data.station("stockton").unwrap();
        let rate = |world: &World, class: &str| {
            demand_per_hour(world, &data, &m, station, data.class(class).unwrap())
        };
        let base = (rate(&world, "commuter"), rate(&world, "business"));

        world.set_fare_level(&data, LINE, 1.5).unwrap();
        let dear = (rate(&world, "commuter"), rate(&world, "business"));
        assert!(dear.0 < base.0 && dear.1 < base.1);
        assert!(
            dear.0 / base.0 < dear.1 / base.1,
            "les navetteurs sont les plus sensibles au prix"
        );

        world.set_fare_level(&data, LINE, 1.0).unwrap();
        world.satisfaction.insert("commuter".into(), 1.0);
        assert!((rate(&world, "commuter") / base.0 - 1.5).abs() < 1e-9);
        world.satisfaction.insert("commuter".into(), 0.0);
        assert!((rate(&world, "commuter") / base.0 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn docked_train_boards_waiting_passengers_up_to_capacity() {
        let (mut world, data) = flat();
        fill(&mut world, "stockton", "commuter", 100.0);
        advance(&mut world, &data, 5.0 * 60.0);
        assert_eq!(
            world.trains[0].count(),
            20,
            "4 voyageurs par minute pendant 5 minutes"
        );
        advance(&mut world, &data, 3600.0);
        assert_eq!(world.trains[0].count(), 40, "plafonné à la capacité");
        assert!(
            world.trains[0].is_docked(),
            "pas de départ automatique sans recherche"
        );
    }

    #[test]
    fn boarding_draws_from_every_queue() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        fill(&mut world, "stockton", "commuter", 30.0);
        fill(&mut world, "stockton", "business", 10.0);
        fill(&mut world, "stockton", "freight", 20.0);
        advance(&mut world, &data, 3600.0);
        let load = &world.trains[0].load;
        assert_eq!(world.trains[0].count(), 40);
        assert!(load["commuter"] > load["business"]);
        assert!(load["business"] >= 1 && load["freight"] >= 1, "{load:?}");
        assert!((total_waiting(&world) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn trip_follows_its_speed_profile_and_pays_the_fare() {
        let (mut world, data) = flat();
        fill(&mut world, "stockton", "commuter", 100.0);
        advance(&mut world, &data, 600.0);
        assert_eq!(world.trains[0].count(), 40);
        let money_before = world.money;
        world.dispatch(&data, 1).unwrap();

        // 19 km à 24 km/h feraient 2850 s ; démarrage et freinage en ajoutent quelques-unes.
        let trip_s = locomotion_trip_s(&data);
        assert!(trip_s > 2850.0 && trip_s < 3300.0, "{trip_s}");
        let events = advance(&mut world, &data, trip_s - 15.0);
        assert!(events.is_empty());
        assert_eq!(world.trains[0].state, TrainState::Moving);
        assert!(
            world.trains[0].speed_kmh(world.clock) < 10.0,
            "le train freine avant la gare"
        );
        let events = advance(&mut world, &data, 30.0);
        let revenue = 40.0 * 0.10 * 19.0;
        match &events[..] {
            [
                SimEvent::Arrived {
                    train: 1,
                    station,
                    passengers: 40,
                    revenue: r,
                    delay_min,
                },
            ] => {
                assert_eq!(station, "darlington");
                assert!((r - revenue).abs() < 1e-9);
                assert_eq!(*delay_min, 0.0, "parti à l'heure sur voie neuve");
            }
            other => panic!("événements inattendus : {other:?}"),
        }
        let train = &world.trains[0];
        assert!(train.is_docked() && train.trip.is_none());
        assert_eq!(train.dir, Dir::Inbound);
        assert_eq!(train.pos_km, 19.0);
        assert_eq!(train.count(), 0);
        assert_eq!((world.passengers_carried, world.trips), (40, 1));
        // Prochain départ prévu 20 minutes après l'arrivée.
        assert!((train.sched_departure - (world.clock + 1200.0)).abs() <= 30.0);

        // Recette, moins l'entretien pendant la marche et le charbon : 12 kg par km à 12 € la tonne.
        let running_s = (trip_s / MAX_STEP).ceil() * MAX_STEP;
        let costs = 2.0 * running_s / 3600.0 + 19.0 * 0.012 * 12.0;
        assert!(
            (world.money - (money_before + revenue - costs)).abs() < 0.01,
            "{}",
            world.money
        );
        assert!(world.income_per_game_second() > 0.0);
    }

    #[test]
    fn a_train_starts_and_stops_gently() {
        let (mut world, data) = setup();
        world.dispatch(&data, 1).unwrap();
        advance(&mut world, &data, 60.0);
        let early = world.trains[0].speed_kmh(world.clock);
        assert!(
            early > 0.0 && early < 24.0,
            "encore en accélération : {early}"
        );
        advance(&mut world, &data, 1200.0);
        assert_eq!(world.trains[0].speed_kmh(world.clock), 24.0);
    }

    #[test]
    fn speed_is_capped_by_signalling_and_by_the_track() {
        let (mut world, data) = setup();
        let crampton = data.locomotive("crampton").unwrap();
        let speed = |world: &World| {
            let m = world.modifiers(&data);
            allowed_speed(&data, &m, crampton, world.lines.get(LINE))
        };
        assert_eq!(speed(&world), 50.0, "sans signalisation, on roule à vue");
        world
            .research_done
            .extend(["hand_signals", "electric_telegraph"].map(String::from));
        assert_eq!(
            speed(&world),
            80.0,
            "le télégraphe autorise 100, la voie d'origine 80"
        );
        world
            .lines
            .get_mut(LINE)
            .unwrap()
            .upgrades
            .insert("steel_rails".into());
        assert_eq!(speed(&world), 100.0);
        world.research_done.insert("semaphore_block".into());
        assert_eq!(
            speed(&world),
            120.0,
            "la machine elle-même ne dépasse pas 120"
        );
    }

    #[test]
    fn better_brakes_shorten_the_trip() {
        let run = |research: &[&str]| {
            let (mut world, data) = setup();
            world
                .research_done
                .extend(research.iter().map(|r| r.to_string()));
            world.dispatch(&data, 1).unwrap();
            world.trains[0].trip.unwrap().profile.duration_s()
        };
        assert!(run(&["hand_brakes"]) < run(&[]));
        assert!(run(&["hand_brakes", "air_brake"]) < run(&["hand_brakes"]));
    }

    #[test]
    fn era_scales_demand_fares_and_costs() {
        let (mut world, data) = flat();
        let m = world.modifiers(&data);
        let station = data.station("stockton").unwrap();
        let before = demand_per_hour(&world, &data, &m, station, &data.traffic[0]);
        enter_era(&mut world, "steam");
        world.research_done.insert("water_troughs".into());
        let after = demand_per_hour(&world, &data, &m, station, &data.traffic[0]);
        assert!((after / before - 3.0).abs() < 1e-9);

        world.fuel.insert("coal".into(), 100.0);
        world.trains[0].load.insert("commuter".into(), 10);
        world.dispatch(&data, 1).unwrap();
        let events = advance(&mut world, &data, 4000.0);
        match &events[..] {
            [SimEvent::Arrived { revenue, .. }] => {
                assert!((revenue - 10.0 * 0.10 * 19.0 * 1.5).abs() < 1e-9)
            }
            other => panic!("événements inattendus : {other:?}"),
        }
    }

    #[test]
    fn coal_is_drawn_from_stock_then_bought_at_a_premium() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        enter_era(&mut world, "steam");
        world.research_done.insert("water_troughs".into());
        // 19 km à 12 kg par km : 0,228 t pour le trajet.
        world.fuel.insert("coal".into(), 0.128);
        world.dispatch(&data, 1).unwrap();
        let money = world.money;
        let events = advance(&mut world, &data, 4000.0);
        let out = events
            .iter()
            .filter(|e| matches!(e, SimEvent::FuelOut { .. }))
            .count();
        assert_eq!(out, 1, "la pénurie n'est signalée qu'une fois");
        assert!(world.fuel_stock("coal").abs() < 1e-12);

        // Les 0,1 t manquantes sont payées au cours du jour majoré de 60 %.
        let coal = data.fuel("coal").unwrap();
        let era = world.era(&data);
        let spent = money
            - world.money
            - 2.0
                * era.cost_mult
                * locomotion_trip_s(&data)
                    .div_euclid(10.0)
                    .mul_add(10.0, 10.0)
                / 3600.0;
        let (low, high) = (
            0.1 * coal.base_price * era.cost_mult * 0.7 * 1.6,
            0.1 * coal.base_price * era.cost_mult * 1.3 * 1.6,
        );
        assert!(
            spent > low - 0.01 && spent < high + 0.01,
            "{spent} hors de {low}..{high}"
        );

        // Avec du stock, le trajet retour ne coûte que l'entretien.
        world.fuel.insert("coal".into(), 5.0);
        world.dispatch(&data, 1).unwrap();
        advance(&mut world, &data, 4000.0);
        // (à peine moins : le conducteur a appris l'économie au premier trajet)
        assert!((world.fuel_stock("coal") - (5.0 - 0.228)).abs() < 1e-4);
    }

    #[test]
    fn trains_stay_at_the_station_when_their_fuel_runs_out() {
        let (mut world, data) = setup();
        enter_era(&mut world, "steam");
        world
            .research_done
            .extend(["hand_signals", "water_troughs"].map(String::from));
        world.fuel.insert("coal".into(), 0.0);
        world.trains[0].load.insert("commuter".into(), 40);
        world.money = 10_000.0;

        // Ni départ manuel, ni départ automatique : le train attend à quai.
        assert_eq!(
            world.dispatch(&data, 1),
            Err(crate::sim::ActionError::NoFuel("charbon".into()))
        );
        let events = advance(&mut world, &data, 3.0 * 3600.0);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, SimEvent::Departed { .. })),
            "{events:?}"
        );
        assert!(world.trains[0].is_docked());

        // Un lot de charbon et il repart.
        world.buy_fuel(&data, "coal", 1).unwrap();
        let events = advance(&mut world, &data, 60.0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::Departed { .. })),
            "{events:?}"
        );
    }

    #[test]
    fn steam_engines_take_water_before_leaving() {
        let (mut world, data) = flat();
        enter_era(&mut world, "steam");
        world.research_done.insert("hand_signals".into());
        world.fuel.insert("coal".into(), 100.0);
        world.trains[0].load.insert("commuter".into(), 40);
        // Plein dès l'arrivée, mais la prise d'eau dure 8 minutes.
        let events = advance(&mut world, &data, 7.0 * 60.0);
        assert!(events.is_empty());
        let events = advance(&mut world, &data, 2.0 * 60.0);
        assert_eq!(
            events,
            vec![SimEvent::Departed {
                train: 1,
                station: "stockton".into()
            }]
        );

        let (mut world, data) = flat();
        enter_era(&mut world, "steam");
        world
            .research_done
            .extend(["hand_signals", "water_troughs"].map(String::from));
        world.fuel.insert("coal".into(), 100.0);
        world.trains[0].load.insert("commuter".into(), 40);
        let events = advance(&mut world, &data, 20.0);
        assert_eq!(
            events.len(),
            1,
            "avec la prise d'eau en marche, plus d'attente"
        );
    }

    #[test]
    fn historical_event_waits_for_the_player() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        assert!(advance(&mut world, &data, 600.0).is_empty());
        world.passengers_carried = 250;
        let events = advance(&mut world, &data, 10.0);
        assert_eq!(
            events,
            vec![SimEvent::EventReady {
                id: "rainhill_trials".into()
            }]
        );
        assert_eq!(world.event_pending.as_deref(), Some("rainhill_trials"));
        // Tant qu'il n'est pas résolu, aucun autre événement ne s'annonce.
        assert!(advance(&mut world, &data, 600.0).is_empty());
    }

    #[test]
    fn each_class_pays_its_own_fare_and_price_level_scales_revenue() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        world.set_fare_level(&data, LINE, 1.5).unwrap();
        world.trains[0].load = [("commuter".to_string(), 10), ("business".to_string(), 5)].into();
        world.dispatch(&data, 1).unwrap();
        let events = advance(&mut world, &data, 4000.0);
        let expected = (10.0 * 0.6 + 5.0 * 2.0) * 0.10 * 19.0 * 1.5;
        match &events[..] {
            [
                SimEvent::Arrived {
                    revenue,
                    passengers: 15,
                    ..
                },
            ] => assert!((revenue - expected).abs() < 1e-9),
            other => panic!("événements inattendus : {other:?}"),
        }
    }

    #[test]
    fn late_departures_hurt_punctuality_sensitive_passengers_most() {
        let run = |late_min: f64| {
            let (mut world, mut data) = setup();
            data.balance.demand_per_1000_per_h = 0.0;
            world.trains[0].load =
                [("business".to_string(), 20), ("tourist".to_string(), 20)].into();
            world.clock = world.trains[0].sched_departure + late_min * 60.0;
            world.dispatch(&data, 1).unwrap();
            let events = advance(&mut world, &data, 4000.0);
            let delay = match &events[..] {
                [SimEvent::Arrived { delay_min, .. }] => *delay_min,
                other => panic!("événements inattendus : {other:?}"),
            };
            (
                world.satisfaction_of("business"),
                world.satisfaction_of("tourist"),
                delay,
            )
        };
        let (business_ok, tourist_ok, delay_ok) = run(0.0);
        let (business_late, tourist_late, delay_late) = run(40.0);
        assert!(delay_ok == 0.0 && (delay_late - 40.0).abs() < 1e-9);
        assert!(business_late < business_ok && tourist_late < tourist_ok);
        assert!(business_ok - business_late > 2.0 * (tourist_ok - tourist_late));
        assert!(
            business_late < 0.5,
            "un gros retard déçoit : {business_late}"
        );
    }

    #[test]
    fn comfort_research_pleases_tourists() {
        let run = |comfortable: bool| {
            let (mut world, mut data) = setup();
            data.balance.demand_per_1000_per_h = 0.0;
            if comfortable {
                world.research_done.insert("covered_carriages".into());
            }
            world.trains[0].load = [("tourist".to_string(), 20)].into();
            world.dispatch(&data, 1).unwrap();
            advance(&mut world, &data, 4000.0);
            world.satisfaction_of("tourist")
        };
        assert!(run(true) > run(false));
    }

    #[test]
    fn trip_score_ignores_what_a_class_does_not_care_about() {
        let data = GameData::embedded().unwrap();
        let freight = data.class("freight").unwrap();
        assert_eq!(trip_score(freight, 0.0, 0.8, 0.0), Some(0.8));
        let mut indifferent = freight.clone();
        indifferent.punctuality_weight = 0.0;
        assert_eq!(trip_score(&indifferent, 1.0, 1.0, 1.0), None);
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
    fn big_trainsets_fill_up_as_fast_as_small_ones() {
        let (mut world, data) = flat();
        world.trains[0].loco = "shinkansen_0".into();
        fill(&mut world, "stockton", "commuter", 5000.0);
        advance(&mut world, &data, 10.0 * 60.0);
        assert!(
            world.trains[0].count() >= 980,
            "{}",
            world.trains[0].count()
        );
    }

    #[test]
    fn auto_dispatch_follows_the_timetable() {
        let (mut world, data) = flat();
        world.research_done.insert("hand_signals".into());
        fill(&mut world, "stockton", "commuter", 10.0);
        // Pas plein : le train attend l'heure prévue, 20 minutes après sa mise à quai.
        let events = advance(&mut world, &data, 19.0 * 60.0);
        assert!(events.is_empty());
        let events = advance(&mut world, &data, 2.0 * 60.0);
        assert_eq!(
            events,
            vec![SimEvent::Departed {
                train: 1,
                station: "stockton".into()
            }]
        );
        assert!(world.trains[0].late_s <= MAX_STEP);
        assert!(world.trains[0].trip.is_some());
    }

    #[test]
    fn auto_dispatch_leaves_early_when_full_and_runs_all_day() {
        let (mut world, data) = flat();
        world.research_done.insert("hand_signals".into());
        fill(&mut world, "stockton", "commuter", 100.0);
        let events = advance(&mut world, &data, 11.0 * 60.0);
        assert_eq!(
            events,
            vec![SimEvent::Departed {
                train: 1,
                station: "stockton".into()
            }]
        );

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
        assert!(
            world.overall_satisfaction(&data) > 0.5,
            "des trains à l'heure doivent plaire"
        );
    }

    #[test]
    fn auto_dispatch_never_sends_an_empty_train() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        world.research_done.insert("hand_signals".into());
        let events = advance(&mut world, &data, 12.0 * 3600.0);
        assert!(events.is_empty());
        assert!(world.trains[0].is_docked());
        // Le départ a été reporté de créneau en créneau, il n'est pas « en retard ».
        assert!(world.trains[0].sched_departure > world.clock);
    }

    #[test]
    fn shorter_dwell_means_more_departures() {
        let run = |dwell: f64| {
            let (mut world, data) = flat();
            world.research_done.insert("hand_signals".into());
            world.set_dwell(&data, LINE, dwell).unwrap();
            world.trains[0].sched_departure = world.clock + dwell * 60.0;
            advance(&mut world, &data, 24.0 * 3600.0);
            world.trips
        };
        assert!(run(5.0) > run(60.0));
    }

    #[test]
    fn worn_track_slows_trains_down_and_makes_them_late() {
        let (mut fresh, data) = setup();
        let mut worn = fresh.clone();
        worn.lines.get_mut(LINE).unwrap().condition = 0.0;
        fresh.dispatch(&data, 1).unwrap();
        worn.dispatch(&data, 1).unwrap();
        assert_eq!(fresh.trains[0].trip.unwrap().profile.peak_kmh, 24.0);
        assert_eq!(worn.trains[0].trip.unwrap().profile.peak_kmh, 12.0);
        advance(&mut fresh, &data, 1500.0);
        advance(&mut worn, &data, 1500.0);
        assert!(fresh.trains[0].pos_km > 1.8 * worn.trains[0].pos_km);
        assert_eq!(condition_factor(0.7, &data.balance), 1.0);
        assert!(condition_factor(0.35, &data.balance) < 1.0);

        let events = advance(&mut worn, &data, 8000.0);
        match &events[..] {
            [SimEvent::Arrived { delay_min, .. }] => {
                assert!((40.0..50.0).contains(delay_min), "{delay_min}")
            }
            other => panic!("événements inattendus : {other:?}"),
        }
    }

    #[test]
    fn running_trains_wear_the_track_and_report_it_once() {
        let (mut world, data) = flat();
        world.research_done.insert("hand_signals".into());
        world.lines.get_mut(LINE).unwrap().condition = 0.7005;
        let events = advance(&mut world, &data, 6.0 * 3600.0);
        let worn = events
            .iter()
            .filter(|e| matches!(e, SimEvent::TrackWorn { .. }))
            .count();
        assert_eq!(worn, 1);
        assert!(world.lines[LINE].condition < 0.7);
    }

    #[test]
    fn heavier_coaches_earn_more_but_start_slower() {
        let (mut world, data) = flat();
        let light = locomotion_trip_s(&data);
        world.research_done.insert("covered_carriages".into());
        fill(&mut world, "stockton", "commuter", 100.0);
        advance(&mut world, &data, 600.0);
        world.dispatch(&data, 1).unwrap();
        assert!(world.trains[0].trip.unwrap().profile.duration_s() > light);
        let events = advance(&mut world, &data, 4000.0);
        match &events[0] {
            SimEvent::Arrived {
                revenue, delay_min, ..
            } => {
                assert!((revenue - 40.0 * 0.10 * 19.0 * 1.2).abs() < 1e-9);
                assert_eq!(
                    *delay_min, 0.0,
                    "le temps de parcours nominal suit la recherche"
                );
            }
            other => panic!("événement inattendu : {other:?}"),
        }
    }

    #[test]
    fn simulation_is_deterministic_and_independent_of_frame_slicing() {
        let (mut a, data) = setup();
        a.research_done.insert("hand_signals".into());
        let mut b = a.clone();
        advance(&mut a, &data, 50_000.0);
        for _ in 0..5000 {
            advance(&mut b, &data, 10.0);
        }
        assert_eq!(a, b);
    }

    #[test]
    fn coarse_steps_stay_close_to_fine_ones() {
        let (mut fine, data) = setup();
        fine.research_done.insert("hand_signals".into());
        let mut coarse = fine.clone();
        advance(&mut fine, &data, 48.0 * 3600.0);
        advance_with_step(&mut coarse, &data, 48.0 * 3600.0, 60.0);
        let gap = (fine.lifetime_income - coarse.lifetime_income).abs() / fine.lifetime_income;
        assert!(gap < 0.08, "écart de recettes de {:.1} %", gap * 100.0);
    }

    #[test]
    fn passengers_are_conserved() {
        let (mut world, mut data) = setup();
        data.balance.demand_per_1000_per_h = 0.0;
        world.research_done.insert("hand_signals".into());
        fill(&mut world, "stockton", "commuter", 60.0);
        fill(&mut world, "stockton", "tourist", 30.0);
        fill(&mut world, "darlington", "freight", 60.0);
        advance(&mut world, &data, 48.0 * 3600.0);
        let on_board: u32 = world.trains.iter().map(|t| t.count()).sum();
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
