use std::path::Path;

use serde_json::Value;

const PATCH_SOURCE: &str = include_str!("shim.js");
const PATCH_FILE: &str = "vmux_patch.js";
const SIDECAR_FILE: &str = "vmux_shim.json";

fn content_hash(source: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in source.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

fn is_generated_name(name: &str) -> bool {
    name.starts_with("vmux_sw_") || name == PATCH_FILE || name == "vmux_shim.js"
}

/// Injects a `chrome.windows`/`chrome.tabs` shim into an unpacked MV3 extension.
///
/// vmux embeds CEF in chrome-bootstrap + alloy-*style* browsers (OSR-capable, with no
/// `chrome/browser` `Browser` object), where `chrome.windows` resolves the focused
/// window to `WINDOW_ID_NONE` (`-1`) and `chrome.tabs.query`/`get` are unpopulated.
/// Extensions that read window bounds or the current tab — e.g. a password manager
/// positioning its autofill menu — then throw or find nothing. This rewrites the
/// extension's `background.service_worker` to a shim that captures the real tab from
/// message/port senders and supplies a synthetic focused window before loading the
/// original worker.
///
/// The loader filename embeds a hash of the shim source, so a shim update lands under
/// a new URL and bypasses Chromium's service-worker script cache. The original worker
/// name is recorded in a sidecar so repeated runs re-target it rather than chaining
/// onto a previous shim. No-op for extensions without a `background.service_worker`.
pub(crate) fn ensure_window_shim(dir: &Path) {
    let manifest_path = dir.join("manifest.json");
    let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
        return;
    };
    let Ok(mut manifest) = serde_json::from_str::<Value>(&raw) else {
        return;
    };

    let (current_worker, is_module) = {
        let Some(background) = manifest.get("background").and_then(Value::as_object) else {
            return;
        };
        let Some(worker) = background.get("service_worker").and_then(Value::as_str) else {
            return;
        };
        let is_module = background.get("type").and_then(Value::as_str) == Some("module");
        (worker.to_string(), is_module)
    };

    let loader_file = format!("vmux_sw_{}.js", content_hash(PATCH_SOURCE));
    if current_worker == loader_file {
        return;
    }

    let sidecar_original = std::fs::read_to_string(dir.join(SIDECAR_FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.get("original")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let original = sidecar_original.unwrap_or_else(|| current_worker.clone());
    if is_generated_name(&original) {
        return;
    }

    let loader = if is_module {
        format!("import \"./{PATCH_FILE}\";\nimport \"./{original}\";\n")
    } else {
        format!("importScripts(\"{PATCH_FILE}\");\nimportScripts(\"{original}\");\n")
    };
    if std::fs::write(dir.join(PATCH_FILE), PATCH_SOURCE).is_err()
        || std::fs::write(dir.join(&loader_file), loader).is_err()
    {
        return;
    }

    let sidecar = serde_json::json!({ "original": original, "loader": loader_file });
    let _ = std::fs::write(dir.join(SIDECAR_FILE), sidecar.to_string());

    if let Some(background) = manifest
        .get_mut("background")
        .and_then(Value::as_object_mut)
    {
        background.insert("service_worker".into(), Value::String(loader_file.clone()));
    }
    if let Ok(serialized) = serde_json::to_string_pretty(&manifest) {
        let _ = std::fs::write(&manifest_path, serialized);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("vmux_sw_") && name.ends_with(".js") && name != loader_file {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
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
    }

    fn worker(dir: &Path) -> String {
        let raw = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
        let manifest: Value = serde_json::from_str(&raw).unwrap();
        manifest["background"]["service_worker"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn loader_contents(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(worker(dir))).unwrap()
    }

    #[test]
    fn classic_worker_loads_patch_then_original() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), json!({ "service_worker": "background.js" }));

        ensure_window_shim(dir.path());

        assert!(worker(dir.path()).starts_with("vmux_sw_"));
        let loader = loader_contents(dir.path());
        assert!(loader.contains("importScripts(\"vmux_patch.js\")"));
        assert!(loader.contains("importScripts(\"background.js\")"));
        assert!(dir.path().join(PATCH_FILE).exists());
        assert!(
            std::fs::read_to_string(dir.path().join(PATCH_FILE))
                .unwrap()
                .contains("chrome.tabs")
        );
    }

    #[test]
    fn module_worker_uses_static_imports() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            json!({ "service_worker": "sw/main.js", "type": "module" }),
        );

        ensure_window_shim(dir.path());

        let loader = loader_contents(dir.path());
        assert!(loader.contains("import \"./vmux_patch.js\""));
        assert!(loader.contains("import \"./sw/main.js\""));
        assert!(!loader.contains("import("));
    }

    #[test]
    fn idempotent_second_run_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), json!({ "service_worker": "background.js" }));

        ensure_window_shim(dir.path());
        let first = worker(dir.path());
        ensure_window_shim(dir.path());

        assert_eq!(first, worker(dir.path()));
    }

    #[test]
    fn recovers_original_from_sidecar_and_cleans_stale_loader() {
        let dir = tempfile::tempdir().unwrap();
        // Simulate a prior shim under a different content hash.
        write_manifest(
            dir.path(),
            json!({ "service_worker": "vmux_sw_deadbeef.js" }),
        );
        std::fs::write(dir.path().join("vmux_sw_deadbeef.js"), "old").unwrap();
        std::fs::write(
            dir.path().join(SIDECAR_FILE),
            json!({ "original": "background.js", "loader": "vmux_sw_deadbeef.js" }).to_string(),
        )
        .unwrap();

        ensure_window_shim(dir.path());

        let loader = loader_contents(dir.path());
        assert!(loader.contains("importScripts(\"background.js\")"));
        assert!(!loader.contains("vmux_sw_deadbeef"));
        assert!(!dir.path().join("vmux_sw_deadbeef.js").exists());
    }

    #[test]
    fn refuses_to_chain_onto_a_shim_without_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), json!({ "service_worker": "vmux_shim.js" }));

        ensure_window_shim(dir.path());

        assert_eq!(worker(dir.path()), "vmux_shim.js");
    }

    #[test]
    fn skips_extension_without_service_worker() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), json!({ "scripts": ["bg.js"] }));

        ensure_window_shim(dir.path());

        assert!(!dir.path().join(PATCH_FILE).exists());
    }
}
