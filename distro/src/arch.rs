use serde::{Deserialize, Serialize};
use std::fmt;

/// Target CPU architecture for distro images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    /// ARM 64-bit (Apple Silicon, AWS Graviton, etc.).
    Aarch64,
    /// x86 64-bit (Intel / AMD).
    X86_64,
}

impl Arch {
    /// Detects the current host architecture.
    pub fn current() -> Self {
        #[cfg(target_arch = "aarch64")]
        return Self::Aarch64;
        #[cfg(target_arch = "x86_64")]
        return Self::X86_64;
    }

    /// Returns the architecture name used by Linux kernel and most distros.
    pub fn linux_name(&self) -> &'static str {
        match self {
            Self::Aarch64 => "aarch64",
            Self::X86_64 => "x86_64",
        }
    }

    /// Returns the Debian/Ubuntu-style architecture name.
    pub fn deb_name(&self) -> &'static str {
        match self {
            Self::Aarch64 => "arm64",
            Self::X86_64 => "amd64",
        }
    }

    /// Returns the architecture name used by LXC Images (same as Debian style).
    pub fn lxc_name(&self) -> &'static str {
        self.deb_name()
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.linux_name())
    }
}
