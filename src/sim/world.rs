use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::data::{BonusKind, Country, Era, GameData, Line};

use super::crew::{CrewMember, TrainCrew, TrainingSession};
use super::kinematics::Profile;
use super::modifiers::Modifiers;

/// Format de la sauvegarde ; repart de 1 avec la première version officielle.
pub const SAVE_VERSION: u32 = 1;

/// Constante de temps (secondes de jeu) de la moyenne glissante des revenus.
pub(crate) const INCOME_TAU: f64 = 2.0 * 3600.0;

fn one() -> f64 {
    1.0
}

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

/// Trajet en cours : profil de marche fixé au départ.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Trip {
    /// Horloge du jeu au départ.
    pub started: f64,
    /// Position sur la ligne au départ.
    pub origin_km: f64,
    /// Durée qu'aurait le trajet sur une voie en parfait état, en secondes.
    pub nominal_s: f64,
    pub profile: Profile,
}

/// Ordre exécuté dès que le train est à quai.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrainOrder {
    /// Revendre la machine à son arrivée.
    Sell,
    /// Remplacer la machine par `loco` ; `paid` a déjà été débité à la commande.
    Replace { loco: String, paid: f64 },
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
    /// Chargement par catégorie de trafic.
    #[serde(default)]
    pub load: BTreeMap<String, u32>,
    /// Fraction de voyageur en cours d'embarquement.
    #[serde(default)]
    pub board_acc: f64,
    /// Heure de départ prévue (horloge du jeu) du prochain ou du dernier départ.
    #[serde(default)]
    pub sched_departure: f64,
    /// Retard pris au départ, en secondes de jeu.
    #[serde(default)]
    pub late_s: f64,
    /// Trajet en cours, quand le train roule.
    #[serde(default)]
    pub trip: Option<Trip>,
    /// Compétences de l'équipage affecté, recopiées depuis la liste des équipiers.
    #[serde(default)]
    pub crew: TrainCrew,
    /// Note d'affluence (0 à 1) relevée au départ ; 1 = à l'aise.
    #[serde(default = "one")]
    pub crowding: f64,
    /// Vente ou remplacement prévu à la prochaine arrivée.
    #[serde(default)]
    pub order: Option<TrainOrder>,
}

impl Train {
    pub fn is_docked(&self) -> bool {
        matches!(self.state, TrainState::AtStation { .. })
    }

    /// Chargement total, toutes catégories confondues.
    pub fn count(&self) -> u32 {
        self.load.values().sum()
    }

