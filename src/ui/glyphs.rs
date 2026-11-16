//! Jeu de caractères : Unicode par défaut, repli en ASCII pur.

use ratatui::symbols::border;

#[derive(Debug, Clone)]
pub struct Glyphs {
    pub ascii: bool,
    pub station: char,
    /// Gare pas encore desservie.
    pub station_idle: char,
    /// Tête de train selon le cap, par huitième de tour depuis l'est (sens horaire à l'écran).
    pub heads: [char; 8],
    pub wagon: char,
    pub smoke: [char; 3],
    pub gauge_full: char,
    pub gauge_empty: char,
    pub sep: &'static str,
    pub arrow: &'static str,
    pub cursor: &'static str,
    pub check: &'static str,
    pub bullet: &'static str,
    pub dash: &'static str,
    pub border: border::Set<'static>,
}

const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

impl Glyphs {
    pub fn new(ascii: bool) -> Glyphs {
        if ascii {
            Glyphs {
                ascii,
                station: 'O',
                station_idle: 'o',
                heads: ['>', '\\', 'v', '/', '<', '\\', '^', '/'],
                wagon: '#',
                smoke: ['.', 'o', 'O'],
                gauge_full: '#',
                gauge_empty: '-',
                sep: "|",
                arrow: "->",
                cursor: ">",
                check: "x",
                bullet: "*",
                dash: "-",
                border: ASCII_BORDER,
            }
        } else {
            Glyphs {
                ascii,
                station: '◉',
                station_idle: '○',
                heads: ['▶', '◢', '▼', '◣', '◀', '◤', '▲', '◥'],
                wagon: '■',
                smoke: ['·', '∘', '○'],
                gauge_full: '█',
                gauge_empty: '░',
                sep: "│",
                arrow: "→",
                cursor: "▸",
                check: "✓",
                bullet: "•",
                dash: "—",
                border: border::DOUBLE,
            }
        }
    }

    /// Caractère de tête pour un vecteur de déplacement à l'écran (y vers le bas).
    pub fn head(&self, dx: f64, dy: f64) -> char {
        let octant = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() as i32;
        self.heads[octant.rem_euclid(8) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_follows_heading() {
        let g = Glyphs::new(true);
        assert_eq!(g.head(1.0, 0.0), '>');
        assert_eq!(g.head(-1.0, 0.0), '<');
        assert_eq!(g.head(0.0, 1.0), 'v');
        assert_eq!(g.head(0.0, -1.0), '^');
        assert_eq!(g.head(-1.0, -1.0), '\\');
    }

    #[test]
    fn ascii_set_is_pure_ascii() {
        let g = Glyphs::new(true);
        let mut all: String = g.heads.iter().chain(&g.smoke).collect();
        all.extend([
            g.station,
            g.station_idle,
            g.wagon,
            g.gauge_full,
            g.gauge_empty,
        ]);
        for s in [g.sep, g.arrow, g.cursor, g.check, g.bullet, g.dash] {
            all.push_str(s);
        }
        assert!(all.is_ascii(), "{all}");
    }
}
