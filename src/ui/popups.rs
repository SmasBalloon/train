//! Fenêtres superposées à la carte : dépôt, recherche, époques, améliorations,
//! combustibles, flotte, événements, tarifs, aide.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph, Widget, Wrap};

use crate::data::{BonusKind, Branch, Effect, EventEffect, Research, StaffRole, UpgradeEffect};
use crate::sim::{
    ActionError, CrewRole, LineProps, TrainOrder, TrainState, allowed_speed, demand_per_hour,
};

use super::app::{App, NetworkRow, Popup};
use super::format;

pub fn draw(app: &App, buf: &mut Buffer, area: Rect) {
    match app.popup {
        Popup::None => {}
        Popup::Shop { sel } => draw_shop(app, buf, area, sel),
        Popup::Research { sel, branch } => draw_research(app, buf, area, sel, branch),
        Popup::Tree { sel } => super::tree::draw(app, buf, area, sel),
        Popup::Manage { sel } => draw_manage(app, buf, area, sel),
        Popup::Eras { sel } => draw_eras(app, buf, area, sel),
        Popup::Upgrades { sel } => draw_upgrades(app, buf, area, sel),
        Popup::Fuel { sel } => draw_fuel(app, buf, area, sel),
        Popup::Network { sel } => draw_network(app, buf, area, sel),
        Popup::Staff { sel } => draw_staff(app, buf, area, sel),
        Popup::Company { sel, .. } => draw_company(app, buf, area, sel),
        Popup::Crew { sel, recruiting } => draw_crew(app, buf, area, sel, recruiting),
        Popup::Fleet { sel } => draw_fleet(app, buf, area, sel),
        Popup::Replace { train, sel, .. } => draw_replace(app, buf, area, train, sel),
        Popup::Training { sel } => draw_training(app, buf, area, sel),
        Popup::TrainingNew { sel } => draw_training_new(app, buf, area, sel),
        Popup::Enroll { session, sel } => draw_enroll(app, buf, area, session, sel),
        Popup::Event { sel } => draw_event(app, buf, area, sel),
        Popup::Outcome { event, choice } => draw_outcome(app, buf, area, event, choice),
        Popup::Welcome => draw_welcome(app, buf, area),
        Popup::Help => draw_help(app, buf, area),
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width
        .min(area.width.saturating_sub(2))
        .max(1)
        .min(area.width);
    let h = height
        .min(area.height.saturating_sub(2))
        .max(1)
        .min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Dessine un cadre centré et renvoie sa zone intérieure.
pub fn window(
    app: &App,
    buf: &mut Buffer,
    area: Rect,
    title: &str,
    width: u16,
    height: u16,
) -> Rect {
    frame_at(app, buf, centered(area, width, height), title)
}

/// Dessine un cadre à l'emplacement donné et renvoie sa zone intérieure.
pub fn frame_at(app: &App, buf: &mut Buffer, rect: Rect, title: &str) -> Rect {
    let block = Block::bordered()
        .border_set(app.glyphs.border)
        .border_style(app.theme.border)
        .padding(Padding::horizontal(1))
        .title(Span::styled(format!(" {title} "), app.theme.title));
    let inner = block.inner(rect);
    Clear.render(rect, buf);
    block.render(rect, buf);
    inner
}

/// Cadre à trois zones : liste (de `rows` lignes au plus), détail, pied.
fn panel(
    app: &App,
    buf: &mut Buffer,
    area: Rect,
    title: &str,
    rows: usize,
    size: (u16, u16),
) -> [Rect; 3] {
    let inner = window(app, buf, area, title, size.0, size.1);
    // La liste ne prend jamais plus de la moitié de la fenêtre.
    let list = (rows as u16 + 1).min(inner.height / 2).max(1);
    Layout::vertical([
        Constraint::Length(list),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner)
}

/// Premier rang affiché d'une liste de `len` lignes dans `height` lignes,
/// de façon à garder la sélection visible et à peu près centrée.
fn scroll_start(sel: usize, len: usize, height: usize) -> usize {
    if len <= height {
        0
    } else {
        sel.saturating_sub(height / 2).min(len - height)
    }
}

/// Affiche la tranche visible d'une liste dont `sel` est la ligne choisie.
fn render_list(buf: &mut Buffer, area: Rect, rows: Vec<Line>, sel: usize) {
    let height = area.height.saturating_sub(1) as usize;
    let start = scroll_start(sel, rows.len(), height.max(1));
    let visible: Vec<Line> = rows.into_iter().skip(start).take(height.max(1)).collect();
    Paragraph::new(visible).render(area, buf);
}

pub fn footer(app: &App, buf: &mut Buffer, area: Rect, hint: &str) {
    let line = match &app.popup_msg {
        Some(msg) => Line::styled(msg.clone(), app.theme.warn),
        None => Line::styled(hint.to_string(), app.theme.dim),
    };
    Paragraph::new(line).render(area, buf);
}

fn row_style(app: &App, selected: bool) -> Style {
    if selected {
        app.theme.text.patch(app.theme.selected)
    } else {
        app.theme.text
    }
}

fn cursor(app: &App, selected: bool) -> &'static str {
    if selected { app.glyphs.cursor } else { " " }
}

/// Raison courte pour laquelle une action est impossible, pour une colonne d'état.
fn short_reason(error: &ActionError) -> &'static str {
    match error {
        ActionError::Locked(_) => "recherche requise",
        ActionError::WrongEra(_) => "époque ultérieure",
        ActionError::NeedsUpgrade(_) => "voie à équiper",
        ActionError::LineFull => "ligne complète",
        ActionError::SameLocomotive => "machine actuelle",
        ActionError::AlreadyResearched => "terminée",
        ActionError::AlreadyInstalled => "installée",
        ActionError::NoConcession(_) => "concession requise",
        ActionError::NotConnected => "non reliée au réseau",
        ActionError::AlreadyOwned => "acquise",
        _ => "indisponible",
    }
}

fn traction_name(traction: &str) -> &str {
    match traction {
        "steam" => "vapeur",
        "diesel" => "diesel",
        "electric" => "électrique",
        "maglev" => "sustentation magnétique",
        "vactrain" => "tube à basse pression",
        other => other,
    }
}

pub fn branch_name(branch: Branch) -> &'static str {
    match branch {
        Branch::Traction => "Traction",
        Branch::RunningGear => "Roulement",
        Branch::Braking => "Freinage",
        Branch::Signalling => "Signalisation",
        Branch::Comfort => "Confort",
    }
}

fn percent_change(label: &str, value: f64) -> String {
    let p = ((value - 1.0) * 100.0).round() as i64;
    format!("{label} {}{p} %", if p >= 0 { "+" } else { "" })
}

/// Résumé lisible d'un effet de recherche : `Tarifs +20 %`.
pub fn describe_effect(effect: &Effect) -> String {
    match effect {
        Effect::SpeedMult { value } => percent_change("Vitesse", *value),
        Effect::CapacityMult { value } => percent_change("Capacité", *value),
        Effect::FareMult { value } => percent_change("Tarifs", *value),
        Effect::DemandMult { value } => percent_change("Demande", *value),
        Effect::TrackWearMult { value } => percent_change("Usure des voies", *value),
        Effect::FuelMult { value } => percent_change("Consommation", *value),
        Effect::WeightMult { value } => percent_change("Masse des trains", *value),
        Effect::Comfort { value } => {
            let p = (value * 100.0).round() as i64;
            format!("Confort {}{p} %", if p >= 0 { "+" } else { "" })
        }
        Effect::BrakeAdd { value } => format!("Freinage +{} m/s²", format::dec2(*value)),
        Effect::SignalSpeed { value } => {
            format!("Signalisation : {} km/h autorisés", value.round())
        }
        Effect::LineCapacity { value } => {
            format!(
                "+{value} train{} par ligne",
                if *value > 1 { "s" } else { "" }
            )
        }
        Effect::AutoDispatch => "Départs automatiques".to_string(),
        Effect::NoWaterStop => "Plus d'arrêt pour faire de l'eau".to_string(),
        Effect::VariableGauge => "Accès aux pays d'un autre écartement".to_string(),
    }
}

fn describe_upgrade_effect(effect: &UpgradeEffect) -> String {
    match effect {
        UpgradeEffect::LineSpeed { value, only } if only.is_empty() => {
            format!("Vitesse autorisée : {} km/h", value.round())
        }
        UpgradeEffect::LineSpeed { value, only } => {
            let tractions: Vec<&str> = only.iter().map(|t| traction_name(t)).collect();
            format!(
                "Vitesse autorisée : {} km/h ({})",
                value.round(),
                tractions.join(", ")
            )
        }
        UpgradeEffect::WearMult { value } => percent_change("Usure de la voie", *value),
    }
}

