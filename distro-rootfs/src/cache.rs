use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::Error;

/// Metadata stored alongside a cached rootfs archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// Distribution name (e.g. `"alpine"`).
    pub distro: String,
    /// Version string (e.g. `"3.21"`).
    pub version: String,
    /// Architecture (e.g. `"aarch64"`).
    pub arch: String,
    /// SHA-256 hex digest of the archive file.
    pub sha256: String,
    /// Archive filename on disk (e.g. `"rootfs.tar.xz"`).
    pub filename: String,
    /// Archive size in bytes.
    pub size: u64,
    /// Unix timestamp (seconds) when the archive was downloaded.
    pub downloaded_at: String,
}

/// A handle to a cached rootfs archive on disk.
#[derive(Debug, Clone)]
pub struct CachedRootfs {
    /// Absolute path to the archive file.
    pub archive_path: PathBuf,
    /// Associated metadata (distro, version, checksum, etc.).
    pub metadata: CacheMetadata,
}

impl CachedRootfs {
    /// Extracts the cached archive to the target directory.
    pub fn extract_to(&self, target: impl AsRef<Path>) -> Result<(), Error> {
        let format = crate::extract::ExtractFormat::detect(&self.archive_path)?;
        crate::extract::extract_archive(&self.archive_path, target.as_ref(), format)
    }

    /// Verifies the archive's SHA256 against the stored metadata using
    /// streaming I/O (8 KiB chunks) to avoid loading the entire file into
    /// memory.
    pub fn verify_integrity(&self) -> Result<bool, Error> {
        let file = std::fs::File::open(&self.archive_path)?;
        let mut reader = std::io::BufReader::with_capacity(8192, file);
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual = hex::encode(hasher.finalize());
        Ok(actual == self.metadata.sha256)
    }
}

/// Loads a cached entry from a directory, verifying integrity.
///
/// Computes a streaming SHA256 over the archive and compares against the
/// stored metadata. If the checksum does not match, the corrupted entry is
/// removed and `None` is returned so a fresh download will be triggered.
pub(crate) fn load_cached(entry_dir: &Path) -> Result<Option<CachedRootfs>, Error> {
    let Some(cached) = load_entry(entry_dir)? else {
        return Ok(None);
    };

    if !cached.verify_integrity()? {
        warn!(
            path = %cached.archive_path.display(),
            expected = %cached.metadata.sha256,
            "cached rootfs integrity check failed, removing corrupted entry"
        );
        let _ = std::fs::remove_dir_all(entry_dir);
        return Ok(None);
    }

    Ok(Some(cached))
}

/// Loads a cached entry without verifying integrity.
///
/// Used by [`list_all`] and [`prune`] to avoid reading every archive file
/// when only metadata is needed.
fn load_entry(entry_dir: &Path) -> Result<Option<CachedRootfs>, Error> {
    let metadata_path = entry_dir.join("metadata.json");
    if !metadata_path.exists() {
        return Ok(None);
    }

    let metadata: CacheMetadata = serde_json::from_str(&std::fs::read_to_string(&metadata_path)?)?;
    let archive_path = entry_dir.join(&metadata.filename);

    if !archive_path.exists() {
        // Metadata exists but archive is missing — treat as uncached.
        return Ok(None);
    }

    Ok(Some(CachedRootfs {
        archive_path,
        metadata,
    }))
}

/// Stores a download result into the cache entry directory.
pub(crate) fn store(
    entry_dir: &Path,
    result: &distro::DownloadResult,
) -> Result<CachedRootfs, Error> {
    let archive_path = entry_dir.join(&result.filename);
    std::fs::write(&archive_path, &result.data)?;

    // Extract distro/version/arch from the directory structure.
    let components: Vec<&str> = entry_dir
        .components()
        .rev()
        .take(3)
        .map(|c| c.as_os_str().to_str().unwrap_or("unknown"))
        .collect();

    let metadata = CacheMetadata {
        distro: components.get(2).unwrap_or(&"unknown").to_string(),
        version: components.get(1).unwrap_or(&"unknown").to_string(),
        arch: components.first().unwrap_or(&"unknown").to_string(),
        sha256: result.sha256.clone(),
        filename: result.filename.clone(),
        size: result.data.len() as u64,
        downloaded_at: chrono_now(),
    };

    let metadata_path = entry_dir.join("metadata.json");
    std::fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

    Ok(CachedRootfs {
        archive_path,
        metadata,
    })
}

