/// Everything that can go wrong in `syzygy-store`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No master key could be loaded or kept, from the keyring or the key file.
    #[error("no encryption key available: {0}")]
    Key(String),

    /// Data was not encrypted by syzygy, was encrypted under another key, or
    /// was tampered with.
    #[error("decryption failed: {0}")]
    Decrypt(String),

    #[error("encryption failed: {0}")]
    Encrypt(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