fn describe_event_effect(app: &App, effect: &EventEffect) -> String {
    let signed = |v: f64| {
        format!(
            "{}{} %",
            if v >= 0.0 { "+" } else { "-" },
            (v.abs() * 100.0).round()
        )
    };
    match effect {
        EventEffect::Money { value } => {
            format!(
                "{}{}",
                if *value >= 0.0 { "+" } else { "-" },
                format::money(value.abs())
            )
        }
        EventEffect::GrantResearch { target } => {
            let name = app
                .data
                .research(target)
                .map_or(target.as_str(), |r| r.name.fr.as_str());
            format!("Recherche offerte : {name}")
        }
        EventEffect::Satisfaction { value, class } => {
            let who = class
                .as_ref()
                .and_then(|c| app.data.class(c))
                .map(|c| c.name.fr.to_lowercase());
            match who {
                Some(who) => format!("Satisfaction ({who}) {}", signed(*value)),
                None => format!("Satisfaction {}", signed(*value)),
            }
        }
        EventEffect::TrackCondition { value } => format!("État de la voie {}", signed(*value)),
    }
}

// ───────────────────────────── Dépôt ─────────────────────────────

fn draw_shop(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let locos = &app.data.locomotives;
    let line_id = app.line_id();
    let m = app.world.modifiers(&app.data);
    let era = app.world.era(&app.data);
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        &format!(
            "Dépôt : une locomotive pour {}",
            app.data.line(app.line_id()).map_or("", |l| l.name.as_str())
        ),
        locos.len(),
        (104, 34),
    );

    let rows: Vec<Line> = locos
        .iter()
        .enumerate()
        .map(|(i, loco)| {
            let (status, style) = match app.world.purchase_blocker(&app.data, loco, line_id) {
                Some(blocker) => (short_reason(&blocker), t.warn),
                None if app.world.money < loco.cost => ("trop chère", t.dim),
                None => ("disponible", t.good),
            };
            let text = format!(
                "{} {:<30} {}  {:>4} km/h  {:>4} pl.  {:>12}  ",
                cursor(app, i == sel),
                loco.name,
                loco.year,
                loco.top_speed_kmh.round(),
                m.capacity_of(loco),
                format::money(loco.cost),
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(format!(" {status}"), style),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    if let Some(loco) = locos.get(sel) {
        let state = app.world.lines.get(line_id);
        let allowed = allowed_speed(&app.data, &m, loco, state);
        let fuel = app.data.fuel(&loco.fuel);
        let record = loco.record_kmh.map_or(String::new(), |r| {
            format!(" (record : {} km/h)", format::quantity(r))
        });
        let mut lines = vec![
            Line::styled(
                format!("{} {} {}, {}", loco.name, g.dash, loco.builder, loco.year),
                t.title,
            ),
            Line::styled(
                format!(
                    "{} {} traction {}",
                    loco.wheels,
                    g.bullet,
                    traction_name(&loco.traction)
                ),
                t.dim,
            ),
            Line::styled(
                format!(
                    "Vitesse {} km/h{record} {} sur ta ligne : {} km/h",
                    format::quantity(loco.top_speed_kmh),
                    g.bullet,
                    format::quantity(allowed)
                ),
                t.text,
            ),
            Line::styled(
                format!(
                    "Puissance {} kW {} {} t {} accélération {} m/s² {} {} places",
                    format::int(loco.power_kw),
                    g.bullet,
                    format::quantity(loco.weight_t),
                    g.bullet,
                    format::dec2(m.accel_of(loco, &app.data.balance)),
                    g.bullet,
                    m.capacity_of(loco)
                ),
                t.text,
            ),
            Line::styled(
                format!(
                    "{} : {:.3} {}/km {} entretien {}/h {} usure de la voie x{}",
                    fuel.map_or("Énergie", |f| f.name.fr.as_str()),
                    loco.fuel_per_km * m.fuel,
                    fuel.map_or("", |f| f.unit.as_str()),
                    g.bullet,
                    format::money(loco.upkeep_per_h * era.cost_mult),
                    g.bullet,
                    format::dec1(loco.track_wear),
                )
                .replace('.', ","),
                t.text,
            ),
        ];
        if loco.speculative {
            lines.push(Line::styled(
                "Concept spéculatif : cette machine n'a jamais existé.",
                t.warn,
            ));
        }
        if let Some(blocker) = app.world.purchase_blocker(&app.data, loco, line_id) {
            lines.push(Line::styled(format!("Indisponible : {blocker}"), t.warn));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(loco.history.fr.clone(), t.text));
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : acheter    haut/bas : choisir    Échap : fermer",
    );
}

// ─────────────────────────── Recherche ───────────────────────────

/// Fiche d'une recherche : description, effets, ce qu'elle débloque, prérequis.
pub fn research_card<'a>(app: &'a App, r: &'a Research) -> Vec<Line<'a>> {
    let t = &app.theme;
    let g = &app.glyphs;
    let era = app
        .data
        .era(&r.era)
        .map_or(r.era.as_str(), |e| e.name.fr.as_str());
    let mut lines = vec![
        Line::styled(format!("{} {} {}", r.name.fr, g.dash, r.year), t.title),
        Line::styled(
            format!("{} {} {era}", branch_name(r.branch), g.bullet),
            t.dim,
        ),
        Line::raw(""),
        Line::styled(r.description.fr.clone(), t.text),
        Line::raw(""),
    ];
    if !r.effects.is_empty() {
        let effects: Vec<String> = r.effects.iter().map(describe_effect).collect();
        lines.push(Line::styled(
            format!("Effets : {}", effects.join(", ")),
            t.good,
        ));
    }
    let mut unlocks: Vec<&str> = app
        .data
        .unlocked_by(&r.id)
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    unlocks.extend(
        app.data
            .upgrades_unlocked_by(&r.id)
            .iter()
            .map(|u| u.name.fr.as_str()),
    );
    if !unlocks.is_empty() {
        lines.push(Line::styled(
            format!("Débloque : {}", unlocks.join(", ")),
            t.good,
        ));
    }
    for required in r.requires.iter().filter_map(|id| app.data.research(id)) {
        let done = app.world.research_done.contains(&required.id);
        lines.push(Line::styled(
            format!(
                "{} Prérequis : {}",
                if done { g.check } else { g.dash },
                required.name.fr
            ),
            if done { t.dim } else { t.warn },
        ));
    }
    if let Some(blocker) = app
        .world
        .research_blocker(&app.data, r)
        .filter(|b| *b != ActionError::AlreadyResearched)
    {
        lines.push(Line::styled(format!("Indisponible : {blocker}"), t.warn));
    }
    lines
}

fn draw_research(app: &App, buf: &mut Buffer, area: Rect, sel: usize, branch: Option<Branch>) {
    let t = &app.theme;
    let g = &app.glyphs;
    let items = app.research_items(branch);
    let time_scale = app.data.balance.time_scale;
    let inner = window(
        app,
        buf,
        area,
        "Bureau d'études : arbre de recherche",
        104,
        34,
    );
    let list_height = (items.len() as u16 + 1)
        .min(inner.height.saturating_sub(4) / 2)
        .max(1);
    let [tabs, list, detail, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(list_height),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Onglets de branche.
    let mut spans = Vec::new();
    for option in std::iter::once(None).chain(Branch::ALL.into_iter().map(Some)) {
        let label = option.map_or("Toutes", branch_name);
        let done = app
            .data
            .research
            .iter()
            .filter(|r| option.is_none_or(|b| r.branch == b));
        let (count, total) = done.fold((0, 0), |(c, n), r| {
            (c + app.world.research_done.contains(&r.id) as u32, n + 1)
        });
        let style = if option == branch {
            t.text.patch(t.selected)
        } else {
            t.text
        };
        spans.push(Span::styled(format!(" {label} {count}/{total} "), style));
        spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(spans)).render(tabs, buf);

    let rows: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let active = app.world.research_active.as_ref().filter(|a| a.id == r.id);
            let (mark, status, style) = if app.world.research_done.contains(&r.id) {
                (g.check, "terminée".to_string(), t.good)
            } else if let Some(a) = active {
                (
                    g.cursor,
                    format!(
                        "en cours {}",
                        format::percent(a.progress_s / (r.duration_h * 3600.0))
                    ),
                    t.title,
                )
            } else if let Some(blocker) = app.world.research_blocker(&app.data, r) {
                (g.dash, short_reason(&blocker).to_string(), t.warn)
            } else if app.world.money < r.cost {
                (g.bullet, "trop chère".to_string(), t.dim)
            } else {
                (g.bullet, "disponible".to_string(), t.good)
            };
            let text = format!(
                "{} {mark} {:<41} {:<13} {:>10} {:>7}  ",
                cursor(app, i == sel),
                r.name.fr,
                branch_name(r.branch),
                format::money(r.cost),
                format::duration(r.duration_h * 3600.0 / time_scale),
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(format!(" {status}"), style),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    if let Some(r) = items.get(sel) {
        Paragraph::new(research_card(app, r))
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : lancer    haut/bas : choisir    Tab : branche    v : vue en arbre    Échap : fermer",
    );
}

// ─────────────────────────── Époques ───────────────────────────

fn draw_eras(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let eras = &app.data.eras;
    let current = app.world.era_index(&app.data);
    let [list, detail, foot] = panel(app, buf, area, "Époques", eras.len(), (96, 30));

    let rows: Vec<Line> = eras
        .iter()
        .enumerate()
        .map(|(i, era)| {
            let (mark, note, style) = match i.cmp(&current) {
                std::cmp::Ordering::Less => (g.check, "passée", t.dim),
                std::cmp::Ordering::Equal => (g.cursor, "époque actuelle", t.good),
                std::cmp::Ordering::Greater if i == current + 1 => (g.bullet, "prochaine", t.title),
                std::cmp::Ordering::Greater => (" ", "", t.dim),
            };
            let text = format!(
                "{} {mark} {}-{}  {:<28}",
                cursor(app, i == sel),
                era.from,
                era.to,
                era.name.fr
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(format!(" {note}"), style),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    if let Some(era) = eras.get(sel) {
        let mut rules = vec![
            format!("demande x{}", format::quantity(era.demand_mult)),
            format!("tarifs x{}", format::dec1(era.fare_mult)),
            format!("coûts x{}", format::dec1(era.cost_mult)),
        ];
        rules.push(if era.rules.fuel_market {
            "combustible acheté d'avance, à cours variable".to_string()
        } else {
            "combustible livré au jour le jour, à prix fixe".to_string()
        });
        if era.rules.water_stop_min > 0.0 {
            rules.push(format!(
                "arrêt de {} min pour l'eau (vapeur)",
                era.rules.water_stop_min as u32
            ));
        }
        let mut lines = vec![
            Line::styled(
                format!("{} {} {}-{}", era.name.fr, g.dash, era.from, era.to),
                t.title,
            ),
            Line::raw(""),
            Line::styled(era.description.fr.clone(), t.text),
            Line::raw(""),
            Line::styled(format!("Règles : {}.", rules.join(" ; ")), t.dim),
        ];
        if sel == current + 1
            && let Some(status) = app.world.next_era(&app.data)
        {
            lines.push(Line::raw(""));
            lines.push(Line::styled("Pour y entrer :", t.title));
            let item = |ok: bool, text: String| {
                Line::styled(
                    format!("{} {text}", if ok { g.check } else { g.dash }),
                    if ok { t.good } else { t.warn },
                )
            };
            for (research, done) in &status.research {
                lines.push(item(*done, format!("Recherche : {}", research.name.fr)));
            }
            lines.push(item(
                status.passengers >= status.passengers_needed,
                format!(
                    "Voyageurs et tonnes transportés : {} / {}",
                    format::int(status.passengers as f64),
                    format::int(status.passengers_needed as f64)
                ),
            ));
            lines.push(item(
                status.affordable,
                format!("Investissement : {}", format::money(status.cost)),
            ));
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : passer à l'époque suivante    haut/bas : parcourir    Échap : fermer",
    );
}

// ───────────────────────── Améliorations ─────────────────────────

fn draw_upgrades(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let upgrades = &app.data.upgrades;
    let line_id = app.line_id();
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        &format!(
            "Améliorations : {}",
            app.data.line(app.line_id()).map_or("", |l| l.name.as_str())
        ),
        upgrades.len(),
        (96, 26),
    );

    let rows: Vec<Line> = upgrades
        .iter()
        .enumerate()
        .map(|(i, upgrade)| {
            let cost = app
                .world
                .upgrade_cost(&app.data, line_id, upgrade)
                .unwrap_or(0.0);
            let (status, style) = match app.world.upgrade_blocker(&app.data, line_id, upgrade) {
                Some(ActionError::AlreadyInstalled) => ("installée", t.good),
                Some(blocker) => (short_reason(&blocker), t.warn),
                None if app.world.money < cost => ("trop chère", t.dim),
                None => ("disponible", t.good),
            };
            let text = format!(
                "{} {:<30} {:>12}  ",
                cursor(app, i == sel),
                upgrade.name.fr,
                format::money(cost)
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(format!(" {status}"), style),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    if let Some(upgrade) = upgrades.get(sel) {
        let mut lines = vec![
            Line::styled(upgrade.name.fr.clone(), t.title),
            Line::styled(upgrade.description.fr.clone(), t.text),
        ];
        if !upgrade.effects.is_empty() {
            let effects: Vec<String> = upgrade
                .effects
                .iter()
                .map(describe_upgrade_effect)
                .collect();
            lines.push(Line::styled(
                format!("Effets : {}", effects.join(", ")),
                t.good,
            ));
        }
        let users: Vec<&str> = app
            .data
            .locomotives
            .iter()
            .filter(|l| l.requires_upgrade.as_ref() == Some(&upgrade.id))
            .map(|l| l.name.as_str())
            .collect();
        if !users.is_empty() {
            lines.push(Line::styled(
                format!("Nécessaire à : {}", users.join(", ")),
                t.good,
            ));
        }
        if let Some(blocker) = app
            .world
            .upgrade_blocker(&app.data, line_id, upgrade)
            .filter(|b| *b != ActionError::AlreadyInstalled)
        {
            lines.push(Line::styled(format!("Indisponible : {blocker}"), t.warn));
        }
        if let (Some(line), Some(state)) = (app.data.line(line_id), app.world.lines.get(line_id)) {
            let props = LineProps::of(state, &app.data);
            let m = app.world.modifiers(&app.data);
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                format!(
                    "{} {} {} km {} voie : {} km/h {} signalisation : {} km/h",
                    line.name,
                    g.bullet,
                    format::quantity(line.length_km),
                    g.bullet,
                    props.speed_limit("steam").round(),
                    g.bullet,
                    m.signal_speed.round()
                ),
                t.dim,
            ));
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : installer    haut/bas : choisir    Échap : fermer",
    );
}

// ───────────────────────── Combustibles ─────────────────────────

fn draw_fuel(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let fuels = &app.data.fuels;
    let era = app.world.era(&app.data);
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        "Combustibles et énergie",
        fuels.len(),
        (96, 20),
    );

    let rows: Vec<Line> = fuels
        .iter()
        .enumerate()
        .map(|(i, fuel)| {
            let price = fuel.price(app.world.clock, era);
            let base = fuel.base_price * era.cost_mult;
            let change = ((price / base - 1.0) * 100.0).round() as i64;
            let burn = app.world.fuel_burn_per_hour(&app.data, &fuel.id);
            let stock = app.world.fuel_stock(&fuel.id);
            let stored = if fuel.stock && era.rules.fuel_market {
                format!("stock {:>7} {:<3}", format::quantity(stock), fuel.unit)
            } else {
                format!("{:<17}", "à la demande")
            };
            let text = format!(
                "{} {:<12} {stored}  cours {:>9}/{:<3} {:>5}  conso {:>6} {}/h ",
                cursor(app, i == sel),
                fuel.name.fr,
                format::money(price),
                fuel.unit,
                format!("{}{change} %", if change >= 0 { "+" } else { "" }),
                format::dec1(burn),
                fuel.unit,
            );
            let low = fuel.stock && era.rules.fuel_market && burn > 0.0 && stock < burn * 2.0;
            let trend = if change <= -10 {
                t.good
            } else if change >= 10 || low {
                t.warn
            } else {
                t.dim
            };
            let note = if low {
                " stock bas"
            } else if change <= -10 {
                " bon moment"
            } else {
                ""
            };
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(note, trend),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    let mut lines = Vec::new();
    if !era.rules.fuel_market {
        lines.push(Line::styled(
            "À cette époque, le combustible est livré au jour le jour, à prix fixe : rien à gérer ici pour l'instant.",
            t.text,
        ));
    } else if let Some(fuel) = fuels.get(sel) {
        if fuel.stock {
            let lot_cost = fuel.lot * fuel.price(app.world.clock, era);
            lines.push(Line::styled(
                format!(
                    "Un lot de {} {} coûte {} aujourd'hui. Le cours varie de jour en jour : achète quand il est bas.",
                    format::quantity(fuel.lot),
                    fuel.unit,
                    format::money(lot_cost)
                ),
                t.text,
            ));
            lines.push(Line::styled(
                format!(
                    "Si le stock tombe à zéro, les trains roulent quand même, mais chaque livraison d'urgence est payée {} plus cher.",
                    format::percent(app.data.balance.fuel_emergency_mult - 1.0)
                ),
                t.dim,
            ));
        } else {
            lines.push(Line::styled(
                format!(
                    "{} ne se stocke pas : elle est facturée au cours du jour, à la consommation.",
                    fuel.name.fr
                ),
                t.text,
            ));
        }
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "Entrée : acheter un lot    x : dix lots    haut/bas : choisir    Échap : fermer",
    );
}

// ───────────────────────────── Réseau ─────────────────────────────

fn draw_network(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let rows = app.network_rows();
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        "Réseau : pays et lignes",
        rows.len(),
        (104, 36),
    );
    let home_gauge = app.data.home_country().map(|c| c.gauge_mm);

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| match row {
            NetworkRow::Country(country) => {
                let (status, style) = match app.world.concession_blocker(&app.data, country) {
                    Some(ActionError::AlreadyOwned) => ("concession acquise".to_string(), t.good),
                    Some(blocker) => (short_reason(&blocker).to_string(), t.warn),
                    None => (
                        format!("concession : {}", format::money(country.concession_cost)),
                        if app.world.money >= country.concession_cost {
                            t.good
                        } else {
                            t.dim
                        },
                    ),
                };
                let text = format!(
                    "{} {:<34} voie de {:>4} mm  ",
                    cursor(app, i == sel),
                    country.name.fr.to_uppercase(),
                    country.gauge_mm
                );
                let base = if i == sel {
                    t.title.patch(t.selected)
                } else {
                    t.title
                };
                Line::from(vec![
                    Span::styled(text, base),
                    Span::styled(format!(" {status}"), style),
                ])
            }
            NetworkRow::Line(line) => {
                let (status, style) = match app.world.line_blocker(&app.data, line) {
                    Some(ActionError::AlreadyOwned) if line.id == app.line_id() => {
                        ("ligne courante".to_string(), t.title)
                    }
                    Some(ActionError::AlreadyOwned) => (
                        format!(
                            "exploitée, {}/{} trains",
                            app.world.trains_on(&line.id),
                            app.world.line_capacity(&app.data, &line.id)
                        ),
                        t.good,
                    ),
                    Some(blocker) => (short_reason(&blocker).to_string(), t.warn),
                    None => {
                        let cost = app.world.line_cost(&app.data, line);
                        (
                            format!("à construire : {}", format::money(cost)),
                            if app.world.money >= cost {
                                t.good
                            } else {
                                t.dim
                            },
                        )
                    }
                };
                let text = format!(
                    "{}   {:<32} {:>5} km  ",
                    cursor(app, i == sel),
                    line.name,
                    format::quantity(line.length_km)
                );
                Line::from(vec![
                    Span::styled(text, row_style(app, i == sel)),
                    Span::styled(format!(" {status}"), style),
                ])
            }
        })
        .collect();
    render_list(buf, list, lines, sel);

    let mut lines = Vec::new();
    let hint = match rows.get(sel) {
        Some(NetworkRow::Country(country)) => {
            let gauge = if Some(country.gauge_mm) == home_gauge {
                "le même que dans ton pays de départ"
            } else {
                "différent de celui de ton pays de départ"
            };
            let demand: Vec<String> = app
                .data
                .traffic
                .iter()
                .map(|c| {
                    format!(
                        "{} x{}",
                        c.name.fr.to_lowercase(),
                        format::dec1(country.demand_mult(&c.id))
                    )
                })
                .collect();
            lines.push(Line::styled(country.name.fr.clone(), t.title));
            lines.push(Line::styled(
                format!("Écartement des voies : {} mm, {gauge}.", country.gauge_mm),
                t.text,
            ));
            lines.push(Line::styled(
                format!(
                    "Relief {} : construction x{} {} capitale : {}",
                    country.terrain,
                    format::dec1(country.build_cost),
                    g.bullet,
                    country.capital().map_or("", |s| s.name.as_str())
                ),
                t.text,
            ));
            lines.push(Line::styled(
                format!("Demande : {}.", demand.join(", ")),
                t.dim,
            ));
            match app.world.concession_blocker(&app.data, country) {
                Some(ActionError::AlreadyOwned) => {}
                Some(blocker) => lines.push(Line::styled(format!("Indisponible : {blocker}"), t.warn)),
                None => lines.push(Line::styled(
                    "La concession donne le droit de construire dans ce pays. Un nouveau réseau part de la capitale, ou d'une ligne internationale.",
                    t.dim,
                )),
            }
            "Entrée : acheter la concession    haut/bas : parcourir    Échap : fermer"
        }
        Some(NetworkRow::Line(line)) => {
            let b = &app.data.balance;
            let era = app.world.era(&app.data);
            let demand = app.data.line_demand(line) * era.demand_mult;
            let ticket = b.fare_per_km * line.length_km * era.fare_mult;
            lines.push(Line::styled(
                format!(
                    "{} {} {} km",
                    line.name,
                    g.dash,
                    format::quantity(line.length_km)
                ),
                t.title,
            ));
            lines.push(Line::styled(
                format!(
                    "{} {} {} {} jusqu'à {} trains",
                    app.station_name(&line.from),
                    g.arrow,
                    app.station_name(&line.to),
                    g.bullet,
                    app.world.line_capacity(&app.data, &line.id)
                ),
                t.text,
            ));
            lines.push(Line::styled(
                format!(
                    "Demande : environ {} voyageurs par heure à chaque bout {} billet de base : {}",
                    format::int(demand),
                    g.bullet,
                    format::money(ticket)
                ),
                t.text,
            ));
            match app.world.lines.get(&line.id).filter(|state| state.built) {
                Some(state) => {
                    let upgrades: Vec<&str> = state
                        .upgrades
                        .iter()
                        .filter_map(|id| app.data.upgrade(id))
                        .map(|u| u.name.fr.as_str())
                        .collect();
                    lines.push(Line::styled(
                        format!(
                            "État de la voie : {} {} améliorations : {}",
                            format::percent(state.condition),
                            g.bullet,
                            if upgrades.is_empty() {
                                "aucune".to_string()
                            } else {
                                upgrades.join(", ")
                            }
                        ),
                        t.dim,
                    ));
                    "Entrée : en faire la ligne courante    haut/bas : parcourir    Échap : fermer"
                }
                None => {
                    if let Some(blocker) = app.world.line_blocker(&app.data, line) {
                        lines.push(Line::styled(format!("Indisponible : {blocker}"), t.warn));
                    }
                    lines.push(Line::styled(
                        "Une ligne neuve n'a ni trains ni améliorations : il faudra l'équiper. Les longues lignes transportent moins de voyageurs, mais chacun paie plus cher.",
                        t.dim,
                    ));
                    "Entrée : construire la ligne    haut/bas : parcourir    Échap : fermer"
                }
            }
        }
        None => "Échap : fermer",
    };
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(app, buf, foot, hint);
}

// ─────────────────────────── Personnel ───────────────────────────

fn draw_staff(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let staff = &app.data.staff;
    let era = app.world.era(&app.data);
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        "Personnel : automatiser l'exploitation",
        staff.len(),
        (96, 22),
    );

    let rows: Vec<Line> = staff
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (status, style) = match app.world.staff_blocker(&app.data, s) {
                Some(ActionError::AlreadyOwned) => ("en service", t.good),
                Some(blocker) => (short_reason(&blocker), t.warn),
                None if app.world.money < s.cost => ("trop cher", t.dim),
                None => ("disponible", t.good),
            };
            let text = format!(
                "{} {:<32} {:>11}  salaire {:>8}/h  ",
                cursor(app, i == sel),
                s.name.fr,
                format::money(s.cost),
                format::money(s.salary_per_h * era.cost_mult),
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(format!(" {status}"), style),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    let mut lines = Vec::new();
    if let Some(s) = staff.get(sel) {
        lines.push(Line::styled(s.name.fr.clone(), t.title));
        lines.push(Line::styled(s.description.fr.clone(), t.text));
        let rule = match s.role {
            StaffRole::AutoRepair { threshold } => {
                format!(
                    "Répare chaque ligne dès que l'état de sa voie passe sous {}.",
                    format::percent(threshold)
                )
            }
            StaffRole::AutoFuel {
                reserve_h,
                max_price_ratio,
            } => format!(
                "Vise {} heures de réserve ; achète quand le cours ne dépasse pas {} du prix normal.",
                format::quantity(reserve_h),
                format::percent(max_price_ratio)
            ),
            StaffRole::AutoTimetable { crowd_factor } => {
                format!(
                    "Arrêt en gare réduit jusqu'à {} quand les quais sont pleins.",
                    format::percent(crowd_factor)
                )
            }
        };
        lines.push(Line::styled(rule, t.good));
        if let Some(blocker) = app
            .world
            .staff_blocker(&app.data, s)
            .filter(|b| *b != ActionError::AlreadyOwned)
        {
            lines.push(Line::styled(format!("Indisponible : {blocker}"), t.warn));
        }
        lines.push(Line::raw(""));
    }
    let auto = app.world.modifiers(&app.data).auto_dispatch;
    lines.push(Line::styled(
        if auto {
            "Départs automatiques : assurés par les signaleurs de voie."
        } else {
            "Départs automatiques : pas encore. Ils viennent de la recherche « Signaleurs de voie » (r)."
        },
        t.dim,
    ));
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "Entrée : engager    haut/bas : choisir    Échap : fermer",
    );
}

// ─────────────────────────── Compagnie ───────────────────────────

fn draw_company(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let prestige = &app.world.prestige;
    let bonuses = &app.data.bonuses;
    let gain = app.world.prestige_gain(&app.data);
    let title = format!("Compagnie : {} points de prestige", prestige.points);
    let [list, detail, foot] = panel(app, buf, area, &title, bonuses.len() + 1, (96, 26));

    let mut rows = vec![Line::from(vec![
        Span::styled(
            format!(
                "{} {:<44}",
                cursor(app, sel == 0),
                "Nationaliser la compagnie"
            ),
            if sel == 0 {
                t.title.patch(t.selected)
            } else {
                t.title
            },
        ),
        Span::styled(
            if gain > 0 {
                format!("  +{gain} points")
            } else {
                "  pas encore possible".to_string()
            },
            if gain > 0 { t.good } else { t.dim },
        ),
    ])];
    for (i, bonus) in bonuses.iter().enumerate() {
        let level = prestige.level(&bonus.id);
        let (status, style) = if level >= bonus.max_level {
            ("niveau maximal".to_string(), t.dim)
        } else {
            let cost = bonus.cost_at(level);
            (
                format!("{cost} points"),
                if prestige.points >= cost {
                    t.good
                } else {
                    t.dim
                },
            )
        };
        let text = format!(
            "{} {:<32} niveau {:>2}/{:<2} ",
            cursor(app, sel == i + 1),
            bonus.name.fr,
            level,
            bonus.max_level
        );
        rows.push(Line::from(vec![
            Span::styled(text, row_style(app, sel == i + 1)),
            Span::styled(format!("  {status}"), style),
        ]));
    }
    render_list(buf, list, rows, sel);

    let mut lines = Vec::new();
    if sel == 0 {
        let b = &app.data.balance;
        let era = app
            .data
            .era(&b.prestige_min_era)
            .map_or("", |e| e.name.fr.as_str());
        lines.push(Line::styled("Nationaliser la compagnie", t.title));
        lines.push(Line::styled(
            "L'État rachète ton réseau. Tu repars de 1825 avec une nouvelle compagnie : lignes, trains, recherches, personnel et argent sont remis à zéro. Les points de prestige et les bonus déjà acquis, eux, restent pour toujours.",
            t.text,
        ));
        lines.push(Line::styled(
            format!(
                "Recettes cumulées de cette compagnie : {}. Chaque facteur dix au-delà de {} rapporte {} points, à partir de l'époque « {era} ».",
                format::money(app.world.lifetime_income),
                format::money(b.prestige_base),
                format::quantity(b.prestige_per_decade)
            ),
            t.dim,
        ));
        lines.push(Line::styled(
            format!(
                "Compagnies déjà nationalisées : {} {} points gagnés depuis toujours : {}",
                prestige.resets, app.glyphs.bullet, prestige.earned
            ),
            t.dim,
        ));
    } else if let Some(bonus) = bonuses.get(sel - 1) {
        let level = prestige.level(&bonus.id) as f64;
        let describe = |n: f64| match bonus.kind {
            BonusKind::IncomeMult => format!("recettes +{}", format::percent(bonus.value * n)),
            BonusKind::ResearchSpeed => format!("recherche +{}", format::percent(bonus.value * n)),
            BonusKind::BuildCostMult => {
                format!(
                    "construction -{}",
                    format::percent(1.0 - (1.0 - bonus.value).powf(n))
                )
            }
            BonusKind::OfflineEfficiency => format!(
                "efficacité hors ligne +{}",
                format::percent(bonus.value * n)
            ),
            BonusKind::StartMoney => {
                format!("capital de départ +{}", format::money(bonus.value * n))
            }
        };
        lines.push(Line::styled(bonus.name.fr.clone(), t.title));
        lines.push(Line::styled(bonus.description.fr.clone(), t.text));
        lines.push(Line::styled(
            format!(
                "Actuellement : {}. Au niveau suivant : {}.",
                describe(level),
                describe(level + 1.0)
            ),
            t.good,
        ));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "Entrée : nationaliser ou acheter un niveau    haut/bas : choisir    Échap : fermer",
    );
}

// ─────────────────────────── Équipages ───────────────────────────

/// Compétence sur cinq étoiles.
fn stars(app: &App, skill: f64) -> String {
    format::gauge(skill, 5, app.glyphs.star, app.glyphs.star_empty)
}

fn draw_crew(app: &App, buf: &mut Buffer, area: Rect, sel: usize, recruiting: bool) {
    let t = &app.theme;
    let g = &app.glyphs;
    let rules = &app.data.crew;
    let members = if recruiting {
        &app.world.candidates
    } else {
        &app.world.crew
    };
    let inner = window(
        app,
        buf,
        area,
        "Équipages : conducteurs et contrôleurs",
        104,
        30,
    );
    let list_height = (members.len() as u16 + 2)
        .min(inner.height.saturating_sub(4) / 2)
        .max(2);
    let [tabs, list, detail, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(list_height),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);

    let tab = |label: String, on: bool| {
        Span::styled(
            format!(" {label} "),
            if on { t.text.patch(t.selected) } else { t.text },
        )
    };
    let wages = app.world.crew_wages_per_hour(&app.data);
    Paragraph::new(Line::from(vec![
        tab(format!("Employés {}", app.world.crew.len()), !recruiting),
        Span::raw(" "),
        tab(
            format!("Recrutement {}", app.world.candidates.len()),
            recruiting,
        ),
        Span::styled(
            format!("   masse salariale : {}/h", format::money(wages)),
            t.dim,
        ),
    ]))
    .render(tabs, buf);

    let rows: Vec<Line> = members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let post = if recruiting {
                match app.world.hire_fee(&app.data, m) {
                    fee if fee > 0.0 => format!("prime {}", format::money(fee)),
                    _ => "prime offerte".to_string(),
                }
            } else {
                let post = match m.train.and_then(|id| app.world.train(id)) {
                    Some(train) => app.train_name(train),
                    None => "en réserve".to_string(),
                };
                if app.world.in_training(m.id) {
                    format!("{post} (en formation)")
                } else {
                    post
                }
            };
            let text = format!(
                "{} {:<22} {:<11} {}  {}  {}  {:>9}/h  ",
                cursor(app, i == sel),
                m.name,
                m.role.name(),
                stars(app, m.skills[0]),
                stars(app, m.skills[1]),
                stars(app, m.skills[2]),
                format::money(app.world.wage_of(&app.data, m)),
            );
            let style = if recruiting && app.world.money < app.world.hire_fee(&app.data, m) {
                t.dim
            } else if m.train.is_some() || recruiting {
                t.good
            } else {
                t.dim
            };
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(post, style),
            ])
        })
        .collect();
    if rows.is_empty() {
        let text = if recruiting {
            "Plus de candidats aujourd'hui : de nouveaux se présenteront demain."
        } else {
            "Aucun équipier pour l'instant : Tab ouvre le recrutement. Un train ne part pas sans conducteur ; sans contrôleur, il roule avec du personnel ordinaire."
        };
        Paragraph::new(Line::styled(text, t.dim))
            .wrap(Wrap { trim: true })
            .render(list, buf);
    } else {
        render_list(buf, list, rows, sel);
    }

    let mut lines = Vec::new();
    if let Some(m) = members.get(sel) {
        let e = &rules.effects;
        // Ce que vaut chaque compétence à ce niveau.
        let effects: [(f64, &str, &str); 3] = match m.role {
            crate::sim::CrewRole::Driver => [
                (e.driving_accel, "+", "d'accélération et de freinage"),
                (e.economy_fuel, "-", "de consommation"),
                (e.care_wear, "-", "d'usure de la voie"),
            ],
            crate::sim::CrewRole::Conductor => [
                (e.service_comfort, "+", "de confort pour les voyageurs"),
                (e.control_revenue, "+", "de recettes"),
                (e.boarding_speed, "+", "de vitesse d'embarquement"),
            ],
        };
        lines.push(Line::styled(
            format!(
                "{} {} {} {} {} km en service",
                m.name,
                g.dash,
                m.role.name().to_lowercase(),
                g.bullet,
                format::int(m.km)
            ),
            t.title,
        ));
        for ((skill, name), (max, sign, what)) in m.skills.iter().zip(m.role.skills()).zip(effects)
        {
            lines.push(Line::from(vec![
                Span::styled(format!("{name:<13} {} ", stars(app, *skill)), t.text),
                Span::styled(
                    format!(
                        "{:>4}   {sign}{} {what}",
                        format::percent(*skill),
                        format::percent(skill * max)
                    ),
                    t.dim,
                ),
            ]));
        }
        if !recruiting {
            let status = match app.world.enrolled_in(m.id) {
                Some(session) if session.started(app.world.clock) => format!(
                    "En formation ({}) jusqu'à {} : absent de son train.",
                    session.skill_name().to_lowercase(),
                    when(session.end)
                ),
                Some(session) => format!(
                    "Inscrit à la formation {} qui commence {}.",
                    session.skill_name().to_lowercase(),
                    when(session.start)
                ),
                None => "Aucune formation prévue : f pour en programmer une.".to_string(),
            };
            lines.push(Line::styled(status, t.text));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Les compétences progressent avec les kilomètres parcourus en service, ou avec une formation ; un équipier en réserve est payé mais n'apprend rien. Un train ne part pas sans un conducteur affecté.",
            t.dim,
        ));
        if !recruiting {
            let target = app
                .selected
                .and_then(|id| app.world.train(id))
                .map(|train| app.train_name(train));
            if let Some(target) = target {
                lines.push(Line::styled(
                    format!("Train sélectionné sur la carte : {target}."),
                    t.dim,
                ));
            }
        }
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        if recruiting {
            "Entrée : embaucher    Tab : employés    haut/bas : choisir    Échap : fermer"
        } else {
            "Entrée : affecter au train sélectionné (ou réserve)    f : formations    x : licencier    Tab : recrutement    Échap : fermer"
        },
    );
}

// ──────────────────────────── Formations ───────────────────────────

/// Jour et heure de jeu d'une valeur de l'horloge.
fn when(clock: f64) -> String {
    let (h, m) = crate::sim::hour_minute(clock);
    format!("jour {} à {h:02}h{m:02}", (clock / 86_400.0) as u64 + 1)
}

fn draw_training(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let rules = &app.data.crew;
    let sessions = &app.world.trainings;
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        "Formations des équipages",
        sessions.len().max(1),
        (104, 28),
    );
    let clock = app.world.clock;
    let rows: Vec<Line> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (state, style) = if s.started(clock) {
                (format!("en cours, fin {}", when(s.end)), t.good)
            } else {
                (format!("début {}", when(s.start)), t.text)
            };
            let text = format!(
                "{} {:<14} {:<12} {}/{} inscrits   ",
                cursor(app, i == sel),
                s.skill_name(),
                s.role.name(),
                s.enrolled.len(),
                rules.training_seats,
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(state, style),
            ])
        })
        .collect();
    if rows.is_empty() {
        Paragraph::new(Line::styled(
            "Aucune formation programmée : n pour en programmer une.",
            t.dim,
        ))
        .render(list, buf);
    } else {
        render_list(buf, list, rows, sel);
    }

    let mut lines = Vec::new();
    if let Some(s) = sessions.get(sel) {
        let names: Vec<String> = s
            .enrolled
            .iter()
            .filter_map(|e| app.world.crew_member(e.crew))
            .map(|m| m.name.clone())
            .collect();
        lines.push(Line::styled(
            format!(
                "{} {} {}",
                s.skill_name(),
                app.glyphs.dash,
                s.role.name().to_lowercase()
            ),
            t.title,
        ));
        lines.push(Line::styled(
            if names.is_empty() {
                "Personne n'est encore inscrit.".to_string()
            } else {
                format!("Inscrits : {}", names.join(", "))
            },
            t.text,
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "Une session dure {} h et accueille {} équipiers au plus. Chacun gagne +{} dans la compétence travaillée, sauf malchance ({} de risque de ne rien retenir). Les inscrits sont absents pendant la session : un conducteur en formation laisse son train à quai.",
            format::quantity(rules.training_duration_h),
            rules.training_seats,
            format::percent(rules.training_gain),
            format::percent(rules.training_fail_chance),
        ),
        t.dim,
    ));
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "n : programmer    Entrée : inscriptions    x : annuler (remboursé)    Échap : fermer",
    );
}

