use serde_json::Value;
use std::path::{Component, Path};
use vmux_core::extension::protocol::{BRIDGE_CHANNEL, KEEPALIVE_CHANNEL};

const PATCH_TEMPLATE: &str = include_str!("shim.js");
const PATCH_FILE: &str = "vmux_patch.js";
const WORKER_LOADER_FILE: &str = "vmux_sw.js";
const PAGE_LOADER: &str =
    r#"<script src="/vmux_runtime.js"></script><script src="/vmux_patch.js"></script>"#;

pub(crate) fn patch_source() -> Result<String, String> {
    super::template::render(
        PATCH_TEMPLATE,
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
    if original == WORKER_LOADER_FILE || original == PATCH_FILE || original == runtime_file {
        return Err("manifest service worker is already generated".into());
    }
    let is_module = background.get("type").and_then(Value::as_str) == Some("module");
    let patch_source = patch_source()?;
    let loader_file = WORKER_LOADER_FILE.to_string();
    let loader = if is_module {
        format!(
            "import \"./{runtime_file}\";\nimport \"./{PATCH_FILE}\";\nimport \"./{original}\";\n"
        )
    } else {
        format!(
            "importScripts(\"{runtime_file}\");\nimportScripts(\"{PATCH_FILE}\");\nimportScripts(\"{original}\");\n"
        )
    };
    std::fs::write(dir.join(PATCH_FILE), patch_source).map_err(|error| error.to_string())?;
    std::fs::write(dir.join(&loader_file), loader).map_err(|error| error.to_string())?;
    background.insert("service_worker".into(), Value::String(loader_file.clone()));
    std::fs::write(
        manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(loader_file)
}

pub(crate) fn install_page_loader(dir: &Path, popup: &str) -> Result<(), String> {
    if popup.is_empty()
        || Path::new(popup)
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("manifest popup path is invalid".into());
    }
    let path = dir.join(popup);
    let html = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    if html.contains(PAGE_LOADER) {
        return Ok(());
    }
    let patched = if let Some(index) = html.find("<head>") {
        let index = index + "<head>".len();
        format!("{}{}{}", &html[..index], PAGE_LOADER, &html[index..])
    } else {
        format!("{PAGE_LOADER}{html}")
    };
    std::fs::write(path, patched).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "shim.test.rs"]
mod tests;
