//! A card's play button: read the tracks of what the card leads to, then
//! play all of them, as Shuffle says. "Add to playlist" reads them all
//! too, to the end of a long list.

use futures::StreamExt;
use futures::stream::BoxStream;
use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::{Catalog, Error, Paged, Read, Track, TrackSort};

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
        // The rest of a long list is read once it plays.
        let long = |kind, name: &str, tracks: Paged<_>| {
            request(kind, name, &tracks.items).map(|r| page::with_rest(r, tracks.has_more))
        };
        Ok(match card.target {
            Target::Album(id) => {
                let album = first(catalog.album(id)).await?;
                let tracks = page::album_tracks(id, &album);
                request(SourceRef::Album(id), &album.title, &tracks)
            }
            Target::Playlist(uuid) => {
                let tracks = first(catalog.playlist_tracks(&uuid, sort)).await?;
                long(SourceRef::Playlist { uuid, sort }, &card.title, tracks)
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
                long(SourceRef::LovedTracks(sort), "Loved Tracks", tracks)
            }
            Target::Track(id) => Some(page::single(&catalog.track(id).await?)),
            Target::Video(_) | Target::None => None,
        })
    }
}

/// Every track of what a card leads to, in its own order: an album's, a
/// playlist's, a mix's, the Loved tracks or the card's track. None for an
/// artist's card, whose tracks aren't one list, or a video's.
pub fn tracks(
    card: Card,
    catalog: &Catalog,
    user_id: Option<u64>,
) -> impl Future<Output = Result<Vec<Track>, Arc<Error>>> + Send + 'static {
    let catalog = catalog.clone();
    async move {
        Ok(match card.target {
            Target::Album(id) => page::album_tracks(id, &first(catalog.album(id)).await?),
            Target::Playlist(uuid) => {
                let first_page = first(catalog.playlist_tracks(&uuid, None)).await?;
                all(first_page, |offset| {
                    catalog.more_playlist_tracks(&uuid, None, offset)
                })
                .await?
            }
            Target::Mix(id) => first(catalog.mix(&id)).await?.tracks,
            Target::Favorites => {
                let user_id = user_id.ok_or_else(|| Arc::new(Error::UnknownUser))?;
                let first_page = first(catalog.loved_tracks(user_id, None)).await?;
                all(first_page, |offset| {
                    catalog.more_loved_tracks(user_id, None, offset)
                })
                .await?
            }
            Target::Track(id) => vec![catalog.track(id).await?],
            Target::Artist(_) | Target::Video(_) | Target::None => vec![],
        })
    }
}

/// A paged list from its first page to its end.
async fn all<F>(first: Paged<Track>, more: impl Fn(usize) -> F) -> Result<Vec<Track>, Arc<Error>>
where
    F: Future<Output = Result<Paged<Track>, Arc<Error>>>,
{
    let mut tracks = first.items;
    let mut has_more = first.has_more;
    while has_more {
        let page = more(tracks.len()).await?;
        has_more = page.has_more && !page.items.is_empty();
        tracks.extend(page.items);
    }
    Ok(tracks)
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
