//! Actions du joueur : achats, départs, recherche, époques, améliorations.

use std::fmt;

use crate::data::{
    BonusKind, Country, Effect, Era, EventEffect, GameData, Line, Locomotive, Research, Staff,
    Upgrade,
};

use super::tick::{plan_trip, station_capacity, water_wait_s};
use super::world::{ActiveResearch, TrainOrder, TrainState, World};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionError {
    UnknownLocomotive,
    UnknownLine,
    UnknownTrain,
    UnknownResearch,
    UnknownUpgrade,
    UnknownFuel,
    /// Nom de la recherche manquante.
    Locked(String),
    /// Nom de l'époque à atteindre.
    WrongEra(String),
    /// Nom de l'amélioration de ligne manquante.
    NeedsUpgrade(String),
    NotEnoughMoney {
        needed: f64,
    },
    LineFull,
    NotAtStation,
    /// Minutes de jeu restantes avant la fin de la prise d'eau.
    TakingWater {
        minutes: u32,
    },
    AlreadyResearched,
    ResearchBusy,
    NothingToRepair,
    AlreadyInstalled,
    LastEra,
    /// Conditions d'entrée dans l'époque suivante non réunies.
    EraNotReady,
    NotStockable,
    NoEvent,
    UnknownCountry,
    /// Nom du pays où il faut d'abord une concession.
    NoConcession(String),
    AlreadyOwned,
    UnknownStaff,
    UnknownBonus,
    UnknownCrew,
    /// Recettes insuffisantes, ou époque trop précoce, pour nationaliser.
    PrestigeNotReady,
    NotEnoughPoints {
        needed: u32,
    },
    MaxLevel,
    /// La ligne ne touche aucune gare déjà desservie.
    NotConnected,
    LineNotBuilt,
    /// Personne pour conduire ce train.
    NoDriver,
    /// Nom du combustible épuisé : le train ne peut pas partir.
    NoFuel(String),
    /// Le train est déjà tiré par cette machine.
    SameLocomotive,
    UnknownTraining,
    /// Les inscriptions sont closes : la session a commencé.
    TrainingStarted,
    /// Toutes les places de la session sont prises.
    TrainingFull,
    /// La session s'adresse à l'autre métier.
    WrongRole,
    /// L'équipier est déjà inscrit à une session.
    AlreadyEnrolled,
    /// La compagnie a déjà choisi son pays : seule une nationalisation permet
    /// de repartir ailleurs.
    CompanyFounded,
}

/// Issue d'une vente ou d'un remplacement : fait sur-le-champ (montant
/// encaissé, ou payé pour un remplacement), ou prévu à la prochaine arrivée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderOutcome {
    Done(f64),
    Scheduled,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::UnknownLocomotive => write!(f, "Locomotive inconnue"),
            ActionError::UnknownLine => write!(f, "Ligne inconnue"),
            ActionError::UnknownTrain => write!(f, "Aucun train sélectionné"),
            ActionError::UnknownResearch => write!(f, "Recherche inconnue"),
            ActionError::UnknownUpgrade => write!(f, "Amélioration inconnue"),
            ActionError::UnknownFuel => write!(f, "Combustible inconnu"),
            ActionError::Locked(name) => write!(f, "Il faut d'abord rechercher : {name}"),
            ActionError::WrongEra(name) => write!(f, "Pas avant l'époque : {name}"),
            ActionError::NeedsUpgrade(name) => write!(f, "La ligne doit d'abord recevoir : {name}"),
            ActionError::NotEnoughMoney { needed } => {
                write!(f, "Pas assez d'argent (il faut {} €)", needed.ceil())
            }
            ActionError::LineFull => write!(f, "La ligne ne peut pas accueillir plus de trains"),
            ActionError::NotAtStation => write!(f, "Ce train est en route"),
            ActionError::TakingWater { minutes } => {
                write!(f, "La machine fait de l'eau : encore {minutes} min")
            }
            ActionError::AlreadyResearched => write!(f, "Recherche déjà terminée"),
            ActionError::ResearchBusy => write!(f, "Une recherche est déjà en cours"),
            ActionError::NothingToRepair => write!(f, "La voie est déjà en parfait état"),
            ActionError::AlreadyInstalled => write!(f, "Amélioration déjà installée"),
            ActionError::LastEra => write!(f, "Tu as atteint la dernière époque"),
            ActionError::EraNotReady => write!(
                f,
                "Les conditions de la prochaine époque ne sont pas réunies"
            ),
            ActionError::NotStockable => write!(f, "Cette énergie ne se stocke pas"),
            ActionError::NoEvent => write!(f, "Aucun événement en attente"),
            ActionError::UnknownCountry => write!(f, "Pays inconnu"),
            ActionError::NoConcession(name) => write!(f, "Il faut d'abord une concession : {name}"),
            ActionError::AlreadyOwned => write!(f, "Déjà acquis"),
            ActionError::UnknownStaff => write!(f, "Service inconnu"),
            ActionError::UnknownBonus => write!(f, "Bonus inconnu"),
            ActionError::UnknownCrew => write!(f, "Équipier inconnu"),
            ActionError::PrestigeNotReady => {
                write!(
                    f,
                    "Il faut terminer tout l'arbre de recherche avant de nationaliser"
                )
            }
            ActionError::NotEnoughPoints { needed } => {
                write!(f, "Pas assez de points de prestige (il en faut {needed})")
            }
            ActionError::MaxLevel => write!(f, "Niveau maximal atteint"),
            ActionError::NotConnected => {
                write!(
                    f,
                    "La ligne doit partir d'une gare déjà desservie, ou de la capitale du pays"
                )
            }
            ActionError::LineNotBuilt => write!(f, "Cette ligne n'est pas encore construite"),
            ActionError::NoDriver => {
                write!(
                    f,
                    "Ce train n'a pas de conducteur affecté (s : recruter et affecter)"
                )
            }
            ActionError::SameLocomotive => write!(f, "Ce train est déjà tiré par cette machine"),
            ActionError::UnknownTraining => write!(f, "Formation inconnue"),
            ActionError::TrainingStarted => {
                write!(f, "La formation a commencé : inscriptions closes")
            }
            ActionError::TrainingFull => write!(f, "Formation complète"),
            ActionError::WrongRole => write!(f, "Cette formation s'adresse à l'autre métier"),
            ActionError::AlreadyEnrolled => write!(f, "Déjà inscrit à une formation"),
            ActionError::NoFuel(name) => {
                write!(
                    f,
                    "Plus de {name} en stock : le train reste à quai (c pour en acheter)"
                )
            }
            ActionError::CompanyFounded => write!(
                f,
                "La compagnie est déjà fondée : seule une nationalisation permet de repartir ailleurs"
            ),
        }
    }
}

impl std::error::Error for ActionError {}

/// Où en est le joueur des conditions d'entrée dans l'époque suivante.
#[derive(Debug, Clone)]
pub struct UnlockStatus<'a> {
    pub era: &'a Era,
    /// Recherches exigées, avec leur état.
    pub research: Vec<(&'a Research, bool)>,
    pub passengers: u64,
    pub passengers_needed: u64,
    pub cost: f64,
    pub affordable: bool,
}

