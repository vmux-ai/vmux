use std::path::{Path, PathBuf};

use bevy::asset::io::embedded::EmbeddedAssetRegistry;

use crate::page::PageManifest;

pub(crate) struct PageAssets;

impl PageAssets {
    pub(crate) fn default_document(host: &str, index: &str) -> String {
        let host = host.trim().trim_matches('/');
        if host.is_empty() {
            return index.to_string();
        }
        format!("{host}/{index}")
    }

    pub(crate) fn current_resources() -> Option<PathBuf> {
        std::env::current_exe()
            .ok()
            .and_then(|exe| Self::resources_from_exe(&exe))
    }

    pub(crate) fn resources_from_exe(exe: &Path) -> Option<PathBuf> {
        let macos = exe.parent()?;
        if macos.file_name()? != "MacOS" {
            return None;
        }
        let contents = macos.parent()?;
        if contents.file_name()? != "Contents" {
            return None;
        }
        Some(contents.join("Resources"))
    }

    pub(crate) fn packaged_root(resources: Option<&Path>, host: &str) -> Option<PathBuf> {
        let host = host.trim().trim_matches('/');
        if host.is_empty() {
            return None;
        }
        let root = resources?.join("webview-apps");
        let feature = root.join(host);
        if feature.is_dir() {
            return Some(feature);
        }
        let shared = root.join("_shared");
        shared.is_dir().then_some(shared)
    }

    pub(crate) fn root(manifest: &PageManifest, resources: Option<&Path>) -> PathBuf {
        Self::packaged_root(resources, manifest.asset_host)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_ui/dist"))
    }

    pub(crate) fn embed(
        registry: &mut EmbeddedAssetRegistry,
        manifest: &PageManifest,
        root: &Path,
    ) -> std::io::Result<()> {
        let host = manifest.asset_host.trim().trim_matches('/');
        let prefix = (!host.is_empty()).then(|| PathBuf::from(host));
        Self::embed_dir(registry, root, root, prefix.as_deref())
    }

    fn embed_dir(
        registry: &mut EmbeddedAssetRegistry,
        root: &Path,
        current: &Path,
        prefix: Option<&Path>,
    ) -> std::io::Result<()> {
        let entries = match std::fs::read_dir(current) {
            Ok(entries) => entries,
            Err(error) if current == root => return Err(error),
            Err(_) => return Ok(()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::embed_dir(registry, root, &path, prefix)?;
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let relative = PathBuf::from(relative.to_string_lossy().replace('\\', "/"));
            let embedded = prefix.map_or(relative.clone(), |prefix| prefix.join(&relative));
            let bytes = std::fs::read(&path)?;
            registry.insert_asset(path, embedded.as_path(), bytes);
        }
        Ok(())
    }
}
