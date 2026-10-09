//! Who syzygy is on this machine. Engine crates hardcode none of this; the
//! binary passes it in, so syzygy never touches sone's data.

use std::path::PathBuf;
use syzygy_store::{KeySource, KeyringEntry};

pub const APP_NAME: &str = "syzygy";
pub const DISPLAY_NAME: &str = "Syzygy";
pub const APP_ID: &str = "com.lorenzorapetti.syzygy";
const KEYRING_SERVICE: &str = "syzygy";
const KEYRING_ENTRY: &str = "master-key";

/// Where syzygy keeps its files.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `<config_dir>/syzygy`: settings, Session, the queue, the fallback key
    /// file.
    pub config_dir: PathBuf,
    /// `<cache_dir>/syzygy/catalog`: the disk cache of Catalog reads.
    pub catalog_cache_dir: PathBuf,
    /// `<state_dir>/syzygy/logs`, or `<cache_dir>/syzygy/logs` where there is no
    /// state dir.
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn locate() -> Self {
        let under =
            |base: Option<PathBuf>| base.unwrap_or_else(|| PathBuf::from(".")).join(APP_NAME);
        let cache_dir = under(dirs::cache_dir());
        let log_dir = dirs::state_dir()
            .map(|d| d.join(APP_NAME))
            .unwrap_or_else(|| cache_dir.clone())
            .join("logs");
        Self {
            catalog_cache_dir: cache_dir.join("catalog"),
            config_dir: under(dirs::config_dir()),
            log_dir,
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// The Session: tokens, Login method, user id, country.
    pub fn session_file(&self) -> PathBuf {
        self.config_dir.join("session.json")
    }

    /// Where listening is: the queue, History, the current track and the
    /// position.
    pub fn queue_file(&self) -> PathBuf {
        self.config_dir.join("queue.json")
    }

    /// The master key: the OS keyring first, then a 0600 key file.
    pub fn key_source(&self) -> KeySource {
        KeySource {
            keyring: Some(KeyringEntry {
                service: KEYRING_SERVICE.to_string(),
                entry: KEYRING_ENTRY.to_string(),
            }),
            key_file: self.config_dir.join("syzygy.key"),
        }
    }
}
