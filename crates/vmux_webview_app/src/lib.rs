use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::prelude::{
    App, Commands, Component, IntoScheduleConfigs, On, Plugin, Res, ResMut, Resource, Startup,
    SystemSet,
};
use bevy_cef::prelude::{JsEmitEventPlugin, Receive};
use bevy_cef_core::prelude::{CefEmbeddedHost, CefEmbeddedHosts};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[cfg(feature = "build")]
pub mod build;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WebviewAppEmbedSet;

#[derive(Clone, Copy, Component, Debug, Default, Deserialize)]
pub struct UiReady {}

#[derive(Clone, Debug)]
pub struct WebviewAppConfig {
    pub scheme: &'static str,
    pub host: &'static str,
    pub bundle_dir: &'static str,
    pub index_file_path: &'static str,
}

impl WebviewAppConfig {
    pub const fn with_custom_host(host: &'static str) -> Self {
        Self {
            scheme: "vmux",
            host,
            bundle_dir: "dist",
            index_file_path: "index.html",
        }
    }
}

#[derive(Clone, Debug)]
struct WebviewAppEntry {
    manifest_dir: PathBuf,
    bundle_dir: String,
    host: String,
    index_file_path: String,
}

#[derive(Clone, Debug, Resource, Default)]
pub struct WebviewAppRegistry {
    entries: Vec<WebviewAppEntry>,
}

impl WebviewAppRegistry {
    pub fn register(&mut self, manifest_dir: impl Into<PathBuf>, config: &WebviewAppConfig) {
        self.entries.push(WebviewAppEntry {
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

pub struct WebviewAppRegistryPlugin;

impl Plugin for WebviewAppRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WebviewAppRegistry>()
            .configure_sets(Startup, WebviewAppEmbedSet)
            .add_systems(
                Startup,
                embed_webview_app_static_assets.in_set(WebviewAppEmbedSet),
            );
    }
}

pub struct JsEmitUiReadyPlugin;

impl Plugin for JsEmitUiReadyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<UiReady>::default())
            .add_observer(mark_webview_ui_ready_on_js_emit);
    }
}

fn mark_webview_ui_ready_on_js_emit(trigger: On<Receive<UiReady>>, mut commands: Commands) {
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

fn embed_webview_app_static_assets(
    registry: Res<WebviewAppRegistry>,
    mut reg: ResMut<EmbeddedAssetRegistry>,
) {
    for entry in &registry.entries {
        let bundle_root = entry.manifest_dir.join(&entry.bundle_dir);
        if !bundle_root.is_dir() {
            bevy::log::warn!("WebviewAppPlugin: skip {:?}: not a directory", bundle_root);
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
            bevy::log::error!("WebviewAppPlugin: failed to embed {:?}: {e}", bundle_root);
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
