//! Tableau des départs et arrivées, façon affichage à palettes : chaque
//! caractère défile dans l'alphabet jusqu'à atteindre sa valeur.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::data::Station;
use crate::sim::TrainState;

use super::app::App;
use super::format;
use super::map::{ascii_fold, fill_row, put_str};

/// Ordre de défilement des palettes.
const ALPHABET: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:/#-'.";

/// Largeurs des colonnes : heure, gare, train, voie, charge, état.
const COLUMNS: [usize; 6] = [5, 14, 20, 4, 7, 16];
const GAP: usize = 2;
pub const WIDTH: usize = 5 + 14 + 20 + 4 + 7 + 16 + 5 * GAP;
/// Colonne où commence l'état, pour le colorer à part.
const STATUS_AT: usize = WIDTH - 16;

/// Intervalle entre deux battements de palettes, en secondes réelles.
const FLAP_PERIOD: f64 = 0.035;

/// Palette suivante sur le chemin de `current` vers `target`.
fn flip(current: char, target: char) -> char {
    if current == target {
        return current;
    }
    let position = |c: char| ALPHABET.iter().position(|&a| a as char == c);
    match (position(current), position(target)) {
        (Some(i), Some(_)) => ALPHABET[(i + 1) % ALPHABET.len()] as char,
        // Caractère hors alphabet : la palette se pose directement.
        _ => target,
    }
}

/// Texte tel qu'il s'affiche sur le tableau : capitales sans accents.
fn board_text(text: &str) -> String {
    text.chars()
        .map(ascii_fold)
        .collect::<String>()
        .to_uppercase()
}

fn row(cells: [&str; 6]) -> String {
    let mut out = String::with_capacity(WIDTH);
    for (i, (cell, width)) in cells.iter().zip(COLUMNS).enumerate() {
        if i > 0 {
            out.extend(std::iter::repeat_n(' ', GAP));
        }
        let text: String = board_text(cell).chars().take(width).collect();
        out.push_str(&format!("{text:<width$}"));
    }
    out
}

fn clock_text(clock: f64) -> String {
    let (h, m) = crate::sim::hour_minute(clock);
    format!("{h:02}:{m:02}")
}

/// Retard annoncé par tranches de cinq minutes, comme sur un vrai tableau :
/// un chiffre qui changerait à chaque minute ferait battre les palettes sans fin.
fn delay_text(delay_s: f64, on_time: &str) -> String {
    if delay_s >= 60.0 {
        let minutes = ((delay_s / 300.0).floor() as u32).max(1) * 5;
        format!("RETARD {minutes} MIN")
    } else {
        on_time.to_string()
    }
}

/// Lignes à afficher pour une gare : départs puis arrivées, triés par heure.
pub struct Listing {
    pub departures: Vec<String>,
    pub arrivals: Vec<String>,
}

pub fn listing(app: &App, station: &Station) -> Listing {
    let world = &app.world;
    let m = world.modifiers(&app.data);
    let name = |id: &str| {
        app.data
            .station(id)
            .map_or(id.to_string(), |s| s.name.clone())
    };
    let mut departures: Vec<(f64, String)> = Vec::new();
    let mut arrivals: Vec<(f64, String)> = Vec::new();

    for train in &world.trains {
        let (Some(line), Some(loco)) =
            (app.data.line(&train.line), app.data.locomotive(&train.loco))
        else {
            continue;
        };
        let capacity = m.capacity_of(loco);
        let load = format!("{}/{}", train.count(), capacity);
        let track = train.id.to_string();
        match train.state {
            TrainState::AtStation { .. } if train.dir.origin(line) == station.id => {
                let status = if world.clock - train.sched_departure >= 60.0 {
                    delay_text(world.clock - train.sched_departure, "")
                } else if train.count() >= capacity {
                    "COMPLET".to_string()
                } else {
                    "EMBARQUEMENT".to_string()
                };
                let cells = [
                    &clock_text(train.sched_departure),
                    &name(train.dir.destination(line)),
                    &app.train_name(train),
                    &track,
                    &load,
                    &status,
                ];
                departures.push((train.sched_departure, row(cells.map(|s| s.as_str()))));
            }
            TrainState::Moving if train.dir.destination(line) == station.id => {
                let Some(trip) = train.trip else { continue };
                let duration = trip.profile.duration_s();
                let delay_s = train.late_s + (duration - trip.nominal_s).max(0.0);
                let eta = trip.started + duration;
                let cells = [
                    &clock_text(eta),
                    &name(train.dir.origin(line)),
                    &app.train_name(train),
                    &track,
                    &load,
                    &delay_text(delay_s, "A L'HEURE"),
                ];
                arrivals.push((eta, row(cells.map(|s| s.as_str()))));
            }
            _ => {}
        }
    }
    let sorted = |mut rows: Vec<(f64, String)>| {
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        rows.into_iter().map(|(_, text)| text).collect()
    };
    Listing {
        departures: sorted(departures),
        arrivals: sorted(arrivals),
    }
}

