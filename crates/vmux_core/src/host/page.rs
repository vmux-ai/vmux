use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry,
    prelude::{
        App, Commands, Component, Entity, IntoScheduleConfigs, Message, On, Plugin, Query, ResMut,
        Startup, SystemSet,
    },
};
use bevy_cef::prelude::BinReceive;
use bevy_cef_core::prelude::CefEmbeddedHost;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Embeds each page manifest's static webview bundle into Bevy's asset registry so pages
/// can be served over `vmux://` URLs.
pub struct PagePlugin;

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Startup, PageEmbedSet)
            .add_systems(Startup, embed_page_static_assets.in_set(PageEmbedSet));
    }
}

pub const PAGE_READY_BIN_EVENT_ID: &str = "vmux-page-ready";

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageManifest {
    pub host: &'static str,
    pub title: &'static str,
    /// Fluent id naming this page in the command bar, resolved against the active locale.
    ///
    /// `None` falls back to [`Self::title`], which is untranslated. Declared here rather than
    /// looked up by host so a page names itself, and adding one does not mean editing the command
    /// bar.
    pub title_message_id: Option<&'static str>,
    /// A command id this page supersedes.
    ///
    /// The command's own row is dropped from the command bar and its shortcut is shown on this
    /// page's entry instead, because reaching the page is what the command did. Its menu item and
    /// its keybinding are untouched.
    pub replaces_command: Option<&'static str>,
    pub keywords: &'static [&'static str],
    pub icon: Option<crate::icon::BuiltinIcon>,
    pub command_bar: bool,
}

/// A Dioxus page is mounted on this entity, wherever its components run.
///
/// `WebviewSource` used to be the way to ask this, because a page was always a URL a CEF browser
/// loaded. That stopped being true when the layout began running its components in the host
/// process: it hosts a page and has no source, so every query filtered on one silently skipped it
/// — which left its keyboard claims empty and its published context read by nobody.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostsPage;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrewarmPage {
    pub host: &'static str,
    pub url: &'static str,
    pub title: &'static str,
    pub pool_size: usize,
}

/// A page whose components run in this process, declared beside its [`PageManifest`].
///
/// The opposite of [`PrewarmPage`], and a page has one or the other. A prewarmed page keeps hidden
/// browsers mounted so it can be revealed rather than loaded; this one has nothing to keep warm,
/// because mounting it is building a `VirtualDom` rather than starting a browser and fetching a
/// bundle into it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativelyHosted {
    pub url: &'static str,
    pub title: &'static str,
}

impl PageManifest {
    pub fn embedded_host(&self) -> CefEmbeddedHost {
        CefEmbeddedHost {
            host: self.host.to_string(),
            default_document: embedded_default_document(self.host, "index.html"),
        }
    }

    pub fn url(&self) -> String {
        let host = self.host.trim().trim_matches('/');
        format!("vmux://{host}/")
    }

    fn bundle_root(&self, resources_dir: Option<&Path>) -> PathBuf {
        packaged_page_root(resources_dir, self.host)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_page/dist"))
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageEmbedSet;

#[derive(
    Clone,
    Copy,
    Component,
    Debug,
    Default,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PageReady {}

pub fn mark_webview_page_ready(trigger: On<BinReceive<PageReady>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .insert(trigger.event().payload);
}

fn embedded_default_document(host: &str, index_file_path: &str) -> String {
    let h = host.trim().trim_matches('/');
    if h.is_empty() {
        return index_file_path.to_string();
    }
    format!("{h}/{index_file_path}")
}

fn macos_resources_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()? != "MacOS" {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name()? != "Contents" {
        return None;
    }
    Some(contents_dir.join("Resources"))
}

fn current_app_resources_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| macos_resources_dir_from_exe(&exe))
}

fn packaged_page_root(resources_dir: Option<&Path>, host: &str) -> Option<PathBuf> {
    let h = host.trim().trim_matches('/');
    if h.is_empty() {
        return None;
    }
    let root = resources_dir?.join("webview-apps");
    let host_root = root.join(h);
    if host_root.is_dir() {
        return Some(host_root);
    }
    let shared_root = root.join("_shared");
    shared_root.is_dir().then_some(shared_root)
}

