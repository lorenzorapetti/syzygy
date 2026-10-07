use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use crate::Error;
use crate::crypto::{Crypto, KeySource};

/// Encrypted JSON files. A cheap `Clone` handle around the master key.
#[derive(Clone)]
pub struct Store {
    pub(crate) crypto: Arc<Crypto>,
}

impl Store {
    /// Load (or create) the master key. Fails when neither the keyring nor
    /// the key file can hold one, in which case nothing may be stored.
    pub fn open(keys: &KeySource) -> Result<Self, Error> {
        Ok(Self {
            crypto: Arc::new(Crypto::load(keys)?),
        })
    }

    /// Read and decrypt a JSON file. `Ok(None)` when the file doesn't exist.
    pub fn read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<Option<T>, Error> {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let plain = self.crypto.decrypt(&data)?;
        Ok(Some(serde_json::from_slice(&plain)?))
    }

    /// Encrypt and write a JSON file, creating parent directories. The write
    /// goes through a temporary file and a rename, so a crash never leaves a
    /// half-written file behind.
    pub fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), Error> {
        let json = serde_json::to_vec(value)?;
        let encrypted = self.crypto.encrypt(&json)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        fs::write(&tmp, encrypted)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Delete a file. A file that's already gone is not an error.
    pub fn remove(&self, path: &Path) -> Result<(), Error> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