/// État des palettes : ce qui est affiché en ce moment, ligne par ligne.
#[derive(Debug, Default)]
pub struct Board {
    shown: Vec<Vec<char>>,
    acc: f64,
}

impl Board {
    /// Fait battre les palettes vers les lignes voulues pendant `dt` secondes.
    pub fn update(&mut self, targets: &[String], dt: f64) {
        self.shown.resize(targets.len(), Vec::new());
        self.acc += dt;
        let steps = (self.acc / FLAP_PERIOD) as usize;
        self.acc -= steps as f64 * FLAP_PERIOD;
        for _ in 0..steps.min(ALPHABET.len()) {
            self.step(targets);
        }
    }

    fn step(&mut self, targets: &[String]) {
        for (shown, target) in self.shown.iter_mut().zip(targets) {
            shown.resize(WIDTH, ' ');
            let mut wanted = target.chars();
            for cell in shown.iter_mut() {
                *cell = flip(*cell, wanted.next().unwrap_or(' '));
            }
        }
    }

    pub fn line(&self, index: usize) -> String {
        self.shown
            .get(index)
            .map(|row| row.iter().collect())
            .unwrap_or_default()
    }
}

/// Nombre de lignes par section pour une hauteur d'écran donnée.
pub fn rows_per_section(height: u16) -> usize {
    (height.saturating_sub(9) as usize / 2).clamp(1, 6)
}

/// Lignes cibles du tableau : `rows` départs puis `rows` arrivées.
pub fn targets(app: &App, rows: usize) -> Vec<String> {
    let Some(station) = app.board_station() else {
        return Vec::new();
    };
    let listing = listing(app, station);
    let blank = " ".repeat(WIDTH);
    let section = |lines: Vec<String>| {
        lines
            .into_iter()
            .chain(std::iter::repeat(blank.clone()))
            .take(rows)
            .collect::<Vec<_>>()
    };
    let mut out = section(listing.departures);
    out.extend(section(listing.arrivals));
    out
}

