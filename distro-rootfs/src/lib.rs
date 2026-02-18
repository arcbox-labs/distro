#![warn(missing_docs)]

//! Linux distribution rootfs extraction, caching, and lifecycle management.
//!
//! This crate builds on top of [`distro`] to provide:
//! - Local caching of downloaded rootfs archives
//! - Archive extraction (tar.gz, tar.xz)
//! - Cache pruning and management
//! - Mirror selection for LXC Images source
//!
//! # Example
//!
//! ```no_run
//! use distro::{Distro, Arch, Mirror};
//! use distro_rootfs::RootfsManager;
//!
//! # async fn example() -> Result<(), distro_rootfs::Error> {
//! let manager = RootfsManager::new("~/.local/share/arcbox/rootfs")?;
//!
//! // Ensure rootfs is downloaded and cached (uses LXC Images by default).
//! let rootfs = manager.ensure(
//!     Distro::Alpine,
//!     &"3.21".into(),
//!     Arch::current(),
//!     &Mirror::default(),
//!     |downloaded, total| {
//!         eprintln!("{downloaded}/{total} bytes");
//!     },
//! ).await?;
//!
//! // Extract to a target directory.
//! rootfs.extract_to("/tmp/alpine-rootfs")?;
//! # Ok(())
//! # }
//! ```

mod cache;
mod error;
mod extract;

pub use cache::CachedRootfs;
pub use error::Error;
pub use extract::ExtractFormat;

use std::path::{Path, PathBuf};

use distro::{Arch, Distro, Mirror, Version};
use tracing::{debug, info};

/// Manages rootfs downloads, caching, and extraction.
pub struct RootfsManager {
    cache_dir: PathBuf,
}

impl RootfsManager {
    /// Creates a new manager with the given cache directory.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Result<Self, Error> {
        let cache_dir = cache_dir.into();
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Ensures a rootfs archive is available locally, downloading if necessary.
    ///
    /// Uses LXC Images (via the specified mirror) as the download source,
    /// which supports all 16 distros through a unified API.
    pub async fn ensure<F>(
        &self,
        distro: Distro,
        version: &Version,
        arch: Arch,
        mirror: &Mirror,
        on_progress: F,
    ) -> Result<CachedRootfs, Error>
    where
        F: FnMut(u64, u64),
    {
        let entry_dir = self.entry_dir(distro, version, arch);

        // Check cache first.
        if let Some(cached) = cache::load_cached(&entry_dir)? {
            info!(
                distro = %distro,
                version = %version,
                arch = %arch,
                "using cached rootfs"
            );
            return Ok(cached);
        }

        // Download from LXC Images.
        info!(distro = %distro, version = %version, arch = %arch, mirror = %mirror, "downloading rootfs");
        let result =
            distro::download_from_lxc(distro, version, arch, mirror, on_progress).await?;

        // Save to cache.
        std::fs::create_dir_all(&entry_dir)?;
        let cached = cache::store(&entry_dir, &result)?;

        debug!(path = %cached.archive_path.display(), "rootfs cached");
        Ok(cached)
    }

    /// Lists all cached rootfs entries.
    pub fn list_cached(&self) -> Result<Vec<CachedRootfs>, Error> {
        cache::list_all(&self.cache_dir)
    }

    /// Removes cached rootfs entries, keeping only the N most recent per distro.
    pub fn prune(&self, keep_latest: usize) -> Result<u64, Error> {
        cache::prune(&self.cache_dir, keep_latest)
    }

    /// Returns the cache directory path for a specific distro/version/arch combination.
    fn entry_dir(&self, distro: Distro, version: &Version, arch: Arch) -> PathBuf {
        self.cache_dir
            .join(distro.as_str())
            .join(version.as_str())
            .join(arch.linux_name())
    }
}

/// Returns the default cache directory (`~/.local/share/arcbox/rootfs`).
pub fn default_cache_dir() -> PathBuf {
    dirs_cache_dir().join("arcbox").join("rootfs")
}

fn dirs_cache_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        // Follow XDG on macOS for developer tooling (not ~/Library).
        if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(dir);
        }
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(".local/share");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(dir);
        }
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(".local/share");
        }
    }
    PathBuf::from("/tmp")
}
