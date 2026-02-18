#![warn(missing_docs)]

//! Linux distribution metadata, download, and verification.
//!
//! This crate provides:
//! - Distribution registry with version and URL resolution
//! - Architecture detection
//! - HTTP download with progress callbacks and SHA256 verification
//! - LXC Images (Simplestreams) unified source for all distros
//! - Mirror selection (official, TUNA, USTC, custom R2)
//!
//! # Example
//!
//! ```no_run
//! use distro::{Distro, Version, Arch, download_distro};
//!
//! # async fn example() -> Result<(), distro::Error> {
//! let bytes = download_distro(
//!     Distro::Alpine,
//!     &Version::new("3.20"),
//!     Arch::current(),
//!     |downloaded, total| {
//!         eprintln!("{downloaded}/{total} bytes");
//!     },
//! ).await?;
//! # Ok(())
//! # }
//! ```

mod arch;
mod download;
mod error;
pub mod lxc;
pub mod mirror;
pub mod provider;

pub use arch::Arch;
pub use download::{download_distro, download_from_lxc, download_with_verification, DownloadResult};
pub use error::Error;
pub use mirror::Mirror;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported Linux distributions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Distro {
    /// AlmaLinux — RHEL-compatible enterprise distribution.
    Alma,
    /// Alpine Linux — lightweight musl-based distribution.
    Alpine,
    /// Arch Linux — rolling-release distribution.
    Arch,
    /// CentOS Stream — upstream RHEL development platform.
    CentOS,
    /// Debian — universal, stable distribution.
    Debian,
    /// Devuan — Debian fork without systemd.
    Devuan,
    /// Fedora — cutting-edge RPM-based distribution.
    Fedora,
    /// Gentoo — source-based distribution.
    Gentoo,
    /// Kali Linux — penetration testing distribution.
    Kali,
    /// NixOS — declarative, reproducible distribution.
    NixOS,
    /// openEuler — enterprise distribution by Huawei.
    OpenEuler,
    /// openSUSE — community RPM-based distribution.
    OpenSuse,
    /// Oracle Linux — RHEL-compatible enterprise distribution.
    Oracle,
    /// Rocky Linux — RHEL-compatible community distribution.
    Rocky,
    /// Ubuntu — popular Debian-based distribution.
    Ubuntu,
    /// Void Linux — independent rolling-release distribution.
    Void,
}

impl Distro {
    /// Returns the string identifier used in cache paths.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Alma => "alma",
            Self::Alpine => "alpine",
            Self::Arch => "arch",
            Self::CentOS => "centos",
            Self::Debian => "debian",
            Self::Devuan => "devuan",
            Self::Fedora => "fedora",
            Self::Gentoo => "gentoo",
            Self::Kali => "kali",
            Self::NixOS => "nixos",
            Self::OpenEuler => "openeuler",
            Self::OpenSuse => "opensuse",
            Self::Oracle => "oracle",
            Self::Rocky => "rocky",
            Self::Ubuntu => "ubuntu",
            Self::Void => "void",
        }
    }

    /// Returns the name used in LXC Images (images.linuxcontainers.org).
    pub fn lxc_name(&self) -> &'static str {
        match self {
            Self::Alma => "almalinux",
            Self::Alpine => "alpine",
            Self::Arch => "archlinux",
            Self::CentOS => "centos",
            Self::Debian => "debian",
            Self::Devuan => "devuan",
            Self::Fedora => "fedora",
            Self::Gentoo => "gentoo",
            Self::Kali => "kali",
            Self::NixOS => "nixos",
            Self::OpenEuler => "openeuler",
            Self::OpenSuse => "opensuse",
            Self::Oracle => "oracle",
            Self::Rocky => "rockylinux",
            Self::Ubuntu => "ubuntu",
            Self::Void => "voidlinux",
        }
    }

    /// Returns the default version for this distribution.
    pub fn default_version(&self) -> Version {
        match self {
            Self::Alma => Version::new("9"),
            Self::Alpine => Version::new("3.21"),
            Self::Arch => Version::new("current"),
            Self::CentOS => Version::new("9-Stream"),
            Self::Debian => Version::new("12"),
            Self::Devuan => Version::new("daedalus"),
            Self::Fedora => Version::new("41"),
            Self::Gentoo => Version::new("current"),
            Self::Kali => Version::new("current"),
            Self::NixOS => Version::new("25.05"),
            Self::OpenEuler => Version::new("24.03"),
            Self::OpenSuse => Version::new("tumbleweed"),
            Self::Oracle => Version::new("9"),
            Self::Rocky => Version::new("9"),
            Self::Ubuntu => Version::new("24.04"),
            Self::Void => Version::new("current"),
        }
    }

    /// Maps a user-facing version to the LXC release name.
    ///
    /// For most distros the version is used as-is, but some distros use
    /// codenames in LXC (e.g. Ubuntu "24.04" → "noble").
    pub fn lxc_release(&self, version: &Version) -> String {
        match self {
            Self::Ubuntu => match version.as_str() {
                "20.04" => "focal".to_owned(),
                "22.04" => "jammy".to_owned(),
                "24.04" => "noble".to_owned(),
                "24.10" => "oracular".to_owned(),
                "25.04" => "plucky".to_owned(),
                other => other.to_owned(),
            },
            Self::Debian => match version.as_str() {
                "10" => "buster".to_owned(),
                "11" => "bullseye".to_owned(),
                "12" => "bookworm".to_owned(),
                "13" => "trixie".to_owned(),
                other => other.to_owned(),
            },
            Self::Devuan => match version.as_str() {
                "4" => "chimaera".to_owned(),
                "5" => "daedalus".to_owned(),
                "6" => "excalibur".to_owned(),
                other => other.to_owned(),
            },
            Self::OpenSuse => match version.as_str() {
                "15.6" => "15.6".to_owned(),
                "16.0" => "16.0".to_owned(),
                "tumbleweed" => "tumbleweed".to_owned(),
                other => other.to_owned(),
            },
            _ => version.as_str().to_owned(),
        }
    }

    /// Returns all supported distributions.
    pub fn all() -> &'static [Distro] {
        &[
            Self::Alma,
            Self::Alpine,
            Self::Arch,
            Self::CentOS,
            Self::Debian,
            Self::Devuan,
            Self::Fedora,
            Self::Gentoo,
            Self::Kali,
            Self::NixOS,
            Self::OpenEuler,
            Self::OpenSuse,
            Self::Oracle,
            Self::Rocky,
            Self::Ubuntu,
            Self::Void,
        ]
    }
}

