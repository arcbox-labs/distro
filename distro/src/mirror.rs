//! LXC Images mirror selection for downloading rootfs archives.

use serde::{Deserialize, Serialize};
use std::fmt;

/// LXC Images mirror selection.
///
/// All mirrors serve the same Simplestreams API and image files.
/// Choose a mirror based on geographic proximity or network conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Mirror {
    /// Official server: images.linuxcontainers.org (Canada, GeoIP DNS)
    Official,
    /// Tsinghua University TUNA: mirrors.tuna.tsinghua.edu.cn/lxc-images
    Tuna,
    /// USTC: mirrors.ustc.edu.cn/lxc-images
    Ustc,
    /// Beijing Foreign Studies University: mirrors.bfsu.edu.cn/lxc-images
    Bfsu,
    /// Custom mirror URL (e.g. Cloudflare R2 self-hosted CDN)
    Custom(String),
}

impl Mirror {
    /// Returns the base URL for this mirror (no trailing slash).
    pub fn base_url(&self) -> &str {
        match self {
            Self::Official => "https://images.linuxcontainers.org",
            Self::Tuna => "https://mirrors.tuna.tsinghua.edu.cn/lxc-images",
            Self::Ustc => "https://mirrors.ustc.edu.cn/lxc-images",
            Self::Bfsu => "https://mirrors.bfsu.edu.cn/lxc-images",
            Self::Custom(url) => url.trim_end_matches('/'),
        }
    }

    /// Returns the Simplestreams index URL.
    pub fn streams_url(&self) -> String {
        format!("{}/streams/v1/images.json", self.base_url())
    }

    /// Returns the full download URL for a given image path from the index.
    pub fn image_url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url(), path)
    }

    /// Returns all preset mirrors (excluding Custom).
    pub fn presets() -> &'static [Mirror] {
        &[Self::Official, Self::Tuna, Self::Ustc, Self::Bfsu]
    }
}

impl Default for Mirror {
    fn default() -> Self {
        Self::Official
    }
}

impl fmt::Display for Mirror {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Official => write!(f, "official"),
            Self::Tuna => write!(f, "tuna"),
            Self::Ustc => write!(f, "ustc"),
            Self::Bfsu => write!(f, "bfsu"),
            Self::Custom(url) => write!(f, "custom({url})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls() {
        assert_eq!(
            Mirror::Official.base_url(),
            "https://images.linuxcontainers.org"
        );
        assert_eq!(
            Mirror::Tuna.base_url(),
            "https://mirrors.tuna.tsinghua.edu.cn/lxc-images"
        );
    }

    #[test]
    fn streams_url() {
        assert_eq!(
            Mirror::Official.streams_url(),
            "https://images.linuxcontainers.org/streams/v1/images.json"
        );
    }

    #[test]
    fn image_url() {
        let path = "images/alpine/3.21/amd64/default/20260218/rootfs.tar.xz";
        assert_eq!(
            Mirror::Official.image_url(path),
            "https://images.linuxcontainers.org/images/alpine/3.21/amd64/default/20260218/rootfs.tar.xz"
        );
    }

    #[test]
    fn custom_mirror() {
        let m = Mirror::Custom("https://images.arcbox.dev".to_owned());
        assert_eq!(m.base_url(), "https://images.arcbox.dev");
        assert_eq!(
            m.streams_url(),
            "https://images.arcbox.dev/streams/v1/images.json"
        );
    }

    #[test]
    fn custom_trailing_slash() {
        let m = Mirror::Custom("https://example.com/".to_owned());
        assert_eq!(m.base_url(), "https://example.com");
    }
}
