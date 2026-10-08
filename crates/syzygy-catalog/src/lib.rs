//! The binary's only way to reach the Catalog. Reads go through the disk
//! cache with sone's stale-while-revalidate policy (tiers and TTLs from
//! [`syzygy_store::CacheTier`]) and come back as a stream of [`Read`]s.

pub mod album;
pub mod artist;
mod error;
pub mod explore;
pub mod feed;
mod home;
pub mod home_feed;
pub mod library;
pub mod mix;
mod paged;
pub mod playlist;
pub mod profile;
pub mod search;
mod swr;
pub mod track;

use futures::StreamExt;
use futures::stream::BoxStream;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use syzygy_store::{CacheResult, CacheTier, DiskCache};
use syzygy_tidal::TidalClient;
use syzygy_tidal::models::{
    HomePageResponse, HomePageSection, PaginatedResponse, TidalAlbumDetail, TidalArtistDetail,
    TidalFavoriteMix,
};

pub use album::Album;
pub use artist::Artist;
pub use error::Error;
pub use explore::ExplorePage;
pub use feed::Feed;
pub use home_feed::{Card, Cover, HomeFeed};
pub use library::{Kind, LibraryOrder, LibrarySort, Shelf};
pub use mix::Mix;
pub use paged::Paged;
pub use playlist::{Direction, Playlist, TrackOrder, TrackSort};
pub use profile::Profile;
pub use search::{Hit, SearchResults, Suggestion, Suggestions};
pub use swr::Read;
pub use track::Track;

use swr::Entry;

/// A cheap `Clone` handle over the TIDAL client and the disk cache.
#[derive(Clone)]
pub struct Catalog {
    tidal: TidalClient,
    cache: Arc<DiskCache>,
}

impl Catalog {
    pub fn new(tidal: TidalClient, cache: DiskCache) -> Self {
        Self {
            tidal,
            cache: Arc::new(cache),
        }
    }

