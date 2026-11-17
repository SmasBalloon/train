//! Validation des données : identifiants uniques, références existantes,
//! valeurs plausibles. Les messages visent quelqu'un qui écrit un mod.

use std::collections::HashSet;

use anyhow::{Result, bail};

use super::{Effect, EventEffect, GameData, StaffRole, UpgradeEffect};

fn unique<'a>(what: &str, ids: impl Iterator<Item = &'a String>) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.is_empty() {
            bail!("{what} : identifiant vide");
        }
        if !seen.insert(id) {
            bail!("{what} : identifiant « {id} » en double");
        }
    }
    Ok(())
}

fn positive(what: &str, value: f64) -> Result<()> {
    if !(value.is_finite() && value > 0.0) {
        bail!("{what} doit être strictement positif (valeur : {value})");
    }
    Ok(())
}

fn non_negative(what: &str, value: f64) -> Result<()> {
    if !(value.is_finite() && value >= 0.0) {
        bail!("{what} ne peut pas être négatif (valeur : {value})");
    }
    Ok(())
}

fn ratio(what: &str, value: f64) -> Result<()> {
    if !(0.0..=1.0).contains(&value) {
        bail!("{what} doit être compris entre 0 et 1 (valeur : {value})");
    }
    Ok(())
}

impl GameData {
    pub(super) fn validate(&self) -> Result<()> {
        unique("eras.toml", self.eras.iter().map(|e| &e.id))?;
        unique("fuels.toml", self.fuels.iter().map(|f| &f.id))?;
        unique("traffic.toml", self.traffic.iter().map(|c| &c.id))?;
        unique("research.toml", self.research.iter().map(|r| &r.id))?;
        unique("locomotives.toml", self.locomotives.iter().map(|l| &l.id))?;
        unique("upgrades.toml", self.upgrades.iter().map(|u| &u.id))?;
        unique("events.toml", self.events.iter().map(|e| &e.id))?;
        unique(
            "countries.toml (pays)",
            self.countries.iter().map(|c| &c.id),
        )?;
        unique("countries.toml (gares)", self.stations().map(|s| &s.id))?;
        unique("countries.toml (lignes)", self.lines().map(|l| &l.id))?;

        self.validate_eras()?;
        self.validate_fuels_and_traffic()?;
        self.validate_research()?;
        self.validate_locomotives()?;
        self.validate_upgrades()?;
        self.validate_countries()?;
        self.validate_events()?;
        self.validate_staff_and_prestige()?;
        self.validate_balance()
    }

    /// Rang de l'époque `id`, ou une erreur nommant ce qui y fait référence.
    fn era_rank(&self, what: &str, id: &str) -> Result<usize> {
        match self.era_index(id) {
            Some(i) => Ok(i),
            None => bail!("{what} : époque « {id} » inconnue"),
        }
    }

    fn check_research_refs(&self, what: &str, ids: &[String]) -> Result<()> {
        for id in ids {
            if self.research(id).is_none() {
                bail!("{what} : recherche « {id} » inconnue");
            }
        }
        Ok(())
    }

