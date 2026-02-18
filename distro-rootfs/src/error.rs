/// Errors from rootfs caching and extraction.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error propagated from the [`distro`] crate (download, verification).
    #[error("distro error: {0}")]
    Distro(#[from] distro::Error),

    /// A filesystem I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Cache metadata JSON could not be parsed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The archive has an unrecognized file extension.
    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),
}
