use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry,
    prelude::{
        App, Commands, Component, Entity, IntoScheduleConfigs, Message, On, Plugin, Query, ResMut,
        Startup, SystemSet,
    },
};
use bevy_cef::prelude::BinReceive;
use bevy_cef_core::prelude::{CefEmbeddedHost, webview_debug_log};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Embeds each page manifest's static webview bundle into Bevy's asset registry so pages
/// can be served over `vmux://` URLs.
pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Startup, ServerEmbedSet)
            .add_systems(Startup, embed_page_static_assets.in_set(ServerEmbedSet));
    }
}

pub const PAGE_READY_BIN_EVENT_ID: &str = "vmux-page-ready";

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageManifest {
    pub host: &'static str,
    pub title: &'static str,
    pub keywords: &'static [&'static str],
    pub icon: Option<crate::icon::BuiltinIcon>,
    pub command_bar: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrewarmPage {
    pub host: &'static str,
    pub url: &'static str,
    pub title: &'static str,
    pub pool_size: usize,
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
        packaged_page_root(resources_dir, self.host).unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_server/dist")
        })
    }
}

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

pub fn mark_webview_page_ready(trigger: On<BinReceive<PageReady>>, mut commands: Commands) {
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

#[derive(Message, Debug, Clone)]
pub struct SettingsPageSpawnRequest {
    pub target_stack: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct SpacesPageSpawnRequest {
    pub target_stack: Entity,
}

#[cfg(test)]
#[path = "page.page_ready.test.rs"]
mod page_ready_tests;
#[cfg(test)]
#[path = "page.test.rs"]
mod tests;
