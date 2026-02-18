//! Distribution-specific URL resolution and checksum parsing.
//!
//! All distros share a single [`TemplateProvider`] driven by a [`DistroSpec`]
//! config. Adding a new distro only requires adding a new `DistroSpec` entry —
//! no new types or trait implementations needed.

use crate::{Arch, Distro, Error, Version};

// ---------------------------------------------------------------------------
// Hash algorithm
// ---------------------------------------------------------------------------

/// Hash algorithm used in checksum files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-256 (used by most distros).
    Sha256,
    /// SHA-512 (used by Debian).
    Sha512,
}

// ---------------------------------------------------------------------------
// Checksum format
// ---------------------------------------------------------------------------

/// Describes how a distro's checksum file is formatted.
#[derive(Debug, Clone, Copy)]
pub enum ChecksumFormat {
    /// Single-file entry: the first whitespace-delimited token on the first
    /// line is the hash. Used by Alpine (e.g. `<hash>  <filename>\n`).
    SingleEntry,
    /// GNU coreutils style with multiple files:
    /// `<hash> *<filename>` or `<hash>  <filename>`, one per line.
    /// Matches by exact filename. Used by Ubuntu and Debian.
    GnuCoreutils,
    /// BSD style: `SHA256 (<filename>) = <hash>`. Used by Fedora.
    Bsd,
}

// ---------------------------------------------------------------------------
// Architecture naming
// ---------------------------------------------------------------------------

/// How the architecture string appears in URLs.
#[derive(Debug, Clone, Copy)]
pub enum ArchNaming {
    /// Linux kernel style: `aarch64` / `x86_64`.
    Linux,
    /// Debian style: `arm64` / `amd64`.
    Debian,
}

impl ArchNaming {
    fn resolve(self, arch: Arch) -> &'static str {
        match self {
            Self::Linux => arch.linux_name(),
            Self::Debian => arch.deb_name(),
        }
    }
}

// ---------------------------------------------------------------------------
// Version resolver
// ---------------------------------------------------------------------------

/// How to transform the raw version string before interpolating into URLs.
#[derive(Debug, Clone, Copy)]
pub enum VersionTransform {
    /// Use the version string as-is.
    Identity,
    /// Extract `major.minor` from a potentially longer version string
    /// (e.g. `3.21.3` → `3.21`). Used by Alpine.
    MajorMinor,
}

/// A static table that maps version numbers to codenames.
type CodenameTable = &'static [(&'static str, &'static str)];

// ---------------------------------------------------------------------------
// DistroSpec — the data that fully describes a distro
// ---------------------------------------------------------------------------

/// Static configuration that fully describes how to download and verify a
/// distribution's rootfs image. Everything is `&'static` so specs can live as
/// constants.
pub struct DistroSpec {
    /// URL template for the rootfs archive. Supported placeholders:
    /// - `{version}`  — raw version string (e.g. "3.21.3")
    /// - `{arch}`     — resolved via `arch_naming`
    /// - `{codename}` — resolved via `codename_table` (empty string if None)
    /// - `{major_minor}` — resolved via `version_transform`
    pub rootfs_url: &'static str,

    /// URL template for the checksum file, same placeholders as `rootfs_url`.
    pub checksum_url: Option<&'static str>,

    /// Format of the checksum file.
    pub checksum_format: ChecksumFormat,

    /// Hash algorithm used in the checksum file.
    pub hash_algorithm: HashAlgorithm,

    /// How architecture names appear in URLs.
    pub arch_naming: ArchNaming,

    /// Optional version → codename mapping table.
    pub codename_table: Option<CodenameTable>,

    /// Default codename when version is not found in the table.
    pub default_codename: &'static str,

    /// How to derive `{major_minor}` from the version string.
    pub version_transform: VersionTransform,
}

// ---------------------------------------------------------------------------
// TemplateProvider — the one provider to rule them all
// ---------------------------------------------------------------------------

/// A single provider implementation driven entirely by a [`DistroSpec`].
pub struct TemplateProvider {
    spec: &'static DistroSpec,
}

impl TemplateProvider {
    /// Creates a provider from a static distro specification.
    pub const fn new(spec: &'static DistroSpec) -> Self {
        Self { spec }
    }

    /// Resolves all placeholders in a URL template.
    fn resolve_url(&self, template: &str, version: &Version, arch: Arch) -> String {
        let arch_str = self.spec.arch_naming.resolve(arch);
        let codename = self.resolve_codename(version);
        let major_minor = self.resolve_major_minor(version);

        template
            .replace("{version}", version.as_str())
            .replace("{arch}", arch_str)
            .replace("{codename}", codename)
            .replace("{major_minor}", &major_minor)
    }

