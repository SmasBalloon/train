//! Données de jeu : chargées depuis des fichiers TOML embarqués dans le
//! binaire, puis complétées ou remplacées par les fichiers du dossier `mods/`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

const BALANCE: &str = include_str!("../../data/balance.toml");
const LOCOMOTIVES: &str = include_str!("../../data/locomotives.toml");
const RESEARCH: &str = include_str!("../../data/research.toml");
const COUNTRIES: &str = include_str!("../../data/countries.toml");

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
    pub station_waiting_cap: f64,
    pub boarding_per_min: f64,
    pub min_dwell_min: f64,
    pub max_wait_min: f64,
    pub wear_per_train_km: f64,
    pub wear_slow_threshold: f64,
    pub wear_min_speed_factor: f64,
    pub repair_cost_per_km: f64,
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
    pub weight_t: f64,
    pub capacity: u32,
    pub cost: f64,
    pub upkeep_per_h: f64,
    pub track_wear: f64,
    #[serde(default)]
    pub requires: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    SpeedMult { value: f64 },
    CapacityMult { value: f64 },
    FareMult { value: f64 },
    DemandMult { value: f64 },
    TrackWearMult { value: f64 },
    AutoDispatch,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Research {
    pub id: String,
    pub name: Localized,
    pub description: Localized,
    pub branch: Branch,
    pub year: u32,
    pub cost: f64,
    pub duration_h: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub effects: Vec<Effect>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorKind {
    River,
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
    #[serde(default, rename = "station")]
    pub stations: Vec<Station>,
    #[serde(default, rename = "line")]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub decor: Vec<Decor>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocomotiveFile {
    #[serde(default)]
    locomotive: Vec<Locomotive>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchFile {
    #[serde(default)]
    research: Vec<Research>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CountryFile {
    #[serde(default)]
    country: Vec<Country>,
}

/// Sources TOML brutes, avant validation.
pub struct Sources<'a> {
    pub balance: &'a str,
    pub locomotives: &'a str,
    pub research: &'a str,
    pub countries: &'a str,
}

impl Sources<'static> {
    pub fn embedded() -> Self {
        Sources {
            balance: BALANCE,
            locomotives: LOCOMOTIVES,
            research: RESEARCH,
            countries: COUNTRIES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GameData {
    pub balance: Balance,
    pub locomotives: Vec<Locomotive>,
    pub research: Vec<Research>,
    pub countries: Vec<Country>,
}

fn parse<T: DeserializeOwned>(src: &str, file: &str) -> Result<T> {
    toml::from_str(src).with_context(|| format!("{file} : fichier TOML invalide"))
}

/// Remplace les entrées de même identifiant, ajoute les nouvelles.
fn merge<T>(base: &mut Vec<T>, extra: Vec<T>, id: impl Fn(&T) -> &str) {
    for item in extra {
        match base.iter().position(|b| id(b) == id(&item)) {
            Some(i) => base[i] = item,
            None => base.push(item),
        }
    }
}

fn read_mod(dir: &Path, file: &str) -> Result<Option<String>> {
    let path = dir.join(file);
    if !path.is_file() {
        return Ok(None);
    }
    fs::read_to_string(&path)
        .map(Some)
        .with_context(|| format!("lecture de {} impossible", path.display()))
}

impl GameData {
    /// Données embarquées seules.
    pub fn embedded() -> Result<GameData> {
        Self::from_sources(&Sources::embedded(), None)
    }

    /// Données embarquées, complétées par les fichiers de `mods_dir` s'il existe.
    pub fn load(mods_dir: Option<&Path>) -> Result<GameData> {
        let mods = match mods_dir {
            Some(dir) if dir.is_dir() => Some((
                read_mod(dir, "balance.toml")?,
                read_mod(dir, "locomotives.toml")?,
                read_mod(dir, "research.toml")?,
                read_mod(dir, "countries.toml")?,
            )),
            _ => None,
        };
        let empty = String::new();
        let overlay = mods.as_ref().map(|(b, l, r, c)| Sources {
            balance: b.as_ref().unwrap_or(&empty),
            locomotives: l.as_ref().unwrap_or(&empty),
            research: r.as_ref().unwrap_or(&empty),
            countries: c.as_ref().unwrap_or(&empty),
        });
        Self::from_sources(&Sources::embedded(), overlay.as_ref())
    }

    /// Construit et valide les données à partir d'une base et d'une surcouche.
    pub fn from_sources(base: &Sources, overlay: Option<&Sources>) -> Result<GameData> {
        let mut balance: toml::Table = parse(base.balance, "balance.toml")?;
        let mut locomotives =
            parse::<LocomotiveFile>(base.locomotives, "locomotives.toml")?.locomotive;
        let mut research = parse::<ResearchFile>(base.research, "research.toml")?.research;
        let mut countries = parse::<CountryFile>(base.countries, "countries.toml")?.country;

        if let Some(o) = overlay {
            let extra: toml::Table = parse(o.balance, "mods/balance.toml")?;
            balance.extend(extra);
            merge(
                &mut locomotives,
                parse::<LocomotiveFile>(o.locomotives, "mods/locomotives.toml")?.locomotive,
                |l| &l.id,
            );
            merge(
                &mut research,
                parse::<ResearchFile>(o.research, "mods/research.toml")?.research,
                |r| &r.id,
            );
            merge(
                &mut countries,
                parse::<CountryFile>(o.countries, "mods/countries.toml")?.country,
                |c| &c.id,
            );
        }

        let balance: Balance = toml::Value::Table(balance)
            .try_into()
            .context("balance.toml : clé manquante, inconnue ou de mauvais type")?;

        let data = GameData {
            balance,
            locomotives,
            research,
            countries,
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

    pub fn stations(&self) -> impl Iterator<Item = &Station> {
        self.countries.iter().flat_map(|c| c.stations.iter())
    }

    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.countries.iter().flat_map(|c| c.lines.iter())
    }

    pub fn station(&self, id: &str) -> Option<&Station> {
        self.stations().find(|s| s.id == id)
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

    fn validate(&self) -> Result<()> {
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

        unique("locomotives.toml", self.locomotives.iter().map(|l| &l.id))?;
        unique("research.toml", self.research.iter().map(|r| &r.id))?;
        unique(
            "countries.toml (pays)",
            self.countries.iter().map(|c| &c.id),
        )?;
        unique("countries.toml (gares)", self.stations().map(|s| &s.id))?;
        unique("countries.toml (lignes)", self.lines().map(|l| &l.id))?;

        for l in &self.locomotives {
            let what = format!("locomotive « {} »", l.id);
            positive(&format!("{what} : top_speed_kmh"), l.top_speed_kmh)?;
            positive(&format!("{what} : weight_t"), l.weight_t)?;
            non_negative(&format!("{what} : cost"), l.cost)?;
            non_negative(&format!("{what} : upkeep_per_h"), l.upkeep_per_h)?;
            non_negative(&format!("{what} : track_wear"), l.track_wear)?;
            if l.capacity == 0 {
                bail!("{what} : capacity doit être au moins 1");
            }
            for r in &l.requires {
                if self.research(r).is_none() {
                    bail!("{what} : recherche requise « {r} » inconnue");
                }
            }
        }

        for r in &self.research {
            let what = format!("recherche « {} »", r.id);
            non_negative(&format!("{what} : cost"), r.cost)?;
            positive(&format!("{what} : duration_h"), r.duration_h)?;
            for req in &r.requires {
                if req == &r.id {
                    bail!("{what} : se requiert elle-même");
                }
                if self.research(req).is_none() {
                    bail!("{what} : prérequis « {req} » inconnu");
                }
            }
            for e in &r.effects {
                match e {
                    Effect::SpeedMult { value }
                    | Effect::CapacityMult { value }
                    | Effect::FareMult { value }
                    | Effect::DemandMult { value }
                    | Effect::TrackWearMult { value } => {
                        positive(&format!("{what} : multiplicateur d'effet"), *value)?
                    }
                    Effect::AutoDispatch => {}
                }
            }
        }

        for c in &self.countries {
            positive(&format!("pays « {} » : build_cost", c.id), c.build_cost)?;
            for l in &c.lines {
                let what = format!("ligne « {} »", l.id);
                positive(&format!("{what} : length_km"), l.length_km)?;
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

        let b = &self.balance;
        positive("balance : time_scale", b.time_scale)?;
        non_negative("balance : start_money", b.start_money)?;
        non_negative("balance : fare_per_km", b.fare_per_km)?;
        non_negative("balance : demand_per_1000_per_h", b.demand_per_1000_per_h)?;
        positive("balance : station_waiting_cap", b.station_waiting_cap)?;
        positive("balance : boarding_per_min", b.boarding_per_min)?;
        non_negative("balance : min_dwell_min", b.min_dwell_min)?;
        non_negative("balance : max_wait_min", b.max_wait_min)?;
        non_negative("balance : wear_per_train_km", b.wear_per_train_km)?;
        non_negative("balance : repair_cost_per_km", b.repair_cost_per_km)?;
        for (name, v) in [
            ("wear_slow_threshold", b.wear_slow_threshold),
            ("wear_min_speed_factor", b.wear_min_speed_factor),
        ] {
            if !(v > 0.0 && v <= 1.0) {
                bail!("balance : {name} doit être compris entre 0 (exclu) et 1");
            }
        }
        if self.locomotive(&b.start_locomotive).is_none() {
            bail!(
                "balance : start_locomotive « {} » inconnue",
                b.start_locomotive
            );
        }
        if self.line(&b.start_line).is_none() {
            bail!("balance : start_line « {} » inconnue", b.start_line);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay<'a>(locomotives: &'a str, balance: &'a str) -> Sources<'a> {
        Sources {
            balance,
            locomotives,
            research: "",
            countries: "",
        }
    }

    #[test]
    fn embedded_data_is_valid() {
        let data = GameData::embedded().unwrap();
        assert_eq!(data.locomotives.len(), 2);
        assert_eq!(data.research.len(), 3);
        assert!(data.line("stockton_darlington").is_some());
        assert_eq!(data.unlocked_by("multitubular_boiler")[0].id, "rocket");
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

    #[test]
    fn mod_overrides_by_id_and_adds_new_entries() {
        let mods = r#"
            [[locomotive]]
            id = "rocket"
            name = "Rocket"
            builder = "Mod"
            year = 1829
            era = "pioneers"
            traction = "steam"
            wheels = "0-2-2"
            top_speed_kmh = 60.0
            weight_t = 4.3
            capacity = 30
            cost = 10.0
            upkeep_per_h = 1.0
            track_wear = 1.0
            history = { fr = "Modifiée." }

            [[locomotive]]
            id = "planet"
            name = "Planet"
            builder = "Robert Stephenson & Co."
            year = 1830
            era = "pioneers"
            traction = "steam"
            wheels = "2-2-0"
            top_speed_kmh = 56.0
            weight_t = 8.0
            capacity = 50
            cost = 2500.0
            upkeep_per_h = 10.0
            track_wear = 1.2
            history = { fr = "Ajoutée par un mod." }
        "#;
        let data = GameData::from_sources(
            &Sources::embedded(),
            Some(&overlay(mods, "start_money = 999.0")),
        )
        .unwrap();
        assert_eq!(data.locomotives.len(), 3);
        assert_eq!(data.locomotive("rocket").unwrap().top_speed_kmh, 60.0);
        assert!(data.locomotive("rocket").unwrap().requires.is_empty());
        assert_eq!(data.balance.start_money, 999.0);
        assert_eq!(data.balance.time_scale, 120.0);
    }

    #[test]
    fn unknown_research_reference_is_rejected() {
        let mods = r#"
            [[locomotive]]
            id = "ghost"
            name = "Ghost"
            builder = "?"
            year = 1830
            era = "pioneers"
            traction = "steam"
            wheels = "0-4-0"
            top_speed_kmh = 30.0
            weight_t = 5.0
            capacity = 10
            cost = 1.0
            upkeep_per_h = 1.0
            track_wear = 1.0
            requires = ["does_not_exist"]
            history = { fr = "..." }
        "#;
        let err = GameData::from_sources(&Sources::embedded(), Some(&overlay(mods, "")))
            .unwrap_err()
            .to_string();
        assert!(err.contains("does_not_exist"), "{err}");
    }

    #[test]
    fn unknown_keys_and_bad_values_are_rejected() {
        assert!(
            GameData::from_sources(&Sources::embedded(), Some(&overlay("", "tyme_scale = 2.0")))
                .is_err()
        );
        assert!(
            GameData::from_sources(&Sources::embedded(), Some(&overlay("", "time_scale = 0.0")))
                .is_err()
        );
    }
}
