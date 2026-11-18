//! Mode démon : la partie tourne en arrière-plan, sans interface.
//!
//! Le démon tient le verrou de la sauvegarde : pendant qu'il tourne, les
//! autres commandes ouvrent la partie en lecture seule.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::data::GameData;
use crate::notify;
use crate::save::{self, SaveLock};
use crate::sim::{World, advance};

const PID_FILE: &str = "daemon.pid";
const TICK: Duration = Duration::from_millis(500);
const AUTOSAVE: Duration = Duration::from_secs(10);
/// Au-delà, on considère que la machine a été suspendue : pas de rattrapage brutal.
const MAX_TICK_S: f64 = 5.0;

fn pid_path(dir: &Path) -> PathBuf {
    dir.join(PID_FILE)
}

fn read_pid(dir: &Path) -> Option<u32> {
    fs::read_to_string(pid_path(dir)).ok()?.trim().parse().ok()
}

/// Identifiant du démon s'il tourne : un fichier de PID et un verrou tenu.
pub fn running(dir: &Path) -> Option<u32> {
    let pid = read_pid(dir)?;
    match SaveLock::acquire(dir) {
        // Verrou libre : le fichier de PID est un reste d'un arrêt brutal.
        Ok(_) => {
            let _ = fs::remove_file(pid_path(dir));
            None
        }
        Err(_) => Some(pid),
    }
}

/// Fait avancer une partie en temps réel et signale ce qui mérite une notification.
pub struct Runner {
    pub world: World,
    data: GameData,
    era_ready: bool,
}

impl Runner {
    pub fn new(world: World, data: GameData) -> Runner {
        let era_ready = Self::era_ready(&world, &data);
        Runner {
            world,
            data,
            era_ready,
        }
    }

    fn era_ready(world: &World, data: &GameData) -> bool {
        world
            .next_era(data)
            .is_some_and(|s| s.ready() && s.affordable)
    }

    /// Avance de `real_s` secondes réelles ; renvoie les notifications à envoyer.
    pub fn tick(&mut self, real_s: f64) -> Vec<notify::Notice> {
        let events = advance(
            &mut self.world,
            &self.data,
            real_s * self.data.balance.time_scale,
        );
        let mut notices: Vec<notify::Notice> = events
            .iter()
            .filter_map(|e| notify::notice_for(e, &self.data))
            .collect();
        // L'époque suivante devient accessible : on le dit une fois.
        let ready = Self::era_ready(&self.world, &self.data);
        if ready
            && !self.era_ready
            && let Some(status) = self.world.next_era(&self.data)
        {
            notices.push(notify::era_notice(&status.era.name.fr));
        }
        self.era_ready = ready;
        notices
    }
}

/// Boucle du démon, au premier plan : tourne jusqu'à un signal d'arrêt.
pub fn run(dir: &Path, data: GameData, notifications: bool) -> Result<()> {
    let _lock = SaveLock::acquire(dir)?;
    let mut world = match save::load_current(dir, &data)? {
        Some((world, _)) => world,
        None => World::new(&data),
    };
    // Sans joueur pour choisir, la compagnie reste dans son pays actuel.
    world.choosing_home = false;
    fs::write(pid_path(dir), std::process::id().to_string())
        .with_context(|| format!("écriture de {} impossible", pid_path(dir).display()))?;

    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst))
        .context("installation du gestionnaire de signaux impossible")?;

    let mut runner = Runner::new(world, data);
    let mut last = Instant::now();
    let mut last_save = Instant::now();
    let mut result = Ok(());
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(TICK);
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64().min(MAX_TICK_S);
        last = now;
        for notice in runner.tick(dt) {
            if notifications {
                notify::send(&notice);
            }
        }
        if last_save.elapsed() >= AUTOSAVE {
            last_save = Instant::now();
            // Une sauvegarde ratée est retentée au cycle suivant ; on garde la dernière erreur.
            result = save::save(dir, &runner.world);
        }
    }
    let saved = save::save(dir, &runner.world);
    let _ = fs::remove_file(pid_path(dir));
    saved.and(result)
}

