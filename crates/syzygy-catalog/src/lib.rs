//! The binary's only way to reach the Catalog. Reads go through the disk
//! cache with sone's stale-while-revalidate policy (tiers and TTLs from
//! [`syzygy_store::CacheTier`]) and come back as a stream of [`Read`]s.

mod error;
mod home;
pub mod home_feed;
mod swr;

use futures::StreamExt;
use futures::stream::BoxStream;
use std::future::Future;
use std::sync::Arc;
use syzygy_store::{CacheTier, DiskCache};
use syzygy_tidal::TidalClient;
use syzygy_tidal::models::{HomePageResponse, HomePageSection};

pub use error::Error;
pub use home_feed::HomeFeed;
pub use swr::Read;

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
