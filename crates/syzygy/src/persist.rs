//! Writing encrypted files off the UI thread.

use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use syzygy_store::Store;

/// Orders saves as `update` asked for them.
static NEXT_SAVE: AtomicU64 = AtomicU64::new(0);

/// The newest save or removal done to each path. Held while writing, so two
/// saves of one file never share its temporary file.
static WRITTEN: LazyLock<Mutex<HashMap<PathBuf, u64>>> = LazyLock::new(Default::default);

/// Encrypt and write `value` on a blocking thread, for `update` to run as a
/// `Task`. Saves can finish out of order; one that's older than what is
/// already on disk is skipped, so the newest value always wins.
pub fn write_json<T: Serialize + Send + 'static>(
    store: Store,
    path: PathBuf,
    value: T,
) -> impl Future<Output = Result<(), Arc<syzygy_store::Error>>> {
    in_order(path, move |path| store.write_json(path, &value))
}

/// Delete a file, in order with the saves: an older save still in flight
/// won't bring it back.
pub fn remove(
    store: Store,
    path: PathBuf,
) -> impl Future<Output = Result<(), Arc<syzygy_store::Error>>> {
    in_order(path, move |path| store.remove(path))
}

fn in_order(
    path: PathBuf,
    op: impl FnOnce(&Path) -> Result<(), syzygy_store::Error> + Send + 'static,
) -> impl Future<Output = Result<(), Arc<syzygy_store::Error>>> {
    // Taken now, in `update`, not when the future first runs.
    let save = NEXT_SAVE.fetch_add(1, Ordering::Relaxed);
    async move {
        tokio::task::spawn_blocking(move || {
            let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
            if written.get(&path).is_some_and(|&newest| newest > save) {
                return Ok(());
            }
            op(&path)?;
            written.insert(path, save);
            Ok(())
        })
        .await
        .unwrap_or_else(|e| Err(std::io::Error::other(e).into()))
        .map_err(Arc::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syzygy_store::KeySource;

    /// Two saves in flight at once: the one `update` asked for last wins,
    /// whichever finishes first.
    #[test]
    fn the_newest_save_wins_when_saves_finish_out_of_order() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&KeySource {
            keyring: None,
            key_file: dir.path().join("master.key"),
        })
        .unwrap();
        let path = dir.path().join("value.json");

        let older = write_json(store.clone(), path.clone(), "older");
        let newer = write_json(store.clone(), path.clone(), "newer");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(newer).unwrap();
        runtime.block_on(older).unwrap();

        assert_eq!(
            store.read_json::<String>(&path).unwrap().as_deref(),
            Some("newer")
        );
    }
}
