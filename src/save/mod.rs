//! Sauvegarde locale : un fichier JSON versionné, écrit de façon atomique,
//! et un verrou pour qu'un seul processus fasse avancer la partie.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::sim::{OfflineReport, SAVE_VERSION, World, catch_up};

const SAVE_FILE: &str = "save.json";
const LOCK_FILE: &str = "rainhill.lock";

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    /// Secondes Unix au moment de l'écriture (pour les gains hors ligne).
    saved_at: u64,
    world: World,
}

/// Dossier de données : `$RAINHILL_DATA_DIR`, sinon l'emplacement standard
/// de la plateforme (`~/.local/share/rainhill` sous Linux).
pub fn data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("RAINHILL_DATA_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let dirs = ProjectDirs::from("", "", "rainhill")
        .context("impossible de déterminer le dossier de données de l'utilisateur")?;
    Ok(dirs.data_dir().to_path_buf())
}

pub fn save_path(dir: &Path) -> PathBuf {
    dir.join(SAVE_FILE)
}

pub fn save(dir: &Path, world: &World) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("création de {} impossible", dir.display()))?;
    let file = SaveFile {
        version: SAVE_VERSION,
        saved_at: unix_now(),
        world: world.clone(),
    };
    let json = serde_json::to_string_pretty(&file).context("sérialisation de la sauvegarde")?;
    let path = save_path(dir);
    let tmp = dir.join(format!("{SAVE_FILE}.tmp"));
    fs::write(&tmp, json).with_context(|| format!("écriture de {} impossible", tmp.display()))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("écriture de {} impossible", path.display()))?;
    Ok(())
}

const SETTINGS_FILE: &str = "settings.json";

fn yes() -> bool {
    true
}

/// Préférences d'affichage, gardées d'une partie à l'autre (hors sauvegarde).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Animation de la locomotive façon `sl` entre les écrans.
    #[serde(default = "yes")]
    pub transitions: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { transitions: true }
    }
}

/// Préférences enregistrées ; les valeurs par défaut si le fichier manque ou est illisible.
pub fn load_settings(dir: &Path) -> Settings {
    fs::read_to_string(dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, settings: &Settings) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("création de {} impossible", dir.display()))?;
    let path = dir.join(SETTINGS_FILE);
    let json = serde_json::to_string_pretty(settings).context("sérialisation des préférences")?;
    fs::write(&path, json).with_context(|| format!("écriture de {} impossible", path.display()))
}

/// Partie relue sur le disque, avec l'heure de son écriture (secondes Unix).
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub world: World,
    pub saved_at: u64,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Charge la sauvegarde et la remet à l'heure : conversion de format, puis
/// rattrapage du temps écoulé depuis son écriture. `Ok(None)` s'il n'y en a pas.
pub fn load_current(dir: &Path, data: &GameData) -> Result<Option<(World, OfflineReport)>> {
    let Some(Loaded {
        mut world,
        saved_at,
    }) = load(dir)?
    else {
        return Ok(None);
    };
    world.reconcile(data);
    let away = unix_now().saturating_sub(saved_at) as f64;
    let report = catch_up(&mut world, data, away);
    Ok(Some((world, report)))
}

/// Charge la sauvegarde telle quelle. `Ok(None)` s'il n'y en a pas encore.
pub fn load(dir: &Path) -> Result<Option<Loaded>> {
    let path = save_path(dir);
    if !path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(&path)
        .with_context(|| format!("lecture de {} impossible", path.display()))?;
    let file: SaveFile = serde_json::from_str(&json).with_context(|| {
        format!(
            "{} est illisible ; relance avec --new pour repartir de zéro (l'ancien fichier sera gardé en .bak)",
            path.display()
        )
    })?;
    if file.version > SAVE_VERSION {
        bail!(
            "{} vient d'une version plus récente du jeu (format {}, connu : {})",
            path.display(),
            file.version,
            SAVE_VERSION
        );
    }
    Ok(Some(Loaded {
        world: file.world,
        saved_at: file.saved_at,
    }))
}

/// Met de côté la sauvegarde existante (`save.json.bak`) avant une nouvelle partie.
pub fn backup(dir: &Path) -> Result<()> {
    let path = save_path(dir);
    if path.exists() {
        let bak = dir.join(format!("{SAVE_FILE}.bak"));
        fs::rename(&path, &bak)
            .with_context(|| format!("renommage de {} impossible", path.display()))?;
    }
    Ok(())
}

/// Verrou exclusif sur le dossier de sauvegarde, relâché à la destruction.
pub struct SaveLock {
    _file: File,
}

impl SaveLock {
    pub fn acquire(dir: &Path) -> Result<SaveLock> {
        fs::create_dir_all(dir)
            .with_context(|| format!("création de {} impossible", dir.display()))?;
        let path = dir.join(LOCK_FILE);
        let file = File::create(&path)
            .with_context(|| format!("ouverture de {} impossible", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(SaveLock { _file: file }),
            Err(TryLockError::WouldBlock) => {
                bail!("une autre instance de rail fait déjà tourner cette partie")
            }
            Err(TryLockError::Error(e)) => {
                Err(e).with_context(|| format!("verrouillage de {} impossible", path.display()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::sim::advance;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rainhill-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_then_load_restores_the_same_world() {
        let dir = temp_dir("roundtrip");
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.money = 1234.5;
        world.research_done.insert("hand_signals".into());
        advance(&mut world, &data, 7777.0);

        assert!(load(&dir).unwrap().is_none());
        save(&dir, &world).unwrap();
        let loaded = load(&dir).unwrap().unwrap();
        assert_eq!(loaded.world, world);
        assert!(unix_now() - loaded.saved_at < 5);

        // Rechargée aussitôt, la partie n'a presque pas bougé.
        let (current, report) = load_current(&dir, &data).unwrap().unwrap();
        assert!(report.away_s < 5.0);
        assert!(current.clock - world.clock <= 5.0 * data.balance.time_scale);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_save_is_reported_and_backup_keeps_it() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(save_path(&dir), "{ pas du json").unwrap();
        assert!(load(&dir).is_err());
        backup(&dir).unwrap();
        assert!(load(&dir).unwrap().is_none());
        assert!(dir.join("save.json.bak").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn time_away_is_caught_up_on_load() {
        let dir = temp_dir("offline");
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.test_driver(1);
        world.research_done.insert("hand_signals".into());
        save(&dir, &world).unwrap();

        // On antidate la sauvegarde d'une heure.
        let path = save_path(&dir);
        let mut file: SaveFile = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        file.saved_at -= 3600;
        fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

        let (current, report) = load_current(&dir, &data).unwrap().unwrap();
        assert!((3600.0..3605.0).contains(&report.away_s));
        assert!(report.trips > 20 && current.money > world.money);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_one_lock_holder_at_a_time() {
        let dir = temp_dir("lock");
        let first = SaveLock::acquire(&dir).unwrap();
        assert!(SaveLock::acquire(&dir).is_err());
        drop(first);
        assert!(SaveLock::acquire(&dir).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }
}
