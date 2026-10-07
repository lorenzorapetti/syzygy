//! The binary's only way to reach the Catalog. Reads go through the disk
//! cache with sone's stale-while-revalidate policy (tiers and TTLs from
//! [`syzygy_store::CacheTier`]) and come back as a stream of [`Read`]s.

mod error;
mod home;
mod swr;

use futures::stream::BoxStream;
use std::sync::Arc;
use syzygy_store::{CacheTier, DiskCache};
use syzygy_tidal::TidalClient;
use syzygy_tidal::models::HomePageResponse;

pub use error::Error;
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
    pub fn home_feed(&self, slug: &str) -> BoxStream<'static, Read<HomePageResponse>> {
        let slug = slug.to_lowercase();
        let entry = Entry {
            key: format!("home_feed_{slug}"),
            tier: CacheTier::Dynamic,
            tags: vec!["home-page".to_string()],
            encode: home::encode,
            decode: home::decode,
        };
        let tidal = self.tidal.clone();
        swr::read(self.cache.clone(), entry, move || async move {
            Ok(tidal.get_home_page(&slug).await?)
        })
    }
}