fn draw_training_new(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let courses = App::courses();
    let [list, detail, foot] = panel(
        app,
        buf,
        area,
        "Programmer une formation",
        courses.len(),
        (80, 18),
    );
    let rows: Vec<Line> = courses
        .iter()
        .enumerate()
        .map(|(i, (role, skill))| {
            Line::styled(
                format!(
                    "{} {:<14} {}",
                    cursor(app, i == sel),
                    role.skills()[*skill],
                    role.name().to_lowercase()
                ),
                row_style(app, i == sel),
            )
        })
        .collect();
    render_list(buf, list, rows, sel);
    Paragraph::new(Line::styled(
        format!(
            "La session commencera dans {} h : d'ici là, inscris jusqu'à {} équipiers du métier.",
            format::quantity(app.data.crew.training_delay_h),
            app.data.crew.training_seats
        ),
        t.dim,
    ))
    .wrap(Wrap { trim: true })
    .render(detail, buf);
    footer(app, buf, foot, "Entrée : programmer    Échap : retour");
}

fn draw_enroll(app: &App, buf: &mut Buffer, area: Rect, session: u32, sel: usize) {
    let t = &app.theme;
    let Some(s) = app.world.training(session) else {
        return;
    };
    let members = app.enroll_candidates(session);
    let title = format!(
        "Inscriptions : {} ({}) {}/{}",
        s.skill_name(),
        s.role.name().to_lowercase(),
        s.enrolled.len(),
        app.data.crew.training_seats
    );
    let [list, detail, foot] = panel(app, buf, area, &title, members.len().max(1), (96, 26));
    let rows: Vec<Line> = members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (status, style) = if s.has(m.id) {
                ("inscrit".to_string(), t.good)
            } else {
                match app.world.enroll_blocker(&app.data, session, m.id) {
                    Some(ActionError::MaxLevel) => ("déjà parfait".to_string(), t.dim),
                    Some(ActionError::AlreadyEnrolled) => ("autre formation".to_string(), t.dim),
                    Some(ActionError::TrainingFull) => ("complet".to_string(), t.dim),
                    Some(ActionError::TrainingStarted) => {
                        ("inscriptions closes".to_string(), t.dim)
                    }
                    Some(_) => ("indisponible".to_string(), t.dim),
                    None => {
                        let cost = app
                            .world
                            .training_cost(&app.data, m, s.skill)
                            .unwrap_or(0.0);
                        let style = if app.world.money < cost {
                            t.dim
                        } else {
                            t.text
                        };
                        (format::money(cost), style)
                    }
                }
            };
            let text = format!(
                "{} {:<22} {}  {:>4}   ",
                cursor(app, i == sel),
                m.name,
                stars(app, m.skills[s.skill]),
                format::percent(m.skills[s.skill]),
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(status, style),
            ])
        })
        .collect();
    if rows.is_empty() {
        Paragraph::new(Line::styled(
            format!(
                "Aucun {} employé : recrute-en d'abord (s).",
                s.role.name().to_lowercase()
            ),
            t.warn,
        ))
        .render(list, buf);
    } else {
        render_list(buf, list, rows, sel);
    }
    let text = if s.started(app.world.clock) {
        format!("Session en cours jusqu'à {}.", when(s.end))
    } else {
        format!(
            "Début {}. On peut se désinscrire (remboursé) jusque-là.",
            when(s.start)
        )
    };
    Paragraph::new(Line::styled(text, t.dim))
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "Entrée : inscrire / désinscrire    haut/bas : choisir    Échap : retour",
    );
}

