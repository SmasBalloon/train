//! Interface terminal (Ratatui + Crossterm).

mod app;
mod board;
mod camera;
mod canvas;
mod format;
mod glyphs;
mod map;
mod popups;
mod sl;
mod theme;

use std::io::stdout;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use crate::data::GameData;
use crate::save;
use crate::sim::{OfflineReport, World};

pub use app::{App, Popup, Screen, UiOptions};
pub use theme::ColorMode;

const FRAME: Duration = Duration::from_millis(50);
const AUTOSAVE: Duration = Duration::from_secs(10);
/// En lecture seule, fréquence de vérification du fichier de sauvegarde.
const RELOAD_CHECK: Duration = Duration::from_secs(1);
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

/// Conditions de lancement de l'interface.
pub struct Session<'a> {
    pub save_dir: &'a Path,
    /// Faux si une autre instance tient la partie : on l'observe sans l'écrire.
    pub writable: bool,
    pub screen: Screen,
    /// Résumé de l'absence, s'il y en a une à raconter.
    pub report: Option<OfflineReport>,
    /// Fenêtre à ouvrir d'emblée (`rail research`, `rail fleet`…).
    pub popup: Popup,
}

/// Ligne d'état compacte pour un prompt ou une barre tmux.
pub fn status_line(world: &World, data: &GameData, ascii: bool) -> String {
    let income = world.income_per_game_second() * data.balance.time_scale;
    let moving = world.trains_moving();
    let text = format!(
        "{} │ {moving} train{} en route │ {}{} €/s",
        format::money(world.money),
        if moving > 1 { "s" } else { "" },
        if income > -0.05 { "+" } else { "-" },
        format::dec1(income.abs()),
    );
    if ascii {
        text.replace('│', "|")
            .chars()
            .map(map::ascii_fold)
            .collect()
    } else {
        text
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Lance l'interface. En écriture, la partie est sauvegardée régulièrement
/// et en quittant ; en lecture seule, elle est relue quand le fichier change.
pub fn run(world: World, data: GameData, session: Session, opts: UiOptions) -> Result<()> {
    let mut terminal = ratatui::try_init().context("initialisation du terminal impossible")?;
    let _guard = TerminalGuard;
    execute!(stdout(), EnableMouseCapture).context("activation de la souris impossible")?;

    let save_dir = session.save_dir;
    let save_path = save::save_path(save_dir);
    let mut app = App::new(world, data, opts);
    app.screen = session.screen;
    app.read_only = !session.writable;
    app.open(session.popup);
    if app.read_only {
        app.push_log(
            "Une autre instance fait tourner la partie : lecture seule (rail daemon stop pour reprendre la main)",
            app::LogKind::Warn,
        );
    }
    if let Some(report) = session.report.filter(|_| session.writable) {
        app.welcome_back(report);
    }
    // Nouvelle partie : on choisit d'abord le pays de la compagnie.
    if session.writable && app.world.choosing_home {
        app.offer_home();
    }

    let mut last = Instant::now();
    let mut last_save = Instant::now();
    let mut last_check = Instant::now();
    let mut seen = modified(&save_path);
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

        if app.settings_dirty {
            app.settings_dirty = false;
            let settings = save::Settings {
                transitions: app.transitions,
            };
            if let Err(e) = save::save_settings(save_dir, &settings) {
                app.push_log(
                    format!("Préférences non enregistrées : {e:#}"),
                    app::LogKind::Warn,
                );
            }
        }

        if session.writable {
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
        } else if last_check.elapsed() >= RELOAD_CHECK {
            last_check = Instant::now();
            let current = modified(&save_path);
            if current != seen {
                seen = current;
                // Une lecture ratée (écriture en cours) sera retentée au prochain passage.
                if let Ok(Some((world, _))) = save::load_current(save_dir, &app.data) {
                    app.replace_world(world);
                }
            }
        }
    }

    if session.writable {
        save::save(save_dir, &app.world)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn app(ascii: bool, color: ColorMode) -> App {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.test_driver(1);
        App::new(
            world,
            data,
            UiOptions {
                ascii,
                color,
                transitions: false,
            },
        )
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
            "1825",
            "STOCKTON",
            "DARLINGTON",
            "Locomotion n°1 #1",
            "50 €",
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
            text.contains("| 49 E |") && text.contains("Locomotion no1 #1"),
            "{text}"
        );
        for popup in [
            'a', 'r', 't', 'e', 'u', 'c', 'v', 'n', 'g', 'o', 's', '?', 'b',
        ] {
            press(&mut app, KeyCode::Char(popup));
            let text = screen(&mut app, 120, 40);
            assert!(text.is_ascii(), "fenêtre {popup} : {text}");
            press(&mut app, KeyCode::Esc);
        }
    }

    #[test]
    fn dispatching_moves_the_train_and_earns_money() {
        let mut app = app(false, ColorMode::Colorblind);
        fill(&mut app, "stockton", 100.0);
        // Dix minutes de jeu (10 s réelles) : le temps de remplir les 40 places.
        for _ in 0..40 {
            app.update(0.25);
        }
        assert_eq!(app.world.trains[0].count(), 40);
        press(&mut app, KeyCode::Char('d'));
        assert!(!app.world.trains[0].is_docked());
        let before = app.world.money;
        // 19 km à 24 km/h : moins d'une minute réelle à l'échelle 60.
        for _ in 0..240 {
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
            text.contains("Rocket") && text.contains("recherche requise"),
            "{text}"
        );
        assert!(
            text.contains("TGV Sud-Est") && text.contains("époque ultérieure"),
            "{text}"
        );
        assert!(
            text.contains("27 septembre 1825"),
            "la fiche historique est affichée :\n{text}"
        );
        press(&mut app, KeyCode::Down);
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Indisponible : Il faut d'abord rechercher : Chaudière multitubulaire"),
            "{text}"
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
        // La fenêtre se referme, et le lancement se voit dans le bandeau du haut…
        assert_eq!(app.popup, app::Popup::None);
        // … comme dans la liste, rouverte sur la recherche en cours.
        press(&mut app, KeyCode::Char('r'));
        let text = screen(&mut app, 120, 40);
        let row = text
            .lines()
            .find(|l| l.contains("Signaleurs de voie") && l.contains("Signalisation"))
            .unwrap_or_default();
        assert!(row.contains("en cours"), "{text}");
        press(&mut app, KeyCode::Esc);
        let top = screen(&mut app, 120, 40)
            .lines()
            .next()
            .unwrap()
            .to_string();
        assert!(
            top.contains("Signaleurs de voie") && top.trim_end().ends_with("4 min"),
            "{top}"
        );
    }

    fn enter(app: &mut App, era: &str) {
        app.world.era = era.to_string();
        app.world.era_started = app.world.clock;
    }

    #[test]
    fn network_popup_buys_concessions_and_builds_lines() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('n'));
        let text = screen(&mut app, 130, 44);
        for expected in [
            "ROYAUME-UNI",
            "concession acquise",
            "Stockton & Darlington",
            "ligne courante",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        // La liste défile jusqu'aux pays lointains, fermés pour l'instant.
        press(&mut app, KeyCode::End);
        let text = screen(&mut app, 130, 44);
        for expected in [
            "RUSSIE",
            "voie de 1520 mm",
            "époque ultérieure",
            "Kazan – Iekaterinbourg",
            "950 km",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        press(&mut app, KeyCode::Home);
        press(&mut app, KeyCode::Down);

        // Darlington – York : la ligne voisine, constructible tout de suite.
        press(&mut app, KeyCode::Down);
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("à construire : 3 500 €") && text.contains("Darlington → York"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 130, 44).contains("Pas assez d'argent"));
        app.world.money = 20_000.0;
        press(&mut app, KeyCode::Enter);
        assert!(app.world.is_built("darlington_york"));
        assert_eq!(
            (app.line_id(), app.popup),
            ("darlington_york", app::Popup::None)
        );
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("YORK") && text.contains("Darlington – York"),
            "{text}"
        );

        // Le dépôt équipe désormais la nouvelle ligne.
        press(&mut app, KeyCode::Char('a'));
        assert!(screen(&mut app, 130, 44).contains("une locomotive pour Darlington – York"));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.trains_on("darlington_york"), 1);

        // Une ligne non reliée est refusée ; une concession s'achète depuis la même fenêtre.
        press(&mut app, KeyCode::Char('n'));
        let rows = app.network_rows();
        let find = |wanted: &str| {
            rows.iter()
                .position(|row| match row {
                    app::NetworkRow::Country(c) => c.id == wanted,
                    app::NetworkRow::Line(l) => l.id == wanted,
                })
                .unwrap()
        };
        let (france, lyon_marseille, stockton) = (
            find("france"),
            find("lyon_marseille"),
            find("stockton_darlington"),
        );
        app.open(app::Popup::Network {
            sel: lyon_marseille,
        });
        assert!(screen(&mut app, 130, 44).contains("Il faut d'abord une concession : France"));
        app.open(app::Popup::Network { sel: france });
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("FRANCE") && text.contains("concession : 8 000 €"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(app.world.has_concession("france"));
        app.open(app::Popup::Network {
            sel: lyon_marseille,
        });
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 130, 44).contains("déjà desservie, ou de la capitale"));

        // Choisir une ligne déjà construite en fait la ligne courante.
        app.open(app::Popup::Network { sel: stockton });
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.line_id(), "stockton_darlington");
    }

    #[test]
    fn the_sl_locomotive_crosses_between_screens_and_can_be_turned_off() {
        let mut app = app(false, ColorMode::Color);
        app.transitions = true;
        app.update(0.1);
        // Ouvrir une fenêtre depuis la carte fait passer la locomotive.
        press(&mut app, KeyCode::Char('a'));
        let start = app.transition.expect("transition lancée");
        app.update(super::sl::DURATION * 0.45);
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("_D _|  |_______/") && !text.contains("Dépôt"),
            "{text}"
        );
        // Elle finit seule, et la fenêtre apparaît.
        app.update(super::sl::DURATION);
        let text = screen(&mut app, 120, 40);
        assert!(app.transition.is_none() && text.contains("Dépôt"), "{text}");
        assert!(app.time > start);
        // Naviguer dans une fenêtre ne la relance pas ; changer d'écran, si.
        press(&mut app, KeyCode::Down);
        assert!(app.transition.is_none());
        press(&mut app, KeyCode::Esc);
        assert!(app.transition.is_none(), "fermer ne la relance pas");
        press(&mut app, KeyCode::Char('b'));
        assert!(app.transition.is_some());
        // Une touche l'interrompt et agit normalement.
        press(&mut app, KeyCode::Char('b'));
        assert_eq!(app.screen, Screen::Map);
        app.transition = None;

        // S la coupe, et demande d'enregistrer la préférence.
        press(&mut app, KeyCode::Char('S'));
        assert!(!app.transitions && app.settings_dirty);
        press(&mut app, KeyCode::Char('a'));
        assert!(app.transition.is_none());
        assert!(screen(&mut app, 120, 40).contains("Dépôt"));
    }

    #[test]
    fn shop_buys_several_locomotives_at_once() {
        let mut app = app(false, ColorMode::Color);
        app.update(0.1);
        app.world.money = 10_000.0;
        press(&mut app, KeyCode::Char('a'));
        assert_eq!(app.popup, app::Popup::Shop { sel: 0, qty: 1 });
        // Droite augmente la quantité, sans dépasser les places libres (3).
        for _ in 0..5 {
            press(&mut app, KeyCode::Right);
        }
        assert_eq!(app.popup, app::Popup::Shop { sel: 0, qty: 3 });
        press(&mut app, KeyCode::Left);
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("Dépôt : 2 locomotives") && text.contains("Commande : 2 × 400 € = 800 €"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.trains.len(), 3);
        assert_eq!(app.world.money, 10_000.0 - 800.0);
        assert_eq!(app.popup, app::Popup::None);
        assert_eq!(app.selected, Some(2));
    }

    #[test]
    fn crew_popup_recruits_and_assigns() {
        let data = GameData::embedded().unwrap();
        let mut app = App::new(
            World::new(&data),
            data,
            UiOptions {
                ascii: false,
                color: ColorMode::Color,
                transitions: false,
            },
        );
        app.update(0.1);
        // Nouvelle partie : personne aux commandes, le train reste à quai.
        assert!(app.world.crew.is_empty() && !app.world.trains[0].crew.has_driver);
        assert!(screen(&mut app, 130, 44).contains("SANS CONDUCTEUR"));
        press(&mut app, KeyCode::Char('d'));
        assert!(app.world.trains[0].is_docked());
        press(&mut app, KeyCode::Char('s'));
        // Personne n'est employé : la fenêtre s'ouvre sur le recrutement.
        assert_eq!(
            app.popup,
            app::Popup::Crew {
                sel: 0,
                recruiting: true
            }
        );
        let text = screen(&mut app, 130, 44);
        for expected in [
            "Recrutement 4",
            "Conducteur",
            "Contrôleur",
            "Conduite",
            "Douceur",
            "prime offerte",
            "prime ",
            " €",
            "★",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        // Le premier conducteur s'embauche sans prime, même sans le sou.
        app.world.money = 0.0;
        let driver = app.world.candidates[0].name.clone();
        press(&mut app, KeyCode::Enter);
        assert_eq!((app.world.crew.len(), app.world.candidates.len()), (1, 3));
        // Le conducteur recruté prend d'office le train qui n'en avait pas.
        assert_eq!(app.world.crew[0].train, Some(1));
        assert!(app.world.trains[0].crew.has_driver);
        // Le suivant de la liste est un contrôleur, payant.
        assert_eq!(
            app.world.candidates[0].role,
            crate::sim::CrewRole::Conductor
        );
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 130, 44).contains("Pas assez d'argent"));
        app.world.money = 5000.0;
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.crew.len(), 2);
        assert_eq!(
            app.world.crew[1].train, None,
            "seuls les conducteurs s'affectent seuls"
        );

        // Onglet des employés : Entrée affecte au train sélectionné, une seconde fois renvoie en réserve.
        press(&mut app, KeyCode::Tab);
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("Employés 2") && text.contains("en réserve") && text.contains(&driver),
            "{text}"
        );
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert!(app.world.crew.iter().all(|m| m.train == Some(1)));
        assert!(screen(&mut app, 130, 44).contains("Locomotion n°1 #1"));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.crew[1].train, None);

        // Licencier, puis vérifier le rappel de l'équipage sur la carte.
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.world.crew.len(), 1);
        press(&mut app, KeyCode::Esc);
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains(&format!("Équipage : {driver}")) && text.contains("contrôleur ordinaire"),
            "{text}"
        );
    }

    #[test]
    fn training_popup_schedules_and_enrolls() {
        let mut app = app(false, ColorMode::Color);
        app.update(0.1);
        app.world.money = 1e6;
        press(&mut app, KeyCode::Char('i'));
        assert!(screen(&mut app, 130, 44).contains("Aucune formation programmée"));
        // n : choisir la formation ; Entrée la programme et ouvre les inscriptions.
        press(&mut app, KeyCode::Char('n'));
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("Programmer une formation") && text.contains("Embarquement"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.trainings.len(), 1);
        let session = app.world.trainings[0].id;
        assert_eq!(app.popup, app::Popup::Enroll { session, sel: 0 });
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("Inscriptions : Conduite") && text.contains("0/5"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.trainings[0].enrolled.len(), 1);
        assert!(screen(&mut app, 130, 44).contains("inscrit"));
        // Une seconde fois : désinscrit et remboursé.
        let money = app.world.money;
        press(&mut app, KeyCode::Enter);
        assert!(app.world.trainings[0].enrolled.is_empty() && app.world.money > money);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.popup, app::Popup::Training { sel: 0 });
        assert!(screen(&mut app, 130, 44).contains("1/5 inscrits"));
        // La fiche de l'équipier rappelle sa formation ; f y mène aussi.
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('s'));
        assert!(screen(&mut app, 130, 44).contains("Inscrit à la formation conduite"));
        press(&mut app, KeyCode::Char('f'));
        assert_eq!(app.popup, app::Popup::Training { sel: 0 });
        // x annule la session et rembourse.
        let money = app.world.money;
        press(&mut app, KeyCode::Char('x'));
        assert!(app.world.trainings.is_empty() && app.world.money > money);
    }

    #[test]
    fn staff_popup_hires_services() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('g'));
        let text = screen(&mut app, 130, 44);
        for expected in [
            "Brigades de la voie",
            "recherche requise",
            "Service des approvisionnements",
            "époque ultérieure",
            "Départs automatiques : pas encore",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        app.world.research_done.insert("wrought_iron_rails".into());
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 130, 44).contains("Pas assez d'argent"));
        app.world.money = 5000.0;
        press(&mut app, KeyCode::Enter);
        assert!(app.world.staff.contains("track_gangs"));
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("en service") && text.contains("sous 80 %"),
            "{text}"
        );
    }

    #[test]
    fn nationalising_needs_a_second_confirmation() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('o'));
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("Compagnie : 0 points de prestige")
                && text.contains("pas encore possible"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 130, 44).contains("terminer tout l'arbre de recherche"));

        enter(&mut app, "steam");
        let ids: Vec<String> = app.data.research.iter().map(|r| r.id.clone()).collect();
        app.world.research_done.extend(ids);
        app.world.money = 77_777.0;
        app.world.prestige.points = 2;
        assert!(screen(&mut app, 130, 44).contains("+1 points"));
        // Première validation : on demande confirmation ; une autre touche annule.
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 130, 44).contains("Entrée pour confirmer"));
        press(&mut app, KeyCode::Down);
        assert_eq!(app.world.era, "steam");
        assert_eq!(
            app.popup,
            app::Popup::Company {
                sel: 0,
                confirm: false
            }
        );

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            (app.world.era.as_str(), app.world.prestige.points),
            ("pioneers", 3)
        );
        assert_eq!(app.world.money, app.data.balance.start_money);
        // La nouvelle compagnie choisit d'abord son pays : ici, la France.
        assert!(matches!(app.popup, app::Popup::Home { sel: 0 }));
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("dans quel pays") && text.contains("Saint-Étienne – Andrézieux"),
            "{text}"
        );
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.home, "france");
        assert_eq!(app.world.trains[0].line, "saint_etienne_andrezieux");
        assert!(app.world.has_concession("france") && !app.world.has_concession("uk"));
        assert_eq!(
            (app.world.prestige.points, app.world.money),
            (3, app.data.balance.start_money)
        );
        // La fenêtre revient sur les bonus, pour dépenser les points.
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("Compagnie : 3 points de prestige")
                && text.contains("Réputation de la compagnie"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            (
                app.world.prestige.level("reputation"),
                app.world.prestige.points
            ),
            (1, 1)
        );
        assert!(screen(&mut app, 130, 44).contains("recettes +10 %"));
    }

    #[test]
    fn map_has_three_zoom_levels() {
        let mut app = app(false, ColorMode::Color);
        // Vue de départ : la ligne, au niveau des gares.
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("vue gare") && text.contains("N0 T0 A0 F0"),
            "{text}"
        );
        assert!(
            !text.contains("LONDRES") && !text.contains("FRANCE"),
            "{text}"
        );

        // Vue pays : toutes les gares du Royaume-Uni, sans le détail des quais.
        press(&mut app, KeyCode::Char('2'));
        let text = screen(&mut app, 130, 44);
        for expected in ["vue pays", "LONDRES", "GLASGOW", "LIVERPOOL", "ROYAUME-UNI"] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        assert!(!text.contains("N0 T0"), "{text}");

        // Vue continent : les pays, et seulement les capitales des pays fermés.
        press(&mut app, KeyCode::Char('3'));
        let text = screen(&mut app, 130, 44);
        for expected in ["vue continent", "FRANCE", "ALLEMAGNE", "ESPAGNE", "RUSSIE"] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        assert!(
            !text.contains("MARSEILLE") && !text.contains("MUNICH"),
            "{text}"
        );

        // Retour à la gare du train sélectionné, puis à la ligne.
        press(&mut app, KeyCode::Char('1'));
        let text = screen(&mut app, 130, 44);
        assert!(
            text.contains("vue gare") && text.contains("STOCKTON"),
            "{text}"
        );
        press(&mut app, KeyCode::Char('0'));
        assert!(screen(&mut app, 130, 44).contains("DARLINGTON"));
    }

    #[test]
    fn board_only_cycles_through_served_stations() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('b'));
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..6 {
            seen.insert(app.board_station().unwrap().id.clone());
            press(&mut app, KeyCode::Tab);
        }
        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            ["darlington", "stockton"]
        );
    }

    #[test]
    fn research_list_shows_who_needs_what() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(
            app.popup,
            app::Popup::Research {
                sel: 0,
                branch: None
            }
        );
        let text = screen(&mut app, 160, 44);
        for expected in [
            "Bureau d'études : recherches",
            "Toutes 0/52",
            "Signalisation",
            "Signaleurs de voie",
            "Télégraphe électrique",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        // La liste suit les époques : les signaleurs avant le télégraphe.
        let line_of =
            |text: &str, needle: &str| text.lines().position(|l| l.contains(needle)).unwrap();
        assert!(line_of(&text, "Signaleurs de voie") < line_of(&text, "Télégraphe électrique"));

        // La fiche du bas suit la sélection, avec ses prérequis.
        let telegraph = app
            .research_items(None)
            .iter()
            .position(|r| r.id == "electric_telegraph")
            .unwrap();
        for _ in 0..telegraph {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(
            app.popup,
            app::Popup::Research {
                sel: telegraph,
                branch: None
            }
        );
        let text = screen(&mut app, 160, 44);
        assert!(
            text.contains("Cooke et Wheatstone") && text.contains("Prérequis : Signaleurs de voie"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(
            screen(&mut app, 160, 44).contains("Il faut d'abord rechercher : Signaleurs de voie")
        );

        // On lance une recherche ; la fenêtre rouverte se place dessus.
        press(&mut app, KeyCode::Home);
        app.world.money = 600.0;
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world.research_active.as_ref().map(|a| a.id.as_str()),
            Some("hand_signals")
        );
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.popup, app::Popup::None);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('r'));
        let active = app
            .research_items(None)
            .iter()
            .position(|r| r.id == "hand_signals")
            .unwrap();
        assert_eq!(
            app.popup,
            app::Popup::Research {
                sel: active,
                branch: None
            }
        );
        // Plus de vue en arbre : v ne change rien.
        press(&mut app, KeyCode::Char('v'));
        assert!(matches!(app.popup, app::Popup::Research { .. }));
        for size in [(160, 44), (80, 24), (30, 8)] {
            for _ in 0..app.data.research.len() {
                press(&mut app, KeyCode::Down);
                screen(&mut app, size.0, size.1);
            }
        }
    }

    #[test]
    fn research_list_filters_by_branch_and_scrolls() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('r'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Toutes 0/52") && text.contains("Signaleurs de voie"),
            "{text}"
        );

        // Dernière entrée de la liste : une recherche du futur, inaccessible pour l'instant.
        press(&mut app, KeyCode::End);
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Tube à basse pression (spéculatif)"),
            "{text}"
        );
        assert!(
            text.contains("Indisponible : Pas avant l'époque : Futur"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(app.world.research_active.is_none());

        // Branche freinage : la première recherche est celle des serre-freins.
        for _ in 0..3 {
            press(&mut app, KeyCode::Tab);
        }
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Freinage +0,08 m/s²") && !text.contains("Signaleurs de voie"),
            "{text}"
        );
        app.world.money = 2000.0;
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world.research_active.as_ref().map(|a| a.id.as_str()),
            Some("hand_brakes")
        );

        // Chaque fiche de recherche et de locomotive s'affiche, même dans une petite fenêtre.
        for size in [(120, 40), (70, 16)] {
            press(&mut app, KeyCode::Char('r'));
            for _ in 0..app.data.research.len() {
                screen(&mut app, size.0, size.1);
                press(&mut app, KeyCode::Down);
            }
            press(&mut app, KeyCode::Esc);
            press(&mut app, KeyCode::Char('a'));
            for _ in 0..app.data.locomotives.len() {
                screen(&mut app, size.0, size.1);
                press(&mut app, KeyCode::Down);
            }
            press(&mut app, KeyCode::Esc);
        }
    }

    #[test]
    fn eras_popup_lists_conditions_and_advances() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('e'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("1825-1850  Pionniers") && text.contains("époque actuelle"),
            "{text}"
        );
        press(&mut app, KeyCode::Down);
        let text = screen(&mut app, 120, 40);
        for expected in [
            "Pour y entrer",
            "Recherche : Signaleurs de voie",
            "0 / 15 000",
            "Investissement : 40 000 €",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("ne sont pas réunies"));

        app.world
            .research_done
            .extend(["hand_signals", "multitubular_boiler", "covered_carriages"].map(String::from));
        app.world.passengers_carried = 15_000;
        app.world.money = 50_000.0;
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.era, "steam");
        press(&mut app, KeyCode::Esc);
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("1850") && text.contains("Âge d'or de la vapeur"),
            "{text}"
        );
        assert!(
            text.contains("Nouvelle époque") && text.contains("d'avance"),
            "{text}"
        );
    }

    #[test]
    fn upgrades_and_fuel_can_be_bought() {
        let mut app = app(false, ColorMode::Color);
        enter(&mut app, "steam");
        app.world.research_done.insert("bessemer_steel".into());
        app.world.money = 20_000.0;

        press(&mut app, KeyCode::Char('u'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Rails en acier")
                && text.contains("7 600 €")
                && text.contains("disponible"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(
            app.world.lines["stockton_darlington"]
                .upgrades
                .contains("steel_rails")
        );
        assert!(screen(&mut app, 120, 40).contains("installée"));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("déjà installée"));
        press(&mut app, KeyCode::Esc);

        press(&mut app, KeyCode::Char('c'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Charbon")
                && text.contains("stock bas")
                && text.contains("Un lot de 10 t"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.world.fuel_stock("coal"), 110.0);
        press(&mut app, KeyCode::Esc);
        assert!(screen(&mut app, 120, 40).contains("Charbon 110 t"));
    }

    #[test]
    fn fleet_popup_sells_and_replaces_trains() {
        let mut app = app(false, ColorMode::Color);
        app.world.money = 1000.0;
        let data = app.data.clone();
        app.world
            .buy_locomotive(&data, "locomotion_1", "stockton_darlington")
            .unwrap();
        app.world.dispatch(&data, 1).unwrap();
        app.update(0.1);

        press(&mut app, KeyCode::Char('v'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Flotte : 2 trains sur le réseau")
                && text.contains("à quai à Stockton")
                && text.contains("sans conducteur"),
            "{text}"
        );
        // Le train en route sera revendu à son arrivée ; une seconde fois, on annule.
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(
            app.world.trains[0].order,
            Some(crate::sim::TrainOrder::Sell)
        );
        assert!(screen(&mut app, 120, 40).contains("Vente prévue"));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.world.trains[0].order, None);

        // Remplacer la machine à quai : r ouvre le choix, Entrée remplace.
        press(&mut app, KeyCode::Down);
        app.world.research_done.insert("multitubular_boiler".into());
        app.world.money = 5000.0;
        press(&mut app, KeyCode::Char('r'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("Remplacer Locomotion n°1 #2") && text.contains("machine actuelle"),
            "{text}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("déjà tiré par cette machine"));
        let rocket = data
            .locomotives
            .iter()
            .position(|l| l.id == "rocket")
            .unwrap();
        app.open(app::Popup::Replace {
            train: 2,
            fleet: 1,
            sel: rocket,
        });
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world.train(2).unwrap().loco, "rocket");
        assert_eq!(app.popup, app::Popup::Fleet { sel: 1 });

        // Revente sur-le-champ du train à quai.
        let money = app.world.money;
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.world.trains.len(), 1);
        let refund = data.locomotive("rocket").unwrap().cost * data.balance.resale_ratio;
        assert!((app.world.money - money - refund).abs() < 0.05);
        press(&mut app, KeyCode::Enter);
        assert!(app.follow && app.popup == app::Popup::None);
    }

    #[test]
    fn historical_event_must_be_decided() {
        let mut app = app(false, ColorMode::Color);
        app.world.passengers_carried = 250;
        app.update(0.1);
        app.update(0.1);
        assert_eq!(app.popup, app::Popup::Event { sel: 0 });
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("1829")
                && text.contains("Le concours de Rainhill")
                && text.contains("Novelty"),
            "{text}"
        );
        // Ni Échap ni q ne permettent d'esquiver la décision.
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('q'));
        assert!(matches!(app.popup, app::Popup::Event { .. }) && !app.quit);

        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("+500 €")
                && text.contains("Recherche offerte : Chaudière multitubulaire"),
            "{text}"
        );
        assert!(app.world.research_done.contains("multitubular_boiler"));
        press(&mut app, KeyCode::Char(' '));
        app.update(0.1);
        assert_eq!(app.popup, app::Popup::None, "l'événement ne revient pas");
    }

    fn fill(app: &mut App, station: &str, n: f64) {
        app.world
            .stations
            .get_mut(station)
            .unwrap()
            .waiting
            .insert("commuter".into(), n);
    }

    #[test]
    fn board_lists_departures_and_arrivals_with_flaps() {
        let mut app = app(false, ColorMode::Color);
        app.world.money = 1000.0;
        app.world
            .buy_locomotive(&app.data.clone(), "locomotion_1", "stockton_darlington")
            .unwrap();
        fill(&mut app, "stockton", 100.0);
        press(&mut app, KeyCode::Char('b'));
        assert_eq!(app.screen, Screen::Board);
        screen(&mut app, 100, 30);

        // Les palettes n'ont pas encore fini de tourner.
        app.update(0.05);
        let early = screen(&mut app, 100, 30);
        assert!(
            early.contains("GARE DE STOCKTON")
                && early.contains("DEPARTS")
                && early.contains("ARRIVEES")
        );
        assert!(!early.contains("EMBARQUEMENT"), "{early}");
        for _ in 0..40 {
            app.update(0.05);
        }
        let text = screen(&mut app, 100, 30);
        for expected in [
            "08:20  DARLINGTON",
            "LOCOMOTION NO1 #1",
            "LOCOMOTION NO1 #2",
            "EMBARQUEMENT",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }

        // Vu de Darlington : un train annoncé à l'arrivée, puis en retard au départ.
        let data = app.data.clone();
        app.world.dispatch(&data, 1).unwrap();
        press(&mut app, KeyCode::Tab);
        for _ in 0..40 {
            app.update(0.05);
        }
        let text = screen(&mut app, 100, 30);
        assert!(
            text.contains("GARE DE DARLINGTON") && text.contains("A L'HEURE"),
            "{text}"
        );
        assert!(text.contains("STOCKTON        LOCOMOTION NO1 #1"), "{text}");
        for _ in 0..400 {
            app.update(0.25);
        }
        let text = screen(&mut app, 100, 30);
        assert!(
            text.contains("RETARD"),
            "sans départ manuel, le train prend du retard :\n{text}"
        );

        press(&mut app, KeyCode::Char('b'));
        assert_eq!(app.screen, Screen::Map);
    }

    #[test]
    fn fares_and_timetable_are_adjustable() {
        let mut app = app(false, ColorMode::Color);
        press(&mut app, KeyCode::Char('t'));
        let text = screen(&mut app, 120, 40);
        for expected in [
            "Prix des billets",
            "Arrêt en gare",
            "Navetteurs",
            "Touristes",
            "Affaires",
            "Fret",
            "50 %",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Left);
        let line = &app.world.lines["stockton_darlington"];
        assert_eq!((line.fare_level, line.dwell_min), (1.2, 15.0));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("x1,2") && text.contains("15 min") && text.contains("+20 %"),
            "{text}"
        );
        // L'explication du réglage choisi s'affiche en entier, même longue.
        app.world.research_done.insert("hand_signals".into());
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("départ automatique") && text.contains("ou dès que le train est plein."),
            "{text}"
        );
        press(&mut app, KeyCode::Up);
        assert!(
            screen(&mut app, 120, 40)
                .contains("20 % plus cher : moins de monde, plus de recette par billet.")
        );
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.popup, app::Popup::None);
        assert!(screen(&mut app, 120, 40).contains("Clients 50 %"));
    }

    #[test]
    fn welcome_back_summarises_the_absence() {
        let mut app = app(false, ColorMode::Color);
        app.world.research_done.insert("hand_signals".into());
        let data = app.data.clone();
        let short = crate::sim::catch_up(&mut app.world, &data, 20.0);
        app.welcome_back(short);
        assert_eq!(
            app.popup,
            app::Popup::None,
            "une absence de 20 s ne mérite pas de résumé"
        );

        let report = crate::sim::catch_up(&mut app.world, &data, 2.0 * 3600.0);
        app.welcome_back(report);
        let text = screen(&mut app, 120, 40);
        for expected in [
            "Pendant ton absence",
            "Tu es parti 2 h 00",
            "trajets",
            "Trésorerie : +",
        ] {
            assert!(
                text.contains(expected),
                "« {expected} » absent de :\n{text}"
            );
        }
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.popup, app::Popup::None);
    }

    #[test]
    fn read_only_viewer_cannot_change_the_game() {
        let mut app = app(false, ColorMode::Color);
        app.read_only = true;
        let before = app.world.clone();
        for key in ['d', ' ', 'a', 'r', 't', 'm', 'p'] {
            press(&mut app, KeyCode::Char(key));
        }
        assert_eq!(app.world, before);
        assert_eq!(app.popup, app::Popup::None);
        assert!(!app.paused);
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("LECTURE SEULE") && text.contains("Lecture seule"),
            "{text}"
        );
        // Regarder reste permis.
        press(&mut app, KeyCode::Char('b'));
        assert_eq!(app.screen, Screen::Board);
    }

    #[test]
    fn status_line_is_short_and_ascii_safe() {
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.test_driver(1);
        world.money = 1240.7;
        assert_eq!(
            status_line(&world, &data, false),
            "1 240 € │ 0 train en route │ +0,0 €/s"
        );
        world.dispatch(&data, 1).unwrap();
        crate::sim::advance(&mut world, &data, 2400.0);
        let line = status_line(&world, &data, true);
        assert!(
            line.starts_with("1 2")
                && line.contains("| 1 train en route | ")
                && line.ends_with("0,0 E/s"),
            "{line}"
        );
        assert!(line.is_ascii() && !line.contains('\n'));
    }

    /// Aperçu manuel : `cargo test preview -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn preview() {
        let mut app = app(false, ColorMode::Color);
        app.data.events.clear();
        app.update(0.1);
        app.world.money = 9000.0;
        press(&mut app, KeyCode::Char('s'));
        println!("{}\n", screen(&mut app, 130, 36));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Enter);
        println!("{}", screen(&mut app, 130, 36));
    }

    #[test]
    fn the_map_draws_real_europe_in_ascii() {
        for ascii in [false, true] {
            let mut app = app(ascii, ColorMode::Color);
            app.update(0.1);
            screen(&mut app, 140, 44);
            press(&mut app, KeyCode::Char('3'));
            let text = screen(&mut app, 140, 44);
            for name in ["ROYAUME-UNI", "FRANCE", "ALLEMAGNE", "ESPAGNE", "RUSSIE"] {
                assert!(text.contains(name), "« {name} » absent de :\n{text}");
            }
            // Terres en damier, côtes en caractères ASCII, frontières en pointillés.
            for glyph in [". . .", "-", "|", "/", "\\", ":"] {
                assert!(text.contains(glyph), "« {glyph} » absent de :\n{text}");
            }
            // Le fond de carte n'utilise jamais le braille, réservé au réseau.
            let braille = text
                .chars()
                .filter(|c| ('\u{2800}'..='\u{28ff}').contains(c))
                .count();
            assert!(braille < 40, "{braille} caractères braille :\n{text}");
        }
        // En vue pays, les fleuves et leur nom apparaissent.
        let mut app = app(false, ColorMode::Color);
        app.update(0.1);
        screen(&mut app, 140, 44);
        press(&mut app, KeyCode::Char('2'));
        let text = screen(&mut app, 140, 44);
        assert!(
            text.contains('~')
                && ["Tamise", "Rhin", "Elbe", "Seine"]
                    .iter()
                    .any(|r| text.contains(r)),
            "{text}"
        );
    }

    /// Aperçu de la carte aux trois niveaux de zoom :
    /// `cargo test map_preview -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn map_preview() {
        let mut app = app(false, ColorMode::Color);
        app.update(0.1);
        app.popup_msg = None;
        screen(&mut app, 140, 44);
        for key in ['3', '2', '1'] {
            press(&mut app, KeyCode::Char(key));
            println!("{}\n", screen(&mut app, 140, 44));
        }
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
            for popup in ['t', 'e', 'u', 'c', 'v', 'r', 'n', 'g', 'o', 's'] {
                press(&mut app, KeyCode::Char(popup));
                screen(&mut app, w, h);
                press(&mut app, KeyCode::End);
                screen(&mut app, w, h);
                press(&mut app, KeyCode::Esc);
            }
            app.world.event_pending = Some("rainhill_trials".into());
            app.update(0.0);
            screen(&mut app, w, h);
            press(&mut app, KeyCode::Enter);
            screen(&mut app, w, h);
            press(&mut app, KeyCode::Enter);
            app.world.events_done.clear();
            press(&mut app, KeyCode::Char('b'));
            app.update(0.1);
            screen(&mut app, w, h);
            press(&mut app, KeyCode::Char('b'));
        }
        assert!(!app.quit);
    }
}
