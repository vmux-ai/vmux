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
    assert_eq!(loader_file, WORKER_LOADER_FILE);
    let loader = std::fs::read_to_string(dir.path().join(loader_file)).unwrap();
    assert!(loader.contains("importScripts(\"vmux_runtime.js\")"));
    assert!(loader.contains("importScripts(\"vmux_patch.js\")"));
    assert!(loader.contains("importScripts(\"background.js\")"));
    assert!(dir.path().join(PATCH_FILE).exists());
    let patch = std::fs::read_to_string(dir.path().join(PATCH_FILE)).unwrap();
    assert!(patch.contains("message.channel === BRIDGE_CHANNEL"));
    assert!(patch.contains("capture(message, sender)"));
    for method in [
        "get",
        "getCurrent",
        "getLastFocused",
        "getAll",
        "create",
        "update",
        "remove",
    ] {
        assert!(patch.contains(&format!("windowRequest(\"{method}\"")));
    }
    for event in [
        "onCreated",
        "onRemoved",
        "onFocusChanged",
        "onBoundsChanged",
    ] {
        assert!(patch.contains(&format!("patchWindowEvent(\"{event}\")")));
    }
    assert!(patch.contains("WINDOW_ID_NONE"));
    assert!(patch.contains("WINDOW_ID_CURRENT"));
    assert!(patch.contains("normalizeTabWindowIds"));
    assert!(patch.contains("tabMatchesQuery"));
    assert!(patch.contains("knownWindows.length"));
    assert!(patch.contains("typeof result === \"undefined\" ? useFallback() : result"));
    assert!(!patch.contains("self.clients.openWindow"));
    assert!(!patch.contains("openPopout(info)"));
    assert!(patch.contains("__vmux_active_tab_v1"));
    assert!(patch.contains("c.storage.session.set(stored)"));
    assert!(patch.contains("event.addListener = function"));
}

#[test]
fn windows_namespace_is_fully_shimmed() {
    let patch = patch_source().unwrap();

    for member in [
        "WINDOW_ID_NONE",
        "WINDOW_ID_CURRENT",
        "c.windows.get =",
        "c.windows.getCurrent =",
        "c.windows.getLastFocused =",
        "c.windows.getAll =",
        "c.windows.create =",
        "c.windows.update =",
        "c.windows.remove =",
        "onCreated",
        "onRemoved",
        "onFocusChanged",
        "onBoundsChanged",
        "globalThis.close = function",
        "tabMatchesQuery",
    ] {
        assert!(patch.contains(member));
    }
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

#[test]
fn page_loader_runs_before_extension_scripts_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("popup")).unwrap();
    let path = dir.path().join("popup/index.html");
    std::fs::write(
        &path,
        "<!doctype html><html><head><script defer src=\"main.js\"></script></head></html>",
    )
    .unwrap();

    install_page_loader(dir.path(), "popup/index.html").unwrap();
    install_page_loader(dir.path(), "popup/index.html").unwrap();

    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.find(PAGE_LOADER).unwrap() < html.find("main.js").unwrap());
    assert!(html.find("vmux_runtime.js").unwrap() < html.find("vmux_patch.js").unwrap());
    assert_eq!(html.matches(PAGE_LOADER).count(), 1);
}

#[test]
fn page_loader_rejects_path_escape() {
    let dir = tempfile::tempdir().unwrap();

    assert!(install_page_loader(dir.path(), "../popup.html").is_err());
}
