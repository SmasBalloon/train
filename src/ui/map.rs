//! Écran carte : réseau schématique plein écran, trains animés, bandeaux.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::data::DecorKind;
use crate::sim::TrainState;

use super::app::{App, LOG_TTL, LogKind};
use super::canvas::{MapCanvas, Stroke};
use super::format;

const PRIO_GRID: u8 = 1;
const PRIO_BORDER: u8 = 2;
const PRIO_DECOR: u8 = 3;
const PRIO_PROJECT: u8 = 4;
const PRIO_TRACK: u8 = 5;

// Niveaux de détail, selon l'échelle (sous-pixels par unité de carte). Le seuil
// entre la vue continent et la vue pays dépend de la taille du terminal : voir
// `App::country_scale`.
/// Vue gare au-dessus : files d'attente par clientèle, trains à quai, convois.
pub const SCALE_STATION: f64 = 12.0;
/// Rails et traverses dessinés un par un.
const SCALE_RAILS: f64 = 100.0;

/// Nom du niveau de zoom : `country` est l'échelle où commence la vue pays.
pub fn view_name(scale: f64, country: f64) -> &'static str {
    if scale >= SCALE_STATION {
        "gare"
    } else if scale >= country {
        "pays"
    } else {
        "continent"
    }
}

/// Remplace un caractère non ASCII par son équivalent le plus proche.
pub fn ascii_fold(ch: char) -> char {
    match ch {
        c if c.is_ascii() => c,
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'à' | 'â' => 'a',
        'ù' | 'û' | 'ü' => 'u',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ç' => 'c',
        'É' | 'È' | 'Ê' => 'E',
        'À' | 'Â' => 'A',
        'Ç' => 'C',
        '°' => 'o',
        '×' => 'x',
        '£' => 'L',
        '’' => '\'',
        '«' | '»' => '"',
        _ => '?',
    }
}

/// Repli ASCII strict : aucun caractère hors ASCII ne doit atteindre le terminal.
pub fn fold_to_ascii(buf: &mut Buffer, area: Rect) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buf.cell_mut((x, y))
                && !cell.symbol().is_ascii()
            {
                let folded: String = cell.symbol().chars().map(ascii_fold).collect();
                cell.set_symbol(&folded);
            }
        }
    }
}

/// Écrit du texte en coordonnées de cellule, rogné aux bords de `area`.
pub fn put_str(buf: &mut Buffer, area: Rect, x: i32, y: i32, text: &str, style: Style) {
    if y < area.y as i32 || y >= area.bottom() as i32 {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        let cx = x + i as i32;
        if cx < area.x as i32 {
            continue;
        }
        if cx >= area.right() as i32 {
            break;
        }
        if let Some(cell) = buf.cell_mut((cx as u16, y as u16)) {
            cell.reset();
            cell.set_char(ch).set_style(style);
        }
    }
}

fn put_centered(buf: &mut Buffer, area: Rect, cx: i32, y: i32, text: &str, style: Style) {
    put_str(
        buf,
        area,
        cx - text.chars().count() as i32 / 2,
        y,
        text,
        style,
    );
}

fn cell_of(px: (f64, f64)) -> (i32, i32) {
    ((px.0 / 2.0).floor() as i32, (px.1 / 4.0).floor() as i32)
}

pub fn draw(app: &App, buf: &mut Buffer, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut canvas = MapCanvas::new(area.width, area.height, app.glyphs.ascii);
    draw_grid(app, &mut canvas);
    draw_decor(app, &mut canvas);
    draw_tracks(app, &mut canvas);
    canvas.render(buf, area);

    draw_decor_labels(app, buf, area);
    draw_stations(app, buf, area);
    draw_trains(app, buf, area);

    draw_log(app, buf, area);
    draw_train_panel(app, buf, area);
    draw_top_bar(app, buf, area);
    draw_bottom_bar(app, buf, area);
}

fn draw_grid(app: &App, canvas: &mut MapCanvas) {
    let cam = &app.camera;
    let size = canvas.px_size();
    let spacing = [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]
        .into_iter()
        .find(|s| s * cam.scale >= 24.0)
        .unwrap_or(200.0);
    let top_left = cam.to_world((0.0, 0.0), size);
    let bottom_right = cam.to_world(size, size);
    let mut gy = (top_left[1] / spacing).floor() * spacing;
    while gy <= bottom_right[1] {
        let mut gx = (top_left[0] / spacing).floor() * spacing;
        while gx <= bottom_right[0] {
            let (x, y) = cam.to_px([gx, gy], size);
            canvas.point(x, y, app.theme.grid, PRIO_GRID);
            gx += spacing;
        }
        gy += spacing;
    }
}

