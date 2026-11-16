//! Interface terminal (Ratatui + Crossterm).

mod app;
mod camera;
mod canvas;
mod format;
mod glyphs;
mod map;
mod popups;
mod theme;

use std::io::stdout;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use crate::data::GameData;
use crate::save;
use crate::sim::World;

pub use app::{App, UiOptions};
pub use theme::ColorMode;

const FRAME: Duration = Duration::from_millis(50);
const AUTOSAVE: Duration = Duration::from_secs(10);
/// Au-delà, on considère que le processus a été suspendu : pas de rattrapage.
const MAX_FRAME_DT: f64 = 0.25;

/// Rend le terminal dans son état normal, y compris en cas de panique.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}

/// Lance l'interface interactive et sauvegarde la partie en quittant.
pub fn run(world: World, data: GameData, save_dir: &Path, opts: UiOptions) -> Result<()> {
    let mut terminal = ratatui::try_init().context("initialisation du terminal impossible")?;
    let _guard = TerminalGuard;
    execute!(stdout(), EnableMouseCapture).context("activation de la souris impossible")?;

    let mut app = App::new(world, data, opts);
    let mut last = Instant::now();
    let mut last_save = Instant::now();
    let mut save_failed = false;

    while !app.quit {
        terminal.draw(|frame| app.draw(frame))?;

        if event::poll(FRAME.saturating_sub(last.elapsed()))? {
            // Vide la file pour ne pas prendre de retard sur la molette ou le glisser.
            loop {
                app.on_event(event::read()?);
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        let now = Instant::now();
        app.update(now.duration_since(last).as_secs_f64().min(MAX_FRAME_DT));
        last = now;

        if last_save.elapsed() >= AUTOSAVE {
            last_save = Instant::now();
            match save::save(save_dir, &app.world) {
                Ok(()) => save_failed = false,
                Err(e) if !save_failed => {
                    save_failed = true;
                    app.push_log(
                        format!("Sauvegarde automatique impossible : {e:#}"),
                        app::LogKind::Warn,
                    );
                }
                Err(_) => {}
            }
        }
    }

    save::save(save_dir, &app.world)
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn app(ascii: bool, color: ColorMode) -> App {
        let data = GameData::embedded().unwrap();
        App::new(World::new(&data), data, UiOptions { ascii, color })
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    #[test]
    fn map_shows_stations_hud_and_docked_train() {
        let mut app = app(false, ColorMode::Color);
        let text = screen(&mut app, 120, 40);
        for expected in [
            "RAINHILL",
            "STOCKTON",
            "DARLINGTON",
            "Locomotion n°1 #1",
            "£ 50",
            "Trains 1/4",
            "Tees",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        assert!(
            text.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)),
            "aucun tracé braille"
        );
    }

    #[test]
    fn ascii_mode_draws_only_ascii_on_the_map() {
        let mut app = app(true, ColorMode::NoColor);
        press(&mut app, KeyCode::Char('d'));
        app.update(0.2);
        app.update(0.2);
        let text = screen(&mut app, 120, 40);
        let stray: Vec<char> = text.chars().filter(|c| !c.is_ascii()).collect();
        assert!(stray.is_empty(), "caractères non ASCII : {stray:?}");
        assert!(text.contains('='), "pas de voie dessinée");
        assert!(
            text.contains("| L 4") && text.contains("Locomotion no1 #1"),
            "{text}"
        );
        for popup in ['a', 'r', '?'] {
            press(&mut app, KeyCode::Char(popup));
            let text = screen(&mut app, 120, 40);
            assert!(text.is_ascii(), "fenêtre {popup} : {text}");
            press(&mut app, KeyCode::Esc);
        }
    }

    #[test]
    fn dispatching_moves_the_train_and_earns_money() {
        let mut app = app(false, ColorMode::Colorblind);
        app.world.stations.get_mut("stockton").unwrap().waiting = 100.0;
        // Dix minutes de jeu : le temps de remplir les 40 places.
        for _ in 0..20 {
            app.update(0.25);
        }
        assert_eq!(app.world.trains[0].passengers, 40);
        press(&mut app, KeyCode::Char('d'));
        assert!(!app.world.trains[0].is_docked());
        let before = app.world.money;
        // 19 km à 24 km/h : moins de 30 secondes réelles à l'échelle 120.
        for _ in 0..120 {
            app.update(0.25);
        }
        assert!(app.world.trains[0].is_docked());
        assert!(app.world.money > before);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("arrive à Darlington"), "{text}");
    }

    #[test]
    fn shop_and_research_popups_work_from_the_keyboard() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('a'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Rocket") && text.contains("verrouillée"),
            "{text}"
        );
        press(&mut app, KeyCode::Down);
        assert!(
            screen(&mut app, 120, 40).contains("Requiert la recherche : Chaudière multitubulaire")
        );
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("Pas assez d'argent"));

        app.world.money = 1000.0;
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.trains.len(), 2);
        assert_eq!(app.selected, Some(2));

        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world.research_active.as_ref().map(|a| a.id.as_str()),
            Some("hand_signals")
        );
        assert!(screen(&mut app, 120, 40).contains("Recherche : Signaleurs de voie"));
    }

    /// Aperçu manuel : `cargo test preview -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn preview() {
        let mut app = app(false, ColorMode::Color);
        app.world.stations.get_mut("stockton").unwrap().waiting = 100.0;
        app.world.money = 2000.0;
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Enter);
        for _ in 0..20 {
            app.update(0.25);
        }
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Char('d'));
        for _ in 0..36 {
            app.update(0.25);
        }
        println!("{}\n", screen(&mut app, 110, 32));
        press(&mut app, KeyCode::Char('f'));
        for _ in 0..5 {
            press(&mut app, KeyCode::Char('+'));
        }
        app.update(0.01);
        println!("{}\n", screen(&mut app, 110, 32));
        let mut ascii = self::app(true, ColorMode::NoColor);
        press(&mut ascii, KeyCode::Char('d'));
        for _ in 0..30 {
            ascii.update(0.25);
        }
        println!("{}", screen(&mut ascii, 110, 32));
    }

    #[test]
    fn zoom_pan_and_tiny_terminals_do_not_break_rendering() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('d'));
        app.update(0.2);
        screen(&mut app, 100, 30);
        for _ in 0..40 {
            press(&mut app, KeyCode::Char('+'));
            press(&mut app, KeyCode::Left);
            screen(&mut app, 100, 30);
        }
        for _ in 0..80 {
            press(&mut app, KeyCode::Char('-'));
            screen(&mut app, 100, 30);
        }
        press(&mut app, KeyCode::Char('0'));
        press(&mut app, KeyCode::Char('f'));
        app.update(0.1);
        for (w, h) in [(1, 1), (10, 3), (40, 8), (300, 100)] {
            screen(&mut app, w, h);
            press(&mut app, KeyCode::Char('a'));
            screen(&mut app, w, h);
            press(&mut app, KeyCode::Char('?'));
            screen(&mut app, w, h);
            press(&mut app, KeyCode::Esc);
        }
        assert!(!app.quit);
    }
}
