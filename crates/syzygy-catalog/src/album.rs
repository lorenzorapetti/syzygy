//! An album's Page: the album, its tracks and the sections under them.

use syzygy_tidal::models::AlbumPageResponse;

use crate::home_feed::{self, Cover, Section};
use crate::track::{self, ArtistRef, Track};

#[derive(Debug, Clone)]
pub struct Album {
    pub title: String,
    pub cover: Option<Cover>,
    pub artists: Vec<ArtistRef>,
    /// As "ALBUM", "EP" or "SINGLE".
    pub kind: Option<String>,
    /// In seconds: TIDAL's figure, else the tracks' sum.
    pub duration: u32,
    pub release_date: Option<String>,
    /// "HI-RES LOSSLESS" or "LOSSLESS" when it's better than High.
    pub quality: Option<&'static str>,
    pub copyright: Option<String>,
    pub tracks: Vec<Track>,
    /// "More by …" and the like.
    pub sections: Vec<Section>,
}

impl From<AlbumPageResponse> for Album {
    fn from(page: AlbumPageResponse) -> Self {
        let album = page.album;
        let tracks: Vec<Track> = page.tracks.into_iter().map(Track::from).collect();
        let duration = album
            .duration
            .unwrap_or_else(|| tracks.iter().map(|t| t.duration).sum());
        let tags = album.media_metadata.map(|m| m.tags).unwrap_or_default();
        let quality = quality(&tags, album.audio_quality.as_deref());
        let sections = page
            .sections
            .iter()
            .filter(|s| !s.items.is_empty())
            .map(|s| home_feed::row(&s.title, &s.section_type, &s.items))
            .collect();
        Album {
            title: track::with_version(album.title, album.version.as_deref()),
            cover: album.cover.map(Cover::Image),
            artists: track::artists(album.artists, album.artist),
            kind: album.album_type,
            duration,
            release_date: album.release_date,
            quality,
            copyright: page.copyright.or(album.copyright),
            tracks,
            sections,
        }
    }
}

/// sone's `getMediaQualityBadge`.
fn quality(tags: &[String], audio_quality: Option<&str>) -> Option<&'static str> {
    let has = |tag: &str| tags.iter().any(|t| t == tag);
    if has("HIRES_LOSSLESS") {
        return Some("HI-RES LOSSLESS");
    }
    if has("LOSSLESS") {
        return Some("LOSSLESS");
    }
    match audio_quality {
        Some("HI_RES" | "HI_RES_LOSSLESS") => Some("HI-RES LOSSLESS"),
        Some("LOSSLESS") => Some("LOSSLESS"),
        _ => None,
    }
}

pub(crate) fn encode(page: &AlbumPageResponse) -> Option<Vec<u8>> {
    serde_json::to_vec(page).ok()
}

pub(crate) fn decode(bytes: &[u8]) -> Option<AlbumPageResponse> {
    serde_json::from_slice(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page() -> AlbumPageResponse {
        serde_json::from_value(json!({
            "album": {
                "id": 9,
                "title": "Homogenic",
                "cover": "aa-bb",
                "artist": { "id": 1, "name": "Björk" },
                "mediaMetadata": { "tags": ["LOSSLESS"] },
            },
            "tracks": [
                { "id": 11, "title": "Hunter", "duration": 255, "volumeNumber": 1 },
                { "id": 12, "title": "Jóga", "duration": 305, "volumeNumber": 1 },
            ],
            "totalTracks": 2,
            "vibrantColor": null,
            "videoCover": null,
            "copyright": "One Little Indian",
            "credits": [],
            "review": null,
            "sections": [{
                "title": "More by Björk",
                "type": "ALBUM_LIST",
                "items": [{ "id": 10, "title": "Post", "cover": "cc-dd" }],
                "apiPath": null,
            }],
        }))
        .expect("an album page")
    }

    #[test]
    fn an_album_names_its_artist_and_adds_up_its_tracks() {
        let album = Album::from(page());
        assert_eq!(album.title, "Homogenic");
        assert_eq!(album.artists[0].name, "Björk");
        assert_eq!(album.duration, 560);
        assert_eq!(album.quality, Some("LOSSLESS"));
        assert_eq!(album.tracks.len(), 2);
        assert_eq!(album.sections[0].title, "More by Björk");
    }

    #[test]
    fn a_cached_album_page_reads_back_with_its_sections() {
        let bytes = encode(&page()).expect("encodes");
        let album = Album::from(decode(&bytes).expect("decodes"));
        assert_eq!(album.sections.len(), 1);
        assert_eq!(album.sections[0].cards[0].title, "Post");
    }
}