impl UnlockStatus<'_> {
    pub fn ready(&self) -> bool {
        self.research.iter().all(|(_, done)| *done) && self.passengers >= self.passengers_needed
    }
}

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

    /// Erreur si l'époque `era_id` n'est pas encore atteinte.
    fn check_era(&self, data: &GameData, era_id: &str) -> Result<(), ActionError> {
        match data.era_index(era_id) {
            Some(rank) if rank > self.era_index(data) => {
                Err(ActionError::WrongEra(data.eras[rank].name.fr.clone()))
            }
            _ => Ok(()),
        }
    }

    /// Nombre de trains admis sur une ligne, compte tenu de la signalisation.
    pub fn line_capacity(&self, data: &GameData, line_id: &str) -> usize {
        let extra = self.modifiers(data).line_capacity;
        data.line(line_id)
            .map_or(0, |l| (l.max_trains + extra) as usize)
    }

    /// Ce qui empêche d'acheter cette machine pour cette ligne, argent mis à part.
    pub fn purchase_blocker(
        &self,
        data: &GameData,
        loco: &Locomotive,
        line_id: &str,
    ) -> Option<ActionError> {
        if let Some(blocker) = self.loco_blocker(data, loco, line_id) {
            return Some(blocker);
        }
        if self.trains_on(line_id) >= self.line_capacity(data, line_id) {
            return Some(ActionError::LineFull);
        }
        None
    }

    /// Ce qui empêche cette machine de rouler sur cette ligne : époque,
    /// recherche ou équipement de la voie (la place sur la ligne mise à part).
    fn loco_blocker(
        &self,
        data: &GameData,
        loco: &Locomotive,
        line_id: &str,
    ) -> Option<ActionError> {
        if let Err(e) = self.check_era(data, &loco.era) {
            return Some(e);
        }
        if let Some(name) = self.missing_requirement(data, &loco.requires) {
            return Some(ActionError::Locked(name));
        }
        if let Some(upgrade) = &loco.requires_upgrade
            && !self
                .lines
                .get(line_id)
                .is_some_and(|l| l.upgrades.contains(upgrade))
        {
            let name = data
                .upgrade(upgrade)
                .map_or(upgrade.clone(), |u| u.name.fr.clone());
            return Some(ActionError::NeedsUpgrade(name));
        }
        None
    }

    /// Places encore libres sur une ligne.
    pub fn line_room(&self, data: &GameData, line_id: &str) -> usize {
        self.line_capacity(data, line_id)
            .saturating_sub(self.trains_on(line_id))
    }

    /// Achète une locomotive et la met à quai au départ de la ligne.
    pub fn buy_locomotive(
        &mut self,
        data: &GameData,
        loco_id: &str,
        line_id: &str,
    ) -> Result<u32, ActionError> {
        let ids = self.buy_locomotives(data, loco_id, line_id, 1)?;
        Ok(ids[0])
    }

    /// Achète `count` locomotives d'un coup, tout ou rien : il faut la place
    /// sur la ligne et l'argent pour toute la commande. Renvoie leurs numéros.
    pub fn buy_locomotives(
        &mut self,
        data: &GameData,
        loco_id: &str,
        line_id: &str,
        count: u32,
    ) -> Result<Vec<u32>, ActionError> {
        let count = count.max(1);
        let loco = data
            .locomotive(loco_id)
            .ok_or(ActionError::UnknownLocomotive)?;
        data.line(line_id).ok_or(ActionError::UnknownLine)?;
        if !self.is_built(line_id) {
            return Err(ActionError::LineNotBuilt);
        }
        if let Some(blocker) = self.purchase_blocker(data, loco, line_id) {
            return Err(blocker);
        }
        if count as usize > self.line_room(data, line_id) {
            return Err(ActionError::LineFull);
        }
        self.pay(loco.cost * count as f64)?;
        let ids: Vec<u32> = (0..count)
            .map(|_| self.spawn_train(loco_id, line_id))
            .collect();
        // Les conducteurs en réserve prennent les nouvelles machines.
        self.staff_trains(&ids);
        Ok(ids)
    }

    /// Somme récupérée en revendant une machine.
    pub fn resale_value(&self, data: &GameData, train_id: u32) -> Option<f64> {
        let loco = data.locomotive(&self.train(train_id)?.loco)?;
        Some(loco.cost * data.balance.resale_ratio)
    }

    /// Revend un train : sur-le-champ s'il est à quai, sinon à sa prochaine
    /// arrivée. Un remplacement déjà commandé est alors annulé et remboursé.
    pub fn sell_train(
        &mut self,
        data: &GameData,
        train_id: u32,
    ) -> Result<OrderOutcome, ActionError> {
        let train = self.train(train_id).ok_or(ActionError::UnknownTrain)?;
        if train.is_docked() {
            self.cancel_order(train_id)?;
            return self.sell_now(data, train_id).map(OrderOutcome::Done);
        }
        self.cancel_order(train_id)?;
        if let Some(train) = self.train_mut(train_id) {
            train.order = Some(TrainOrder::Sell);
        }
        Ok(OrderOutcome::Scheduled)
    }

    /// Annule la vente ou le remplacement prévu ; rembourse un remplacement payé.
    pub fn cancel_order(&mut self, train_id: u32) -> Result<(), ActionError> {
        let train = self.train_mut(train_id).ok_or(ActionError::UnknownTrain)?;
        if let Some(TrainOrder::Replace { paid, .. }) = train.order.take() {
            self.money += paid;
        }
        Ok(())
    }

    /// Ce qui empêche de remplacer la machine d'un train par `loco`, argent mis à part.
    pub fn replace_blocker(
        &self,
        data: &GameData,
        train_id: u32,
        loco: &Locomotive,
    ) -> Option<ActionError> {
        let Some(train) = self.train(train_id) else {
            return Some(ActionError::UnknownTrain);
        };
        if train.loco == loco.id {
            return Some(ActionError::SameLocomotive);
        }
        self.loco_blocker(data, loco, &train.line)
    }

    /// Prix d'un remplacement : la nouvelle machine, moins la reprise de l'ancienne.
    /// Négatif quand on passe à une machine moins chère.
    pub fn replacement_cost(
        &self,
        data: &GameData,
        train_id: u32,
        loco: &Locomotive,
    ) -> Option<f64> {
        Some(loco.cost - self.resale_value(data, train_id)?)
    }

    /// Remplace la machine d'un train sans toucher à sa ligne ni à son équipage :
    /// sur-le-champ s'il est à quai, sinon à sa prochaine arrivée (payé dès la
    /// commande). Renvoie le montant payé quand c'est fait.
    pub fn replace_train(
        &mut self,
        data: &GameData,
        train_id: u32,
        loco_id: &str,
    ) -> Result<OrderOutcome, ActionError> {
        let loco = data
            .locomotive(loco_id)
            .ok_or(ActionError::UnknownLocomotive)?;
        if let Some(blocker) = self.replace_blocker(data, train_id, loco) {
            return Err(blocker);
        }
        // Une commande précédente est annulée (et remboursée) avant de payer la nouvelle.
        let previous = self.train(train_id).and_then(|t| t.order.clone());
        self.cancel_order(train_id)?;
        let net = self
            .replacement_cost(data, train_id, loco)
            .ok_or(ActionError::UnknownTrain)?;
        let paid = net.max(0.0);
        if let Err(e) = self.pay(paid) {
            // Rien ne change si l'on ne peut pas payer.
            if let Some(TrainOrder::Replace { paid, .. }) = &previous {
                self.money -= paid;
            }
            if let Some(train) = self.train_mut(train_id) {
                train.order = previous;
            }
            return Err(e);
        }
        if let Some(train) = self.train_mut(train_id) {
            train.order = Some(TrainOrder::Replace {
                loco: loco_id.to_string(),
                paid,
            });
        }
        if self.train(train_id).is_some_and(|t| t.is_docked()) {
            self.execute_order(data, train_id);
            return Ok(OrderOutcome::Done(net));
        }
        Ok(OrderOutcome::Scheduled)
    }

    /// Exécute l'ordre d'un train à quai. Renvoie ce qui a été fait :
    /// `(ancienne machine, nouvelle machine ou None si vendu, montant encaissé)`.
    pub(crate) fn execute_order(
        &mut self,
        data: &GameData,
        train_id: u32,
    ) -> Option<(String, Option<String>, f64)> {
        let train = self.train_mut(train_id)?;
        if !train.is_docked() {
            return None;
        }
        let old = train.loco.clone();
        match train.order.take()? {
            TrainOrder::Sell => {
                let refund = self.sell_now(data, train_id).ok()?;
                Some((old, None, refund))
            }
            TrainOrder::Replace { loco, paid } => {
                let Some(new) = data.locomotive(&loco) else {
                    self.money += paid;
                    return None;
                };
                // Reprise de l'ancienne machine au-delà de ce qui a été payé (passage
                // à une machine moins chère).
                let credit = (self.resale_value(data, train_id).unwrap_or(0.0) - new.cost).max(0.0);
                self.money += credit;
                let capacity = self.modifiers(data).capacity_of(new);
                let line = data.line(&self.train(train_id)?.line);
                let train = self.train_mut(train_id)?;
                train.loco = loco.clone();
                // Les voyageurs en trop redescendent sur le quai.
                let mut excess = std::collections::BTreeMap::new();
                while train.count() > capacity {
                    let over = train.count() - capacity;
                    let Some((class, n)) = train.load.iter_mut().max_by_key(|(_, n)| **n) else {
                        break;
                    };
                    let off = (*n).min(over);
                    if off == 0 {
                        break;
                    }
                    *n -= off;
                    *excess.entry(class.clone()).or_insert(0.0) += off as f64;
                }
                train.load.retain(|_, n| *n > 0);
                let origin = line.map(|l| train.dir.origin(l).to_string());
                if let Some(station) = origin.and_then(|id| self.stations.get_mut(&id)) {
                    for (class, n) in excess {
                        *station.waiting.entry(class).or_default() += n;
                    }
                }
                Some((old, Some(loco), credit))
            }
        }
    }

    fn train_mut(&mut self, id: u32) -> Option<&mut super::world::Train> {
        self.trains.iter_mut().find(|t| t.id == id)
    }

    /// Revend sur-le-champ un train à quai ; ses voyageurs retournent attendre en gare.
    fn sell_now(&mut self, data: &GameData, train_id: u32) -> Result<f64, ActionError> {
        let index = self
            .trains
            .iter()
            .position(|t| t.id == train_id)
            .ok_or(ActionError::UnknownTrain)?;
        if !self.trains[index].is_docked() {
            return Err(ActionError::NotAtStation);
        }
        let refund = self.resale_value(data, train_id).unwrap_or(0.0);
        let train = self.trains.remove(index);
        self.sync_crew();
        if let Some(station) = data
            .line(&train.line)
            .and_then(|line| self.stations.get_mut(train.dir.origin(line)))
        {
            for (class, n) in train.load {
                *station.waiting.entry(class).or_default() += n as f64;
            }
        }
        self.money += refund;
        Ok(refund)
    }

    /// Donne le départ à un train à quai.
    pub fn dispatch(&mut self, data: &GameData, train_id: u32) -> Result<(), ActionError> {
        let index = self
            .trains
            .iter()
            .position(|t| t.id == train_id)
            .ok_or(ActionError::UnknownTrain)?;
        self.depart(data, index)
    }

    /// Donne le départ à tous les trains à quai qui peuvent partir ; renvoie leur nombre.
    pub fn dispatch_all(&mut self, data: &GameData) -> usize {
        (0..self.trains.len())
            .filter(|&index| self.depart(data, index).is_ok())
            .count()
    }

    fn depart(&mut self, data: &GameData, index: usize) -> Result<(), ActionError> {
        let modifiers = self.modifiers(data);
        let era = self.era(data);
        let clock = self.clock;
        let train = &self.trains[index];
        let TrainState::AtStation { waited } = train.state else {
            return Err(ActionError::NotAtStation);
        };
        if !train.crew.has_driver {
            return Err(ActionError::NoDriver);
        }
        let loco = data
            .locomotive(&train.loco)
            .ok_or(ActionError::UnknownLocomotive)?;
        if self.is_out_of_fuel(data, &loco.fuel) {
            let name = data
                .fuel(&loco.fuel)
                .map_or(loco.fuel.clone(), |f| f.name.fr.to_lowercase());
            return Err(ActionError::NoFuel(name));
        }
        let water_left = water_wait_s(era, &modifiers, loco) - waited;
        if water_left > 0.0 {
            return Err(ActionError::TakingWater {
                minutes: (water_left / 60.0).ceil() as u32,
            });
        }
        let trip = plan_trip(data, &modifiers, train, self.lines.get(&train.line), clock)
            .ok_or(ActionError::UnknownLine)?;
        let station_fill = data
            .line(&train.line)
            .map(|line| train.dir.origin(line))
            .and_then(|id| Some(self.stations.get(id)?.total() / station_capacity(self, data, id)))
            .unwrap_or(0.0);
        self.trains[index].depart(clock, modifiers.capacity_of(loco), station_fill, trip);
        Ok(())
    }

    /// Règle le prix des billets d'une ligne ; renvoie le niveau retenu.
    pub fn set_fare_level(
        &mut self,
        data: &GameData,
        line_id: &str,
        level: f64,
    ) -> Result<f64, ActionError> {
        let b = &data.balance;
        let state = self
            .lines
            .get_mut(line_id)
            .ok_or(ActionError::UnknownLine)?;
        // Arrondi au centième pour éviter les dérives d'une série de petits pas.
        state.fare_level =
            ((level * 100.0).round() / 100.0).clamp(b.fare_level_min, b.fare_level_max);
        Ok(state.fare_level)
    }

    /// Règle l'arrêt prévu en gare (minutes de jeu) ; renvoie la valeur retenue.
    /// Les trains déjà à quai gardent l'heure de départ annoncée.
    pub fn set_dwell(
        &mut self,
        data: &GameData,
        line_id: &str,
        minutes: f64,
    ) -> Result<f64, ActionError> {
        let b = &data.balance;
        let state = self
            .lines
            .get_mut(line_id)
            .ok_or(ActionError::UnknownLine)?;
        state.dwell_min = minutes.round().clamp(b.dwell_min_min, b.dwell_max_min);
        Ok(state.dwell_min)
    }

    /// Ce qui empêche de lancer cette recherche, argent mis à part.
    pub fn research_blocker(&self, data: &GameData, research: &Research) -> Option<ActionError> {
        if self.research_done.contains(&research.id) {
            return Some(ActionError::AlreadyResearched);
        }
        if let Err(e) = self.check_era(data, &research.era) {
            return Some(e);
        }
        self.missing_requirement(data, &research.requires)
            .map(ActionError::Locked)
    }

    pub fn start_research(&mut self, data: &GameData, id: &str) -> Result<(), ActionError> {
        let research = data.research(id).ok_or(ActionError::UnknownResearch)?;
        if let Some(blocker) = self.research_blocker(data, research) {
            return Err(blocker);
        }
        if self.research_active.is_some() {
            return Err(ActionError::ResearchBusy);
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
        let per_km = data.balance.repair_cost_per_km * self.era(data).cost_mult;
        Some((1.0 - state.condition) * line.length_km * per_km)
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

    // ----- Époques -----

    /// Conditions d'entrée dans l'époque suivante, ou `None` à la dernière.
    pub fn next_era<'a>(&self, data: &'a GameData) -> Option<UnlockStatus<'a>> {
        let era = data.eras.get(self.era_index(data) + 1)?;
        Some(UnlockStatus {
            era,
            research: era
                .unlock
                .research
                .iter()
                .filter_map(|id| data.research(id))
                .map(|r| (r, self.research_done.contains(&r.id)))
                .collect(),
            passengers: self.passengers_carried,
            passengers_needed: era.unlock.passengers,
            cost: era.unlock.cost,
            affordable: self.money >= era.unlock.cost,
        })
    }

    /// Passe à l'époque suivante ; renvoie son identifiant.
    pub fn advance_era(&mut self, data: &GameData) -> Result<String, ActionError> {
        let status = self.next_era(data).ok_or(ActionError::LastEra)?;
        if !status.ready() {
            return Err(ActionError::EraNotReady);
        }
        self.pay(status.cost)?;
        let had_market = self.era(data).rules.fuel_market;
        self.era = status.era.id.clone();
        self.era_started = self.clock;
        // Ouverture du marché des combustibles : un premier lot offert, pour que
        // la flotte ne reste pas à quai dès le changement d'époque.
        if !had_market && status.era.rules.fuel_market {
            for fuel in data.fuels.iter().filter(|f| f.stock) {
                let used = self
                    .trains
                    .iter()
                    .any(|t| data.locomotive(&t.loco).is_some_and(|l| l.fuel == fuel.id));
                if used {
                    let stock = self.fuel.entry(fuel.id.clone()).or_insert(0.0);
                    *stock = stock.max(fuel.lot);
                }
            }
        }
        Ok(self.era.clone())
    }

    // ----- Expansion du réseau -----

    /// Erreur si l'écartement de l'un de ces pays diffère de celui du pays de
    /// départ sans que la recherche d'écartement variable soit faite.
    fn check_gauge(&self, data: &GameData, countries: &[&Country]) -> Result<(), ActionError> {
        let home = self.home_country(data).map(|c| c.gauge_mm);
        if countries.iter().all(|c| Some(c.gauge_mm) == home) || self.modifiers(data).variable_gauge
        {
            return Ok(());
        }
        let research = data
            .research
            .iter()
            .find(|r| r.effects.contains(&Effect::VariableGauge));
        Err(ActionError::Locked(
            research.map_or("Écartement variable".to_string(), |r| r.name.fr.clone()),
        ))
    }

    /// Ce qui empêche d'obtenir la concession d'un pays, argent mis à part.
    pub fn concession_blocker(&self, data: &GameData, country: &Country) -> Option<ActionError> {
        if self.has_concession(&country.id) {
            return Some(ActionError::AlreadyOwned);
        }
        if let Some(era) = &country.era
            && let Err(e) = self.check_era(data, era)
        {
            return Some(e);
        }
        self.check_gauge(data, &[country]).err()
    }

    /// Achète la concession d'un pays : on peut ensuite y construire des lignes.
    pub fn buy_concession(
        &mut self,
        data: &GameData,
        country_id: &str,
    ) -> Result<f64, ActionError> {
        let country = data
            .country(country_id)
            .ok_or(ActionError::UnknownCountry)?;
        if let Some(blocker) = self.concession_blocker(data, country) {
            return Err(blocker);
        }
        self.pay(country.concession_cost)?;
        self.countries.insert(country.id.clone());
        Ok(country.concession_cost)
    }

    /// Prix de construction d'une ligne : longueur, relief du pays et époque.
    pub fn line_cost(&self, data: &GameData, line: &Line) -> f64 {
        let relief = data.country_of_line(&line.id).map_or(1.0, |c| c.build_cost);
        data.balance.build_cost_per_km
            * line.length_km
            * relief
            * self.era(data).build_mult
            * self.modifiers(data).build_cost
    }

    /// Ce qui empêche de construire une ligne, argent mis à part.
    pub fn line_blocker(&self, data: &GameData, line: &Line) -> Option<ActionError> {
        if self.is_built(&line.id) {
            return Some(ActionError::AlreadyOwned);
        }
        let countries = data.line_countries(line);
        if let Some(missing) = countries.iter().find(|c| !self.has_concession(&c.id)) {
            return Some(ActionError::NoConcession(missing.name.fr.clone()));
        }
        if let Some(era) = &line.era
            && let Err(e) = self.check_era(data, era)
        {
            return Some(e);
        }
        if let Err(e) = self.check_gauge(data, &countries) {
            return Some(e);
        }
        // Le réseau s'étend de proche en proche ; dans un pays encore vide, il part de la capitale.
        let connected = self.is_served(data, &line.from) || self.is_served(data, &line.to);
        let fresh_start = data.country_of_line(&line.id).is_some_and(|country| {
            !country.lines.iter().any(|l| self.is_built(&l.id))
                && country
                    .capital()
                    .is_some_and(|c| c.id == line.from || c.id == line.to)
        });
        (!connected && !fresh_start).then_some(ActionError::NotConnected)
    }

    pub fn build_line(&mut self, data: &GameData, line_id: &str) -> Result<f64, ActionError> {
        let line = data.line(line_id).ok_or(ActionError::UnknownLine)?;
        if let Some(blocker) = self.line_blocker(data, line) {
            return Err(blocker);
        }
        let cost = self.line_cost(data, line);
        self.pay(cost)?;
        let state = self
            .lines
            .get_mut(line_id)
            .ok_or(ActionError::UnknownLine)?;
        state.built = true;
        state.condition = 1.0;
        Ok(cost)
    }

    // ----- Personnel -----

    /// Ce qui empêche d'engager ce service, argent mis à part.
    pub fn staff_blocker(&self, data: &GameData, staff: &Staff) -> Option<ActionError> {
        if self.staff.contains(&staff.id) {
            return Some(ActionError::AlreadyOwned);
        }
        if let Err(e) = self.check_era(data, &staff.era) {
            return Some(e);
        }
        self.missing_requirement(data, &staff.requires)
            .map(ActionError::Locked)
    }

    /// Engage un service ; son salaire court ensuite à chaque heure de jeu.
    pub fn hire(&mut self, data: &GameData, staff_id: &str) -> Result<f64, ActionError> {
        let staff = data.staff(staff_id).ok_or(ActionError::UnknownStaff)?;
        if let Some(blocker) = self.staff_blocker(data, staff) {
            return Err(blocker);
        }
        self.pay(staff.cost)?;
        self.staff.insert(staff.id.clone());
        Ok(staff.cost)
    }

    // ----- Prestige -----

    /// Recherches terminées sur le total de l'arbre.
    pub fn research_progress(&self, data: &GameData) -> (usize, usize) {
        let done = data
            .research
            .iter()
            .filter(|r| self.research_done.contains(&r.id))
            .count();
        (done, data.research.len())
    }

    /// Points de prestige que rapporterait la nationalisation de la compagnie :
    /// rien tant que l'arbre de recherche n'est pas terminé, puis un nombre fixe.
    pub fn prestige_gain(&self, data: &GameData) -> u32 {
        let (done, total) = self.research_progress(data);
        if done < total {
            return 0;
        }
        data.balance.prestige_points
    }

    /// Nationalise la compagnie : la partie repart de zéro, les points et les
    /// bonus permanents restent. Renvoie les points gagnés.
    pub fn nationalise(&mut self, data: &GameData) -> Result<u32, ActionError> {
        let gain = self.prestige_gain(data);
        if gain == 0 {
            return Err(ActionError::PrestigeNotReady);
        }
        let mut prestige = std::mem::take(&mut self.prestige);
        prestige.points += gain;
        prestige.earned += gain;
        prestige.resets += 1;
        let home = self.home.clone();
        self.refound(data, &home, prestige);
        // La nouvelle compagnie peut s'installer dans un autre pays.
        self.choosing_home = true;
        Ok(gain)
    }

    /// Repart de zéro dans un pays, en gardant le prestige et ses bonus.
    fn refound(&mut self, data: &GameData, country_id: &str, prestige: super::world::Prestige) {
        *self = World::founded_in(data, country_id);
        self.money += prestige.total(data, BonusKind::StartMoney);
        self.prestige = prestige;
    }

    /// Fonde la nouvelle compagnie dans ce pays, tant que le choix est ouvert
    /// (nouvelle partie ou nationalisation) : la partie repart sur sa ligne de départ.
    pub fn found_company_in(
        &mut self,
        data: &GameData,
        country_id: &str,
    ) -> Result<(), ActionError> {
        if !self.choosing_home {
            return Err(ActionError::CompanyFounded);
        }
        let country = data
            .country(country_id)
            .filter(|c| c.start_line.is_some())
            .ok_or(ActionError::UnknownCountry)?;
        if country.id != self.home {
            let prestige = std::mem::take(&mut self.prestige);
            self.refound(data, &country.id, prestige);
        }
        self.choosing_home = false;
        Ok(())
    }

    /// Achète un niveau d'un bonus permanent ; renvoie son prix en points.
    pub fn buy_bonus(&mut self, data: &GameData, bonus_id: &str) -> Result<u32, ActionError> {
        let bonus = data.bonus(bonus_id).ok_or(ActionError::UnknownBonus)?;
        let level = self.prestige.level(bonus_id);
        if level >= bonus.max_level {
            return Err(ActionError::MaxLevel);
        }
        let cost = bonus.cost_at(level);
        if self.prestige.points < cost {
            return Err(ActionError::NotEnoughPoints { needed: cost });
        }
        self.prestige.points -= cost;
        self.prestige.levels.insert(bonus_id.to_string(), level + 1);
        Ok(cost)
    }

    // ----- Améliorations de ligne -----

    pub fn upgrade_cost(&self, data: &GameData, line_id: &str, upgrade: &Upgrade) -> Option<f64> {
        let line = data.line(line_id)?;
        let country = data.country_of_line(line_id).map_or(1.0, |c| c.build_cost);
        Some(upgrade.cost_per_km * line.length_km * country * self.modifiers(data).build_cost)
    }

    /// Ce qui empêche d'installer cette amélioration, argent mis à part.
    pub fn upgrade_blocker(
        &self,
        data: &GameData,
        line_id: &str,
        upgrade: &Upgrade,
    ) -> Option<ActionError> {
        let installed = &self.lines.get(line_id)?.upgrades;
        if installed.contains(&upgrade.id) {
            return Some(ActionError::AlreadyInstalled);
        }
        if let Err(e) = self.check_era(data, &upgrade.era) {
            return Some(e);
        }
        if let Some(name) = self.missing_requirement(data, &upgrade.requires) {
            return Some(ActionError::Locked(name));
        }
        if let Some(previous) = &upgrade.requires_upgrade
            && !installed.contains(previous)
        {
            let name = data
                .upgrade(previous)
                .map_or(previous.clone(), |u| u.name.fr.clone());
            return Some(ActionError::NeedsUpgrade(name));
        }
        None
    }

    pub fn buy_upgrade(
        &mut self,
        data: &GameData,
        line_id: &str,
        upgrade_id: &str,
    ) -> Result<f64, ActionError> {
        let upgrade = data
            .upgrade(upgrade_id)
            .ok_or(ActionError::UnknownUpgrade)?;
        let cost = self
            .upgrade_cost(data, line_id, upgrade)
            .ok_or(ActionError::UnknownLine)?;
        if !self.lines.contains_key(line_id) {
            return Err(ActionError::UnknownLine);
        }
        if let Some(blocker) = self.upgrade_blocker(data, line_id, upgrade) {
            return Err(blocker);
        }
        self.pay(cost)?;
        if let Some(state) = self.lines.get_mut(line_id) {
            state.upgrades.insert(upgrade_id.to_string());
        }
        Ok(cost)
    }

    // ----- Combustibles -----

    /// Achète `lots` lots d'un combustible au cours du jour ; renvoie la dépense.
    pub fn buy_fuel(
        &mut self,
        data: &GameData,
        fuel_id: &str,
        lots: u32,
    ) -> Result<f64, ActionError> {
        let fuel = data.fuel(fuel_id).ok_or(ActionError::UnknownFuel)?;
        if !fuel.stock {
            return Err(ActionError::NotStockable);
        }
        let quantity = fuel.lot * lots as f64;
        let cost = quantity * fuel.price(self.clock, self.era(data));
        self.pay(cost)?;
        *self.fuel.entry(fuel_id.to_string()).or_default() += quantity;
        Ok(cost)
    }

    /// Consommation d'une énergie par heure de jeu si tous les trains roulaient sans arrêt.
    pub fn fuel_burn_per_hour(&self, data: &GameData, fuel_id: &str) -> f64 {
        let m = self.modifiers(data);
        self.trains
            .iter()
            .filter_map(|t| Some((data.locomotive(&t.loco)?, self.lines.get(&t.line))))
            .filter(|(loco, _)| loco.fuel == fuel_id)
            .map(|(loco, line)| {
                loco.fuel_per_km * m.fuel * super::tick::allowed_speed(data, &m, loco, line)
            })
            .sum()
    }

    // ----- Événements historiques -----

    /// Applique le choix du joueur pour l'événement en attente.
    pub fn resolve_event(&mut self, data: &GameData, choice: usize) -> Result<(), ActionError> {
        let id = self.event_pending.clone().ok_or(ActionError::NoEvent)?;
        let Some(event) = data.event(&id) else {
            self.event_pending = None;
            return Err(ActionError::NoEvent);
        };
        let choice = event.choices.get(choice).ok_or(ActionError::NoEvent)?;
        if self.money < choice.cost() {
            return Err(ActionError::NotEnoughMoney {
                needed: choice.cost(),
            });
        }
        for effect in &choice.effects {
            match effect {
                EventEffect::Money { value } => self.money += value,
                EventEffect::GrantResearch { target } => {
                    self.research_done.insert(target.clone());
                    if self
                        .research_active
                        .as_ref()
                        .is_some_and(|a| &a.id == target)
                    {
                        // La recherche en cours est offerte : son prix est rendu.
                        self.money += data.research(target).map_or(0.0, |r| r.cost);
                        self.research_active = None;
                    }
                }
                EventEffect::Satisfaction { value, class } => {
                    for (id, mood) in &mut self.satisfaction {
                        if class.as_ref().is_none_or(|c| c == id) {
                            *mood = (*mood + value).clamp(0.0, 1.0);
                        }
                    }
                }
                EventEffect::TrackCondition { value } => {
                    for line in self.lines.values_mut() {
                        line.condition = (line.condition + value).clamp(0.0, 1.0);
                    }
                }
            }
        }
        self.events_done.insert(id);
        self.event_pending = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{CrewRole, SimEvent, advance};

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

    fn enter(world: &mut World, era: &str) {
        world.era = era.to_string();
        world.era_started = world.clock;
    }

    #[test]
    fn new_game_starts_with_one_docked_locomotive() {
        let (world, data) = setup();
        assert_eq!(world.money, data.balance.start_money);
        assert_eq!(world.trains.len(), 1);
        assert_eq!(world.trains[0].loco, "locomotion_1");
        assert!(world.trains[0].is_docked());
        assert_eq!(world.time_of_day(), (8, 0));
        assert_eq!((world.day(), world.era.as_str()), (1, "pioneers"));
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
    fn locomotives_need_their_research_their_era_and_their_infrastructure() {
        let (mut world, data) = setup();
        world.money = 1e12;
        assert_eq!(
            world.buy_locomotive(&data, "rocket", LINE),
            Err(ActionError::Locked("Chaudière multitubulaire".into()))
        );
        world.research_done.insert("multitubular_boiler".into());
        assert!(world.buy_locomotive(&data, "rocket", LINE).is_ok());

        assert_eq!(
            world.buy_locomotive(&data, "crampton", LINE),
            Err(ActionError::WrongEra("Âge d'or de la vapeur".into()))
        );
        enter(&mut world, "high_speed");
        assert!(world.buy_locomotive(&data, "crampton", LINE).is_ok());

        // Pas de grande vitesse sans signalisation en cabine, ni de machine électrique sans caténaire.
        world.research_done.insert("articulated_trainset".into());
        assert_eq!(
            world.buy_locomotive(&data, "tgv_sud_est", LINE),
            Err(ActionError::Locked("Signalisation en cabine".into()))
        );
        world.research_done.insert("cab_signalling".into());
        assert_eq!(
            world.buy_locomotive(&data, "tgv_sud_est", LINE),
            Err(ActionError::NeedsUpgrade("Caténaire".into()))
        );
        world
            .lines
            .get_mut(LINE)
            .unwrap()
            .upgrades
            .insert("catenary".into());
        assert!(world.buy_locomotive(&data, "tgv_sud_est", LINE).is_ok());
    }

    #[test]
    fn several_locomotives_can_be_bought_at_once() {
        let (mut world, data) = setup();
        assert_eq!(world.line_room(&data, LINE), 3);
        world.money = 1000.0;
        assert_eq!(
            world.buy_locomotives(&data, "locomotion_1", LINE, 3),
            Err(ActionError::NotEnoughMoney { needed: 1200.0 })
        );
        assert_eq!(
            world.buy_locomotives(&data, "locomotion_1", LINE, 4),
            Err(ActionError::LineFull)
        );
        assert_eq!(world.trains.len(), 1, "tout ou rien");
        assert_eq!(
            world.buy_locomotives(&data, "locomotion_1", LINE, 2),
            Ok(vec![2, 3])
        );
        assert_eq!(world.money, 200.0);
        assert_eq!(world.line_room(&data, LINE), 1);
        assert!(
            world
                .trains
                .iter()
                .skip(1)
                .all(|t| t.is_docked() && !t.crew.has_driver)
        );
    }

    #[test]
    fn signalling_research_raises_line_capacity() {
        let (mut world, data) = setup();
        world.money = 100_000.0;
        for _ in 0..3 {
            world.buy_locomotive(&data, "locomotion_1", LINE).unwrap();
        }
        assert_eq!(
            world.buy_locomotive(&data, "locomotion_1", LINE),
            Err(ActionError::LineFull)
        );
        world
            .research_done
            .extend(["hand_signals", "electric_telegraph"].map(String::from));
        assert_eq!(world.line_capacity(&data, LINE), 5);
        assert!(world.buy_locomotive(&data, "locomotion_1", LINE).is_ok());
        assert_eq!(
            world.buy_locomotive(&data, "locomotion_1", LINE),
            Err(ActionError::LineFull)
        );
    }

    #[test]
    fn selling_refunds_half_and_puts_passengers_back_on_the_platform() {
        let (mut world, data) = setup();
        world.trains[0].load.insert("tourist".into(), 7);
        assert_eq!(world.resale_value(&data, 1), Some(200.0));
        assert_eq!(world.sell_train(&data, 1), Ok(OrderOutcome::Done(200.0)));
        assert!(world.trains.is_empty());
        assert_eq!(world.money, 250.0);
        assert_eq!(world.stations["stockton"].waiting["tourist"], 7.0);
        assert_eq!(world.sell_train(&data, 1), Err(ActionError::UnknownTrain));
    }

    #[test]
    fn a_running_train_is_sold_when_it_arrives() {
        let (mut world, data) = setup();
        world.dispatch(&data, 1).unwrap();
        assert_eq!(world.sell_train(&data, 1), Ok(OrderOutcome::Scheduled));
        assert_eq!(world.trains[0].order, Some(TrainOrder::Sell));
        // On peut se raviser tant qu'il roule.
        world.cancel_order(1).unwrap();
        assert_eq!(world.trains[0].order, None);
        world.sell_train(&data, 1).unwrap();
        let events = advance(&mut world, &data, 4000.0);
        assert!(world.trains.is_empty(), "revendu à l'arrivée");
        assert!(events.iter().any(|e| matches!(
            e,
            SimEvent::TrainSold { train: 1, refund, .. } if *refund == 200.0
        )));
        // Son conducteur retourne en réserve.
        assert!(world.crew.iter().all(|m| m.train.is_none()));
    }

    #[test]
    fn replacing_keeps_line_crew_and_id_and_costs_the_difference() {
        let (mut world, data) = setup();
        world.research_done.insert("multitubular_boiler".into());
        assert_eq!(
            world.replace_train(&data, 1, "locomotion_1"),
            Err(ActionError::SameLocomotive)
        );
        assert_eq!(
            world.replace_train(&data, 1, "crampton"),
            Err(ActionError::WrongEra("Âge d'or de la vapeur".into()))
        );
        let rocket = data.locomotive("rocket").unwrap();
        let net = rocket.cost - 200.0;
        assert_eq!(world.replacement_cost(&data, 1, rocket), Some(net));
        assert_eq!(
            world.replace_train(&data, 1, "rocket"),
            Err(ActionError::NotEnoughMoney { needed: net })
        );
        // La ligne est pleine : remplacer n'y ajoute pas de train.
        world.money = 1e6;
        while world.trains_on(LINE) < world.line_capacity(&data, LINE) {
            world.buy_locomotive(&data, "locomotion_1", LINE).unwrap();
        }
        assert_eq!(world.trains_on(LINE), world.line_capacity(&data, LINE));
        let driver = world.crew_on(1, CrewRole::Driver).unwrap().id;
        let money = world.money;
        assert_eq!(
            world.replace_train(&data, 1, "rocket"),
            Ok(OrderOutcome::Done(net))
        );
        assert_eq!(world.money, money - net);
        let train = world.train(1).unwrap();
        assert_eq!((train.loco.as_str(), train.line.as_str()), ("rocket", LINE));
        assert_eq!(world.crew_on(1, CrewRole::Driver).unwrap().id, driver);
        assert_eq!(world.trains_on(LINE), world.line_capacity(&data, LINE));
    }

    #[test]
    fn a_running_train_is_replaced_on_arrival_and_cancelling_refunds() {
        let (mut world, data) = setup();
        world.research_done.insert("multitubular_boiler".into());
        world.money = 10_000.0;
        world.dispatch(&data, 1).unwrap();
        assert_eq!(
            world.replace_train(&data, 1, "rocket"),
            Ok(OrderOutcome::Scheduled)
        );
        let net = data.locomotive("rocket").unwrap().cost - 200.0;
        assert_eq!(world.money, 10_000.0 - net, "payé à la commande");
        world.cancel_order(1).unwrap();
        assert_eq!(world.money, 10_000.0, "remboursé à l'annulation");
        // Vendre après avoir commandé un remplacement l'annule et le rembourse.
        world.replace_train(&data, 1, "rocket").unwrap();
        world.sell_train(&data, 1).unwrap();
        assert_eq!(world.money, 10_000.0);
        assert_eq!(world.trains[0].order, Some(TrainOrder::Sell));

        world.replace_train(&data, 1, "rocket").unwrap();
        assert_eq!(world.trains[0].loco, "locomotion_1", "pas avant l'arrivée");
        let events = advance(&mut world, &data, 4000.0);
        assert_eq!(world.trains[0].loco, "rocket");
        assert_eq!(world.trains[0].order, None);
        assert!(events.iter().any(|e| matches!(
            e,
            SimEvent::TrainReplaced { train: 1, new, .. } if new == "rocket"
        )));
    }

    #[test]
    fn a_train_without_driver_stays_at_the_station() {
        let (mut world, data) = setup();
        let driver = world.crew_on(1, CrewRole::Driver).unwrap().id;
        world.assign_crew(driver, None).unwrap();
        assert_eq!(world.dispatch(&data, 1), Err(ActionError::NoDriver));
        assert_eq!(world.dispatch_all(&data), 0);
        // Ni les départs automatiques : le train reste à quai.
        world.research_done.insert("hand_signals".into());
        advance(&mut world, &data, 6.0 * 3600.0);
        assert!(world.trains[0].is_docked() && world.trips == 0);
        // Une machine achetée prend d'office un conducteur de la réserve…
        world.money = 1000.0;
        let id = world.buy_locomotive(&data, "locomotion_1", LINE).unwrap();
        assert_eq!(world.crew_on(id, CrewRole::Driver).unwrap().id, driver);
        assert!(world.crew_on(1, CrewRole::Driver).is_none());
        assert!(world.dispatch(&data, id).is_ok());
        // … et attend s'il n'y en a plus.
        let other = world.buy_locomotive(&data, "locomotion_1", LINE).unwrap();
        assert_eq!(world.dispatch(&data, other), Err(ActionError::NoDriver));
    }

    #[test]
    fn a_batch_of_locomotives_shares_the_reserve_drivers() {
        let (mut world, data) = setup();
        advance(&mut world, &data, 10.0); // tire le premier arrivage de candidats
        world.money = 1e6;
        world.drivers_hired = 1;
        let reserve: Vec<u32> = (0..2)
            .map(|_| {
                let index = world
                    .candidates
                    .iter()
                    .position(|c| c.role == CrewRole::Driver)
                    .unwrap();
                world.hire_crew(&data, index).unwrap()
            })
            .collect();
        // Le train 1 a déjà son conducteur : les recrues attendent en réserve.
        assert!(
            reserve
                .iter()
                .all(|id| world.crew_member(*id).unwrap().train.is_none())
        );
        let ids = world
            .buy_locomotives(&data, "locomotion_1", LINE, 3)
            .unwrap();
        let manned = ids
            .iter()
            .filter(|t| world.crew_on(**t, CrewRole::Driver).is_some())
            .count();
        assert_eq!(manned, 2, "deux conducteurs pour trois machines");
        // Jamais deux conducteurs sur un même train.
        for t in &world.trains {
            let drivers = world
                .crew
                .iter()
                .filter(|m| m.train == Some(t.id) && m.role == CrewRole::Driver)
                .count();
            assert!(drivers <= 1);
        }
    }

    #[test]
    fn dispatch_only_works_for_docked_trains() {
        let (mut world, data) = setup();
        assert_eq!(world.dispatch(&data, 99), Err(ActionError::UnknownTrain));
        world.dispatch(&data, 1).unwrap();
        assert_eq!(world.dispatch(&data, 1), Err(ActionError::NotAtStation));
        assert_eq!(world.dispatch_all(&data), 0);
    }

    #[test]
    fn the_fuel_market_opens_with_a_free_first_lot() {
        let (mut world, data) = setup();
        assert_eq!(world.fuel_stock("coal"), 0.0);
        let steam = data.era("steam").unwrap();
        world.passengers_carried = steam.unlock.passengers;
        world
            .research_done
            .extend(steam.unlock.research.iter().cloned());
        world.money = steam.unlock.cost + 1.0;
        world.advance_era(&data).unwrap();
        assert_eq!(world.era, "steam");
        assert_eq!(world.fuel_stock("coal"), data.fuel("coal").unwrap().lot);
        // La machine fait de l'eau, mais elle ne manque pas de charbon.
        assert!(matches!(
            world.dispatch(&data, 1),
            Err(ActionError::TakingWater { .. })
        ));
    }

    #[test]
    fn steam_engine_cannot_leave_while_taking_water() {
        let (mut world, data) = setup();
        enter(&mut world, "steam");
        world.fuel.insert("coal".into(), 100.0);
        assert_eq!(
            world.dispatch(&data, 1),
            Err(ActionError::TakingWater { minutes: 8 })
        );
        assert_eq!(world.dispatch_all(&data), 0);
        advance(&mut world, &data, 5.0 * 60.0);
        assert_eq!(
            world.dispatch(&data, 1),
            Err(ActionError::TakingWater { minutes: 3 })
        );
        advance(&mut world, &data, 3.0 * 60.0);
        assert_eq!(world.dispatch(&data, 1), Ok(()));
    }

    #[test]
    fn fare_level_and_dwell_are_clamped() {
        let (mut world, data) = setup();
        assert_eq!(world.set_fare_level(&data, LINE, 1.3000001), Ok(1.3));
        assert_eq!(world.set_fare_level(&data, LINE, 9.0), Ok(2.0));
        assert_eq!(world.set_fare_level(&data, LINE, 0.0), Ok(0.5));
        assert_eq!(world.set_dwell(&data, LINE, 12.4), Ok(12.0));
        assert_eq!(world.set_dwell(&data, LINE, 1.0), Ok(5.0));
        assert_eq!(world.set_dwell(&data, LINE, 999.0), Ok(120.0));
        assert_eq!(
            world.set_fare_level(&data, "nope", 1.0),
            Err(ActionError::UnknownLine)
        );
        assert_eq!(world.lines[LINE].dwell_min, 120.0);
    }

    #[test]
    fn manual_dispatch_records_lateness() {
        let (mut world, data) = setup();
        world.clock = world.trains[0].sched_departure + 600.0;
        world.dispatch(&data, 1).unwrap();
        assert_eq!(world.trains[0].late_s, 600.0);
        assert_eq!(world.trains[0].trip.unwrap().started, world.clock);
    }

    #[test]
    fn research_rules() {
        let (mut world, data) = setup();
        assert!(matches!(
            world.start_research(&data, "hand_signals"),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        world.money = 1e9;
        world.start_research(&data, "hand_signals").unwrap();
        assert_eq!(world.money, 1e9 - 500.0);
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
        // Prérequis, puis époque.
        assert_eq!(
            world.start_research(&data, "variable_expansion"),
            Err(ActionError::Locked("Chaudière multitubulaire".into()))
        );
        assert_eq!(
            world.start_research(&data, "air_brake"),
            Err(ActionError::WrongEra("Âge d'or de la vapeur".into()))
        );
    }

    #[test]
    fn repair_cost_scales_with_damage_and_with_the_era() {
        let (mut world, data) = setup();
        assert_eq!(
            world.repair_line(&data, LINE),
            Err(ActionError::NothingToRepair)
        );
        world.lines.get_mut(LINE).unwrap().condition = 0.5;
        // Moitié de 19 km à 20 € le km.
        assert_eq!(world.repair_cost(&data, LINE), Some(190.0));
        assert!(matches!(
            world.repair_line(&data, LINE),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        world.money = 200.0;
        assert_eq!(world.repair_line(&data, LINE), Ok(190.0));
        assert_eq!(world.money, 10.0);
        assert_eq!(world.lines[LINE].condition, 1.0);

        world.lines.get_mut(LINE).unwrap().condition = 0.5;
        enter(&mut world, "steam");
        assert_eq!(world.repair_cost(&data, LINE), Some(190.0 * 1.3));
    }

    #[test]
    fn next_era_needs_research_passengers_and_money() {
        let (mut world, data) = setup();
        let status = world.next_era(&data).unwrap();
        assert_eq!(status.era.id, "steam");
        assert_eq!(status.research.len(), 3);
        assert!(!status.ready() && !status.affordable);
        assert_eq!(world.advance_era(&data), Err(ActionError::EraNotReady));

        world
            .research_done
            .extend(["hand_signals", "multitubular_boiler", "covered_carriages"].map(String::from));
        assert_eq!(
            world.advance_era(&data),
            Err(ActionError::EraNotReady),
            "il manque les voyageurs"
        );
        world.passengers_carried = 15_000;
        assert_eq!(
            world.advance_era(&data),
            Err(ActionError::NotEnoughMoney { needed: 40_000.0 })
        );
        world.money = 41_000.0;
        world.clock = 123_456.0;
        assert_eq!(world.advance_era(&data), Ok("steam".to_string()));
        assert_eq!((world.money, world.era_started), (1000.0, 123_456.0));
        assert_eq!(world.year(&data), 1850);

        enter(&mut world, "future");
        assert!(world.next_era(&data).is_none());
        assert_eq!(world.advance_era(&data), Err(ActionError::LastEra));
    }

    #[test]
    fn upgrades_follow_research_era_and_order() {
        let (mut world, data) = setup();
        world.money = 1e9;
        assert_eq!(
            world.buy_upgrade(&data, LINE, "steel_rails"),
            Err(ActionError::WrongEra("Âge d'or de la vapeur".into()))
        );
        enter(&mut world, "apogee");
        assert_eq!(
            world.buy_upgrade(&data, LINE, "steel_rails"),
            Err(ActionError::Locked("Acier Bessemer".into()))
        );
        world
            .research_done
            .extend(["bessemer_steel", "welded_rails"].map(String::from));
        assert_eq!(
            world.buy_upgrade(&data, LINE, "heavy_track"),
            Err(ActionError::NeedsUpgrade("Rails en acier".into()))
        );
        // 19 km à 400 € le km.
        assert_eq!(world.buy_upgrade(&data, LINE, "steel_rails"), Ok(7600.0));
        assert_eq!(
            world.buy_upgrade(&data, LINE, "steel_rails"),
            Err(ActionError::AlreadyInstalled)
        );
        assert!(world.buy_upgrade(&data, LINE, "heavy_track").is_ok());
        assert_eq!(
            world.buy_upgrade(&data, LINE, "nope"),
            Err(ActionError::UnknownUpgrade)
        );
        assert_eq!(world.lines[LINE].upgrades.len(), 2);
    }

    #[test]
    fn network_grows_from_served_stations() {
        let (mut world, data) = setup();
        world.money = 1e9;
        assert_eq!(
            world.buy_locomotive(&data, "locomotion_1", "darlington_york"),
            Err(ActionError::LineNotBuilt)
        );
        // York – Leeds ne touche encore aucune gare desservie.
        assert_eq!(
            world.build_line(&data, "york_leeds"),
            Err(ActionError::NotConnected)
        );
        // 70 km à 50 € le km.
        assert_eq!(world.build_line(&data, "darlington_york"), Ok(3500.0));
        assert_eq!(
            world.build_line(&data, "darlington_york"),
            Err(ActionError::AlreadyOwned)
        );
        assert!(world.is_served(&data, "york"));
        assert!(world.build_line(&data, "york_leeds").is_ok());
        assert!(
            world
                .buy_locomotive(&data, "locomotion_1", "darlington_york")
                .is_ok()
        );
        assert_eq!(
            world.build_line(&data, "nope"),
            Err(ActionError::UnknownLine)
        );
    }

    #[test]
    fn a_new_country_needs_a_concession_and_starts_from_its_capital() {
        let (mut world, data) = setup();
        world.money = 1e9;
        assert_eq!(
            world.build_line(&data, "paris_lyon"),
            Err(ActionError::NoConcession("France".into()))
        );
        assert_eq!(world.buy_concession(&data, "france"), Ok(8000.0));
        assert_eq!(
            world.buy_concession(&data, "france"),
            Err(ActionError::AlreadyOwned)
        );
        assert_eq!(
            world.build_line(&data, "lyon_marseille"),
            Err(ActionError::NotConnected)
        );
        // Relief français : 430 km × 50 € × 1,1.
        let cost = world.build_line(&data, "paris_lyon").unwrap();
        assert!((cost - 430.0 * 50.0 * 1.1).abs() < 1e-6, "{cost}");
        assert!(world.build_line(&data, "lyon_marseille").is_ok());

        // L'Allemagne n'ouvre qu'à l'âge d'or ; la ligne Londres – Paris qu'à l'apogée.
        assert_eq!(
            world.buy_concession(&data, "germany"),
            Err(ActionError::WrongEra("Âge d'or de la vapeur".into()))
        );
        world.build_line(&data, "darlington_york").unwrap();
        world.build_line(&data, "york_london").unwrap();
        assert_eq!(
            world.build_line(&data, "london_paris"),
            Err(ActionError::WrongEra("Apogée de la vapeur".into()))
        );
        enter(&mut world, "apogee");
        assert!(world.build_line(&data, "london_paris").is_ok());
        assert_eq!(
            world.buy_concession(&data, "atlantis"),
            Err(ActionError::UnknownCountry)
        );
    }

    #[test]
    fn a_different_gauge_needs_variable_gauge_research() {
        let (mut world, data) = setup();
        world.money = 1e12;
        enter(&mut world, "modern");
        for country in ["spain", "russia"] {
            assert_eq!(
                world.buy_concession(&data, country),
                Err(ActionError::Locked("Écartement variable".into())),
                "{country}"
            );
        }
        world.research_done.insert("variable_gauge".into());
        assert_eq!(world.buy_concession(&data, "spain"), Ok(30_000_000.0));
        assert!(world.build_line(&data, "madrid_barcelona").is_ok());
        // Même écartement que le pays de départ : aucune recherche requise.
        world.research_done.clear();
        assert_eq!(world.buy_concession(&data, "germany"), Ok(400_000.0));
        // Sans la recherche, la ligne transfrontalière vers l'Espagne reste impossible.
        world.buy_concession(&data, "france").unwrap();
        world.build_line(&data, "paris_bordeaux").unwrap();
        assert_eq!(
            world.build_line(&data, "bordeaux_madrid"),
            Err(ActionError::Locked("Écartement variable".into()))
        );
    }

    #[test]
    fn staff_is_hired_once_and_paid_by_the_hour() {
        let (mut world, data) = setup();
        world.money = 10_000.0;
        assert_eq!(
            world.hire(&data, "track_gangs"),
            Err(ActionError::Locked("Rails en fer laminé".into()))
        );
        assert_eq!(
            world.hire(&data, "supply_office"),
            Err(ActionError::WrongEra("Âge d'or de la vapeur".into()))
        );
        world.research_done.insert("wrought_iron_rails".into());
        assert_eq!(world.hire(&data, "track_gangs"), Ok(4000.0));
        assert_eq!(
            world.hire(&data, "track_gangs"),
            Err(ActionError::AlreadyOwned)
        );
        assert_eq!(world.hire(&data, "nope"), Err(ActionError::UnknownStaff));

        // Aucun train ne roule : seule la paie sort de la caisse, 15 € par heure de jeu.
        let before = world.money;
        advance(&mut world, &data, 10.0 * 3600.0);
        assert!(
            (before - world.money - 150.0).abs() < 1e-6,
            "{}",
            before - world.money
        );
    }

    #[test]
    fn track_gangs_repair_worn_lines_when_money_allows() {
        let (mut world, data) = setup();
        world.staff.insert("track_gangs".into());
        world.lines.get_mut(LINE).unwrap().condition = 0.5;
        // Pas assez d'argent : la voie reste usée, sans dette.
        world.money = 100.0;
        advance(&mut world, &data, 60.0);
        assert_eq!(world.lines[LINE].condition, 0.5);

        world.money = 1000.0;
        let events = advance(&mut world, &data, 10.0);
        assert!(events.iter().any(|e| matches!(e, crate::sim::SimEvent::Repaired { cost, .. } if (*cost - 190.0).abs() < 1e-6)));
        assert_eq!(world.lines[LINE].condition, 1.0);
        // Une ligne en projet n'est jamais « réparée ».
        assert!(!world.is_built("darlington_york"));
    }

    #[test]
    fn supply_office_restocks_coal_before_it_runs_out() {
        let (mut world, data) = setup();
        enter(&mut world, "steam");
        world
            .research_done
            .extend(["hand_signals", "water_troughs"].map(String::from));
        world.staff.insert("supply_office".into());
        world.money = 100_000.0;
        let events = advance(&mut world, &data, 30.0 * 3600.0);
        let bought = events
            .iter()
            .filter(|e| matches!(e, crate::sim::SimEvent::FuelBought { .. }))
            .count();
        let out = events
            .iter()
            .filter(|e| matches!(e, crate::sim::SimEvent::FuelOut { .. }))
            .count();
        assert!(bought >= 1, "aucun achat en trente heures");
        assert_eq!(
            out, 0,
            "le stock ne doit jamais tomber à zéro une fois le service engagé"
        );
        assert!(world.fuel_stock("coal") > 0.0);

        // Sans le service, rien n'est acheté et le stock reste vide.
        let (mut manual, data) = setup();
        enter(&mut manual, "steam");
        manual
            .research_done
            .extend(["hand_signals", "water_troughs"].map(String::from));
        advance(&mut manual, &data, 30.0 * 3600.0);
        assert_eq!(manual.fuel_stock("coal"), 0.0);
    }

    #[test]
    fn timetable_office_shortens_stops_at_crowded_stations() {
        let run = |office: bool| {
            let (mut world, data) = setup();
            world.research_done.insert("hand_signals".into());
            if office {
                world.staff.insert("timetable_office".into());
            }
            // Darlington est bondée ; le train y arrive.
            world
                .stations
                .get_mut("darlington")
                .unwrap()
                .waiting
                .insert("commuter".into(), 150.0);
            world.dispatch(&data, 1).unwrap();
            advance(&mut world, &data, 3200.0);
            assert!(world.trains[0].is_docked());
            world.trains[0].sched_departure - world.clock
        };
        let (normal, short) = (run(false), run(true));
        // Arrêt prévu de 20 minutes, dont une partie est déjà écoulée depuis l'arrivée.
        assert!(normal > 900.0, "{normal}");
        assert!(
            short > 0.0 && short < normal * 0.6,
            "{short} contre {normal}"
        );
    }

    #[test]
    fn a_company_can_be_founded_in_any_starting_country() {
        let data = GameData::embedded().unwrap();
        assert_eq!(data.starting_countries().count(), 5);
        for country in data.starting_countries() {
            let mut world = World {
                choosing_home: true,
                ..World::new(&data)
            };
            world.found_company_in(&data, &country.id).unwrap();
            let line = country.start_line.as_deref().unwrap();
            assert_eq!(world.home, country.id);
            assert_eq!(world.trains.len(), 1);
            assert_eq!(world.trains[0].line, line);
            assert!(world.is_built(line));
            assert_eq!(world.countries.iter().collect::<Vec<_>>(), [&country.id]);
            // Le choix est fait : plus moyen de déménager sans nationaliser.
            assert_eq!(
                world.found_company_in(&data, "uk"),
                Err(ActionError::CompanyFounded)
            );
        }
    }

    #[test]
    fn a_broad_gauge_home_needs_variable_gauge_to_go_abroad() {
        let data = GameData::embedded().unwrap();
        let mut world = World {
            choosing_home: true,
            ..World::new(&data)
        };
        world.found_company_in(&data, "spain").unwrap();
        world.money = 1e9;
        world.era = "modern".into();
        let uk = data.country("uk").unwrap();
        assert!(matches!(
            world.concession_blocker(&data, uk),
            Some(ActionError::Locked(_))
        ));
        world.research_done.insert("variable_gauge".into());
        assert_eq!(world.buy_concession(&data, "uk"), Ok(uk.concession_cost));
    }

    #[test]
    fn a_new_home_must_connect_its_first_line_to_the_network() {
        let data = GameData::embedded().unwrap();
        let mut world = World {
            choosing_home: true,
            ..World::new(&data)
        };
        world.found_company_in(&data, "france").unwrap();
        world.money = 1e9;
        // Paris n'est pas reliée à Saint-Étienne ; Lyon, si.
        let paris_lille = data.line("paris_lille").unwrap();
        assert_eq!(
            world.line_blocker(&data, paris_lille),
            Some(ActionError::NotConnected)
        );
        let lyon = data.line("saint_etienne_lyon").unwrap();
        assert_eq!(world.line_blocker(&data, lyon), None);
    }

    #[test]
    fn nationalising_resets_the_company_but_keeps_prestige() {
        let (mut world, data) = setup();
        world.lifetime_income = 5_000_000.0;
        enter(&mut world, "steam");
        let ids: Vec<String> = data.research.iter().map(|r| r.id.clone()).collect();
        world.research_done.extend(ids[1..].iter().cloned());
        assert_eq!(
            world.prestige_gain(&data),
            0,
            "il faut d'abord terminer tout l'arbre de recherche"
        );
        assert_eq!(world.nationalise(&data), Err(ActionError::PrestigeNotReady));
        world.research_done.insert(ids[0].clone());
        assert_eq!(world.prestige_gain(&data), 1);

        world.money = 123_456.0;
        world.research_done.insert("hand_signals".into());
        world.staff.insert("track_gangs".into());
        world.lines.get_mut("darlington_york").unwrap().built = true;
        assert_eq!(world.nationalise(&data), Ok(1));
        assert_eq!(
            (
                world.prestige.points,
                world.prestige.earned,
                world.prestige.resets
            ),
            (1, 1, 1)
        );
        assert_eq!(
            (world.era.as_str(), world.money),
            ("pioneers", data.balance.start_money)
        );
        assert!(world.research_done.is_empty() && world.staff.is_empty());
        assert!(!world.is_built("darlington_york") && world.is_built(LINE));
        assert_eq!((world.trains.len(), world.lifetime_income), (1, 0.0));
        assert_eq!(world.prestige_gain(&data), 0);
    }

    #[test]
    fn prestige_bonuses_cost_more_each_level_and_change_the_game() {
        let (mut world, data) = setup();
        assert_eq!(
            world.buy_bonus(&data, "reputation"),
            Err(ActionError::NotEnoughPoints { needed: 2 })
        );
        world.prestige.points = 100;
        assert_eq!(world.buy_bonus(&data, "reputation"), Ok(2));
        assert_eq!(world.buy_bonus(&data, "reputation"), Ok(3));
        assert_eq!(world.prestige.points, 95);
        assert_eq!(
            world.buy_bonus(&data, "nope"),
            Err(ActionError::UnknownBonus)
        );
        for id in [
            "engineering_school",
            "works_department",
            "night_shift",
            "shareholders",
        ] {
            world.buy_bonus(&data, id).unwrap();
        }

        // Recettes +20 %, recherche +20 %, construction -8 %.
        let m = world.modifiers(&data);
        assert!((m.fare - 1.2).abs() < 1e-9 && (m.research_speed - 1.2).abs() < 1e-9);
        assert!((m.build_cost - 0.92).abs() < 1e-9);
        let line = data.line("darlington_york").unwrap();
        assert!((world.line_cost(&data, line) - 3500.0 * 0.92).abs() < 1e-6);

        world.money = 1000.0;
        world.start_research(&data, "hand_signals").unwrap();
        advance(&mut world, &data, 4.0 * 3600.0 / 1.2 + 10.0);
        assert!(
            world.research_done.contains("hand_signals"),
            "la recherche va 20 % plus vite"
        );

        // Hors ligne : 60 % au lieu de 50 %.
        let report = crate::sim::catch_up(&mut world.clone(), &data, 1000.0);
        assert!((report.simulated_s - 600.0).abs() < 1e-9);

        // Le capital de départ suit à la prochaine nationalisation, et les niveaux restent.
        world
            .research_done
            .extend(data.research.iter().map(|r| r.id.clone()));
        world.nationalise(&data).unwrap();
        assert_eq!(world.money, data.balance.start_money + 2000.0);
        assert_eq!(world.prestige.level("reputation"), 2);

        world.prestige.levels.insert("night_shift".into(), 5);
        assert_eq!(
            world.buy_bonus(&data, "night_shift"),
            Err(ActionError::MaxLevel)
        );
    }

    #[test]
    fn fuel_is_bought_by_the_lot_at_the_day_price() {
        let (mut world, data) = setup();
        enter(&mut world, "steam");
        world.money = 10_000.0;
        let coal = data.fuel("coal").unwrap();
        let price = coal.price(world.clock, world.era(&data));
        let cost = world.buy_fuel(&data, "coal", 3).unwrap();
        assert!((cost - 30.0 * price).abs() < 1e-9);
        assert_eq!(world.fuel_stock("coal"), 30.0);
        assert!((world.money - (10_000.0 - cost)).abs() < 1e-9);
        assert_eq!(
            world.buy_fuel(&data, "electricity", 1),
            Err(ActionError::NotStockable)
        );
        assert_eq!(
            world.buy_fuel(&data, "peat", 1),
            Err(ActionError::UnknownFuel)
        );
        world.money = 1.0;
        assert!(matches!(
            world.buy_fuel(&data, "coal", 1),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        // Une Locomotion à 24 km/h brûle 12 kg par km.
        assert!((world.fuel_burn_per_hour(&data, "coal") - 24.0 * 0.012).abs() < 1e-9);
        assert_eq!(world.fuel_burn_per_hour(&data, "diesel"), 0.0);
    }

    #[test]
    fn rainhill_choice_grants_the_boiler_and_the_prize() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        assert_eq!(world.resolve_event(&data, 0), Err(ActionError::NoEvent));
        world.event_pending = Some("rainhill_trials".into());
        // La recherche était en cours : elle est offerte et remboursée.
        world.money = 1000.0;
        world.start_research(&data, "multitubular_boiler").unwrap();
        world.resolve_event(&data, 0).unwrap();
        assert_eq!(world.money, 1000.0 + 500.0);
        assert!(world.research_done.contains("multitubular_boiler"));
        assert!(world.research_active.is_none() && world.event_pending.is_none());
        assert!(world.events_done.contains("rainhill_trials"));
        // Un événement joué ne revient pas.
        world.passengers_carried = 10_000;
        let events = advance(&mut world, &data, 60.0);
        assert!(!events.iter().any(
            |e| matches!(e, crate::sim::SimEvent::EventReady { id } if id == "rainhill_trials")
        ));
    }

    #[test]
    fn event_effects_touch_satisfaction_track_and_money() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.event_pending = Some("orient_express".into());
        assert_eq!(
            world.resolve_event(&data, 0),
            Err(ActionError::NotEnoughMoney { needed: 150_000.0 })
        );
        assert!(
            world.event_pending.is_some(),
            "un choix impayable ne consomme pas l'événement"
        );
        world.money = 151_000.0;
        world.resolve_event(&data, 0).unwrap();
        assert_eq!(world.money, 1000.0);
        assert_eq!(world.satisfaction_of("tourist"), 0.8);
        assert_eq!(world.satisfaction_of("business"), 0.7);
        assert_eq!(world.satisfaction_of("commuter"), 0.5);

        world.event_pending = Some("landes_record".into());
        world.resolve_event(&data, 0).unwrap();
        assert!(
            (world.lines[LINE].condition - 0.55).abs() < 1e-9,
            "la voie sort tordue du record"
        );
        assert!((world.satisfaction_of("commuter") - 0.6).abs() < 1e-9);
        assert_eq!(world.resolve_event(&data, 5), Err(ActionError::NoEvent));
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
        world.era = "bronze_age".into();
        world.fuel.insert("peat".into(), 3.0);
        world.events_done.insert("gone".into());
        world.event_pending = Some("gone".into());
        world
            .lines
            .get_mut(LINE)
            .unwrap()
            .upgrades
            .insert("gone".into());
        world.reconcile(&data);
        assert!(world.trains.is_empty());
        assert!(world.research_done.is_empty());
        assert!(world.research_active.is_none());
        assert_eq!(world.stations.len(), data.stations().count());
        assert_eq!(world.era, "pioneers");
        assert!(
            world.fuel.is_empty() && world.events_done.is_empty() && world.event_pending.is_none()
        );
        assert!(world.lines[LINE].upgrades.is_empty());
    }
}
