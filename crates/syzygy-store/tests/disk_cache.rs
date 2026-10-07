use std::fs;
use std::path::Path;

use syzygy_store::{CacheResult, CacheTier, DiskCache, KeySource, Store};

fn store(dir: &Path) -> Store {
    Store::open(&KeySource {
        keyring: None,
        key_file: dir.join("master.key"),
    })
    .unwrap()
}

fn fresh(result: CacheResult) -> Option<Vec<u8>> {
    match result {
        CacheResult::Fresh(data) => Some(data),
        _ => None,
    }
}

#[tokio::test]
async fn a_put_entry_reads_back_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));

    cache
        .put("album:1", b"payload", CacheTier::StaticMeta, &["album:1"])
        .await
        .unwrap();

    assert_eq!(
        fresh(cache.get("album:1", CacheTier::StaticMeta).await),
        Some(b"payload".to_vec())
    );
}

#[tokio::test]
async fn an_unknown_key_misses() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));

    assert!(matches!(
        cache.get("nope", CacheTier::Dynamic).await,
        CacheResult::Miss
    ));
}

#[tokio::test]
async fn entries_are_encrypted_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let cache = DiskCache::new(&cache_dir, &store(dir.path()));

    cache
        .put("k", b"secret-marker", CacheTier::Image, &[])
        .await
        .unwrap();

    let dat_files: Vec<_> = walk(&cache_dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "dat"))
        .collect();
    assert_eq!(dat_files.len(), 1);
    let raw = fs::read(&dat_files[0]).unwrap();
    assert_eq!(&raw[..4], b"SYZY");
    assert!(!String::from_utf8_lossy(&raw).contains("secret-marker"));
}

#[tokio::test]
async fn invalidating_a_tag_drops_every_entry_carrying_it() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));
    let tier = CacheTier::UserContent;
    cache.put("a", b"a", tier, &["playlist:1"]).await.unwrap();
    cache
        .put("b", b"b", tier, &["playlist:1", "user:7"])
        .await
        .unwrap();
    cache.put("c", b"c", tier, &["user:7"]).await.unwrap();

    cache.invalidate_tag("playlist:1").await;

    assert!(matches!(cache.get("a", tier).await, CacheResult::Miss));
    assert!(matches!(cache.get("b", tier).await, CacheResult::Miss));
    assert_eq!(fresh(cache.get("c", tier).await), Some(b"c".to_vec()));
}

#[tokio::test]
async fn invalidating_a_key_drops_only_that_entry() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));
    let tier = CacheTier::Dynamic;
    cache.put("a", b"a", tier, &[]).await.unwrap();
    cache.put("b", b"b", tier, &[]).await.unwrap();

    cache.invalidate_key("a").await;

    assert!(matches!(cache.get("a", tier).await, CacheResult::Miss));
    assert_eq!(fresh(cache.get("b", tier).await), Some(b"b".to_vec()));
}

#[tokio::test]
async fn overwriting_a_key_replaces_its_data_and_tags() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));
    let tier = CacheTier::UserContent;
    cache.put("a", b"old", tier, &["old-tag"]).await.unwrap();

    cache.put("a", b"new", tier, &["new-tag"]).await.unwrap();
    cache.invalidate_tag("old-tag").await;

    assert_eq!(fresh(cache.get("a", tier).await), Some(b"new".to_vec()));
}

#[tokio::test]
async fn clearing_drops_everything() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));
    cache.put("a", b"a", CacheTier::Image, &[]).await.unwrap();
    cache
        .put("b", b"b", CacheTier::StaticMeta, &[])
        .await
        .unwrap();

    cache.clear().await;

    assert!(matches!(
        cache.get("a", CacheTier::Image).await,
        CacheResult::Miss
    ));
    assert!(matches!(
        cache.get("b", CacheTier::StaticMeta).await,
        CacheResult::Miss
    ));
}

#[tokio::test]
async fn entries_survive_a_reopen_with_their_tags() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let tier = CacheTier::UserContent;
    {
        let cache = DiskCache::new(&cache_dir, &store(dir.path()));
        cache.put("a", b"a", tier, &["fav-tracks"]).await.unwrap();
        cache.put("b", b"b", tier, &[]).await.unwrap();
    }

    let reopened = DiskCache::new(&cache_dir, &store(dir.path()));
    assert_eq!(fresh(reopened.get("b", tier).await), Some(b"b".to_vec()));
    reopened.invalidate_tag("fav-tracks").await;
    assert!(matches!(reopened.get("a", tier).await, CacheResult::Miss));
}

#[tokio::test]
async fn entries_written_under_another_key_miss() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    {
        let other = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(&cache_dir, &store(other.path()));
        cache.put("a", b"a", CacheTier::Dynamic, &[]).await.unwrap();
    }

    let cache = DiskCache::new(&cache_dir, &store(dir.path()));

    assert!(matches!(
        cache.get("a", CacheTier::Dynamic).await,
        CacheResult::Miss
    ));
}

#[tokio::test]
async fn old_schema_folders_are_removed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    fs::create_dir_all(cache_dir.join("v0/user")).unwrap();
    fs::write(cache_dir.join("v0/user/x.dat"), b"x").unwrap();

    DiskCache::new(&cache_dir, &store(dir.path()));

    assert!(!cache_dir.join("v0").exists());
}

#[tokio::test]
async fn only_one_refresh_of_a_key_is_in_flight() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));

    assert!(cache.mark_in_flight("a").await);
    assert!(!cache.mark_in_flight("a").await);
    cache.clear_in_flight("a").await;
    assert!(cache.mark_in_flight("a").await);
}

#[tokio::test]
async fn a_refresh_is_retried_only_after_the_interval() {
    let dir = tempfile::tempdir().unwrap();
    let cache = DiskCache::new(&dir.path().join("cache"), &store(dir.path()));
    cache.put("a", b"a", CacheTier::Dynamic, &[]).await.unwrap();

    assert!(cache.should_retry_refresh("a", 300).await);
    cache.mark_refresh_attempt("a").await;
    assert!(!cache.should_retry_refresh("a", 300).await);
    assert!(cache.should_retry_refresh("a", 0).await);
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