    fn validate_eras(&self) -> Result<()> {
        if self.eras.is_empty() {
            bail!("eras.toml : il faut au moins une époque");
        }
        for (i, era) in self.eras.iter().enumerate() {
            let what = format!("époque « {} »", era.id);
            positive(&format!("{what} : demand_mult"), era.demand_mult)?;
            positive(&format!("{what} : fare_mult"), era.fare_mult)?;
            positive(&format!("{what} : cost_mult"), era.cost_mult)?;
            positive(&format!("{what} : build_mult"), era.build_mult)?;
            non_negative(
                &format!("{what} : water_stop_min"),
                era.rules.water_stop_min,
            )?;
            non_negative(&format!("{what} : unlock.cost"), era.unlock.cost)?;
            if era.to < era.from {
                bail!("{what} : l'année de fin précède l'année de début");
            }
            self.check_research_refs(&what, &era.unlock.research)?;
            // On ne peut exiger, pour entrer dans une époque, que des recherches antérieures.
            for id in &era.unlock.research {
                let research = self.research(id).expect("référence vérifiée");
                if self.era_rank(&what, &research.era)? >= i {
                    bail!(
                        "{what} : la recherche requise « {id} » n'est accessible qu'à partir de cette époque"
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_fuels_and_traffic(&self) -> Result<()> {
        for f in &self.fuels {
            let what = format!("énergie « {} »", f.id);
            positive(&format!("{what} : base_price"), f.base_price)?;
            non_negative(&format!("{what} : lot"), f.lot)?;
            if !(0.0..1.0).contains(&f.volatility) {
                bail!("{what} : volatility doit être comprise entre 0 et 1 (exclu)");
            }
            if f.stock && f.lot <= 0.0 {
                bail!("{what} : une énergie stockable doit avoir un lot d'achat positif");
            }
        }

        if self.traffic.is_empty() {
            bail!("traffic.toml : il faut au moins une catégorie de trafic");
        }
        for c in &self.traffic {
            let what = format!("catégorie « {} »", c.id);
            non_negative(&format!("{what} : share"), c.share)?;
            non_negative(&format!("{what} : fare_mult"), c.fare_mult)?;
            non_negative(&format!("{what} : elasticity"), c.elasticity)?;
            non_negative(&format!("{what} : comfort_weight"), c.comfort_weight)?;
            non_negative(
                &format!("{what} : punctuality_weight"),
                c.punctuality_weight,
            )?;
            non_negative(&format!("{what} : crowding_weight"), c.crowding_weight)?;
            if c.hourly.len() != 24 {
                bail!(
                    "{what} : hourly doit contenir 24 valeurs (une par heure), pas {}",
                    c.hourly.len()
                );
            }
            for v in &c.hourly {
                non_negative(&format!("{what} : hourly"), *v)?;
            }
        }
        Ok(())
    }

    fn validate_research(&self) -> Result<()> {
        for r in &self.research {
            let what = format!("recherche « {} »", r.id);
            let rank = self.era_rank(&what, &r.era)?;
            non_negative(&format!("{what} : cost"), r.cost)?;
            positive(&format!("{what} : duration_h"), r.duration_h)?;
            self.check_research_refs(&what, &r.requires)?;
            for req in &r.requires {
                if req == &r.id {
                    bail!("{what} : se requiert elle-même");
                }
                let required = self.research(req).expect("référence vérifiée");
                if self.era_rank(&what, &required.era)? > rank {
                    bail!("{what} : le prérequis « {req} » appartient à une époque ultérieure");
                }
            }
            for e in &r.effects {
                match e {
                    Effect::SpeedMult { value }
                    | Effect::CapacityMult { value }
                    | Effect::FareMult { value }
                    | Effect::DemandMult { value }
                    | Effect::TrackWearMult { value }
                    | Effect::FuelMult { value }
                    | Effect::WeightMult { value }
                    | Effect::SignalSpeed { value } => {
                        positive(&format!("{what} : valeur d'effet"), *value)?
                    }
                    Effect::BrakeAdd { value } => {
                        non_negative(&format!("{what} : brake_add"), *value)?
                    }
                    Effect::Comfort { value } => {
                        if !(value.is_finite() && (-1.0..=1.0).contains(value)) {
                            bail!("{what} : le bonus de confort doit être compris entre -1 et 1");
                        }
                    }
                    Effect::LineCapacity { .. }
                    | Effect::AutoDispatch
                    | Effect::NoWaterStop
                    | Effect::VariableGauge => {}
                }
            }
        }
        Ok(())
    }

    fn validate_locomotives(&self) -> Result<()> {
        for l in &self.locomotives {
            let what = format!("locomotive « {} »", l.id);
            self.era_rank(&what, &l.era)?;
            positive(&format!("{what} : top_speed_kmh"), l.top_speed_kmh)?;
            positive(&format!("{what} : power_kw"), l.power_kw)?;
            positive(&format!("{what} : weight_t"), l.weight_t)?;
            non_negative(&format!("{what} : cost"), l.cost)?;
            non_negative(&format!("{what} : upkeep_per_h"), l.upkeep_per_h)?;
            non_negative(&format!("{what} : fuel_per_km"), l.fuel_per_km)?;
            non_negative(&format!("{what} : track_wear"), l.track_wear)?;
            if l.capacity == 0 {
                bail!("{what} : capacity doit être au moins 1");
            }
            if self.fuel(&l.fuel).is_none() {
                bail!("{what} : énergie « {} » inconnue", l.fuel);
            }
            self.check_research_refs(&what, &l.requires)?;
            if let Some(upgrade) = &l.requires_upgrade
                && self.upgrade(upgrade).is_none()
            {
                bail!("{what} : amélioration requise « {upgrade} » inconnue");
            }
        }
        Ok(())
    }

    fn validate_upgrades(&self) -> Result<()> {
        for u in &self.upgrades {
            let what = format!("amélioration « {} »", u.id);
            self.era_rank(&what, &u.era)?;
            non_negative(&format!("{what} : cost_per_km"), u.cost_per_km)?;
            self.check_research_refs(&what, &u.requires)?;
            if let Some(previous) = &u.requires_upgrade {
                if previous == &u.id {
                    bail!("{what} : se requiert elle-même");
                }
                if self.upgrade(previous).is_none() {
                    bail!("{what} : amélioration préalable « {previous} » inconnue");
                }
            }
            for e in &u.effects {
                match e {
                    UpgradeEffect::LineSpeed { value, .. } | UpgradeEffect::WearMult { value } => {
                        positive(&format!("{what} : valeur d'effet"), *value)?
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_countries(&self) -> Result<()> {
        for c in &self.countries {
            positive(&format!("pays « {} » : build_cost", c.id), c.build_cost)?;
            non_negative(
                &format!("pays « {} » : concession_cost", c.id),
                c.concession_cost,
            )?;
            if let Some(era) = &c.era {
                self.era_rank(&format!("pays « {} »", c.id), era)?;
            }
            if c.stations.is_empty() {
                bail!(
                    "pays « {} » : il faut au moins une gare (la première est la capitale)",
                    c.id
                );
            }
            for (class, mult) in &c.demand {
                if self.class(class).is_none() {
                    bail!(
                        "pays « {} » : catégorie de trafic « {class} » inconnue dans demand",
                        c.id
                    );
                }
                non_negative(&format!("pays « {} » : demand.{class}", c.id), *mult)?;
            }
            for l in &c.lines {
                let what = format!("ligne « {} »", l.id);
                positive(&format!("{what} : length_km"), l.length_km)?;
                if let Some(era) = &l.era {
                    self.era_rank(&what, era)?;
                }
                if l.max_trains == 0 {
                    bail!("{what} : max_trains doit être au moins 1");
                }
                if l.from == l.to {
                    bail!("{what} : les deux extrémités sont la même gare");
                }
                for end in [&l.from, &l.to] {
                    if self.station(end).is_none() {
                        bail!("{what} : gare « {end} » inconnue");
                    }
                }
            }
            for d in &c.decor {
                if d.path.len() < 2 {
                    bail!(
                        "pays « {} » : un décor doit avoir au moins deux points",
                        c.id
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_events(&self) -> Result<()> {
        for e in &self.events {
            let what = format!("événement « {} »", e.id);
            if e.choices.is_empty() {
                bail!("{what} : il faut au moins un choix");
            }
            if let Some(era) = &e.trigger.era {
                self.era_rank(&what, era)?;
            }
            self.check_research_refs(&what, &e.trigger.research)?;
            for loco in &e.trigger.owns {
                if self.locomotive(loco).is_none() {
                    bail!("{what} : locomotive « {loco} » inconnue");
                }
            }
            for effect in e.choices.iter().flat_map(|c| &c.effects) {
                match effect {
                    EventEffect::GrantResearch { target } => {
                        self.check_research_refs(&what, std::slice::from_ref(target))?
                    }
                    EventEffect::Satisfaction {
                        class: Some(class), ..
                    } if self.class(class).is_none() => {
                        bail!("{what} : catégorie de trafic « {class} » inconnue");
                    }
                    EventEffect::Money { value }
                    | EventEffect::Satisfaction { value, .. }
                    | EventEffect::TrackCondition { value }
                        if !value.is_finite() =>
                    {
                        bail!("{what} : valeur d'effet invalide");
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn validate_staff_and_prestige(&self) -> Result<()> {
        unique("staff.toml", self.staff.iter().map(|s| &s.id))?;
        unique("prestige.toml", self.bonuses.iter().map(|b| &b.id))?;
        for s in &self.staff {
            let what = format!("service « {} »", s.id);
            self.era_rank(&what, &s.era)?;
            self.check_research_refs(&what, &s.requires)?;
            non_negative(&format!("{what} : cost"), s.cost)?;
            non_negative(&format!("{what} : salary_per_h"), s.salary_per_h)?;
            match s.role {
                StaffRole::AutoRepair { threshold } => {
                    ratio(&format!("{what} : threshold"), threshold)?
                }
                StaffRole::AutoFuel {
                    reserve_h,
                    max_price_ratio,
                } => {
                    positive(&format!("{what} : reserve_h"), reserve_h)?;
                    positive(&format!("{what} : max_price_ratio"), max_price_ratio)?;
                }
                StaffRole::AutoTimetable { crowd_factor } => {
                    ratio(&format!("{what} : crowd_factor"), crowd_factor)?
                }
            }
        }
        for b in &self.bonuses {
            let what = format!("bonus « {} »", b.id);
            positive(&format!("{what} : value"), b.value)?;
            if b.cost == 0 || b.max_level == 0 || b.cost_growth < 1.0 || !b.cost_growth.is_finite()
            {
                bail!("{what} : il faut cost >= 1, max_level >= 1 et cost_growth >= 1");
            }
        }
        let c = &self.crew;
        non_negative("crew : base_wage", c.base_wage)?;
        non_negative("crew : skill_wage", c.skill_wage)?;
        non_negative("crew : hire_fee_hours", c.hire_fee_hours)?;
        non_negative("crew : xp_per_km", c.xp_per_km)?;
        non_negative("crew : training_cost", c.training_cost)?;
        non_negative("crew : training_cost_growth", c.training_cost_growth)?;
        if !(c.training_gain > 0.0 && c.training_gain <= 1.0) {
            bail!("crew : training_gain doit être compris entre 0 (exclu) et 1");
        }
        let e = &c.effects;
        for (name, v) in [
            ("driving_accel", e.driving_accel),
            ("service_comfort", e.service_comfort),
            ("control_revenue", e.control_revenue),
            ("boarding_speed", e.boarding_speed),
        ] {
            non_negative(&format!("crew : effects.{name}"), v)?;
        }
        ratio("crew : effects.economy_fuel", e.economy_fuel)?;
        ratio("crew : effects.care_wear", e.care_wear)?;
        if c.names.first.is_empty() || c.names.last.is_empty() {
            bail!("crew : il faut au moins un prénom et un nom dans [names]");
        }
        let b = &self.balance;
        self.era_rank("balance : prestige_min_era", &b.prestige_min_era)?;
        positive("balance : prestige_base", b.prestige_base)?;
        positive("balance : prestige_per_decade", b.prestige_per_decade)
    }

    fn validate_balance(&self) -> Result<()> {
        let b = &self.balance;
        positive("balance : time_scale", b.time_scale)?;
        non_negative("balance : start_money", b.start_money)?;
        non_negative("balance : fare_per_km", b.fare_per_km)?;
        non_negative("balance : demand_per_1000_per_h", b.demand_per_1000_per_h)?;
        positive("balance : demand_ref_km", b.demand_ref_km)?;
        non_negative("balance : demand_distance_exp", b.demand_distance_exp)?;
        non_negative("balance : build_cost_per_km", b.build_cost_per_km)?;
        positive("balance : station_waiting_cap", b.station_waiting_cap)?;
        positive("balance : boarding_per_min", b.boarding_per_min)?;
        positive("balance : boarding_full_min", b.boarding_full_min)?;
        positive("balance : days_per_year", b.days_per_year)?;
        positive("balance : base_line_speed_kmh", b.base_line_speed_kmh)?;
        positive("balance : base_signal_speed_kmh", b.base_signal_speed_kmh)?;
        positive("balance : accel_k", b.accel_k)?;
        positive("balance : accel_max", b.accel_max)?;
        positive("balance : base_decel", b.base_decel)?;
        positive("balance : decel_max", b.decel_max)?;
        non_negative("balance : unit_weight_t", b.unit_weight_t)?;
        positive("balance : dwell_min_min", b.dwell_min_min)?;
        positive("balance : delay_tolerance_min", b.delay_tolerance_min)?;
        positive("balance : fare_level_min", b.fare_level_min)?;
        positive("balance : satisfaction_inertia", b.satisfaction_inertia)?;
        non_negative("balance : offline_max_h", b.offline_max_h)?;
        non_negative("balance : wear_per_train_km", b.wear_per_train_km)?;
        non_negative("balance : repair_cost_per_km", b.repair_cost_per_km)?;
        if b.fuel_emergency_mult < 1.0 {
            bail!("balance : fuel_emergency_mult doit valoir au moins 1");
        }
        if b.dwell_max_min < b.dwell_min_min
            || !(b.dwell_min_min..=b.dwell_max_min).contains(&b.default_dwell_min)
        {
            bail!("balance : il faut dwell_min_min <= default_dwell_min <= dwell_max_min");
        }
        if !(b.fare_level_min <= 1.0 && b.fare_level_max >= 1.0) {
            bail!("balance : il faut fare_level_min <= 1 <= fare_level_max");
        }
        ratio("balance : base_comfort", b.base_comfort)?;
        ratio("balance : offline_efficiency", b.offline_efficiency)?;
        ratio("balance : resale_ratio", b.resale_ratio)?;
        for (name, v) in [
            ("wear_slow_threshold", b.wear_slow_threshold),
            ("wear_min_speed_factor", b.wear_min_speed_factor),
        ] {
            if !(v > 0.0 && v <= 1.0) {
                bail!("balance : {name} doit être compris entre 0 (exclu) et 1");
            }
        }
        match self.locomotive(&b.start_locomotive) {
            None => bail!(
                "balance : start_locomotive « {} » inconnue",
                b.start_locomotive
            ),
            Some(loco) => {
                if self.era_index(&loco.era) != Some(0)
                    || !loco.requires.is_empty()
                    || loco.requires_upgrade.is_some()
                {
                    bail!(
                        "balance : start_locomotive doit être disponible dès le début de la partie"
                    );
                }
            }
        }
        match self.line(&b.start_line) {
            None => bail!("balance : start_line « {} » inconnue", b.start_line),
            Some(line) => {
                if line
                    .era
                    .as_ref()
                    .is_some_and(|e| self.era_index(e) != Some(0))
                    || self.line_countries(line).len() != 1
                {
                    bail!(
                        "balance : start_line doit être une ligne intérieure, constructible dès le début"
                    );
                }
            }
        }
        // Le gain en variété ne doit pas rendre un pays inaccessible faute de recherche.
        let home_gauge = self.home_country().map(|c| c.gauge_mm);
        let has_variable_gauge = self
            .research
            .iter()
            .any(|r| r.effects.contains(&Effect::VariableGauge));
        if !has_variable_gauge
            && self
                .countries
                .iter()
                .any(|c| Some(c.gauge_mm) != home_gauge)
        {
            bail!(
                "research.toml : un pays a un autre écartement, il faut une recherche à effet variable_gauge"
            );
        }
        Ok(())
    }
}
