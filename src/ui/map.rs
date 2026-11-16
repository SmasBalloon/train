//! Écran carte : réseau schématique plein écran, trains animés, bandeaux.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::data::DecorKind;
use crate::sim::{Modifiers, TrainState, condition_factor};

use super::app::{App, LOG_TTL, LogKind};
use super::canvas::{MapCanvas, Stroke};
use super::format;

const PRIO_GRID: u8 = 1;
const PRIO_DECOR: u8 = 2;
const PRIO_TRACK: u8 = 3;

/// Échelle (sous-pixels par unité) à partir de laquelle on dessine rails et traverses.
const DETAIL_SCALE: f64 = 10.0;

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
    let spacing = [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0]
        .into_iter()
        .find(|s| s * cam.scale >= 24.0)
        .unwrap_or(500.0);
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

fn draw_decor(app: &App, canvas: &mut MapCanvas) {
    let size = canvas.px_size();
    for decor in app.data.countries.iter().flat_map(|c| &c.decor) {
        let (style, ch) = match decor.kind {
            DecorKind::River => (app.theme.river, Stroke::Fixed('~')),
        };
        for pair in decor.path.windows(2) {
            let (a, b) = (
                app.camera.to_px(pair[0], size),
                app.camera.to_px(pair[1], size),
            );
            canvas.line(a, b, style, PRIO_DECOR, ch);
            if !app.glyphs.ascii && app.camera.scale >= DETAIL_SCALE {
                canvas.line((a.0, a.1 + 1.0), (b.0, b.1 + 1.0), style, PRIO_DECOR, ch);
            }
        }
    }
}

fn draw_decor_labels(app: &App, buf: &mut Buffer, area: Rect) {
    if app.camera.scale < 4.0 {
        return;
    }
    let size = app.px_size();
    for decor in app.data.countries.iter().flat_map(|c| &c.decor) {
        let (Some(name), Some(first)) = (&decor.name, decor.path.first()) else {
            continue;
        };
        // Étiquette près du deuxième point, pour rester à l'écart du bord du tracé.
        let anchor = decor.path.get(1).unwrap_or(first);
        let (cx, cy) = cell_of(app.camera.to_px(*anchor, size));
        put_centered(buf, area, cx, cy + 1, name, app.theme.river);
    }
}

fn draw_tracks(app: &App, canvas: &mut MapCanvas) {
    let size = canvas.px_size();
    let detailed = !app.glyphs.ascii && app.camera.scale >= DETAIL_SCALE;
    for line in app.data.lines() {
        let Some(path) = app.path(&line.id) else {
            continue;
        };
        let condition = app.world.lines.get(&line.id).map_or(1.0, |l| l.condition);
        let style = if condition < app.data.balance.wear_slow_threshold {
            app.theme.track_worn
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
                    k += 4.0 * ((len / 4000.0).ceil().max(1.0));
                }
            }
        }
    }
}

fn draw_stations(app: &App, buf: &mut Buffer, area: Rect) {
    let size = app.px_size();
    let modifiers = Modifiers::compute(&app.world.research_done, &app.data);
    for station in app.data.stations() {
        let (cx, cy) = cell_of(app.camera.to_px(station.pos, size));
        if cy < -6 || cy > area.height as i32 + 1 || cx < -40 || cx > area.width as i32 + 40 {
            continue;
        }
        let (x, y) = (area.x as i32 + cx, area.y as i32 + cy);
        if app.glyphs.ascii {
            put_str(buf, area, x - 1, y, "(O)", app.theme.station);
        } else {
            put_str(
                buf,
                area,
                x,
                y,
                &app.glyphs.station.to_string(),
                app.theme.station,
            );
        }
        put_centered(
            buf,
            area,
            x,
            y + 1,
            &station.name.to_uppercase(),
            app.theme.station,
        );
        if app.camera.scale < 2.5 {
            continue;
        }
        let waiting = app
            .world
            .stations
            .get(&station.id)
            .map_or(0.0, |s| s.waiting);
        put_centered(
            buf,
            area,
            x,
            y + 2,
            &format!("{} en attente", waiting.floor() as u32),
            app.theme.dim,
        );

        // Trains à quai dans cette gare.
        let mut row = y + 3;
        for (index, train) in app.world.trains.iter().enumerate() {
            let (Some(line), Some(loco)) =
                (app.data.line(&train.line), app.data.locomotive(&train.loco))
            else {
                continue;
            };
            if !train.is_docked() || train.dir.origin(line) != station.id {
                continue;
            }
            let selected = app.selected == Some(train.id);
            let text = format!(
                "{} {} {}/{} ",
                if selected { app.glyphs.cursor } else { " " },
                app.train_name(train),
                train.passengers,
                modifiers.capacity_of(loco)
            );
            let style = if selected {
                app.theme.train(index).patch(app.theme.selected)
            } else {
                app.theme.train(index)
            };
            put_centered(buf, area, x, row, &text, style);
            row += 1;
        }
    }
}

