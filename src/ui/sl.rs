//! Animation de transition : la locomotive D51 de `sl`, qui traverse l'écran
//! en crachant sa fumée.
//!
//! Les dessins, les motifs de fumée et leur mouvement viennent de `sl` :
//!
//! > Copyright 1993,1998,2014 Toyoda Masashi (mtoyoda@acm.org)
//! >
//! > Everyone is permitted to do anything on this program including copying,
//! > modifying, and improving, unless you try to pretend that you wrote it.
//! > i.e., the above copyright notice has to appear in all copies.
//! > THE AUTHOR DISCLAIMS ANY RESPONSIBILITY WITH REGARD TO THIS SOFTWARE.
//!
//! `sl` avance d'une colonne par image ; ici, la position dépend du temps
//! écoulé, et la fumée est recalculée à partir de la position du train, avec
//! les mêmes règles, pour que l'animation ne dépende pas de la cadence.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Clear, Widget};

/// Durée de la traversée, en secondes réelles.
pub const DURATION: f64 = 0.4;

/// Caisse de la D51, commune à toutes les images.
const BODY: [&str; 7] = [
    "      ====        ________                ___________ ",
    "  _D _|  |_______/        \\__I_I_____===__|_________| ",
    "   |(_)---  |   H\\________/ |   |        =|___ ___|   ",
    "   /     |  |   H  |  |     |   |         ||_| |_||   ",
    "  |      |  |   H  |__--------------------| [___] |   ",
    "  | ________|___H__/__|_____/[][]~\\_______|       |   ",
    "  |/ |   |-----------I_____I [][] []  D   |=======|__ ",
];

/// Les six positions des roues et des bielles.
const WHEELS: [[&str; 3]; 6] = [
    [
        "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ",
        " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ",
        "  \\_/      \\O=====O=====O=====O_/      \\_/            ",
    ],
    [
        "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ",
        " |/-=|___|=O=====O=====O=====O   |_____/~\\___/        ",
        "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ",
    ],
    [
        "__/ =| o |=-O=====O=====O=====O \\ ____Y___________|__ ",
        " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ",
        "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ",
    ],
    [
        "__/ =| o |=-~O=====O=====O=====O\\ ____Y___________|__ ",
        " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ",
        "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ",
    ],
    [
        "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ",
        " |/-=|___|=   O=====O=====O=====O|_____/~\\___/        ",
        "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ",
    ],
    [
        "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ",
        " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ",
        "  \\_/      \\_O=====O=====O=====O/      \\_/            ",
    ],
];

/// Tender à charbon, accroché 53 colonnes derrière l'avant de la machine.
const COAL: [&str; 10] = [
    "                              ",
    "                              ",
    "    _________________         ",
    "   _|                \\_____A  ",
    " =|                        |  ",
    " -|                        |  ",
    "__|________________________|_ ",
    "|__________________________|_ ",
    "   |_D__D__D_|  |_D__D__D_|   ",
    "    \\_/   \\_/    \\_/   \\_/    ",
];

const COAL_OFFSET: i32 = 53;
/// Longueur du convoi : il a fini de passer quand il est sorti de cette largeur.
pub const LENGTH: i32 = 83;
/// Colonne de la cheminée, d'où part la fumée.
const FUNNEL: i32 = 7;
const HEIGHT: i32 = 10;

