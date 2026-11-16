//! Données de jeu : chargées depuis des fichiers TOML embarqués dans le
//! binaire, puis complétées ou remplacées par les fichiers du dossier `mods/`.

mod validate;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Fichiers de données, dans l'ordre de chargement, avec leur contenu embarqué.
pub const FILES: [(&str, &str); 12] = [
    ("balance.toml", include_str!("../../data/balance.toml")),
    ("eras.toml", include_str!("../../data/eras.toml")),
    ("fuels.toml", include_str!("../../data/fuels.toml")),
    ("traffic.toml", include_str!("../../data/traffic.toml")),
    ("research.toml", include_str!("../../data/research.toml")),
    (
        "locomotives.toml",
        include_str!("../../data/locomotives.toml"),
    ),
    ("upgrades.toml", include_str!("../../data/upgrades.toml")),
    ("countries.toml", include_str!("../../data/countries.toml")),
    ("events.toml", include_str!("../../data/events.toml")),
    ("staff.toml", include_str!("../../data/staff.toml")),
    ("prestige.toml", include_str!("../../data/prestige.toml")),
    ("crew.toml", include_str!("../../data/crew.toml")),
];

/// Texte traduit. Le français est obligatoire, l'anglais facultatif.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Localized {
    pub fr: String,
    #[serde(default)]
    pub en: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Balance {
    pub time_scale: f64,
    pub start_money: f64,
    pub start_locomotive: String,
    pub start_line: String,
    pub fare_per_km: f64,
    pub demand_per_1000_per_h: f64,
    pub demand_ref_km: f64,
    pub demand_distance_exp: f64,
    pub build_cost_per_km: f64,
    pub station_waiting_cap: f64,
    pub boarding_per_min: f64,
    pub boarding_full_min: f64,
    pub resale_ratio: f64,
    pub days_per_year: f64,
    pub base_line_speed_kmh: f64,
    pub base_signal_speed_kmh: f64,
    pub accel_k: f64,
    pub accel_max: f64,
    pub base_decel: f64,
    pub decel_max: f64,
    pub unit_weight_t: f64,
    pub fuel_emergency_mult: f64,
    pub default_dwell_min: f64,
    pub dwell_min_min: f64,
    pub dwell_max_min: f64,
    pub delay_tolerance_min: f64,
    pub fare_level_min: f64,
    pub fare_level_max: f64,
    pub base_comfort: f64,
    pub satisfaction_inertia: f64,
    pub offline_max_h: f64,
    pub offline_efficiency: f64,
    pub prestige_min_era: String,
    pub prestige_base: f64,
    pub prestige_per_decade: f64,
    pub wear_per_train_km: f64,
    pub wear_slow_threshold: f64,
    pub wear_min_speed_factor: f64,
    pub repair_cost_per_km: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EraRules {
    /// Le combustible s'achète d'avance, à un cours variable.
    pub fuel_market: bool,
    /// Arrêt imposé aux locomotives à vapeur pour faire de l'eau, en minutes de jeu.
    pub water_stop_min: f64,
}

/// Conditions d'entrée dans une époque.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EraUnlock {
    #[serde(default)]
    pub research: Vec<String>,
    #[serde(default)]
    pub passengers: u64,
    #[serde(default)]
    pub cost: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Era {
    pub id: String,
    pub name: Localized,
    pub description: Localized,
    pub from: u32,
    pub to: u32,
    pub demand_mult: f64,
    pub fare_mult: f64,
    pub cost_mult: f64,
    /// Niveau du coût de construction des lignes.
    pub build_mult: f64,
    pub rules: EraRules,
    #[serde(default)]
    pub unlock: EraUnlock,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fuel {
    pub id: String,
    pub name: Localized,
    pub unit: String,
    pub base_price: f64,
    pub volatility: f64,
    /// Vrai si l'énergie se stocke ; sinon elle est payée à la consommation.
    pub stock: bool,
    pub lot: f64,
}

impl Fuel {
    /// Cours du jour, par unité. Sans marché, le prix est fixe.
    ///
    /// Le cours suit deux ondes de périodes sans rapport simple, pour varier
    /// sans hasard : la simulation reste déterministe.
    pub fn price(&self, clock: f64, era: &Era) -> f64 {
        let base = self.base_price * era.cost_mult;
        if !era.rules.fuel_market {
            return base;
        }
        let days = clock / 86_400.0;
        let tau = std::f64::consts::TAU;
        let wave = 0.6 * (tau * days / 3.7).sin() + 0.4 * (tau * days / 11.3 + 1.3).sin();
        base * (1.0 + self.volatility * wave)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locomotive {
    pub id: String,
    pub name: String,
    pub builder: String,
    pub year: u32,
    pub era: String,
    pub traction: String,
    pub wheels: String,
    pub top_speed_kmh: f64,
    #[serde(default)]
    pub record_kmh: Option<f64>,
    pub power_kw: f64,
    pub weight_t: f64,
    pub capacity: u32,
    pub cost: f64,
    pub upkeep_per_h: f64,
    pub fuel: String,
    pub fuel_per_km: f64,
    pub track_wear: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub requires_upgrade: Option<String>,
    #[serde(default)]
    pub speculative: bool,
    pub history: Localized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Branch {
    Traction,
    RunningGear,
    Braking,
    Signalling,
    Comfort,
}

impl Branch {
    pub const ALL: [Branch; 5] = [
        Branch::Traction,
        Branch::RunningGear,
        Branch::Braking,
        Branch::Signalling,
        Branch::Comfort,
    ];
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    SpeedMult {
        value: f64,
    },
    CapacityMult {
        value: f64,
    },
    FareMult {
        value: f64,
    },
    DemandMult {
        value: f64,
    },
    TrackWearMult {
        value: f64,
    },
    FuelMult {
        value: f64,
    },
    /// Masse des trains : plus lourd, donc moins d'accélération.
    WeightMult {
        value: f64,
    },
    /// Bonus additif de confort (0 à 1).
    Comfort {
        value: f64,
    },
    /// Décélération de freinage ajoutée, en m/s².
    BrakeAdd {
        value: f64,
    },
    /// Vitesse maximale autorisée par la signalisation, en km/h.
    SignalSpeed {
        value: f64,
    },
    /// Trains supplémentaires admis sur chaque ligne.
    LineCapacity {
        value: u32,
    },
    AutoDispatch,
    NoWaterStop,
    /// Permet de s'installer dans un pays dont l'écartement diffère.
    VariableGauge,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Research {
    pub id: String,
    pub name: Localized,
    pub description: Localized,
    pub branch: Branch,
    pub era: String,
    pub year: u32,
    pub cost: f64,
    pub duration_h: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpgradeEffect {
    /// Vitesse autorisée sur la ligne ; `only` la réserve à certaines tractions.
    LineSpeed {
        value: f64,
        #[serde(default)]
        only: Vec<String>,
    },
    WearMult {
        value: f64,
    },
}

/// Amélioration d'une ligne (rails, caténaire, ligne nouvelle…).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upgrade {
    pub id: String,
    pub name: Localized,
    pub description: Localized,
    pub era: String,
    pub cost_per_km: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub requires_upgrade: Option<String>,
    #[serde(default)]
    pub effects: Vec<UpgradeEffect>,
}

/// Effet de chaque compétence d'équipage quand elle est au maximum.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewEffects {
    pub driving_accel: f64,
    pub economy_fuel: f64,
    pub care_wear: f64,
    pub service_comfort: f64,
    pub control_revenue: f64,
    pub boarding_speed: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewNames {
    pub first: Vec<String>,
    pub last: Vec<String>,
}

/// Règles des équipages : recrutement, salaires, progression, effets.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewRules {
    pub candidates_per_day: u32,
    pub base_wage: f64,
    pub skill_wage: f64,
    pub hire_fee_hours: f64,
    pub xp_per_km: f64,
    pub effects: CrewEffects,
    pub names: CrewNames,
}

/// Tâche qu'un service automatise.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StaffRole {
    /// Remet la voie à neuf sous ce seuil d'état.
    AutoRepair { threshold: f64 },
    /// Reconstitue `reserve_h` heures de combustible quand le cours est correct.
    AutoFuel {
        reserve_h: f64,
        max_price_ratio: f64,
    },
    /// Raccourcit l'arrêt en gare selon l'affluence.
    AutoTimetable { crowd_factor: f64 },
}

/// Service à engager pour automatiser une tâche.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Staff {
    pub id: String,
    pub name: Localized,
    pub description: Localized,
    pub era: String,
    #[serde(default)]
    pub requires: Vec<String>,
    pub cost: f64,
    pub salary_per_h: f64,
    pub role: StaffRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BonusKind {
    IncomeMult,
    ResearchSpeed,
    BuildCostMult,
    OfflineEfficiency,
    StartMoney,
}

/// Bonus permanent acheté avec des points de prestige.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrestigeBonus {
    pub id: String,
    pub name: Localized,
    pub description: Localized,
    pub kind: BonusKind,
    /// Effet de chaque niveau.
    pub value: f64,
    /// Prix du premier niveau, en points de prestige.
    pub cost: u32,
    pub cost_growth: f64,
    pub max_level: u32,
}

impl PrestigeBonus {
    /// Prix du niveau suivant quand `level` niveaux sont déjà acquis.
    pub fn cost_at(&self, level: u32) -> u32 {
        (self.cost as f64 * self.cost_growth.powi(level as i32))
            .round()
            .max(1.0) as u32
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventTrigger {
    /// Époque à atteindre (ou dépasser).
    #[serde(default)]
    pub era: Option<String>,
    #[serde(default)]
    pub passengers: u64,
    #[serde(default)]
    pub research: Vec<String>,
    /// Il suffit de posséder l'une de ces machines.
    #[serde(default)]
    pub owns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventEffect {
    /// Gain, ou somme à payer si la valeur est négative.
    Money {
        value: f64,
    },
    GrantResearch {
        target: String,
    },
    Satisfaction {
        value: f64,
        #[serde(default)]
        class: Option<String>,
    },
    TrackCondition {
        value: f64,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventChoice {
    pub label: Localized,
    pub outcome: Localized,
    #[serde(default)]
    pub effects: Vec<EventEffect>,
}

impl EventChoice {
    /// Somme à débourser pour faire ce choix.
    pub fn cost(&self) -> f64 {
        self.effects
            .iter()
            .map(|e| match e {
                EventEffect::Money { value } if *value < 0.0 => -value,
                _ => 0.0,
            })
            .sum()
    }
}

/// Événement historique jouable.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameEvent {
    pub id: String,
    pub year: u32,
    pub title: Localized,
    pub text: Localized,
    #[serde(default)]
    pub trigger: EventTrigger,
    #[serde(rename = "choice")]
    pub choices: Vec<EventChoice>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub pos: [f64; 2],
    pub population: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub id: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub length_km: f64,
    #[serde(default)]
    pub via: Vec<[f64; 2]>,
    pub max_trains: u32,
    pub opened: u32,
    /// Époque à partir de laquelle la ligne peut être construite.
    #[serde(default)]
    pub era: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorKind {
    River,
    /// Contour schématique d'un pays.
    Border,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decor {
    pub kind: DecorKind,
    #[serde(default)]
    pub name: Option<String>,
    pub path: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Country {
    pub id: String,
    pub name: Localized,
    pub gauge_mm: u32,
    pub build_cost: f64,
    pub terrain: String,
    /// Époque à partir de laquelle la concession est proposée.
    #[serde(default)]
    pub era: Option<String>,
    #[serde(default)]
    pub concession_cost: f64,
    /// Multiplicateur de demande par catégorie de trafic.
    #[serde(default)]
    pub demand: BTreeMap<String, f64>,
    #[serde(default, rename = "station")]
    pub stations: Vec<Station>,
    #[serde(default, rename = "line")]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub decor: Vec<Decor>,
}

impl Country {
    pub fn demand_mult(&self, class_id: &str) -> f64 {
        self.demand.get(class_id).copied().unwrap_or(1.0)
    }

    /// Gare où commence un nouveau réseau dans ce pays.
    pub fn capital(&self) -> Option<&Station> {
        self.stations.first()
    }
}

/// Catégorie de trafic : un type de voyageurs, ou le fret.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficClass {
    pub id: String,
    pub name: Localized,
    /// Lettre affichée sur la carte.
    pub symbol: char,
    pub unit: String,
    pub share: f64,
    pub fare_mult: f64,
    pub elasticity: f64,
    pub comfort_weight: f64,
    pub punctuality_weight: f64,
    pub crowding_weight: f64,
    /// Multiplicateur de demande pour chaque heure de la journée.
    pub hourly: Vec<f64>,
}

impl TrafficClass {
    /// Effet du niveau de prix sur la demande (1.0 au tarif normal).
    pub fn price_factor(&self, fare_level: f64) -> f64 {
        fare_level.max(0.01).powf(-self.elasticity)
    }

    pub fn hourly_mult(&self, hour: u32) -> f64 {
        self.hourly.get(hour as usize % 24).copied().unwrap_or(1.0)
    }
}

/// Contenu d'un fichier de liste : une seule clé, un tableau d'entrées.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct ListFile<T> {
    #[serde(
        default,
        alias = "era",
        alias = "fuel",
        alias = "class",
        alias = "research"
    )]
    #[serde(
        alias = "locomotive",
        alias = "upgrade",
        alias = "country",
        alias = "event",
        alias = "staff",
        alias = "bonus"
    )]
    items: Vec<T>,
}

/// Sources TOML brutes par nom de fichier, avant validation.
#[derive(Debug, Clone, Default)]
pub struct Sources(pub BTreeMap<String, String>);

impl Sources {
    pub fn embedded() -> Sources {
        Sources(
            FILES
                .iter()
                .map(|(name, text)| (name.to_string(), text.to_string()))
                .collect(),
        )
    }

    /// Fichiers connus présents dans un dossier de mods.
    pub fn from_dir(dir: &Path) -> Result<Sources> {
        let mut sources = Sources::default();
        for (name, _) in FILES {
            let path = dir.join(name);
            if path.is_file() {
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("lecture de {} impossible", path.display()))?;
                sources.0.insert(name.to_string(), text);
            }
        }
        Ok(sources)
    }

    /// Sources ne contenant que le fichier donné (pratique pour un mod ou un test).
    pub fn single(name: &str, text: &str) -> Sources {
        Sources(BTreeMap::from([(name.to_string(), text.to_string())]))
    }

    fn get(&self, name: &str) -> &str {
        self.0.get(name).map_or("", String::as_str)
    }
}

#[derive(Debug, Clone)]
pub struct GameData {
    pub balance: Balance,
    pub eras: Vec<Era>,
    pub fuels: Vec<Fuel>,
    pub traffic: Vec<TrafficClass>,
    pub research: Vec<Research>,
    pub locomotives: Vec<Locomotive>,
    pub upgrades: Vec<Upgrade>,
    pub countries: Vec<Country>,
    pub events: Vec<GameEvent>,
    pub staff: Vec<Staff>,
    pub bonuses: Vec<PrestigeBonus>,
    pub crew: CrewRules,
}

fn parse<T: DeserializeOwned>(src: &str, file: &str) -> Result<T> {
    toml::from_str(src).with_context(|| format!("{file} : fichier TOML invalide"))
}

/// Lit une liste dans la base, puis remplace ou ajoute les entrées de la
/// surcouche qui portent le même identifiant.
fn load_list<T: DeserializeOwned>(
    file: &str,
    base: &Sources,
    overlay: Option<&Sources>,
    id: impl Fn(&T) -> &str,
) -> Result<Vec<T>> {
    let mut items = parse::<ListFile<T>>(base.get(file), file)?.items;
    if let Some(overlay) = overlay {
        for item in parse::<ListFile<T>>(overlay.get(file), &format!("mods/{file}"))?.items {
            match items.iter().position(|existing| id(existing) == id(&item)) {
                Some(i) => items[i] = item,
                None => items.push(item),
            }
        }
    }
    Ok(items)
}

impl GameData {
    /// Données embarquées seules.
    pub fn embedded() -> Result<GameData> {
        Self::from_sources(&Sources::embedded(), None)
    }

    /// Données embarquées, complétées par les fichiers de `mods_dir` s'il existe.
    pub fn load(mods_dir: Option<&Path>) -> Result<GameData> {
        let overlay = match mods_dir {
            Some(dir) if dir.is_dir() => Some(Sources::from_dir(dir)?),
            _ => None,
        };
        Self::from_sources(&Sources::embedded(), overlay.as_ref())
    }

    /// Construit et valide les données à partir d'une base et d'une surcouche.
    pub fn from_sources(base: &Sources, overlay: Option<&Sources>) -> Result<GameData> {
        // L'équilibrage se surcharge clé par clé.
        let mut balance: toml::Table = parse(base.get("balance.toml"), "balance.toml")?;
        if let Some(o) = overlay {
            balance.extend(parse::<toml::Table>(
                o.get("balance.toml"),
                "mods/balance.toml",
            )?);
        }
        let balance: Balance = toml::Value::Table(balance)
            .try_into()
            .context("balance.toml : clé manquante, inconnue ou de mauvais type")?;

        // Les règles des équipages se surchargent clé par clé, comme l'équilibrage.
        let mut crew: toml::Table = parse(base.get("crew.toml"), "crew.toml")?;
        if let Some(o) = overlay {
            crew.extend(parse::<toml::Table>(o.get("crew.toml"), "mods/crew.toml")?);
        }
        let crew: CrewRules = toml::Value::Table(crew)
            .try_into()
            .context("crew.toml : clé manquante, inconnue ou de mauvais type")?;

        let data = GameData {
            balance,
            crew,
            eras: load_list("eras.toml", base, overlay, |e: &Era| &e.id)?,
            fuels: load_list("fuels.toml", base, overlay, |f: &Fuel| &f.id)?,
            traffic: load_list("traffic.toml", base, overlay, |c: &TrafficClass| &c.id)?,
            research: load_list("research.toml", base, overlay, |r: &Research| &r.id)?,
            locomotives: load_list("locomotives.toml", base, overlay, |l: &Locomotive| &l.id)?,
            upgrades: load_list("upgrades.toml", base, overlay, |u: &Upgrade| &u.id)?,
            countries: load_list("countries.toml", base, overlay, |c: &Country| &c.id)?,
            events: load_list("events.toml", base, overlay, |e: &GameEvent| &e.id)?,
            staff: load_list("staff.toml", base, overlay, |s: &Staff| &s.id)?,
            bonuses: load_list("prestige.toml", base, overlay, |b: &PrestigeBonus| &b.id)?,
        };
        data.validate()?;
        Ok(data)
    }

    pub fn locomotive(&self, id: &str) -> Option<&Locomotive> {
        self.locomotives.iter().find(|l| l.id == id)
    }

    pub fn research(&self, id: &str) -> Option<&Research> {
        self.research.iter().find(|r| r.id == id)
    }

    pub fn upgrade(&self, id: &str) -> Option<&Upgrade> {
        self.upgrades.iter().find(|u| u.id == id)
    }

    pub fn fuel(&self, id: &str) -> Option<&Fuel> {
        self.fuels.iter().find(|f| f.id == id)
    }

    pub fn staff(&self, id: &str) -> Option<&Staff> {
        self.staff.iter().find(|s| s.id == id)
    }

    pub fn bonus(&self, id: &str) -> Option<&PrestigeBonus> {
        self.bonuses.iter().find(|b| b.id == id)
    }

    pub fn event(&self, id: &str) -> Option<&GameEvent> {
        self.events.iter().find(|e| e.id == id)
    }

    pub fn era(&self, id: &str) -> Option<&Era> {
        self.eras.iter().find(|e| e.id == id)
    }

    /// Rang d'une époque dans la chronologie.
    pub fn era_index(&self, id: &str) -> Option<usize> {
        self.eras.iter().position(|e| e.id == id)
    }

    pub fn stations(&self) -> impl Iterator<Item = &Station> {
        self.countries.iter().flat_map(|c| c.stations.iter())
    }

    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.countries.iter().flat_map(|c| c.lines.iter())
    }

    pub fn station(&self, id: &str) -> Option<&Station> {
        self.stations().find(|s| s.id == id)
    }

    pub fn class(&self, id: &str) -> Option<&TrafficClass> {
        self.traffic.iter().find(|c| c.id == id)
    }

    /// Pays auquel appartient une gare.
    pub fn country_of_station(&self, station_id: &str) -> Option<&Country> {
        self.countries
            .iter()
            .find(|c| c.stations.iter().any(|s| s.id == station_id))
    }

    pub fn country(&self, id: &str) -> Option<&Country> {
        self.countries.iter().find(|c| c.id == id)
    }

    /// Pays où le joueur commence : celui de la ligne de départ.
    pub fn home_country(&self) -> Option<&Country> {
        self.country_of_line(&self.balance.start_line)
    }

    /// Pays des deux extrémités d'une ligne (identiques pour une ligne intérieure).
    pub fn line_countries(&self, line: &Line) -> Vec<&Country> {
        let mut countries: Vec<&Country> = [&line.from, &line.to]
            .into_iter()
            .filter_map(|s| self.country_of_station(s))
            .collect();
        countries.dedup_by_key(|c| &c.id);
        countries
    }

    /// Demande de base d'une ligne à chacune de ses extrémités, en voyageurs par
    /// heure, avant les effets de l'époque, de l'heure, du prix et de l'humeur.
    ///
    /// Modèle gravitaire : elle croît avec le poids des deux villes et décroît
    /// avec la distance, si bien que les voyageurs-kilomètres offerts par une
    /// ligne dépendent surtout des villes qu'elle relie.
    pub fn line_demand(&self, line: &Line) -> f64 {
        let b = &self.balance;
        let weight = |id: &str| self.station(id).map_or(0.0, |s| s.population as f64);
        let cities = (weight(&line.from) * weight(&line.to)).sqrt() / 1000.0;
        cities
            * b.demand_per_1000_per_h
            * (b.demand_ref_km / line.length_km).powf(b.demand_distance_exp)
    }

    /// Pays auquel appartient une ligne.
    pub fn country_of_line(&self, line_id: &str) -> Option<&Country> {
        self.countries
            .iter()
            .find(|c| c.lines.iter().any(|l| l.id == line_id))
    }

    pub fn line(&self, id: &str) -> Option<&Line> {
        self.lines().find(|l| l.id == id)
    }

    /// Locomotives dont l'achat dépend de cette recherche.
    pub fn unlocked_by(&self, research_id: &str) -> Vec<&Locomotive> {
        self.locomotives
            .iter()
            .filter(|l| l.requires.iter().any(|r| r == research_id))
            .collect()
    }

    /// Améliorations de ligne que cette recherche rend possibles.
    pub fn upgrades_unlocked_by(&self, research_id: &str) -> Vec<&Upgrade> {
        self.upgrades
            .iter()
            .filter(|u| u.requires.iter().any(|r| r == research_id))
            .collect()
    }

    /// Tracé schématique d'une ligne : gare de départ, points de passage, arrivée.
    pub fn line_path(&self, line: &Line) -> Vec<[f64; 2]> {
        let mut pts = Vec::with_capacity(line.via.len() + 2);
        if let Some(s) = self.station(&line.from) {
            pts.push(s.pos);
        }
        pts.extend(line.via.iter().copied());
        if let Some(s) = self.station(&line.to) {
            pts.push(s.pos);
        }
        pts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_mod(file: &str, text: &str) -> Result<GameData> {
        GameData::from_sources(&Sources::embedded(), Some(&Sources::single(file, text)))
    }

    #[test]
    fn embedded_data_is_valid_and_complete() {
        let data = GameData::embedded().unwrap();
        assert_eq!(data.eras.len(), 6);
        assert_eq!(data.locomotives.len(), 13);
        assert!(data.research.len() >= 50);
        assert_eq!(data.traffic.len(), 4);
        assert!(data.events.len() >= 7);
        assert!(data.line("stockton_darlington").is_some());
        assert_eq!(data.unlocked_by("multitubular_boiler")[0].id, "rocket");
        assert_eq!(
            data.upgrades_unlocked_by("electric_traction")[0].id,
            "catenary"
        );
        assert_eq!(data.era_index("apogee"), Some(2));
        assert_eq!((data.staff.len(), data.bonuses.len()), (3, 5));
        let reputation = data.bonus("reputation").unwrap();
        assert_eq!(
            (
                reputation.cost_at(0),
                reputation.cost_at(1),
                reputation.cost_at(4)
            ),
            (2, 3, 10)
        );
        let shares: f64 = data.traffic.iter().map(|c| c.share).sum();
        assert!(
            (shares - 1.0).abs() < 1e-9,
            "les parts doivent totaliser 1 : {shares}"
        );
        assert_eq!(
            data.country_of_station("stockton")
                .unwrap()
                .demand_mult("freight"),
            1.3
        );
        assert_eq!(
            data.country_of_station("stockton")
                .unwrap()
                .demand_mult("unknown"),
            1.0
        );
        assert_eq!(
            data.country_of_line("stockton_darlington").unwrap().id,
            "uk"
        );
    }

    #[test]
    fn every_branch_has_research_in_every_era() {
        let data = GameData::embedded().unwrap();
        for era in &data.eras {
            for branch in Branch::ALL {
                assert!(
                    data.research
                        .iter()
                        .any(|r| r.era == era.id && r.branch == branch),
                    "aucune recherche {branch:?} dans l'époque {}",
                    era.id
                );
            }
            assert!(
                data.locomotives.iter().any(|l| l.era == era.id),
                "aucune machine en {}",
                era.id
            );
        }
    }

    #[test]
    fn historical_facts_are_in_the_data() {
        let data = GameData::embedded().unwrap();
        let speed = |id: &str| data.locomotive(id).unwrap().record_kmh;
        assert_eq!(speed("mallard"), Some(202.6));
        assert_eq!(speed("cc_7107"), Some(331.0));
        assert_eq!(speed("bb_9004"), Some(331.0));
        assert_eq!(speed("v150"), Some(574.8));
        assert_eq!(data.locomotive("shinkansen_0").unwrap().year, 1964);
        assert_eq!(data.locomotive("tgv_sud_est").unwrap().year, 1981);
        assert_eq!(data.event("rainhill_trials").unwrap().year, 1829);
        assert_eq!(data.event("orient_express").unwrap().year, 1883);
        // Pas de grande vitesse sans signalisation en cabine.
        for id in ["shinkansen_0", "tgv_sud_est"] {
            assert!(
                data.locomotive(id)
                    .unwrap()
                    .requires
                    .contains(&"cab_signalling".to_string())
            );
        }
        assert!(
            data.locomotives
                .iter()
                .filter(|l| l.speculative)
                .all(|l| l.era == "future")
        );
    }

    #[test]
    fn line_path_goes_from_station_to_station() {
        let data = GameData::embedded().unwrap();
        let line = data.line("stockton_darlington").unwrap();
        let path = data.line_path(line);
        assert_eq!(path.first(), Some(&data.station("stockton").unwrap().pos));
        assert_eq!(path.last(), Some(&data.station("darlington").unwrap().pos));
        assert_eq!(path.len(), line.via.len() + 2);
    }

    const PLANET: &str = r#"
        [[locomotive]]
        id = "planet"
        name = "Planet"
        builder = "Robert Stephenson & Co."
        year = 1830
        era = "pioneers"
        traction = "steam"
        wheels = "2-2-0"
        top_speed_kmh = 56.0
        power_kw = 25.0
        weight_t = 8.0
        capacity = 50
        cost = 2500.0
        upkeep_per_h = 4.0
        fuel = "coal"
        fuel_per_km = 0.011
        track_wear = 1.2
        history = { fr = "Ajoutée par un mod." }
    "#;

    #[test]
    fn mod_overrides_by_id_and_adds_new_entries() {
        let rocket = PLANET
            .replace("\"planet\"", "\"rocket\"")
            .replace("56.0", "60.0");
        let data = with_mod("locomotives.toml", &format!("{PLANET}{rocket}")).unwrap();
        assert_eq!(data.locomotives.len(), 14);
        assert_eq!(data.locomotive("rocket").unwrap().top_speed_kmh, 60.0);
        assert!(data.locomotive("rocket").unwrap().requires.is_empty());
        assert_eq!(data.locomotive("planet").unwrap().capacity, 50);

        let data = with_mod("balance.toml", "start_money = 999.0").unwrap();
        assert_eq!(data.balance.start_money, 999.0);
        assert_eq!(data.balance.time_scale, 120.0);
    }

    #[test]
    fn bad_references_are_rejected_with_a_clear_message() {
        let cases = [
            (PLANET.replace("\"coal\"", "\"peat\""), "peat"),
            (PLANET.replace("\"pioneers\"", "\"stone_age\""), "stone_age"),
            (
                format!("{PLANET}requires = [\"does_not_exist\"]"),
                "does_not_exist",
            ),
            (
                format!("{PLANET}requires_upgrade = \"monorail\""),
                "monorail",
            ),
        ];
        for (text, needle) in cases {
            let err = with_mod("locomotives.toml", &text).unwrap_err().to_string();
            assert!(err.contains(needle), "{err}");
        }
    }

    #[test]
    fn research_cannot_depend_on_a_later_era() {
        let text = r#"
            [[research]]
            id = "hand_signals"
            branch = "signalling"
            era = "pioneers"
            year = 1830
            cost = 500.0
            duration_h = 4.0
            requires = ["cab_signalling"]
            name = { fr = "Signaleurs" }
            description = { fr = "..." }
        "#;
        let err = with_mod("research.toml", text).unwrap_err().to_string();
        assert!(err.contains("cab_signalling"), "{err}");
    }

    #[test]
    fn demand_is_elastic_to_price() {
        let data = GameData::embedded().unwrap();
        let commuter = data.class("commuter").unwrap();
        let business = data.class("business").unwrap();
        assert_eq!(commuter.price_factor(1.0), 1.0);
        // Une hausse de prix fait fuir les navetteurs bien plus que la clientèle d'affaires.
        assert!(commuter.price_factor(1.5) < business.price_factor(1.5));
        assert!(commuter.price_factor(0.7) > 1.0);
        assert_eq!(commuter.hourly_mult(8), 2.8);
        assert_eq!(commuter.hourly_mult(3), 0.1);
    }

    #[test]
    fn fuel_price_is_fixed_without_a_market_and_bounded_with_one() {
        let data = GameData::embedded().unwrap();
        let coal = data.fuel("coal").unwrap();
        let (pioneers, steam) = (&data.eras[0], &data.eras[1]);
        assert_eq!(coal.price(0.0, pioneers), 12.0);
        assert_eq!(coal.price(1e6, pioneers), 12.0);
        let base = 12.0 * steam.cost_mult;
        let prices: Vec<f64> = (0..400)
            .map(|d| coal.price(d as f64 * 21_600.0, steam))
            .collect();
        let (min, max) = prices
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        assert!(
            min >= base * 0.7 - 1e-9 && max <= base * 1.3 + 1e-9,
            "{min}..{max}"
        );
        assert!(
            max - min > base * 0.3,
            "le cours doit vraiment varier : {min}..{max}"
        );
    }

    #[test]
    fn unknown_keys_and_bad_values_are_rejected() {
        assert!(with_mod("balance.toml", "tyme_scale = 2.0").is_err());
        assert!(with_mod("balance.toml", "time_scale = 0.0").is_err());
        let hours = include_str!("../../data/traffic.toml").replacen(
            "0.1, 0.1, 0.1, 0.1, 0.1, 0.4,",
            "0.1,",
            1,
        );
        let err = with_mod("traffic.toml", &hours).unwrap_err().to_string();
        assert!(err.contains("24"), "{err}");
    }

    #[test]
    fn event_choice_cost_is_the_sum_of_its_payments() {
        let data = GameData::embedded().unwrap();
        let orient = data.event("orient_express").unwrap();
        assert_eq!(orient.choices[0].cost(), 150_000.0);
        assert_eq!(orient.choices[1].cost(), 0.0);
    }
}
