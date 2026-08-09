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
#[path = "archive.test.rs"]
mod tests;
