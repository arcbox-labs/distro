use std::path::Path;

use crate::Error;

/// Supported archive formats for rootfs tarballs.
#[derive(Debug, Clone, Copy)]
pub enum ExtractFormat {
    /// Gzip-compressed tar archive (`.tar.gz` / `.tgz`).
    TarGz,
    /// XZ-compressed tar archive (`.tar.xz` / `.txz`).
    TarXz,
}

impl ExtractFormat {
    /// Detects the format from the file extension.
    pub fn detect(path: &Path) -> Result<Self, Error> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Ok(Self::TarGz)
        } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            Ok(Self::TarXz)
        } else {
            Err(Error::UnsupportedFormat(name.to_owned()))
        }
    }
}

/// Extracts an archive to the target directory.
pub fn extract_archive(archive: &Path, target: &Path, format: ExtractFormat) -> Result<(), Error> {
    std::fs::create_dir_all(target)?;

    let file = std::fs::File::open(archive)?;

    match format {
        ExtractFormat::TarGz => {
            let decoder = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(target)?;
        }
        ExtractFormat::TarXz => {
            let decoder = xz2::read::XzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(target)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn detect_tar_gz() {
        let p = Path::new("/tmp/rootfs.tar.gz");
        assert!(matches!(ExtractFormat::detect(p).unwrap(), ExtractFormat::TarGz));
    }

    #[test]
    fn detect_tgz() {
        let p = Path::new("/tmp/rootfs.tgz");
        assert!(matches!(ExtractFormat::detect(p).unwrap(), ExtractFormat::TarGz));
    }

    #[test]
    fn detect_tar_xz() {
        let p = Path::new("/tmp/rootfs.tar.xz");
        assert!(matches!(ExtractFormat::detect(p).unwrap(), ExtractFormat::TarXz));
    }

    #[test]
    fn detect_txz() {
        let p = Path::new("/tmp/rootfs.txz");
        assert!(matches!(ExtractFormat::detect(p).unwrap(), ExtractFormat::TarXz));
    }

    #[test]
    fn detect_unsupported() {
        let p = Path::new("/tmp/rootfs.zip");
        assert!(ExtractFormat::detect(p).is_err());
    }

    #[test]
    fn detect_no_extension() {
        let p = Path::new("/tmp/rootfs");
        assert!(ExtractFormat::detect(p).is_err());
    }

    fn write_tar_entry<W: Write>(
        builder: &mut tar::Builder<W>,
        path: &str,
        data: &[u8],
    ) -> std::io::Result<()> {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, data)
    }

    fn create_tar_gz(path: &Path, entry_path: &str, data: &[u8]) -> std::io::Result<()> {
        let file = File::create(path)?;
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        write_tar_entry(&mut builder, entry_path, data)?;
        let encoder = builder.into_inner()?;
        let _ = encoder.finish()?;
        Ok(())
    }

    fn create_tar_xz(path: &Path, entry_path: &str, data: &[u8]) -> std::io::Result<()> {
        let file = File::create(path)?;
        let encoder = xz2::write::XzEncoder::new(file, 6);
        let mut builder = tar::Builder::new(encoder);
        write_tar_entry(&mut builder, entry_path, data)?;
        let mut encoder = builder.into_inner()?;
        encoder.try_finish()?;
        Ok(())
    }

    #[test]
    fn extract_archive_tar_gz_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("rootfs.tar.gz");
        let target = dir.path().join("out-gz");

        create_tar_gz(&archive, "etc/os-release", b"ID=alpine\n").unwrap();
        extract_archive(&archive, &target, ExtractFormat::TarGz).unwrap();

        let extracted = target.join("etc/os-release");
        assert!(extracted.exists());
        assert_eq!(std::fs::read_to_string(extracted).unwrap(), "ID=alpine\n");
    }

    #[test]
    fn extract_archive_tar_xz_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("rootfs.tar.xz");
        let target = dir.path().join("out-xz");

        create_tar_xz(&archive, "usr/lib/os-release", b"ID=debian\n").unwrap();
        extract_archive(&archive, &target, ExtractFormat::TarXz).unwrap();

        let extracted = target.join("usr/lib/os-release");
        assert!(extracted.exists());
        assert_eq!(std::fs::read_to_string(extracted).unwrap(), "ID=debian\n");
    }

    #[test]
    fn extract_archive_invalid_tar_gz_errors() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("broken.tar.gz");
        let target = dir.path().join("out-broken");
        std::fs::write(&archive, b"not-a-valid-gzip-tar").unwrap();

        let err = extract_archive(&archive, &target, ExtractFormat::TarGz).unwrap_err();
        match err {
            Error::Io(_) => {}
            _ => panic!("unexpected error variant"),
        }
    }

    #[test]
    fn extract_archive_format_mismatch_errors() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("rootfs.tar.xz");
        let target = dir.path().join("out-mismatch");

        create_tar_xz(&archive, "etc/issue", b"Welcome\n").unwrap();
        assert!(extract_archive(&archive, &target, ExtractFormat::TarGz).is_err());
    }
}