/// Lists all cached rootfs entries under the cache root.
pub(crate) fn list_all(cache_dir: &Path) -> Result<Vec<CachedRootfs>, Error> {
    let mut entries = Vec::new();

    if !cache_dir.exists() {
        return Ok(entries);
    }

    // Walk: cache_dir/{distro}/{version}/{arch}/metadata.json
    for distro_entry in std::fs::read_dir(cache_dir)? {
        let distro_dir = distro_entry?.path();
        if !distro_dir.is_dir() {
            continue;
        }
        for version_entry in std::fs::read_dir(&distro_dir)? {
            let version_dir = version_entry?.path();
            if !version_dir.is_dir() {
                continue;
            }
            for arch_entry in std::fs::read_dir(&version_dir)? {
                let arch_dir = arch_entry?.path();
                if !arch_dir.is_dir() {
                    continue;
                }
                if let Some(cached) = load_entry(&arch_dir)? {
                    entries.push(cached);
                }
            }
        }
    }

    Ok(entries)
}

/// Prunes old cache entries, keeping at most `keep_latest` per distro.
/// Returns the number of bytes freed.
pub(crate) fn prune(cache_dir: &Path, keep_latest: usize) -> Result<u64, Error> {
    let mut freed = 0u64;
    let all = list_all(cache_dir)?;

    // Group by distro.
    let mut by_distro: std::collections::HashMap<String, Vec<CachedRootfs>> =
        std::collections::HashMap::new();
    for entry in all {
        by_distro
            .entry(entry.metadata.distro.clone())
            .or_default()
            .push(entry);
    }

    for (_distro, mut entries) in by_distro {
        // Sort by download time (newest first).
        entries.sort_by(|a, b| b.metadata.downloaded_at.cmp(&a.metadata.downloaded_at));

        // Remove entries beyond the keep limit.
        for old in entries.into_iter().skip(keep_latest) {
            if let Some(parent) = old.archive_path.parent() {
                if std::fs::remove_dir_all(parent).is_ok() {
                    freed += old.metadata.size;
                }
            }
        }
    }

    Ok(freed)
}

