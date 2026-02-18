use futures::StreamExt;
use sha2::{Digest, Sha256, Sha512};
use tracing::{debug, info};

use crate::lxc::LxcClient;
use crate::mirror::Mirror;
use crate::provider::HashAlgorithm;
use crate::{Arch, Distro, Error, Version};

/// Result of a successful download.
pub struct DownloadResult {
    /// Raw bytes of the downloaded archive.
    pub data: Vec<u8>,
    /// SHA256 hex digest of the downloaded data (always computed).
    pub sha256: String,
    /// Original filename from the URL.
    pub filename: String,
}

impl DownloadResult {
    /// Compute a SHA-512 digest of the downloaded data.
    pub fn sha512(&self) -> String {
        hex::encode(Sha512::digest(&self.data))
    }
}

fn actual_hash(result: &DownloadResult, algorithm: HashAlgorithm) -> String {
    match algorithm {
        HashAlgorithm::Sha256 => result.sha256.clone(),
        HashAlgorithm::Sha512 => result.sha512(),
    }
}

fn verify_hash(
    expected: &str,
    result: &DownloadResult,
    algorithm: HashAlgorithm,
) -> Result<(), Error> {
    let actual = actual_hash(result, algorithm);
    if actual != expected {
        return Err(Error::ChecksumMismatch {
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}

/// Downloads a distro rootfs using LXC Images as the source.
///
/// This is the recommended method — it supports all 16 distributions through
/// a single unified API.
pub async fn download_from_lxc<F>(
    distro: Distro,
    version: &Version,
    arch: Arch,
    mirror: &Mirror,
    mut on_progress: F,
) -> Result<DownloadResult, Error>
where
    F: FnMut(u64, u64),
{
    let client = LxcClient::new(mirror.clone());
    let resolved = client.resolve(distro, version, arch).await?;

    info!(
        distro = %distro,
        version = %version,
        arch = %arch,
        mirror = %mirror,
        url = %resolved.url,
        "downloading from LXC images"
    );

    let data = download_url(&resolved.url, &mut on_progress).await?;
    let sha256 = hex::encode(Sha256::digest(&data));

    // Verify SHA256 against the value from the Simplestreams index.
    if sha256 != resolved.sha256 {
        return Err(Error::ChecksumMismatch {
            expected: resolved.sha256,
            actual: sha256,
        });
    }

    info!("SHA256 checksum verified");

    Ok(DownloadResult {
        data,
        sha256,
        filename: resolved.filename,
    })
}

/// Downloads a distro image from the official source using DistroSpec templates.
///
/// Only available for distros that have an official DistroSpec defined
/// (Alpine, Ubuntu, Debian, Fedora). For all other distros, use
/// [`download_from_lxc`] instead.
pub async fn download_distro<F>(
    distro: Distro,
    version: &Version,
    arch: Arch,
    on_progress: F,
) -> Result<DownloadResult, Error>
where
    F: FnMut(u64, u64),
{
    let provider = crate::provider::get_official_provider(distro)
        .ok_or_else(|| Error::UnsupportedDistro(distro.as_str().to_owned()))?;
    let url = provider.rootfs_url(version, arch);
    let filename = url.rsplit('/').next().unwrap_or("rootfs.tar.gz").to_owned();

    info!(distro = %distro, version = %version, arch = %arch, url = %url, "downloading from official source");

    let data = download_url(&url, on_progress).await?;
    let sha256 = hex::encode(Sha256::digest(&data));

    debug!(sha256 = %sha256, size = data.len(), "download complete");

    Ok(DownloadResult {
        data,
        sha256,
        filename,
    })
}

/// Downloads from the official source with checksum verification.
///
/// Only available for distros with DistroSpec (Alpine, Ubuntu, Debian, Fedora).
pub async fn download_with_verification<F>(
    distro: Distro,
    version: &Version,
    arch: Arch,
    on_progress: F,
) -> Result<DownloadResult, Error>
where
    F: FnMut(u64, u64),
{
    let provider = crate::provider::get_official_provider(distro)
        .ok_or_else(|| Error::UnsupportedDistro(distro.as_str().to_owned()))?;
    let result = download_distro(distro, version, arch, on_progress).await?;

    // Fetch and verify checksum if available.
    if let Some(checksum_url) = provider.checksum_url(version, arch) {
        info!(url = %checksum_url, "fetching checksum");

        let checksum_data = download_url(&checksum_url, |_, _| {}).await?;
        let checksum_text = String::from_utf8_lossy(&checksum_data);
        let expected = provider.parse_checksum(&checksum_text, &result.filename)?;

        let algorithm = provider.hash_algorithm();
        verify_hash(&expected, &result, algorithm)?;
        info!(algorithm = ?algorithm, "checksum verified");
    }

    Ok(result)
}

/// Downloads raw bytes from a URL with streaming progress.
pub(crate) async fn download_url<F>(url: &str, mut on_progress: F) -> Result<Vec<u8>, Error>
where
    F: FnMut(u64, u64),
{
    let client = reqwest::Client::builder()
        .user_agent("arcbox/0.1")
        .build()?;

    let response = client.get(url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);

    let mut data = Vec::with_capacity(total as usize);
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        data.extend_from_slice(&chunk);
        on_progress(downloaded, total);
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_result_sha256_consistent() {
        let data = b"hello world";
        let sha256 = hex::encode(Sha256::digest(data));
        let result = DownloadResult {
            data: data.to_vec(),
            sha256: sha256.clone(),
            filename: "test.tar.gz".to_owned(),
        };
        assert_eq!(result.sha256, sha256);
    }

    #[test]
    fn download_result_sha512() {
        let data = b"hello world";
        let expected = hex::encode(Sha512::digest(data));
        let result = DownloadResult {
            data: data.to_vec(),
            sha256: hex::encode(Sha256::digest(data)),
            filename: "test.tar.xz".to_owned(),
        };
        assert_eq!(result.sha512(), expected);
        // SHA-512 and SHA-256 must differ.
        assert_ne!(result.sha256, result.sha512());
    }

    #[test]
    fn hash_algorithm_dispatch() {
        // Verify that the dispatch logic selects the correct hash for each
        // algorithm variant.
        let data = b"test payload";
        let result = DownloadResult {
            data: data.to_vec(),
            sha256: hex::encode(Sha256::digest(data)),
            filename: "rootfs.tar.xz".to_owned(),
        };

        let sha256_actual = actual_hash(&result, HashAlgorithm::Sha256);
        assert_eq!(sha256_actual, result.sha256);

        let sha512_actual = actual_hash(&result, HashAlgorithm::Sha512);
        assert_eq!(sha512_actual, hex::encode(Sha512::digest(data)));
        assert_ne!(sha256_actual, sha512_actual);
    }

    #[test]
    fn verify_hash_success_sha256() {
        let data = b"verify ok";
        let result = DownloadResult {
            data: data.to_vec(),
            sha256: hex::encode(Sha256::digest(data)),
            filename: "rootfs.tar.gz".to_owned(),
        };
        assert!(verify_hash(&result.sha256, &result, HashAlgorithm::Sha256).is_ok());
    }

    #[test]
    fn verify_hash_mismatch_returns_error() {
        let data = b"verify mismatch";
        let result = DownloadResult {
            data: data.to_vec(),
            sha256: hex::encode(Sha256::digest(data)),
            filename: "rootfs.tar.xz".to_owned(),
        };

        let err = verify_hash("deadbeef", &result, HashAlgorithm::Sha256).unwrap_err();
        match err {
            Error::ChecksumMismatch { expected, actual } => {
                assert_eq!(expected, "deadbeef");
                assert_eq!(actual, result.sha256);
            }
            _ => panic!("unexpected error variant"),
        }
    }
}