fn draw_trains(app: &App, buf: &mut Buffer, area: Rect) {
    let size = app.px_size();
    let cam = &app.camera;
    let modifiers = Modifiers::compute(&app.world.research_done, &app.data);
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
        let wagons = if cam.scale < 3.0 {
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

        if cam.scale < 3.0 {
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
            train.passengers,
            modifiers.capacity_of(loco)
        );
        put_str(buf, area, x + 2, y - 1, &label, app.theme.train(index));
    }
}

fn fill_row(buf: &mut Buffer, area: Rect, y: u16, style: Style) {
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
    let line_id = &b.start_line;
    let max_trains = app.data.line(line_id).map_or(0, |l| l.max_trains);
    let condition = app.world.lines.get(line_id).map_or(1.0, |l| l.condition);
    let track_style = if condition < b.wear_slow_threshold {
        t.warn
    } else {
        t.hud
    };

    let mut spans = vec![
        (" RAINHILL ".to_string(), t.hud_title),
        (
            format!(" Jour {} {hour:02}:{minute:02}", app.world.day()),
            t.hud,
        ),
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
    ];
    if let Some(active) = &app.world.research_active
        && let Some(research) = app.data.research(&active.id)
    {
        let progress = active.progress_s / (research.duration_h * 3600.0);
        spans.push(sep.clone());
        spans.push((
            format!(
                "Recherche : {} {}",
                research.name.fr,
                format::percent(progress)
            ),
            t.hud,
        ));
    }
    put_spans(buf, area, area.x as i32, area.y as i32, &spans);

    if app.paused {
        let text = " PAUSE ";
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
        ("tab", "train"),
        ("a", "acheter"),
        ("r", "recherche"),
        ("m", "réparer"),
        ("?", "aide"),
        ("q", "quitter"),
        ("f", "suivre"),
        ("+/-", "zoom"),
        ("flèches", "carte"),
        ("0", "recadrer"),
    ];
    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push((format!(" {key}"), t.hud_key));
        spans.push((format!(" {label} "), t.hud));
    }
    put_spans(buf, area, area.x as i32, y as i32, &spans);

    if area.height > 3 {
        let zoom = format!(" zoom ×{} ", format::dec1(app.camera.scale / app.fit_scale));
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
    const WIDTH: u16 = 38;
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
    let modifiers = Modifiers::compute(&app.world.research_done, &app.data);
    let capacity = modifiers.capacity_of(loco);
    let name = |id: &str| {
        app.data
            .station(id)
            .map_or(id.to_string(), |s| s.name.clone())
    };
    let origin = name(train.dir.origin(line));
    let destination = name(train.dir.destination(line));

    let (status, detail) = match train.state {
        TrainState::AtStation { waited } => (
            format!("À quai à {origin} {} {destination}", g.arrow),
            format!("Embarquement depuis {} min", (waited / 60.0) as u32),
        ),
        TrainState::Moving => {
            let condition = app.world.lines.get(&line.id).map_or(1.0, |l| l.condition);
            let speed = loco.top_speed_kmh
                * modifiers.speed
                * condition_factor(condition, &app.data.balance);
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
    let load = train.passengers as f64 / capacity as f64;
    let lines = vec![
        Line::styled(status, t.text),
        Line::styled(detail, t.dim),
        Line::from(vec![
            Span::styled("Voyageurs ", t.dim),
            Span::styled(
                format::gauge(load, 12, g.gauge_full, g.gauge_empty),
                t.train(index),
            ),
            Span::styled(format!(" {}/{capacity}", train.passengers), t.text),
        ]),
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
