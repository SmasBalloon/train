use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::data::{GameData, Line};

pub const SAVE_VERSION: u32 = 1;

/// Constante de temps (secondes de jeu) de la moyenne glissante des revenus.
pub(crate) const INCOME_TAU: f64 = 2.0 * 3600.0;

/// Sens de circulation sur une ligne : `Outbound` va de `from` vers `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    Outbound,
    Inbound,
}

impl Dir {
    pub fn flipped(self) -> Dir {
        match self {
            Dir::Outbound => Dir::Inbound,
            Dir::Inbound => Dir::Outbound,
        }
    }

    /// Gare d'où part (ou partira) un train roulant dans ce sens.
    pub fn origin(self, line: &Line) -> &str {
        match self {
            Dir::Outbound => &line.from,
            Dir::Inbound => &line.to,
        }
    }

    pub fn destination(self, line: &Line) -> &str {
        self.flipped().origin(line)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TrainState {
    /// À quai depuis `waited` secondes de jeu, en cours d'embarquement.
    AtStation {
        waited: f64,
    },
    Moving,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Train {
    pub id: u32,
    pub loco: String,
    pub line: String,
    /// Distance depuis la gare `from` de la ligne.
    pub pos_km: f64,
    pub dir: Dir,
    pub state: TrainState,
    pub passengers: u32,
    /// Fraction de voyageur en cours d'embarquement.
    #[serde(default)]
    pub board_acc: f64,
}

impl Train {
    pub fn is_docked(&self) -> bool {
        matches!(self.state, TrainState::AtStation { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StationState {
    pub waiting: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineState {
    /// État de la voie, de 0 (ruinée) à 1 (neuve).
    pub condition: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveResearch {
    pub id: String,
    pub progress_s: f64,
}

/// État complet d'une partie. Sérialisé tel quel dans la sauvegarde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub version: u32,
    /// Secondes de jeu écoulées depuis le jour 1 à minuit.
    pub clock: f64,
    pub money: f64,
    pub lifetime_income: f64,
    pub passengers_carried: u64,
    pub stations: BTreeMap<String, StationState>,
    pub lines: BTreeMap<String, LineState>,
    pub trains: Vec<Train>,
    pub next_train_id: u32,
    pub research_done: BTreeSet<String>,
    pub research_active: Option<ActiveResearch>,
    /// Revenus nets récents, amortis exponentiellement.
    #[serde(default)]
    pub income_recent: f64,
}

impl World {
    /// Nouvelle partie : une ligne, une locomotive à quai, le matin du jour 1.
    pub fn new(data: &GameData) -> World {
        let mut world = World {
            version: SAVE_VERSION,
            clock: 8.0 * 3600.0,
            money: data.balance.start_money,
            lifetime_income: 0.0,
            passengers_carried: 0,
            stations: BTreeMap::new(),
            lines: BTreeMap::new(),
            trains: Vec::new(),
            next_train_id: 1,
            research_done: BTreeSet::new(),
            research_active: None,
            income_recent: 0.0,
        };
        world.reconcile(data);
        world.spawn_train(&data.balance.start_locomotive, &data.balance.start_line);
        world
    }

    /// Aligne l'état sur les données (utile après un changement de mods) :
    /// ajoute les gares et lignes manquantes, retire ce qui n'existe plus.
    pub fn reconcile(&mut self, data: &GameData) {
        for s in data.stations() {
            self.stations.entry(s.id.clone()).or_default();
        }
        for l in data.lines() {
            self.lines
                .entry(l.id.clone())
                .or_insert(LineState { condition: 1.0 });
        }
        self.trains
            .retain(|t| data.locomotive(&t.loco).is_some() && data.line(&t.line).is_some());
        for t in &mut self.trains {
            if let Some(line) = data.line(&t.line) {
                t.pos_km = t.pos_km.clamp(0.0, line.length_km);
            }
        }
        self.research_done.retain(|id| data.research(id).is_some());
        if self
            .research_active
            .as_ref()
            .is_some_and(|a| data.research(&a.id).is_none())
        {
            self.research_active = None;
        }
    }

    pub(crate) fn spawn_train(&mut self, loco: &str, line: &str) -> u32 {
        let id = self.next_train_id;
        self.next_train_id += 1;
        self.trains.push(Train {
            id,
            loco: loco.to_string(),
            line: line.to_string(),
            pos_km: 0.0,
            dir: Dir::Outbound,
            state: TrainState::AtStation { waited: 0.0 },
            passengers: 0,
            board_acc: 0.0,
        });
        id
    }

    pub fn train(&self, id: u32) -> Option<&Train> {
        self.trains.iter().find(|t| t.id == id)
    }

    pub fn trains_on(&self, line: &str) -> usize {
        self.trains.iter().filter(|t| t.line == line).count()
    }

    pub fn day(&self) -> u64 {
        (self.clock / 86_400.0) as u64 + 1
    }

    /// Heure et minute de la journée en cours.
    pub fn time_of_day(&self) -> (u32, u32) {
        let s = self.clock.rem_euclid(86_400.0) as u32;
        (s / 3600, s % 3600 / 60)
    }

    /// Revenu net moyen récent, en £ par seconde de jeu.
    pub fn income_per_game_second(&self) -> f64 {
        self.income_recent / INCOME_TAU
    }
}
