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
    /// Which disc of its album the track is on, from 1.
    pub volume: u32,
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
            volume: track.volume_number.unwrap_or(1).max(1),
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