    /// One Home feed tab, by its slug (`"static"` for the default tab).
    pub fn home_feed(&self, slug: &str) -> BoxStream<'static, Read<HomeFeed>> {
        let slug = slug.to_lowercase();
        let tidal = self.tidal.clone();
        swr::read(
            self.cache.clone(),
            home_feed_entry(&slug),
            move || async move { Ok(tidal.get_home_page(&slug).await?) },
        )
        .map(|read| read.map(HomeFeed::from))
        .boxed()
    }

    /// One Home feed tab straight from TIDAL, cached for the next read. Yields
    /// nothing when TIDAL sends a feed with no sections, so the copy on screen
    /// stays.
    pub fn refresh_home_feed(&self, slug: &str) -> BoxStream<'static, Read<HomeFeed>> {
        let slug = slug.to_lowercase();
        let tidal = self.tidal.clone();
        swr::refresh(
            self.cache.clone(),
            home_feed_entry(&slug),
            move || async move { Ok(tidal.get_home_page(&slug).await?) },
        )
        .map(|read| read.map(HomeFeed::from))
        .boxed()
    }

    /// The sections of a Home feed tab after `cursor`. Not cached: they only
    /// follow on from the first page on screen.
    pub fn more_home_feed(
        &self,
        slug: &str,
        cursor: &str,
    ) -> impl Future<Output = Result<HomeFeed, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let slug = slug.to_lowercase();
        let cursor = cursor.to_string();
        async move {
            let (_, mut sections, cursor) = tidal
                .fetch_v2_home_feed(&slug, Some(&cursor))
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            sections.retain(is_more_content);
            Ok(HomeFeed::from(HomePageResponse {
                tabs: vec![],
                sections,
                cursor,
            }))
        }
    }

    /// An album's Page.
    pub fn album(&self, id: u64) -> BoxStream<'static, Read<Album>> {
        let tidal = self.tidal.clone();
        let entry = Entry {
            key: format!("album-page:{id}"),
            tier: CacheTier::Dynamic,
            tags: artist_or_album_tags("album", id),
            encode: album::encode,
            decode: album::decode,
        };
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_album_page(id).await?)
        })
        .map(|read| read.map(Album::from))
        .boxed()
    }

    /// An artist's Page.
    pub fn artist(&self, id: u64) -> BoxStream<'static, Read<Artist>> {
        let tidal = self.tidal.clone();
        let entry = json_entry(
            format!("artist-page:{id}"),
            artist_or_album_tags("artist", id),
        );
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_artist_page(id).await?)
        })
        .map(|read| read.map(Artist::from))
        .boxed()
    }

    /// The first page of all an artist's top tracks.
    pub fn artist_tracks(&self, id: u64) -> BoxStream<'static, Read<Paged<Track>>> {
        let tidal = self.tidal.clone();
        let entry = json_entry(
            format!("artist-top-tracks-all:{id}"),
            artist_or_album_tags("artist", id),
        );
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_artist_top_tracks_all(id, 0, PAGE_SIZE).await?)
        })
        .map(|read| read.map(|page| artist::tracks_page(&page, PAGE_SIZE as usize)))
        .boxed()
    }

    /// An artist's top tracks after the first `offset`. Not cached: they
    /// only follow on from the pages on screen.
    pub fn more_artist_tracks(
        &self,
        id: u64,
        offset: usize,
    ) -> impl Future<Output = Result<Paged<Track>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        async move {
            let page = tidal
                .get_artist_top_tracks_all(id, offset as u32, PAGE_SIZE)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(artist::tracks_page(&page, PAGE_SIZE as usize))
        }
    }

    /// The first page of one of an artist's sections, by its view-all path.
    pub fn artist_view_all(&self, id: u64, path: &str) -> BoxStream<'static, Read<Paged<Card>>> {
        let tidal = self.tidal.clone();
        let path = path.to_string();
        let entry = json_entry(
            format!("artist-view-all:{id}:{path}"),
            artist_or_album_tags("artist", id),
        );
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_artist_view_all(id, &path, 0, PAGE_SIZE).await?)
        })
        .map(|read| read.map(|page| artist::cards_page(&page, PAGE_SIZE as usize)))
        .boxed()
    }

    /// One of an artist's sections after the first `offset` items. Not cached.
    pub fn more_artist_view_all(
        &self,
        id: u64,
        path: &str,
        offset: usize,
    ) -> impl Future<Output = Result<Paged<Card>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let path = path.to_string();
        async move {
            let page = tidal
                .get_artist_view_all(id, &path, offset as u32, PAGE_SIZE)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(artist::cards_page(&page, PAGE_SIZE as usize))
        }
    }

    /// A mix's Page.
    pub fn mix(&self, id: &str) -> BoxStream<'static, Read<Mix>> {
        let tidal = self.tidal.clone();
        let id = id.to_string();
        let entry = Entry {
            key: format!("mix-page:{id}"),
            tier: CacheTier::Dynamic,
            tags: vec!["mix-page".to_string(), format!("mix:{id}")],
            encode: mix::encode,
            decode: mix::decode,
        };
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_mix_items(&id).await?)
        })
        .map(|read| read.map(Mix::from))
        .boxed()
    }

    /// A playlist: what it is and who made it.
    pub fn playlist(&self, uuid: &str) -> BoxStream<'static, Read<Playlist>> {
        let tidal = self.tidal.clone();
        let uuid = uuid.to_string();
        let entry = Entry {
            key: format!("playlist-details:{uuid}"),
            tier: CacheTier::Dynamic,
            tags: playlist_tags(&uuid),
            encode: playlist::encode,
            decode: playlist::decode,
        };
        swr::read(self.cache.clone(), entry, move || async move {
            let details = tidal.get_playlist_details(&uuid).await?;
            playlist::from_details(&details).ok_or_else(|| {
                syzygy_tidal::Error::Parse(format!("Not a playlist: {details}")).into()
            })
        })
        .boxed()
    }

    /// The first page of a playlist's tracks, in `sort`'s order or else the
    /// playlist's own.
    pub fn playlist_tracks(
        &self,
        uuid: &str,
        sort: Option<TrackSort>,
    ) -> BoxStream<'static, Read<Paged<Track>>> {
        let tidal = self.tidal.clone();
        let uuid = uuid.to_string();
        let entry = Entry {
            key: format!("playlist-page:{uuid}:{}", TrackSort::key(sort)),
            tier: CacheTier::Dynamic,
            tags: playlist_tags(&uuid),
            encode: playlist::encode_page,
            decode: playlist::decode_page,
        };
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(playlist_page(&tidal, &uuid, sort, 0).await?)
        })
        .map(|read| read.map(playlist::tracks_page))
        .boxed()
    }

    /// A playlist's tracks after the first `offset`. Not cached: they only
    /// follow on from the pages on screen.
    pub fn more_playlist_tracks(
        &self,
        uuid: &str,
        sort: Option<TrackSort>,
        offset: usize,
    ) -> impl Future<Output = Result<Paged<Track>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let uuid = uuid.to_string();
        async move {
            let page = playlist_page(&tidal, &uuid, sort, offset as u32)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(playlist::tracks_page(page))
        }
    }

    /// Tracks TIDAL recommends for a playlist, from `offset`. Not cached:
    /// each batch is asked for once, when the last one has been seen.
    pub fn playlist_recommendations(
        &self,
        uuid: &str,
        offset: usize,
    ) -> impl Future<Output = Result<Vec<Track>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let uuid = uuid.to_string();
        async move {
            let page = tidal
                .get_playlist_recommendations(&uuid, offset as u32, PAGE_SIZE)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(page.items.into_iter().map(Track::from).collect())
        }
    }

    /// The first page of a shelf of the user's Library.
    pub fn library(&self, shelf: &Shelf) -> BoxStream<'static, Read<Paged<library::Item>>> {
        let tidal = self.tidal.clone();
        let shelf = shelf.clone();
        let key = format!(
            "library:{}:{:?}:{}:{}",
            shelf.user_id,
            shelf.kind,
            shelf.folder_id(),
            shelf.sort.key()
        );
        let tags = library_tags(&shelf);
        let cache = self.cache.clone();
        match shelf.kind {
            Kind::Playlists => {
                swr::read(cache, serde_entry::<Value>(key, tags), move || async move {
                    Ok(folder(&tidal, &shelf, 0, None).await?)
                })
                .map(|read| read.map(|page| library::folder_page(&page)))
                .boxed()
            }
            Kind::Albums => swr::read(cache, serde_entry(key, tags), move || async move {
                Ok(favorite_albums(&tidal, &shelf, 0).await?)
            })
            .map(|read| read.map(library::albums_page))
            .boxed(),
            Kind::Artists => swr::read(cache, serde_entry(key, tags), move || async move {
                Ok(favorite_artists(&tidal, &shelf, 0).await?)
            })
            .map(|read| read.map(library::artists_page))
            .boxed(),
            Kind::Mixes => swr::read(cache, serde_entry(key, tags), move || async move {
                Ok(favorite_mixes(&tidal, &shelf, 0).await?)
            })
            .map(|read| read.map(library::mixes_page))
            .boxed(),
        }
    }

    /// A shelf after the first `offset` items, from `cursor` for the
    /// playlists. Not cached: they only follow on from the pages on screen.
    pub fn more_library(
        &self,
        shelf: &Shelf,
        offset: usize,
        cursor: Option<String>,
    ) -> impl Future<Output = Result<Paged<library::Item>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let shelf = shelf.clone();
        async move {
            let offset = offset as u32;
            let page = match shelf.kind {
                Kind::Playlists => folder(&tidal, &shelf, offset, cursor.as_deref())
                    .await
                    .map(|page| library::folder_page(&page)),
                Kind::Albums => favorite_albums(&tidal, &shelf, offset)
                    .await
                    .map(library::albums_page),
                Kind::Artists => favorite_artists(&tidal, &shelf, offset)
                    .await
                    .map(library::artists_page),
                Kind::Mixes => favorite_mixes(&tidal, &shelf, offset)
                    .await
                    .map(library::mixes_page),
            };
            page.map_err(|e| Arc::new(Error::from(e)))
        }
    }

    /// The first page of the user's Loved tracks, in `sort`'s order or else
    /// last added first.
    pub fn loved_tracks(
        &self,
        user_id: u64,
        sort: Option<TrackSort>,
    ) -> BoxStream<'static, Read<Paged<Track>>> {
        let tidal = self.tidal.clone();
        let entry = Entry {
            key: format!("fav-tracks:{user_id}:{}", TrackSort::key(sort)),
            tier: CacheTier::UserContent,
            tags: vec!["fav-tracks".to_string(), format!("user:{user_id}")],
            encode: playlist::encode_page,
            decode: playlist::decode_page,
        };
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(loved_page(&tidal, user_id, sort, 0).await?)
        })
        .map(|read| read.map(playlist::tracks_page))
        .boxed()
    }

    /// The user's Loved tracks after the first `offset`. Not cached.
    pub fn more_loved_tracks(
        &self,
        user_id: u64,
        sort: Option<TrackSort>,
        offset: usize,
    ) -> impl Future<Output = Result<Paged<Track>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        async move {
            let page = loved_page(&tidal, user_id, sort, offset as u32)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(playlist::tracks_page(page))
        }
    }

    /// What a search for `query` finds. Not cached: a search is read once
    /// per Search Page.
    pub fn search(
        &self,
        query: &str,
    ) -> impl Future<Output = Result<SearchResults, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let query = query.to_string();
        async move {
            let results = tidal
                .search(&query, PAGE_SIZE)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(SearchResults::from(results))
        }
    }

    /// What to offer for `query` while it's typed. Nothing when TIDAL
    /// can't say: suggestions are never worth an error.
    pub fn suggestions(&self, query: &str) -> impl Future<Output = Suggestions> + Send + 'static {
        let tidal = self.tidal.clone();
        let query = query.to_string();
        async move { Suggestions::from(tidal.get_suggestions(&query, SUGGESTIONS).await) }
    }

    /// An Explore Page, by its path: [`explore::ROOT`] for Explore itself.
    pub fn explore(&self, path: &str) -> BoxStream<'static, Read<ExplorePage>> {
        let tidal = self.tidal.clone();
        let path = path.to_string();
        swr::read(
            self.cache.clone(),
            explore_entry(&path),
            move || async move { Ok(tidal.get_page(&path).await?) },
        )
        .map(|read| read.map(ExplorePage::from))
        .boxed()
    }

    /// The user's Feed. Not cached, so how many they haven't seen is never
    /// a stale count from before they opened it.
    pub fn feed(
        &self,
        user_id: u64,
    ) -> impl Future<Output = Result<Feed, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        async move {
            let feed = tidal
                .fetch_feed(user_id)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            Ok(Feed::from(feed))
        }
    }

    /// Mark everything in the user's Feed seen.
    pub fn mark_feed_seen(
        &self,
        user_id: u64,
    ) -> impl Future<Output = Result<(), Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        async move {
            tidal
                .mark_feed_seen(user_id)
                .await
                .map_err(|e| Arc::new(Error::from(e)))
        }
    }

    /// A user's profile, the signed-in user's or anyone's.
    pub fn profile(&self, user_id: u64) -> BoxStream<'static, Read<Profile>> {
        let tidal = self.tidal.clone();
        let entry = serde_entry(
            format!("profile:{user_id}"),
            vec!["profile".to_string(), format!("profile:{user_id}")],
        );
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_profile(user_id).await?)
        })
        .map(|read| read.map(Profile::from))
        .boxed()
    }

    /// The bytes of a picture, from the disk cache when it's there (stale
    /// is good enough: a cover doesn't change under its URL).
    pub fn image(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<Vec<u8>, Arc<Error>>> + Send + 'static {
        let tidal = self.tidal.clone();
        let cache = self.cache.clone();
        let url = url.to_string();
        async move {
            if let CacheResult::Fresh(bytes) | CacheResult::Stale(bytes) =
                cache.get(&url, CacheTier::Image).await
            {
                return Ok(bytes);
            }
            let bytes = tidal
                .get_image(&url)
                .await
                .map_err(|e| Arc::new(Error::from(e)))?;
            if let Err(e) = cache.put(&url, &bytes, CacheTier::Image, &["image"]).await {
                log::warn!("Could not cache an image: {e}");
            }
            Ok(bytes)
        }
    }
}