// ───────────────────────────── Flotte ─────────────────────────────

fn draw_fleet(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let trains = &app.world.trains;
    let m = app.world.modifiers(&app.data);
    let line_id = app.line_id();
    let title = format!("Flotte : {} trains sur le réseau", trains.len());
    let [list, detail, foot] = panel(app, buf, area, &title, trains.len().max(1), (116, 26));

    let rows: Vec<Line> = trains
        .iter()
        .enumerate()
        .map(|(i, train)| {
            let (line, loco) = (app.data.line(&train.line), app.data.locomotive(&train.loco));
            let state = match (train.state, line) {
                (TrainState::AtStation { .. }, Some(line)) => {
                    format!("à quai à {}", app.station_name(train.dir.origin(line)))
                }
                (TrainState::Moving, Some(line)) => format!(
                    "{} {}, {} km/h",
                    g.arrow,
                    app.station_name(train.dir.destination(line)),
                    train.speed_kmh(app.world.clock).round()
                ),
                _ => String::new(),
            };
            let state = match &train.order {
                Some(TrainOrder::Sell) => format!("{state} {} vente", g.bullet),
                Some(TrainOrder::Replace { loco, .. }) => format!(
                    "{state} {} {}",
                    g.bullet,
                    app.data
                        .locomotive(loco)
                        .map_or("remplacement", |l| l.name.as_str())
                ),
                None if !train.crew.has_driver => format!("{state} {} sans conducteur", g.bullet),
                None => state,
            };
            let capacity = loco.map_or(0, |l| m.capacity_of(l));
            let text = format!(
                "{} {:<30} {:<24} {:<38} {:>9}  revente {:>10} ",
                cursor(app, i == sel),
                app.train_name(train),
                line.map_or("", |l| l.name.as_str()),
                state,
                format!("{}/{capacity}", train.count()),
                format::money(app.world.resale_value(&app.data, train.id).unwrap_or(0.0)),
            );
            Line::styled(
                text,
                if i == sel {
                    app.theme.train(i).patch(t.selected)
                } else {
                    app.theme.train(i)
                },
            )
        })
        .collect();
    if rows.is_empty() {
        Paragraph::new(Line::styled(
            "Aucun train : achète une locomotive au dépôt (a).",
            t.warn,
        ))
        .render(list, buf);
    } else {
        render_list(buf, list, rows, sel);
    }

    if let Some(loco) = trains
        .get(sel)
        .and_then(|train| app.data.locomotive(&train.loco))
    {
        let allowed = allowed_speed(&app.data, &m, loco, app.world.lines.get(line_id));
        let lines = vec![
            Line::styled(
                format!("{} {} {}, {}", loco.name, g.dash, loco.builder, loco.year),
                t.title,
            ),
            Line::styled(
                format!(
                    "Traction {} {} {} km/h sur ta ligne (machine : {} km/h) {} {} places",
                    traction_name(&loco.traction),
                    g.bullet,
                    format::quantity(allowed),
                    format::quantity(loco.top_speed_kmh),
                    g.bullet,
                    m.capacity_of(loco)
                ),
                t.text,
            ),
            Line::raw(""),
            Line::styled(
                format!(
                    "Revendre une machine rend {} de son prix d'achat et libère une place sur la ligne. Un train en route est revendu à son arrivée. Remplacer (r) garde la ligne et l'équipage : on paie la nouvelle machine, moins la reprise de l'ancienne.",
                    format::percent(app.data.balance.resale_ratio)
                ),
                t.dim,
            ),
        ];
        let mut lines = lines;
        if let Some(train) = trains.get(sel) {
            let driver = app
                .world
                .crew_on(train.id, CrewRole::Driver)
                .map(|c| c.name.clone());
            lines.insert(
                2,
                match driver {
                    Some(name) => Line::styled(format!("Conducteur : {name}"), t.text),
                    None => Line::styled(
                        "Sans conducteur, ce train reste à quai : recrute ou affecte-en un (s).",
                        t.warn,
                    ),
                },
            );
            let order = match &train.order {
                Some(TrainOrder::Sell) => Some("Vente prévue à la prochaine arrivée.".to_string()),
                Some(TrainOrder::Replace { loco, paid }) => Some(format!(
                    "Remplacement prévu à la prochaine arrivée par {} (payé {}).",
                    app.data
                        .locomotive(loco)
                        .map_or(loco.as_str(), |l| l.name.as_str()),
                    format::money(*paid)
                )),
                None => None,
            };
            if let Some(order) = order {
                lines.insert(3, Line::styled(format!("{order} x pour annuler."), t.warn));
            }
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : suivre    x : revendre / annuler    r : remplacer la machine    haut/bas : choisir    Échap : fermer",
    );
}

// ─────────────────────────── Remplacement ─────────────────────────

fn draw_replace(app: &App, buf: &mut Buffer, area: Rect, train_id: u32, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let Some(train) = app.world.train(train_id) else {
        return;
    };
    let locos = &app.data.locomotives;
    let m = app.world.modifiers(&app.data);
    let resale = app.world.resale_value(&app.data, train_id).unwrap_or(0.0);
    let title = format!("Remplacer {}", app.train_name(train));
    let [list, detail, foot] = panel(app, buf, area, &title, locos.len(), (104, 30));

    let rows: Vec<Line> = locos
        .iter()
        .enumerate()
        .map(|(i, loco)| {
            let net = loco.cost - resale;
            let (status, style) = match app.world.replace_blocker(&app.data, train_id, loco) {
                Some(blocker) => (short_reason(&blocker).to_string(), t.warn),
                None if app.world.money < net => ("trop chère".to_string(), t.dim),
                None if net < 0.0 => (format!("+{} de reprise", format::money(-net)), t.good),
                None => (format!("{} à payer", format::money(net)), t.good),
            };
            let text = format!(
                "{} {:<30} {}  {:>4} km/h  {:>4} pl.  {:>12}  ",
                cursor(app, i == sel),
                loco.name,
                loco.year,
                loco.top_speed_kmh.round(),
                m.capacity_of(loco),
                format::money(loco.cost),
            );
            Line::from(vec![
                Span::styled(text, row_style(app, i == sel)),
                Span::styled(format!(" {status}"), style),
            ])
        })
        .collect();
    render_list(buf, list, rows, sel);

    let mut lines = Vec::new();
    if let (Some(old), Some(new)) = (app.data.locomotive(&train.loco), locos.get(sel)) {
        let state = app.world.lines.get(&train.line);
        let speed = |l| allowed_speed(&app.data, &m, l, state);
        lines.push(Line::styled(
            format!("{} {} {}", old.name, g.arrow, new.name),
            t.title,
        ));
        lines.push(Line::styled(
            format!(
                "Vitesse sur la ligne : {} {} {} km/h {} places : {} {} {}",
                format::quantity(speed(old)),
                g.arrow,
                format::quantity(speed(new)),
                g.bullet,
                m.capacity_of(old),
                g.arrow,
                m.capacity_of(new)
            ),
            t.text,
        ));
        lines.push(Line::styled(
            format!(
                "Prix {} {} reprise de l'ancienne machine {} = {}",
                format::money(new.cost),
                g.dash,
                format::money(resale),
                format::money(new.cost - resale)
            ),
            t.text,
        ));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            if train.is_docked() {
                "Le train est à quai : la machine est changée sur-le-champ. La ligne, l'horaire et l'équipage ne changent pas."
            } else {
                "Le train roule : la nouvelle machine prendra le relais à son arrivée. Elle est payée dès la commande, et remboursée si tu annules (x dans la flotte)."
            },
            t.dim,
        ));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "Entrée : remplacer    haut/bas : choisir    Échap : retour à la flotte",
    );
}

