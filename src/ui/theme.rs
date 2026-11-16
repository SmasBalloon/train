//! Palettes : couleur, daltonien (Okabe-Ito), et sans couleur.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Color,
    Colorblind,
    NoColor,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub hud: Style,
    pub hud_title: Style,
    pub hud_key: Style,
    pub text: Style,
    pub dim: Style,
    pub title: Style,
    pub grid: Style,
    pub river: Style,
    pub track: Style,
    pub track_worn: Style,
    /// Ligne sur laquelle portent les commandes.
    pub track_current: Style,
    /// Contour d'un pays où la compagnie est installée.
    pub border_owned: Style,
    pub station: Style,
    pub money: Style,
    pub good: Style,
    pub warn: Style,
    pub selected: Style,
    pub smoke: Style,
    pub border: Style,
    pub trains: Vec<Style>,
    /// Tableau des départs : fond, titres, palettes.
    pub board: Style,
    pub board_title: Style,
    pub board_header: Style,
    pub flap: Style,
    pub flap_late: Style,
}

fn fg(index: u8) -> Style {
    Style::new().fg(Color::Indexed(index))
}

impl Theme {
    pub fn new(mode: ColorMode) -> Theme {
        match mode {
            ColorMode::Color => Theme {
                hud: Style::new().fg(Color::Indexed(252)).bg(Color::Indexed(236)),
                hud_title: Style::new()
                    .fg(Color::Indexed(232))
                    .bg(Color::Indexed(214))
                    .add_modifier(Modifier::BOLD),
                hud_key: Style::new()
                    .fg(Color::Indexed(214))
                    .bg(Color::Indexed(236))
                    .add_modifier(Modifier::BOLD),
                text: fg(252),
                dim: fg(244),
                title: fg(214).add_modifier(Modifier::BOLD),
                grid: fg(238),
                river: fg(31),
                track: fg(250),
                track_worn: fg(167),
                track_current: fg(222).add_modifier(Modifier::BOLD),
                border_owned: fg(244),
                station: fg(231).add_modifier(Modifier::BOLD),
                money: fg(220).add_modifier(Modifier::BOLD),
                good: fg(114),
                warn: fg(203),
                selected: Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                smoke: fg(246),
                border: fg(214),
                trains: [209, 80, 150, 176, 222, 111]
                    .into_iter()
                    .map(|c| fg(c).add_modifier(Modifier::BOLD))
                    .collect(),
                board: Style::new().fg(Color::Indexed(250)).bg(Color::Indexed(16)),
                board_title: Style::new()
                    .fg(Color::Indexed(231))
                    .bg(Color::Indexed(16))
                    .add_modifier(Modifier::BOLD),
                board_header: Style::new().fg(Color::Indexed(244)).bg(Color::Indexed(16)),
                flap: Style::new()
                    .fg(Color::Indexed(220))
                    .bg(Color::Indexed(234))
                    .add_modifier(Modifier::BOLD),
                flap_late: Style::new()
                    .fg(Color::Indexed(203))
                    .bg(Color::Indexed(234))
                    .add_modifier(Modifier::BOLD),
            },
            // Okabe-Ito : distinguable pour les principales formes de daltonisme.
            ColorMode::Colorblind => Theme {
                hud: Style::new().fg(Color::Indexed(255)).bg(Color::Indexed(236)),
                hud_title: Style::new()
                    .fg(Color::Indexed(232))
                    .bg(Color::Indexed(227))
                    .add_modifier(Modifier::BOLD),
                hud_key: Style::new()
                    .fg(Color::Indexed(227))
                    .bg(Color::Indexed(236))
                    .add_modifier(Modifier::BOLD),
                text: fg(255),
                dim: fg(246),
                title: fg(227).add_modifier(Modifier::BOLD),
                grid: fg(240),
                river: fg(32),
                track: fg(253),
                track_worn: fg(202).add_modifier(Modifier::BOLD),
                track_current: fg(227).add_modifier(Modifier::BOLD),
                border_owned: fg(246),
                station: fg(231).add_modifier(Modifier::BOLD),
                money: fg(227).add_modifier(Modifier::BOLD),
                good: fg(81),
                warn: fg(202).add_modifier(Modifier::BOLD),
                selected: Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                smoke: fg(248),
                border: fg(227),
                trains: [214, 81, 36, 175, 227, 32]
                    .into_iter()
                    .map(|c| fg(c).add_modifier(Modifier::BOLD))
                    .collect(),
                board: Style::new().fg(Color::Indexed(250)).bg(Color::Indexed(16)),
                board_title: Style::new()
                    .fg(Color::Indexed(231))
                    .bg(Color::Indexed(16))
                    .add_modifier(Modifier::BOLD),
                board_header: Style::new().fg(Color::Indexed(244)).bg(Color::Indexed(16)),
                flap: Style::new()
                    .fg(Color::Indexed(227))
                    .bg(Color::Indexed(234))
                    .add_modifier(Modifier::BOLD),
                flap_late: Style::new()
                    .fg(Color::Indexed(202))
                    .bg(Color::Indexed(234))
                    .add_modifier(Modifier::BOLD),
            },
            ColorMode::NoColor => {
                let plain = Style::new();
                let bold = plain.add_modifier(Modifier::BOLD);
                let dim = plain.add_modifier(Modifier::DIM);
                let reversed = plain.add_modifier(Modifier::REVERSED);
                Theme {
                    hud: reversed,
                    hud_title: reversed.add_modifier(Modifier::BOLD),
                    hud_key: reversed.add_modifier(Modifier::BOLD),
                    text: plain,
                    dim,
                    title: bold,
                    grid: dim,
                    river: dim,
                    track: plain,
                    track_worn: bold,
                    track_current: bold,
                    border_owned: plain,
                    station: bold,
                    money: bold,
                    good: plain,
                    warn: bold.add_modifier(Modifier::UNDERLINED),
                    selected: reversed.add_modifier(Modifier::BOLD),
                    smoke: dim,
                    border: plain,
                    trains: vec![bold],
                    board: plain,
                    board_title: bold,
                    board_header: dim,
                    flap: bold,
                    flap_late: bold.add_modifier(Modifier::UNDERLINED),
                }
            }
        }
    }

    pub fn train(&self, index: usize) -> Style {
        self.trains[index % self.trains.len()]
    }
}