/// Trace un segment en pointillés : `on` sous-pixels pleins, puis `off` vides.
fn dashed(
    canvas: &mut MapCanvas,
    a: (f64, f64),
    b: (f64, f64),
    style: Style,
    prio: u8,
    on: f64,
    off: f64,
) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    let size = canvas.px_size();
    if len < 1e-6 || len > 20.0 * (size.0 + size.1) {
        // Segment démesuré (zoom extrême) : un trait plein, que le canevas rogne.
        canvas.line(a, b, style, prio, Stroke::Fixed('.'));
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut k = 0.0;
    while k < len {
        let end = (k + on).min(len);
        canvas.line(
            (a.0 + ux * k, a.1 + uy * k),
            (a.0 + ux * end, a.1 + uy * end),
            style,
            prio,
            Stroke::Fixed('.'),
        );
        k += on + off;
    }
}

fn draw_decor(app: &App, canvas: &mut MapCanvas) {
    let size = canvas.px_size();
    let scale = app.camera.scale;
    for country in &app.data.countries {
        let owned = app.world.has_concession(&country.id);
        for decor in &country.decor {
            for pair in decor.path.windows(2) {
                let (a, b) = (
                    app.camera.to_px(pair[0], size),
                    app.camera.to_px(pair[1], size),
                );
                match decor.kind {
                    // Les rivières ne se voient que de près.
                    DecorKind::River if scale >= SCALE_STATION => {
                        canvas.line(a, b, app.theme.river, PRIO_DECOR, Stroke::Fixed('~'));
                    }
                    DecorKind::River => {}
                    DecorKind::Border => {
                        let style = if owned {
                            app.theme.border_owned
                        } else {
                            app.theme.grid
                        };
                        canvas.line(a, b, style, PRIO_BORDER, Stroke::Fixed('.'));
                    }
                }
            }
        }
    }
}

fn draw_decor_labels(app: &App, buf: &mut Buffer, area: Rect) {
    let size = app.px_size();
    let scale = app.camera.scale;
    for country in &app.data.countries {
        let owned = app.world.has_concession(&country.id);
        for decor in &country.decor {
            match decor.kind {
                DecorKind::River if scale >= SCALE_STATION * 2.0 => {
                    let (Some(name), Some(anchor)) =
                        (&decor.name, decor.path.get(1).or(decor.path.first()))
                    else {
                        continue;
                    };
                    let (cx, cy) = cell_of(app.camera.to_px(*anchor, size));
                    put_centered(
                        buf,
                        area,
                        area.x as i32 + cx,
                        area.y as i32 + cy + 1,
                        name,
                        app.theme.river,
                    );
                }
                // Nom du pays, en haut de son contour, tant qu'on n'est pas au niveau des gares.
                DecorKind::Border if scale < SCALE_STATION => {
                    let n = decor.path.len().max(1) as f64;
                    let cx = decor.path.iter().map(|p| p[0]).sum::<f64>() / n;
                    let top = decor.path.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
                    let (x, y) = cell_of(app.camera.to_px([cx, top], size));
                    let style = if owned {
                        app.theme.title
                    } else {
                        app.theme.dim
                    };
                    let name = country.name.fr.to_uppercase();
                    put_centered(
                        buf,
                        area,
                        area.x as i32 + x,
                        area.y as i32 + y - 1,
                        &name,
                        style,
                    );
                }
                _ => {}
            }
        }
    }
}

