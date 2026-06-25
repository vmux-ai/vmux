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
    let unpack_dir = staging.join("unpacked");
    crx::unpack_crx(&bytes, &unpack_dir)?;

    let manifest_json =
        std::fs::read_to_string(unpack_dir.join("manifest.json")).map_err(|e| e.to_string())?;
    let m = manifest::parse(&manifest_json)?;
    let name = manifest::resolve_name(&unpack_dir, &m);
    let icon = m
        .icon
        .as_ref()
        .and_then(|rel| icon_data_url(&unpack_dir, rel));

    if let Some(pk) = crx::crx_public_key(&bytes) {
        let key_b64 = base64::engine::general_purpose::STANDARD.encode(&pk);
        let _ = manifest::prepare_unpacked(&unpack_dir, &key_b64, m.popup.as_deref());
    }

    let final_dir = root.join(&id);
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&unpack_dir, &final_dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging);

    let entry = store::ExtEntry {
        id: id.clone(),
        name: if name.trim().is_empty() {
            id.clone()
        } else {
            name
        },
        version: m.version,
        popup: m.popup,
        icon,
        enabled: true,
    };
    let upsert_entry = entry.clone();
    store::update_index(&root, move |idx| idx.upsert(upsert_entry))?;
    progress(ExtInstallPhase::Done, Some(100), "done");
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
