/// A Catalog read or edit that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tidal(#[from] syzygy_tidal::Error),
    /// A read of the user's Library before TIDAL has said who they are.
    #[error("syzygy doesn't know who's signed in yet")]
    UnknownUser,
}

impl Error {
    /// TIDAL has nothing at that id: the Page doesn't exist.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Error::Tidal(syzygy_tidal::Error::Api { status: 404, .. })
        )
    }
}
