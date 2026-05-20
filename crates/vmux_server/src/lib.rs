use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::prelude::{
    App, Commands, Component, IntoScheduleConfigs, On, Plugin, Res, ResMut, Resource, Startup,
    SystemSet,
};
use bevy_cef::prelude::BinReceive;
use bevy_cef_core::prelude::{CefEmbeddedHost, CefEmbeddedHosts, webview_debug_log};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const PAGE_READY_BIN_EVENT_ID: &str = "vmux-page-ready";

#[cfg(feature = "build")]
pub mod build;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerEmbedSet;

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

#[derive(Clone, Debug)]
pub struct PageConfig {
    pub scheme: &'static str,
    pub host: &'static str,
    pub bundle_dir: &'static str,
    pub index_file_path: &'static str,
}

impl PageConfig {
    pub const fn with_custom_host(host: &'static str) -> Self {
        Self {
            scheme: "vmux",
            host,
            bundle_dir: "dist",
            index_file_path: "index.html",
        }
    }

    pub const fn with_bundle_dir(mut self, bundle_dir: &'static str) -> Self {
        self.bundle_dir = bundle_dir;
        self
    }
}

#[derive(Clone, Debug)]
struct PageEntry {
    manifest_dir: PathBuf,
    bundle_dir: String,
    host: String,
    index_file_path: String,
}

impl PageEntry {
    fn bundle_root(&self, resources_dir: Option<&Path>) -> PathBuf {
        packaged_page_root(resources_dir, &self.host)
            .unwrap_or_else(|| self.manifest_dir.join(&self.bundle_dir))
    }
}

#[derive(Clone, Debug, Resource, Default)]
pub struct Server {
    entries: Vec<PageEntry>,
}

impl Server {
    pub fn register(&mut self, manifest_dir: impl Into<PathBuf>, config: &PageConfig) {
        self.entries.push(PageEntry {
            manifest_dir: manifest_dir.into(),
            bundle_dir: config.bundle_dir.to_string(),
            host: config.host.to_string(),
            index_file_path: config.index_file_path.to_string(),
        });
    }

    pub fn embedded_hosts(&self) -> CefEmbeddedHosts {
        CefEmbeddedHosts(
            self.entries
                .iter()
                .map(|e| {
                    let index = e.index_file_path.replace('\\', "/");
                    let index_norm = index.trim_start_matches('/');
                    CefEmbeddedHost {
                        host: e.host.clone(),
                        default_document: embedded_default_document(&e.host, index_norm),
                    }
                })
                .collect(),
        )
    }
}

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Server>()
            .configure_sets(Startup, ServerEmbedSet)
            .add_systems(Startup, embed_page_static_assets.in_set(ServerEmbedSet));
    }
}

pub fn mark_webview_page_ready_on_js_emit(
    trigger: On<BinReceive<PageReady>>,
    mut commands: Commands,
) {
    webview_debug_log(format!("PageReady entity={:?}", trigger.event().webview));
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
    let root = resources_dir?.join("webview-apps").join(h);
    root.is_dir().then_some(root)
}

fn embed_page_static_assets(registry: Res<Server>, mut reg: ResMut<EmbeddedAssetRegistry>) {
    let resources_dir = current_app_resources_dir();
    for entry in &registry.entries {
        let bundle_root = entry.bundle_root(resources_dir.as_deref());
        if !bundle_root.is_dir() {
            bevy::log::warn!("PagePlugin: skip {:?}: not a directory", bundle_root);
            continue;
        }
        let host_trim = entry.host.trim().trim_matches('/');
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
            webview_debug_log(format!(
                "embed asset source={} embedded={}",
                p.display(),
                embedded_path.display()
            ));
            reg.insert_asset(p, embedded_path.as_path(), bytes);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn macos_resources_dir_resolves_from_bundle_executable() {
        let exe = Path::new("/Applications/Vmux.app/Contents/MacOS/Vmux");

        let resources = macos_resources_dir_from_exe(exe);

        assert_eq!(
            resources,
            Some(PathBuf::from("/Applications/Vmux.app/Contents/Resources"))
        );
    }
}
