//! Arbre de recherche : les époques en colonnes, les branches en bandes, et
//! un trait de chaque prérequis vers la recherche qu'il rend possible.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout as Split, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::data::{Branch, GameData};

use super::app::App;
use super::format;
use super::map::put_str;
use super::popups::{branch_name, footer, frame_at, research_card};

/// Largeur d'une case de recherche et espace entre deux colonnes.
const NODE_W: usize = 26;
const GAP: usize = 6;
const PITCH: usize = NODE_W + GAP;
/// Marge de gauche réservée aux noms des branches.
const LANE_W: u16 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    /// Rang de la recherche dans les données.
    pub research: usize,
    pub col: usize,
    pub row: usize,
}

/// Disposition de l'arbre, indépendante de l'écran.
#[derive(Debug, Clone)]
pub struct TreeLayout {
    pub nodes: Vec<Node>,
    pub cols: usize,
    pub rows: usize,
    /// Pour chaque époque : première colonne et nombre de colonnes.
    pub eras: Vec<(usize, usize)>,
    /// Pour chaque branche : première ligne et nombre de lignes.
    pub lanes: Vec<(Branch, usize, usize)>,
}

impl TreeLayout {
    /// Place chaque recherche : sa colonne vient de son époque et de ses
    /// prérequis de la même époque, sa ligne de sa branche.
    pub fn new(data: &GameData) -> TreeLayout {
        let n = data.research.len();
        let index = |id: &str| data.research.iter().position(|r| r.id == id);
        let era_of = |i: usize| data.era_index(&data.research[i].era).unwrap_or(0);

        // Colonnes : dans une époque, une recherche se place après ses prérequis de la même époque.
        let mut col = vec![0usize; n];
        let mut eras = Vec::new();
        let mut base = 0;
        for era in 0..data.eras.len() {
            let members: Vec<usize> = (0..n).filter(|&i| era_of(i) == era).collect();
            let mut depth = vec![0usize; n];
            // Les prérequis sont sans cycle utile : quelques passes suffisent à stabiliser.
            for _ in 0..members.len() {
                for &i in &members {
                    for req in data.research[i].requires.iter().filter_map(|id| index(id)) {
                        if era_of(req) == era && req != i {
                            depth[i] = depth[i].max((depth[req] + 1).min(members.len()));
                        }
                    }
                }
            }
            let width = members.iter().map(|&i| depth[i] + 1).max().unwrap_or(1);
            for &i in &members {
                col[i] = base + depth[i];
            }
            eras.push((base, width));
            base += width;
        }
        let cols = base.max(1);

        // Lignes : une bande par branche, assez haute pour sa colonne la plus chargée.
        let mut row = vec![0usize; n];
        let mut lanes = Vec::new();
        let mut first = 0;
        for branch in Branch::ALL {
            let mut height = 1;
            let mut taken: Vec<Vec<bool>> = vec![Vec::new(); cols];
            for i in (0..n).filter(|&i| data.research[i].branch == branch) {
                // On reste si possible sur la ligne d'un prérequis de la même branche,
                // pour que le trait qui les relie soit droit.
                let wanted = data.research[i]
                    .requires
                    .iter()
                    .filter_map(|id| index(id))
                    .find(|&req| data.research[req].branch == branch && col[req] < col[i])
                    .map(|req| row[req] - first);
                let slots = &mut taken[col[i]];
                let free = |slots: &Vec<bool>, s: usize| slots.get(s).is_none_or(|used| !used);
                let slot = match wanted {
                    Some(s) if free(slots, s) => s,
                    _ => (0..).find(|&s| free(slots, s)).unwrap_or(0),
                };
                if slots.len() <= slot {
                    slots.resize(slot + 1, false);
                }
                slots[slot] = true;
                row[i] = first + slot;
                height = height.max(slot + 1);
            }
            lanes.push((branch, first, height));
            first += height;
        }

        TreeLayout {
            nodes: (0..n)
                .map(|i| Node {
                    research: i,
                    col: col[i],
                    row: row[i],
                })
                .collect(),
            cols,
            rows: first.max(1),
            eras,
            lanes,
        }
    }

