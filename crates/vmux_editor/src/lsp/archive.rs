use std::path::Path;

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

/// Extract `file` into `dest_dir`. For single-file kinds (`Gz`, `Raw`) the output
/// is written as `dest_dir/single_name`.
pub fn extract(
    file: &Path,
    kind: ArchiveKind,
    dest_dir: &Path,
    single_name: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    match kind {
        ArchiveKind::Gz => {
            let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
            let mut dec = flate2::read::GzDecoder::new(f);
            let mut out =
                std::fs::File::create(dest_dir.join(single_name)).map_err(|e| e.to_string())?;
            std::io::copy(&mut dec, &mut out).map_err(|e| e.to_string())?;
        }
        ArchiveKind::TarGz => {
            let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
            let dec = flate2::read::GzDecoder::new(f);
            tar::Archive::new(dec)
                .unpack(dest_dir)
                .map_err(|e| e.to_string())?;
        }
        ArchiveKind::Zip => {
            let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
            zip.extract(dest_dir).map_err(|e| e.to_string())?;
        }
        ArchiveKind::Raw => {
            std::fs::copy(file, dest_dir.join(single_name)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn kind_detection() {
        assert_eq!(kind_for("x.tar.gz"), ArchiveKind::TarGz);
        assert_eq!(kind_for("x.tgz"), ArchiveKind::TarGz);
        assert_eq!(kind_for("rust-analyzer-aarch64-apple-darwin.gz"), ArchiveKind::Gz);
        assert_eq!(kind_for("x.zip"), ArchiveKind::Zip);
        assert_eq!(kind_for("plain-binary"), ArchiveKind::Raw);
    }

    #[test]
    fn extracts_gz_single_file() {
        let tmp = tempfile::tempdir().unwrap();
        let gz = tmp.path().join("payload.gz");
        {
            let f = std::fs::File::create(&gz).unwrap();
            let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            enc.write_all(b"binary-contents").unwrap();
            enc.finish().unwrap();
        }
        let dest = tmp.path().join("out");
        extract(&gz, ArchiveKind::Gz, &dest, "server").unwrap();
        assert_eq!(std::fs::read(dest.join("server")).unwrap(), b"binary-contents");
    }

    #[test]
    fn extracts_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("a.zip");
        {
            let f = std::fs::File::create(&zp).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("inner.txt", opts).unwrap();
            w.write_all(b"zipped").unwrap();
            w.finish().unwrap();
        }
        let dest = tmp.path().join("out");
        extract(&zp, ArchiveKind::Zip, &dest, "_").unwrap();
        assert_eq!(std::fs::read(dest.join("inner.txt")).unwrap(), b"zipped");
    }
}
