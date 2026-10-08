//! A card's play button: read the tracks of what the card leads to, then
//! play all of them, as Shuffle says.

use futures::StreamExt;
use futures::stream::BoxStream;
use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::{Catalog, Error, Read, TrackSort};

use crate::page::{self, Context};
use crate::playback::{PlayRequest, SourceRef, Start};

/// The source a card leads to, ready to start. `None` for a card with
/// nothing to play, such as a video's.
pub fn read(
    card: Card,
    catalog: &Catalog,
    context: &Context,
) -> impl Future<Output = Result<Option<PlayRequest>, Arc<Error>>> + Send + 'static {
    let catalog = catalog.clone();
    let user_id = context.user_id;
    // A playlist and the Loved tracks play in the order the user sorted them.
    let sort: Option<TrackSort> = match &card.target {
        Target::Playlist(uuid) => context.settings.track_sorts.get(uuid).copied(),
        Target::Favorites => context.settings.loved_tracks_sort,
        _ => None,
    };
    async move {
        let request = |kind, name: &str, tracks: &[_]| {
            (!tracks.is_empty()).then(|| page::request(kind, name, tracks, Start::All))
        };
        Ok(match card.target {
            Target::Album(id) => {
                let album = first(catalog.album(id)).await?;
                let tracks = page::album_tracks(id, &album);
                request(SourceRef::Album(id), &album.title, &tracks)
            }
            Target::Playlist(uuid) => {
                let tracks = first(catalog.playlist_tracks(&uuid, sort)).await?;
                request(SourceRef::Playlist(uuid), &card.title, &tracks.items)
            }
            Target::Mix(id) => {
                let mix = first(catalog.mix(&id)).await?;
                let source = page::mix_source(&id, &mix, &card.title);
                request(source.kind, &source.name, &mix.tracks)
            }
            Target::Artist(id) => {
                let artist = first(catalog.artist(id)).await?;
                let top = page::top_tracks(&artist).map(|(_, tracks)| tracks);
                request(SourceRef::Artist(id), &artist.name, top.unwrap_or_default())
            }
            Target::Favorites => {
                let user_id = user_id.ok_or_else(|| Arc::new(Error::UnknownUser))?;
                let tracks = first(catalog.loved_tracks(user_id, sort)).await?;
                request(SourceRef::LovedTracks, "Loved Tracks", &tracks.items)
            }
            Target::Track(id) => Some(page::single(&catalog.track(id).await?)),
            Target::Video(_) | Target::None => None,
        })
    }
}

/// The first value a Catalog read gives: the cached copy if there is one,
/// else TIDAL's.
async fn first<T>(mut reads: BoxStream<'static, Read<T>>) -> Result<T, Arc<Error>> {
    match reads.next().await {
        Some(Read::Cached(value) | Read::Fresh(Ok(value))) => Ok(value),
        Some(Read::Fresh(Err(e))) => Err(e),
        // A read always gives at least TIDAL's answer.
        None => unreachable!("a Catalog read ended without a value"),
    }
}
