use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use syzygy_store::{Error, KeySource, Store};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Prefs {
    volume: f32,
    name: String,
}

fn prefs() -> Prefs {
    Prefs {
        volume: 0.5,
        name: "secret-marker".into(),
    }
}

/// Key file only, so tests never touch the user's real keyring.
fn file_key(dir: &Path) -> KeySource {
    KeySource {
        keyring: None,
        key_file: dir.join("master.key"),
    }
}

#[test]
fn json_round_trips_through_an_encrypted_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("prefs.json");

    store.write_json(&path, &prefs()).unwrap();

    assert_eq!(store.read_json::<Prefs>(&path).unwrap(), Some(prefs()));
}

#[test]
fn a_missing_file_reads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();

    let read = store
        .read_json::<Prefs>(&dir.path().join("nope.json"))
        .unwrap();

    assert_eq!(read, None);
}

#[test]
fn files_on_disk_are_encrypted_with_the_syzy_magic() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("prefs.json");

    store.write_json(&path, &prefs()).unwrap();

    let raw = fs::read(&path).unwrap();
    assert_eq!(&raw[..4], b"SYZY");
    let as_text = String::from_utf8_lossy(&raw);
    assert!(!as_text.contains("secret-marker"));
}

#[test]
fn writing_creates_missing_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("a/b/prefs.json");

    store.write_json(&path, &prefs()).unwrap();

    assert_eq!(store.read_json::<Prefs>(&path).unwrap(), Some(prefs()));
}

#[test]
fn the_key_file_is_reused_across_opens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.json");
    Store::open(&file_key(dir.path()))
        .unwrap()
        .write_json(&path, &prefs())
        .unwrap();

    let reopened = Store::open(&file_key(dir.path())).unwrap();

    assert_eq!(reopened.read_json::<Prefs>(&path).unwrap(), Some(prefs()));
}

#[cfg(unix)]
#[test]
fn a_generated_key_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();

    Store::open(&file_key(dir.path())).unwrap();

    let mode = fs::metadata(dir.path().join("master.key"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn the_key_file_directory_is_created_if_missing() {
    let dir = tempfile::tempdir().unwrap();
    let keys = KeySource {
        keyring: None,
        key_file: dir.path().join("config/syzygy/master.key"),
    };

    Store::open(&keys).unwrap();

    assert!(keys.key_file.exists());
}

#[test]
fn a_file_written_under_another_key_does_not_decrypt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.json");
    let other = tempfile::tempdir().unwrap();
    Store::open(&file_key(other.path()))
        .unwrap()
        .write_json(&path, &prefs())
        .unwrap();

    let store = Store::open(&file_key(dir.path())).unwrap();

    assert!(matches!(
        store.read_json::<Prefs>(&path),
        Err(Error::Decrypt(_))
    ));
}

#[test]
fn a_tampered_file_does_not_decrypt() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("prefs.json");
    store.write_json(&path, &prefs()).unwrap();
    let mut raw = fs::read(&path).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 0xff;
    fs::write(&path, raw).unwrap();

    assert!(matches!(
        store.read_json::<Prefs>(&path),
        Err(Error::Decrypt(_))
    ));
}

/// syzygy never stores plaintext, so it never reads it either.
#[test]
fn a_plaintext_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("prefs.json");
    fs::write(&path, serde_json::to_vec(&prefs()).unwrap()).unwrap();

    assert!(matches!(
        store.read_json::<Prefs>(&path),
        Err(Error::Decrypt(_))
    ));
}

/// A key file that exists but isn't a key is never overwritten: that would
/// silently orphan everything encrypted under the old key.
#[test]
fn an_invalid_key_file_fails_to_open_and_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let keys = file_key(dir.path());
    fs::write(&keys.key_file, b"too short").unwrap();

    assert!(matches!(Store::open(&keys), Err(Error::Key(_))));
    assert_eq!(fs::read(&keys.key_file).unwrap(), b"too short");
}

/// The keyring is unavailable (none configured) and the key file can't be
/// written: there is nowhere to keep a key, so opening fails.
#[test]
fn opening_fails_when_no_key_can_be_kept() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-dir");
    fs::write(&blocker, b"").unwrap();
    let keys = KeySource {
        keyring: None,
        key_file: blocker.join("master.key"),
    };

    assert!(matches!(Store::open(&keys), Err(Error::Key(_))));
}

#[test]
fn removing_a_file_deletes_it_and_tolerates_a_missing_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("prefs.json");
    store.write_json(&path, &prefs()).unwrap();

    store.remove(&path).unwrap();
    store.remove(&path).unwrap();

    assert_eq!(store.read_json::<Prefs>(&path).unwrap(), None);
}

/// A format this build doesn't know is refused, not misread.
#[test]
fn a_file_from_a_newer_format_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&file_key(dir.path())).unwrap();
    let path = dir.path().join("prefs.json");
    store.write_json(&path, &prefs()).unwrap();
    let mut raw = fs::read(&path).unwrap();
    raw[4] += 1;
    fs::write(&path, raw).unwrap();

    assert!(matches!(
        store.read_json::<Prefs>(&path),
        Err(Error::Decrypt(_))
    ));
}
