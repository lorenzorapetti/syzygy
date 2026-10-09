//! A fill: the rest of a long Playback source, read a page at a time in the
//! background while it plays.

use futures::FutureExt;
use futures::future::BoxFuture;
use futures::stream::{self, Stream};
use std::sync::Arc;
use syzygy_catalog::{Catalog, Error, Paged, Track};

use crate::playback::{Continuation, SourceRef};

/// Each page of the source from `continuation` on, until it runs out or a
/// read fails.
pub fn pages(
    catalog: Catalog,
    user_id: Option<u64>,
    continuation: Continuation,
) -> impl Stream<Item = Vec<Track>> + Send + 'static {
    let Continuation { source, offset } = continuation;
    stream::unfold(Some(offset), move |offset| {
        let read = offset.and_then(|offset| read(&catalog, user_id, &source, offset));
        async move {
            let offset = offset?;
            match read?.await {
                Ok(page) if !page.items.is_empty() => {
                    let next = page.has_more.then_some(offset + page.items.len());
                    Some((page.items, next))
                }
                Ok(_) => None,
                Err(e) => {
                    log::warn!("Could not read more of the Playback source: {e}");
                    None
                }
            }
        }
    })
}

type Page = BoxFuture<'static, Result<Paged<Track>, Arc<Error>>>;

/// The page of `source` at `offset`, for the sources read a page at a time.
fn read(
    catalog: &Catalog,
    user_id: Option<u64>,
    source: &SourceRef,
    offset: usize,
) -> Option<Page> {
    match source {
        SourceRef::Playlist { uuid, sort } => {
            Some(catalog.more_playlist_tracks(uuid, *sort, offset).boxed())
        }
        SourceRef::LovedTracks(sort) => match user_id {
            Some(user_id) => Some(catalog.more_loved_tracks(user_id, *sort, offset).boxed()),
            None => {
                log::warn!("Can't read more Loved tracks before TIDAL says who the user is");
                None
            }
        },
        SourceRef::Artist(id) => Some(catalog.more_artist_tracks(*id, offset).boxed()),
        SourceRef::Album(_)
        | SourceRef::Mix(_)
        | SourceRef::TrackRadio(_)
        | SourceRef::Search(_)
        | SourceRef::Track(_) => {
            log::warn!("{source:?} isn't read a page at a time");
            None
        }
    }
}
