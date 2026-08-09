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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PrepareRuntimeError {
    Corrupt(String),
    Infrastructure(String),
}

#[derive(serde::Serialize)]
pub(crate) struct BridgeConfig<'a> {
    pub endpoint: &'a str,
    pub extension: &'a str,
    pub profile: &'a str,
    pub token: &'a str,
    pub conformance: bool,
}

pub(crate) fn prepare_runtime_in(
    root: &Path,
    runtime_store: &Path,
    profile: &str,
    entry: &store::ExtEntry,
) -> Result<PreparedRuntime, PrepareRuntimeError> {
    let expected_source = store::source_dir(root, &entry.id, &entry.version);
    let source = if expected_source.exists() {
        expected_source
    } else {
        store::migrate_legacy_package(root, entry).map_err(PrepareRuntimeError::Infrastructure)?
    };
    let source_hash = store::tree_sha256(&source).map_err(PrepareRuntimeError::Infrastructure)?;
    if !entry.source_hash.is_empty() && source_hash != entry.source_hash {
        return Err(PrepareRuntimeError::Corrupt(format!(
            "source hash mismatch for {}",
            entry.id
        )));
    }

    let worker_source = render_worker_source().map_err(PrepareRuntimeError::Infrastructure)?;
    let runtime_hash =
        runtime_hash(&source_hash, &worker_source).map_err(PrepareRuntimeError::Infrastructure)?;
    let runtime_root = store::runtime_profile_dir(runtime_store, profile, &entry.id);
    std::fs::create_dir_all(&runtime_root)
        .map_err(|error| PrepareRuntimeError::Infrastructure(error.to_string()))?;
    let temp_dir = runtime_root.join(format!("current-{runtime_hash}.tmp"));
    let final_dir = runtime_root.join("current");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)
            .map_err(|error| PrepareRuntimeError::Infrastructure(error.to_string()))?;
    }
    copy_tree(&source, &temp_dir).map_err(PrepareRuntimeError::Infrastructure)?;
    if let Some(key) = entry.public_key_b64.as_deref() {
        manifest::prepare_unpacked(&temp_dir, key, entry.popup.as_deref())
            .map_err(PrepareRuntimeError::Infrastructure)?;
    }
    std::fs::write(temp_dir.join("vmux_runtime.js"), worker_source)
        .map_err(|error| PrepareRuntimeError::Infrastructure(error.to_string()))?;
    std::fs::write(temp_dir.join("vmux_bridge.html"), BRIDGE_HTML)
        .map_err(|error| PrepareRuntimeError::Infrastructure(error.to_string()))?;
    install_content_script(&temp_dir).map_err(PrepareRuntimeError::Infrastructure)?;
    let loader = shim::install_worker_loader(&temp_dir, "vmux_runtime.js")
        .map_err(PrepareRuntimeError::Infrastructure)?;
    if let Some(popup) = entry.popup.as_deref() {
        shim::install_page_loader(&temp_dir, popup).map_err(PrepareRuntimeError::Infrastructure)?;
    }
    validate_runtime(&temp_dir, &loader).map_err(PrepareRuntimeError::Infrastructure)?;
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir)
            .map_err(|error| PrepareRuntimeError::Infrastructure(error.to_string()))?;
    }
    std::fs::rename(&temp_dir, &final_dir)
        .map_err(|error| PrepareRuntimeError::Infrastructure(error.to_string()))?;
    remove_sibling_runtimes(&runtime_root, &final_dir)
        .map_err(PrepareRuntimeError::Infrastructure)?;

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
#[path = "runtime.test.rs"]
mod tests;
