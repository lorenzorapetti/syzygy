//! Stale-while-revalidate over the disk cache: sone's policy, as a stream.

use futures::stream::{self, BoxStream, StreamExt};
use std::future::Future;
use std::sync::Arc;
use syzygy_store::{CacheResult, CacheTier, DiskCache};

use crate::Error;

/// One value from a Catalog read. A read yields `Cached` when the disk cache
/// has the entry, then `Fresh` when it went to TIDAL: a cache hit inside the
/// TTL is just `Cached`, a miss just `Fresh`, and a stale hit both.
#[derive(Debug, Clone)]
pub enum Read<T> {
    Cached(T),
    Fresh(Result<T, Arc<Error>>),
}

impl<T> Read<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Read<U> {
        match self {
            Read::Cached(value) => Read::Cached(f(value)),
            Read::Fresh(result) => Read::Fresh(result.map(f)),
        }
    }
}

/// A failed refresh of a stale entry isn't retried for this long; the stale
/// copy is served alone until then.
const REFRESH_RETRY_SECS: u64 = 300;

/// Where a read lives in the cache and how it's stored.
pub(crate) struct Entry<T> {
    pub key: String,
    pub tier: CacheTier,
    pub tags: Vec<String>,
    /// `None` refuses to cache the value.
    pub encode: fn(&T) -> Option<Vec<u8>>,
    /// `None` treats the cached bytes as a miss.
    pub decode: fn(&[u8]) -> Option<T>,
}

pub(crate) fn read<T, F, Fut>(
    cache: Arc<DiskCache>,
    entry: Entry<T>,
    fetch: F,
) -> BoxStream<'static, Read<T>>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, Error>> + Send,
{
    enum Step<F> {
        Lookup(F),
        Fetch(F),
        Done,
    }

    let reader = Arc::new(Reader { cache, entry });
    stream::unfold(Step::Lookup(fetch), move |step| {
        let reader = reader.clone();
        async move {
            match step {
                Step::Lookup(fetch) => match reader.lookup().await {
                    Some(hit) => {
                        let next = if hit.refresh {
                            Step::Fetch(fetch)
                        } else {
                            Step::Done
                        };
                        Some((Read::Cached(hit.value), next))
                    }
                    None => reader
                        .fetch(fetch, false)
                        .await
                        .map(|fresh| (fresh, Step::Done)),
                },
                Step::Fetch(fetch) => reader
                    .fetch(fetch, true)
                    .await
                    .map(|fresh| (fresh, Step::Done)),
                Step::Done => None,
            }
        }
    })
    .boxed()
}

/// Go to TIDAL whatever the cache holds, and cache what comes back: one
/// `Fresh`, or nothing when the encoder refuses the value, so a cached copy
/// already on screen stays.
pub(crate) fn refresh<T, F, Fut>(
    cache: Arc<DiskCache>,
    entry: Entry<T>,
    fetch: F,
) -> BoxStream<'static, Read<T>>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, Error>> + Send,
{
    let reader = Reader { cache, entry };
    stream::once(async move { reader.fetch(fetch, true).await })
        .filter_map(std::future::ready)
        .boxed()
}

/// A usable cached value.
struct Hit<T> {
    value: T,
    /// Stale, and due another try at a refresh.
    refresh: bool,
}

struct Reader<T> {
    cache: Arc<DiskCache>,
    entry: Entry<T>,
}

impl<T> Reader<T> {
    /// The cached value, if there is a usable one, and whether to refresh it.
    async fn lookup(&self) -> Option<Hit<T>> {
        let (bytes, stale) = match self.cache.get(&self.entry.key, self.entry.tier).await {
            CacheResult::Fresh(bytes) => (bytes, false),
            CacheResult::Stale(bytes) => (bytes, true),
            CacheResult::Miss => return None,
        };
        let value = (self.entry.decode)(&bytes)?;
        let refresh = stale
            && self
                .cache
                .should_retry_refresh(&self.entry.key, REFRESH_RETRY_SECS)
                .await;
        Some(Hit { value, refresh })
    }

