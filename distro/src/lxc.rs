//! LXC Images Simplestreams client.
//!
//! Provides a unified interface to download rootfs images for all supported
//! distributions from [images.linuxcontainers.org](https://images.linuxcontainers.org)
//! or compatible mirrors.
//!
//! The Simplestreams protocol uses a static JSON index (`images.json`) that
//! lists all available products with their download paths and SHA256 checksums.

use std::collections::HashMap;

use serde::Deserialize;
use tracing::{debug, info};

use crate::mirror::Mirror;
use crate::{Arch, Distro, Error, Version};

/// Resolved image info from the Simplestreams index.
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    /// Full download URL.
    pub url: String,
    /// Expected SHA256 hash of the file.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// Filename (e.g. "rootfs.tar.xz").
    pub filename: String,
}

/// Client for the LXC Images Simplestreams API.
pub struct LxcClient {
    mirror: Mirror,
    http: reqwest::Client,
}

impl LxcClient {
    /// Creates a new client backed by the given mirror.
    pub fn new(mirror: Mirror) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("arcbox/0.1")
            .build()
            .expect("failed to build HTTP client");
        Self { mirror, http }
    }

    /// Resolves the download URL and SHA256 for a rootfs image.
    pub async fn resolve(
        &self,
        distro: Distro,
        version: &Version,
        arch: Arch,
    ) -> Result<ResolvedImage, Error> {
        let index = self.fetch_index().await?;
        self.resolve_from_index(&index, distro, version, arch)
    }

    /// Fetches and parses the Simplestreams images.json index.
    pub async fn fetch_index(&self) -> Result<SimplestreamsIndex, Error> {
        let url = self.mirror.streams_url();
        info!(mirror = %self.mirror, url = %url, "fetching simplestreams index");

        let response = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?;
        let index: SimplestreamsIndex = response.json().await?;

        debug!(products = index.products.len(), "index loaded");
        Ok(index)
    }

    /// Resolves an image from a pre-fetched index.
    pub fn resolve_from_index(
        &self,
        index: &SimplestreamsIndex,
        distro: Distro,
        version: &Version,
        arch: Arch,
    ) -> Result<ResolvedImage, Error> {
        let lxc_distro = distro.lxc_name();
        let lxc_release = distro.lxc_release(version);
        let lxc_arch = arch.lxc_name();

        // Try "default" variant first, then "cloud".
        let variants = ["default", "cloud"];
        let mut product = None;
        let mut used_key = String::new();

        for variant in &variants {
            let key = format!("{lxc_distro}:{lxc_release}:{lxc_arch}:{variant}");
            if let Some(p) = index.products.get(&key) {
                product = Some(p);
                used_key = key;
                break;
            }
        }

        let product = product.ok_or_else(|| Error::ProductNotFound {
            distro: distro.as_str().to_owned(),
            version: version.as_str().to_owned(),
            arch: lxc_arch.to_owned(),
        })?;

        debug!(key = %used_key, "found product");

        // Get the latest version (keys are date strings like "20260218_07:42").
        let latest_version = product
            .versions
            .keys()
            .max()
            .ok_or_else(|| Error::RootfsNotFound {
                product_key: used_key.clone(),
            })?;

        let version_data = &product.versions[latest_version];

        // Find rootfs.tar.xz item. Try common ftype names.
        let rootfs_item = version_data
            .items
            .values()
            .find(|item| item.ftype == "root.tar.xz")
            .or_else(|| {
                version_data
                    .items
                    .values()
                    .find(|item| item.path.ends_with("rootfs.tar.xz"))
            })
            .ok_or_else(|| Error::RootfsNotFound {
                product_key: used_key,
            })?;

        let filename = rootfs_item
            .path
            .rsplit('/')
            .next()
            .unwrap_or("rootfs.tar.xz")
            .to_owned();

        Ok(ResolvedImage {
            url: self.mirror.image_url(&rootfs_item.path),
            sha256: rootfs_item.sha256.clone(),
            size: rootfs_item.size,
            filename,
        })
    }
}

// ---------------------------------------------------------------------------
// Simplestreams JSON types
// ---------------------------------------------------------------------------

/// Top-level Simplestreams `images.json` structure.
#[derive(Debug, Deserialize)]
pub struct SimplestreamsIndex {
    /// Map from product key (e.g. `"alpine:3.21:amd64:default"`) to product.
    pub products: HashMap<String, Product>,
}

/// A single product (distro + release + arch + variant).
#[derive(Debug, Deserialize)]
pub struct Product {
    /// Architecture string (e.g. `"amd64"`).
    pub arch: String,
    /// OS name (e.g. `"Alpine"`).
    pub os: String,
    /// Release identifier (e.g. `"3.21"`, `"noble"`).
    pub release: String,
    /// Human-readable release title (e.g. `"24.04 LTS"`).
    #[serde(default)]
    pub release_title: String,
    /// Image variant (e.g. `"default"`, `"cloud"`).
    #[serde(default)]
    pub variant: String,
    /// Map from build timestamp (e.g. `"20260218_07:42"`) to version data.
    pub versions: HashMap<String, ProductVersion>,
}

