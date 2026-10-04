use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub struct CrxArchive(Vec<u8>);

impl CrxArchive {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn zip_offset(&self) -> Result<usize, String> {
        if self.0.len() < 16 || &self.0[0..4] != b"Cr24" {
            return Err("not a crx (bad magic)".into());
        }
        let version = u32::from_le_bytes(self.0[4..8].try_into().unwrap());
        match version {
            3 => {
                let header_len = u32::from_le_bytes(self.0[8..12].try_into().unwrap()) as usize;
                12usize
                    .checked_add(header_len)
                    .filter(|offset| *offset <= self.0.len())
                    .ok_or_else(|| "crx3 header length out of range".to_string())
            }
            2 => {
                let public_key_len = u32::from_le_bytes(self.0[8..12].try_into().unwrap()) as usize;
                let signature_len = u32::from_le_bytes(self.0[12..16].try_into().unwrap()) as usize;
                16usize
                    .checked_add(public_key_len)
                    .and_then(|offset| offset.checked_add(signature_len))
                    .filter(|offset| *offset <= self.0.len())
                    .ok_or_else(|| "crx2 header length out of range".to_string())
            }
            version => Err(format!("unsupported crx version {version}")),
        }
    }

    pub fn unpack(&self, destination: &Path) -> Result<(), String> {
        let offset = self.zip_offset()?;
        let cursor = std::io::Cursor::new(&self.0[offset..]);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|error| error.to_string())?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
            let Some(name) = file.enclosed_name() else {
                continue;
            };
            let output = destination.join(name);
            if file.is_dir() {
                std::fs::create_dir_all(&output).map_err(|error| error.to_string())?;
                continue;
            }
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            std::fs::write(&output, bytes).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn public_key_for(&self, expected_id: &str) -> Option<Vec<u8>> {
        self.public_keys().into_iter().find(|public_key| {
            ChromeExtensionId::from_public_key(public_key).as_str() == expected_id
        })
    }

    pub fn public_keys(&self) -> Vec<Vec<u8>> {
        if self.0.len() < 12 || &self.0[0..4] != b"Cr24" {
            return Vec::new();
        }
        if u32::from_le_bytes(self.0[4..8].try_into().unwrap_or_default()) != 3 {
            return Vec::new();
        }
        let header_len = u32::from_le_bytes(self.0[8..12].try_into().unwrap_or_default()) as usize;
        let Some(end) = 12usize.checked_add(header_len) else {
            return Vec::new();
        };
        if end > self.0.len() {
            return Vec::new();
        }
        Self::header_public_keys(&self.0[12..end])
    }

    fn header_public_keys(header: &[u8]) -> Vec<Vec<u8>> {
        let mut keys = Vec::new();
        let mut index = 0;
        while index < header.len() {
            let Some((tag, advanced)) = Self::read_varint(header, index) else {
                break;
            };
            index += advanced;
            match tag & 7 {
                0 => {
                    let Some((_, advanced)) = Self::read_varint(header, index) else {
                        break;
                    };
                    index += advanced;
                }
                1 => index += 8,
                5 => index += 4,
                2 => {
                    let Some((length, advanced)) = Self::read_varint(header, index) else {
                        break;
                    };
                    index += advanced;
                    let Some(stop) = index.checked_add(length as usize) else {
                        break;
                    };
                    if stop > header.len() {
                        break;
                    }
                    if tag >> 3 == 2
                        && let Some(public_key) = Self::proof_public_key(&header[index..stop])
                    {
                        keys.push(public_key);
                    }
                    index = stop;
                }
                _ => break,
            }
        }
        keys
    }

    fn proof_public_key(message: &[u8]) -> Option<Vec<u8>> {
        let mut index = 0;
        while index < message.len() {
            let (tag, advanced) = Self::read_varint(message, index)?;
            index += advanced;
            match tag & 7 {
                0 => index += Self::read_varint(message, index)?.1,
                1 => index += 8,
                5 => index += 4,
                2 => {
                    let (length, advanced) = Self::read_varint(message, index)?;
                    index += advanced;
                    let stop = index.checked_add(length as usize)?;
                    if stop > message.len() {
                        return None;
                    }
                    if tag >> 3 == 1 {
                        return Some(message[index..stop].to_vec());
                    }
                    index = stop;
                }
                _ => return None,
            }
        }
        None
    }

    fn read_varint(bytes: &[u8], start: usize) -> Option<(u64, usize)> {
        let mut value = 0u64;
        let mut shift = 0u32;
        let mut index = start;
        loop {
            let byte = *bytes.get(index)?;
            index += 1;
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some((value, index - start));
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
    }
}

pub struct ChromeExtensionId(String);

impl ChromeExtensionId {
    pub fn from_public_key(public_key: &[u8]) -> Self {
        let digest = Sha256::digest(public_key);
        let mut id = String::with_capacity(32);
        for byte in &digest[..16] {
            id.push((b'a' + (byte >> 4)) as char);
            id.push((b'a' + (byte & 0x0f)) as char);
        }
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<ChromeExtensionId> for String {
    fn from(id: ChromeExtensionId) -> Self {
        id.0
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
        CrxArchive::new(crx).unpack(dir.path()).unwrap();
        let manifest = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        assert!(manifest.contains("\"version\":\"1.0\""));
        assert!(dir.path().join("sub/popup.html").exists());
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            CrxArchive::new(b"NOPExxxxxxxxxxxx".to_vec())
                .unpack(dir.path())
                .is_err()
        );
    }

    #[test]
    fn computes_crx3_offset() {
        let crx = make_crx3(&make_zip());
        assert_eq!(
            CrxArchive::new(crx).zip_offset().unwrap(),
            12 + "fakeheaderbytes".len()
        );
    }

    #[test]
    fn extracts_public_keys_and_matches_id() {
        let header = [0x12u8, 0x08, 0x0a, 0x06, b'P', b'U', b'B', b'K', b'E', b'Y'];
        let mut crx = Vec::new();
        crx.extend_from_slice(b"Cr24");
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&(header.len() as u32).to_le_bytes());
        crx.extend_from_slice(&header);
        let archive = CrxArchive::new(crx);
        assert_eq!(archive.public_keys(), vec![b"PUBKEY".to_vec()]);
        let id = ChromeExtensionId::from_public_key(b"PUBKEY");
        let id = id.as_str();
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|b| (b'a'..=b'p').contains(&b)));
        assert_eq!(archive.public_key_for(id).unwrap(), b"PUBKEY");
        assert!(
            archive
                .public_key_for("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .is_none()
        );
    }
}
