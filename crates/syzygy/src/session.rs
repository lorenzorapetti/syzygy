//! The Session: the tokens, which Login method they belong to, and who the
//! user is. Kept in its own encrypted `session.json`, apart from `Settings`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syzygy_store::Store;
use syzygy_tidal::LoginMethod;
use syzygy_tidal::models::AuthTokens;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub tokens: AuthTokens,
    /// Which Login method, and so which embedded credential pair, the tokens
    /// belong to.
    pub login_method: LoginMethod,
    /// Missing until the token response or `get_session_info` reports it.
    pub user_id: Option<u64>,
    /// The account's country, from `get_session_info`.
    pub country_code: Option<String>,
}

impl Session {
    /// Read the session file. `None` when there is none or it can't be read,
    /// which means signing in again. Unlike settings, an unreadable file is
    /// not kept aside: signing in replaces it and loses nothing.
    pub fn load(store: &Store, path: &Path) -> Option<Self> {
        match store.read_json(path) {
            Ok(session) => session,
            Err(e) => {
                log::warn!("Could not read {}, signing in again: {e}", path.display());
                None
            }
        }
    }

    /// Encrypt and write the session off the UI thread.
    pub fn save(
        self,
        store: Store,
        path: PathBuf,
    ) -> impl Future<Output = Result<(), Arc<syzygy_store::Error>>> + use<> {
        crate::persist::write_json(store, path, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syzygy_store::KeySource;

    fn store(dir: &std::path::Path) -> Store {
        Store::open(&KeySource {
            keyring: None,
            key_file: dir.join("master.key"),
        })
        .unwrap()
    }

    fn session() -> Session {
        Session {
            tokens: AuthTokens {
                access_token: "access".into(),
                refresh_token: "refresh".into(),
                expires_in: 3600,
                token_type: "Bearer".into(),
                user_id: Some(42),
            },
            login_method: LoginMethod::DeviceCode,
            user_id: Some(42),
            country_code: Some("IT".into()),
        }
    }

    #[test]
    fn a_saved_session_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let path = dir.path().join("session.json");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime
            .block_on(session().save(store.clone(), path.clone()))
            .unwrap();

        assert_eq!(Session::load(&store, &path), Some(session()));
    }

    #[test]
    fn no_session_file_means_signed_out() {
        let dir = tempfile::tempdir().unwrap();

        let loaded = Session::load(&store(dir.path()), &dir.path().join("session.json"));

        assert_eq!(loaded, None);
    }

    #[test]
    fn an_unreadable_session_file_means_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"garbage").unwrap();

        assert_eq!(Session::load(&store(dir.path()), &path), None);
    }
}
