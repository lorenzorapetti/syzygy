//! The Session: the tokens, which Login method they belong to, and who the
//! user is. Kept in its own encrypted `session.json`, apart from `Settings`.
//!
//! After Session expiry the file stays without tokens, so the next sign-in
//! can tell whether the same user is back.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syzygy_store::Store;
use syzygy_tidal::LoginMethod;
use syzygy_tidal::models::AuthTokens;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// `None` after Session expiry: signed out until the user signs in
    /// again.
    pub tokens: Option<AuthTokens>,
    /// Which Login method, and so which embedded credential pair, the tokens
    /// belong to.
    pub login_method: LoginMethod,
    /// Missing until the token response or `get_session_info` reports it.
    pub user_id: Option<u64>,
    /// The account's country, from `get_session_info`.
    pub country_code: Option<String>,
}

impl Session {
    pub fn is_signed_in(&self) -> bool {
        self.tokens.is_some()
    }

    /// Session expiry: the tokens go, who the user is and their country
    /// stay.
    pub fn expire(&mut self) {
        self.tokens = None;
    }

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

/// Whether `user_id`, signing in, is `previous`, the user last signed in
/// on this machine, whose queue, cache and sorts are still here. Someone
/// unknown on either side is someone else.
pub fn same_user(previous: Option<u64>, user_id: Option<u64>) -> bool {
    previous.is_some() && previous == user_id
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
            tokens: Some(AuthTokens {
                access_token: "access".into(),
                refresh_token: "refresh".into(),
                expires_in: 3600,
                token_type: "Bearer".into(),
                user_id: Some(42),
            }),
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

    #[test]
    fn an_expired_session_loads_back_knowing_who_and_where_but_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let path = dir.path().join("session.json");
        let mut expired = session();
        expired.expire();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime
            .block_on(expired.save(store.clone(), path.clone()))
            .unwrap();
        let loaded = Session::load(&store, &path).unwrap();

        assert!(!loaded.is_signed_in());
        assert_eq!(loaded.user_id, Some(42));
        assert_eq!(loaded.country_code.as_deref(), Some("IT"));
    }

    #[test]
    fn a_session_with_tokens_is_signed_in() {
        assert!(session().is_signed_in());
    }

    #[test]
    fn only_the_same_known_user_is_the_same_user() {
        assert!(same_user(Some(42), Some(42)));
        assert!(!same_user(Some(42), Some(7)));
        // Nobody known to compare to, or the one signing in isn't known:
        // whatever is on the machine may be someone else's.
        assert!(!same_user(None, Some(42)));
        assert!(!same_user(Some(42), None));
        assert!(!same_user(None, None));
    }
}