/// Returns the current UTC timestamp as an ISO 8601 string.
fn chrono_now() -> String {
    // Avoid pulling in chrono — use a simple format.
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_result(content: &[u8], filename: &str) -> distro::DownloadResult {
        let sha256 = hex::encode(Sha256::digest(content));
        distro::DownloadResult {
            data: content.to_vec(),
            sha256,
            filename: filename.to_owned(),
        }
    }

    #[test]
    fn store_and_load_cached() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("alpine").join("3.21").join("aarch64");
        std::fs::create_dir_all(&entry).unwrap();

        let result = make_test_result(b"fake rootfs data", "rootfs.tar.gz");
        let cached = store(&entry, &result).unwrap();

        assert_eq!(cached.metadata.sha256, result.sha256);
        assert_eq!(cached.metadata.filename, "rootfs.tar.gz");
        assert!(cached.archive_path.exists());

        // Load should succeed.
        let loaded = load_cached(&entry).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.metadata.sha256, result.sha256);
    }

    #[test]
    fn load_cached_missing_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("alpine").join("3.21").join("x86_64");
        std::fs::create_dir_all(&entry).unwrap();

        assert!(load_cached(&entry).unwrap().is_none());
    }

    #[test]
    fn load_cached_missing_archive() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("debian").join("12").join("amd64");
        std::fs::create_dir_all(&entry).unwrap();

        // Write metadata but no archive file.
        let metadata = CacheMetadata {
            distro: "debian".to_owned(),
            version: "12".to_owned(),
            arch: "amd64".to_owned(),
            sha256: "deadbeef".to_owned(),
            filename: "rootfs.tar.xz".to_owned(),
            size: 100,
            downloaded_at: "0".to_owned(),
        };
        std::fs::write(
            entry.join("metadata.json"),
            serde_json::to_string(&metadata).unwrap(),
        )
        .unwrap();

        assert!(load_cached(&entry).unwrap().is_none());
    }

    #[test]
    fn load_cached_corrupted_archive() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("ubuntu").join("24.04").join("arm64");
        std::fs::create_dir_all(&entry).unwrap();

        // Store a valid entry first.
        let result = make_test_result(b"original data", "rootfs.tar.xz");
        store(&entry, &result).unwrap();

        // Corrupt the archive.
        std::fs::write(entry.join("rootfs.tar.xz"), b"corrupted").unwrap();

        // Load should detect corruption and return None.
        let loaded = load_cached(&entry).unwrap();
        assert!(loaded.is_none());
        // The corrupted entry should have been cleaned up.
        assert!(!entry.exists());
    }

    #[test]
    fn list_all_empty() {
        let dir = tempfile::tempdir().unwrap();
        let entries = list_all(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn list_all_with_entries() {
        let dir = tempfile::tempdir().unwrap();

        // Create two entries.
        for (distro, ver) in [("alpine", "3.21"), ("debian", "12")] {
            let entry = dir.path().join(distro).join(ver).join("aarch64");
            std::fs::create_dir_all(&entry).unwrap();
            let result = make_test_result(
                format!("data-{distro}").as_bytes(),
                "rootfs.tar.gz",
            );
            store(&entry, &result).unwrap();
        }

        let entries = list_all(dir.path()).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn prune_keeps_latest() {
        let dir = tempfile::tempdir().unwrap();

        // Create 3 entries for the same distro with different timestamps.
        for (i, ver) in ["1", "2", "3"].iter().enumerate() {
            let entry = dir.path().join("alpine").join(ver).join("aarch64");
            std::fs::create_dir_all(&entry).unwrap();
            let result = make_test_result(
                format!("data-{ver}").as_bytes(),
                "rootfs.tar.gz",
            );
            let mut cached = store(&entry, &result).unwrap();
            // Set increasing timestamps so "3" is newest.
            cached.metadata.downloaded_at = format!("{}", 1000 + i);
            std::fs::write(
                entry.join("metadata.json"),
                serde_json::to_string_pretty(&cached.metadata).unwrap(),
            )
            .unwrap();
        }

        // Keep only 1 latest — should free 2.
        let freed = prune(dir.path(), 1).unwrap();
        assert!(freed > 0);

        let remaining = list_all(dir.path()).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].metadata.version, "3");
    }

    #[test]
    fn verify_integrity_valid() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("alpine").join("3.21").join("aarch64");
        std::fs::create_dir_all(&entry).unwrap();

        let result = make_test_result(b"valid content", "rootfs.tar.gz");
        let cached = store(&entry, &result).unwrap();
        assert!(cached.verify_integrity().unwrap());
    }

    #[test]
    fn verify_integrity_corrupted() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("debian").join("12").join("arm64");
        std::fs::create_dir_all(&entry).unwrap();

        let result = make_test_result(b"original", "rootfs.tar.xz");
        let cached = store(&entry, &result).unwrap();

        // Corrupt the archive on disk.
        std::fs::write(&cached.archive_path, b"tampered").unwrap();
        assert!(!cached.verify_integrity().unwrap());
    }

    #[test]
    fn prune_only_counts_successful_deletions() {
        let dir = tempfile::tempdir().unwrap();

        // Create 2 entries.
        for ver in ["1", "2"] {
            let entry = dir.path().join("fedora").join(ver).join("x86_64");
            std::fs::create_dir_all(&entry).unwrap();
            let result = make_test_result(b"data", "rootfs.tar.gz");
            let mut cached = store(&entry, &result).unwrap();
            cached.metadata.downloaded_at = if ver == "1" {
                "1000".to_owned()
            } else {
                "2000".to_owned()
            };
            std::fs::write(
                entry.join("metadata.json"),
                serde_json::to_string_pretty(&cached.metadata).unwrap(),
            )
            .unwrap();
        }

        // Prune keeping 1.
        let freed = prune(dir.path(), 1).unwrap();
        // Only the deleted entry's size should be counted.
        assert_eq!(freed, 4); // b"data".len() == 4
    }
}
