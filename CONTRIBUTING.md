# Contributing

## Workspace Structure

```
distro/
├── distro/          # Core: metadata, download, verification
│   └── src/
│       ├── lib.rs       # Distro enum (16 variants), Version, parse_distro_spec()
│       ├── arch.rs      # Arch enum (Aarch64, X86_64)
│       ├── download.rs  # HTTP download with streaming progress + checksum
│       ├── error.rs     # Error types
│       ├── lxc.rs       # Simplestreams client (images.json → ResolvedImage)
│       ├── mirror.rs    # Mirror selection (Official, Tuna, USTC, BFSU, Custom)
│       └── provider/    # Official source templates (Alpine, Ubuntu, Debian, Fedora)
└── distro-rootfs/   # Caching, extraction, lifecycle
    └── src/
        ├── lib.rs       # RootfsManager (ensure / list / prune)
        ├── cache.rs     # Disk cache with streaming SHA256 integrity checks
        ├── extract.rs   # Archive extraction (tar.gz, tar.xz)
        └── error.rs     # Error types
```

## Requirements

- Rust 1.85+ (Edition 2024)
- Architectures: `aarch64`, `x86_64`
- Platforms: macOS, Linux

## Testing

```bash
cargo test
```

66 tests (44 in `distro`, 20 in `distro-rootfs`, 2 doc-tests). All tests are offline — network-dependent tests use mock data.