// ─────────────────────────── Événements ───────────────────────────

fn draw_event(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let Some(event) = app
        .world
        .event_pending
        .as_ref()
        .and_then(|id| app.data.event(id))
    else {
        return;
    };
    let t = &app.theme;
    let title = format!("{} {} {}", event.year, app.glyphs.dash, event.title.fr);
    let inner = window(app, buf, area, &title, 92, 24);
    let [text, choices, foot] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(event.choices.len() as u16 + 1),
        Constraint::Length(1),
    ])
    .areas(inner);
    Paragraph::new(Line::styled(event.text.fr.clone(), t.text))
        .wrap(Wrap { trim: true })
        .render(text, buf);

    let rows: Vec<Line> = event
        .choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let affordable = app.world.money >= choice.cost();
            let style = if !affordable {
                t.dim
            } else {
                row_style(app, i == sel)
            };
            Line::styled(
                format!("{} {} ", cursor(app, i == sel), choice.label.fr),
                style,
            )
        })
        .collect();
    Paragraph::new(rows).render(choices, buf);
    footer(app, buf, foot, "haut/bas : choisir    Entrée : décider");
}

fn draw_outcome(app: &App, buf: &mut Buffer, area: Rect, event: usize, choice: usize) {
    let Some(event) = app.data.events.get(event) else {
        return;
    };
    let Some(choice) = event.choices.get(choice) else {
        return;
    };
    let t = &app.theme;
    let title = format!("{} {} {}", event.year, app.glyphs.dash, event.title.fr);
    let inner = window(app, buf, area, &title, 92, 18);
    let mut lines = vec![
        Line::styled(choice.label.fr.clone(), t.title),
        Line::raw(""),
        Line::styled(choice.outcome.fr.clone(), t.text),
        Line::raw(""),
    ];
    for effect in &choice.effects {
        let good = match effect {
            EventEffect::Money { value }
            | EventEffect::Satisfaction { value, .. }
            | EventEffect::TrackCondition { value } => *value >= 0.0,
            EventEffect::GrantResearch { .. } => true,
        };
        lines.push(Line::styled(
            describe_event_effect(app, effect),
            if good { t.good } else { t.warn },
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Appuie sur une touche pour continuer.", t.dim));
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(inner, buf);
}

// ─────────────────────── Tarifs et horaires ───────────────────────

fn draw_manage(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let b = &app.data.balance;
    let [settings, detail, foot] = panel(
        app,
        buf,
        area,
        &format!(
            "Tarifs et horaires : {}",
            app.data.line(app.line_id()).map_or("", |l| l.name.as_str())
        ),
        2,
        (88, 22),
    );
    let line_id = app.line_id();
    let (Some(line), Some(state)) = (app.data.line(line_id), app.world.lines.get(line_id)) else {
        return;
    };
    let m = app.world.modifiers(&app.data);
    let era = app.world.era(&app.data);

    let price_note = match ((state.fare_level - 1.0) * 100.0).round() as i64 {
        0 => "tarif normal".to_string(),
        p if p > 0 => format!("{p} % plus cher : moins de monde, plus de recette par billet"),
        p => format!(
            "{} % moins cher : plus de monde, moins de recette par billet",
            -p
        ),
    };
    let rows = [
        (
            "Prix des billets",
            format!("x{}", format::dec1(state.fare_level)),
            price_note,
        ),
        (
            "Arrêt en gare",
            format!("{} min", state.dwell_min as u32),
            if m.auto_dispatch {
                "départ automatique à l'heure prévue, ou dès que le train est plein".to_string()
            } else {
                "passé ce délai après l'arrivée, le train est en retard".to_string()
            },
        ),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, value, note))| {
            Line::from(vec![
                Span::styled(
                    format!("{} {label:<17} < {value:^7} > ", cursor(app, i == sel)),
                    row_style(app, i == sel),
                ),
                Span::styled(format!(" {note}"), t.dim),
            ])
        })
        .collect();
    Paragraph::new(lines).render(settings, buf);

    let mut table = vec![Line::styled(
        format!(
            "{:<12} {:>5} {:>10} {:>9}   {}",
            "Clientèle", "Part", "Demande/h", "Billet", "Satisfaction"
        ),
        t.title,
    )];
    for class in &app.data.traffic {
        let demand: f64 = [&line.from, &line.to]
            .into_iter()
            .filter_map(|id| app.data.station(id))
            .map(|station| demand_per_hour(&app.world, &app.data, &m, station, class))
            .sum();
        let ticket = b.fare_per_km
            * line.length_km
            * era.fare_mult
            * m.fare
            * class.fare_mult
            * state.fare_level;
        let mood = app.world.satisfaction_of(&class.id);
        let mood_style = if mood >= 0.6 {
            t.good
        } else if mood < 0.4 {
            t.warn
        } else {
            t.text
        };
        table.push(Line::from(vec![
            Span::styled(
                format!(
                    "{:<12} {:>5} {:>10} {:>9}   ",
                    class.name.fr,
                    format::percent(class.share),
                    format::dec1(demand),
                    format!("£ {}", format::dec2(ticket)),
                ),
                t.text,
            ),
            Span::styled(
                format::gauge(mood, 10, g.gauge_full, g.gauge_empty),
                mood_style,
            ),
            Span::styled(format!(" {}", format::percent(mood)), mood_style),
        ]));
    }
    table.push(Line::raw(""));
    table.push(Line::styled(
        format!(
            "Confort des voitures : {}. La satisfaction dépend du confort, des retards et de l'affluence ; elle fait monter ou baisser la demande. Les navetteurs voyagent surtout aux heures de pointe, le fret jour et nuit.",
            format::percent(m.comfort)
        ),
        t.dim,
    ));
    Paragraph::new(table)
        .wrap(Wrap { trim: false })
        .render(detail, buf);
    footer(
        app,
        buf,
        foot,
        "gauche/droite : régler    haut/bas : choisir    Échap : fermer",
    );
}

// ───────────────────── Retour d'absence et aide ─────────────────────

fn draw_welcome(app: &App, buf: &mut Buffer, area: Rect) {
    let Some(report) = &app.offline else { return };
    let t = &app.theme;
    let b = &app.data.balance;
    let mut lines = vec![
        Line::styled(
            format!("Tu es parti {}.", format::duration(report.away_s)),
            t.text,
        ),
        Line::styled(
            format!(
                "Le réseau a tourné l'équivalent de {} (hors ligne : {} du temps, {} h au plus).",
                format::duration(report.simulated_s),
                format::percent(b.offline_efficiency),
                b.offline_max_h
            ),
            t.dim,
        ),
        Line::raw(""),
    ];
    if report.trips > 0 {
        lines.push(Line::styled(
            format!(
                "{} trajets, {} voyageurs et tonnes transportés, {} de recettes.",
                format::int(report.trips as f64),
                format::int(report.carried as f64),
                format::money(report.revenue)
            ),
            t.text,
        ));
    }
    let sign = if report.net >= 0.0 { "+" } else { "-" };
    lines.push(Line::styled(
        format!("Trésorerie : {sign}{}", format::money(report.net.abs())),
        if report.net >= 0.0 { t.good } else { t.warn },
    ));
    for id in &report.research_done {
        let name = app
            .data
            .research(id)
            .map_or(id.as_str(), |r| r.name.fr.as_str());
        lines.push(Line::styled(format!("Recherche terminée : {name}"), t.good));
    }
    if report.track_worn {
        lines.push(Line::styled(
            "La voie s'est usée : pense à la réparer (m).",
            t.warn,
        ));
    }
    if report.fuel_out {
        lines.push(Line::styled(
            "Un stock de combustible s'est vidé : les trains ont roulé au prix fort (c pour en racheter).",
            t.warn,
        ));
    }
    if report.idle_trains {
        lines.push(Line::styled(
            "Tes trains sont restés à quai : sans départs automatiques, rien ne roule en ton absence.",
            t.warn,
        ));
    }
    if report.event_waiting {
        lines.push(Line::styled("Un événement t'attend.", t.title));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Appuie sur une touche pour reprendre.", t.dim));

    let height = lines.len() as u16 + 3;
    let inner = window(app, buf, area, "Pendant ton absence", 78, height);
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(inner, buf);
}

fn draw_help(app: &App, buf: &mut Buffer, area: Rect) {
    let t = &app.theme;
    let keys = [
        ("d, Entrée", "donner le départ au train sélectionné"),
        ("Espace", "donner le départ à tous les trains prêts"),
        ("Tab", "sélectionner le train suivant"),
        ("a", "dépôt : acheter une locomotive, lire sa fiche"),
        ("r", "bureau d'études : arbre de recherche"),
        (
            "n",
            "réseau : concessions, lignes à construire, ligne courante",
        ),
        ("e", "époques : conditions pour passer à la suivante"),
        ("u", "améliorations de la ligne (rails, caténaire...)"),
        ("c", "combustibles : stocks et cours du jour"),
        ("t", "tarifs et horaires, satisfaction des clientèles"),
        (
            "s",
            "équipages : recrutement, affectation au train sélectionné",
        ),
        (
            "i",
            "formations : programmer une session, inscrire des équipiers",
        ),
        ("g", "services : automatiser réparations, achats, horaires"),
        ("o", "compagnie : nationaliser, bonus permanents"),
        (
            "v",
            "flotte : revente (même en route), remplacement de machine (r)",
        ),
        ("b", "tableau des départs et arrivées"),
        ("m", "remettre la voie à neuf"),
        ("f", "suivre le train sélectionné"),
        ("+ / -, molette", "zoomer et dézoomer"),
        ("Flèches, hjkl, glisser", "déplacer la carte"),
        ("0", "recadrer la ligne courante"),
        ("1 / 2 / 3", "vue gare, vue pays, vue continent"),
        ("p", "pause"),
        ("q, Échap", "quitter (la partie est sauvegardée)"),
    ];
    let mut lines: Vec<Line> = keys
        .iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(format!("{key:<24}"), t.title),
                Span::styled(*label, t.text),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled("Appuie sur une touche pour fermer.", t.dim));
    let inner = window(app, buf, area, "Aide", 78, lines.len() as u16 + 2);
    Paragraph::new(lines).render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_read_naturally() {
        assert_eq!(
            describe_effect(&Effect::FareMult { value: 1.2 }),
            "Tarifs +20 %"
        );
        assert_eq!(
            describe_effect(&Effect::SpeedMult { value: 0.95 }),
            "Vitesse -5 %"
        );
        assert_eq!(
            describe_effect(&Effect::FuelMult { value: 0.85 }),
            "Consommation -15 %"
        );
        assert_eq!(
            describe_effect(&Effect::BrakeAdd { value: 0.25 }),
            "Freinage +0,25 m/s²"
        );
        assert_eq!(
            describe_effect(&Effect::SignalSpeed { value: 320.0 }),
            "Signalisation : 320 km/h autorisés"
        );
        assert_eq!(
            describe_effect(&Effect::LineCapacity { value: 2 }),
            "+2 trains par ligne"
        );
        assert_eq!(
            describe_effect(&Effect::AutoDispatch),
            "Départs automatiques"
        );
        assert_eq!(
            describe_upgrade_effect(&UpgradeEffect::LineSpeed {
                value: 650.0,
                only: vec!["maglev".into()]
            }),
            "Vitesse autorisée : 650 km/h (sustentation magnétique)"
        );
    }

    #[test]
    fn long_lists_scroll_to_keep_the_selection_visible() {
        assert_eq!(scroll_start(0, 5, 10), 0);
        assert_eq!(scroll_start(3, 50, 10), 0);
        assert_eq!(scroll_start(20, 50, 10), 15);
        assert_eq!(scroll_start(49, 50, 10), 40);
        for sel in 0..50 {
            let start = scroll_start(sel, 50, 10);
            assert!((start..start + 10).contains(&sel));
        }
    }
}
