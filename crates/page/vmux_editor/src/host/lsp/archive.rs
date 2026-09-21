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

pub fn kind_for(file: &str) -> ArchiveKind {
    let f = file.to_ascii_lowercase();
    if f.ends_with(".tar.gz") || f.ends_with(".tgz") {
        ArchiveKind::TarGz
    } else if f.ends_with(".gz") {
        ArchiveKind::Gz
    } else if f.ends_with(".zip") {
        ArchiveKind::Zip
    } else {
        ArchiveKind::Raw
    }
}

fn relative_path(path: &Path) -> Result<PathBuf, String> {
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
    Ok(relative)
}

fn add_expanded(total: &mut u64, size: u64) -> Result<(), String> {
    *total = total
        .checked_add(size)
        .ok_or_else(|| "archive expanded size overflow".to_string())?;
    if *total > MAX_EXPANDED_BYTES {
        return Err(format!("archive expands beyond {MAX_EXPANDED_BYTES} bytes"));
    }
    Ok(())
}

fn copy_bounded(mut reader: impl Read, mut writer: impl Write) -> Result<(), String> {
    let mut limited = reader.by_ref().take(MAX_EXPANDED_BYTES + 1);
    let written = std::io::copy(&mut limited, &mut writer).map_err(|error| error.to_string())?;
    if written > MAX_EXPANDED_BYTES {
        return Err(format!("archive expands beyond {MAX_EXPANDED_BYTES} bytes"));
    }
    writer.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn extract_tar_gz(file: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(file).map_err(|error| error.to_string())?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries().map_err(|error| error.to_string())?;
    let mut entry_count = 0usize;
    let mut expanded = 0u64;
    for entry in entries {
        entry_count += 1;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            return Err(format!(
                "archive contains more than {MAX_ARCHIVE_ENTRIES} entries"
            ));
        }
        let mut entry = entry.map_err(|error| error.to_string())?;
        let relative = relative_path(&entry.path().map_err(|error| error.to_string())?)?;
        let output = dest_dir.join(relative);
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
        add_expanded(
            &mut expanded,
            entry.header().size().map_err(|error| error.to_string())?,
        )?;
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        entry.unpack(&output).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn extract_zip(file: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(file).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "archive contains more than {MAX_ARCHIVE_ENTRIES} entries"
        ));
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("unsafe archive path: {}", entry.name()))?;
        let relative = relative_path(&enclosed)?;
        let output = dest_dir.join(relative);
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
        add_expanded(&mut expanded, entry.size())?;
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output_file = std::fs::File::create(&output).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output_file).map_err(|error| error.to_string())?;
        output_file.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn extract(
    file: &Path,
    kind: ArchiveKind,
    dest_dir: &Path,
    single_name: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|error| error.to_string())?;
    match kind {
        ArchiveKind::Gz => {
            let relative = relative_path(Path::new(single_name))?;
            let file = std::fs::File::open(file).map_err(|error| error.to_string())?;
            let decoder = flate2::read::GzDecoder::new(file);
            let output = dest_dir.join(relative);
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let output = std::fs::File::create(output).map_err(|error| error.to_string())?;
            copy_bounded(decoder, output)
        }
        ArchiveKind::TarGz => extract_tar_gz(file, dest_dir),
        ArchiveKind::Zip => extract_zip(file, dest_dir),
        ArchiveKind::Raw => {
            let relative = relative_path(Path::new(single_name))?;
            let input = std::fs::File::open(file).map_err(|error| error.to_string())?;
            let output_path = dest_dir.join(relative);
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let output = std::fs::File::create(output_path).map_err(|error| error.to_string())?;
            copy_bounded(input, output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_detection() {
        assert_eq!(kind_for("x.tar.gz"), ArchiveKind::TarGz);
        assert_eq!(kind_for("x.tgz"), ArchiveKind::TarGz);
        assert_eq!(
            kind_for("rust-analyzer-aarch64-apple-darwin.gz"),
            ArchiveKind::Gz
        );
        assert_eq!(kind_for("x.zip"), ArchiveKind::Zip);
        assert_eq!(kind_for("plain-binary"), ArchiveKind::Raw);
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
        extract(&gz, ArchiveKind::Gz, &dest, "server").unwrap();
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
        extract(&zip_path, ArchiveKind::Zip, &dest, "_").unwrap();
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
        assert!(extract(&zip_path, ArchiveKind::Zip, &dest, "_").is_err());
        assert!(!tmp.path().join("escape").exists());
    }

    #[test]
    fn rejects_unsafe_single_file_name() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("payload");
        std::fs::write(&file, b"payload").unwrap();
        let dest = tmp.path().join("out");
        assert!(extract(&file, ArchiveKind::Raw, &dest, "../escape").is_err());
        assert!(!tmp.path().join("escape").exists());
    }
}