fn draw_tracks(app: &App, canvas: &mut MapCanvas) {
    let size = canvas.px_size();
    let detailed = !app.glyphs.ascii && app.camera.scale >= SCALE_RAILS;
    for line in app.data.lines() {
        let Some(path) = app.path(&line.id) else {
            continue;
        };
        let state = app.world.lines.get(&line.id);
        let built = state.is_some_and(|l| l.built);
        if !built {
            // Projet : en pointillés, là où la compagnie a une concession.
            let reachable = app
                .data
                .line_countries(line)
                .iter()
                .all(|c| app.world.has_concession(&c.id));
            if reachable && app.camera.scale >= app.country_scale() * 0.5 {
                for pair in path.points().windows(2) {
                    let (a, b) = (
                        app.camera.to_px(pair[0], size),
                        app.camera.to_px(pair[1], size),
                    );
                    dashed(canvas, a, b, app.theme.dim, PRIO_PROJECT, 1.0, 4.0);
                }
            }
            continue;
        }
        let condition = state.map_or(1.0, |l| l.condition);
        let style = if condition < app.data.balance.wear_slow_threshold {
            app.theme.track_worn
        } else if line.id == app.line_id() {
            app.theme.track_current
        } else {
            app.theme.track
        };
        for pair in path.points().windows(2) {
            let (a, b) = (
                app.camera.to_px(pair[0], size),
                app.camera.to_px(pair[1], size),
            );
            if !detailed {
                canvas.line(a, b, style, PRIO_TRACK, Stroke::Slope('='));
                continue;
            }
            // Deux rails écartés de 3 sous-pixels, et une traverse tous les 4.
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if len < 1e-6 {
                continue;
            }
            let (ux, uy) = (dx / len, dy / len);
            let (nx, ny) = (-uy * 1.5, ux * 1.5);
            canvas.line(
                (a.0 + nx, a.1 + ny),
                (b.0 + nx, b.1 + ny),
                style,
                PRIO_TRACK,
                Stroke::Slope('='),
            );
            canvas.line(
                (a.0 - nx, a.1 - ny),
                (b.0 - nx, b.1 - ny),
                style,
                PRIO_TRACK,
                Stroke::Slope('='),
            );
            // On ne trace que les traverses proches de l'écran.
            let visible = |p: (f64, f64)| {
                p.0 > -8.0 && p.1 > -8.0 && p.0 < size.0 + 8.0 && p.1 < size.1 + 8.0
            };
            let stride = 4.0 * (len / 4000.0).ceil().max(1.0);
            let mut k = 0.0;
            while k <= len {
                let p = (a.0 + ux * k, a.1 + uy * k);
                if visible(p) {
                    canvas.line(
                        (p.0 + nx, p.1 + ny),
                        (p.0 - nx, p.1 - ny),
                        style,
                        PRIO_TRACK,
                        Stroke::Slope('='),
                    );
                    k += 4.0;
                } else {
                    // Saute rapidement les portions hors champ.
                    k += stride;
                }
            }
        }
    }
}

/// Vrai si deux étiquettes (colonne de gauche, ligne, largeur, hauteur) se chevauchent.
fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 + 1 && b.0 < a.0 + a.2 + 1 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

