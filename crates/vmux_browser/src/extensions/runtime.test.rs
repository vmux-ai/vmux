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
    let generated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap())
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

    let generated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap())
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

    let generated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap())
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
