//! A track as a list row draws it, with what it links to.

use serde_json::Value;
use syzygy_tidal::models::{TidalArtist, TidalTrack};

use crate::home_feed::Cover;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: u64,
    /// The title with its version, as "Song (Live)".
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    /// In seconds.
    pub duration: u32,
    pub explicit: bool,
    /// Whether TIDAL streams it now. False when it says it isn't ready or
    /// isn't allowed; a list that doesn't say counts as available.
    pub available: bool,
    /// Which disc of its album the track is on, from 1.
    pub volume: u32,
    /// When it was added to the playlist it's read from, as TIDAL sends it.
    pub date_added: Option<String>,
    /// The id of its Track radio, when the list it came in says. Many
    /// don't; reading the track on its own does.
    pub track_radio: Option<String>,
}

/// An artist a track or album credits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistRef {
    pub id: u64,
    pub name: String,
}

/// The album a track is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumRef {
    pub id: u64,
    pub title: String,
    pub cover: Option<Cover>,
}

impl Track {
    /// A track from a loosely typed list item, either bare or in a v2
    /// `{ "data": … }` wrapper. `None` if it isn't one.
    pub(crate) fn from_value(item: &Value) -> Option<Track> {
        let item = item
            .get("data")
            .filter(|data| data.is_object())
            .unwrap_or(item);
        serde_json::from_value::<TidalTrack>(item.clone())
            .ok()
            .map(Track::from)
    }
}

impl From<TidalTrack> for Track {
    fn from(track: TidalTrack) -> Self {
        Track {
            id: track.id,
            title: with_version(track.title, track.version.as_deref()),
            artists: artists(track.artists, track.artist),
            album: track.album.map(|album| AlbumRef {
                id: album.id,
                title: album.title,
                cover: album.cover.map(Cover::Image),
            }),
            duration: track.duration,
            explicit: track.explicit.unwrap_or(false),
            available: track.stream_ready != Some(false) && track.allow_streaming != Some(false),
            volume: track.volume_number.unwrap_or(1).max(1),
            date_added: track.date_added,
            track_radio: track
                .mixes
                .as_ref()
                .and_then(|mixes| mixes.get("TRACK_MIX"))
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }
}

/// `artists` when TIDAL sends the list, else the one `artist`.
pub(crate) fn artists(
    artists: Option<Vec<TidalArtist>>,
    artist: Option<TidalArtist>,
) -> Vec<ArtistRef> {
    artists
        .filter(|artists| !artists.is_empty())
        .or_else(|| artist.map(|artist| vec![artist]))
        .unwrap_or_default()
        .into_iter()
        .map(|artist| ArtistRef {
            id: artist.id,
            name: artist.name,
        })
        .collect()
}

pub(crate) fn with_version(title: String, version: Option<&str>) -> String {
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(version) if !title.contains(version) => format!("{title} ({version})"),
        _ => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_track_knows_its_track_radio_when_tidal_sends_it() {
        let track = Track::from_value(&json!({
            "id": 11,
            "title": "Hunter",
            "duration": 255,
            "mixes": { "TRACK_MIX": "0123abc", "MASTER_TRACK_MIX": "0456def" },
        }))
        .expect("a track");

        assert_eq!(track.track_radio.as_deref(), Some("0123abc"));
    }

    #[test]
    fn a_track_without_mixes_has_no_track_radio_yet() {
        let track = Track::from_value(&json!({ "id": 11, "title": "Hunter", "duration": 255 }))
            .expect("a track");

        assert_eq!(track.track_radio, None);
    }

    #[test]
    fn a_track_tidal_wont_stream_is_unavailable() {
        let unavailable = |fields: serde_json::Value| {
            let mut item = json!({ "id": 11, "title": "Hunter", "duration": 255 });
            item.as_object_mut()
                .expect("an object")
                .extend(fields.as_object().expect("an object").clone());
            !Track::from_value(&item).expect("a track").available
        };

        assert!(unavailable(json!({ "streamReady": false })));
        assert!(unavailable(json!({ "allowStreaming": false })));
        assert!(!unavailable(
            json!({ "streamReady": true, "allowStreaming": true })
        ));
        assert!(!unavailable(json!({})));
    }
}