/// Where a Home feed tab lives in the cache.
fn home_feed_entry(slug: &str) -> Entry<HomePageResponse> {
    Entry {
        key: format!("home_feed_{slug}"),
        tier: CacheTier::Dynamic,
        tags: vec!["home-page".to_string()],
        encode: home::encode,
        decode: home::decode,
    }
}

/// Where an Explore Page lives in the cache.
fn explore_entry(path: &str) -> Entry<HomePageResponse> {
    Entry {
        key: format!("section:{path}"),
        tier: CacheTier::Dynamic,
        tags: vec!["section".to_string()],
        encode: home::encode,
        decode: home::decode,
    }
}

/// Later pages carry no quick-access grid and no page links, as in sone.
fn is_more_content(section: &HomePageSection) -> bool {
    !section.title.trim().is_empty()
        && !matches!(
            section.section_type.as_str(),
            "PAGE_LINKS_CLOUD" | "PAGE_LINKS" | "SHORTCUT_LIST"
        )
}

/// How many items a paged read asks for at a time, as in sone.
pub const PAGE_SIZE: u32 = 50;

/// How many queries the dropdown suggests, as in sone.
const SUGGESTIONS: u32 = 10;

/// The tags of an artist's or album's reads, as `["artist", "artist:7"]`.
fn artist_or_album_tags(kind: &str, id: u64) -> Vec<String> {
    vec![kind.to_string(), format!("{kind}:{id}")]
}

