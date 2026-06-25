use std::io::Read;
use std::path::Path;

pub fn zip_offset(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < 16 || &bytes[0..4] != b"Cr24" {
        return Err("not a crx (bad magic)".into());
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    match version {
        3 => {
            let header_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let off = 12 + header_len;
            if off > bytes.len() {
                return Err("crx3 header length out of range".into());
            }
            Ok(off)
        }
        2 => {
            let pubkey_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let sig_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let off = 16 + pubkey_len + sig_len;
            if off > bytes.len() {
                return Err("crx2 header length out of range".into());
            }
            Ok(off)
        }
        v => Err(format!("unsupported crx version {v}")),
    }
}

pub fn unpack_crx(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let off = zip_offset(bytes)?;
    let cursor = std::io::Cursor::new(&bytes[off..]);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(name);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(&out_path, buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(br#"{"name":"x","version":"1.0"}"#).unwrap();
            zip.start_file("sub/popup.html", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"<html></html>").unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    fn make_crx3(zip: &[u8]) -> Vec<u8> {
        let header = b"fakeheaderbytes";
        let mut out = Vec::new();
        out.extend_from_slice(b"Cr24");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(header);
        out.extend_from_slice(zip);
        out
    }

    #[test]
    fn unpacks_crx3_to_dir() {
        let dir = tempfile::tempdir().unwrap();
        let crx = make_crx3(&make_zip());
        unpack_crx(&crx, dir.path()).unwrap();
        let manifest = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        assert!(manifest.contains("\"version\":\"1.0\""));
        assert!(dir.path().join("sub/popup.html").exists());
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(unpack_crx(b"NOPExxxxxxxxxxxx", dir.path()).is_err());
    }

    #[test]
    fn computes_crx3_offset() {
        let crx = make_crx3(&make_zip());
        assert_eq!(zip_offset(&crx).unwrap(), 12 + "fakeheaderbytes".len());
    }
}
