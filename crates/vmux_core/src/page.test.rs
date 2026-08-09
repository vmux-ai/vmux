use super::*;

#[test]
fn page_manifest_url_derives_from_host() {
    let manifest = PageManifest {
        host: "settings",
        title: "Settings",
        keywords: &["preferences"],
        icon: Some(crate::icon::BuiltinIcon::Settings),
        command_bar: true,
    };
    assert_eq!(manifest.url(), "vmux://settings/");
}

#[test]
fn packaged_page_root_uses_resources_webview_host_dir() {
    let root = std::env::temp_dir().join(format!("vmux-webview-app-test-{}", std::process::id()));
    let host_dir = root.join("webview-apps").join("terminal");
    std::fs::create_dir_all(&host_dir).unwrap();

    let found = packaged_page_root(Some(&root), "terminal");

    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(found, Some(host_dir));
}

#[test]
fn packaged_page_root_ignores_missing_host_dir() {
    let root = std::env::temp_dir().join(format!(
        "vmux-webview-app-missing-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();

    let found = packaged_page_root(Some(&root), "terminal");

    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(found, None);
}

#[test]
fn page_manifest_registers_host() {
    let mut app = App::new();
    app.world_mut().spawn(PageManifest {
        host: "history",
        title: "History",
        keywords: &["recent", "visited"],
        icon: Some(crate::icon::BuiltinIcon::Clock),
        command_bar: true,
    });
    let mut query = app.world_mut().query::<&PageManifest>();

    let hosts = bevy_cef_core::prelude::CefEmbeddedHosts(
        query
            .iter(app.world())
            .map(PageManifest::embedded_host)
            .collect(),
    );

    assert!(hosts.entry_for_host("history").is_some());
}

#[test]
fn registered_hosts_use_vmux_server_dist() {
    let manifest = PageManifest {
        host: "history",
        title: "History",
        keywords: &["recent", "visited"],
        icon: Some(crate::icon::BuiltinIcon::Clock),
        command_bar: true,
    };

    assert_eq!(
        manifest.bundle_root(None),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_server/dist")
    );
}

#[test]
fn packaged_page_root_falls_back_to_shared_webview_dist() {
    let root = std::env::temp_dir().join(format!(
        "vmux-webview-app-shared-test-{}",
        std::process::id()
    ));
    let shared = root.join("webview-apps").join("_shared");
    std::fs::create_dir_all(&shared).unwrap();

    let found = packaged_page_root(Some(&root), "history");

    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(found, Some(shared));
}

#[test]
fn macos_resources_dir_resolves_from_bundle_executable() {
    let exe = Path::new("/Applications/Vmux.app/Contents/MacOS/Vmux");

    let resources = macos_resources_dir_from_exe(exe);

    assert_eq!(
        resources,
        Some(PathBuf::from("/Applications/Vmux.app/Contents/Resources"))
    );
}