/// The tags of a playlist's reads.
fn playlist_tags(uuid: &str) -> Vec<String> {
    vec!["playlist".to_string(), format!("playlist:{uuid}")]
}

/// `PAGE_SIZE` of a playlist's tracks from `offset`, sorted by TIDAL.
async fn playlist_page(
    tidal: &TidalClient,
    uuid: &str,
    sort: Option<TrackSort>,
    offset: u32,
) -> Result<syzygy_tidal::models::PaginatedTracks, syzygy_tidal::Error> {
    let params = sort.map(TrackSort::params);
    tidal
        .get_playlist_tracks_page(
            uuid,
            offset,
            PAGE_SIZE,
            params.map(|(order, _)| order),
            params.map(|(_, direction)| direction),
        )
        .await
}

/// The tags of a Library shelf's reads: what its edits invalidate.
fn library_tags(shelf: &Shelf) -> Vec<String> {
    let kind = match shelf.kind {
        // Favorite playlists sit among the user's own, and in Folders.
        Kind::Playlists => vec!["folders".to_string(), "fav-playlists".to_string()],
        Kind::Albums => vec!["fav-albums".to_string()],
        Kind::Artists => vec!["fav-artists".to_string()],
        Kind::Mixes => vec!["fav-mixes".to_string()],
    };
    kind.into_iter()
        .chain([format!("user:{}", shelf.user_id)])
        .collect()
}

