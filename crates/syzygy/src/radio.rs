//! A track's Track radio: TIDAL's mix generated from it. Autoplay plays the
//! last track's, and the drawer's Suggested tab shows the current one's.

use futures::StreamExt;
use std::sync::Arc;
use syzygy_catalog::{Catalog, Read, Track};

use crate::playback::{Radio, Source, SourceRef};

/// `track`'s Track radio, for Autoplay. None when it has none, or it can't
/// be read.
pub async fn read(catalog: Catalog, track: Track) -> Option<Radio> {
    let id = track.id;
    fetch(catalog, track).await.unwrap_or_else(|e| {
        log::warn!("Could not read the Track radio of track {id}: {e}");
        None
    })
}

/// `track`'s Track radio. None when it has none.
pub async fn fetch(
    catalog: Catalog,
    track: Track,
) -> Result<Option<Radio>, Arc<syzygy_catalog::Error>> {
    let mix_id = match track.track_radio.clone() {
        Some(mix_id) => mix_id,
        // Many lists don't say; the track read on its own does.
        None => match catalog.track(track.id).await?.track_radio {
            Some(mix_id) => mix_id,
            None => return Ok(None),
        },
    };
    // A cached radio will do: the first read, cached or fresh.
    let mix = match catalog.mix(&mix_id).next().await {
        Some(Read::Cached(mix) | Read::Fresh(Ok(mix))) => mix,
        Some(Read::Fresh(Err(e))) => return Err(e),
        None => return Ok(None),
    };
    let name = if mix.title.is_empty() {
        format!("{} Radio", track.title)
    } else {
        mix.title
    };
    Ok(Some(Radio {
        source: Source {
            kind: SourceRef::TrackRadio(mix_id),
            name,
        },
        tracks: mix.tracks,
    }))
}
