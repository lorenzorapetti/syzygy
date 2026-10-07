//! Encrypted on-disk storage: the master key, encrypted JSON files and the
//! disk cache. Everything written here is AES-256-GCM encrypted with a `SYZY`
//! header. App identity (keyring names, directories) comes from the caller.

mod cache;
mod crypto;
mod error;
mod store;

use std::time::{SystemTime, UNIX_EPOCH};

pub use cache::{CacheResult, CacheTier, DiskCache};
pub use crypto::{KeySource, KeyringEntry};
pub use error::Error;
pub use store::Store;

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
