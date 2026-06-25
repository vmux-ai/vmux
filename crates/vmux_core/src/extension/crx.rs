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

pub fn crx_public_key(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 12 || &bytes[0..4] != b"Cr24" {
        return None;
    }
    if u32::from_le_bytes(bytes[4..8].try_into().ok()?) != 3 {
        return None;
    }
    let header_len = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    let end = 12 + header_len;
    if end > bytes.len() {
        return None;
    }
    header_public_key(&bytes[12..end])
}

fn header_public_key(header: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    while i < header.len() {
        let (tag, adv) = read_varint(header, i)?;
        i += adv;
        match tag & 7 {
            0 => i += read_varint(header, i)?.1,
            1 => i += 8,
            5 => i += 4,
            2 => {
                let (len, n) = read_varint(header, i)?;
                i += n;
                let stop = i + len as usize;
                if stop > header.len() {
                    return None;
                }
                if tag >> 3 == 2
                    && let Some(pk) = proof_public_key(&header[i..stop])
                {
                    return Some(pk);
                }
                i = stop;
            }
            _ => return None,
        }
    }
    None
}

fn proof_public_key(msg: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    while i < msg.len() {
        let (tag, adv) = read_varint(msg, i)?;
        i += adv;
        match tag & 7 {
            0 => i += read_varint(msg, i)?.1,
            1 => i += 8,
            5 => i += 4,
            2 => {
                let (len, n) = read_varint(msg, i)?;
                i += n;
                let stop = i + len as usize;
                if stop > msg.len() {
                    return None;
                }
                if tag >> 3 == 1 {
                    return Some(msg[i..stop].to_vec());
                }
                i = stop;
            }
            _ => return None,
        }
    }
    None
}

fn read_varint(b: &[u8], start: usize) -> Option<(u64, usize)> {
    let mut val = 0u64;
    let mut shift = 0u32;
    let mut i = start;
    loop {
        let byte = *b.get(i)?;
        i += 1;
        val |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((val, i - start));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
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

    #[test]
    fn extracts_public_key_from_crx3_header() {
        // CrxFileHeader { sha256_with_rsa[0] { public_key: "PUBKEY" } }
        let header = [0x12u8, 0x08, 0x0a, 0x06, b'P', b'U', b'B', b'K', b'E', b'Y'];
        let mut crx = Vec::new();
        crx.extend_from_slice(b"Cr24");
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&(header.len() as u32).to_le_bytes());
        crx.extend_from_slice(&header);
        assert_eq!(crx_public_key(&crx).unwrap(), b"PUBKEY");
    }
}
