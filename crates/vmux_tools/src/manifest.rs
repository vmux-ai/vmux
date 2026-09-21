use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dotfiles::DotfilesManifest;
use crate::homebrew::{brewfile_path, sync_manifest_from_brewfile, write_managed_brewfile};
use crate::mcp::McpManifest;

const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolsManifest {
    #[serde(default = "manifest_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packages: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "McpManifest::is_empty")]
    pub mcp: McpManifest,
    #[serde(default, skip_serializing_if = "DotfilesManifest::is_empty")]
    pub dotfiles: DotfilesManifest,
}

impl Default for ToolsManifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            packages: BTreeMap::new(),
            mcp: McpManifest::default(),
            dotfiles: DotfilesManifest::default(),
        }
    }
}

impl ToolsManifest {
    pub fn contains(&self, provider: &str, name: &str) -> bool {
        self.packages
            .get(provider)
            .is_some_and(|packages| packages.iter().any(|package| package == name))
    }

    pub fn set_package(&mut self, provider: &str, name: &str, enabled: bool) {
        if enabled {
            let packages = self.packages.entry(provider.to_string()).or_default();
            if !packages.iter().any(|package| package == name) {
                packages.push(name.to_string());
            }
        } else if let Some(packages) = self.packages.get_mut(provider) {
            packages.retain(|package| package != name);
            if packages.is_empty() {
                self.packages.remove(provider);
            }
        }
        self.normalize();
    }

    pub fn set_dotfile_package(&mut self, name: &str, enabled: bool) {
        if enabled {
            if !self.dotfiles.packages.iter().any(|package| package == name) {
                self.dotfiles.packages.push(name.to_string());
            }
        } else {
            self.dotfiles.packages.retain(|package| package != name);
        }
        self.normalize();
    }

    pub(crate) fn normalize(&mut self) {
        self.packages.retain(|_, packages| {
            packages.sort_by_key(|package| package.to_ascii_lowercase());
            packages.dedup();
            !packages.is_empty()
        });
        self.dotfiles
            .packages
            .sort_by_key(|package| package.to_ascii_lowercase());
        self.dotfiles.packages.dedup();
        self.mcp.servers.remove("vmux");
    }
}

pub fn root_dir() -> PathBuf {
    vmux_profile::config_dir().join("tools")
}

pub fn manifest_path() -> PathBuf {
    root_dir().join("tools.toml")
}

pub(crate) fn migrate_legacy_storage() -> Result<(), String> {
    migrate_legacy_storage_in(&vmux_profile::config_dir())
}

pub(crate) fn migrate_legacy_storage_in(config_dir: &Path) -> Result<(), String> {
    let legacy_root = config_dir.join("registry");
    let tools_root = config_dir.join("tools");
    if legacy_root.symlink_metadata().is_ok() {
        if tools_root.symlink_metadata().is_ok() {
            return Err(format!(
                "cannot migrate {} because {} already exists",
                legacy_root.display(),
                tools_root.display()
            ));
        }
        rename_for_migration(&legacy_root, &tools_root)?;
    }
    let legacy_manifest = tools_root.join("registry.toml");
    let tools_manifest = tools_root.join("tools.toml");
    if legacy_manifest.symlink_metadata().is_ok() {
        if tools_manifest.symlink_metadata().is_ok() {
            return Err(format!(
                "cannot migrate {} because {} already exists",
                legacy_manifest.display(),
                tools_manifest.display()
            ));
        }
        rename_for_migration(&legacy_manifest, &tools_manifest)?;
    }
    Ok(())
}

fn rename_for_migration(source: &Path, destination: &Path) -> Result<(), String> {
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) if source.symlink_metadata().is_err() && destination.symlink_metadata().is_ok() => {
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

pub fn load_manifest() -> Result<ToolsManifest, String> {
    migrate_legacy_storage()?;
    let mut manifest = load_manifest_from(&manifest_path())?;
    let brewfile = brewfile_path();
    if brewfile.is_file() {
        sync_manifest_from_brewfile(&mut manifest, &brewfile)?;
    } else {
        write_managed_brewfile(&manifest)?;
    }
    Ok(manifest)
}

pub fn load_manifest_from(path: &Path) -> Result<ToolsManifest, String> {
    if !path.is_file() {
        return Ok(ToolsManifest::default());
    }
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut manifest: ToolsManifest = toml::from_str(&source).map_err(|error| error.to_string())?;
    if manifest.version != MANIFEST_VERSION {
        return Err(format!(
            "unsupported tools manifest version: {}",
            manifest.version
        ));
    }
    manifest.normalize();
    Ok(manifest)
}

pub fn write_manifest(manifest: &ToolsManifest) -> Result<(), String> {
    migrate_legacy_storage()?;
    write_manifest_to(&manifest_path(), manifest)?;
    write_managed_brewfile(manifest)
}

pub fn write_manifest_to(path: &Path, manifest: &ToolsManifest) -> Result<(), String> {
    let mut manifest = manifest.clone();
    manifest.version = MANIFEST_VERSION;
    manifest.normalize();
    let source = toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    let parent = path.parent().ok_or("tools manifest has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, source).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

pub(crate) fn add_packages(
    manifest: &mut ToolsManifest,
    provider: &str,
    names: &[String],
) -> usize {
    let mut imported = 0;
    for name in names {
        imported += usize::from(!manifest.contains(provider, name));
        manifest.set_package(provider, name, true);
    }
    imported
}

pub(crate) fn normalize_names(names: &mut Vec<String>) {
    names.retain(|name| !name.trim().is_empty());
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup();
}

pub(crate) fn expand_user_path(path: &Path) -> Result<PathBuf, String> {
    if let Ok(relative) = path.strip_prefix("~") {
        return Ok(home_dir().join(relative));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn manifest_version() -> u32 {
    MANIFEST_VERSION
}

pub fn managed_package_set(manifest: &ToolsManifest, provider: &str) -> BTreeSet<String> {
    manifest
        .packages
        .get(provider)
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}