fn draw_stations(app: &App, buf: &mut Buffer, area: Rect) {
    let country_scale = app.country_scale();
    let size = app.px_size();
    let scale = app.camera.scale;
    let t = &app.theme;
    let modifiers = app.world.modifiers(&app.data);

    // Les gares desservies et les grandes villes passent d'abord : en cas de
    // chevauchement, ce sont leurs étiquettes qui restent.
    let mut stations: Vec<_> = app
        .data
        .countries
        .iter()
        .flat_map(|c| {
            c.stations
                .iter()
                .enumerate()
                .map(move |(i, s)| (c, i == 0, s))
        })
        .map(|(country, capital, station)| {
            (
                app.world.is_served(&app.data, &station.id),
                country,
                capital,
                station,
            )
        })
        .collect();
    stations.sort_by_key(|(served, _, capital, station)| {
        (!served, !capital, std::cmp::Reverse(station.population))
    });

    let mut placed: Vec<(i32, i32, i32, i32)> = Vec::new();
    for (served, country, capital, station) in stations {
        let (cx, cy) = cell_of(app.camera.to_px(station.pos, size));
        if cy < -8 || cy > area.height as i32 + 1 || cx < -40 || cx > area.width as i32 + 40 {
            continue;
        }
        let owned = app.world.has_concession(&country.id);
        // Ce qui s'affiche dépend du niveau de zoom et de l'intérêt de la gare.
        let named = if served {
            true
        } else if owned {
            scale >= country_scale || capital
        } else {
            capital && scale >= country_scale
        };
        if !served && !named {
            continue;
        }
        let (x, y) = (area.x as i32 + cx, area.y as i32 + cy);
        let (marker, style) = if served {
            (app.glyphs.station, t.station)
        } else {
            (app.glyphs.station_idle, t.dim)
        };
        put_str(buf, area, x, y, &marker.to_string(), style);
        if !named {
            continue;
        }

        let mut lines: Vec<(String, Style)> = vec![(station.name.to_uppercase(), style)];
        if served && scale >= country_scale {
            let state = app.world.stations.get(&station.id);
            let waiting = state.map_or(0.0, |s| s.total());
            lines.push((format!("{} en attente", waiting.floor() as u32), t.dim));
            if scale >= SCALE_STATION {
                // Détail par catégorie de trafic : « N12 T3 A2 F8 ».
                let detail: Vec<String> = app
                    .data
                    .traffic
                    .iter()
                    .map(|c| {
                        let n = state
                            .and_then(|s| s.waiting.get(&c.id))
                            .map_or(0, |w| w.floor() as u32);
                        format!("{}{n}", c.symbol)
                    })
                    .collect();
                lines.push((detail.join(" "), t.dim));

                // Trains à quai dans cette gare (les premiers seulement dans un grand nœud).
                let docked: Vec<_> = app
                    .world
                    .trains
                    .iter()
                    .enumerate()
                    .filter(|(_, train)| {
                        train.is_docked()
                            && app
                                .data
                                .line(&train.line)
                                .is_some_and(|l| train.dir.origin(l) == station.id)
                    })
                    .collect();
                for (index, train) in docked.iter().take(6) {
                    let selected = app.selected == Some(train.id);
                    let capacity = app
                        .data
                        .locomotive(&train.loco)
                        .map_or(0, |l| modifiers.capacity_of(l));
                    let text = format!(
                        "{} {} {}/{capacity} ",
                        if selected { app.glyphs.cursor } else { " " },
                        app.train_name(train),
                        train.count(),
                    );
                    let style = if selected {
                        t.train(*index).patch(t.selected)
                    } else {
                        t.train(*index)
                    };
                    lines.push((text, style));
                }
                if docked.len() > 6 {
                    lines.push((format!("et {} autres à quai", docked.len() - 6), t.dim));
                }
            }
        }

        let width = lines
            .iter()
            .map(|(text, _)| text.chars().count() as i32)
            .max()
            .unwrap_or(0);
        let rect = (x - width / 2, y + 1, width, lines.len() as i32);
        if placed.iter().any(|other| overlaps(rect, *other)) {
            continue;
        }
        placed.push(rect);
        for (row, (text, style)) in lines.iter().enumerate() {
            put_centered(buf, area, x, y + 1 + row as i32, text, *style);
        }
    }
}