    /// Note au départ le retard et l'affluence, puis met le train en route.
    pub(crate) fn depart(&mut self, clock: f64, capacity: u32, station_fill: f64, trip: Trip) {
        let load = self.count() as f64 / capacity.max(1) as f64;
        let overload = ((load - 0.5) / 0.5).clamp(0.0, 1.0);
        self.crowding = (1.0 - 0.6 * overload - 0.4 * station_fill.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        self.late_s = (clock - self.sched_departure).max(0.0);
        self.trip = Some(trip);
        self.state = TrainState::Moving;
    }

    /// Vitesse actuelle, en km/h.
    pub fn speed_kmh(&self, clock: f64) -> f64 {
        match (&self.state, &self.trip) {
            (TrainState::Moving, Some(trip)) => trip.profile.at(clock - trip.started).1,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StationState {
    /// Voyageurs (ou tonnes) en attente, par catégorie de trafic.
    #[serde(default)]
    pub waiting: BTreeMap<String, f64>,
}

impl StationState {
    pub fn total(&self) -> f64 {
        self.waiting.values().sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineState {
    /// État de la voie, de 0 (ruinée) à 1 (neuve).
    pub condition: f64,
    /// Niveau de prix des billets ; 1 = tarif normal.
    #[serde(default = "one")]
    pub fare_level: f64,
    /// Arrêt prévu en gare entre une arrivée et le départ suivant, en minutes de jeu.
    #[serde(default)]
    pub dwell_min: f64,
    /// Améliorations installées (rails, caténaire, ligne nouvelle…).
    #[serde(default)]
    pub upgrades: BTreeSet<String>,
    /// Vrai une fois la ligne construite ; sinon ce n'est qu'un projet.
    #[serde(default)]
    pub built: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveResearch {
    pub id: String,
    pub progress_s: f64,
}

/// Ce qui survit à une nationalisation : points et bonus permanents.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Prestige {
    /// Points disponibles.
    pub points: u32,
    /// Points gagnés depuis toujours.
    pub earned: u32,
    /// Nombre de compagnies déjà nationalisées.
    pub resets: u32,
    /// Niveau acquis de chaque bonus.
    pub levels: BTreeMap<String, u32>,
}

impl Prestige {
    pub fn level(&self, bonus: &str) -> u32 {
        self.levels.get(bonus).copied().unwrap_or(0)
    }

    /// Somme des effets (valeur × niveau) des bonus d'un type donné.
    pub fn total(&self, data: &GameData, kind: BonusKind) -> f64 {
        data.bonuses
            .iter()
            .filter(|b| b.kind == kind)
            .map(|b| b.value * self.level(&b.id) as f64)
            .sum()
    }
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
    #[serde(default)]
    pub trips: u64,
    pub stations: BTreeMap<String, StationState>,
    pub lines: BTreeMap<String, LineState>,
    pub trains: Vec<Train>,
    pub next_train_id: u32,
    pub research_done: BTreeSet<String>,
    pub research_active: Option<ActiveResearch>,
    /// Revenus nets récents, amortis exponentiellement.
    #[serde(default)]
    pub income_recent: f64,
    /// Satisfaction par catégorie de trafic, de 0 à 1 (0,5 = neutre).
    #[serde(default)]
    pub satisfaction: BTreeMap<String, f64>,
    /// Identifiant de l'époque en cours.
    #[serde(default)]
    pub era: String,
    /// Horloge du jeu à l'entrée dans l'époque, pour le calendrier affiché.
    #[serde(default)]
    pub era_started: f64,
    /// Stocks de combustible, par énergie.
    #[serde(default)]
    pub fuel: BTreeMap<String, f64>,
    /// Événements historiques déjà joués.
    #[serde(default)]
    pub events_done: BTreeSet<String>,
    /// Événement en attente d'un choix du joueur.
    #[serde(default)]
    pub event_pending: Option<String>,
    /// Pays où la compagnie détient une concession.
    #[serde(default)]
    pub countries: BTreeSet<String>,
    /// Services engagés (automatisation).
    #[serde(default)]
    pub staff: BTreeSet<String>,
    #[serde(default)]
    pub prestige: Prestige,
    /// Conducteurs et contrôleurs employés.
    #[serde(default)]
    pub crew: Vec<CrewMember>,
    /// Candidats présents au bureau de recrutement.
    #[serde(default)]
    pub candidates: Vec<CrewMember>,
    /// Arrivage pour lequel les candidats ont été tirés.
    #[serde(default, alias = "candidates_day")]
    pub candidates_batch: u64,
    #[serde(default)]
    pub next_crew_id: u32,
    /// Conducteurs embauchés depuis la fondation (le premier est sans prime).
    #[serde(default)]
    pub drivers_hired: u32,
    /// Sessions de formation à venir ou en cours.
    #[serde(default)]
    pub trainings: Vec<TrainingSession>,
    #[serde(default)]
    pub next_training_id: u32,
    /// Pays où la compagnie a été fondée (vide : pays de départ par défaut).
    #[serde(default)]
    pub home: String,
    /// Vrai tant que le joueur n'a pas choisi le pays de sa nouvelle compagnie.
    #[serde(default)]
    pub choosing_home: bool,
}

impl World {
    /// Nouvelle partie dans le pays de départ par défaut.
    pub fn new(data: &GameData) -> World {
        World::founded_in(data, &data.balance.start_country)
    }

    /// Nouvelle partie dans un pays : sa ligne de départ, une locomotive à
    /// quai, le matin du jour 1. Un pays sans ligne de départ est remplacé par
    /// le pays par défaut.
    pub fn founded_in(data: &GameData, country_id: &str) -> World {
        let mut world = World {
            version: SAVE_VERSION,
            clock: 8.0 * 3600.0,
            money: data.balance.start_money,
            lifetime_income: 0.0,
            passengers_carried: 0,
            trips: 0,
            stations: BTreeMap::new(),
            lines: BTreeMap::new(),
            trains: Vec::new(),
            next_train_id: 1,
            research_done: BTreeSet::new(),
            research_active: None,
            income_recent: 0.0,
            satisfaction: BTreeMap::new(),
            era: String::new(),
            era_started: 0.0,
            fuel: BTreeMap::new(),
            events_done: BTreeSet::new(),
            event_pending: None,
            countries: BTreeSet::new(),
            staff: BTreeSet::new(),
            prestige: Prestige::default(),
            crew: Vec::new(),
            candidates: Vec::new(),
            candidates_batch: 0,
            next_crew_id: 0,
            drivers_hired: 0,
            trainings: Vec::new(),
            next_training_id: 0,
            home: country_id.to_string(),
            choosing_home: false,
        };
        world.reconcile(data);
        let line = world.start_line(data).to_string();
        world.spawn_train(&data.balance.start_locomotive, &line);
        world
    }

    /// Pays où la compagnie a été fondée.
    pub fn home_country<'a>(&self, data: &'a GameData) -> Option<&'a Country> {
        data.country(&self.home)
            .filter(|c| c.start_line.is_some())
            .or(data.default_home())
    }

    /// Ligne où la compagnie a commencé, toujours construite.
    pub fn start_line<'a>(&self, data: &'a GameData) -> &'a str {
        self.home_country(data)
            .and_then(|c| c.start_line.as_deref())
            .unwrap_or_default()
    }

    /// Aligne l'état sur les données (après un changement de mods ou de
    /// version) : ajoute ce qui manque, retire ce qui n'existe plus, et
    /// corrige les valeurs hors bornes.
    pub fn reconcile(&mut self, data: &GameData) {
        let b = &data.balance;
        let known = |id: &String| data.class(id).is_some();
        self.version = SAVE_VERSION;

        for s in data.stations() {
            self.stations.entry(s.id.clone()).or_default();
        }
        for station in self.stations.values_mut() {
            station.waiting.retain(|id, _| known(id));
        }

        for l in data.lines() {
            self.lines.entry(l.id.clone()).or_insert(LineState {
                condition: 1.0,
                fare_level: 1.0,
                dwell_min: b.default_dwell_min,
                upgrades: BTreeSet::new(),
                built: false,
            });
        }
        // Pays de fondation : celui des sauvegardes antérieures au choix du pays
        // est le pays par défaut.
        if let Some(home) = self.home_country(data) {
            self.home = home.id.clone();
        }
        // La ligne de départ est toujours construite, comme toute ligne où roule
        // un train (sauvegardes antérieures aux lignes à construire).
        let in_use: BTreeSet<&str> = std::iter::once(self.start_line(data))
            .chain(self.trains.iter().map(|t| t.line.as_str()))
            .collect();
        for (id, line) in &mut self.lines {
            line.built |= in_use.contains(id.as_str());
        }
        // Concessions : le pays de départ, et tout pays touché par une ligne construite.
        self.countries.retain(|id| data.country(id).is_some());
        self.countries.insert(self.home.clone());
        for line in data
            .lines()
            .filter(|l| self.lines.get(&l.id).is_some_and(|s| s.built))
        {
            self.countries
                .extend(data.line_countries(line).into_iter().map(|c| c.id.clone()));
        }
        for line in self.lines.values_mut() {
            line.condition = line.condition.clamp(0.0, 1.0);
            line.fare_level = line.fare_level.clamp(b.fare_level_min, b.fare_level_max);
            line.dwell_min = if line.dwell_min > 0.0 {
                line.dwell_min.clamp(b.dwell_min_min, b.dwell_max_min)
            } else {
                b.default_dwell_min
            };
            line.upgrades.retain(|id| data.upgrade(id).is_some());
        }

        if data.era(&self.era).is_none() {
            self.era = data.eras.first().map(|e| e.id.clone()).unwrap_or_default();
            self.era_started = 0.0;
        }
        self.fuel
            .retain(|id, stock| data.fuel(id).is_some() && stock.is_finite() && *stock >= 0.0);
        self.events_done.retain(|id| data.event(id).is_some());
        self.staff.retain(|id| data.staff(id).is_some());
        for member in self.crew.iter_mut().chain(&mut self.candidates) {
            for skill in &mut member.skills {
                *skill = if skill.is_finite() {
                    skill.clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }
        }
        self.prestige
            .levels
            .retain(|id, _| data.bonus(id).is_some());
        for bonus in &data.bonuses {
            if let Some(level) = self.prestige.levels.get_mut(&bonus.id) {
                *level = (*level).min(bonus.max_level);
            }
        }
        if self
            .event_pending
            .as_ref()
            .is_some_and(|id| data.event(id).is_none())
        {
            self.event_pending = None;
        }
        self.research_done.retain(|id| data.research(id).is_some());
        let modifiers = self.modifiers(data);

        self.trains
            .retain(|t| data.locomotive(&t.loco).is_some() && data.line(&t.line).is_some());
        for t in &mut self.trains {
            let Some(line) = data.line(&t.line) else {
                continue;
            };
            t.pos_km = t.pos_km.clamp(0.0, line.length_km);
            t.load.retain(|id, _| known(id));
            // Horaire absent ou incohérent : on repart d'un horaire neutre.
            let dwell_s = self
                .lines
                .get(&t.line)
                .map_or(b.default_dwell_min, |l| l.dwell_min)
                * 60.0;
            match t.state {
                TrainState::AtStation { .. } => {
                    t.trip = None;
                    if t.sched_departure <= 0.0 {
                        t.sched_departure = self.clock + dwell_s;
                    }
                }
                // Train en route sans profil de marche (sauvegarde incomplète) : il
                // repart de sa position, pour la distance qui lui reste.
                TrainState::Moving if t.trip.is_none() => {
                    match super::tick::plan_trip(
                        data,
                        &modifiers,
                        t,
                        self.lines.get(&t.line),
                        self.clock,
                    ) {
                        Some(trip) => {
                            t.sched_departure = self.clock;
                            t.late_s = 0.0;
                            t.trip = Some(trip);
                        }
                        None => t.state = TrainState::AtStation { waited: 0.0 },
                    }
                }
                TrainState::Moving => {}
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

        self.satisfaction.retain(|id, _| known(id));
        for class in &data.traffic {
            let s = self.satisfaction.entry(class.id.clone()).or_insert(0.5);
            *s = s.clamp(0.0, 1.0);
        }
        self.sync_crew();
        // Un seul conducteur et un seul contrôleur par train : les doublons
        // d'une ancienne sauvegarde retournent en réserve.
        let mut posts = BTreeSet::new();
        for member in &mut self.crew {
            if let Some(train) = member.train
                && !posts.insert((train, member.role == super::crew::CrewRole::Driver))
            {
                member.train = None;
            }
        }
        self.sync_crew();
        // Formations : on oublie les inscrits disparus.
        let ids: BTreeSet<u32> = self.crew.iter().map(|m| m.id).collect();
        for t in &mut self.trainings {
            t.skill = t.skill.min(2);
            t.enrolled.retain(|e| ids.contains(&e.crew));
        }
        // Un remplacement vers une machine disparue des données est annulé et remboursé.
        let mut refund = 0.0;
        for train in &mut self.trains {
            if let Some(TrainOrder::Replace { loco, paid }) = &train.order
                && data.locomotive(loco).is_none()
            {
                refund += paid;
                train.order = None;
            }
        }
        self.money += refund;
    }

    /// Arrêt prévu en gare sur une ligne, en secondes de jeu.
    pub fn dwell_s(&self, line: &str) -> f64 {
        self.lines.get(line).map_or(0.0, |l| l.dwell_min * 60.0)
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
            load: BTreeMap::new(),
            board_acc: 0.0,
            sched_departure: self.clock + self.dwell_s(line),
            late_s: 0.0,
            trip: None,
            crew: TrainCrew::default(),
            crowding: 1.0,
            order: None,
        });
        id
    }

    pub fn train(&self, id: u32) -> Option<&Train> {
        self.trains.iter().find(|t| t.id == id)
    }

    pub fn trains_on(&self, line: &str) -> usize {
        self.trains.iter().filter(|t| t.line == line).count()
    }

    pub fn trains_moving(&self) -> usize {
        self.trains.iter().filter(|t| !t.is_docked()).count()
    }

    pub fn day(&self) -> u64 {
        (self.clock / 86_400.0) as u64 + 1
    }

    /// Heure et minute de la journée en cours.
    pub fn time_of_day(&self) -> (u32, u32) {
        hour_minute(self.clock)
    }

    /// Revenu net moyen récent, en £ par seconde de jeu.
    pub fn income_per_game_second(&self) -> f64 {
        self.income_recent / INCOME_TAU
    }

    /// Époque en cours (la première si l'identifiant est inconnu).
    pub fn era<'a>(&self, data: &'a GameData) -> &'a Era {
        data.era(&self.era)
            .or(data.eras.first())
            .expect("au moins une époque (validé au chargement)")
    }

    pub fn era_index(&self, data: &GameData) -> usize {
        data.era_index(&self.era).unwrap_or(0)
    }

    /// Année affichée : elle avance dans l'époque sans en dépasser la fin.
    pub fn year(&self, data: &GameData) -> u32 {
        let era = self.era(data);
        let days = ((self.clock - self.era_started) / 86_400.0).max(0.0);
        let years = (days / data.balance.days_per_year) as u32;
        (era.from + years).min(era.to)
    }

    /// Effets des recherches terminées, bonus de prestige compris.
    pub fn modifiers(&self, data: &GameData) -> Modifiers {
        let mut m = Modifiers::compute(&self.research_done, data);
        m.fare *= 1.0 + self.prestige.total(data, BonusKind::IncomeMult);
        m.research_speed = 1.0 + self.prestige.total(data, BonusKind::ResearchSpeed);
        m.build_cost = data
            .bonuses
            .iter()
            .filter(|b| b.kind == BonusKind::BuildCostMult)
            .map(|b| {
                (1.0 - b.value)
                    .max(0.0)
                    .powi(self.prestige.level(&b.id) as i32)
            })
            .product();
        m
    }

    pub fn is_built(&self, line_id: &str) -> bool {
        self.lines.get(line_id).is_some_and(|l| l.built)
    }

    /// Vrai si une ligne construite dessert cette gare.
    pub fn is_served(&self, data: &GameData, station_id: &str) -> bool {
        data.lines()
            .any(|l| (l.from == station_id || l.to == station_id) && self.is_built(&l.id))
    }

    pub fn has_concession(&self, country_id: &str) -> bool {
        self.countries.contains(country_id)
    }

    pub fn fuel_stock(&self, fuel: &str) -> f64 {
        self.fuel.get(fuel).copied().unwrap_or(0.0)
    }

    pub fn satisfaction_of(&self, class: &str) -> f64 {
        self.satisfaction.get(class).copied().unwrap_or(0.5)
    }

    /// Satisfaction moyenne, pondérée par la part de chaque catégorie.
    pub fn overall_satisfaction(&self, data: &GameData) -> f64 {
        let total: f64 = data.traffic.iter().map(|c| c.share).sum();
        if total <= 0.0 {
            return 0.5;
        }
        data.traffic
            .iter()
            .map(|c| c.share * self.satisfaction_of(&c.id))
            .sum::<f64>()
            / total
    }
}

/// Heure et minute de la journée pour une valeur de l'horloge du jeu.
pub fn hour_minute(clock: f64) -> (u32, u32) {
    let s = clock.rem_euclid(86_400.0) as u32;
    (s / 3600, s % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_save_is_completed_on_load() {
        let data = GameData::embedded().unwrap();
        let old = r#"{
            "version": 1, "clock": 40000.0, "money": 321.0, "lifetime_income": 900.0,
            "passengers_carried": 480,
            "stations": {"stockton": {"waiting": {"commuter": 37.5, "inconnu": 3.0}},
                         "darlington": {"waiting": {"commuter": 4.0}}},
            "lines": {"stockton_darlington": {"condition": 0.8}},
            "trains": [
                {"id": 1, "loco": "locomotion_1", "line": "stockton_darlington", "pos_km": 9.5,
                 "dir": "Outbound", "state": "Moving", "load": {"commuter": 31}, "board_acc": 0.2},
                {"id": 2, "loco": "locomotion_1", "line": "stockton_darlington", "pos_km": 19.0,
                 "dir": "Inbound", "state": {"AtStation": {"waited": 120.0}}, "load": {"commuter": 5}}
            ],
            "next_train_id": 3, "research_done": ["hand_signals"], "research_active": null,
            "income_recent": 12.0
        }"#;
        let mut world: World = serde_json::from_str(old).unwrap();
        world.reconcile(&data);

        assert_eq!(world.version, SAVE_VERSION);
        assert_eq!(world.stations["stockton"].waiting["commuter"], 37.5);
        assert_eq!(
            world.stations["stockton"].total(),
            37.5,
            "catégorie inconnue retirée"
        );
        assert_eq!(world.trains[0].count(), 31);
        assert_eq!(world.trains[1].load["commuter"], 5);
        let line = &world.lines["stockton_darlington"];
        assert_eq!(
            (line.condition, line.fare_level, line.dwell_min),
            (0.8, 1.0, 20.0)
        );
        // Le train en route reprend sa marche depuis sa position, pour la distance restante.
        let trip = world.trains[0]
            .trip
            .expect("un profil de marche est reconstruit");
        assert_eq!((trip.started, trip.origin_km), (40000.0, 9.5));
        assert!((trip.profile.distance_km - 9.5).abs() < 1e-9);
        assert_eq!((world.era.as_str(), world.year(&data)), ("pioneers", 1825));
        assert!(world.lines["stockton_darlington"].upgrades.is_empty());
        // Seule la ligne exploitée est construite, et seul son pays est ouvert.
        assert!(world.is_built("stockton_darlington") && !world.is_built("darlington_york"));
        assert_eq!(world.countries.iter().collect::<Vec<_>>(), ["uk"]);
        assert!(world.is_served(&data, "stockton") && !world.is_served(&data, "york"));
        assert_eq!(world.trains[1].sched_departure, 40000.0 + 20.0 * 60.0);
        assert_eq!(world.satisfaction.len(), 4);
        assert_eq!(world.overall_satisfaction(&data), 0.5);

        // Une fois complétée, la partie se sérialise et se relit à l'identique.
        let json = serde_json::to_string(&world).unwrap();
        assert_eq!(serde_json::from_str::<World>(&json).unwrap(), world);
    }

    #[test]
    fn crowding_note_drops_with_full_trains_and_packed_platforms() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        let train = &mut world.trains[0];
        train.sched_departure = 1000.0;
        let trip = |started| Trip {
            started,
            origin_km: 0.0,
            nominal_s: 100.0,
            profile: Profile::new(19.0, 24.0, 0.02, 0.12),
        };
        train.depart(1000.0, 40, 0.0, trip(1000.0));
        assert_eq!((train.crowding, train.late_s), (1.0, 0.0));
        assert_eq!(train.speed_kmh(1000.0), 0.0);
        assert!(train.speed_kmh(2000.0) > 20.0);

        train.load.insert("commuter".into(), 40);
        train.depart(1300.0, 40, 1.0, trip(1300.0));
        assert!(train.crowding < 0.01);
        assert_eq!(train.late_s, 300.0);
        assert_eq!(train.trip.unwrap().started, 1300.0);
    }

    #[test]
    fn calendar_year_advances_within_the_era() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        assert_eq!(world.year(&data), 1825);
        world.clock += 10.0 * 86_400.0;
        assert_eq!(world.year(&data), 1830, "deux jours de jeu par année");
        world.clock += 1000.0 * 86_400.0;
        assert_eq!(
            world.year(&data),
            1850,
            "le calendrier ne dépasse pas la fin de l'époque"
        );
        world.era = "steam".into();
        world.era_started = world.clock;
        assert_eq!(world.year(&data), 1850);
    }
}
