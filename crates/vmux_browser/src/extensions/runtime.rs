use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use vmux_core::extension::protocol::{
    BRIDGE_CHANNEL, BRIDGE_CONTEXT_ID, BRIDGE_MAX_FRAME_SIZE, BRIDGE_MAX_MESSAGE_SIZE,
    BRIDGE_PROTOCOL_VERSION, KEEPALIVE_CHANNEL,
};
use vmux_core::extension::{manifest, store};

use super::shim;

const WORKER_TEMPLATE: &str = include_str!("runtime/worker.js");
const CONTENT_SCRIPT: &str = include_str!("runtime/content.js");
const CONTENT_SCRIPT_FILE: &str = "vmux_content.js";
const BRIDGE_HTML: &str = include_str!("runtime/bridge.html");
const BRIDGE_TEMPLATE: &str = include_str!("runtime/bridge.js");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRuntime {
    pub extension_id: String,
    pub dir: PathBuf,
    pub runtime_hash: String,
    pub source_hash: String,
    pub permissions: Vec<String>,
    pub optional_permissions: Vec<String>,
    pub host_permissions: Vec<String>,
    pub optional_host_permissions: Vec<String>,
    pub granted_permissions: Vec<String>,
    pub granted_host_permissions: Vec<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct BridgeConfig<'a> {
    pub endpoint: &'a str,
    pub extension: &'a str,
    pub profile: &'a str,
    pub token: &'a str,
    pub conformance: bool,
}

pub fn prepare_runtime_in(
    root: &Path,
    runtime_store: &Path,
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

    let worker_source = render_worker_source()?;
    let runtime_hash = runtime_hash(&source_hash, &worker_source)?;
    let runtime_root = store::runtime_profile_dir(runtime_store, profile, &entry.id);
    std::fs::create_dir_all(&runtime_root).map_err(|error| error.to_string())?;
    let temp_dir = runtime_root.join(format!("current-{runtime_hash}.tmp"));
    let final_dir = runtime_root.join("current");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).map_err(|error| error.to_string())?;
    }
    copy_tree(&source, &temp_dir)?;
    if let Some(key) = entry.public_key_b64.as_deref() {
        manifest::prepare_unpacked(&temp_dir, key, entry.popup.as_deref())?;
    }
    std::fs::write(temp_dir.join("vmux_runtime.js"), worker_source)
        .map_err(|error| error.to_string())?;
    std::fs::write(temp_dir.join("vmux_bridge.html"), BRIDGE_HTML)
        .map_err(|error| error.to_string())?;
    install_content_script(&temp_dir)?;
    let loader = shim::install_worker_loader(&temp_dir, "vmux_runtime.js")?;
    if let Some(popup) = entry.popup.as_deref() {
        shim::install_page_loader(&temp_dir, popup)?;
    }
    validate_runtime(&temp_dir, &loader)?;
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temp_dir, &final_dir).map_err(|error| error.to_string())?;
    remove_sibling_runtimes(&runtime_root, &final_dir)?;

    let grants = entry.grants_for(profile);
    Ok(PreparedRuntime {
        extension_id: entry.id.clone(),
        dir: final_dir,
        runtime_hash,
        source_hash,
        permissions: entry.permissions.clone(),
        optional_permissions: entry.optional_permissions.clone(),
        host_permissions: entry.host_permissions.clone(),
        optional_host_permissions: entry.optional_host_permissions.clone(),
        granted_permissions: grants.permissions,
        granted_host_permissions: grants.host_permissions,
    })
}

pub(crate) fn bridge_source(config: &BridgeConfig<'_>) -> Result<String, String> {
    let mut replacements = protocol_replacements()?;
    replacements.push((
        "__VMUX_BRIDGE_CONFIG__",
        serde_json::to_string(config).map_err(|error| error.to_string())?,
    ));
    super::template::render(BRIDGE_TEMPLATE, &replacements)
}

