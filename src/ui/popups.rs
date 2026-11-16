//! Fenêtres superposées à la carte : dépôt, recherche, aide.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph, Widget, Wrap};

use crate::data::{Branch, Effect};

use super::app::{App, Popup};
use super::format;

pub fn draw(app: &App, buf: &mut Buffer, area: Rect) {
    match app.popup {
        Popup::None => {}
        Popup::Shop { sel } => draw_shop(app, buf, area, sel),
        Popup::Research { sel } => draw_research(app, buf, area, sel),
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

/// Dessine le cadre et renvoie les zones liste, détail et pied.
fn frame(
    app: &App,
    buf: &mut Buffer,
    area: Rect,
    title: &str,
    rows: u16,
    height: u16,
) -> [Rect; 3] {
    let rect = centered(area, 84, height);
    let block = Block::bordered()
        .border_set(app.glyphs.border)
        .border_style(app.theme.border)
        .padding(Padding::horizontal(1))
        .title(Span::styled(format!(" {title} "), app.theme.title));
    let inner = block.inner(rect);
    Clear.render(rect, buf);
    block.render(rect, buf);
    Layout::vertical([
        Constraint::Length(rows + 1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner)
}

fn footer(app: &App, buf: &mut Buffer, area: Rect, hint: &str) {
    let line = match &app.popup_msg {
        Some(msg) => Line::styled(msg.clone(), app.theme.warn),
        None => Line::styled(hint.to_string(), app.theme.dim),
    };
    Paragraph::new(line).render(area, buf);
}

fn draw_shop(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let locos = &app.data.locomotives;
    let [list, detail, foot] = frame(
        app,
        buf,
        area,
        "Dépôt : acheter une locomotive",
        locos.len() as u16,
        18,
    );

    let line_id = &app.data.balance.start_line;
    let full = app
        .data
        .line(line_id)
        .is_some_and(|l| app.world.trains_on(line_id) >= l.max_trains as usize);
    let rows: Vec<Line> = locos
        .iter()
        .enumerate()
        .map(|(i, loco)| {
            let missing = loco
                .requires
                .iter()
                .find(|r| !app.world.research_done.contains(*r));
            let (status, status_style) = match missing {
                Some(_) => ("verrouillée".to_string(), t.warn),
                None if full => ("ligne complète".to_string(), t.warn),
                None if app.world.money < loco.cost => ("trop chère".to_string(), t.dim),
                None => ("disponible".to_string(), t.good),
            };
            let text = format!(
                "{} {:<16} {}  {:>3} km/h  {:>3} voy.  {:>8}  ",
                if i == sel { g.cursor } else { " " },
                loco.name,
                loco.year,
                loco.top_speed_kmh.round(),
                loco.capacity,
                format::money(loco.cost),
            );
            let base = if i == sel {
                t.text.patch(t.selected)
            } else {
                t.text
            };
            Line::from(vec![
                Span::styled(text, base),
                Span::styled(format!(" {status}"), status_style),
            ])
        })
        .collect();
    Paragraph::new(rows).render(list, buf);

    if let Some(loco) = locos.get(sel) {
        let mut lines = vec![
            Line::styled(
                format!("{} {} {}, {}", loco.name, g.dash, loco.builder, loco.year),
                t.title,
            ),
            Line::styled(
                format!(
                    "Disposition {}  {}  {} t  {}  entretien {}/h  {}  usure de la voie x{}",
                    loco.wheels,
                    g.bullet,
                    format::dec1(loco.weight_t),
                    g.bullet,
                    format::money(loco.upkeep_per_h),
                    g.bullet,
                    format::dec1(loco.track_wear),
                ),
                t.dim,
            ),
            Line::raw(""),
            Line::styled(loco.history.fr.clone(), t.text),
        ];
        let missing: Vec<&str> = loco
            .requires
            .iter()
            .filter(|r| !app.world.research_done.contains(*r))
            .map(|id| {
                app.data
                    .research(id)
                    .map_or(id.as_str(), |r| r.name.fr.as_str())
            })
            .collect();
        if !missing.is_empty() {
            lines.insert(
                2,
                Line::styled(
                    format!("Requiert la recherche : {}", missing.join(", ")),
                    t.warn,
                ),
            );
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : acheter    haut/bas : choisir    r : recherche    Échap : fermer",
    );
}

fn branch_name(branch: Branch) -> &'static str {
    match branch {
        Branch::Traction => "Traction",
        Branch::RunningGear => "Roulement",
        Branch::Braking => "Freinage",
        Branch::Signalling => "Signalisation",
        Branch::Comfort => "Confort",
    }
}

/// Résumé lisible d'un effet : `Tarifs +20 %`.
pub fn describe_effect(effect: &Effect) -> String {
    let pct = |label: &str, value: f64| {
        let p = ((value - 1.0) * 100.0).round() as i64;
        format!("{label} {}{p} %", if p >= 0 { "+" } else { "" })
    };
    match effect {
        Effect::SpeedMult { value } => pct("Vitesse", *value),
        Effect::CapacityMult { value } => pct("Capacité", *value),
        Effect::FareMult { value } => pct("Tarifs", *value),
        Effect::DemandMult { value } => pct("Demande", *value),
        Effect::TrackWearMult { value } => pct("Usure des voies", *value),
        Effect::AutoDispatch => "Départs automatiques".to_string(),
    }
}

fn draw_research(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let items = &app.data.research;
    let [list, detail, foot] = frame(
        app,
        buf,
        area,
        "Bureau d'études : recherche",
        items.len() as u16,
        18,
    );
    let time_scale = app.data.balance.time_scale;

    let rows: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let active = app.world.research_active.as_ref().filter(|a| a.id == r.id);
            let locked = r
                .requires
                .iter()
                .any(|req| !app.world.research_done.contains(req));
            let (state, state_style) = if app.world.research_done.contains(&r.id) {
                (format!("{} terminée", g.check), t.good)
            } else if let Some(a) = active {
                (
                    format!(
                        "en cours {}",
                        format::percent(a.progress_s / (r.duration_h * 3600.0))
                    ),
                    t.title,
                )
            } else if locked {
                ("prérequis manquant".to_string(), t.warn)
            } else if app.world.money < r.cost {
                ("trop chère".to_string(), t.dim)
            } else {
                ("disponible".to_string(), t.good)
            };
            let text = format!(
                "{} {:<26} {:<13} {:>8}  {:>8}  ",
                if i == sel { g.cursor } else { " " },
                r.name.fr,
                branch_name(r.branch),
                format::money(r.cost),
                format::duration(r.duration_h * 3600.0 / time_scale),
            );
            let base = if i == sel {
                t.text.patch(t.selected)
            } else {
                t.text
            };
            Line::from(vec![
                Span::styled(text, base),
                Span::styled(format!(" {state}"), state_style),
            ])
        })
        .collect();
    Paragraph::new(rows).render(list, buf);

    if let Some(r) = items.get(sel) {
        let mut lines = vec![
            Line::styled(format!("{} {} vers {}", r.name.fr, g.dash, r.year), t.title),
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
        let unlocks: Vec<&str> = app
            .data
            .unlocked_by(&r.id)
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        if !unlocks.is_empty() {
            lines.push(Line::styled(
                format!("Débloque : {}", unlocks.join(", ")),
                t.good,
            ));
        }
        let requires: Vec<&str> = r
            .requires
            .iter()
            .filter_map(|id| app.data.research(id))
            .map(|x| x.name.fr.as_str())
            .collect();
        if !requires.is_empty() {
            lines.push(Line::styled(
                format!("Prérequis : {}", requires.join(", ")),
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
        "Entrée : lancer    haut/bas : choisir    a : dépôt    Échap : fermer",
    );
}

fn draw_help(app: &App, buf: &mut Buffer, area: Rect) {
    let t = &app.theme;
    let keys = [
        ("d, Entrée", "donner le départ au train sélectionné"),
        ("Espace", "donner le départ à tous les trains à quai"),
        ("Tab", "sélectionner le train suivant"),
        ("a", "dépôt : acheter une locomotive"),
        ("r", "bureau d'études : lancer une recherche"),
        ("m", "remettre la voie à neuf"),
        ("f", "suivre le train sélectionné"),
        ("+ / -, molette", "zoomer et dézoomer"),
        ("Flèches, hjkl, glisser", "déplacer la carte"),
        ("0", "recadrer la carte"),
        ("p", "pause"),
        ("q, Échap", "quitter (la partie est sauvegardée)"),
    ];
    let rect = centered(area, 66, keys.len() as u16 + 6);
    let block = Block::bordered()
        .border_set(app.glyphs.border)
        .border_style(t.border)
        .padding(Padding::horizontal(1))
        .title(Span::styled(" Aide ", t.title));
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
    lines.push(Line::styled(
        "Transporte des voyageurs, achète des machines, lance des recherches.",
        t.dim,
    ));
    lines.push(Line::styled("Appuie sur une touche pour fermer.", t.dim));
    Clear.render(rect, buf);
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .render(rect, buf);
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
            describe_effect(&Effect::AutoDispatch),
            "Départs automatiques"
        );
    }
}