pub fn draw(app: &App, buf: &mut Buffer, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let t = &app.theme;
    for y in area.y..area.bottom() {
        fill_row(buf, area, y, t.board);
    }
    let rows = rows_per_section(area.height);
    let x0 = area.x as i32 + (area.width as i32 - WIDTH as i32).max(0) / 2;
    let station = app
        .board_station()
        .map_or(String::new(), |s| board_text(&s.name));
    let (hour, minute) = app.world.time_of_day();

    // Bandeau : gare à gauche, horloge à droite.
    fill_row(buf, area, area.y, t.hud);
    put_str(
        buf,
        area,
        area.x as i32,
        area.y as i32,
        &format!(" GARE DE {station} "),
        t.hud_title,
    );
    let clock = format!(" JOUR {}  {hour:02}:{minute:02} ", app.world.day());
    put_str(
        buf,
        area,
        area.right() as i32 - clock.chars().count() as i32,
        area.y as i32,
        &clock,
        t.hud,
    );

    let header = |place: &str| row(["HEURE", place, "TRAIN", "VOIE", "CHARGE", "ETAT"]);
    let mut y = area.y as i32 + 2;
    for (section, (title, place)) in [("DEPARTS", "DESTINATION"), ("ARRIVEES", "PROVENANCE")]
        .into_iter()
        .enumerate()
    {
        put_str(buf, area, x0, y, title, t.board_title);
        put_str(buf, area, x0, y + 1, &header(place), t.board_header);
        for i in 0..rows {
            let line = app.board.line(section * rows + i);
            let late = line.contains("RETARD");
            let (left, status): (String, String) = (
                line.chars().take(STATUS_AT).collect(),
                line.chars().skip(STATUS_AT).collect(),
            );
            put_str(buf, area, x0, y + 2 + i as i32, &left, t.flap);
            put_str(
                buf,
                area,
                x0 + STATUS_AT as i32,
                y + 2 + i as i32,
                &status,
                if late { t.flap_late } else { t.flap },
            );
        }
        y += rows as i32 + 3;
    }

    if area.height >= 2 {
        let y = area.bottom() - 1;
        fill_row(buf, area, y, t.hud);
        let mut x = area.x as i32;
        let mut keys = vec![
            ("tab", "gare suivante"),
            ("b", "carte"),
            ("p", "pause"),
            ("q", "quitter"),
        ];
        if app.read_only {
            keys.push(("", "lecture seule"));
        }
        for (key, label) in keys {
            put_str(buf, area, x, y as i32, &format!(" {key}"), t.hud_key);
            x += key.chars().count() as i32 + 1;
            put_str(buf, area, x, y as i32, &format!(" {label} "), t.hud);
            x += label.chars().count() as i32 + 2;
        }
        let money = format!(" {} ", format::money(app.world.money));
        put_str(
            buf,
            area,
            area.right() as i32 - money.chars().count() as i32,
            y as i32,
            &money,
            t.hud.patch(t.money),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flaps_cycle_through_the_alphabet_until_they_match() {
        assert_eq!(flip('A', 'A'), 'A');
        assert_eq!(flip(' ', 'C'), 'A');
        assert_eq!(flip('.', 'A'), ' ', "l'alphabet boucle");
        assert_eq!(flip('A', '~'), '~', "caractère inconnu : pose directe");
        assert_eq!(flip('~', 'A'), 'A');

        let mut c = ' ';
        let mut steps = 0;
        while c != '9' {
            c = flip(c, '9');
            steps += 1;
        }
        assert_eq!(steps, 36);
    }

    #[test]
    fn board_settles_on_its_targets() {
        let mut board = Board::default();
        let targets = vec![row([
            "08:20",
            "Darlington",
            "Locomotion n°1 #1",
            "1",
            "12/40",
            "Embarquement",
        ])];
        assert_eq!(targets[0].chars().count(), WIDTH);
        assert!(targets[0].starts_with("08:20  DARLINGTON      LOCOMOTION NO1 #1"));

        board.update(&targets, FLAP_PERIOD * 1.5);
        assert_ne!(board.line(0), targets[0], "une seule palette ne suffit pas");
        for _ in 0..ALPHABET.len() {
            board.update(&targets, FLAP_PERIOD * 1.01);
        }
        assert_eq!(board.line(0), targets[0]);

        // Une ligne qui disparaît se vide palette par palette.
        let blank = vec![" ".repeat(WIDTH)];
        for _ in 0..ALPHABET.len() {
            board.update(&blank, FLAP_PERIOD * 1.01);
        }
        assert_eq!(board.line(0), blank[0]);
    }

    #[test]
    fn delays_are_announced_in_five_minute_steps() {
        assert_eq!(delay_text(30.0, "A L'HEURE"), "A L'HEURE");
        assert_eq!(delay_text(90.0, ""), "RETARD 5 MIN");
        assert_eq!(delay_text(9.9 * 60.0, ""), "RETARD 5 MIN");
        assert_eq!(delay_text(27.0 * 60.0, ""), "RETARD 25 MIN");
    }

    #[test]
    fn long_names_are_cut_to_their_column() {
        let text = row(["08:20", "Une gare au nom interminable", "", "", "", ""]);
        assert_eq!(text.chars().count(), WIDTH);
        assert!(text.starts_with("08:20  UNE GARE AU NO  "));
    }
}
