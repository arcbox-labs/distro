# Usage

## `distro` crate — Download and Verification

### Download via LXC Images (recommended)

All 16 distributions are available through a single unified source with embedded SHA256 verification:

```rust
use distro::{Distro, Version, Arch, Mirror, download_from_lxc};

let result = download_from_lxc(
    Distro::Alpine,
    &Version::new("3.21"),
    Arch::current(),
    &Mirror::default(),
    |downloaded, total| eprintln!("{downloaded}/{total} bytes"),
).await?;

println!("SHA256: {}", result.sha256);
println!("Size: {} bytes", result.data.len());
```

### Download from official sources

For distros with official DistroSpec (Alpine, Ubuntu, Debian, Fedora), you can download directly from official mirrors with checksum verification:

```rust
use distro::{Distro, Version, Arch, download_with_verification};

let result = download_with_verification(
    Distro::Alpine,
    &Version::new("3.21"),
    Arch::current(),
    |downloaded, total| eprintln!("{downloaded}/{total} bytes"),
).await?;
```

### Mirror selection

Choose a mirror based on geographic proximity:

```rust
use distro::Mirror;

// Built-in presets
let mirror = Mirror::Official; // images.linuxcontainers.org (default)
let mirror = Mirror::Tuna;     // mirrors.tuna.tsinghua.edu.cn
let mirror = Mirror::Ustc;     // mirrors.ustc.edu.cn
let mirror = Mirror::Bfsu;     // mirrors.bfsu.edu.cn

// Self-hosted (e.g. Cloudflare R2 CDN)
let mirror = Mirror::Custom("https://images.arcbox.dev".into());

// List all presets
for m in Mirror::presets() {
    println!("{}: {}", m, m.base_url());
}
```

### Parse distro spec strings

Parse user input like `"alpine:3.20"` or `"ubuntu"`:

```rust
use distro::parse_distro_spec;

let (distro, version) = parse_distro_spec("alpine:3.20")?;
let (distro, version) = parse_distro_spec("ubuntu")?;       // default → 24.04
let (distro, version) = parse_distro_spec("rockylinux:9")?;  // alias supported
```

Supported aliases:

| Input | Resolves to |
|-------|-------------|
| `alma`, `almalinux` | `Distro::Alma` |
| `arch`, `archlinux` | `Distro::Arch` |
| `rocky`, `rockylinux` | `Distro::Rocky` |
| `void`, `voidlinux` | `Distro::Void` |

### LXC Simplestreams client (advanced)

Use `LxcClient` directly for fine-grained control:

```rust
use distro::{Distro, Version, Arch, Mirror};
use distro::lxc::LxcClient;

let client = LxcClient::new(Mirror::default());

// Fetch index once, resolve multiple images
let index = client.fetch_index().await?;

let alpine = client.resolve_from_index(
    &index, Distro::Alpine, &Version::new("3.21"), Arch::Aarch64,
)?;
let ubuntu = client.resolve_from_index(
    &index, Distro::Ubuntu, &Version::new("24.04"), Arch::Aarch64,
)?;

println!("Alpine: {} ({})", alpine.url, alpine.sha256);
println!("Ubuntu: {} ({})", ubuntu.url, ubuntu.sha256);
```

## `distro-rootfs` crate — Caching and Extraction

### RootfsManager

The primary entry point for cached downloads and extraction:

```rust
use distro::{Distro, Arch, Mirror};
use distro_rootfs::RootfsManager;

let manager = RootfsManager::new("~/.local/share/arcbox/rootfs")?;

// Downloads once, then serves from cache on subsequent calls.
// Cache integrity is verified via streaming SHA256 on each load.
let rootfs = manager.ensure(
    Distro::Ubuntu,
    &"24.04".into(),
    Arch::current(),
    &Mirror::default(),
    |downloaded, total| eprintln!("{downloaded}/{total}"),
).await?;

// Extract to a target directory (supports .tar.gz and .tar.xz).
rootfs.extract_to("/tmp/ubuntu-rootfs")?;
```

### Cache management

```rust
let manager = RootfsManager::new("/path/to/cache")?;

// List all cached entries (metadata only, no I/O on archive files).
let entries = manager.list_cached()?;
for entry in &entries {
    println!("{} {} {} ({} bytes)",
        entry.metadata.distro,
        entry.metadata.version,
        entry.metadata.arch,
        entry.metadata.size,
    );
}

// Keep only the 2 most recent entries per distro, returns bytes freed.
let freed = manager.prune(2)?;
println!("Freed {} bytes", freed);
```

### Verify cached archive integrity

```rust
let rootfs = manager.ensure(
    Distro::Alpine, &"3.21".into(), Arch::current(),
    &Mirror::default(), |_, _| {},
).await?;

// Streaming SHA256 verification (8 KiB chunks, no full file load).
assert!(rootfs.verify_integrity()?);
```

### Default cache directory

```rust
use distro_rootfs::default_cache_dir;

let dir = default_cache_dir();
// → ~/.local/share/arcbox/rootfs (respects XDG_DATA_HOME)
```
