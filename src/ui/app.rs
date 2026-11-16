//! État de l'interface : partie en cours, caméra, sélection, fenêtres.

use std::collections::{HashMap, VecDeque};

use ratatui::Frame;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::data::GameData;
use crate::sim::{Dir, SimEvent, Train, World, advance};

use super::camera::{Camera, Polyline};
use super::format;
use super::glyphs::Glyphs;
use super::theme::{ColorMode, Theme};
use super::{map, popups};

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
    Shop { sel: usize },
    Research { sel: usize },
    Help,
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
    /// Secondes réelles écoulées depuis le lancement (animations, journal).
    pub time: f64,
    size: (u16, u16),
    drag: Option<(u16, u16)>,
    paths: HashMap<String, Polyline>,
}

impl App {
    pub fn new(world: World, data: GameData, opts: UiOptions) -> App {
        let paths = data
            .lines()
            .map(|l| (l.id.clone(), Polyline::new(data.line_path(l))))
            .collect();
        let selected = world.trains.first().map(|t| t.id);
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
            time: 0.0,
            size: (80, 24),
            drag: None,
            paths,
        }
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

    fn station_name(&self, id: &str) -> String {
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
            } => {
                let text = format!(
                    "{} arrive à {} : {passengers} voy., +{}",
                    name(self, train),
                    self.station_name(&station),
                    format::money(revenue)
                );
                self.push_log(text, LogKind::Good);
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
        }
    }

    // ----- Caméra -----

    fn fit_camera(&mut self) {
        let mut points: Vec<[f64; 2]> = self.data.stations().map(|s| s.pos).collect();
        points.extend(self.paths.values().flat_map(|p| p.points().iter().copied()));
        self.camera = Camera::fit(&points, self.px_size());
        self.fit_scale = self.camera.scale;
        self.follow = false;
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
        self.selected = Some(self.world.trains[current as usize].id);
    }

    fn dispatch_selected(&mut self) {
        let Some(id) = self.selected else {
            self.push_log("Aucun train sélectionné", LogKind::Warn);
            return;
        };
        match self.world.dispatch(id) {
            Ok(()) => {
                if let Some(train) = self.world.train(id) {
                    let dest = self
                        .data
                        .line(&train.line)
                        .map(|l| self.station_name(train.dir.destination(l)));
                    let text = format!(
                        "{} part pour {} avec {} voy.",
                        self.train_name(train),
                        dest.unwrap_or_default(),
                        train.passengers
                    );
                    self.push_log(text, LogKind::Info);
                }
            }
            Err(e) => self.push_log(e.to_string(), LogKind::Warn),
        }
    }

    fn dispatch_all(&mut self) {
        match self.world.dispatch_all() {
            0 => self.push_log("Aucun train à quai", LogKind::Warn),
            1 => self.push_log("Départ donné à 1 train", LogKind::Info),
            n => self.push_log(format!("Départ donné à {n} trains"), LogKind::Info),
        }
    }

    fn repair(&mut self) {
        let line = self.data.balance.start_line.clone();
        match self.world.repair_line(&self.data, &line) {
            Ok(cost) => self.push_log(
                format!("Voie remise à neuf pour {}", format::money(cost)),
                LogKind::Good,
            ),
            Err(e) => self.push_log(e.to_string(), LogKind::Warn),
        }
    }

    fn buy(&mut self, index: usize) {
        let Some(loco) = self.data.locomotives.get(index) else {
            return;
        };
        let (id, name) = (loco.id.clone(), loco.name.clone());
        let line = self.data.balance.start_line.clone();
        match self.world.buy_locomotive(&self.data, &id, &line) {
            Ok(train) => {
                self.selected = Some(train);
                self.popup = Popup::None;
                self.push_log(
                    format!("{name} achetée : elle attend à quai"),
                    LogKind::Good,
                );
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn research(&mut self, index: usize) {
        let Some(research) = self.data.research.get(index) else {
            return;
        };
        let (id, name) = (research.id.clone(), research.name.fr.clone());
        match self.world.start_research(&self.data, &id) {
            Ok(()) => {
                self.popup = Popup::None;
                self.push_log(format!("Recherche lancée : {name}"), LogKind::Info);
            }
            Err(e) => self.popup_msg = Some(e.to_string()),
        }
    }

    fn open(&mut self, popup: Popup) {
        self.popup = popup;
        self.popup_msg = None;
    }

    // ----- Entrées -----

    pub fn on_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
            Event::Mouse(mouse) if self.popup == Popup::None => self.on_mouse(mouse),
            _ => {}
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.popup {
            Popup::None => self.on_map_key(key),
            Popup::Help => self.popup = Popup::None,
            Popup::Shop { sel } => {
                let count = self.data.locomotives.len();
                match key.code {
                    KeyCode::Enter | KeyCode::Char(' ') => self.buy(sel),
                    KeyCode::Esc | KeyCode::Char('q' | 'a') => self.popup = Popup::None,
                    KeyCode::Char('r') => self.open(Popup::Research { sel: 0 }),
                    code => {
                        if let Some(sel) = move_selection(code, sel, count) {
                            self.open(Popup::Shop { sel });
                        }
                    }
                }
            }
            Popup::Research { sel } => {
                let count = self.data.research.len();
                match key.code {
                    KeyCode::Enter | KeyCode::Char(' ') => self.research(sel),
                    KeyCode::Esc | KeyCode::Char('q' | 'r') => self.popup = Popup::None,
                    KeyCode::Char('a') => self.open(Popup::Shop { sel: 0 }),
                    code => {
                        if let Some(sel) = move_selection(code, sel, count) {
                            self.open(Popup::Research { sel });
                        }
                    }
                }
            }
        }
    }

    fn on_map_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('d') | KeyCode::Enter => self.dispatch_selected(),
            KeyCode::Char(' ') => self.dispatch_all(),
            KeyCode::Tab => self.select_next(1),
            KeyCode::BackTab => self.select_next(-1),
            KeyCode::Char('a') => self.open(Popup::Shop { sel: 0 }),
            KeyCode::Char('r') => self.open(Popup::Research { sel: 0 }),
            KeyCode::Char('?') => self.open(Popup::Help),
            KeyCode::Char('m') => self.repair(),
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('f') => self.follow = !self.follow && self.selected.is_some(),
            KeyCode::Char('0') | KeyCode::Home => self.fit_camera(),
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
        map::draw(self, frame.buffer_mut(), area);
        popups::draw(self, frame.buffer_mut(), area);
        if self.glyphs.ascii {
            map::fold_to_ascii(frame.buffer_mut(), area);
        }
    }
}

fn move_selection(code: KeyCode, sel: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some((sel + count - 1) % count),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => Some((sel + 1) % count),
        _ => None,
    }
}