    pub fn node(&self, research: usize) -> Option<Node> {
        self.nodes
            .iter()
            .copied()
            .find(|node| node.research == research)
    }

    /// Recherche voisine dans une direction, la plus proche à l'écran.
    pub fn neighbour(&self, research: usize, direction: Move) -> Option<usize> {
        let from = self.node(research)?;
        let (col, row) = (from.col as i64, from.row as i64);
        self.nodes
            .iter()
            .filter_map(|node| {
                let (dc, dr) = (node.col as i64 - col, node.row as i64 - row);
                // Coût : d'abord la distance dans la direction demandée, puis l'écart latéral.
                let cost = match direction {
                    Move::Right if dc > 0 => dc * 1000 + dr.abs(),
                    Move::Left if dc < 0 => -dc * 1000 + dr.abs(),
                    Move::Down if dr > 0 => dc.abs() * 1000 + dr,
                    Move::Up if dr < 0 => dc.abs() * 1000 - dr,
                    _ => return None,
                };
                Some((cost, node.research))
            })
            .min()
            .map(|(_, research)| research)
    }
}

const N: u8 = 1;
const E: u8 = 2;
const S: u8 = 4;
const W: u8 = 8;

/// Caractère de tracé pour un croisement de directions.
fn line_char(mask: u8, ascii: bool) -> char {
    if ascii {
        return match mask {
            0 => ' ',
            m if m & (N | S) == 0 => '-',
            m if m & (E | W) == 0 => '|',
            _ => '+',
        };
    }
    match mask {
        0 => ' ',
        2 | 8 | 10 => '─',
        1 | 4 | 5 => '│',
        6 => '┌',
        12 => '┐',
        3 => '└',
        9 => '┘',
        7 => '├',
        13 => '┤',
        14 => '┬',
        11 => '┴',
        _ => '┼',
    }
}

/// Grille de traits : pour chaque cellule, les directions reliées et leur style.
struct Wires {
    w: usize,
    h: usize,
    mask: Vec<u8>,
    style: Vec<Style>,
    prio: Vec<u8>,
}

impl Wires {
    fn new(w: usize, h: usize) -> Wires {
        Wires {
            w,
            h,
            mask: vec![0; w * h],
            style: vec![Style::new(); w * h],
            prio: vec![0; w * h],
        }
    }

    fn mark(&mut self, x: usize, y: usize, bits: u8, style: Style, prio: u8) {
        if x >= self.w || y >= self.h {
            return;
        }
        let i = y * self.w + x;
        self.mask[i] |= bits;
        if prio >= self.prio[i] {
            self.prio[i] = prio;
            self.style[i] = style;
        }
    }

    /// Relie une suite de points par des segments horizontaux ou verticaux.
    fn path(&mut self, points: &[(usize, usize)], style: Style, prio: u8) {
        for pair in points.windows(2) {
            let (mut x, mut y) = pair[0];
            let (tx, ty) = pair[1];
            while (x, y) != (tx, ty) {
                let (nx, ny, out, back) = if x < tx {
                    (x + 1, y, E, W)
                } else if x > tx {
                    (x - 1, y, W, E)
                } else if y < ty {
                    (x, y + 1, S, N)
                } else {
                    (x, y - 1, N, S)
                };
                self.mark(x, y, out, style, prio);
                self.mark(nx, ny, back, style, prio);
                (x, y) = (nx, ny);
            }
        }
    }
}

/// Géométrie de l'arbre à l'écran, selon son orientation.
///
/// À l'horizontale, les époques vont de gauche à droite et les branches sont
/// des bandes ; à la verticale, les époques descendent et les branches sont
/// des colonnes.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    vertical: bool,
}