fn render_worker_source() -> Result<String, String> {
    super::template::render(
        WORKER_TEMPLATE,
        &[
            (
                "__VMUX_BRIDGE_CHANNEL__",
                serde_json::to_string(BRIDGE_CHANNEL).map_err(|error| error.to_string())?,
            ),
            (
                "__VMUX_KEEPALIVE_CHANNEL__",
                serde_json::to_string(KEEPALIVE_CHANNEL).map_err(|error| error.to_string())?,
            ),
        ],
    )
}

fn install_content_script(dir: &Path) -> Result<(), String> {
    let manifest_path = dir.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let Some(content_scripts) = manifest
        .get_mut("content_scripts")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    let mut installed = false;
    for content_script in content_scripts {
        let Some(scripts) = content_script
            .get_mut("js")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        if scripts
            .iter()
            .any(|script| script.as_str() == Some(CONTENT_SCRIPT_FILE))
        {
            continue;
        }
        scripts.insert(
            0,
            serde_json::Value::String(CONTENT_SCRIPT_FILE.to_string()),
        );
        installed = true;
    }
    if !installed {
        return Ok(());
    }
    std::fs::write(dir.join(CONTENT_SCRIPT_FILE), CONTENT_SCRIPT)
        .map_err(|error| error.to_string())?;
    std::fs::write(
        manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn protocol_replacements() -> Result<Vec<(&'static str, String)>, String> {
    Ok(vec![
        (
            "__VMUX_BRIDGE_CHANNEL__",
            serde_json::to_string(BRIDGE_CHANNEL).map_err(|error| error.to_string())?,
        ),
        (
            "__VMUX_KEEPALIVE_CHANNEL__",
            serde_json::to_string(KEEPALIVE_CHANNEL).map_err(|error| error.to_string())?,
        ),
        (
            "__VMUX_BRIDGE_CONTEXT_ID__",
            serde_json::to_string(BRIDGE_CONTEXT_ID).map_err(|error| error.to_string())?,
        ),
        (
            "__VMUX_BRIDGE_PROTOCOL_VERSION__",
            BRIDGE_PROTOCOL_VERSION.to_string(),
        ),
        (
            "__VMUX_BRIDGE_MAX_FRAME_SIZE__",
            BRIDGE_MAX_FRAME_SIZE.to_string(),
        ),
        (
            "__VMUX_BRIDGE_MAX_MESSAGE_SIZE__",
            BRIDGE_MAX_MESSAGE_SIZE.to_string(),
        ),
    ])
}

fn runtime_hash(source_hash: &str, worker_source: &str) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(source_hash.as_bytes());
    hasher.update(worker_source.as_bytes());
    hasher.update(CONTENT_SCRIPT.as_bytes());
    hasher.update(BRIDGE_HTML.as_bytes());
    hasher.update(shim::patch_source()?.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
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
    let has_content_scripts = manifest
        .get("content_scripts")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|content_scripts| {
            content_scripts.iter().any(|content_script| {
                content_script
                    .get("js")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|scripts| !scripts.is_empty())
            })
        });
    if has_content_scripts && !dir.join(CONTENT_SCRIPT_FILE).is_file() {
        return Err("runtime content script is missing".into());
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
                profile_enabled: Default::default(),
                permissions: Vec::new(),
                optional_permissions: Vec::new(),
                host_permissions: Vec::new(),
                optional_host_permissions: Vec::new(),
                approved_grants: Default::default(),
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

        let prepared = prepare_runtime_in(root.path(), root.path(), "personal", &entry).unwrap();

        assert!(prepared.dir.starts_with(store::runtime_profile_dir(
            root.path(),
            "personal",
            &entry.id
        )));
        assert_eq!(prepared.dir.file_name().unwrap(), "current");
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
        assert!(!prepared.dir.join("vmux_bridge.js").exists());
        assert!(
            std::fs::read_to_string(prepared.dir.join("vmux_runtime.js"))
                .unwrap()
                .contains("__vmux_extension_keepalive_v1")
        );
        let bridge = bridge_source(&BridgeConfig {
            endpoint: "ws://127.0.0.1:1",
            extension: &entry.id,
            profile: "personal",
            token: "token",
            conformance: false,
        })
        .unwrap();
        assert!(bridge.contains("pulseWorker"));
        assert!(bridge.contains("CLOSE_POLICY_ERROR = 4008"));
        assert!(!bridge.contains(", 1002,"));
        assert!(!bridge.contains(", 1008,"));
        assert!(!bridge.contains(", 1009,"));
        assert!(!bridge.contains(", 1013,"));
        assert!(!bridge.contains("__VMUX_"));
    }

    #[test]
    fn prepares_runtime_outside_shared_package_store() {
        let root = tempfile::tempdir().unwrap();
        let runtime_store = tempfile::tempdir().unwrap();
        let (entry, _, _) = source_entry(root.path(), "background.js", false);

        let prepared =
            prepare_runtime_in(root.path(), runtime_store.path(), "personal", &entry).unwrap();

        assert!(prepared.dir.starts_with(store::runtime_profile_dir(
            runtime_store.path(),
            "personal",
            &entry.id
        )));
        assert!(!prepared.dir.starts_with(store::runtimes_root(root.path())));
    }

    #[test]
    fn prepares_module_worker_with_static_imports_in_order() {
        let root = tempfile::tempdir().unwrap();
        let (entry, _, _) = source_entry(root.path(), "sw/main.js", true);

        let prepared = prepare_runtime_in(root.path(), root.path(), "personal", &entry).unwrap();

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

    #[test]
    fn prepends_message_retry_to_static_content_scripts() {
        let root = tempfile::tempdir().unwrap();
        let (mut entry, source, _) = source_entry(root.path(), "background.js", false);
        std::fs::write(
            source.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "test",
                "version": "1.0",
                "background": { "service_worker": "background.js" },
                "content_scripts": [
                    { "matches": ["<all_urls>"], "js": ["first.js", "second.js"] },
                    { "matches": ["<all_urls>"], "css": ["style.css"] }
                ]
            }"#,
        )
        .unwrap();
        entry.source_hash = store::tree_sha256(&source).unwrap();

        let prepared = prepare_runtime_in(root.path(), root.path(), "personal", &entry).unwrap();

        let generated: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            generated["content_scripts"][0]["js"],
            serde_json::json!([CONTENT_SCRIPT_FILE, "first.js", "second.js"])
        );
        assert!(generated["content_scripts"][1].get("js").is_none());
        let retry = std::fs::read_to_string(prepared.dir.join(CONTENT_SCRIPT_FILE)).unwrap();
        assert!(retry.contains("Receiving end does not exist"));
        assert!(retry.contains("sendCallback(args, callback, attempt + 1)"));
        assert!(retry.contains("sendPromise(args, attempt + 1)"));
        assert!(retry.contains("__vmuxSenderUrl"));
        let worker = std::fs::read_to_string(prepared.dir.join("vmux_runtime.js")).unwrap();
        assert!(worker.contains("senderWithTab(message, sender, useLastTab)"));
        assert!(worker.contains("triggerAutofillScriptInjection"));
        assert!(worker.contains("normalizePortSender(port)"));
        assert!(worker.contains("endsWith(\"-message-connector\")"));
        assert!(retry.contains("nativeSetTimeout(resolve, 1000)"));
    }

    #[test]
    fn injects_page_shim_before_popup_application() {
        let root = tempfile::tempdir().unwrap();
        let (mut entry, source, _) = source_entry(root.path(), "background.js", false);
        entry.popup = Some("popup/index.html".into());
        std::fs::create_dir_all(source.join("popup")).unwrap();
        std::fs::write(
            source.join("popup/index.html"),
            "<!doctype html><html><head><script defer src=\"main.js\"></script></head></html>",
        )
        .unwrap();
        entry.source_hash = store::tree_sha256(&source).unwrap();

        let prepared = prepare_runtime_in(root.path(), root.path(), "personal", &entry).unwrap();

        let popup = std::fs::read_to_string(prepared.dir.join("popup/index.html")).unwrap();
        assert!(popup.find("/vmux_runtime.js").unwrap() < popup.find("/vmux_patch.js").unwrap());
        assert!(popup.find("/vmux_patch.js").unwrap() < popup.find("main.js").unwrap());
    }
}
