//! Actions du joueur sur l'économie : achats, départs, recherche, entretien.

use std::fmt;

use crate::data::GameData;

use super::world::{ActiveResearch, TrainState, World};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionError {
    UnknownLocomotive,
    UnknownLine,
    UnknownTrain,
    UnknownResearch,
    /// Nom de la recherche manquante.
    Locked(String),
    NotEnoughMoney {
        needed: f64,
    },
    LineFull,
    NotAtStation,
    AlreadyResearched,
    ResearchBusy,
    NothingToRepair,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::UnknownLocomotive => write!(f, "Locomotive inconnue"),
            ActionError::UnknownLine => write!(f, "Ligne inconnue"),
            ActionError::UnknownTrain => write!(f, "Aucun train sélectionné"),
            ActionError::UnknownResearch => write!(f, "Recherche inconnue"),
            ActionError::Locked(name) => write!(f, "Il faut d'abord rechercher : {name}"),
            ActionError::NotEnoughMoney { needed } => {
                write!(f, "Pas assez d'argent (il faut £ {})", needed.ceil())
            }
            ActionError::LineFull => write!(f, "La ligne ne peut pas accueillir plus de trains"),
            ActionError::NotAtStation => write!(f, "Ce train est déjà en route"),
            ActionError::AlreadyResearched => write!(f, "Recherche déjà terminée"),
            ActionError::ResearchBusy => write!(f, "Une recherche est déjà en cours"),
            ActionError::NothingToRepair => write!(f, "La voie est déjà en parfait état"),
        }
    }
}

impl std::error::Error for ActionError {}

impl World {
    fn pay(&mut self, cost: f64) -> Result<(), ActionError> {
        if self.money < cost {
            return Err(ActionError::NotEnoughMoney { needed: cost });
        }
        self.money -= cost;
        Ok(())
    }

    /// Première recherche manquante parmi `requires`, sous son nom affichable.
    fn missing_requirement(&self, data: &GameData, requires: &[String]) -> Option<String> {
        requires
            .iter()
            .find(|r| !self.research_done.contains(*r))
            .map(|r| {
                data.research(r)
                    .map_or_else(|| r.clone(), |res| res.name.fr.clone())
            })
    }

    /// Achète une locomotive et la met à quai au départ de la ligne.
    pub fn buy_locomotive(
        &mut self,
        data: &GameData,
        loco_id: &str,
        line_id: &str,
    ) -> Result<u32, ActionError> {
        let loco = data
            .locomotive(loco_id)
            .ok_or(ActionError::UnknownLocomotive)?;
        let line = data.line(line_id).ok_or(ActionError::UnknownLine)?;
        if let Some(name) = self.missing_requirement(data, &loco.requires) {
            return Err(ActionError::Locked(name));
        }
        if self.trains_on(line_id) >= line.max_trains as usize {
            return Err(ActionError::LineFull);
        }
        self.pay(loco.cost)?;
        Ok(self.spawn_train(loco_id, line_id))
    }

    /// Donne le départ à un train à quai.
    pub fn dispatch(&mut self, train_id: u32) -> Result<(), ActionError> {
        let train = self
            .trains
            .iter_mut()
            .find(|t| t.id == train_id)
            .ok_or(ActionError::UnknownTrain)?;
        if !train.is_docked() {
            return Err(ActionError::NotAtStation);
        }
        train.state = TrainState::Moving;
        Ok(())
    }

    /// Donne le départ à tous les trains à quai ; renvoie leur nombre.
    pub fn dispatch_all(&mut self) -> usize {
        let mut count = 0;
        for train in self.trains.iter_mut().filter(|t| t.is_docked()) {
            train.state = TrainState::Moving;
            count += 1;
        }
        count
    }

    pub fn start_research(&mut self, data: &GameData, id: &str) -> Result<(), ActionError> {
        let research = data.research(id).ok_or(ActionError::UnknownResearch)?;
        if self.research_done.contains(id) {
            return Err(ActionError::AlreadyResearched);
        }
        if self.research_active.is_some() {
            return Err(ActionError::ResearchBusy);
        }
        if let Some(name) = self.missing_requirement(data, &research.requires) {
            return Err(ActionError::Locked(name));
        }
        self.pay(research.cost)?;
        self.research_active = Some(ActiveResearch {
            id: id.to_string(),
            progress_s: 0.0,
        });
        Ok(())
    }

    /// Coût d'une remise à neuf de la voie, ou `None` si la ligne est inconnue.
    pub fn repair_cost(&self, data: &GameData, line_id: &str) -> Option<f64> {
        let line = data.line(line_id)?;
        let state = self.lines.get(line_id)?;
        Some((1.0 - state.condition) * line.length_km * data.balance.repair_cost_per_km)
    }

