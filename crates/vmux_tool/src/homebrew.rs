use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::tool::{ToolAction, ToolProvider};

use crate::manifest::{
    ToolStore, ToolsManifest, add_packages, expand_user_path, load_manifest_from, normalize_names,
    write_manifest_to,
};
use crate::{
    ToolActionCompletion, ToolActionRequest, ToolActionRouteSet, ToolOperation,
    ToolOperationPlugin, ToolStoreAction, ToolStoreTarget,
};

pub(crate) struct HomebrewToolPlugin;

impl Plugin for HomebrewToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolOperationPlugin::<ImportBrewfile>::default())
            .add_systems(Update, route.in_set(ToolActionRouteSet))
            .add_systems(Update, complete);
    }
}

fn route(
    requests: Query<
        (Entity, &ToolActionRequest),
        (Added<ToolActionRequest>, With<ToolStoreTarget>),
    >,
    mut commands: Commands,
) {
    for (entity, action) in &requests {
        let request = action.request();
        if !matches!(
            request.provider,
            ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask
        ) || request.action != ToolAction::Import
        {
            continue;
        }
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreAction, ImportBrewfile::new(path)));
    }
}

fn complete(
    actions: Query<
        (Entity, &ImportedBrewfile),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
                "imported {} formulae and {} casks",
                output.formulae, output.casks
            )));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ImportBrewfile {
    path: PathBuf,
}

impl ImportBrewfile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportedBrewfile {
    pub formulae: usize,
    pub casks: usize,
}

impl ToolOperation for ImportBrewfile {
    type Output = ImportedBrewfile;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let (formulae, casks) = store.import_brewfile(&self.path)?;
        Ok(ImportedBrewfile { formulae, casks })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrewfileImport {
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
}

pub fn brewfile_path() -> PathBuf {
    ToolStore::current().brewfile_path()
}

pub fn import_brewfile(path: &Path) -> Result<(usize, usize), String> {
    ToolStore::current().import_brewfile(path)
}

pub fn import_brewfile_to(path: &Path, manifest_path: &Path) -> Result<(usize, usize), String> {
    let path = expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let imported = parse_brewfile(&source);
    if imported.formulae.is_empty() && imported.casks.is_empty() {
        return Err(format!("no formulae or casks found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let formulae = add_packages(&mut manifest, "homebrew-formula", &imported.formulae);
    let casks = add_packages(&mut manifest, "homebrew-cask", &imported.casks);
    write_manifest_to(manifest_path, &manifest)?;
    Ok((formulae, casks))
}

pub fn parse_brewfile(source: &str) -> BrewfileImport {
    let mut import = BrewfileImport::default();
    for line in source.lines() {
        if let Some(name) = parse_quoted_call(line, "brew") {
            import.formulae.push(name);
        } else if let Some(name) = parse_quoted_call(line, "cask") {
            import.casks.push(name);
        }
    }
    normalize_names(&mut import.formulae);
    normalize_names(&mut import.casks);
    import
}

pub(crate) fn sync_manifest_from_brewfile(
    manifest: &mut ToolsManifest,
    path: &Path,
) -> Result<(), String> {
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let imported = parse_brewfile(&source);
    set_packages(manifest, "homebrew-formula", imported.formulae);
    set_packages(manifest, "homebrew-cask", imported.casks);
    Ok(())
}

fn set_packages(manifest: &mut ToolsManifest, provider: &str, packages: Vec<String>) {
    if packages.is_empty() {
        manifest.packages.remove(provider);
    } else {
        manifest.packages.insert(provider.to_string(), packages);
    }
    manifest.normalize();
}

impl ToolStore {
    pub fn import_brewfile(&self, path: &Path) -> Result<(usize, usize), String> {
        self.migrate_legacy_storage()?;
        let source_path = self.expand_user_path(path)?;
        let source = std::fs::read_to_string(&source_path).map_err(|error| error.to_string())?;
        let imported = import_brewfile_to(&source_path, &self.manifest_path())?;
        vmux_path::AtomicFile::write(self.brewfile_path(), source.as_bytes())
            .map_err(|error| error.to_string())?;
        let manifest = ToolsManifest::read(&self.manifest_path())?;
        self.write_managed_brewfile(&manifest)?;
        Ok(imported)
    }

    pub(crate) fn sync_manifest_from_brewfile(
        &self,
        manifest: &mut ToolsManifest,
        path: &Path,
    ) -> Result<(), String> {
        sync_manifest_from_brewfile(manifest, path)
    }

    pub(crate) fn write_managed_brewfile(&self, manifest: &ToolsManifest) -> Result<(), String> {
        write_brewfile_to(&self.brewfile_path(), manifest)
    }
}

pub(crate) fn write_brewfile_to(path: &Path, manifest: &ToolsManifest) -> Result<(), String> {
    let formulae = manifest
        .packages
        .get("homebrew-formula")
        .cloned()
        .unwrap_or_default();
    let casks = manifest
        .packages
        .get("homebrew-cask")
        .cloned()
        .unwrap_or_default();
    if formulae.is_empty() && casks.is_empty() && !path.exists() {
        return Ok(());
    }
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let source = merge_brewfile(&existing, &formulae, &casks);
    vmux_path::AtomicFile::write(path, source.as_bytes()).map_err(|error| error.to_string())
}

fn merge_brewfile(source: &str, formulae: &[String], casks: &[String]) -> String {
    let desired_formulae = formulae.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let desired_casks = casks.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut seen_formulae = BTreeSet::new();
    let mut seen_casks = BTreeSet::new();
    let mut lines = Vec::new();
    for line in source.lines() {
        if let Some(name) = parse_quoted_call(line, "brew") {
            if desired_formulae.contains(name.as_str()) {
                seen_formulae.insert(name);
                lines.push(line.to_string());
            }
        } else if let Some(name) = parse_quoted_call(line, "cask") {
            if desired_casks.contains(name.as_str()) {
                seen_casks.insert(name);
                lines.push(line.to_string());
            }
        } else {
            lines.push(line.to_string());
        }
    }
    for package in formulae {
        if !seen_formulae.contains(package) {
            lines.push(format!("brew {:?}", package));
        }
    }
    for package in casks {
        if !seen_casks.contains(package) {
            lines.push(format!("cask {:?}", package));
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn parse_quoted_call(line: &str, call: &str) -> Option<String> {
    let line = line.trim_start();
    let rest = line.strip_prefix(call)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let quote = rest.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let mut escaped = false;
    let mut name = String::new();
    for character in rest[quote.len_utf8()..].chars() {
        if escaped {
            name.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return (!name.is_empty()).then_some(name);
        } else {
            name.push(character);
        }
    }
    None
}