fn draw_trains(app: &App, buf: &mut Buffer, area: Rect) {
    let country_scale = app.country_scale();
    let size = app.px_size();
    let cam = &app.camera;
    let modifiers = app.world.modifiers(&app.data);
    for (index, train) in app.world.trains.iter().enumerate() {
        if train.is_docked() {
            continue;
        }
        let (Some(line), Some(loco), Some(path)) = (
            app.data.line(&train.line),
            app.data.locomotive(&train.loco),
            app.path(&train.line),
        ) else {
            continue;
        };
        let Some((pos, heading)) = app.train_position(train) else {
            continue;
        };
        let (hx, hy) = cell_of(cam.to_px(pos, size));
        if hx < -30 || hy < -3 || hx > area.width as i32 + 30 || hy > area.height as i32 + 3 {
            continue;
        }
        let (x, y) = (area.x as i32 + hx, area.y as i32 + hy);
        let selected = app.selected == Some(train.id);
        let style = if selected {
            app.theme.train(index).patch(app.theme.selected)
        } else {
            app.theme.train(index)
        };

        // Wagons : on remonte le tracé sous-pixel par sous-pixel et on garde
        // les cellules distinctes, pour un convoi continu quelle que soit la pente.
        let wagons = if cam.scale < SCALE_STATION {
            0
        } else {
            (loco.capacity / 12).clamp(1, 6) as usize
        };
        let along = train.pos_km / line.length_km * path.length();
        let backwards = match train.dir {
            crate::sim::Dir::Outbound => -1.0,
            crate::sim::Dir::Inbound => 1.0,
        };
        let mut cells = vec![(hx, hy)];
        let mut k = 1.0;
        while cells.len() <= wagons && k <= 8.0 * (wagons as f64 + 1.0) {
            let (p, _) = path.at(along + backwards * k / cam.scale);
            let cell = cell_of(cam.to_px(p, size));
            if !cells.contains(&cell) {
                cells.push(cell);
            }
            k += 1.0;
        }
        for &(wx, wy) in cells.iter().skip(1) {
            put_str(
                buf,
                area,
                area.x as i32 + wx,
                area.y as i32 + wy,
                &app.glyphs.wagon.to_string(),
                style,
            );
        }
        put_str(
            buf,
            area,
            x,
            y,
            &app.glyphs.head(heading[0], heading[1]).to_string(),
            style,
        );

        if cam.scale < SCALE_STATION {
            // De loin, seul le train sélectionné garde son nom.
            if selected && cam.scale >= country_scale {
                put_str(
                    buf,
                    area,
                    x + 2,
                    y - 1,
                    &app.train_name(train),
                    app.theme.train(index),
                );
            }
            continue;
        }
        // Panache de fumée qui traîne derrière la machine.
        let back = if heading[0] >= 0.0 { -1 } else { 1 };
        let phase = (app.time * 4.0 + train.id as f64) as usize;
        for (i, (dx, dy)) in [(0, -1), (back, -2)].into_iter().enumerate() {
            let puff = app.glyphs.smoke[(phase + i) % app.glyphs.smoke.len()];
            put_str(
                buf,
                area,
                x + dx,
                y + dy,
                &puff.to_string(),
                app.theme.smoke,
            );
        }
        let label = format!(
            "{} {}/{}",
            app.train_name(train),
            train.count(),
            modifiers.capacity_of(loco)
        );
        put_str(buf, area, x + 2, y - 1, &label, app.theme.train(index));
    }
}

pub fn fill_row(buf: &mut Buffer, area: Rect, y: u16, style: Style) {
    for x in area.x..area.right() {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.reset();
            cell.set_style(style);
        }
    }
}

/// Écrit une suite de segments stylés et renvoie la colonne suivante.
fn put_spans(buf: &mut Buffer, area: Rect, mut x: i32, y: i32, spans: &[(String, Style)]) -> i32 {
    for (text, style) in spans {
        put_str(buf, area, x, y, text, *style);
        x += text.chars().count() as i32;
    }
    x
}