    pub fn repair_line(&mut self, data: &GameData, line_id: &str) -> Result<f64, ActionError> {
        let cost = self
            .repair_cost(data, line_id)
            .ok_or(ActionError::UnknownLine)?;
        if cost < 0.5 {
            return Err(ActionError::NothingToRepair);
        }
        self.pay(cost)?;
        if let Some(state) = self.lines.get_mut(line_id) {
            state.condition = 1.0;
        }
        Ok(cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "stockton_darlington";

    fn setup() -> (World, GameData) {
        let data = GameData::embedded().unwrap();
        (World::new(&data), data)
    }

    #[test]
    fn new_game_starts_with_one_docked_locomotive() {
        let (world, data) = setup();
        assert_eq!(world.money, data.balance.start_money);
        assert_eq!(world.trains.len(), 1);
        assert_eq!(world.trains[0].loco, "locomotion_1");
        assert!(world.trains[0].is_docked());
        assert_eq!(world.time_of_day(), (8, 0));
        assert_eq!(world.day(), 1);
    }

    #[test]
    fn buying_needs_money_and_deducts_the_price() {
        let (mut world, data) = setup();
        assert_eq!(
            world.buy_locomotive(&data, "locomotion_1", LINE),
            Err(ActionError::NotEnoughMoney { needed: 400.0 })
        );
        assert_eq!(world.trains.len(), 1);
        world.money = 450.0;
        let id = world.buy_locomotive(&data, "locomotion_1", LINE).unwrap();
        assert_eq!(id, 2);
        assert_eq!(world.money, 50.0);
        assert_eq!(world.trains.len(), 2);
    }

    #[test]
    fn rocket_is_locked_until_the_boiler_is_researched() {
        let (mut world, data) = setup();
        world.money = 10_000.0;
        assert_eq!(
            world.buy_locomotive(&data, "rocket", LINE),
            Err(ActionError::Locked("Chaudière multitubulaire".into()))
        );
        assert_eq!(world.money, 10_000.0);
        world.research_done.insert("multitubular_boiler".into());
        assert!(world.buy_locomotive(&data, "rocket", LINE).is_ok());
        assert_eq!(world.money, 8_500.0);
    }

    #[test]
    fn line_capacity_limits_the_fleet() {
        let (mut world, data) = setup();
        world.money = 100_000.0;
        for _ in 0..3 {
            world.buy_locomotive(&data, "locomotion_1", LINE).unwrap();
        }
        assert_eq!(
            world.buy_locomotive(&data, "locomotion_1", LINE),
            Err(ActionError::LineFull)
        );
        assert_eq!(world.trains.len(), 4);
    }

    #[test]
    fn dispatch_only_works_for_docked_trains() {
        let (mut world, _) = setup();
        assert_eq!(world.dispatch(99), Err(ActionError::UnknownTrain));
        world.dispatch(1).unwrap();
        assert_eq!(world.dispatch(1), Err(ActionError::NotAtStation));
        assert_eq!(world.dispatch_all(), 0);
    }

    #[test]
    fn research_rules() {
        let (mut world, data) = setup();
        assert!(matches!(
            world.start_research(&data, "hand_signals"),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        world.money = 2_000.0;
        world.start_research(&data, "hand_signals").unwrap();
        assert_eq!(world.money, 1_500.0);
        assert_eq!(
            world.start_research(&data, "multitubular_boiler"),
            Err(ActionError::ResearchBusy)
        );
        world.research_active = None;
        world.research_done.insert("hand_signals".into());
        assert_eq!(
            world.start_research(&data, "hand_signals"),
            Err(ActionError::AlreadyResearched)
        );
        assert_eq!(
            world.start_research(&data, "nope"),
            Err(ActionError::UnknownResearch)
        );
    }

    #[test]
    fn repair_cost_scales_with_damage() {
        let (mut world, data) = setup();
        assert_eq!(
            world.repair_line(&data, LINE),
            Err(ActionError::NothingToRepair)
        );
        world.lines.get_mut(LINE).unwrap().condition = 0.5;
        // Moitié de 19 km à £ 20 le km.
        assert_eq!(world.repair_cost(&data, LINE), Some(190.0));
        assert!(matches!(
            world.repair_line(&data, LINE),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        world.money = 200.0;
        assert_eq!(world.repair_line(&data, LINE), Ok(190.0));
        assert_eq!(world.money, 10.0);
        assert_eq!(world.lines[LINE].condition, 1.0);
    }

    #[test]
    fn reconcile_drops_what_the_data_no_longer_defines() {
        let (mut world, data) = setup();
        world.trains[0].loco = "removed_by_mod".into();
        world.research_done.insert("gone".into());
        world.research_active = Some(ActiveResearch {
            id: "gone".into(),
            progress_s: 1.0,
        });
        world.stations.clear();
        world.reconcile(&data);
        assert!(world.trains.is_empty());
        assert!(world.research_done.is_empty());
        assert!(world.research_active.is_none());
        assert_eq!(world.stations.len(), 2);
    }
}
