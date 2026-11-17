//! Tracé vectoriel de la carte : points braille (2×4 par cellule) en Unicode,
//! caractères `- | / \` choisis selon la pente en ASCII. Le fond de carte est
//! toujours dessiné en caractères ASCII, quel que soit le mode.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// Bit braille pour chaque (ligne, colonne) de sous-pixel dans une cellule.
const BRAILLE: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

#[derive(Clone, Copy, Default)]
struct Cell {
    bits: u8,
    ch: Option<char>,
    style: Style,
    prio: u8,
    /// Caractère affiché tel quel, même en mode Unicode (fond de carte).
    text: bool,
}

/// Rendu d'un segment en mode ASCII.
#[derive(Debug, Clone, Copy)]
pub enum Stroke {
    /// `- | / \` selon la pente, avec le caractère donné pour l'horizontale.
    Slope(char),
    /// Toujours le même caractère (rivières…).
    Fixed(char),
}

pub struct MapCanvas {
    w: usize,
    h: usize,
    ascii: bool,
    cells: Vec<Cell>,
}

impl MapCanvas {
    pub fn new(width: u16, height: u16, ascii: bool) -> MapCanvas {
        let (w, h) = (width as usize, height as usize);
        MapCanvas {
            w,
            h,
            ascii,
            cells: vec![Cell::default(); w * h],
        }
    }

    /// Taille de la surface en sous-pixels.
    pub fn px_size(&self) -> (f64, f64) {
        (self.w as f64 * 2.0, self.h as f64 * 4.0)
    }

    fn plot_as(&mut self, x: f64, y: f64, ch: char, style: Style, prio: u8, text: bool) {
        if x < 0.0 || y < 0.0 {
            return;
        }
        let (px, py) = (x as usize, y as usize);
        let (cx, cy) = (px / 2, py / 4);
        if cx >= self.w || cy >= self.h {
            return;
        }
        let cell = &mut self.cells[cy * self.w + cx];
        // Une cellule n'a qu'une couleur : le tracé le plus prioritaire la garde.
        if prio < cell.prio {
            return;
        }
        if prio > cell.prio {
            cell.bits = 0;
        }
        cell.prio = prio;
        cell.style = style;
        cell.ch = Some(ch);
        cell.text = text;
        cell.bits |= BRAILLE[py % 4][px % 2];
    }

    #[cfg(test)]
    pub fn point(&mut self, x: f64, y: f64, style: Style, prio: u8) {
        self.plot_as(x, y, '.', style, prio, false);
    }

    /// Remplit une cellule entière d'un caractère (fond de carte).
    pub fn glyph(&mut self, cx: usize, cy: usize, ch: char, style: Style, prio: u8) {
        if cx < self.w && cy < self.h {
            self.plot_as((cx * 2) as f64, (cy * 4) as f64, ch, style, prio, true);
        }
    }

    /// Segment entre deux points en sous-pixels. `stroke` décide du caractère
    /// employé en mode ASCII.
    pub fn line(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        style: Style,
        prio: u8,
        stroke: Stroke,
    ) {
        self.segment(from, to, style, prio, stroke, false);
    }

    /// Segment toujours dessiné en caractères ASCII, même en mode Unicode
    /// (côtes, frontières et fleuves du fond de carte).
    pub fn text_line(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        style: Style,
        prio: u8,
        stroke: Stroke,
    ) {
        self.segment(from, to, style, prio, stroke, true);
    }

