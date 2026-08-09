use sha2::{Digest, Sha256};
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
            let off = 12usize
                .checked_add(header_len)
                .filter(|off| *off <= bytes.len())
                .ok_or("crx3 header length out of range")?;
            Ok(off)
        }
        2 => {
            let pubkey_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let sig_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let off = 16usize
                .checked_add(pubkey_len)
                .and_then(|x| x.checked_add(sig_len))
                .filter(|off| *off <= bytes.len())
                .ok_or("crx2 header length out of range")?;
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

// CWS CRXs carry several signing proofs (Google's publisher key, the
// developer key, an ecdsa proof); the extension id derives from the developer
// RSA key, so pick the proof whose id matches the expected Web Store id.
pub fn crx_public_key_for(bytes: &[u8], expected_id: &str) -> Option<Vec<u8>> {
    crx_public_keys(bytes)
        .into_iter()
        .find(|pk| extension_id_from_key(pk) == expected_id)
}

pub fn crx_public_keys(bytes: &[u8]) -> Vec<Vec<u8>> {
    if bytes.len() < 12 || &bytes[0..4] != b"Cr24" {
        return Vec::new();
    }
    if u32::from_le_bytes(bytes[4..8].try_into().unwrap_or_default()) != 3 {
        return Vec::new();
    }
    let header_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap_or_default()) as usize;
    let Some(end) = 12usize.checked_add(header_len) else {
        return Vec::new();
    };
    if end > bytes.len() {
        return Vec::new();
    }
    header_public_keys(&bytes[12..end])
}

pub fn extension_id_from_key(pubkey_der: &[u8]) -> String {
    let digest = Sha256::digest(pubkey_der);
    let mut id = String::with_capacity(32);
    for byte in &digest[..16] {
        id.push((b'a' + (byte >> 4)) as char);
        id.push((b'a' + (byte & 0x0f)) as char);
    }
    id
}

fn header_public_keys(header: &[u8]) -> Vec<Vec<u8>> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < header.len() {
        let Some((tag, adv)) = read_varint(header, i) else {
            break;
        };
        i += adv;
        match tag & 7 {
            0 => {
                let Some((_, n)) = read_varint(header, i) else {
                    break;
                };
                i += n;
            }
            1 => i += 8,
            5 => i += 4,
            2 => {
                let Some((len, n)) = read_varint(header, i) else {
                    break;
                };
                i += n;
                let Some(stop) = i.checked_add(len as usize) else {
                    break;
                };
                if stop > header.len() {
                    break;
                }
                if tag >> 3 == 2
                    && let Some(pk) = proof_public_key(&header[i..stop])
                {
                    keys.push(pk);
                }
                i = stop;
            }
            _ => break,
        }
    }
    keys
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
                let stop = i.checked_add(len as usize)?;
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
#[path = "crx.test.rs"]
mod tests;