/// Lignes d'écran par rang d'époque à la verticale : la case, puis deux lignes de traits.
const V_PITCH: usize = 3;
/// Largeur d'un emplacement à la verticale : la case et deux colonnes de passage.
const V_SLOT: usize = NODE_W + 2;

impl Geometry {
    /// Coin haut gauche de la case d'une recherche.
    fn origin(self, node: Node) -> (usize, usize) {
        if self.vertical {
            (node.row * V_SLOT, node.col * V_PITCH)
        } else {
            (node.col * PITCH, node.row * 2)
        }
    }

    fn size(self, layout: &TreeLayout) -> (usize, usize) {
        if self.vertical {
            (layout.rows * V_SLOT, layout.cols * V_PITCH)
        } else {
            (layout.cols * PITCH, layout.rows * 2)
        }
    }

    /// Trajet du trait qui va d'un prérequis à la recherche qu'il débloque ;
    /// le dernier point est la cellule de la pointe de flèche.
    fn route(self, from: Node, to: Node) -> Vec<(usize, usize)> {
        let (fx, fy) = self.origin(from);
        let (tx, ty) = self.origin(to);
        if self.vertical {
            let exit = (fx + NODE_W / 2, fy + 1);
            let entry = (tx + NODE_W / 2, ty - 1);
            if to.col == from.col + 1 {
                // Rangs voisins : on glisse sur la ligne de traits, puis on descend dans la case.
                vec![exit, (entry.0, exit.1), entry]
            } else {
                // Plus loin : on descend par le couloir à droite de la case, sans en traverser aucune.
                let corridor = fx + NODE_W;
                vec![
                    exit,
                    (corridor, exit.1),
                    (corridor, entry.1 - 1),
                    (entry.0, entry.1 - 1),
                    entry,
                ]
            }
        } else {
            let exit = (fx + NODE_W, fy);
            let channel = tx - 2;
            let entry = (tx - 1, ty);
            if to.col == from.col + 1 {
                // Colonnes voisines : tout droit, puis on monte ou descend juste avant l'arrivée.
                vec![exit, (channel, exit.1), (channel, entry.1), entry]
            } else {
                // Plus loin : on passe par l'interligne, pour ne traverser aucune case.
                let gutter = exit.1 + 1;
                vec![
                    exit,
                    (exit.0, gutter),
                    (channel, gutter),
                    (channel, entry.1),
                    entry,
                ]
            }
        }
    }

    /// Direction par laquelle le trait quitte la case de départ.
    fn exit_side(self) -> u8 {
        if self.vertical { N } else { W }
    }

    fn arrow(self, ascii: bool) -> &'static str {
        match (self.vertical, ascii) {
            (true, false) => "▼",
            (true, true) => "v",
            (false, false) => "▶",
            (false, true) => ">",
        }
    }

    /// Traduit une flèche du clavier en déplacement dans l'arbre.
    fn translate(self, direction: Move) -> Move {
        if !self.vertical {
            return direction;
        }
        match direction {
            Move::Down => Move::Right,
            Move::Up => Move::Left,
            Move::Right => Move::Down,
            Move::Left => Move::Up,
        }
    }
}

/// Recherche atteinte par une flèche du clavier, selon l'orientation de l'arbre.
pub fn step(
    layout: &TreeLayout,
    research: usize,
    direction: Move,
    vertical: bool,
) -> Option<usize> {
    layout.neighbour(research, Geometry { vertical }.translate(direction))
}