fn embed_page_static_assets(
    manifests: Query<&PageManifest>,
    mut reg: ResMut<EmbeddedAssetRegistry>,
) {
    let resources_dir = current_app_resources_dir();
    for manifest in &manifests {
        let bundle_root = manifest.bundle_root(resources_dir.as_deref());
        if !bundle_root.is_dir() {
            bevy::log::warn!("PagePlugin: skip {:?}: not a directory", bundle_root);
            continue;
        }
        let host_trim = manifest.host.trim().trim_matches('/');
        let prefix = if host_trim.is_empty() {
            None
        } else {
            Some(PathBuf::from(host_trim))
        };
        if let Err(e) = embed_dir_recursive(
            &mut reg,
            &bundle_root,
            &bundle_root,
            None,
            prefix.as_deref(),
        ) {
            bevy::log::error!("PagePlugin: failed to embed {:?}: {e}", bundle_root);
        }
    }
}

fn embed_dir_recursive(
    reg: &mut EmbeddedAssetRegistry,
    root_dir: &Path,
    cur: &Path,
    map_root_index_to: Option<&Path>,
    embed_path_prefix: Option<&Path>,
) -> std::io::Result<()> {
    let read_dir = match std::fs::read_dir(cur) {
        Ok(rd) => rd,
        Err(e) if cur == root_dir => return Err(e),
        Err(_) => return Ok(()),
    };
    for e in read_dir.flatten() {
        let p = e.path();
        if p.is_dir() {
            embed_dir_recursive(reg, root_dir, &p, map_root_index_to, embed_path_prefix)?;
        } else {
            let Ok(rel) = p.strip_prefix(root_dir) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let mut embedded_path: PathBuf = if rel_str == "index.html" {
                map_root_index_to
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(&rel_str))
            } else {
                PathBuf::from(&rel_str)
            };
            if let Some(prefix) = embed_path_prefix {
                embedded_path = prefix.join(&embedded_path);
            }
            let bytes = std::fs::read(&p)?;
            reg.insert_asset(p, embedded_path.as_path(), bytes);
        }
    }
    Ok(())
}

#[derive(Message, Debug, Clone)]
pub struct SettingsPageSpawnRequest {
    pub target_stack: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct SpacesPageSpawnRequest {
    pub target_stack: Entity,
}

#[cfg(test)]
mod page_ready_tests {
    use super::*;

    #[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct PageReadyPayloadProbe {}

    #[test]
    fn page_ready_cross_type_rkyv_compat() {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&PageReadyPayloadProbe {}).expect("ser");
        println!("PageReady archive byte length: {}", bytes.len());
        println!("PageReady archive bytes: {:?}", &bytes[..]);
        let _decoded =
            rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&bytes).expect("cross-type decode");
    }

    #[test]
    fn page_ready_self_rkyv_roundtrip() {
        let original = PageReady {};
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        println!("PageReady self archive byte length: {}", bytes.len());
        let _decoded =
            rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&bytes).expect("self decode");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_manifest_url_derives_from_host() {
        let manifest = PageManifest {
            host: "settings",
            title: "Settings",
            title_message_id: Some("settings-title"),
            replaces_command: None,
            keywords: &["preferences"],
            icon: Some(crate::icon::BuiltinIcon::Settings),
            command_bar: true,
        };
        assert_eq!(manifest.url(), "vmux://settings/");
    }

    #[test]
    fn packaged_page_root_uses_resources_webview_host_dir() {
        let root =
            std::env::temp_dir().join(format!("vmux-webview-app-test-{}", std::process::id()));
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
            title_message_id: Some("history-title"),
            replaces_command: Some("browser_open_history"),
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
    fn registered_hosts_use_vmux_page_dist() {
        let manifest = PageManifest {
            host: "history",
            title: "History",
            title_message_id: Some("history-title"),
            replaces_command: Some("browser_open_history"),
            keywords: &["recent", "visited"],
            icon: Some(crate::icon::BuiltinIcon::Clock),
            command_bar: true,
        };

        assert_eq!(
            manifest.bundle_root(None),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_page/dist")
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
}
