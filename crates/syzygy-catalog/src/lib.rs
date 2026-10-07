//! The binary's only way to reach the Catalog. Reads go through the disk
//! cache with sone's stale-while-revalidate policy (tiers and TTLs from
//! [`syzygy_store::CacheTier`]) and come back as a stream of [`Read`]s.

pub mod album;
pub mod artist;
mod error;
mod home;
pub mod home_feed;
pub mod mix;
mod paged;
mod swr;
pub mod track;

use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use syzygy_store::{CacheResult, CacheTier, DiskCache};
use syzygy_tidal::TidalClient;
use syzygy_tidal::models::{HomePageResponse, HomePageSection};

pub use album::Album;
pub use artist::Artist;
pub use error::Error;
pub use home_feed::{Card, Cover, HomeFeed};
pub use mix::Mix;
pub use paged::Paged;
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

/// Later pages carry no quick-access grid and no page links, as in sone.
fn is_more_content(section: &HomePageSection) -> bool {
    !section.title.trim().is_empty()
        && !matches!(
            section.section_type.as_str(),
            "PAGE_LINKS_CLOUD" | "PAGE_LINKS" | "SHORTCUT_LIST"
        )
}

/// How many items a paged read asks for at a time, as in sone.
const PAGE_SIZE: u32 = 50;

/// The tags of an artist's or album's reads, as `["artist", "artist:7"]`.
fn artist_or_album_tags(kind: &str, id: u64) -> Vec<String> {
    vec![kind.to_string(), format!("{kind}:{id}")]
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