impl fmt::Display for Distro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A distribution version string (e.g. "3.20", "24.04", "bookworm").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version(String);

impl Version {
    /// Creates a new version from a string (e.g. `"3.21"`, `"24.04"`).
    pub fn new(version: &str) -> Self {
        Self(version.to_owned())
    }

    /// Returns the version as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Version {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Parse a distro spec string like "alpine:3.20" or "ubuntu".
///
/// If no version is specified, the default version for that distro is used.
pub fn parse_distro_spec(spec: &str) -> Result<(Distro, Version), Error> {
    let (name, version) = match spec.split_once(':') {
        Some((n, v)) => (n, Some(v)),
        None => (spec, None),
    };

    let distro = match name.to_lowercase().as_str() {
        "alma" | "almalinux" => Distro::Alma,
        "alpine" => Distro::Alpine,
        "arch" | "archlinux" => Distro::Arch,
        "centos" => Distro::CentOS,
        "debian" => Distro::Debian,
        "devuan" => Distro::Devuan,
        "fedora" => Distro::Fedora,
        "gentoo" => Distro::Gentoo,
        "kali" => Distro::Kali,
        "nixos" => Distro::NixOS,
        "openeuler" => Distro::OpenEuler,
        "opensuse" => Distro::OpenSuse,
        "oracle" => Distro::Oracle,
        "rocky" | "rockylinux" => Distro::Rocky,
        "ubuntu" => Distro::Ubuntu,
        "void" | "voidlinux" => Distro::Void,
        _ => return Err(Error::UnsupportedDistro(name.to_owned())),
    };

    let version = match version {
        Some(v) => Version::new(v),
        None => distro.default_version(),
    };

    Ok((distro, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_version() {
        let (d, v) = parse_distro_spec("alpine:3.20").unwrap();
        assert_eq!(d, Distro::Alpine);
        assert_eq!(v.as_str(), "3.20");
    }

    #[test]
    fn parse_without_version() {
        let (d, v) = parse_distro_spec("ubuntu").unwrap();
        assert_eq!(d, Distro::Ubuntu);
        assert_eq!(v.as_str(), "24.04");
    }

    #[test]
    fn parse_aliases() {
        assert_eq!(parse_distro_spec("almalinux").unwrap().0, Distro::Alma);
        assert_eq!(parse_distro_spec("archlinux").unwrap().0, Distro::Arch);
        assert_eq!(parse_distro_spec("rockylinux").unwrap().0, Distro::Rocky);
        assert_eq!(parse_distro_spec("voidlinux").unwrap().0, Distro::Void);
    }

    #[test]
    fn parse_unsupported() {
        assert!(parse_distro_spec("windows").is_err());
    }

    #[test]
    fn lxc_release_ubuntu() {
        assert_eq!(Distro::Ubuntu.lxc_release(&Version::new("24.04")), "noble");
        assert_eq!(Distro::Ubuntu.lxc_release(&Version::new("22.04")), "jammy");
    }

    #[test]
    fn lxc_release_debian() {
        assert_eq!(Distro::Debian.lxc_release(&Version::new("12")), "bookworm");
        assert_eq!(Distro::Debian.lxc_release(&Version::new("13")), "trixie");
    }

    #[test]
    fn lxc_release_passthrough() {
        assert_eq!(Distro::Alpine.lxc_release(&Version::new("3.21")), "3.21");
        assert_eq!(Distro::Fedora.lxc_release(&Version::new("41")), "41");
    }

    #[test]
    fn all_distros_count() {
        assert_eq!(Distro::all().len(), 16);
    }

    #[test]
    fn lxc_names_unique() {
        let names: Vec<_> = Distro::all().iter().map(|d| d.lxc_name()).collect();
        let mut deduped = names.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(names.len(), deduped.len());
    }
}