pub fn draw(app: &App, buf: &mut Buffer, area: Rect, sel: usize) {
    let t = &app.theme;
    let g = &app.glyphs;
    let data = &app.data;
    let layout = TreeLayout::new(data);
    let geo = Geometry {
        vertical: app.tree_vertical,
    };
    // L'arbre occupe tout l'écran, entre le bandeau du haut et la barre des touches.
    let rect = Rect {
        y: area.y + 1.min(area.height),
        height: area.height.saturating_sub(2),
        ..area
    };
    if rect.width < 4 || rect.height < 4 {
        return;
    }
    let inner = frame_at(app, buf, rect, "Bureau d'études : arbre de recherche");
    let [header, body, status, detail, foot] = Split::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(9.min(inner.height / 3)),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Ce que fait le bureau d'études en ce moment.
    let now = match app.research_progress() {
        Some((research, progress, left_s)) => Line::styled(
            format!(
                "{} En cours : {} {} {} {} encore {}",
                g.cursor,
                research.name.fr,
                format::gauge(progress, 20, g.gauge_full, g.gauge_empty),
                format::percent(progress),
                g.bullet,
                format::duration(left_s)
            ),
            t.title,
        ),
        None => Line::styled(
            "Aucune recherche en cours : choisis-en une et appuie sur Entrée.",
            t.dim,
        ),
    };
    Paragraph::new(now).render(status, buf);
    let margin_w = LANE_W.min(body.width / 3);
    let view = Rect {
        x: body.x + margin_w,
        width: body.width - margin_w,
        ..body
    };
    if view.width == 0 || view.height == 0 {
        return;
    }

    // Décalage de la fenêtre : la recherche choisie reste visible, à peu près centrée.
    let (total_w, total_h) = geo.size(&layout);
    let scroll = |pos: usize, size: usize, total: usize, span: usize| {
        if total <= size {
            0
        } else {
            (pos + span / 2).saturating_sub(size / 2).min(total - size)
        }
    };
    let (ox, oy) = match layout.node(sel) {
        Some(node) => {
            let (x, y) = geo.origin(node);
            (
                scroll(x, view.width as usize, total_w, NODE_W),
                scroll(y, view.height as usize, total_h, 1),
            )
        }
        None => (0, 0),
    };
    let to_screen = |x: usize, y: usize| {
        (
            view.x as i32 + x as i32 - ox as i32,
            view.y as i32 + y as i32 - oy as i32,
        )
    };

    // Traits : d'abord tous, discrets ; puis ceux de la recherche choisie, en couleur.
    let done = |i: usize| app.world.research_done.contains(&data.research[i].id);
    let mut wires = Wires::new(total_w, total_h);
    let mut related: Vec<(usize, Style)> = Vec::new();
    let mut arrows: Vec<((usize, usize), u8)> = Vec::new();
    for pass in 0..2 {
        for node in &layout.nodes {
            let research = &data.research[node.research];
            for req in research
                .requires
                .iter()
                .filter_map(|id| data.research.iter().position(|r| &r.id == id))
            {
                let Some(from) = layout.node(req) else {
                    continue;
                };
                let style = if node.research == sel {
                    Some(if done(req) { t.good } else { t.warn })
                } else if req == sel {
                    Some(t.title)
                } else {
                    None
                };
                let (style, prio) = match (pass, style) {
                    (0, _) => (t.grid, 1),
                    (1, Some(style)) => {
                        related.push((
                            if node.research == sel {
                                req
                            } else {
                                node.research
                            },
                            style,
                        ));
                        (style, 2)
                    }
                    _ => continue,
                };
                let points = geo.route(from, *node);
                // Le trait part du bord de la case : sa première cellule lui est reliée.
                wires.mark(points[0].0, points[0].1, geo.exit_side(), style, prio);
                wires.path(&points, style, prio);
                if let Some(tip) = points.last() {
                    arrows.push((*tip, prio));
                }
            }
        }
    }
    for y in 0..total_h {
        for x in 0..total_w {
            let i = y * total_w + x;
            if wires.mask[i] == 0 {
                continue;
            }
            let (sx, sy) = to_screen(x, y);
            put_str(
                buf,
                view,
                sx,
                sy,
                &line_char(wires.mask[i], g.ascii).to_string(),
                wires.style[i],
            );
        }
    }
    // Pointes de flèche à l'entrée des cases, de la couleur du trait qui y arrive.
    for ((x, y), _) in &arrows {
        let style = wires.style.get(y * total_w + x).copied().unwrap_or(t.grid);
        let (sx, sy) = to_screen(*x, *y);
        put_str(buf, view, sx, sy, geo.arrow(g.ascii), style);
    }

    // Cases.
    for node in &layout.nodes {
        let research = &data.research[node.research];
        let active = app
            .world
            .research_active
            .as_ref()
            .filter(|a| a.id == research.id);
        let (mark, mut style) = if done(node.research) {
            (g.check, t.good)
        } else if active.is_some() {
            (g.cursor, t.title)
        } else if app.world.research_blocker(data, research).is_some() {
            (g.dash, t.dim)
        } else if app.world.money < research.cost {
            (g.bullet, t.text)
        } else {
            (g.bullet, t.station)
        };
        // Les prérequis et les suites de la recherche choisie ressortent, comme leurs traits.
        if let Some((_, linked)) = related.iter().find(|(i, _)| *i == node.research) {
            style = *linked;
        }
        if node.research == sel {
            style = style.patch(t.selected);
        }
        // La recherche en cours affiche son avancement dans sa case.
        let percent = active.map(|a| {
            format!(
                " {}%",
                (a.progress_s / (research.duration_h * 36.0)).clamp(0.0, 99.0) as u32
            )
        });
        let room = NODE_W - 4;
        let name_room = room - percent.as_ref().map_or(0, |p| p.chars().count());
        let mut name: String = research.name.fr.chars().take(name_room).collect();
        if research.name.fr.chars().count() > name_room {
            name.pop();
            name.push(if g.ascii { '.' } else { '…' });
        }
        let name = format!("{name:<name_room$}{}", percent.unwrap_or_default());
        let (ox, oy) = geo.origin(*node);
        let (x, y) = to_screen(ox, oy);
        put_str(buf, view, x, y, &format!("[{mark} {name:<room$}]"), style);
    }

    // Repères : époques et branches, en tête ou en marge selon l'orientation.
    let top = Rect {
        x: view.x,
        width: view.width,
        ..header
    };
    let margin = Rect {
        x: body.x,
        width: margin_w,
        ..body
    };
    let rule = |n: usize| line_char(E | W, g.ascii).to_string().repeat(n);
    for (era, (first, width)) in data.eras.iter().zip(&layout.eras) {
        let reached = app.world.era_index(data) >= data.era_index(&era.id).unwrap_or(0);
        let style = if reached { t.title } else { t.dim };
        if geo.vertical {
            let (_, y) = to_screen(0, first * V_PITCH);
            // Le nom tient sur deux lignes de marge au plus, les années en dessous.
            let width = margin_w.saturating_sub(1) as usize;
            let mut lines: Vec<String> = vec![String::new()];
            for word in era.name.fr.split_whitespace() {
                let first_line = lines.len() < 2;
                let last = lines.last_mut().expect("au moins une ligne");
                let full = last.chars().count() + 1 + word.chars().count() > width;
                if !last.is_empty() && full && first_line {
                    lines.push(word.to_string());
                } else {
                    if !last.is_empty() {
                        last.push(' ');
                    }
                    last.push_str(word);
                }
            }
            lines.push(format!("{}-{}", era.from, era.to));
            for (i, line) in lines.iter().enumerate() {
                let text: String = line.chars().take(width).collect();
                let style = if i + 1 == lines.len() { t.dim } else { style };
                put_str(buf, margin, margin.x as i32, y + i as i32, &text, style);
            }
        } else {
            let (x, _) = to_screen(first * PITCH, 0);
            let span = width * PITCH - GAP;
            let label: String = format!("{} ({}-{})", era.name.fr, era.from, era.to)
                .chars()
                .take(span)
                .collect();
            put_str(buf, top, x, header.y as i32, &label, style);
            put_str(buf, top, x, header.y as i32 + 1, &rule(span), t.grid);
        }
    }
    for (branch, first, height) in &layout.lanes {
        if geo.vertical {
            let (x, _) = to_screen(first * V_SLOT, 0);
            put_str(buf, top, x, header.y as i32, branch_name(*branch), t.title);
            put_str(
                buf,
                top,
                x,
                header.y as i32 + 1,
                &rule(height * V_SLOT - 2),
                t.grid,
            );
        } else {
            let (_, y) = to_screen(0, first * 2);
            put_str(
                buf,
                margin,
                margin.x as i32,
                y,
                branch_name(*branch),
                t.title,
            );
        }
    }

    if let Some(research) = data.research.get(sel) {
        Paragraph::new(research_card(app, research))
            .wrap(Wrap { trim: true })
            .render(detail, buf);
    } else {
        Paragraph::new(Line::raw("")).render(detail, buf);
    }
    footer(
        app,
        buf,
        foot,
        "Entrée : lancer   flèches : parcourir   Tab : pivoter l'arbre   v : liste   Échap : fermer   couleurs : prérequis et suites",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> (TreeLayout, GameData) {
        let data = GameData::embedded().unwrap();
        (TreeLayout::new(&data), data)
    }

    fn find(data: &GameData, id: &str) -> usize {
        data.research.iter().position(|r| r.id == id).unwrap()
    }

    #[test]
    fn every_research_has_its_own_cell() {
        let (layout, data) = layout();
        assert_eq!(layout.nodes.len(), data.research.len());
        let mut cells: Vec<(usize, usize)> = layout.nodes.iter().map(|n| (n.col, n.row)).collect();
        cells.sort();
        cells.dedup();
        assert_eq!(
            cells.len(),
            data.research.len(),
            "deux recherches dans la même case"
        );
        assert!(
            layout
                .nodes
                .iter()
                .all(|n| n.col < layout.cols && n.row < layout.rows)
        );
    }

    #[test]
    fn prerequisites_are_always_to_the_left() {
        let (layout, data) = layout();
        for node in &layout.nodes {
            for req in &data.research[node.research].requires {
                let from = layout.node(find(&data, req)).unwrap();
                assert!(
                    from.col < node.col,
                    "{req} doit précéder {}",
                    data.research[node.research].id
                );
            }
        }
    }

    #[test]
    fn eras_are_contiguous_blocks_and_branches_are_bands() {
        let (layout, data) = layout();
        for node in &layout.nodes {
            let research = &data.research[node.research];
            let (first, width) = layout.eras[data.era_index(&research.era).unwrap()];
            assert!(
                (first..first + width).contains(&node.col),
                "{}",
                research.id
            );
            let (_, start, height) = layout
                .lanes
                .iter()
                .find(|(b, _, _)| *b == research.branch)
                .unwrap();
            assert!(
                (*start..start + height).contains(&node.row),
                "{}",
                research.id
            );
        }
        // Le télégraphe suit les signaleurs, sur la même ligne.
        let (signals, telegraph) = (
            layout.node(find(&data, "hand_signals")).unwrap(),
            layout.node(find(&data, "electric_telegraph")).unwrap(),
        );
        assert_eq!(
            (telegraph.col, telegraph.row),
            (signals.col + 1, signals.row)
        );
    }

    #[test]
    fn arrows_walk_the_tree_and_can_reach_every_node() {
        let (layout, data) = layout();
        let signals = find(&data, "hand_signals");
        assert_eq!(
            layout.neighbour(signals, Move::Right),
            Some(find(&data, "electric_telegraph"))
        );
        assert_eq!(
            layout.neighbour(find(&data, "electric_telegraph"), Move::Left),
            Some(signals)
        );
        assert_eq!(layout.neighbour(signals, Move::Left), None);

        // En partant de n'importe où, les flèches permettent d'atteindre toutes les recherches.
        let mut seen = vec![false; data.research.len()];
        let mut queue = vec![signals];
        while let Some(i) = queue.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            for direction in [Move::Left, Move::Right, Move::Up, Move::Down] {
                queue.extend(layout.neighbour(i, direction));
            }
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn wires_join_with_the_right_corners() {
        assert_eq!(line_char(E | W, false), '─');
        assert_eq!(line_char(N | S, false), '│');
        assert_eq!(line_char(E | S, false), '┌');
        assert_eq!(line_char(N | W, false), '┘');
        assert_eq!(line_char(N | E | S | W, false), '┼');
        assert_eq!(
            (
                line_char(E | W, true),
                line_char(N | S, true),
                line_char(N | E, true)
            ),
            ('-', '|', '+')
        );

        // Un trajet en L laisse un coin au bon endroit.
        let mut wires = Wires::new(4, 3);
        wires.path(&[(0, 0), (2, 0), (2, 2)], Style::new(), 1);
        assert_eq!(wires.mask[2], W | S);
        assert_eq!(wires.mask[1], E | W);
        assert_eq!(wires.mask[2 * 4 + 2], N);
    }

    #[test]
    fn far_links_use_the_gutter_between_rows() {
        let geo = Geometry { vertical: false };
        let (a, near, far) = (
            Node {
                research: 0,
                col: 0,
                row: 1,
            },
            Node {
                research: 1,
                col: 1,
                row: 3,
            },
            Node {
                research: 2,
                col: 3,
                row: 0,
            },
        );
        let path = geo.route(a, near);
        assert!(path.iter().all(|(_, y)| *y == 2 || *y == 6), "{path:?}");
        let path = geo.route(a, far);
        assert!(
            path.contains(&(NODE_W, 3)),
            "le trait descend dans l'interligne : {path:?}"
        );
        assert_eq!(*path.last().unwrap(), (3 * PITCH - 1, 0));
    }

    #[test]
    fn vertical_tree_puts_eras_in_rows_and_never_crosses_a_node() {
        let (layout, data) = layout();
        let geo = Geometry { vertical: true };
        let (w, h) = geo.size(&layout);
        // Cellules occupées par les cases.
        let mut boxes = vec![false; w * h];
        for node in &layout.nodes {
            let (x, y) = geo.origin(*node);
            for dx in 0..NODE_W {
                boxes[y * w + x + dx] = true;
            }
        }
        let mut wires = Wires::new(w, h);
        for node in &layout.nodes {
            for req in &data.research[node.research].requires {
                let from = layout.node(find(&data, req)).unwrap();
                // Un prérequis est toujours plus haut que ce qu'il débloque.
                assert!(geo.origin(from).1 < geo.origin(*node).1);
                let path = geo.route(from, *node);
                assert_eq!(
                    *path.last().unwrap(),
                    (geo.origin(*node).0 + NODE_W / 2, geo.origin(*node).1 - 1)
                );
                wires.path(&path, Style::new(), 1);
            }
        }
        let crossing = (0..w * h)
            .filter(|&i| boxes[i] && wires.mask[i] != 0)
            .count();
        assert_eq!(crossing, 0, "un trait traverse une case");
    }

    #[test]
    fn arrow_keys_follow_the_orientation() {
        let (layout, data) = layout();
        let (signals, telegraph) = (
            find(&data, "hand_signals"),
            find(&data, "electric_telegraph"),
        );
        assert_eq!(step(&layout, signals, Move::Right, false), Some(telegraph));
        assert_eq!(step(&layout, signals, Move::Down, true), Some(telegraph));
        assert_eq!(step(&layout, telegraph, Move::Up, true), Some(signals));
        assert_eq!(step(&layout, signals, Move::Up, true), None);
    }
}
