use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, Generate, Key, KeyInit, Nonce};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

use crate::Error;

const MAGIC: &[u8; 4] = b"SYZY";
const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
/// 4 (magic) + 1 (version) + 12 (nonce) = 17 bytes header before ciphertext.
const HEADER_LEN: usize = MAGIC.len() + 1 + NONCE_LEN;
const KEY_LEN: usize = 32;

/// Where the master key lives. The binary owns these names; the store
/// hardcodes no app identity.
#[derive(Debug, Clone)]
pub struct KeySource {
    /// The OS keyring entry tried first. `None` skips the keyring.
    pub keyring: Option<KeyringEntry>,
    /// Fallback key file, created with mode 0600.
    pub key_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct KeyringEntry {
    pub service: String,
    pub entry: String,
}

pub(crate) struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    /// Load or generate the master key and construct the cipher.
    ///
    /// Key sources, in order:
    /// 1. the OS keyring entry, if any;
    /// 2. the key file;
    /// 3. a new key, written to the key file (always, since the keyring may
    ///    be unreachable on the next launch) and to the keyring.
    ///
    /// Fails when no key can be loaded or kept. A key file that exists but
    /// can't be read as a key is an error, never overwritten: replacing it
    /// would orphan everything encrypted under the old key.
    pub(crate) fn load(source: &KeySource) -> Result<Self, Error> {
        let mut raw_key = load_or_generate_key(source)?;
        let key = Key::<Aes256Gcm>::from(raw_key);
        let cipher = Aes256Gcm::new(&key);
        raw_key.zeroize();
        Ok(Self { cipher })
    }

    /// Encrypt plaintext. Returns `[magic][version][nonce][ciphertext+tag]`.
    pub(crate) fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| Error::Encrypt(e.to_string()))?;

        let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt data written by [`Crypto::encrypt`]. Anything without the magic
    /// header is refused: syzygy never stores plaintext.
    pub(crate) fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, Error> {
        if data.len() < HEADER_LEN || &data[..MAGIC.len()] != MAGIC {
            return Err(Error::Decrypt("not encrypted by syzygy".into()));
        }
        let version = data[MAGIC.len()];
        if version != VERSION {
            return Err(Error::Decrypt(format!("unknown format version {version}")));
        }

        let nonce = Nonce::<Aes256Gcm>::try_from(&data[MAGIC.len() + 1..HEADER_LEN])
            .map_err(|_| Error::Decrypt("bad nonce".into()))?;
        let ciphertext = &data[HEADER_LEN..];

        self.cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|e| Error::Decrypt(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Key management
// ---------------------------------------------------------------------------

fn load_or_generate_key(source: &KeySource) -> Result<[u8; KEY_LEN], Error> {
    // 1. Try the OS keyring.
    if let Some(entry) = &source.keyring {
        match load_key_from_keyring(entry) {
            Ok(key) => {
                // Keep the file backup too, so a keyring that's unreachable
                // on a later launch doesn't lead to a new key.
                if !source.key_file.exists()
                    && let Err(e) = store_key_in_file(&source.key_file, &key)
                {
                    log::warn!("Could not back up the master key to a file: {e}");
                }
                return Ok(key);
            }
            Err(e) => log::debug!("Keyring load failed (will try file): {e}"),
        }
    }

    // 2. Try the file fallback.
    if let Some(key) = load_key_from_file(&source.key_file)? {
        if let Some(entry) = &source.keyring {
            // Also try to store it in the keyring for next time.
            if let Err(e) = store_key_in_keyring(entry, &key) {
                log::debug!("Could not store key in keyring: {e}");
            }
        }
        return Ok(key);
    }

    // 3. Generate a new key.
    log::info!("Generating new encryption master key");
    let key: [u8; KEY_LEN] = Key::<Aes256Gcm>::generate().into();

    // Always write the file backup: the keyring may be unreachable on the
    // next launch (e.g. AppImage with a different D-Bus session).
    store_key_in_file(&source.key_file, &key)?;

    if let Some(entry) = &source.keyring {
        match store_key_in_keyring(entry, &key) {
            Ok(()) => log::info!("Master key stored in OS keyring + file backup"),
            Err(e) => log::info!("Keyring unavailable ({e}), using file-based key"),
        }
    }

    Ok(key)
}

fn load_key_from_keyring(entry: &KeyringEntry) -> Result<[u8; KEY_LEN], String> {
    let entry = keyring::Entry::new(&entry.service, &entry.entry).map_err(|e| e.to_string())?;
    let mut secret = entry.get_secret().map_err(|e| e.to_string())?;
    let key = <[u8; KEY_LEN]>::try_from(secret.as_slice())
        .map_err(|_| format!("keyring key wrong length: {}", secret.len()));
    secret.zeroize();
    key
}

fn store_key_in_keyring(entry: &KeyringEntry, key: &[u8; KEY_LEN]) -> Result<(), String> {
    let entry = keyring::Entry::new(&entry.service, &entry.entry).map_err(|e| e.to_string())?;
    entry.set_secret(key).map_err(|e| e.to_string())
}

/// `Ok(None)` when there is no key file yet.
fn load_key_from_file(path: &Path) -> Result<Option<[u8; KEY_LEN]>, Error> {
    let mut data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Key(format!("reading {}: {e}", path.display()))),
    };
    let key = <[u8; KEY_LEN]>::try_from(data.as_slice()).map_err(|_| {
        Error::Key(format!(
            "key file {} has the wrong length: {}",
            path.display(),
            data.len()
        ))
    });
    data.zeroize();
    key.map(Some)
}

fn store_key_in_file(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), Error> {
    write_private(path, key).map_err(|e| Error::Key(format!("writing {}: {e}", path.display())))?;
    log::info!("Master key stored at {} (mode 0600)", path.display());
    Ok(())
}

/// Create the file with mode 0600 from the start, so the key is never
/// readable by others, even briefly. It's written to a temporary file and
/// renamed into place, so a crash never leaves a truncated key behind.
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
}