    fn resolve_codename(&self, version: &Version) -> &'static str {
        match self.spec.codename_table {
            Some(table) => table
                .iter()
                .find(|(v, _)| *v == version.as_str())
                .map(|(_, c)| *c)
                .unwrap_or(self.spec.default_codename),
            None => self.spec.default_codename,
        }
    }

    fn resolve_major_minor(&self, version: &Version) -> String {
        match self.spec.version_transform {
            VersionTransform::Identity => version.as_str().to_owned(),
            VersionTransform::MajorMinor => {
                let v = version.as_str();
                // "3.21.3" → "3.21", "3.21" stays "3.21"
                if let Some(first_dot) = v.find('.') {
                    if let Some(second_dot) = v[first_dot + 1..].find('.') {
                        return v[..first_dot + 1 + second_dot].to_owned();
                    }
                }
                v.to_owned()
            }
        }
    }

    /// Parses a checksum file according to the spec's format.
    fn parse_checksum_impl(
        &self,
        content: &str,
        filename: &str,
    ) -> Result<String, Error> {
        match self.spec.checksum_format {
            ChecksumFormat::SingleEntry => {
                // First whitespace-delimited token on the first line.
                content
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().next())
                    .map(|s| s.to_lowercase())
                    .ok_or(Error::ChecksumParse)
            }
            ChecksumFormat::GnuCoreutils => {
                // "<hash> *<filename>" or "<hash>  <filename>", exact filename match.
                for line in content.lines() {
                    let Some((hash, rest)) = line.split_once(char::is_whitespace) else {
                        continue;
                    };
                    // Strip the optional binary indicator '*' and leading whitespace.
                    let fname = rest.trim_start().trim_start_matches('*');
                    if fname == filename {
                        return Ok(hash.to_lowercase());
                    }
                }
                Err(Error::ChecksumParse)
            }
            ChecksumFormat::Bsd => {
                // "SHA256 (<filename>) = <hash>", exact filename match.
                for line in content.lines() {
                    if !line.starts_with("SHA") {
                        continue;
                    }
                    let Some(start) = line.find('(') else {
                        continue;
                    };
                    let Some(end) = line.find(')') else {
                        continue;
                    };
                    if &line[start + 1..end] == filename {
                        return line
                            .rsplit('=')
                            .next()
                            .map(|s| s.trim().to_lowercase())
                            .ok_or(Error::ChecksumParse);
                    }
                }
                Err(Error::ChecksumParse)
            }
        }
    }

    // -- Public interface matching the old DistroProvider trait ----------------

    /// Returns the resolved rootfs download URL for the given version and arch.
    pub fn rootfs_url(&self, version: &Version, arch: Arch) -> String {
        self.resolve_url(self.spec.rootfs_url, version, arch)
    }

    /// Returns the resolved checksum file URL, if one is defined for this distro.
    pub fn checksum_url(&self, version: &Version, arch: Arch) -> Option<String> {
        self.spec
            .checksum_url
            .map(|tpl| self.resolve_url(tpl, version, arch))
    }

    /// Parses a checksum file's content and extracts the hash for `filename`.
    pub fn parse_checksum(&self, content: &str, filename: &str) -> Result<String, Error> {
        self.parse_checksum_impl(content, filename)
    }

    /// Returns the hash algorithm used by the checksum file.
    pub fn hash_algorithm(&self) -> HashAlgorithm {
        self.spec.hash_algorithm
    }
}

// ---------------------------------------------------------------------------
// Static distro specs
// ---------------------------------------------------------------------------

/// Alpine Linux official source specification.
pub static ALPINE: DistroSpec = DistroSpec {
    rootfs_url: "https://dl-cdn.alpinelinux.org/alpine/v{major_minor}/releases/{arch}/alpine-minirootfs-{version}-{arch}.tar.gz",
    checksum_url: Some("https://dl-cdn.alpinelinux.org/alpine/v{major_minor}/releases/{arch}/alpine-minirootfs-{version}-{arch}.tar.gz.sha256"),
    checksum_format: ChecksumFormat::SingleEntry,
    hash_algorithm: HashAlgorithm::Sha256,
    arch_naming: ArchNaming::Linux,
    codename_table: None,
    default_codename: "",
    version_transform: VersionTransform::MajorMinor,
};

/// Ubuntu cloud images official source specification.
pub static UBUNTU: DistroSpec = DistroSpec {
    rootfs_url: "https://cloud-images.ubuntu.com/{codename}/current/{codename}-server-cloudimg-{arch}-root.tar.xz",
    checksum_url: Some("https://cloud-images.ubuntu.com/{codename}/current/SHA256SUMS"),
    checksum_format: ChecksumFormat::GnuCoreutils,
    hash_algorithm: HashAlgorithm::Sha256,
    arch_naming: ArchNaming::Debian,
    codename_table: Some(&[
        ("20.04", "focal"),
        ("22.04", "jammy"),
        ("24.04", "noble"),
        ("24.10", "oracular"),
        ("25.04", "plucky"),
    ]),
    default_codename: "noble",
    version_transform: VersionTransform::Identity,
};

