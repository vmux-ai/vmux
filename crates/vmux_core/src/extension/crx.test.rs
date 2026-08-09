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
fn extracts_public_keys_and_matches_id() {
    // CrxFileHeader { sha256_with_rsa[0] { public_key: "PUBKEY" } }
    let header = [0x12u8, 0x08, 0x0a, 0x06, b'P', b'U', b'B', b'K', b'E', b'Y'];
    let mut crx = Vec::new();
    crx.extend_from_slice(b"Cr24");
    crx.extend_from_slice(&3u32.to_le_bytes());
    crx.extend_from_slice(&(header.len() as u32).to_le_bytes());
    crx.extend_from_slice(&header);
    assert_eq!(crx_public_keys(&crx), vec![b"PUBKEY".to_vec()]);
    let id = extension_id_from_key(b"PUBKEY");
    assert_eq!(id.len(), 32);
    assert!(id.bytes().all(|b| (b'a'..=b'p').contains(&b)));
    assert_eq!(crx_public_key_for(&crx, &id).unwrap(), b"PUBKEY");
    assert!(crx_public_key_for(&crx, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
}