    /// Go to TIDAL and cache what comes back. A failure is remembered, so a
    /// stale entry isn't refreshed again until [`REFRESH_RETRY_SECS`] pass.
    /// Nothing is marked before the fetch: a read aborted mid-flight must
    /// not hold off the next one.
    ///
    /// A value the encoder refuses is a failure in disguise. It is `None`
    /// when it would replace a cached copy already sent, which then stays.
    async fn fetch<Fut>(&self, fetch: impl FnOnce() -> Fut, refreshing: bool) -> Option<Read<T>>
    where
        Fut: Future<Output = Result<T, Error>>,
    {
        let Entry {
            key,
            tier,
            tags,
            encode,
            ..
        } = &self.entry;
        match fetch().await {
            Ok(value) => {
                match encode(&value) {
                    Some(bytes) => {
                        let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
                        if let Err(e) = self.cache.put(key, &bytes, *tier, &tags).await {
                            log::warn!("Could not cache {key}: {e}");
                        }
                    }
                    None if refreshing => {
                        log::warn!("Ignoring a refresh of {key} that can't be cached");
                        self.cache.mark_refresh_attempt(key).await;
                        return None;
                    }
                    None => {}
                }
                Some(Read::Fresh(Ok(value)))
            }
            Err(e) => {
                self.cache.mark_refresh_attempt(key).await;
                Some(Read::Fresh(Err(Arc::new(e))))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use syzygy_store::{KeySource, Store};

    fn cache(dir: &std::path::Path) -> Arc<DiskCache> {
        let store = Store::open(&KeySource {
            keyring: None,
            key_file: dir.join("master.key"),
        })
        .unwrap();
        Arc::new(DiskCache::new(&dir.join("cache"), &store))
    }

    fn entry() -> Entry<String> {
        Entry {
            key: "k".to_string(),
            tier: CacheTier::Dynamic,
            tags: vec!["tag".to_string()],
            encode: |value| Some(value.as_bytes().to_vec()),
            decode: |bytes| String::from_utf8(bytes.to_vec()).ok(),
        }
    }

    /// Run a read whose fetch returns `result`, counting fetches.
    async fn run(
        cache: &Arc<DiskCache>,
        entry: Entry<String>,
        fetches: &Arc<AtomicUsize>,
        result: Result<&str, ()>,
    ) -> Vec<Read<String>> {
        let fetches = fetches.clone();
        let result = result.map(str::to_string);
        read(cache.clone(), entry, move || async move {
            fetches.fetch_add(1, Ordering::SeqCst);
            result.map_err(|()| Error::Tidal(syzygy_tidal::Error::Network("down".into())))
        })
        .collect()
        .await
    }

    fn values(reads: &[Read<String>]) -> Vec<String> {
        reads
            .iter()
            .map(|read| match read {
                Read::Cached(v) => format!("cached {v}"),
                Read::Fresh(Ok(v)) => format!("fresh {v}"),
                Read::Fresh(Err(e)) => format!("failed {e}"),
            })
            .collect()
    }

    #[tokio::test]
    async fn a_miss_fetches_and_yields_only_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        let reads = run(&cache, entry(), &fetches, Ok("a")).await;

        assert_eq!(values(&reads), ["fresh a"]);
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_fetched_value_is_served_from_cache_next_time_without_fetching() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        run(&cache, entry(), &fetches, Ok("a")).await;
        let reads = run(&cache, entry(), &fetches, Ok("b")).await;

        assert_eq!(values(&reads), ["cached a"]);
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_failed_fetch_yields_the_error_and_caches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        let reads = run(&cache, entry(), &fetches, Err(())).await;
        assert_eq!(values(&reads), ["failed Network error: down"]);

        let reads = run(&cache, entry(), &fetches, Ok("a")).await;
        assert_eq!(values(&reads), ["fresh a"]);
    }

    #[tokio::test]
    async fn a_value_the_encoder_refuses_is_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));
        let refusing = || Entry {
            encode: |_| None,
            ..entry()
        };

        run(&cache, refusing(), &fetches, Ok("a")).await;
        let reads = run(&cache, refusing(), &fetches, Ok("b")).await;

        assert_eq!(values(&reads), ["fresh b"]);
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cached_bytes_the_decoder_refuses_are_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        run(&cache, entry(), &fetches, Ok("a")).await;
        let refusing = Entry {
            decode: |_| None,
            ..entry()
        };
        let reads = run(&cache, refusing, &fetches, Ok("b")).await;

        assert_eq!(values(&reads), ["fresh b"]);
    }

    async fn run_refresh(
        cache: &Arc<DiskCache>,
        entry: Entry<String>,
        result: Result<&str, ()>,
    ) -> Vec<Read<String>> {
        let result = result.map(str::to_string);
        refresh(cache.clone(), entry, move || async move {
            result.map_err(|()| Error::Tidal(syzygy_tidal::Error::Network("down".into())))
        })
        .collect()
        .await
    }

    #[tokio::test]
    async fn a_refresh_fetches_inside_the_ttl_and_replaces_the_cached_value() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        run(&cache, entry(), &fetches, Ok("a")).await;
        let reads = run_refresh(&cache, entry(), Ok("b")).await;
        assert_eq!(values(&reads), ["fresh b"]);

        let reads = run(&cache, entry(), &fetches, Ok("c")).await;
        assert_eq!(values(&reads), ["cached b"]);
    }

    #[tokio::test]
    async fn a_refresh_the_encoder_refuses_yields_nothing_and_keeps_the_cached_value() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        run(&cache, entry(), &fetches, Ok("a")).await;
        let refusing = Entry {
            encode: |_| None,
            ..entry()
        };
        let reads = run_refresh(&cache, refusing, Ok("b")).await;
        assert!(reads.is_empty());

        let reads = run(&cache, entry(), &fetches, Ok("c")).await;
        assert_eq!(values(&reads), ["cached a"]);
    }

    #[tokio::test]
    async fn a_failed_refresh_yields_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());

        let reads = run_refresh(&cache, entry(), Err(())).await;
        assert_eq!(values(&reads), ["failed Network error: down"]);
    }

    #[tokio::test]
    async fn a_mutation_tag_invalidates_the_read() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let fetches = Arc::new(AtomicUsize::new(0));

        run(&cache, entry(), &fetches, Ok("a")).await;
        cache.invalidate_tag("tag").await;
        let reads = run(&cache, entry(), &fetches, Ok("b")).await;

        assert_eq!(values(&reads), ["fresh b"]);
    }
}
