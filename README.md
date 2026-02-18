# distro

[![CI](https://github.com/arcbox-labs/distro/actions/workflows/ci.yml/badge.svg)](https://github.com/arcbox-labs/distro/actions/workflows/ci.yml)
[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)
[![Rust: 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Crates: 2](https://img.shields.io/badge/crates-2-green.svg)](#architecture)
[![Distros: 16](https://img.shields.io/badge/distros-16-purple.svg)](#supported-distributions)

Linux distribution rootfs download, verification, caching, and extraction.

A Rust workspace providing a unified API to download rootfs images for **16 Linux distributions** via the [LXC Images](https://images.linuxcontainers.org) Simplestreams protocol, with built-in mirror selection, SHA256 integrity verification, and local caching.

## Supported Distributions

| Distro | Default Version | Distro | Default Version |
|--------|----------------|--------|----------------|
| Alma | 9 | Kali | current |
| Alpine | 3.21 | NixOS | 25.05 |
| Arch | current | openEuler | 24.03 |
| CentOS | 9-Stream | openSUSE | tumbleweed |
| Debian | 12 | Oracle | 9 |
| Devuan | daedalus | Rocky | 9 |
| Fedora | 41 | Ubuntu | 24.04 |
| Gentoo | current | Void | current |

## Usage

See [USAGE.md](USAGE.md) for detailed examples.

```rust
use distro::{Distro, Version, Arch, Mirror, download_from_lxc};

let result = download_from_lxc(
    Distro::Alpine,
    &Version::new("3.21"),
    Arch::current(),
    &Mirror::default(),
    |downloaded, total| eprintln!("{downloaded}/{total} bytes"),
).await?;
```

## Architecture

### Download Sources

| Source | Coverage | Checksum | Use Case |
|--------|----------|----------|----------|
| **LXC Images** | All 16 distros | SHA256 (from Simplestreams JSON) | Default, recommended |
| **Official** | Alpine, Ubuntu, Debian, Fedora | SHA256/SHA512 (from checksum files) | When official sources are preferred |

### Simplestreams Protocol

The LXC Images source uses the Simplestreams protocol:

1. Fetch `{mirror}/streams/v1/images.json` (static JSON index)
2. Look up product key: `{lxc_name}:{release}:{arch}:default`
3. Select the latest version entry
4. Extract `rootfs.tar.xz` path + SHA256 from items
5. Download and verify

### Cache Layout

```
~/.local/share/arcbox/rootfs/
└── {distro}/
    └── {version}/
        └── {arch}/
            ├── metadata.json    # CacheMetadata (distro, version, arch, sha256, ...)
            └── rootfs.tar.xz    # Downloaded archive
```

Cache integrity is verified using streaming SHA256 (8 KiB chunks) to avoid loading entire archives into memory. Corrupted entries are automatically removed and re-downloaded.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for workspace structure, build requirements, and testing.

## License

MIT OR Apache-2.0
