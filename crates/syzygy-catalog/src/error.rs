/// A Catalog read or edit that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tidal(#[from] syzygy_tidal::Error),
    /// A read of the user's Library before TIDAL has said who they are.
    #[error("syzygy doesn't know who's signed in yet")]
    UnknownUser,
    /// TIDAL answered for a track with something that isn't one.
    #[error("TIDAL sent something other than track {0}")]
    NotATrack(u64),
}

impl Error {
    /// TIDAL has nothing at that id: the Page doesn't exist.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Error::Tidal(syzygy_tidal::Error::Api { status: 404, .. })
        )
    }

    /// TIDAL refused to add a track that the playlist has already, as a
    /// 409 or an error that says "dupe" (sone's test).
    pub fn is_duplicate(&self) -> bool {
        matches!(
            self,
            Error::Tidal(syzygy_tidal::Error::Api { status: 409, .. })
        ) || self.to_string().to_lowercase().contains("dupe")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(status: u16, body: &str) -> Error {
        Error::Tidal(syzygy_tidal::Error::Api {
            status,
            body: body.to_string(),
        })
    }

    #[test]
    fn a_conflict_or_a_dupe_is_a_duplicate() {
        assert!(api(409, "").is_duplicate());
        assert!(api(400, r#"{"userMessage":"Playlist DUPES not allowed"}"#).is_duplicate());
        assert!(!api(400, "bad request").is_duplicate());
        assert!(!Error::UnknownUser.is_duplicate());
    }
}