/// `PAGE_SIZE` of a Folder's playlists and Folders from `offset`, or from
/// `cursor` past the first page.
async fn folder(
    tidal: &TidalClient,
    shelf: &Shelf,
    offset: u32,
    cursor: Option<&str>,
) -> Result<Value, syzygy_tidal::Error> {
    let (order, direction) = shelf.sort.params();
    tidal
        .get_playlist_folders(
            shelf.folder_id(),
            "",
            offset,
            PAGE_SIZE,
            order,
            direction,
            cursor.unwrap_or_default(),
        )
        .await
}

async fn favorite_albums(
    tidal: &TidalClient,
    shelf: &Shelf,
    offset: u32,
) -> Result<PaginatedResponse<TidalAlbumDetail>, syzygy_tidal::Error> {
    let (order, direction) = shelf.sort.params();
    tidal
        .get_favorite_albums(shelf.user_id, offset, PAGE_SIZE, order, direction)
        .await
}

async fn favorite_artists(
    tidal: &TidalClient,
    shelf: &Shelf,
    offset: u32,
) -> Result<PaginatedResponse<TidalArtistDetail>, syzygy_tidal::Error> {
    let (order, direction) = shelf.sort.params();
    tidal
        .get_favorite_artists(shelf.user_id, offset, PAGE_SIZE, order, direction)
        .await
}

async fn favorite_mixes(
    tidal: &TidalClient,
    shelf: &Shelf,
    offset: u32,
) -> Result<PaginatedResponse<TidalFavoriteMix>, syzygy_tidal::Error> {
    let (order, direction) = shelf.sort.params();
    tidal
        .get_favorite_mixes(offset, PAGE_SIZE, order, direction)
        .await
}

/// `PAGE_SIZE` of the user's Loved tracks from `offset`, sorted by TIDAL:
/// last added first unless a sort says otherwise, as in sone.
async fn loved_page(
    tidal: &TidalClient,
    user_id: u64,
    sort: Option<TrackSort>,
    offset: u32,
) -> Result<syzygy_tidal::models::PaginatedTracks, syzygy_tidal::Error> {
    let (order, direction) = sort.map_or(("DATE", "DESC"), TrackSort::params);
    tidal
        .get_favorite_tracks(user_id, offset, PAGE_SIZE, order, direction)
        .await
}

/// A typed read, cached as JSON, in the tier for the user's own content.
fn serde_entry<T: Serialize + DeserializeOwned>(key: String, tags: Vec<String>) -> Entry<T> {
    Entry {
        key,
        tier: CacheTier::UserContent,
        tags,
        encode: |value| serde_json::to_vec(value).ok(),
        decode: |bytes| serde_json::from_slice(bytes).ok(),
    }
}

/// A raw JSON read, cached as it came, in the Dynamic tier.
fn json_entry(key: String, tags: Vec<String>) -> Entry<Value> {
    Entry {
        key,
        tier: CacheTier::Dynamic,
        tags,
        encode: |value| serde_json::to_vec(value).ok(),
        decode: |bytes| serde_json::from_slice(bytes).ok(),
    }
}