/// Debian cloud images official source specification.
pub static DEBIAN: DistroSpec = DistroSpec {
    rootfs_url: "https://cloud.debian.org/images/cloud/{codename}/latest/debian-{version}-nocloud-{arch}.tar.xz",
    checksum_url: Some("https://cloud.debian.org/images/cloud/{codename}/latest/SHA512SUMS"),
    checksum_format: ChecksumFormat::GnuCoreutils,
    hash_algorithm: HashAlgorithm::Sha512,
    arch_naming: ArchNaming::Debian,
    codename_table: Some(&[
        ("10", "buster"),
        ("11", "bullseye"),
        ("12", "bookworm"),
        ("13", "trixie"),
    ]),
    default_codename: "bookworm",
    version_transform: VersionTransform::Identity,
};

/// Fedora cloud images official source specification.
pub static FEDORA: DistroSpec = DistroSpec {
    rootfs_url: "https://download.fedoraproject.org/pub/fedora/linux/releases/{version}/Cloud/{arch}/images/Fedora-Cloud-Base-{version}-1.2.{arch}.raw.xz",
    checksum_url: Some("https://download.fedoraproject.org/pub/fedora/linux/releases/{version}/Cloud/{arch}/images/Fedora-Cloud-{version}-1.2-{arch}-CHECKSUM"),
    checksum_format: ChecksumFormat::Bsd,
    hash_algorithm: HashAlgorithm::Sha256,
    arch_naming: ArchNaming::Linux,
    codename_table: None,
    default_codename: "",
    version_transform: VersionTransform::Identity,
};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Returns the official template provider for a given distro, if one is defined.
///
/// Only Alpine, Ubuntu, Debian, and Fedora have official DistroSpec templates.
/// For other distros, use the LXC Images source instead.
pub fn get_official_provider(distro: Distro) -> Option<TemplateProvider> {
    let spec = match distro {
        Distro::Alpine => &ALPINE,
        Distro::Ubuntu => &UBUNTU,
        Distro::Debian => &DEBIAN,
        Distro::Fedora => &FEDORA,
        _ => return None,
    };
    Some(TemplateProvider::new(spec))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Alpine ---------------------------------------------------------------

    #[test]
    fn alpine_rootfs_url() {
        let p = get_official_provider(Distro::Alpine).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("3.21.3"), Arch::Aarch64),
            "https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/aarch64/alpine-minirootfs-3.21.3-aarch64.tar.gz"
        );
    }

    #[test]
    fn alpine_rootfs_url_short_version() {
        let p = get_official_provider(Distro::Alpine).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("3.21"), Arch::X86_64),
            "https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/x86_64/alpine-minirootfs-3.21-x86_64.tar.gz"
        );
    }

    #[test]
    fn alpine_parse_checksum() {
        let p = get_official_provider(Distro::Alpine).unwrap();
        let content = "abc123def456  alpine-minirootfs-3.20.0-aarch64.tar.gz\n";
        assert_eq!(
            p.parse_checksum(content, "alpine-minirootfs-3.20.0-aarch64.tar.gz").unwrap(),
            "abc123def456"
        );
    }

    // -- Ubuntu ---------------------------------------------------------------

    #[test]
    fn ubuntu_rootfs_url() {
        let p = get_official_provider(Distro::Ubuntu).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("24.04"), Arch::Aarch64),
            "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-arm64-root.tar.xz"
        );
    }

    #[test]
    fn ubuntu_rootfs_url_x86() {
        let p = get_official_provider(Distro::Ubuntu).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("22.04"), Arch::X86_64),
            "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64-root.tar.xz"
        );
    }

    #[test]
    fn ubuntu_parse_checksum() {
        let p = get_official_provider(Distro::Ubuntu).unwrap();
        let content = "\
abc111 *noble-server-cloudimg-amd64.img
def222 *noble-server-cloudimg-arm64-root.tar.xz
ghi333 *noble-server-cloudimg-amd64-root.tar.xz
";
        assert_eq!(
            p.parse_checksum(content, "noble-server-cloudimg-arm64-root.tar.xz").unwrap(),
            "def222"
        );
    }

    // -- Debian ---------------------------------------------------------------

    #[test]
    fn debian_rootfs_url_aarch64() {
        let p = get_official_provider(Distro::Debian).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("12"), Arch::Aarch64),
            "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-nocloud-arm64.tar.xz"
        );
    }

    #[test]
    fn debian_rootfs_url_x86() {
        let p = get_official_provider(Distro::Debian).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("13"), Arch::X86_64),
            "https://cloud.debian.org/images/cloud/trixie/latest/debian-13-nocloud-amd64.tar.xz"
        );
    }

    #[test]
    fn debian_checksum_url() {
        let p = get_official_provider(Distro::Debian).unwrap();
        assert_eq!(
            p.checksum_url(&Version::new("12"), Arch::Aarch64).unwrap(),
            "https://cloud.debian.org/images/cloud/bookworm/latest/SHA512SUMS"
        );
    }

    #[test]
    fn debian_parse_checksum() {
        let p = get_official_provider(Distro::Debian).unwrap();
        let content = "\
aaa111  debian-12-nocloud-amd64.tar.xz
bbb222  debian-12-nocloud-arm64.tar.xz
ccc333  debian-12-genericcloud-amd64.qcow2
";
        assert_eq!(
            p.parse_checksum(content, "debian-12-nocloud-arm64.tar.xz").unwrap(),
            "bbb222"
        );
    }

    #[test]
    fn debian_parse_checksum_not_found() {
        let p = get_official_provider(Distro::Debian).unwrap();
        let content = "aaa111  debian-12-nocloud-amd64.tar.xz\n";
        assert!(p.parse_checksum(content, "debian-12-nocloud-arm64.tar.xz").is_err());
    }

    // -- Fedora ---------------------------------------------------------------

    #[test]
    fn fedora_rootfs_url_aarch64() {
        let p = get_official_provider(Distro::Fedora).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("41"), Arch::Aarch64),
            "https://download.fedoraproject.org/pub/fedora/linux/releases/41/Cloud/aarch64/images/Fedora-Cloud-Base-41-1.2.aarch64.raw.xz"
        );
    }

    #[test]
    fn fedora_rootfs_url_x86() {
        let p = get_official_provider(Distro::Fedora).unwrap();
        assert_eq!(
            p.rootfs_url(&Version::new("40"), Arch::X86_64),
            "https://download.fedoraproject.org/pub/fedora/linux/releases/40/Cloud/x86_64/images/Fedora-Cloud-Base-40-1.2.x86_64.raw.xz"
        );
    }

    #[test]
    fn fedora_checksum_url() {
        let p = get_official_provider(Distro::Fedora).unwrap();
        assert_eq!(
            p.checksum_url(&Version::new("41"), Arch::X86_64).unwrap(),
            "https://download.fedoraproject.org/pub/fedora/linux/releases/41/Cloud/x86_64/images/Fedora-Cloud-41-1.2-x86_64-CHECKSUM"
        );
    }

    #[test]
    fn fedora_parse_checksum_bsd_style() {
        let p = get_official_provider(Distro::Fedora).unwrap();
        let content = "\
# Fedora-Cloud-41-1.2-x86_64-CHECKSUM
SHA256 (Fedora-Cloud-Base-41-1.2.x86_64.raw.xz) = abc123def456
SHA256 (Fedora-Cloud-Base-41-1.2.x86_64.qcow2) = 789ghi000jkl
";
        assert_eq!(
            p.parse_checksum(content, "Fedora-Cloud-Base-41-1.2.x86_64.raw.xz").unwrap(),
            "abc123def456"
        );
    }

    #[test]
    fn fedora_parse_checksum_not_found() {
        let p = get_official_provider(Distro::Fedora).unwrap();
        let content = "SHA256 (other-file.raw.xz) = abc123\n";
        assert!(p.parse_checksum(content, "Fedora-Cloud-Base-41-1.2.x86_64.raw.xz").is_err());
    }

    // -- Substring matching regression tests ----------------------------------

    #[test]
    fn gnu_coreutils_no_substring_match() {
        let p = get_official_provider(Distro::Ubuntu).unwrap();
        // "root.tar.xz" is a substring of "arm64-root.tar.xz".
        // Only the exact filename should match.
        let content = "\
aaa111 *noble-server-cloudimg-arm64-root.tar.xz
bbb222 *noble-server-cloudimg-amd64-root.tar.xz
";
        assert!(p.parse_checksum(content, "root.tar.xz").is_err());
    }

    #[test]
    fn bsd_no_substring_match() {
        let p = get_official_provider(Distro::Fedora).unwrap();
        let content = "SHA256 (Fedora-Cloud-Base-41-1.2.x86_64.raw.xz) = abc123\n";
        // "raw.xz" is a substring, should not match.
        assert!(p.parse_checksum(content, "raw.xz").is_err());
    }

    #[test]
    fn hash_algorithm_debian_sha512() {
        let p = get_official_provider(Distro::Debian).unwrap();
        assert_eq!(p.hash_algorithm(), HashAlgorithm::Sha512);
    }

    #[test]
    fn hash_algorithm_ubuntu_sha256() {
        let p = get_official_provider(Distro::Ubuntu).unwrap();
        assert_eq!(p.hash_algorithm(), HashAlgorithm::Sha256);
    }
}
