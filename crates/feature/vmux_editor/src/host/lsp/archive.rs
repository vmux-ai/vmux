use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Gz,
    TarGz,
    Zip,
    Raw,
}

struct ArchivePath(PathBuf);

impl ArchivePath {
    fn parse(path: &Path) -> Result<Self, String> {
        let mut relative = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(component) => relative.push(component),
                Component::CurDir => {}
                _ => return Err(format!("unsafe archive path: {}", path.display())),
            }
        }
        if relative.as_os_str().is_empty() {
            return Err(format!("unsafe archive path: {}", path.display()));
        }
        Ok(Self(relative))
    }

    fn join(&self, destination: &Path) -> PathBuf {
        destination.join(&self.0)
    }
}

#[derive(Default)]
struct ExpandedSize(u64);

impl ExpandedSize {
    fn add(&mut self, size: u64) -> Result<(), String> {
        self.0 = self
            .0
            .checked_add(size)
            .ok_or_else(|| "archive expanded size overflow".to_string())?;
        if self.0 > MAX_EXPANDED_BYTES {
            return Err(format!("archive expands beyond {MAX_EXPANDED_BYTES} bytes"));
        }
        Ok(())
    }

    fn copy(mut reader: impl Read, mut writer: impl Write) -> Result<(), String> {
        let mut limited = reader.by_ref().take(MAX_EXPANDED_BYTES + 1);
        let written =
            std::io::copy(&mut limited, &mut writer).map_err(|error| error.to_string())?;
        if written > MAX_EXPANDED_BYTES {
            return Err(format!("archive expands beyond {MAX_EXPANDED_BYTES} bytes"));
        }
        writer.flush().map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl ArchiveKind {
    pub fn for_file(file: &str) -> Self {
        let file = file.to_ascii_lowercase();
        if file.ends_with(".tar.gz") || file.ends_with(".tgz") {
            Self::TarGz
        } else if file.ends_with(".gz") {
            Self::Gz
        } else if file.ends_with(".zip") {
            Self::Zip
        } else {
            Self::Raw
        }
    }

    pub fn extract(self, file: &Path, destination: &Path, single_name: &str) -> Result<(), String> {
        std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        match self {
            Self::Gz => {
                let relative = ArchivePath::parse(Path::new(single_name))?;
                let file = std::fs::File::open(file).map_err(|error| error.to_string())?;
                let decoder = flate2::read::GzDecoder::new(file);
                let output = relative.join(destination);
                if let Some(parent) = output.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                let output = std::fs::File::create(output).map_err(|error| error.to_string())?;
                ExpandedSize::copy(decoder, output)
            }
            Self::TarGz => Self::extract_tar_gz(file, destination),
            Self::Zip => Self::extract_zip(file, destination),
            Self::Raw => {
                let relative = ArchivePath::parse(Path::new(single_name))?;
                let input = std::fs::File::open(file).map_err(|error| error.to_string())?;
                let output_path = relative.join(destination);
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                let output =
                    std::fs::File::create(output_path).map_err(|error| error.to_string())?;
                ExpandedSize::copy(input, output)
            }
        }
    }

    fn extract_tar_gz(file: &Path, destination: &Path) -> Result<(), String> {
        let file = std::fs::File::open(file).map_err(|error| error.to_string())?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let entries = archive.entries().map_err(|error| error.to_string())?;
        let mut entry_count = 0usize;
        let mut expanded = ExpandedSize::default();
        for entry in entries {
            entry_count += 1;
            if entry_count > MAX_ARCHIVE_ENTRIES {
                return Err(format!(
                    "archive contains more than {MAX_ARCHIVE_ENTRIES} entries"
                ));
            }
            let mut entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path().map_err(|error| error.to_string())?;
            let output = ArchivePath::parse(&path)?.join(destination);
            let kind = entry.header().entry_type();
            if kind.is_symlink() || kind.is_hard_link() {
                continue;
            }
            if kind.is_dir() {
                std::fs::create_dir_all(&output).map_err(|error| error.to_string())?;
                continue;
            }
            if !kind.is_file() {
                return Err("unsupported tar entry type".to_string());
            }
            expanded.add(entry.header().size().map_err(|error| error.to_string())?)?;
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            entry.unpack(&output).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn extract_zip(file: &Path, destination: &Path) -> Result<(), String> {
        let file = std::fs::File::open(file).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            return Err(format!(
                "archive contains more than {MAX_ARCHIVE_ENTRIES} entries"
            ));
        }
        let mut expanded = ExpandedSize::default();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            let enclosed = entry
                .enclosed_name()
                .ok_or_else(|| format!("unsafe archive path: {}", entry.name()))?;
            let output = ArchivePath::parse(&enclosed)?.join(destination);
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                continue;
            }
            if entry.is_dir() {
                std::fs::create_dir_all(&output).map_err(|error| error.to_string())?;
                continue;
            }
            expanded.add(entry.size())?;
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output_file =
                std::fs::File::create(&output).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut output_file).map_err(|error| error.to_string())?;
            output_file.flush().map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_detection() {
        assert_eq!(ArchiveKind::for_file("x.tar.gz"), ArchiveKind::TarGz);
        assert_eq!(ArchiveKind::for_file("x.tgz"), ArchiveKind::TarGz);
        assert_eq!(
            ArchiveKind::for_file("rust-analyzer-aarch64-apple-darwin.gz"),
            ArchiveKind::Gz
        );
        assert_eq!(ArchiveKind::for_file("x.zip"), ArchiveKind::Zip);
        assert_eq!(ArchiveKind::for_file("plain-binary"), ArchiveKind::Raw);
    }

    #[test]
    fn extracts_gz_single_file() {
        let tmp = tempfile::tempdir().unwrap();
        let gz = tmp.path().join("payload.gz");
        {
            let file = std::fs::File::create(&gz).unwrap();
            let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            encoder.write_all(b"binary-contents").unwrap();
            encoder.finish().unwrap();
        }
        let dest = tmp.path().join("out");
        ArchiveKind::Gz.extract(&gz, &dest, "server").unwrap();
        assert_eq!(
            std::fs::read(dest.join("server")).unwrap(),
            b"binary-contents"
        );
    }

    #[test]
    fn extracts_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("a.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            writer.start_file("inner.txt", options).unwrap();
            writer.write_all(b"zipped").unwrap();
            writer.finish().unwrap();
        }
        let dest = tmp.path().join("out");
        ArchiveKind::Zip.extract(&zip_path, &dest, "_").unwrap();
        assert_eq!(std::fs::read(dest.join("inner.txt")).unwrap(), b"zipped");
    }

    #[test]
    fn rejects_zip_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("a.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            writer.start_file("../escape", options).unwrap();
            writer.write_all(b"escaped").unwrap();
            writer.finish().unwrap();
        }
        let dest = tmp.path().join("out");
        assert!(ArchiveKind::Zip.extract(&zip_path, &dest, "_").is_err());
        assert!(!tmp.path().join("escape").exists());
    }

    #[test]
    fn rejects_unsafe_single_file_name() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("payload");
        std::fs::write(&file, b"payload").unwrap();
        let dest = tmp.path().join("out");
        assert!(ArchiveKind::Raw.extract(&file, &dest, "../escape").is_err());
        assert!(!tmp.path().join("escape").exists());
    }
}
