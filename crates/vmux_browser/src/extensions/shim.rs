use serde_json::Value;
use std::path::Path;

pub(crate) const PATCH_SOURCE: &str = include_str!("shim.js");
const PATCH_FILE: &str = "vmux_patch.js";

fn content_hash(source: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in source.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

pub(crate) fn install_worker_loader(dir: &Path, runtime_file: &str) -> Result<String, String> {
    let manifest_path = dir.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let mut manifest: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    let background = manifest
        .get_mut("background")
        .and_then(Value::as_object_mut)
        .ok_or("manifest has no background object")?;
    let original = background
        .get("service_worker")
        .and_then(Value::as_str)
        .ok_or("manifest has no background service worker")?
        .to_string();
    if original.starts_with("vmux_sw_") || original == PATCH_FILE || original == runtime_file {
        return Err("manifest service worker is already generated".into());
    }
    let is_module = background.get("type").and_then(Value::as_str) == Some("module");
    let loader_file = format!(
        "vmux_sw_{}.js",
        content_hash(&format!("{runtime_file}\0{PATCH_SOURCE}"))
    );
    let loader = if is_module {
        format!(
            "import \"./{runtime_file}\";\nimport \"./{PATCH_FILE}\";\nimport \"./{original}\";\n"
        )
    } else {
        format!(
            "importScripts(\"{runtime_file}\");\nimportScripts(\"{PATCH_FILE}\");\nimportScripts(\"{original}\");\n"
        )
    };
    std::fs::write(dir.join(PATCH_FILE), PATCH_SOURCE).map_err(|error| error.to_string())?;
    std::fs::write(dir.join(&loader_file), loader).map_err(|error| error.to_string())?;
    background.insert("service_worker".into(), Value::String(loader_file.clone()));
    std::fs::write(
        manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(loader_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_manifest(dir: &Path, background: Value) {
        let manifest = json!({
            "manifest_version": 3,
            "name": "t",
            "version": "1.0",
            "background": background,
        });
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("vmux_runtime.js"), "runtime").unwrap();
    }

    fn worker(dir: &Path) -> String {
        let raw = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
        let manifest: Value = serde_json::from_str(&raw).unwrap();
        manifest["background"]["service_worker"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn classic_worker_loads_runtime_patch_and_original() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), json!({ "service_worker": "background.js" }));

        let loader_file = install_worker_loader(dir.path(), "vmux_runtime.js").unwrap();

        assert_eq!(worker(dir.path()), loader_file);
        let loader = std::fs::read_to_string(dir.path().join(loader_file)).unwrap();
        assert!(loader.contains("importScripts(\"vmux_runtime.js\")"));
        assert!(loader.contains("importScripts(\"vmux_patch.js\")"));
        assert!(loader.contains("importScripts(\"background.js\")"));
        assert!(dir.path().join(PATCH_FILE).exists());
    }

    #[test]
    fn module_worker_uses_ordered_static_imports() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            json!({ "service_worker": "sw/main.js", "type": "module" }),
        );

        let loader_file = install_worker_loader(dir.path(), "vmux_runtime.js").unwrap();

        let loader = std::fs::read_to_string(dir.path().join(loader_file)).unwrap();
        let runtime = loader.find("import \"./vmux_runtime.js\"").unwrap();
        let patch = loader.find("import \"./vmux_patch.js\"").unwrap();
        let original = loader.find("import \"./sw/main.js\"").unwrap();
        assert!(runtime < patch && patch < original);
        assert!(!loader.contains("import("));
    }

    #[test]
    fn rejects_extension_without_service_worker() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), json!({ "scripts": ["bg.js"] }));

        assert!(install_worker_loader(dir.path(), "vmux_runtime.js").is_err());
        assert!(!dir.path().join(PATCH_FILE).exists());
    }
}
