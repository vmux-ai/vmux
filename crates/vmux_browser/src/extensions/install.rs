use std::path::Path;

use base64::Engine;
use vmux_core::event::extension::ExtInstallPhase;
use vmux_core::extension::{crx, manifest, store, webstore};

pub const DEFAULT_PRODVERSION: &str = "120.0.0.0";

pub fn install(
    source: &str,
    prodversion: &str,
    mut progress: impl FnMut(ExtInstallPhase, Option<u8>, &str),
) -> Result<store::ExtEntry, String> {
    progress(ExtInstallPhase::Resolving, None, "resolving");
    let id = webstore::extension_id(source).ok_or("not a Chrome Web Store URL or extension id")?;
    let root = store::root();
    let staging = root.join("staging").join(&id);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let crx_path = staging.join("download.crx");
    progress(ExtInstallPhase::Downloading, None, "downloading");
    super::download::fetch(&webstore::crx_url(&id, prodversion), &crx_path, |_, _| {})?;

    progress(ExtInstallPhase::Unpacking, None, "unpacking");
    let bytes = std::fs::read(&crx_path).map_err(|e| e.to_string())?;
    let entry = install_crx(&root, &id, &bytes)?;
    progress(ExtInstallPhase::Done, Some(100), "done");
    Ok(entry)
}

fn install_crx(root: &Path, id: &str, bytes: &[u8]) -> Result<store::ExtEntry, String> {
    let public_key = crx::crx_public_key_for(bytes, id)
        .ok_or_else(|| format!("CRX does not contain the developer key for extension {id}"))?;
    let public_key_b64 = Some(base64::engine::general_purpose::STANDARD.encode(public_key));
    let staging = root.join("staging").join(id);
    std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let unpack_dir = staging.join("unpacked");
    let _ = std::fs::remove_dir_all(&unpack_dir);
    crx::unpack_crx(bytes, &unpack_dir)?;

    let manifest_json =
        std::fs::read_to_string(unpack_dir.join("manifest.json")).map_err(|e| e.to_string())?;
    let m = manifest::parse(&manifest_json)?;
    let name = manifest::resolve_name(&unpack_dir, &m);
    let icon = m
        .icon
        .as_ref()
        .and_then(|rel| icon_data_url(&unpack_dir, rel));

    let final_dir = store::source_dir(root, id, &m.version);
    let final_parent = final_dir.parent().ok_or("source directory has no parent")?;
    std::fs::create_dir_all(final_parent).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&unpack_dir, &final_dir).map_err(|e| e.to_string())?;
    let source_hash = store::tree_sha256(&final_dir)?;
    let _ = std::fs::remove_dir_all(&staging);

    let entry = store::ExtEntry {
        id: id.to_string(),
        name: if name.trim().is_empty() {
            id.to_string()
        } else {
            name
        },
        version: m.version,
        popup: m.popup,
        icon,
        enabled: true,
        source_hash,
        public_key_b64,
    };
    let upsert_entry = entry.clone();
    store::update_index(root, move |idx| idx.upsert(upsert_entry))?;
    Ok(entry)
}

fn icon_data_url(dir: &Path, rel: &str) -> Option<String> {
    let bytes = std::fs::read(dir.join(rel)).ok()?;
    let mime = if rel.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "image/png"
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{mime};base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture_crx() -> (String, Vec<u8>) {
        let public_key = b"PUBKEY";
        let id = crx::extension_id_from_key(public_key);
        let mut zip_bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(
                br#"{
                    "manifest_version": 3,
                    "name": "Fixture",
                    "version": "1.0",
                    "background": { "service_worker": "background.js" }
                }"#,
            )
            .unwrap();
            zip.start_file("background.js", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"chrome.runtime.onInstalled.addListener(() => {});")
                .unwrap();
            zip.finish().unwrap();
        }
        let header = [0x12u8, 0x08, 0x0a, 0x06, b'P', b'U', b'B', b'K', b'E', b'Y'];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"Cr24");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&zip_bytes);
        (id, bytes)
    }

    #[test]
    fn installs_source_under_immutable_package_path() {
        let root = tempfile::tempdir().unwrap();
        let (id, bytes) = fixture_crx();

        let entry = install_crx(root.path(), &id, &bytes).unwrap();

        let source = store::source_dir(root.path(), &id, &entry.version);
        assert!(source.join("manifest.json").exists());
        assert!(!root.path().join(&id).exists());
        assert_eq!(entry.source_hash, store::tree_sha256(&source).unwrap());
        assert_eq!(
            entry.public_key_b64,
            Some(base64::engine::general_purpose::STANDARD.encode(b"PUBKEY"))
        );
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(source.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["background"]["service_worker"], "background.js");
    }
}