    fn segment(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        style: Style,
        prio: u8,
        stroke: Stroke,
        text: bool,
    ) {
        let ch = match stroke {
            Stroke::Slope(horizontal) => ascii_char(to.0 - from.0, to.1 - from.1, horizontal),
            Stroke::Fixed(ch) => ch,
        };
        let (w, h) = self.px_size();
        let Some((a, b)) = clip(from, to, w, h) else {
            return;
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let steps = dx.abs().max(dy.abs()).ceil().max(1.0);
        for i in 0..=steps as usize {
            let t = i as f64 / steps;
            self.plot_as(a.0 + dx * t, a.1 + dy * t, ch, style, prio, text);
        }
    }

    pub fn render(&self, buf: &mut Buffer, area: Rect) {
        for cy in 0..self.h.min(area.height as usize) {
            for cx in 0..self.w.min(area.width as usize) {
                let cell = &self.cells[cy * self.w + cx];
                let ch = match (self.ascii, cell.ch) {
                    (_, None) => continue,
                    (true, Some(ch)) => ch,
                    (false, Some(ch)) if cell.text => ch,
                    (false, Some(_)) => char::from_u32(0x2800 + cell.bits as u32).unwrap_or(' '),
                };
                if let Some(target) = buf.cell_mut((area.x + cx as u16, area.y + cy as u16)) {
                    target.set_char(ch).set_style(cell.style);
                }
            }
        }
    }
}

/// Caractère ASCII qui évoque le mieux la pente (en sous-pixels, y vers le bas).
fn ascii_char(dx: f64, dy: f64, horizontal: char) -> char {
    const TAN_22_5: f64 = 0.4142;
    let (ax, ay) = (dx.abs(), dy.abs());
    if ay <= ax * TAN_22_5 {
        horizontal
    } else if ax <= ay * TAN_22_5 {
        '|'
    } else if (dx > 0.0) == (dy > 0.0) {
        '\\'
    } else {
        '/'
    }
}

/// Découpe un segment au rectangle `[0, w] × [0, h]` (Liang-Barsky).
fn clip(a: (f64, f64), b: (f64, f64), w: f64, h: f64) -> Option<((f64, f64), (f64, f64))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.0), (dx, w - a.0), (-dy, a.1), (dy, h - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    if t0 > t1 || !t0.is_finite() || !t1.is_finite() {
        return None;
    }
    Some((
        (a.0 + dx * t0, a.1 + dy * t0),
        (a.0 + dx * t1, a.1 + dy * t1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(canvas: &MapCanvas, w: u16, h: u16) -> Vec<String> {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        canvas.render(&mut buf, area);
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    #[test]
    fn braille_horizontal_line_fills_the_top_row_of_dots() {
        let mut c = MapCanvas::new(4, 2, false);
        c.line((0.0, 0.0), (7.0, 0.0), Style::new(), 1, Stroke::Slope('-'));
        assert_eq!(rendered(&c, 4, 2), vec!["⠉⠉⠉⠉", "    "]);
    }

    #[test]
    fn braille_vertical_line_fills_a_column() {
        let mut c = MapCanvas::new(2, 2, false);
        c.line((0.0, 0.0), (0.0, 7.0), Style::new(), 1, Stroke::Slope('-'));
        assert_eq!(rendered(&c, 2, 2), vec!["⡇ ", "⡇ "]);
    }

    #[test]
    fn ascii_lines_pick_a_character_from_the_slope() {
        assert_eq!(ascii_char(10.0, 0.0, '='), '=');
        assert_eq!(ascii_char(10.0, 2.0, '-'), '-');
        assert_eq!(ascii_char(0.0, -5.0, '-'), '|');
        assert_eq!(ascii_char(5.0, 5.0, '-'), '\\');
        assert_eq!(ascii_char(-5.0, 5.0, '-'), '/');

        let mut c = MapCanvas::new(4, 1, true);
        c.line((0.0, 1.0), (7.0, 1.0), Style::new(), 1, Stroke::Slope('='));
        assert_eq!(rendered(&c, 4, 1), vec!["===="]);

        let mut c = MapCanvas::new(2, 2, true);
        c.line((0.0, 0.0), (3.0, 7.0), Style::new(), 1, Stroke::Fixed('~'));
        assert_eq!(rendered(&c, 2, 2), vec!["~ ", "~~"]);
    }

    #[test]
    fn higher_priority_owns_the_cell() {
        let mut c = MapCanvas::new(1, 1, true);
        c.point(0.0, 0.0, Style::new(), 2);
        c.line((0.0, 0.0), (1.0, 0.0), Style::new(), 1, Stroke::Slope('-'));
        assert_eq!(rendered(&c, 1, 1), vec!["."]);
        c.line((0.0, 0.0), (1.0, 0.0), Style::new(), 3, Stroke::Slope('-'));
        assert_eq!(rendered(&c, 1, 1), vec!["-"]);
    }

    #[test]
    fn off_screen_and_huge_segments_are_clipped() {
        let mut c = MapCanvas::new(4, 1, true);
        c.line(
            (-1e12, 1.0),
            (1e12, 1.0),
            Style::new(),
            1,
            Stroke::Slope('-'),
        );
        assert_eq!(rendered(&c, 4, 1), vec!["----"]);
        let mut c = MapCanvas::new(4, 1, true);
        c.line(
            (-50.0, -50.0),
            (-10.0, -20.0),
            Style::new(),
            1,
            Stroke::Slope('-'),
        );
        c.point(-3.0, 2.0, Style::new(), 1);
        c.point(100.0, 2.0, Style::new(), 1);
        assert_eq!(rendered(&c, 4, 1), vec!["    "]);
    }
}
