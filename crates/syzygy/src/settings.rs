//! The user's preferences. Loaded once at boot; `update` is the only writer,
//! and it saves through a `Task` that runs [`Settings::save`].
//!
//! The Session (tokens, user id, country) is not here: it lives in its own
//! file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syzygy_catalog::{Kind, LibrarySort, TrackSort};
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
    /// The Loved tracks' sort. Without one they're last added first.
    pub loved_tracks_sort: Option<TrackSort>,
    /// Each Library type's order, in the sidebar and its Library Page. A
    /// type that isn't here is in its default order.
    pub library_sorts: BTreeMap<Kind, LibrarySort>,
    /// The last searches, newest first.
    pub search_history: Vec<String>,
}

/// How many searches the history keeps.
const SEARCH_HISTORY: usize = 10;

/// An order the user picked for one of their lists.
#[derive(Debug, Clone, PartialEq)]
pub enum Sort {
    /// A playlist's track sort; `None` is its own order.
    Playlist(String, Option<TrackSort>),
    /// `None` is last added first.
    LovedTracks(Option<TrackSort>),
    Library(Kind, LibrarySort),
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
            loved_tracks_sort: None,
            library_sorts: BTreeMap::new(),
            search_history: Vec::new(),
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

    /// Remember an order the user picked.
    pub fn save_sort(&mut self, sort: Sort) {
        match sort {
            Sort::Playlist(uuid, Some(sort)) => {
                self.track_sorts.insert(uuid, sort);
            }
            Sort::Playlist(uuid, None) => {
                self.track_sorts.remove(&uuid);
            }
            Sort::LovedTracks(sort) => self.loved_tracks_sort = sort,
            Sort::Library(kind, sort) => {
                self.library_sorts.insert(kind, sort);
            }
        }
    }

    /// Put a search at the front of the history, once whatever its case
    /// (sone's `addToHistory`). Blank searches aren't kept.
    pub fn remember_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        let lower = query.to_lowercase();
        self.search_history
            .retain(|past| past.to_lowercase() != lower);
        self.search_history.insert(0, query.to_string());
        self.search_history.truncate(SEARCH_HISTORY);
    }

    pub fn forget_search(&mut self, query: &str) {
        self.search_history.retain(|past| past != query);
    }

    /// The order a Library type is read in.
    pub fn library_sort(&self, kind: Kind) -> LibrarySort {
        self.library_sorts
            .get(&kind)
            .copied()
            .unwrap_or_else(|| kind.default_sort())
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
    use syzygy_catalog::{Direction, LibraryOrder, TrackOrder};
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

        settings.save_sort(Sort::Playlist("u-1".into(), Some(by_title())));
        settings.save_sort(Sort::Playlist("u-2".into(), Some(by_title())));
        settings.save_sort(Sort::Playlist("u-2".into(), None));

        assert_eq!(settings.track_sorts.get("u-1"), Some(&by_title()));
        assert_eq!(settings.track_sorts.get("u-2"), None);
    }

    #[test]
    fn a_library_type_is_in_its_default_order_until_one_is_picked() {
        let mut settings = Settings::default();
        let by_name = LibrarySort {
            order: LibraryOrder::Name,
            direction: Direction::Ascending,
        };

        settings.save_sort(Sort::Library(Kind::Albums, by_name));

        assert_eq!(settings.library_sort(Kind::Albums), by_name);
        assert_eq!(
            settings.library_sort(Kind::Playlists),
            Kind::Playlists.default_sort()
        );
    }

    #[test]
    fn the_loved_tracks_sort_is_kept_until_it_goes_back_to_the_default() {
        let mut settings = Settings::default();

        settings.save_sort(Sort::LovedTracks(Some(by_title())));
        assert_eq!(settings.loved_tracks_sort, Some(by_title()));

        settings.save_sort(Sort::LovedTracks(None));
        assert_eq!(settings.loved_tracks_sort, None);
    }

    #[test]
    fn a_search_goes_to_the_front_of_the_history_once() {
        let mut settings = Settings::default();

        settings.remember_search("björk");
        settings.remember_search("  Massive Attack ");
        settings.remember_search("Björk");

        assert_eq!(settings.search_history, ["Björk", "Massive Attack"]);
    }

    #[test]
    fn the_history_keeps_the_last_ten_searches() {
        let mut settings = Settings::default();

        for n in 1..=12 {
            settings.remember_search(&format!("query {n}"));
        }

        assert_eq!(settings.search_history.len(), 10);
        assert_eq!(settings.search_history[0], "query 12");
        assert_eq!(settings.search_history[9], "query 3");
    }

    #[test]
    fn a_blank_search_isnt_remembered() {
        let mut settings = Settings::default();

        settings.remember_search("   ");

        assert!(settings.search_history.is_empty());
    }

    #[test]
    fn a_search_can_be_forgotten() {
        let mut settings = Settings::default();
        settings.remember_search("björk");
        settings.remember_search("sigur rós");

        settings.forget_search("björk");

        assert_eq!(settings.search_history, ["sigur rós"]);
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
        settings.save_sort(Sort::Playlist("u-1".into(), Some(by_title())));
        settings.save_sort(Sort::LovedTracks(Some(by_title())));
        settings.remember_search("björk");
        settings.save_sort(Sort::Library(
            Kind::Mixes,
            LibrarySort {
                order: LibraryOrder::MixType,
                direction: Direction::Ascending,
            },
        ));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime
            .block_on(settings.clone().save(store.clone(), path.clone()))
            .unwrap();

        assert_eq!(Settings::load(&store, &path), settings);
    }
}
