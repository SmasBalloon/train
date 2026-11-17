//! État de l'interface : partie en cours, caméra, sélection, fenêtres.

use std::collections::{HashMap, VecDeque};

use ratatui::Frame;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::data::{Branch, Country, GameData, Line, Research, Station};
use crate::sim::{CrewRole, Dir, OfflineReport, OrderOutcome, SimEvent, Train, World, advance};

use super::board::{self, Board};
use super::camera::{Camera, Polyline};
use super::format;
use super::glyphs::Glyphs;
use super::theme::{ColorMode, Theme};
use super::{map, popups, tree};

/// Durée d'affichage d'un message du journal, en secondes réelles.
pub const LOG_TTL: f64 = 12.0;
const LOG_MAX: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Good,
    Warn,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub text: String,
    pub kind: LogKind,
    pub at: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    None,
    /// Dépôt : acheter une ou plusieurs locomotives (`qty`) du même modèle.
    Shop {
        sel: usize,
        qty: u32,
    },
    /// Arbre de recherche, éventuellement filtré sur une branche.
    Research {
        sel: usize,
        branch: Option<Branch>,
    },
    /// Arbre de recherche dessiné : `sel` est le rang de la recherche dans les données.
    Tree {
        sel: usize,
    },
    /// Tarifs et horaires.
    Manage {
        sel: usize,
    },
    /// Chronologie des époques.
    Eras {
        sel: usize,
    },
    /// Améliorations de ligne.
    Upgrades {
        sel: usize,
    },
    /// Marché des combustibles.
    Fuel {
        sel: usize,
    },
    /// Réseau : pays, concessions, lignes à construire ou à choisir.
    Network {
        sel: usize,
    },
    /// Personnel : services d'automatisation à engager.
    Staff {
        sel: usize,
    },
    /// Compagnie : nationalisation (rang 0) et bonus permanents. `confirm`
    /// est vrai quand la nationalisation attend une seconde validation.
    Company {
        sel: usize,
        confirm: bool,
    },
    /// Équipages : conducteurs et contrôleurs employés, ou candidats à l'embauche.
    Crew {
        sel: usize,
        recruiting: bool,
    },
    /// Liste des trains.
    Fleet {
        sel: usize,
    },
    /// Sessions de formation programmées.
    Training {
        sel: usize,
    },
    /// Choix de la formation à programmer, parmi les compétences des deux métiers.
    TrainingNew {
        sel: usize,
    },
    /// Inscriptions à une session : `sel` est le rang de l'équipier du métier concerné.
    Enroll {
        session: u32,
        sel: usize,
    },
    /// Remplacement de la machine d'un train : `sel` est le rang de la
    /// locomotive proposée, `fleet` le rang du train dans la flotte.
    Replace {
        train: u32,
        fleet: usize,
        sel: usize,
    },
    /// Événement historique : choix à faire.
    Event {
        sel: usize,
    },
    /// Événement historique : conséquence du choix (rangs dans les données).
    Outcome {
        event: usize,
        choice: usize,
    },
    /// Résumé de ce qui s'est passé pendant l'absence.
    Welcome,
    Help,
}