/// Lance le démon en arrière-plan ; renvoie son identifiant de processus.
pub fn start(dir: &Path, notifications: bool) -> Result<u32> {
    if let Some(pid) = running(dir) {
        bail!("le démon tourne déjà (processus {pid})");
    }
    // Le verrou peut aussi être tenu par une partie ouverte dans un terminal.
    drop(SaveLock::acquire(dir).context("ferme d'abord la partie ouverte dans un autre terminal")?);

    let exe = std::env::current_exe().context("chemin de l'exécutable introuvable")?;
    let mut cmd = Command::new(exe);
    cmd.args(["daemon", "run"]);
    if !notifications {
        cmd.arg("--no-notify");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        // Groupe de processus à part : fermer le terminal n'arrête pas le démon.
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().context("lancement du démon impossible")?;

    // On attend qu'il ait pris la main, ou qu'il ait échoué.
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(pid) = running(dir) {
            return Ok(pid);
        }
        if let Ok(Some(status)) = child.try_wait() {
            bail!(
                "le démon s'est arrêté aussitôt ({status}) ; lance `rail daemon run` pour voir l'erreur"
            );
        }
    }
    bail!("le démon n'a pas démarré à temps")
}

/// Arrête le démon et attend qu'il ait sauvegardé. Faux s'il ne tournait pas.
pub fn stop(dir: &Path) -> Result<bool> {
    let Some(pid) = running(dir) else {
        return Ok(false);
    };
    let status = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    } else {
        Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
    };
    if !status
        .context("envoi du signal d'arrêt impossible")?
        .success()
    {
        bail!("impossible d'arrêter le processus {pid}");
    }
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(100));
        if running(dir).is_none() {
            return Ok(true);
        }
    }
    bail!("le démon (processus {pid}) ne s'est pas arrêté")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner() -> Runner {
        let data = GameData::embedded().unwrap();
        Runner::new(World::new(&data), data)
    }

    #[test]
    fn runner_advances_in_real_time_and_reports_notable_events() {
        let mut r = runner();
        r.world.money = 1000.0;
        let data = GameData::embedded().unwrap();
        r.world.start_research(&data, "hand_signals").unwrap();
        let clock = r.world.clock;
        // 4 heures de jeu de recherche : 240 secondes réelles à l'échelle 60.
        let notices = r.tick(239.0);
        assert!((r.world.clock - clock - 239.0 * 60.0).abs() < 1e-6);
        assert!(notices.is_empty(), "{notices:?}");
        let notices = r.tick(2.0);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].title, "Recherche terminée");
    }

    #[test]
    fn next_era_within_reach_is_announced_once() {
        let mut r = runner();
        r.world
            .research_done
            .extend(["hand_signals", "multitubular_boiler", "covered_carriages"].map(String::from));
        r.world.events_done.insert("rainhill_trials".into());
        r.world.passengers_carried = 20_000;
        r.world.money = 39_990.0;
        assert!(r.tick(1.0).is_empty(), "il manque encore un peu d'argent");
        r.world.money = 60_000.0;
        let notices = r.tick(1.0);
        assert_eq!(notices.len(), 1);
        assert!(notices[0].body.contains("Âge d'or de la vapeur"));
        assert!(r.tick(1.0).is_empty(), "annoncé une seule fois");
    }

    #[test]
    fn stale_pid_file_does_not_count_as_a_running_daemon() {
        let dir = std::env::temp_dir().join(format!("rainhill-test-{}-daemon", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(running(&dir), None);
        fs::write(pid_path(&dir), "424242").unwrap();
        assert_eq!(running(&dir), None, "verrou libre : le démon n'est plus là");
        assert!(
            !pid_path(&dir).exists(),
            "le fichier de PID périmé est nettoyé"
        );
        assert!(!stop(&dir).unwrap());

        // Verrou tenu et PID présent : le démon est considéré comme vivant.
        let _lock = SaveLock::acquire(&dir).unwrap();
        fs::write(pid_path(&dir), "424242").unwrap();
        assert_eq!(running(&dir), Some(424242));
        drop(_lock);
        fs::remove_dir_all(&dir).unwrap();
    }
}