/// A specific build of a product.
#[derive(Debug, Deserialize)]
pub struct ProductVersion {
    /// Map from item key (e.g. `"root.tar.xz"`) to downloadable file.
    pub items: HashMap<String, Item>,
}

/// A downloadable file within a product version.
#[derive(Debug, Deserialize)]
pub struct Item {
    /// File type identifier (e.g. `"root.tar.xz"`, `"lxd.tar.xz"`).
    pub ftype: String,
    /// SHA-256 hex digest.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// Relative path on the mirror (e.g. `"images/alpine/3.21/amd64/..."`).
    pub path: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_index() -> SimplestreamsIndex {
        let json = r#"{
            "products": {
                "alpine:3.21:amd64:default": {
                    "arch": "amd64",
                    "os": "Alpine",
                    "release": "3.21",
                    "release_title": "3.21",
                    "variant": "default",
                    "versions": {
                        "20260217_13:00": {
                            "items": {
                                "root.tar.xz": {
                                    "ftype": "root.tar.xz",
                                    "sha256": "aabbccdd",
                                    "size": 3145728,
                                    "path": "images/alpine/3.21/amd64/default/20260217_13:00/rootfs.tar.xz"
                                },
                                "lxd.tar.xz": {
                                    "ftype": "lxd.tar.xz",
                                    "sha256": "11223344",
                                    "size": 440,
                                    "path": "images/alpine/3.21/amd64/default/20260217_13:00/lxd.tar.xz"
                                }
                            }
                        },
                        "20260218_13:00": {
                            "items": {
                                "root.tar.xz": {
                                    "ftype": "root.tar.xz",
                                    "sha256": "eeff0011",
                                    "size": 3200000,
                                    "path": "images/alpine/3.21/amd64/default/20260218_13:00/rootfs.tar.xz"
                                }
                            }
                        }
                    }
                },
                "ubuntu:noble:arm64:default": {
                    "arch": "arm64",
                    "os": "Ubuntu",
                    "release": "noble",
                    "release_title": "24.04 LTS",
                    "variant": "default",
                    "versions": {
                        "20260218_07:42": {
                            "items": {
                                "root.tar.xz": {
                                    "ftype": "root.tar.xz",
                                    "sha256": "ubuntuhash",
                                    "size": 314572800,
                                    "path": "images/ubuntu/noble/arm64/default/20260218_07:42/rootfs.tar.xz"
                                }
                            }
                        }
                    }
                }
            }
        }"#;
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn resolve_alpine() {
        let client = LxcClient::new(Mirror::Official);
        let index = mock_index();
        let result = client
            .resolve_from_index(&index, Distro::Alpine, &Version::new("3.21"), Arch::X86_64)
            .unwrap();

        // Should pick the latest version (20260218)
        assert_eq!(result.sha256, "eeff0011");
        assert_eq!(result.size, 3200000);
        assert_eq!(result.filename, "rootfs.tar.xz");
        assert!(result.url.contains("20260218_13:00"));
    }

    #[test]
    fn resolve_ubuntu_codename() {
        let client = LxcClient::new(Mirror::Official);
        let index = mock_index();
        // "24.04" should map to "noble" for the product key lookup
        let result = client
            .resolve_from_index(&index, Distro::Ubuntu, &Version::new("24.04"), Arch::Aarch64)
            .unwrap();

        assert_eq!(result.sha256, "ubuntuhash");
        assert!(result.url.contains("ubuntu/noble/arm64"));
    }

    #[test]
    fn resolve_not_found() {
        let client = LxcClient::new(Mirror::Official);
        let index = mock_index();
        let result =
            client.resolve_from_index(&index, Distro::Fedora, &Version::new("41"), Arch::X86_64);
        assert!(result.is_err());
    }

    #[test]
    fn product_key_format() {
        let distro = Distro::Rocky;
        let version = Version::new("9");
        let arch = Arch::X86_64;
        let key = format!(
            "{}:{}:{}:default",
            distro.lxc_name(),
            distro.lxc_release(&version),
            arch.lxc_name()
        );
        assert_eq!(key, "rockylinux:9:amd64:default");
    }

    #[test]
    fn mirror_url_in_resolved() {
        let client = LxcClient::new(Mirror::Tuna);
        let index = mock_index();
        let result = client
            .resolve_from_index(&index, Distro::Alpine, &Version::new("3.21"), Arch::X86_64)
            .unwrap();
        assert!(result.url.starts_with("https://mirrors.tuna.tsinghua.edu.cn/lxc-images/"));
    }
}