/// Ligne de la fenêtre du réseau : un pays, ou l'une de ses lignes.
#[derive(Debug, Clone, Copy)]
pub enum NetworkRow<'a> {
    Country(&'a Country),
    Line(&'a Line),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Map,
    Board,
}

#[derive(Debug, Clone, Copy)]
pub struct UiOptions {
    pub ascii: bool,
    pub color: ColorMode,
}

pub struct App {
    pub world: World,
    pub data: GameData,
    pub theme: Theme,
    pub glyphs: Glyphs,
    pub camera: Camera,
    /// Échelle du cadrage automatique, référence de l'indicateur de zoom.
    pub fit_scale: f64,
    camera_ready: bool,
    pub selected: Option<u32>,
    pub follow: bool,
    pub popup: Popup,
    pub popup_msg: Option<String>,
    pub log: VecDeque<LogEntry>,
    pub paused: bool,
    pub quit: bool,
    pub screen: Screen,
    pub board: Board,
    /// Rang de la gare affichée sur le tableau.
    board_index: usize,
    /// Vrai quand une autre instance fait tourner la partie : on ne fait que regarder.
    pub read_only: bool,
    pub offline: Option<OfflineReport>,
    /// Secondes réelles écoulées depuis le lancement (animations, journal).
    pub time: f64,
    size: (u16, u16),
    drag: Option<(u16, u16)>,
    paths: HashMap<String, Polyline>,
    /// Ligne sur laquelle portent les commandes (achats, tarifs, améliorations).
    line: String,
    /// Orientation de l'arbre de recherche : époques de haut en bas, ou de gauche à droite.
    pub tree_vertical: bool,
}

impl App {
    pub fn new(world: World, data: GameData, opts: UiOptions) -> App {
        let paths = data
            .lines()
            .map(|l| (l.id.clone(), Polyline::new(data.line_path(l))))
            .collect();
        let selected = world.trains.first().map(|t| t.id);
        let line = world
            .trains
            .first()
            .map_or(data.balance.start_line.clone(), |t| t.line.clone());
        App {
            world,
            data,
            theme: Theme::new(opts.color),
            glyphs: Glyphs::new(opts.ascii),
            camera: Camera {
                cx: 0.0,
                cy: 0.0,
                scale: 8.0,
            },
            fit_scale: 8.0,
            camera_ready: false,
            selected,
            follow: false,
            popup: Popup::None,
            popup_msg: None,
            log: VecDeque::new(),
            paused: false,
            quit: false,
            screen: Screen::Map,
            board: Board::default(),
            board_index: 0,
            read_only: false,
            offline: None,
            time: 0.0,
            size: (80, 24),
            drag: None,
            paths,
            line,
            tree_vertical: true,
        }
    }

    /// Affiche le résumé d'absence s'il vaut la peine d'être lu.
    pub fn welcome_back(&mut self, report: OfflineReport) {
        if report.away_s >= 60.0 {
            self.offline = Some(report);
            self.popup = Popup::Welcome;
        }
    }

    /// Remplace la partie par une version plus récente lue sur le disque.
    pub fn replace_world(&mut self, world: World) {
        self.world = world;
    }

    /// Ligne sur laquelle portent les commandes : achats, tarifs, améliorations.
    pub fn line_id(&self) -> &str {
        if self.world.is_built(&self.line) {
            &self.line
        } else {
            &self.data.balance.start_line
        }
    }

    /// Gare affichée sur le tableau des départs, parmi les gares desservies.
    pub fn board_station(&self) -> Option<&Station> {
        let served: Vec<&Station> = self
            .data
            .stations()
            .filter(|s| self.world.is_served(&self.data, &s.id))
            .collect();
        if served.is_empty() {
            None
        } else {
            Some(served[self.board_index % served.len()])
        }
    }

    /// Lignes de la fenêtre du réseau : chaque pays, suivi de ses lignes.
    pub fn network_rows(&self) -> Vec<NetworkRow<'_>> {
        let mut rows = Vec::new();
        for country in &self.data.countries {
            rows.push(NetworkRow::Country(country));
            rows.extend(country.lines.iter().map(NetworkRow::Line));
        }
        rows
    }

    /// Sélectionne un train, et fait de sa ligne la ligne courante.
    fn select_train(&mut self, id: u32) {
        if let Some(train) = self.world.train(id) {
            self.line = train.line.clone();
            self.selected = Some(id);
        }
    }

    /// Recherches listées dans l'arbre : par époque, puis dans l'ordre des données.
    pub fn research_items(&self, branch: Option<Branch>) -> Vec<&Research> {
        let mut items: Vec<&Research> = self
            .data
            .research
            .iter()
            .filter(|r| branch.is_none_or(|b| r.branch == b))
            .collect();
        items.sort_by_key(|r| self.data.era_index(&r.era).unwrap_or(usize::MAX));
        items
    }

    pub fn path(&self, line_id: &str) -> Option<&Polyline> {
        self.paths.get(line_id)
    }

    /// Taille de la carte en sous-pixels.
    pub fn px_size(&self) -> (f64, f64) {
        (self.size.0 as f64 * 2.0, self.size.1 as f64 * 4.0)
    }

    pub fn train_name(&self, train: &Train) -> String {
        let name = self
            .data
            .locomotive(&train.loco)
            .map_or("Train", |l| l.name.as_str());
        format!("{name} #{}", train.id)
    }

    pub fn station_name(&self, id: &str) -> String {
        self.data
            .station(id)
            .map_or_else(|| id.to_string(), |s| s.name.clone())
    }

    /// Position et cap d'un train sur la carte, en coordonnées du monde.
    pub fn train_position(&self, train: &Train) -> Option<([f64; 2], [f64; 2])> {
        let line = self.data.line(&train.line)?;
        let path = self.paths.get(&train.line)?;
        let (pos, dir) = path.at(train.pos_km / line.length_km * path.length());
        Some(match train.dir {
            Dir::Outbound => (pos, dir),
            Dir::Inbound => (pos, [-dir[0], -dir[1]]),
        })
    }

    pub fn push_log(&mut self, text: impl Into<String>, kind: LogKind) {
        self.log.push_back(LogEntry {
            text: text.into(),
            kind,
            at: self.time,
        });
        while self.log.len() > LOG_MAX {
            self.log.pop_front();
        }
    }

    /// Avance la partie de `dt` secondes réelles.
    pub fn update(&mut self, dt: f64) {
        self.time += dt;
        let rows = board::rows_per_section(self.size.1);
        let targets = board::targets(self, rows);
        self.board.update(&targets, dt);
        if !self.paused {
            let events = advance(
                &mut self.world,
                &self.data,
                dt * self.data.balance.time_scale,
            );
            for event in events {
                self.log_event(event);
            }
        }
        // Un événement historique s'impose dès qu'aucune autre fenêtre n'est ouverte.
        if self.popup == Popup::None && !self.read_only && self.world.event_pending.is_some() {
            self.open(Popup::Event { sel: 0 });
        }
        if self
            .selected
            .is_some_and(|id| self.world.train(id).is_none())
        {
            self.selected = self.world.trains.first().map(|t| t.id);
        }
        if self.follow {
            match self
                .selected
                .and_then(|id| self.world.train(id))
                .and_then(|t| self.train_position(t))
            {
                Some((pos, _)) => {
                    self.camera.cx = pos[0];
                    self.camera.cy = pos[1];
                }
                None => self.follow = false,
            }
        }
    }

    fn log_event(&mut self, event: SimEvent) {
        let name = |app: &App, id: u32| {
            app.world
                .train(id)
                .map_or_else(|| "Un train".to_string(), |t| app.train_name(t))
        };
        match event {
            SimEvent::Arrived {
                train,
                station,
                passengers,
                revenue,
                delay_min,
            } => {
                let late = delay_min >= 1.0;
                let text = format!(
                    "{} arrive à {} : {passengers} à bord, +{}{}",
                    name(self, train),
                    self.station_name(&station),
                    format::money(revenue),
                    if late {
                        format!(", retard {} min", delay_min as u32)
                    } else {
                        String::new()
                    }
                );
                self.push_log(text, if late { LogKind::Info } else { LogKind::Good });
            }
            SimEvent::Departed { train, station } => {
                let text = format!(
                    "{} quitte {}",
                    name(self, train),
                    self.station_name(&station)
                );
                self.push_log(text, LogKind::Info);
            }
            SimEvent::ResearchDone { id } => {
                let label = self
                    .data
                    .research(&id)
                    .map_or(id.clone(), |r| r.name.fr.clone());
                self.push_log(format!("Recherche terminée : {label}"), LogKind::Good);
            }
            SimEvent::TrackWorn { line } => {
                let label = self
                    .data
                    .line(&line)
                    .map_or(line.clone(), |l| l.name.clone());
                self.push_log(
                    format!("Voie usée sur {label} : les trains ralentissent (m pour réparer)"),
                    LogKind::Warn,
                );
            }
            SimEvent::FuelOut { fuel } => {
                let label = self
                    .data
                    .fuel(&fuel)
                    .map_or(fuel.clone(), |f| f.name.fr.clone());
                self.push_log(
                    format!(
                        "{label} : stock épuisé, livraisons d'urgence au prix fort (c pour acheter)"
                    ),
                    LogKind::Warn,
                );
            }
            SimEvent::EventReady { .. } => {}
            SimEvent::Repaired { line, cost } => {
                let label = self
                    .data
                    .line(&line)
                    .map_or(line.clone(), |l| l.name.clone());
                self.push_log(
                    format!(
                        "Brigades de la voie : {label} remise à neuf ({})",
                        format::money(cost)
                    ),
                    LogKind::Info,
                );
            }
            SimEvent::FuelBought {
                fuel,
                quantity,
                cost,
            } => {
                let (label, unit) = self
                    .data
                    .fuel(&fuel)
                    .map_or((fuel.clone(), String::new()), |f| {
                        (f.name.fr.clone(), f.unit.clone())
                    });
                self.push_log(
                    format!(
                        "Approvisionnements : {} {unit} de {} achetés ({})",
                        format::quantity(quantity),
                        label.to_lowercase(),
                        format::money(cost)
                    ),
                    LogKind::Info,
                );
            }
            SimEvent::TrainSold {
                train,
                loco,
                refund,
            } => {
                let label = self
                    .data
                    .locomotive(&loco)
                    .map_or(loco.clone(), |l| l.name.clone());
                self.push_log(
                    format!(
                        "{label} #{train} revendue à son arrivée pour {}",
                        format::money(refund)
                    ),
                    LogKind::Info,
                );
            }
            SimEvent::TrainingDone {
                role,
                skill,
                passed,
                failed,
            } => {
                let names = |ids: &[u32]| {
                    ids.iter()
                        .filter_map(|id| self.world.crew_member(*id))
                        .map(|m| m.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let course = role.skills()[skill.min(2)].to_lowercase();
                let (ok, ko) = (names(&passed), names(&failed));
                if !ok.is_empty() {
                    self.push_log(format!("Formation {course} réussie : {ok}"), LogKind::Good);
                }
                if !ko.is_empty() {
                    self.push_log(
                        format!("Formation {course} : {ko} n'a rien retenu"),
                        LogKind::Warn,
                    );
                }
            }
            SimEvent::TrainReplaced { train, old, new } => {
                let name = |id: &str| {
                    self.data
                        .locomotive(id)
                        .map_or(id.to_string(), |l| l.name.clone())
                };
                let text = format!(
                    "Train #{train} : {} remplacée par {}",
                    name(&old),
                    name(&new)
                );
                self.push_log(text, LogKind::Good);
            }
        }
    }

    // ----- Caméra -----

    fn fit(&mut self, points: &[[f64; 2]]) {
        self.camera = Camera::fit(points, self.px_size());
        self.fit_scale = self.camera.scale;
        self.follow = false;
    }

    /// Cadre la ligne courante.
    fn fit_camera(&mut self) {
        let points = self
            .paths
            .get(self.line_id())
            .map(|p| p.points().to_vec())
            .unwrap_or_default();
        self.fit(&points);
    }

    /// Vue gare : la gare où se trouve (ou d'où vient) le train sélectionné.
    fn view_station(&mut self) {
        let line = self.data.line(self.line_id());
        let station = self
            .selected
            .and_then(|id| self.world.train(id))
            .and_then(|t| Some(t.dir.origin(self.data.line(&t.line)?)))
            .or(line.map(|l| l.from.as_str()))
            .and_then(|id| self.data.station(id));
        if let Some(pos) = station.map(|s| s.pos) {
            self.camera = Camera {
                cx: pos[0],
                cy: pos[1],
                scale: map::SCALE_STATION * 5.0,
            };
            self.follow = false;
        }
    }

    /// Vue pays : toutes les gares du pays de la ligne courante.
    fn view_country(&mut self) {
        let points: Vec<[f64; 2]> = self
            .data
            .country_of_line(self.line_id())
            .map(|c| c.stations.iter().map(|s| s.pos).collect())
            .unwrap_or_default();
        self.fit(&points);
    }

    /// Échelle à partir de laquelle on parle de vue pays : un peu plus près que
    /// le cadrage du continent entier, qui dépend de la taille du terminal.
    pub fn country_scale(&self) -> f64 {
        let points: Vec<[f64; 2]> = self.data.stations().map(|s| s.pos).collect();
        Camera::fit(&points, self.px_size()).scale * 1.3
    }

    /// Vue continent : tout le réseau possible.
    fn view_continent(&mut self) {
        let points: Vec<[f64; 2]> = self.data.stations().map(|s| s.pos).collect();
        self.fit(&points);
    }

    fn zoom(&mut self, factor: f64, anchor: Option<(f64, f64)>) {
        let size = self.px_size();
        let anchor = anchor.unwrap_or((size.0 / 2.0, size.1 / 2.0));
        self.camera.zoom_at(factor, anchor, size);
    }

    fn pan_cells(&mut self, dx: f64, dy: f64) {
        self.camera.pan_px(dx * 2.0, dy * 4.0);
        self.follow = false;
    }

    // ----- Sélection et actions -----

    fn select_next(&mut self, step: isize) {
        let n = self.world.trains.len() as isize;
        if n == 0 {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .and_then(|id| self.world.trains.iter().position(|t| t.id == id))
            .map_or(0, |i| (i as isize + step).rem_euclid(n));
        self.select_train(self.world.trains[current as usize].id);
    }

    /// Agit sur la ligne choisie dans la fenêtre du réseau : concession,
    /// construction, ou sélection d'une ligne déjà construite.
    fn network_action(&mut self, sel: usize) {
        let action = match self.network_rows().get(sel) {
            Some(NetworkRow::Country(country)) => {
                (country.id.clone(), country.name.fr.clone(), true)
            }
            Some(NetworkRow::Line(line)) => (line.id.clone(), line.name.clone(), false),
            None => return,
        };
        let (id, name, is_country) = action;
        if is_country {
            match self.world.buy_concession(&self.data, &id) {
                Ok(cost) => {
                    self.popup_msg = None;
                    self.push_log(
                        format!("Concession obtenue : {name} ({})", format::money(cost)),
                        LogKind::Good,
                    );
                }
                Err(e) => self.popup_msg = Some(e.to_string()),
            }
        } else if self.world.is_built(&id) {
            self.line = id;
            self.popup = Popup::None;
            self.fit_camera();
            self.push_log(format!("Ligne courante : {name}"), LogKind::Info);
        } else {
            match self.world.build_line(&self.data, &id) {
                Ok(cost) => {
                    self.line = id;
                    self.popup = Popup::None;
                    self.fit_camera();
                    self.push_log(
                        format!(
                            "{name} : ligne ouverte pour {}. Achète-lui des trains (a).",
                            format::money(cost)
                        ),
                        LogKind::Good,
                    );
                }
                Err(e) => self.popup_msg = Some(e.to_string()),
            }
        }
    }

    fn dispatch_selected(&mut self) {
        let Some(id) = self.selected else {
            self.push_log("Aucun train sélectionné", LogKind::Warn);
            return;
        };
        match self.world.dispatch(&self.data, id) {
            Ok(()) => {
                if let Some(train) = self.world.train(id) {
                    let dest = self
                        .data
                        .line(&train.line)
                        .map(|l| self.station_name(train.dir.destination(l)));
                    let text = format!(
                        "{} part pour {} avec {} à bord",
                        self.train_name(train),
                        dest.unwrap_or_default(),
                        train.count()
                    );
                    self.push_log(text, LogKind::Info);
                }
            }
            Err(e) => self.push_log(e.to_string(), LogKind::Warn),
        }
    }

    fn dispatch_all(&mut self) {
        match self.world.dispatch_all(&self.data) {
            0 => self.push_log("Aucun train prêt à partir", LogKind::Warn),
            1 => self.push_log("Départ donné à 1 train", LogKind::Info),
            n => self.push_log(format!("Départ donné à {n} trains"), LogKind::Info),
        }
    }

    fn repair(&mut self) {
        let line = self.line_id().to_string();
        match self.world.repair_line(&self.data, &line) {
            Ok(cost) => self.push_log(
                format!("Voie remise à neuf pour {}", format::money(cost)),
                LogKind::Good,
            ),
            Err(e) => self.push_log(e.to_string(), LogKind::Warn),
        }
    }

    fn buy(&mut self, index: usize, qty: u32) {
        let Some(loco) = self.data.locomotives.get(index) else {
            return;
        };
        let (id, name) = (loco.id.clone(), loco.name.clone());
        let line = self.line_id().to_string();
        match self.world.buy_locomotives(&self.data, &id, &line, qty) {
            Ok(trains) => {
                self.selected = trains.first().copied();
                self.popup = Popup::None;
                let text = match trains.len() {
                    1 => format!(
                        "{name} : livrée à quai. Affecte-lui un conducteur (s) pour qu'elle roule"
                    ),
                    n => format!(
                        "{n} × {name} livrées à quai. Affecte-leur un conducteur chacune (s) pour qu'elles roulent"
                    ),
                };
                self.push_log(text, LogKind::Warn);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn research(&mut self, sel: usize, branch: Option<Branch>) {
        let Some((id, name)) = self
            .research_items(branch)
            .get(sel)
            .map(|r| (r.id.clone(), r.name.fr.clone()))
        else {
            return;
        };
        match self.world.start_research(&self.data, &id) {
            Ok(()) => {
                self.popup = Popup::None;
                self.push_log(format!("Recherche lancée : {name}"), LogKind::Info);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn advance_era(&mut self) {
        let had_market = self.world.era(&self.data).rules.fuel_market;
        match self.world.advance_era(&self.data) {
            Ok(_) => {
                let era = self.world.era(&self.data);
                let (name, market) = (era.name.fr.clone(), era.rules.fuel_market);
                self.open(Popup::Eras {
                    sel: self.world.era_index(&self.data),
                });
                self.push_log(format!("Nouvelle époque : {name}"), LogKind::Good);
                if market && !had_market {
                    self.push_log(
                        "Le combustible s'achète désormais d'avance : ouvre le marché avec c",
                        LogKind::Warn,
                    );
                }
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn buy_upgrade(&mut self, sel: usize) {
        let Some(upgrade) = self.data.upgrades.get(sel) else {
            return;
        };
        let (id, name) = (upgrade.id.clone(), upgrade.name.fr.clone());
        let line = self.line_id().to_string();
        match self.world.buy_upgrade(&self.data, &line, &id) {
            Ok(cost) => {
                self.popup_msg = None;
                self.push_log(
                    format!("{name} : installée pour {}", format::money(cost)),
                    LogKind::Good,
                );
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn buy_fuel(&mut self, sel: usize, lots: u32) {
        let Some(fuel) = self.data.fuels.get(sel) else {
            return;
        };
        let (id, name, unit, lot) = (
            fuel.id.clone(),
            fuel.name.fr.clone(),
            fuel.unit.clone(),
            fuel.lot,
        );
        if !self.world.era(&self.data).rules.fuel_market {
            self.popup_msg =
                Some("À cette époque, le combustible est livré au jour le jour".to_string());
            return;
        }
        match self.world.buy_fuel(&self.data, &id, lots) {
            Ok(cost) => {
                self.popup_msg = None;
                let text = format!(
                    "{name} : {} {unit} achetés pour {}",
                    format::quantity(lot * lots as f64),
                    format::money(cost)
                );
                self.push_log(text, LogKind::Good);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Revend le train choisi dans la flotte : tout de suite s'il est à quai,
    /// sinon à son arrivée. Une seconde fois, annule ce qui était prévu.
    fn sell(&mut self, sel: usize) {
        let Some(train) = self.world.trains.get(sel) else {
            return;
        };
        let (id, name) = (train.id, self.train_name(train));
        if train.order.is_some() {
            let replacing = matches!(train.order, Some(crate::sim::TrainOrder::Replace { .. }));
            if self.world.cancel_order(id).is_ok() {
                self.popup_msg = None;
                let what = if replacing { "remplacement" } else { "vente" };
                self.push_log(format!("{name} : {what} annulé(e)"), LogKind::Info);
            }
            return;
        }
        match self.world.sell_train(&self.data, id) {
            Ok(OrderOutcome::Done(refund)) => {
                self.popup_msg = None;
                let count = self.world.trains.len();
                self.popup = Popup::Fleet {
                    sel: sel.min(count.saturating_sub(1)),
                };
                self.push_log(
                    format!("{name} revendue pour {}", format::money(refund)),
                    LogKind::Info,
                );
            }
            Ok(OrderOutcome::Scheduled) => {
                self.popup_msg = None;
                self.push_log(
                    format!("{name} sera revendue à son arrivée (x pour annuler)"),
                    LogKind::Info,
                );
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Remplace la machine d'un train par la locomotive de rang `sel`.
    fn replace(&mut self, train: u32, fleet: usize, sel: usize) {
        let (Some(loco), Some(current)) = (self.data.locomotives.get(sel), self.world.train(train))
        else {
            return;
        };
        let (loco_id, loco_name, old) =
            (loco.id.clone(), loco.name.clone(), self.train_name(current));
        match self.world.replace_train(&self.data, train, &loco_id) {
            Ok(outcome) => {
                self.open(Popup::Fleet { sel: fleet });
                let text = match outcome {
                    OrderOutcome::Done(net) if net >= 0.0 => {
                        format!("{old} remplacée par {loco_name} ({})", format::money(net))
                    }
                    OrderOutcome::Done(net) => format!(
                        "{old} remplacée par {loco_name} (+{} de reprise)",
                        format::money(-net)
                    ),
                    OrderOutcome::Scheduled => format!(
                        "{old} : {loco_name} prendra le relais à son arrivée (x pour annuler)"
                    ),
                };
                self.push_log(text, LogKind::Good);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Formations proposées : (métier, compétence) dans l'ordre affiché.
    pub fn courses() -> [(CrewRole, usize); 6] {
        [
            (CrewRole::Driver, 0),
            (CrewRole::Driver, 1),
            (CrewRole::Driver, 2),
            (CrewRole::Conductor, 0),
            (CrewRole::Conductor, 1),
            (CrewRole::Conductor, 2),
        ]
    }

    /// Équipiers du métier d'une session, dans l'ordre de la liste des inscriptions.
    pub fn enroll_candidates(&self, session: u32) -> Vec<&crate::sim::CrewMember> {
        let Some(t) = self.world.training(session) else {
            return Vec::new();
        };
        self.world
            .crew
            .iter()
            .filter(|m| m.role == t.role)
            .collect()
    }

    fn schedule_training(&mut self, sel: usize) {
        let Some(&(role, skill)) = Self::courses().get(sel) else {
            return;
        };
        match self.world.schedule_training(&self.data, role, skill) {
            Ok(id) => {
                self.open(Popup::Enroll {
                    session: id,
                    sel: 0,
                });
                self.push_log(
                    format!(
                        "Formation programmée : {} ({}), dans {} h. Inscris des équipiers.",
                        role.skills()[skill].to_lowercase(),
                        role.name().to_lowercase(),
                        format::quantity(self.data.crew.training_delay_h)
                    ),
                    LogKind::Info,
                );
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn cancel_training(&mut self, sel: usize) {
        let Some(id) = self.world.trainings.get(sel).map(|t| t.id) else {
            return;
        };
        match self.world.cancel_training(id) {
            Ok(refund) => {
                let left = self.world.trainings.len();
                self.open(Popup::Training {
                    sel: sel.min(left.saturating_sub(1)),
                });
                self.push_log(
                    format!("Formation annulée, {} remboursés", format::money(refund)),
                    LogKind::Info,
                );
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Inscrit l'équipier choisi, ou le désinscrit s'il l'était déjà.
    fn toggle_enrollment(&mut self, session: u32, sel: usize) {
        let Some((id, name)) = self
            .enroll_candidates(session)
            .get(sel)
            .map(|m| (m.id, m.name.clone()))
        else {
            return;
        };
        let enrolled = self.world.training(session).is_some_and(|t| t.has(id));
        let result = if enrolled {
            self.world
                .unenroll(session, id)
                .map(|r| format!("{name} désinscrit ({} remboursés)", format::money(r)))
        } else {
            self.world
                .enroll(&self.data, session, id)
                .map(|c| format!("{name} inscrit à la formation ({})", format::money(c)))
        };
        match result {
            Ok(text) => {
                self.popup_msg = None;
                self.push_log(text, LogKind::Info);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Fenêtre des équipages : embauche un candidat, ou affecte un équipier au
    /// train sélectionné (une seconde fois : retour en réserve).
    fn crew_action(&mut self, sel: usize, recruiting: bool) {
        if recruiting {
            let name = self.world.candidates.get(sel).map(|c| c.name.clone());
            match self.world.hire_crew(&self.data, sel) {
                Ok(id) => {
                    self.popup_msg = None;
                    let left = self.world.candidates.len();
                    self.popup = Popup::Crew {
                        sel: sel.min(left.saturating_sub(1)),
                        recruiting: true,
                    };
                    let name = name.unwrap_or_default();
                    self.push_log(format!("{name} rejoint la compagnie"), LogKind::Good);
                    let is_driver = self
                        .world
                        .crew_member(id)
                        .is_some_and(|m| m.role == CrewRole::Driver);
                    if is_driver {
                        self.push_log(
                            format!("{name} attend son affectation : Tab, puis Entrée sur le train sélectionné"),
                            LogKind::Info,
                        );
                    }
                }
                Err(e) => self.popup_msg = Some(e.to_string()),
            }
            return;
        }
        let Some(member) = self.world.crew.get(sel) else {
            return;
        };
        let (id, name, current) = (member.id, member.name.clone(), member.train);
        let Some(train) = self.selected else {
            self.popup_msg = Some("Sélectionne d'abord un train sur la carte (Tab)".to_string());
            return;
        };
        let target = if current == Some(train) {
            None
        } else {
            Some(train)
        };
        match self.world.assign_crew(id, target) {
            Ok(()) => {
                self.popup_msg = None;
                let train_name = self
                    .world
                    .train(train)
                    .map(|t| self.train_name(t))
                    .unwrap_or_default();
                let text = match target {
                    Some(_) => format!("{name} prend son service sur {train_name}"),
                    None => format!("{name} retourne en réserve"),
                };
                self.push_log(text, LogKind::Info);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn dismiss_crew(&mut self, sel: usize) {
        let Some(member) = self.world.crew.get(sel) else {
            return;
        };
        let (id, name) = (member.id, member.name.clone());
        if self.world.dismiss_crew(id).is_ok() {
            let left = self.world.crew.len();
            self.popup = Popup::Crew {
                sel: sel.min(left.saturating_sub(1)),
                recruiting: false,
            };
            self.push_log(format!("{name} quitte la compagnie"), LogKind::Info);
        }
    }

    fn hire(&mut self, sel: usize) {
        let Some(staff) = self.data.staff.get(sel) else {
            return;
        };
        let (id, name) = (staff.id.clone(), staff.name.fr.clone());
        match self.world.hire(&self.data, &id) {
            Ok(_) => {
                self.popup_msg = None;
                self.push_log(format!("{name} : service engagé"), LogKind::Good);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Fenêtre de la compagnie : rang 0, nationaliser (en deux temps) ; ensuite, les bonus.
    fn company_action(&mut self, sel: usize, confirm: bool) {
        if sel > 0 {
            let Some(bonus) = self.data.bonuses.get(sel - 1) else {
                return;
            };
            let (id, name) = (bonus.id.clone(), bonus.name.fr.clone());
            match self.world.buy_bonus(&self.data, &id) {
                Ok(cost) => {
                    self.popup_msg = None;
                    self.push_log(
                        format!("{name} : niveau acquis pour {cost} points"),
                        LogKind::Good,
                    );
                }
                Err(e) => self.popup_msg = Some(e.to_string()),
            }
            return;
        }
        if self.world.prestige_gain(&self.data) == 0 {
            self.popup_msg = Some(crate::sim::ActionError::PrestigeNotReady.to_string());
            return;
        }
        if !confirm {
            self.popup = Popup::Company { sel, confirm: true };
            self.popup_msg = Some(
                "Tout repart de 1825 : réseau, flotte, recherches et argent. Entrée pour confirmer, Échap pour renoncer."
                    .to_string(),
            );
            return;
        }
        match self.world.nationalise(&self.data) {
            Ok(gain) => {
                self.line = self.data.balance.start_line.clone();
                self.selected = self.world.trains.first().map(|t| t.id);
                self.log.clear();
                self.fit_camera();
                self.open(Popup::Company {
                    sel: 1,
                    confirm: false,
                });
                self.push_log(
                    format!("Compagnie nationalisée : +{gain} points de prestige. Une nouvelle compagnie voit le jour."),
                    LogKind::Good,
                );
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn choose(&mut self, sel: usize) {
        let Some(event) = self
            .world
            .event_pending
            .as_ref()
            .and_then(|id| self.data.events.iter().position(|e| &e.id == id))
        else {
            self.popup = Popup::None;
            return;
        };
        match self.world.resolve_event(&self.data, sel) {
            Ok(()) => self.open(Popup::Outcome { event, choice: sel }),
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    /// Recherche en cours : avancement (0 à 1) et temps réel restant, en secondes.
    pub fn research_progress(&self) -> Option<(&Research, f64, f64)> {
        let active = self.world.research_active.as_ref()?;
        let research = self.data.research(&active.id)?;
        let total = research.duration_h * 3600.0;
        let speed = self.world.modifiers(&self.data).research_speed * self.data.balance.time_scale;
        let left = (total - active.progress_s).max(0.0) / speed;
        Some((research, (active.progress_s / total).clamp(0.0, 1.0), left))
    }

    /// Ouvre l'arbre de recherche sur la recherche en cours, sinon sur la première.
    pub fn open_tree(&mut self) {
        let sel = self
            .world
            .research_active
            .as_ref()
            .and_then(|a| self.data.research.iter().position(|r| r.id == a.id))
            .unwrap_or(0);
        self.open(Popup::Tree { sel });
    }

    /// Lance la recherche de rang `index` dans les données.
    fn research_at(&mut self, index: usize) {
        let Some(research) = self.data.research.get(index) else {
            return;
        };
        let (id, name) = (research.id.clone(), research.name.fr.clone());
        match self.world.start_research(&self.data, &id) {
            Ok(()) => {
                self.popup_msg = None;
                self.push_log(format!("Recherche lancée : {name}"), LogKind::Info);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    pub fn open(&mut self, popup: Popup) {
        self.popup = popup;
        self.popup_msg = None;
    }

    /// Vrai si l'on peut agir sur la partie ; sinon prévient le joueur.
    fn can_act(&mut self) -> bool {
        if self.read_only {
            self.push_log(
                "Lecture seule : une autre instance de rail fait tourner la partie",
                LogKind::Warn,
            );
        }
        !self.read_only
    }

    /// Modifie le réglage choisi dans la fenêtre des tarifs et horaires.
    fn adjust(&mut self, sel: usize, direction: f64) {
        let line = self.line_id().to_string();
        let Some(state) = self.world.lines.get(&line) else {
            return;
        };
        let _ = match sel {
            0 => self
                .world
                .set_fare_level(&self.data, &line, state.fare_level + 0.1 * direction),
            _ => self
                .world
                .set_dwell(&self.data, &line, state.dwell_min + 5.0 * direction),
        };
    }

    // ----- Entrées -----

    pub fn on_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
            Event::Mouse(mouse) if self.popup == Popup::None && self.screen == Screen::Map => {
                self.on_mouse(mouse)
            }
            _ => {}
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        let code = key.code;
        let closes = matches!(code, KeyCode::Esc | KeyCode::Char('q'));
        let confirms = matches!(code, KeyCode::Enter | KeyCode::Char(' '));
        match self.popup {
            Popup::None => self.on_map_key(key),
            Popup::Help | Popup::Welcome | Popup::Outcome { .. } => self.popup = Popup::None,
            // Un événement historique exige une décision : on ne peut pas l'écarter.
            Popup::Event { sel } => {
                let count = self
                    .world
                    .event_pending
                    .as_ref()
                    .and_then(|id| self.data.event(id))
                    .map_or(0, |e| e.choices.len());
                if confirms {
                    self.choose(sel);
                } else if let Some(sel) = move_selection(code, sel, count) {
                    self.open(Popup::Event { sel });
                }
            }
            Popup::Manage { sel } => match code {
                _ if closes || code == KeyCode::Enter || code == KeyCode::Char('t') => {
                    self.popup = Popup::None
                }
                KeyCode::Left | KeyCode::Char('h' | '-') => self.adjust(sel, -1.0),
                KeyCode::Right | KeyCode::Char('l' | '+' | '=') => self.adjust(sel, 1.0),
                _ => {
                    if let Some(sel) = move_selection(code, sel, 2) {
                        self.open(Popup::Manage { sel });
                    }
                }
            },
            Popup::Shop { sel, qty } => {
                // La quantité va de 1 aux places libres sur la ligne.
                let room = self.world.line_room(&self.data, self.line_id()).max(1) as u32;
                let step = match code {
                    KeyCode::Left | KeyCode::Char('h' | '-') => Some(-1),
                    KeyCode::Right | KeyCode::Char('l' | '+' | '=') => Some(1),
                    _ => None,
                };
                if confirms {
                    self.buy(sel, qty);
                } else if let Some(step) = step {
                    let qty = qty.saturating_add_signed(step).clamp(1, room);
                    self.open(Popup::Shop { sel, qty });
                } else if closes || code == KeyCode::Char('a') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.data.locomotives.len()) {
                    self.open(Popup::Shop { sel, qty });
                }
            }
            Popup::Tree { sel } => {
                let direction = match code {
                    KeyCode::Left | KeyCode::Char('h') => Some(tree::Move::Left),
                    KeyCode::Right | KeyCode::Char('l') => Some(tree::Move::Right),
                    KeyCode::Up | KeyCode::Char('k') => Some(tree::Move::Up),
                    KeyCode::Down | KeyCode::Char('j') => Some(tree::Move::Down),
                    _ => None,
                };
                if confirms {
                    self.research_at(sel);
                } else if code == KeyCode::Tab {
                    self.tree_vertical = !self.tree_vertical;
                } else if code == KeyCode::Char('v') {
                    // Vue en liste, sur la même recherche.
                    let sel = self
                        .data
                        .research
                        .get(sel)
                        .and_then(|r| {
                            self.research_items(None)
                                .iter()
                                .position(|item| item.id == r.id)
                        })
                        .unwrap_or(0);
                    self.open(Popup::Research { sel, branch: None });
                } else if closes || code == KeyCode::Char('r') {
                    self.popup = Popup::None;
                } else if let Some(direction) = direction {
                    let layout = tree::TreeLayout::new(&self.data);
                    if let Some(sel) = tree::step(&layout, sel, direction, self.tree_vertical) {
                        self.open(Popup::Tree { sel });
                    }
                }
            }
            Popup::Research { sel, branch } if code == KeyCode::Char('v') => {
                // Retour à l'arbre, sur la même recherche.
                let sel = self
                    .research_items(branch)
                    .get(sel)
                    .and_then(|item| self.data.research.iter().position(|r| r.id == item.id))
                    .unwrap_or(0);
                self.open(Popup::Tree { sel });
            }
            Popup::Research { sel, branch } => {
                if confirms {
                    self.research(sel, branch);
                } else if closes || code == KeyCode::Char('r') {
                    self.popup = Popup::None;
                } else if matches!(
                    code,
                    KeyCode::Tab | KeyCode::Right | KeyCode::Left | KeyCode::BackTab
                ) {
                    let forward = matches!(code, KeyCode::Tab | KeyCode::Right);
                    self.open(Popup::Research {
                        sel: 0,
                        branch: cycle_branch(branch, forward),
                    });
                } else if let Some(sel) =
                    move_selection(code, sel, self.research_items(branch).len())
                {
                    self.open(Popup::Research { sel, branch });
                }
            }
            Popup::Eras { sel } => {
                if confirms {
                    self.advance_era();
                } else if closes || code == KeyCode::Char('e') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.data.eras.len()) {
                    self.open(Popup::Eras { sel });
                }
            }
            Popup::Upgrades { sel } => {
                if confirms {
                    self.buy_upgrade(sel);
                } else if closes || code == KeyCode::Char('u') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.data.upgrades.len()) {
                    self.open(Popup::Upgrades { sel });
                }
            }
            Popup::Fuel { sel } => {
                if confirms {
                    self.buy_fuel(sel, 1);
                } else if code == KeyCode::Char('x') {
                    self.buy_fuel(sel, 10);
                } else if closes || code == KeyCode::Char('c') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.data.fuels.len()) {
                    self.open(Popup::Fuel { sel });
                }
            }
            Popup::Network { sel } => {
                if confirms {
                    self.network_action(sel);
                } else if closes || code == KeyCode::Char('n') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.network_rows().len()) {
                    self.open(Popup::Network { sel });
                }
            }
            Popup::Staff { sel } => {
                if confirms {
                    self.hire(sel);
                } else if closes || code == KeyCode::Char('g') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.data.staff.len()) {
                    self.open(Popup::Staff { sel });
                }
            }
            Popup::Company { sel, confirm } => {
                if confirms {
                    self.company_action(sel, confirm);
                } else if confirm {
                    // Toute autre touche annule la nationalisation en attente.
                    self.open(Popup::Company {
                        sel,
                        confirm: false,
                    });
                } else if closes || code == KeyCode::Char('o') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.data.bonuses.len() + 1) {
                    self.open(Popup::Company {
                        sel,
                        confirm: false,
                    });
                }
            }
            Popup::Crew { sel, recruiting } => {
                let count = if recruiting {
                    self.world.candidates.len()
                } else {
                    self.world.crew.len()
                };
                if confirms {
                    self.crew_action(sel, recruiting);
                } else if code == KeyCode::Tab {
                    self.open(Popup::Crew {
                        sel: 0,
                        recruiting: !recruiting,
                    });
                } else if code == KeyCode::Char('x') && !recruiting {
                    self.dismiss_crew(sel);
                } else if code == KeyCode::Char('f') {
                    self.open(Popup::Training { sel: 0 });
                } else if closes || code == KeyCode::Char('s') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, count) {
                    self.open(Popup::Crew { sel, recruiting });
                }
            }
            Popup::Fleet { sel } => {
                if confirms {
                    if let Some(id) = self.world.trains.get(sel).map(|t| t.id) {
                        self.select_train(id);
                        self.follow = true;
                        self.popup = Popup::None;
                    }
                } else if matches!(code, KeyCode::Char('x') | KeyCode::Delete) {
                    self.sell(sel);
                } else if code == KeyCode::Char('r') {
                    if let Some(train) = self.world.trains.get(sel) {
                        // On ouvre sur la machine actuelle du train.
                        let current = self
                            .data
                            .locomotives
                            .iter()
                            .position(|l| l.id == train.loco)
                            .unwrap_or(0);
                        self.open(Popup::Replace {
                            train: train.id,
                            fleet: sel,
                            sel: current,
                        });
                    }
                } else if closes || code == KeyCode::Char('v') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.world.trains.len()) {
                    self.open(Popup::Fleet { sel });
                }
            }
            Popup::Training { sel } => {
                if confirms {
                    if let Some(id) = self.world.trainings.get(sel).map(|t| t.id) {
                        self.open(Popup::Enroll {
                            session: id,
                            sel: 0,
                        });
                    }
                } else if code == KeyCode::Char('n') {
                    self.open(Popup::TrainingNew { sel: 0 });
                } else if code == KeyCode::Char('x') {
                    self.cancel_training(sel);
                } else if closes || code == KeyCode::Char('i') {
                    self.popup = Popup::None;
                } else if let Some(sel) = move_selection(code, sel, self.world.trainings.len()) {
                    self.open(Popup::Training { sel });
                }
            }
            Popup::TrainingNew { sel } => {
                if confirms {
                    self.schedule_training(sel);
                } else if closes {
                    self.open(Popup::Training { sel: 0 });
                } else if let Some(sel) = move_selection(code, sel, Self::courses().len()) {
                    self.open(Popup::TrainingNew { sel });
                }
            }
            Popup::Enroll { session, sel } => {
                let back = || {
                    let sel = self
                        .world
                        .trainings
                        .iter()
                        .position(|t| t.id == session)
                        .unwrap_or(0);
                    Popup::Training { sel }
                };
                if self.world.training(session).is_none() || closes {
                    let popup = back();
                    self.open(popup);
                } else if confirms {
                    self.toggle_enrollment(session, sel);
                } else if let Some(sel) =
                    move_selection(code, sel, self.enroll_candidates(session).len())
                {
                    self.open(Popup::Enroll { session, sel });
                }
            }
            Popup::Replace { train, fleet, sel } => {
                if self.world.train(train).is_none() {
                    self.open(Popup::Fleet { sel: 0 });
                } else if confirms {
                    self.replace(train, fleet, sel);
                } else if closes || code == KeyCode::Char('r') {
                    self.open(Popup::Fleet { sel: fleet });
                } else if let Some(sel) = move_selection(code, sel, self.data.locomotives.len()) {
                    self.open(Popup::Replace { train, fleet, sel });
                }
            }
        }
    }

    fn on_board_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('b') | KeyCode::Esc => self.screen = Screen::Map,
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => self.board_index += 1,
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
                let count = self.data.stations().count().max(1);
                self.board_index = (self.board_index % count + count - 1) % count;
            }
            KeyCode::Char('p') if !self.read_only => self.paused = !self.paused,
            KeyCode::Char('?') => self.open(Popup::Help),
            _ => {}
        }
    }

    fn on_map_key(&mut self, key: KeyEvent) {
        if self.screen == Screen::Board {
            return self.on_board_key(key);
        }
        // Les commandes qui modifient la partie sont refusées en lecture seule.
        let acts = matches!(
            key.code,
            KeyCode::Char(
                'd' | ' '
                    | 'a'
                    | 'r'
                    | 't'
                    | 'm'
                    | 'p'
                    | 'e'
                    | 'u'
                    | 'c'
                    | 'v'
                    | 'n'
                    | 'g'
                    | 'o'
                    | 's'
                    | 'i'
            ) | KeyCode::Enter
        );
        if acts && !self.can_act() {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('d') | KeyCode::Enter => self.dispatch_selected(),
            KeyCode::Char(' ') => self.dispatch_all(),
            KeyCode::Tab => self.select_next(1),
            KeyCode::BackTab => self.select_next(-1),
            KeyCode::Char('a') => self.open(Popup::Shop { sel: 0, qty: 1 }),
            KeyCode::Char('r') => self.open_tree(),
            KeyCode::Char('t') => self.open(Popup::Manage { sel: 0 }),
            KeyCode::Char('e') => self.open(Popup::Eras {
                sel: self.world.era_index(&self.data),
            }),
            KeyCode::Char('u') => self.open(Popup::Upgrades { sel: 0 }),
            KeyCode::Char('c') => self.open(Popup::Fuel { sel: 0 }),
            KeyCode::Char('g') => self.open(Popup::Staff { sel: 0 }),
            KeyCode::Char('o') => self.open(Popup::Company {
                sel: 0,
                confirm: false,
            }),
            KeyCode::Char('s') => {
                // Sans personne à affecter, on ouvre directement sur le recrutement.
                let recruiting = self.world.crew.is_empty();
                self.open(Popup::Crew { sel: 0, recruiting });
            }
            KeyCode::Char('i') => self.open(Popup::Training { sel: 0 }),
            KeyCode::Char('v') => {
                let sel = self
                    .selected
                    .and_then(|id| self.world.trains.iter().position(|t| t.id == id));
                self.open(Popup::Fleet {
                    sel: sel.unwrap_or(0),
                });
            }
            KeyCode::Char('b') => self.screen = Screen::Board,
            KeyCode::Char('?') => self.open(Popup::Help),
            KeyCode::Char('m') => self.repair(),
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('f') => self.follow = !self.follow && self.selected.is_some(),
            KeyCode::Char('0') | KeyCode::Home => self.fit_camera(),
            KeyCode::Char('1') => self.view_station(),
            KeyCode::Char('2') => self.view_country(),
            KeyCode::Char('3') => self.view_continent(),
            KeyCode::Char('n') => {
                // On ouvre la fenêtre sur la ligne courante.
                let current = self.line_id();
                let sel = self
                    .network_rows()
                    .iter()
                    .position(|row| matches!(row, NetworkRow::Line(l) if l.id == current))
                    .unwrap_or(0);
                self.open(Popup::Network { sel });
            }
            KeyCode::Char('+' | '=') | KeyCode::PageUp => self.zoom(1.25, None),
            KeyCode::Char('-' | '_') | KeyCode::PageDown => self.zoom(0.8, None),
            KeyCode::Left | KeyCode::Char('h') => self.pan_cells(-8.0, 0.0),
            KeyCode::Right | KeyCode::Char('l') => self.pan_cells(8.0, 0.0),
            KeyCode::Up | KeyCode::Char('k') => self.pan_cells(0.0, -4.0),
            KeyCode::Down | KeyCode::Char('j') => self.pan_cells(0.0, 4.0),
            _ => {}
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) {
        let anchor = (
            mouse.column as f64 * 2.0 + 1.0,
            mouse.row as f64 * 4.0 + 2.0,
        );
        match mouse.kind {
            MouseEventKind::ScrollUp => self.zoom(1.25, Some(anchor)),
            MouseEventKind::ScrollDown => self.zoom(0.8, Some(anchor)),
            MouseEventKind::Down(MouseButton::Left) => self.drag = Some((mouse.column, mouse.row)),
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((x, y)) = self.drag {
                    self.pan_cells(x as f64 - mouse.column as f64, y as f64 - mouse.row as f64);
                }
                self.drag = Some((mouse.column, mouse.row));
            }
            MouseEventKind::Up(_) => self.drag = None,
            _ => {}
        }
    }

    // ----- Rendu -----

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.size = (area.width, area.height);
        if !self.camera_ready && area.width > 0 && area.height > 0 {
            self.fit_camera();
            self.camera_ready = true;
        }
        match self.screen {
            Screen::Map => map::draw(self, frame.buffer_mut(), area),
            Screen::Board => board::draw(self, frame.buffer_mut(), area),
        }
        popups::draw(self, frame.buffer_mut(), area);
        if self.glyphs.ascii {
            map::fold_to_ascii(frame.buffer_mut(), area);
        }
    }
}

/// Branche suivante du filtre de l'arbre de recherche : toutes, puis chacune.
fn cycle_branch(current: Option<Branch>, forward: bool) -> Option<Branch> {
    let options: Vec<Option<Branch>> = std::iter::once(None)
        .chain(Branch::ALL.into_iter().map(Some))
        .collect();
    let index = options.iter().position(|b| *b == current).unwrap_or(0);
    let step = if forward { 1 } else { options.len() - 1 };
    options[(index + step) % options.len()]
}

fn move_selection(code: KeyCode, sel: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let page = 8;
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some((sel + count - 1) % count),
        KeyCode::Down | KeyCode::Char('j') => Some((sel + 1) % count),
        KeyCode::PageUp => Some(sel.saturating_sub(page)),
        KeyCode::PageDown => Some((sel + page).min(count - 1)),
        KeyCode::Home => Some(0),
        KeyCode::End => Some(count - 1),
        _ => None,
    }
}