fn draw_top_bar(app: &App, buf: &mut Buffer, area: Rect) {
    let t = &app.theme;
    let b = &app.data.balance;
    fill_row(buf, area, area.y, t.hud);
    let sep = (format!(" {} ", app.glyphs.sep), t.hud.patch(t.dim));
    let (hour, minute) = app.world.time_of_day();
    let income = app.world.income_per_game_second() * b.time_scale;
    let income_style = if income > -0.05 { t.good } else { t.warn };
    let line_id = app.line_id();
    let max_trains = app.world.line_capacity(&app.data, line_id);
    let era = app.world.era(&app.data);
    let condition = app.world.lines.get(line_id).map_or(1.0, |l| l.condition);
    let track_style = if condition < b.wear_slow_threshold {
        t.warn
    } else {
        t.hud
    };

    let satisfaction = app.world.overall_satisfaction(&app.data);
    let mood_style = if satisfaction >= 0.6 {
        t.good
    } else if satisfaction < 0.4 {
        t.warn
    } else {
        t.hud
    };
    // Le nom du jeu ne s'affiche que s'il reste de la place pour l'essentiel.
    let mut spans = Vec::new();
    if area.width >= 140 {
        spans.push((" RAINHILL ".to_string(), t.hud_title));
    }
    spans.extend([
        (
            format!(
                " {} {} {}",
                app.world.year(&app.data),
                app.glyphs.bullet,
                era.name.fr
            ),
            t.hud.patch(t.title),
        ),
        (format!("  {hour:02}:{minute:02}"), t.hud),
        sep.clone(),
        (format::money(app.world.money), t.hud.patch(t.money)),
        sep.clone(),
        (
            format!(
                "{}£ {}/s",
                if income > -0.05 { "+" } else { "-" },
                format::dec1(income.abs())
            ),
            t.hud.patch(income_style),
        ),
        sep.clone(),
        (
            format!("Trains {}/{}", app.world.trains_on(line_id), max_trains),
            t.hud,
        ),
        sep.clone(),
        (
            format!("Voie {}", format::percent(condition)),
            t.hud.patch(track_style),
        ),
        sep.clone(),
        (
            format!("Clients {}", format::percent(satisfaction)),
            t.hud.patch(mood_style),
        ),
    ]);
    // Stocks de combustible, quand l'époque oblige à les gérer.
    if era.rules.fuel_market {
        for fuel in app.data.fuels.iter().filter(|f| f.stock) {
            let burn = app.world.fuel_burn_per_hour(&app.data, &fuel.id);
            if burn <= 0.0 {
                continue;
            }
            let stock = app.world.fuel_stock(&fuel.id);
            let style = if stock < burn * 2.0 { t.warn } else { t.hud };
            spans.push(sep.clone());
            spans.push((
                format!("{} {} {}", fuel.name.fr, format::quantity(stock), fuel.unit),
                t.hud.patch(style),
            ));
        }
    }
    put_spans(buf, area, area.x as i32, area.y as i32, &spans);

    // Recherche en cours : calée à droite, pour rester visible quelle que soit la largeur.
    let mut right = area.right() as i32;
    if app.paused || app.read_only {
        right -= if app.read_only { 15 } else { 7 };
    }
    if let Some((research, progress, left_s)) = app.research_progress() {
        let text = format!(
            " {} {} {} {} ",
            research.name.fr,
            format::gauge(progress, 8, app.glyphs.gauge_full, app.glyphs.gauge_empty),
            format::percent(progress),
            format::duration(left_s)
        );
        put_str(
            buf,
            area,
            right - text.chars().count() as i32,
            area.y as i32,
            &text,
            t.hud.patch(t.title),
        );
    }

    if app.paused || app.read_only {
        let text = if app.read_only {
            " LECTURE SEULE "
        } else {
            " PAUSE "
        };
        put_str(
            buf,
            area,
            area.right() as i32 - text.chars().count() as i32,
            area.y as i32,
            text,
            t.hud_title,
        );
    }

    // Aide au tout premier départ.
    if app.world.lifetime_income == 0.0
        && app.world.trains.iter().all(|tr| tr.is_docked())
        && area.height > 10
    {
        let hint = " Les voyageurs montent à bord. Appuie sur d pour donner le départ ! ";
        put_centered(
            buf,
            area,
            area.x as i32 + area.width as i32 / 2,
            area.y as i32 + 7,
            hint,
            t.title,
        );
    }
}

fn draw_bottom_bar(app: &App, buf: &mut Buffer, area: Rect) {
    if area.height < 2 {
        return;
    }
    let t = &app.theme;
    let y = area.bottom() - 1;
    fill_row(buf, area, y, t.hud);
    let keys = [
        ("d", "départ"),
        ("espace", "tous"),
        ("n", "réseau"),
        ("a", "acheter"),
        ("r", "recherche"),
        ("e", "époque"),
        ("u", "voie"),
        ("c", "combustible"),
        ("t", "tarifs"),
        ("s", "équipages"),
        ("g", "services"),
        ("o", "compagnie"),
        ("v", "flotte"),
        ("b", "tableau"),
        ("m", "réparer"),
        ("?", "aide"),
        ("q", "quitter"),
    ];
    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push((format!(" {key}"), t.hud_key));
        spans.push((format!(" {label} "), t.hud));
    }
    put_spans(buf, area, area.x as i32, y as i32, &spans);

    if area.height > 3 {
        let line = app.data.line(app.line_id()).map_or("", |l| l.name.as_str());
        let zoom = format!(
            " {line} {} vue {} ",
            app.glyphs.bullet,
            view_name(app.camera.scale, app.country_scale())
        );
        let follow = if app.follow { "suivi " } else { "" };
        let text = format!("{follow}{zoom}");
        put_str(
            buf,
            area,
            area.right() as i32 - text.chars().count() as i32,
            y as i32 - 1,
            &text,
            t.dim,
        );
    }
}

