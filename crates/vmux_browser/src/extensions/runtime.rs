use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use vmux_core::extension::protocol::BRIDGE_PROTOCOL_VERSION;
use vmux_core::extension::{manifest, store};

use super::shim;

const WORKER_SOURCE: &str = include_str!("runtime/worker.js");
const BRIDGE_HTML: &str = include_str!("runtime/bridge.html");
const BRIDGE_SOURCE: &str = include_str!("runtime/bridge.js");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRuntime {
    pub extension_id: String,
    pub dir: PathBuf,
    pub runtime_hash: String,
    pub source_hash: String,
}

pub fn prepare_runtime(
    root: &Path,
    profile: &str,
    entry: &store::ExtEntry,
) -> Result<PreparedRuntime, String> {
    let expected_source = store::source_dir(root, &entry.id, &entry.version);
    let source = if expected_source.exists() {
        expected_source
    } else {
        store::migrate_legacy_package(root, entry)?
    };
    let source_hash = store::tree_sha256(&source)?;
    if !entry.source_hash.is_empty() && source_hash != entry.source_hash {
        return Err(format!("source hash mismatch for {}", entry.id));
    }

    let runtime_hash = runtime_hash(&source_hash);
    let runtime_root = store::runtime_profile_dir(root, profile, &entry.id);
    std::fs::create_dir_all(&runtime_root).map_err(|error| error.to_string())?;
    let temp_dir = runtime_root.join(format!("{runtime_hash}.tmp"));
    let final_dir = runtime_root.join(&runtime_hash);
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).map_err(|error| error.to_string())?;
    }
    copy_tree(&source, &temp_dir)?;
    if let Some(key) = entry.public_key_b64.as_deref() {
        manifest::prepare_unpacked(&temp_dir, key, entry.popup.as_deref())?;
    }
    std::fs::write(temp_dir.join("vmux_runtime.js"), WORKER_SOURCE)
        .map_err(|error| error.to_string())?;
    std::fs::write(temp_dir.join("vmux_bridge.html"), BRIDGE_HTML)
        .map_err(|error| error.to_string())?;
    std::fs::write(temp_dir.join("vmux_bridge.js"), BRIDGE_SOURCE)
        .map_err(|error| error.to_string())?;
    let loader = shim::install_worker_loader(&temp_dir, "vmux_runtime.js")?;
    validate_runtime(&temp_dir, &loader)?;
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temp_dir, &final_dir).map_err(|error| error.to_string())?;
    remove_sibling_runtimes(&runtime_root, &final_dir)?;

    Ok(PreparedRuntime {
        extension_id: entry.id.clone(),
        dir: final_dir,
        runtime_hash,
        source_hash,
    })
}

fn runtime_hash(source_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_hash.as_bytes());
    hasher.update(BRIDGE_PROTOCOL_VERSION.to_string().as_bytes());
    hasher.update(WORKER_SOURCE.as_bytes());
    hasher.update(BRIDGE_HTML.as_bytes());
    hasher.update(BRIDGE_SOURCE.as_bytes());
    hasher.update(shim::PATCH_SOURCE.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        } else {
            return Err(format!(
                "unsupported extension source entry: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn validate_runtime(dir: &Path, loader: &str) -> Result<(), String> {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("manifest.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if !manifest.is_object() {
        return Err("runtime manifest is not an object".into());
    }
    if !dir.join(loader).is_file() {
        return Err("runtime worker loader is missing".into());
    }
    Ok(())
}

fn remove_sibling_runtimes(runtime_root: &Path, keep: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(runtime_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path == keep {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        } else {
            std::fs::remove_file(path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_entry(root: &Path, worker: &str, module: bool) -> (store::ExtEntry, PathBuf, String) {
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let version = "1.0";
        let source = store::source_dir(root, id, version);
        std::fs::create_dir_all(&source).unwrap();
        let worker_type = if module { r#", "type": "module""# } else { "" };
        let manifest = format!(
            r#"{{"manifest_version":3,"name":"test","version":"{version}","background":{{"service_worker":"{worker}"{worker_type}}}}}"#
        );
        std::fs::write(source.join("manifest.json"), &manifest).unwrap();
        if let Some(parent) = Path::new(worker).parent() {
            std::fs::create_dir_all(source.join(parent)).unwrap();
        }
        std::fs::write(source.join(worker), "original worker").unwrap();
        let source_hash = store::tree_sha256(&source).unwrap();
        (
            store::ExtEntry {
                id: id.into(),
                name: "test".into(),
                version: version.into(),
                popup: None,
                icon: None,
                enabled: true,
                source_hash,
                public_key_b64: None,
            },
            source,
            manifest,
        )
    }

    #[test]
    fn prepares_classic_worker_without_mutating_source() {
        let root = tempfile::tempdir().unwrap();
        let (entry, source, original_manifest) = source_entry(root.path(), "background.js", false);

        let prepared = prepare_runtime(root.path(), "personal", &entry).unwrap();

        assert!(prepared.dir.starts_with(store::runtime_profile_dir(
            root.path(),
            "personal",
            &entry.id
        )));
        assert_eq!(
            std::fs::read_to_string(source.join("manifest.json")).unwrap(),
            original_manifest
        );
        let generated: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap(),
        )
        .unwrap();
        let worker = generated["background"]["service_worker"].as_str().unwrap();
        let loader = std::fs::read_to_string(prepared.dir.join(worker)).unwrap();
        assert!(loader.contains("importScripts(\"vmux_runtime.js\")"));
        assert!(loader.contains("importScripts(\"vmux_patch.js\")"));
        assert!(loader.contains("importScripts(\"background.js\")"));
        assert!(prepared.dir.join("vmux_bridge.html").exists());
        assert!(prepared.dir.join("vmux_bridge.js").exists());
    }

    #[test]
    fn prepares_module_worker_with_static_imports_in_order() {
        let root = tempfile::tempdir().unwrap();
        let (entry, _, _) = source_entry(root.path(), "sw/main.js", true);

        let prepared = prepare_runtime(root.path(), "personal", &entry).unwrap();

        let generated: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap(),
        )
        .unwrap();
        let worker = generated["background"]["service_worker"].as_str().unwrap();
        let loader = std::fs::read_to_string(prepared.dir.join(worker)).unwrap();
        let runtime = loader.find("import \"./vmux_runtime.js\"").unwrap();
        let patch = loader.find("import \"./vmux_patch.js\"").unwrap();
        let original = loader.find("import \"./sw/main.js\"").unwrap();
        assert!(runtime < patch && patch < original);
        assert!(!loader.contains("import("));
    }
}