/// Les deux sortes de bouffées, qui se dissipent en seize étapes.
const SMOKE: [[&str; 16]; 2] = [
    [
        "(   )", "(    )", "(    )", "(   )", "(  )", "(  )", "( )", "( )", "()", "()", "O", "O",
        "O", "O", "O", " ",
    ],
    [
        "(@@@)", "(@@@@)", "(@@@@)", "(@@@)", "(@@)", "(@@)", "(@)", "(@)", "@@", "@@", "@", "@",
        "@", "@", "@", " ",
    ],
];
/// Déplacement d'une bouffée à chaque étape : elle monte puis file vers l'arrière.
const DY: [i32; 16] = [2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const DX: [i32; 16] = [-2, -1, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3];

/// Colonne de l'avant de la machine après `elapsed` secondes, sur un écran
/// large de `width` : elle entre par la droite et sort complètement à gauche.
pub fn position(width: u16, elapsed: f64) -> i32 {
    let distance = width as f64 + LENGTH as f64;
    let travelled = (elapsed / DURATION).clamp(0.0, 1.0) * distance;
    width as i32 - 1 - travelled.floor() as i32
}

/// Vrai quand le convoi a quitté l'écran.
pub fn finished(width: u16, elapsed: f64) -> bool {
    position(width, elapsed) < -LENGTH
}

/// Écrit `text` à partir de (`x`, `y`) relatifs à `area`, en coupant ce qui dépasse.
fn put(buf: &mut Buffer, area: Rect, x: i32, y: i32, text: &str, style: Style) {
    if y < 0 || y >= area.height as i32 {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        let col = x + i as i32;
        if col < 0 {
            continue;
        }
        if col >= area.width as i32 {
            break;
        }
        buf[(area.x + col as u16, area.y + y as u16)]
            .set_char(ch)
            .set_style(style);
    }
}

/// Dessine l'image de la traversée après `elapsed` secondes : écran vidé, train,
/// puis fumée par-dessus, comme dans `sl`.
pub fn draw(buf: &mut Buffer, area: Rect, elapsed: f64, train: Style, smoke: Style) {
    Clear.render(area, buf);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_style(train);
        }
    }
    let x = position(area.width, elapsed);
    let y = area.height as i32 / 2 - 5;

    let wheels = &WHEELS[(LENGTH + x).rem_euclid(6) as usize];
    for (i, line) in BODY.iter().chain(wheels.iter()).enumerate() {
        put(buf, area, x, y + i as i32, line, train);
    }
    for (i, line) in COAL.iter().enumerate().take(HEIGHT as usize) {
        put(buf, area, x + COAL_OFFSET, y + i as i32, line, train);
    }

    // Une bouffée naît chaque fois que la cheminée passe sur une colonne
    // multiple de 4 ; à chaque naissance, les précédentes avancent d'une étape.
    let start = area.width as i32 - 1;
    let spawns: Vec<i32> = (x..=start)
        .rev()
        .filter(|col| (col + FUNNEL).rem_euclid(4) == 0)
        .collect();
    let count = spawns.len();
    for (index, col) in spawns.into_iter().enumerate() {
        let age = count - 1 - index;
        let (mut sy, mut sx) = (y - 1, col + FUNNEL);
        for step in 0..age {
            let p = step.min(15);
            sy -= DY[p];
            sx += DX[p];
        }
        put(buf, area, sx, sy, SMOKE[index % 2][age.min(15)], smoke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u16, height: u16, elapsed: f64) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        draw(&mut buf, area, elapsed, Style::default(), Style::default());
        (0..height)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    #[test]
    fn the_drawings_line_up_like_in_sl() {
        assert!(BODY.iter().all(|l| l.len() == 54));
        assert!(WHEELS.iter().flatten().all(|l| l.len() == 54));
        assert!(COAL.iter().all(|l| l.len() == 30));
        assert_eq!(COAL_OFFSET + 30, LENGTH);
    }

    #[test]
    fn the_train_enters_on_the_right_and_leaves_on_the_left() {
        assert_eq!(position(100, 0.0), 99);
        assert!(!finished(100, DURATION * 0.99));
        assert!(finished(100, DURATION));
        // Au début, seul l'avant de la machine pointe dans la dernière colonne ;
        // à mi-course, la D51 est là, avec sa fumée.
        assert!(frame(100, 30, 0.0).iter().all(|l| l[..99].trim().is_empty()));
        let middle = frame(100, 30, DURATION * 0.45).join("\n");
        assert!(middle.contains("_D _|  |_______/"), "{middle}");
        assert!(middle.contains("|_D__D__D_|"), "{middle}");
        assert!(middle.contains("(@@") || middle.contains("(  "), "{middle}");
    }

    #[test]
    fn tiny_screens_do_not_panic() {
        for (w, h) in [(1, 1), (5, 3), (200, 4)] {
            for step in 0..20 {
                frame(w, h, DURATION * step as f64 / 19.0);
            }
        }
    }
}
