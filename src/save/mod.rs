//! Sauvegarde locale : un fichier chiffré et versionné, écrit de façon
//! atomique, et un verrou pour qu'un seul processus fasse avancer la partie.
//!
//! Le fichier est chiffré avec ChaCha20-Poly1305 : on ne peut ni le lire ni le
//! retoucher à la main, et la moindre modification le rend illisible. La clé
//! est dans le programme (le jeu doit pouvoir se relire sans mot de passe) :
//! c'est une protection contre la triche ordinaire, pas un coffre-fort.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::sim::{OfflineReport, SAVE_VERSION, World, catch_up};

const SAVE_FILE: &str = "save.dat";
/// En-tête du fichier : identifie le format, et lie les données chiffrées à ce format.
const MAGIC: &[u8; 8] = b"RAILSAV1";
const NONCE_LEN: usize = 12;
/// Clé de chiffrement des sauvegardes.
const KEY: [u8; 32] = [
    0x52, 0x61, 0x69, 0x6e, 0x68, 0x69, 0x6c, 0x6c, 0x9e, 0x37, 0x79, 0xb9, 0x7f, 0x4a, 0x7c, 0x15,
    0xbf, 0x58, 0x47, 0x6d, 0x1c, 0xe4, 0xe5, 0xb9, 0x94, 0xd0, 0x49, 0xbb, 0x13, 0x31, 0x11, 0xeb,
];
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
    let json = serde_json::to_vec(&file).context("sérialisation de la sauvegarde")?;
    let path = save_path(dir);
    let tmp = dir.join(format!("{SAVE_FILE}.tmp"));
    fs::write(&tmp, seal(&json)?)
        .with_context(|| format!("écriture de {} impossible", tmp.display()))?;
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

/// Chiffre des données : en-tête, nonce tiré au hasard, puis texte chiffré.
fn seal(plain: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&KEY));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plain,
                aad: MAGIC,
            },
        )
        .map_err(|_| anyhow!("chiffrement de la sauvegarde impossible"))?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Déchiffre un fichier écrit par `seal` ; `None` s'il a été altéré ou n'en vient pas.
fn open(bytes: &[u8]) -> Option<Vec<u8>> {
    let rest = bytes.strip_prefix(MAGIC.as_slice())?;
    if rest.len() < NONCE_LEN {
        return None;
    }
    let (nonce, sealed) = rest.split_at(NONCE_LEN);
    ChaCha20Poly1305::new(Key::from_slice(&KEY))
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad: MAGIC,
            },
        )
        .ok()
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
    let bytes =
        fs::read(&path).with_context(|| format!("lecture de {} impossible", path.display()))?;
    let json = open(&bytes).with_context(|| {
        format!(
            "{} est altérée ou corrompue ; relance avec --new pour repartir de zéro (l'ancien fichier sera gardé en .bak)",
            path.display()
        )
    })?;
    let file: SaveFile = serde_json::from_slice(&json).with_context(|| {
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

/// Met de côté la sauvegarde existante (`save.dat.bak`) avant une nouvelle partie.
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
        assert!(dir.join("save.dat.bak").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_save_is_encrypted_and_any_tampering_is_refused() {
        let dir = temp_dir("sealed");
        let data = GameData::embedded().unwrap();
        let mut world = World::new(&data);
        world.money = 123_456.0;
        save(&dir, &world).unwrap();
        let bytes = fs::read(save_path(&dir)).unwrap();
        assert!(bytes.starts_with(MAGIC));
        // Rien de lisible : ni l'argent, ni les noms du JSON.
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("123456") && !text.contains("money"),
            "{text}"
        );
        // Deux écritures du même monde ne donnent pas le même fichier (nonce aléatoire).
        save(&dir, &world).unwrap();
        assert_ne!(fs::read(save_path(&dir)).unwrap(), bytes);
        assert_eq!(load(&dir).unwrap().unwrap().world, world);

        // Un seul octet changé, et la sauvegarde est refusée.
        let mut altered = fs::read(save_path(&dir)).unwrap();
        let last = altered.len() - 1;
        altered[last] ^= 1;
        fs::write(save_path(&dir), &altered).unwrap();
        let err = load(&dir).unwrap_err().to_string();
        assert!(err.contains("altérée"), "{err}");
        // Un JSON en clair (ancienne version de développement) l'est aussi.
        fs::write(save_path(&dir), serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(load(&dir).is_err());
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
        let plain = open(&fs::read(&path).unwrap()).unwrap();
        let mut file: SaveFile = serde_json::from_slice(&plain).unwrap();
        file.saved_at -= 3600;
        fs::write(&path, seal(&serde_json::to_vec(&file).unwrap()).unwrap()).unwrap();

        let (current, report) = load_current(&dir, &data).unwrap().unwrap();
        assert!((3600.0..3605.0).contains(&report.away_s));
        assert!(report.trips > 12 && current.money > world.money);
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
