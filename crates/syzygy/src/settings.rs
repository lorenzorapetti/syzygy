//! The user's preferences. Loaded once at boot; `update` is the only writer,
//! and it saves through a `Task` that runs [`Settings::save`].
//!
//! The Session (tokens, user id, country) is not here: it lives in its own
//! file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syzygy_store::Store;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    /// The highest quality to request, as TIDAL names it.
    pub max_quality: String,
    pub volume_normalization: bool,
    pub exclusive_mode: bool,
    pub exclusive_device: Option<String>,
    pub bit_perfect: bool,
    pub gapless: bool,
    pub autoplay: bool,
    pub allow_explicit: bool,
    /// Report plays to TIDAL so they show in Recently Played.
    pub report_plays: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            max_quality: "HI_RES_LOSSLESS".to_string(),
            volume_normalization: false,
            exclusive_mode: false,
            exclusive_device: None,
            bit_perfect: false,
            gapless: true,
            autoplay: false,
            allow_explicit: true,
            report_plays: true,
        }
    }
}

impl Settings {
    /// Read the settings file. A missing or unreadable file gives the
    /// defaults, so a bad file never stops syzygy from starting. An
    /// unreadable file is renamed to `<name>.unreadable` first, so saving the
    /// defaults later never destroys it.
    pub fn load(store: &Store, path: &Path) -> Self {
        match store.read_json(path) {
            Ok(Some(settings)) => settings,
            Ok(None) => Self::default(),
            Err(e) => {
                let mut aside = path.as_os_str().to_owned();
                aside.push(".unreadable");
                log::warn!(
                    "Could not read {}, using defaults and keeping it as {}: {e}",
                    path.display(),
                    Path::new(&aside).display()
                );
                if let Err(e) = std::fs::rename(path, &aside) {
                    log::warn!("Could not move {} aside: {e}", path.display());
                }
                Self::default()
            }
        }
    }

    /// Encrypt and write the settings off the UI thread.
    pub async fn save(self, store: Store, path: PathBuf) -> Result<(), Arc<syzygy_store::Error>> {
        tokio::task::spawn_blocking(move || store.write_json(&path, &self))
            .await
            .unwrap_or_else(|e| Err(std::io::Error::other(e).into()))
            .map_err(Arc::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syzygy_store::{KeySource, Store};

    fn store(dir: &std::path::Path) -> Store {
        Store::open(&KeySource {
            keyring: None,
            key_file: dir.join("master.key"),
        })
        .unwrap()
    }

    // Play reporting ships on. Existing configs predate the field, so the serde
    // default is what governs upgrades — not just Settings::default().
    #[test]
    fn report_plays_defaults_on() {
        assert!(Settings::default().report_plays);
        let upgraded: Settings = serde_json::from_str("{}").unwrap();
        assert!(upgraded.report_plays);
    }

    #[test]
    fn missing_fields_take_the_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"volume": 0.25}"#).unwrap();

        assert_eq!(
            parsed,
            Settings {
                volume: 0.25,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn a_missing_settings_file_loads_the_defaults() {
        let dir = tempfile::tempdir().unwrap();

        let loaded = Settings::load(&store(dir.path()), &dir.path().join("settings.json"));

        assert_eq!(loaded, Settings::default());
    }

    #[test]
    fn an_unreadable_settings_file_loads_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"garbage").unwrap();

        let loaded = Settings::load(&store(dir.path()), &path);

        assert_eq!(loaded, Settings::default());
    }

    /// The defaults are saved on quit, so an unreadable file must be kept
    /// aside rather than silently overwritten.
    #[test]
    fn an_unreadable_settings_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"garbage").unwrap();

        Settings::load(&store(dir.path()), &path);

        assert!(!path.exists());
        assert_eq!(
            std::fs::read(dir.path().join("settings.json.unreadable")).unwrap(),
            b"garbage"
        );
    }

    #[test]
    fn saved_settings_load_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let path = dir.path().join("settings.json");
        let settings = Settings {
            volume: 0.4,
            gapless: false,
            exclusive_device: Some("hw:1,0".into()),
            ..Settings::default()
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime
            .block_on(settings.clone().save(store.clone(), path.clone()))
            .unwrap();

        assert_eq!(Settings::load(&store, &path), settings);
    }
}
