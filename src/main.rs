use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use rainhill::data::GameData;
use rainhill::save::{self, SaveLock};
use rainhill::sim::World;
use rainhill::ui::{self, ColorMode, UiOptions};

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

    /// Recommence une partie (l'ancienne sauvegarde est gardée en save.json.bak)
    #[arg(long, global = true)]
    new: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Interface interactive (par défaut)
    Play,
    /// Carte du réseau en direct, trains animés
    Map,
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

fn run(cli: Cli) -> Result<()> {
    let dir = save::data_dir()?;
    let data = GameData::load(Some(&dir.join("mods")))?;

    match cli.command.unwrap_or(Command::Play) {
        // En phase 1, l'interface complète se résume à l'écran carte.
        Command::Play | Command::Map => {
            if !std::io::stdout().is_terminal() {
                bail!("rail a besoin d'un terminal interactif");
            }
            let _lock = SaveLock::acquire(&dir)?;
            let world = if cli.new {
                save::backup(&dir)?;
                World::new(&data)
            } else {
                match save::load(&dir)? {
                    Some(mut world) => {
                        world.reconcile(&data);
                        world
                    }
                    None => World::new(&data),
                }
            };
            let color =
                if cli.no_color || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
                    ColorMode::NoColor
                } else if cli.colorblind {
                    ColorMode::Colorblind
                } else {
                    ColorMode::Color
                };
            let opts = UiOptions {
                ascii: cli.ascii || !locale_is_utf8(),
                color,
            };
            ui::run(world, data, &dir, opts)
        }
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
