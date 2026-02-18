/// Errors from distro download and verification.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The requested distribution name is not recognized.
    #[error("unsupported distribution: {0}")]
    UnsupportedDistro(String),

    /// The requested version is not available for the given distribution.
    #[error("unsupported version {version} for {distro}")]
    UnsupportedVersion {
        /// Distribution name.
        distro: String,
        /// Requested version string.
        version: String,
    },

    /// An HTTP request failed (network error or non-2xx status).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Downloaded data does not match the expected checksum.
    #[error("SHA256 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Hash from the checksum file or index.
        expected: String,
        /// Hash computed from the downloaded data.
        actual: String,
    },

    /// The checksum file could not be parsed or the target filename was not found.
    #[error("failed to parse checksum file")]
    ChecksumParse,

    /// A filesystem I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The requested distro/version/arch combination was not found in the
    /// Simplestreams index.
    #[error("product not found: {distro} {version} ({arch})")]
    ProductNotFound {
        /// Distribution name.
        distro: String,
        /// Requested version string.
        version: String,
        /// Target architecture.
        arch: String,
    },

    /// A product exists in the index but has no rootfs download.
    #[error("rootfs not found in product: {product_key}")]
    RootfsNotFound {
        /// The Simplestreams product key (e.g. `"alpine:3.21:amd64:default"`).
        product_key: String,
    },
}
