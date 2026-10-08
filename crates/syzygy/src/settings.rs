//! The user's preferences. Loaded once at boot; `update` is the only writer,
//! and it saves through a `Task` that runs [`Settings::save`].
//!
//! The Session (tokens, user id, country) is not here: it lives in its own
//! file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syzygy_catalog::TrackSort;
use syzygy_store::Store;
use syzygy_tidal::Quality;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    /// The highest quality to request.
    pub max_quality: Quality,
    pub volume_normalization: bool,
    pub exclusive_mode: bool,
    pub exclusive_device: Option<String>,
    pub bit_perfect: bool,
    pub gapless: bool,
    pub autoplay: bool,
    pub allow_explicit: bool,
    /// Report plays to TIDAL so they show in Recently Played.
    pub report_plays: bool,
    /// Each playlist's track sort, by uuid. A playlist that isn't here is
    /// in its own order.
    pub track_sorts: BTreeMap<String, TrackSort>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            max_quality: Quality::HiResLossless,
            volume_normalization: false,
            exclusive_mode: false,
            exclusive_device: None,
            bit_perfect: false,
            gapless: true,
            autoplay: false,
            allow_explicit: true,
            report_plays: true,
            track_sorts: BTreeMap::new(),
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

    /// Remember `sort` for a playlist; `None` forgets it.
    pub fn set_track_sort(&mut self, uuid: String, sort: Option<TrackSort>) {
        match sort {
            Some(sort) => self.track_sorts.insert(uuid, sort),
            None => self.track_sorts.remove(&uuid),
        };
    }

    /// Encrypt and write the settings off the UI thread.
    pub fn save(
        self,
        store: Store,
        path: PathBuf,
    ) -> impl Future<Output = Result<(), Arc<syzygy_store::Error>>> + use<> {
        crate::persist::write_json(store, path, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syzygy_catalog::{Direction, TrackOrder};
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

    fn by_title() -> TrackSort {
        TrackSort {
            order: TrackOrder::Title,
            direction: Direction::Descending,
        }
    }

    #[test]
    fn a_track_sort_is_kept_per_playlist_until_it_goes_back_to_the_own_order() {
        let mut settings = Settings::default();

        settings.set_track_sort("u-1".into(), Some(by_title()));
        settings.set_track_sort("u-2".into(), Some(by_title()));
        settings.set_track_sort("u-2".into(), None);

        assert_eq!(settings.track_sorts.get("u-1"), Some(&by_title()));
        assert_eq!(settings.track_sorts.get("u-2"), None);
    }

    #[test]
    fn saved_settings_load_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let path = dir.path().join("settings.json");
        let mut settings = Settings {
            volume: 0.4,
            gapless: false,
            exclusive_device: Some("hw:1,0".into()),
            ..Settings::default()
        };
        settings.set_track_sort("u-1".into(), Some(by_title()));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime
            .block_on(settings.clone().save(store.clone(), path.clone()))
            .unwrap();

        assert_eq!(Settings::load(&store, &path), settings);
    }
}
