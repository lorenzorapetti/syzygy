/// A Catalog read or edit that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tidal(#[from] syzygy_tidal::Error),
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