fn draw_log(app: &App, buf: &mut Buffer, area: Rect) {
    if area.height < 8 {
        return;
    }
    let recent = app
        .log
        .iter()
        .rev()
        .filter(|e| app.time - e.at < LOG_TTL)
        .take(4);
    for (i, entry) in recent.enumerate() {
        let style = match entry.kind {
            LogKind::Info => app.theme.text,
            LogKind::Good => app.theme.good,
            LogKind::Warn => app.theme.warn,
        };
        let y = area.bottom() as i32 - 2 - i as i32;
        put_str(
            buf,
            area,
            area.x as i32,
            y,
            &format!(" {} ", entry.text),
            style,
        );
    }
}

fn draw_train_panel(app: &App, buf: &mut Buffer, area: Rect) {
    const WIDTH: u16 = 52;
    if area.width < WIDTH + 30 || area.height < 14 {
        return;
    }
    let Some((index, train)) = app
        .world
        .trains
        .iter()
        .enumerate()
        .find(|(_, t)| app.selected == Some(t.id))
    else {
        return;
    };
    let (Some(line), Some(loco)) = (app.data.line(&train.line), app.data.locomotive(&train.loco))
    else {
        return;
    };
    let t = &app.theme;
    let g = &app.glyphs;
    let modifiers = app.world.modifiers(&app.data);
    let capacity = modifiers.capacity_of(loco);
    let name = |id: &str| {
        app.data
            .station(id)
            .map_or(id.to_string(), |s| s.name.clone())
    };
    let origin = name(train.dir.origin(line));
    let destination = name(train.dir.destination(line));

    let clock = app.world.clock;
    let (status, detail) = match train.state {
        TrainState::AtStation { .. } => {
            let (h, m) = crate::sim::hour_minute(train.sched_departure);
            let late_min = ((clock - train.sched_departure) / 60.0) as i64;
            (
                format!("À quai à {origin} {} {destination}", g.arrow),
                if late_min >= 1 {
                    format!(
                        "Départ prévu {h:02}:{m:02}  {} retard {late_min} min",
                        g.bullet
                    )
                } else {
                    format!("Départ prévu {h:02}:{m:02}")
                },
            )
        }
        TrainState::Moving => {
            let speed = train.speed_kmh(clock);
            let done = match train.dir {
                crate::sim::Dir::Outbound => train.pos_km,
                crate::sim::Dir::Inbound => line.length_km - train.pos_km,
            };
            (
                format!("En route {} {destination}", g.arrow),
                format!(
                    "{} km/h  {} {}/{} km",
                    speed.round(),
                    g.bullet,
                    format::dec1(done),
                    format::dec1(line.length_km)
                ),
            )
        }
    };
    let load = train.count() as f64 / capacity as f64;
    let late = train.is_docked() && clock - train.sched_departure >= 60.0;
    let by_class: Vec<String> = app
        .data
        .traffic
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                c.name.fr,
                train.load.get(&c.id).copied().unwrap_or(0)
            )
        })
        .collect();
    let lines = vec![
        Line::styled(status, t.text),
        Line::styled(detail, if late { t.warn } else { t.dim }),
        Line::from(vec![
            Span::styled("Charge ", t.dim),
            Span::styled(
                format::gauge(load, 12, g.gauge_full, g.gauge_empty),
                t.train(index),
            ),
            Span::styled(format!(" {}/{capacity}", train.count()), t.text),
        ]),
        Line::styled(by_class.join(format!(" {} ", g.bullet).as_str()), t.dim),
        Line::styled(
            format!(
                "Équipage : {} {} {}",
                match app.world.crew_on(train.id, crate::sim::CrewRole::Driver) {
                    Some(c) if app.world.in_training(c.id) => format!("{} (en formation)", c.name),
                    Some(c) => c.name.clone(),
                    None => "SANS CONDUCTEUR".to_string(),
                },
                g.bullet,
                app.world
                    .crew_on(train.id, crate::sim::CrewRole::Conductor)
                    .map_or("contrôleur ordinaire", |c| c.name.as_str()),
            ),
            t.dim,
        ),
    ];
    let rect = Rect::new(
        area.right() - WIDTH,
        area.y + 1,
        WIDTH,
        lines.len() as u16 + 2,
    );
    let block = Block::bordered()
        .border_set(g.border)
        .border_style(t.border)
        .title(Span::styled(
            format!(" {} ", app.train_name(train)),
            t.train(index),
        ));
    Clear.render(rect, buf);
    Paragraph::new(lines).block(block).render(rect, buf);
}
