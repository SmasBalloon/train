use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use rainhill::data::GameData;
use rainhill::save::{self, SaveLock};
use rainhill::sim::World;
use rainhill::ui::{self, ColorMode, Popup, Screen, Session, UiOptions};

/// Rainhill : bâtis un réseau ferroviaire, de 1825 à demain.
#[derive(Parser)]
#[command(name = "rail", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Rendu en ASCII pur (sans caractères Unicode)
    #[arg(long, global = true)]
    ascii: bool,

    /// Désactive les couleurs (la variable NO_COLOR est aussi respectée)
    #[arg(long, global = true)]
    no_color: bool,

    /// Palette adaptée au daltonisme
    #[arg(long, global = true)]
    colorblind: bool,

    /// Coupe l'animation de la locomotive entre les écrans (pour cette fois)
    #[arg(long, global = true)]
    no_anim: bool,
    /// Recommence une partie (l'ancienne sauvegarde est gardée en save.json.bak)
    #[arg(long, global = true)]
    new: bool,
}

#[derive(Subcommand, Clone, Copy)]
enum Command {
    /// Interface interactive (par défaut)
    Play,
    /// Carte du réseau en direct, trains animés
    Map,
    /// Tableau des départs et arrivées, façon gare
    Board,
    /// Une ligne d'état (argent, trains en route, revenu) pour un prompt ou tmux
    Status,
    /// Liste et gestion des trains
    Fleet,
    /// Arbre de recherche
    Research,
    /// Améliorations du réseau
    Upgrade,
}

/// Vrai si la locale annonce de l'UTF-8 (toujours vrai hors Unix).
fn locale_is_utf8() -> bool {
    if !cfg!(unix) {
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .is_some_and(|value| {
            let value = value.to_lowercase();
            value.contains("utf-8") || value.contains("utf8")
        })
}

/// Ouvre l'interface sur l'écran demandé. Si une autre fenêtre fait déjà
/// tourner la partie, elle s'ouvre en lecture seule.
fn interactive(cli: &Cli, screen: Screen, popup: Popup) -> Result<()> {
    if !std::io::stdout().is_terminal() {
        bail!("rail a besoin d'un terminal interactif (essaie `rail status` pour un script)");
    }
    let dir = save::data_dir()?;
    let data = GameData::load(Some(&dir.join("mods")))?;
    // Si une autre fenêtre tient déjà la partie, on l'observe sans l'écrire.
    let lock = SaveLock::acquire(&dir).ok();
    let writable = lock.is_some();
    // Une fenêtre d'action n'a pas de sens en lecture seule.
    let popup = if writable { popup } else { Popup::None };

    // Une nouvelle partie commence par le choix du pays de la compagnie.
    let fresh = || World {
        choosing_home: true,
        ..World::new(&data)
    };
    let (world, report) = if cli.new && writable {
        save::backup(&dir)?;
        (fresh(), None)
    } else {
        match save::load_current(&dir, &data)? {
            Some((world, report)) => (world, Some(report)),
            None => (fresh(), None),
        }
    };
    // Une fenêtre demandée en ligne de commande passe avant le résumé d'absence.
    let report = if popup == Popup::None { report } else { None };
    let session = Session {
        save_dir: &dir,
        writable,
        screen,
        report,
        popup,
    };
    let transitions = save::load_settings(&dir).transitions && !cli.no_anim;
    ui::run(world, data, session, ui_options(cli, transitions))
}

fn ui_options(cli: &Cli, transitions: bool) -> UiOptions {
    let color = if cli.no_color || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        ColorMode::NoColor
    } else if cli.colorblind {
        ColorMode::Colorblind
    } else {
        ColorMode::Color
    };
    UiOptions {
        ascii: cli.ascii || !locale_is_utf8(),
        color,
        transitions,
    }
}

/// Affiche une ligne d'état sans toucher à la sauvegarde.
fn status(cli: &Cli) -> Result<()> {
    let dir = save::data_dir()?;
    let data = GameData::load(Some(&dir.join("mods")))?;
    match save::load_current(&dir, &data)? {
        Some((world, _)) => {
            println!(
                "{}",
                ui::status_line(&world, &data, cli.ascii || !locale_is_utf8())
            );
            Ok(())
        }
        None => bail!("aucune partie en cours (lance `rail` pour commencer)"),
    }
}

fn run(cli: Cli) -> Result<()> {
    let research = Popup::Research {
        sel: 0,
        branch: None,
    };
    match cli.command.unwrap_or(Command::Play) {
        // L'interface complète s'ouvre sur la carte ; le tableau est à une touche.
        Command::Play | Command::Map => interactive(&cli, Screen::Map, Popup::None),
        Command::Board => interactive(&cli, Screen::Board, Popup::None),
        Command::Fleet => interactive(&cli, Screen::Map, Popup::Fleet { sel: 0 }),
        Command::Research => interactive(&cli, Screen::Map, research),
        Command::Upgrade => interactive(&cli, Screen::Map, Popup::Upgrades { sel: 0 }),
        Command::Status => status(&cli),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rail : {e:#}");
            ExitCode::FAILURE
        }
    }
}
