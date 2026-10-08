//! A mix's Page.

use syzygy_tidal::models::MixPageResult;

use crate::home_feed::Cover;
use crate::track::Track;

#[derive(Debug, Clone)]
pub struct Mix {
    /// Empty when TIDAL only sent the tracks.
    pub title: String,
    pub subtitle: Option<String>,
    pub cover: Option<Cover>,
    /// A track's mix, which plays as that track's Track radio.
    pub track_radio: bool,
    pub tracks: Vec<Track>,
}

impl From<MixPageResult> for Mix {
    fn from(mix: MixPageResult) -> Self {
        Mix {
            title: mix.title.unwrap_or_default(),
            subtitle: mix.subtitle.filter(|s| !s.is_empty()),
            cover: mix.image.map(Cover::Url),
            track_radio: mix.mix_type.as_deref() == Some("TRACK_MIX"),
            tracks: mix.tracks.into_iter().map(Track::from).collect(),
        }
    }
}

pub(crate) fn encode(mix: &MixPageResult) -> Option<Vec<u8>> {
    serde_json::to_vec(mix).ok()
}

pub(crate) fn decode(bytes: &[u8]) -> Option<MixPageResult> {
    serde_json::from_slice(bytes).ok()
}
