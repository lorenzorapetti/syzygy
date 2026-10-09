//! A Track radio for Autoplay: TIDAL's mix generated from the last track.

use futures::StreamExt;
use syzygy_catalog::{Catalog, Read, Track};

use crate::playback::{Radio, Source, SourceRef};

/// `track`'s Track radio. None when it has none, or it can't be read.
pub async fn read(catalog: Catalog, track: Track) -> Option<Radio> {
    let mix_id = match track.track_radio.clone() {
        Some(mix_id) => mix_id,
        // Many lists don't say; the track read on its own does.
        None => match catalog.track(track.id).await {
            Ok(read) => read.track_radio?,
            Err(e) => {
                log::warn!("Could not read track {} for its radio: {e}", track.id);
                return None;
            }
        },
    };
    // A cached radio will do: the first read, cached or fresh.
    let mix = match catalog.mix(&mix_id).next().await? {
        Read::Cached(mix) | Read::Fresh(Ok(mix)) => mix,
        Read::Fresh(Err(e)) => {
            log::warn!("Could not read the Track radio of track {}: {e}", track.id);
            return None;
        }
    };
    let name = if mix.title.is_empty() {
        format!("{} Radio", track.title)
    } else {
        mix.title
    };
    Some(Radio {
        source: Source {
            kind: SourceRef::TrackRadio(mix_id),
            name,
        },
        tracks: mix.tracks,
    })
}
