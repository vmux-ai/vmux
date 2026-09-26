use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::IoTaskPool;
use vmux_core::tool::{ToolImportRequest, ToolProvider};

use crate::manifest::{ToolStore, ToolsManifest, normalize_names};
use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteFlush,
    ToolOperationRouteSet, ToolOperationSucceeded, ToolOperationTask, ToolStoreOperation,
    ToolStoreTarget, finish_tool_operation,
};

pub(crate) struct HomebrewToolPlugin;

impl Plugin for HomebrewToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(
                Update,
                (
                    import_brewfile_system,
                    finish_tool_operation::<ImportedBrewfile>,
                )
                    .after(ToolOperationRouteFlush),
            )
            .add_systems(Update, complete);
    }
}

fn route(
    requests: Query<(Entity, &ToolOperationRequest<ToolImportRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if !matches!(
            request.provider,
            ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask
        ) {
            continue;
        }
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreOperation, ImportBrewfile::new(path)));
    }
}

fn complete(
    operations: Query<
        (Entity, &ImportedBrewfile),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!(
                "imported {} formulae and {} casks",
                output.formulae, output.casks
            )),
        ));
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

fn import_brewfile_system(
    operations: Query<
        (Entity, &ImportBrewfile, &ToolStoreTarget),
        (
            Without<ToolOperationTask<ImportedBrewfile>>,
            Without<ImportedBrewfile>,
            Without<ToolOperationFailed>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.0).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let path = operation.path.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                let (formulae, casks) = import_brewfile_in(&store, &path)?;
                Ok(ImportedBrewfile { formulae, casks })
            })));
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
    import_brewfile_in(&ToolStore::current(), path)
}

pub fn import_brewfile_to(path: &Path, manifest_path: &Path) -> Result<(usize, usize), String> {
    let path = ToolStore::current().expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let imported = parse_brewfile(&source);
    if imported.formulae.is_empty() && imported.casks.is_empty() {
        return Err(format!("no formulae or casks found in {}", path.display()));
    }
    let mut manifest = ToolsManifest::read(manifest_path)?;
    let formulae = manifest.add_packages("homebrew-formula", &imported.formulae);
    let casks = manifest.add_packages("homebrew-cask", &imported.casks);
    manifest.write_to(manifest_path)?;
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

fn import_brewfile_in(store: &ToolStore, path: &Path) -> Result<(usize, usize), String> {
    store.migrate_legacy_storage()?;
    let source_path = store.expand_user_path(path)?;
    let source = std::fs::read_to_string(&source_path).map_err(|error| error.to_string())?;
    let imported = import_brewfile_to(&source_path, &store.manifest_path())?;
    vmux_path::AtomicFile::write(store.brewfile_path(), source.as_bytes())
        .map_err(|error| error.to_string())?;
    let manifest = ToolsManifest::read(&store.manifest_path())?;
    write_managed_brewfile(store, &manifest)?;
    Ok(imported)
}

pub(crate) fn write_managed_brewfile(
    store: &ToolStore,
    manifest: &ToolsManifest,
) -> Result<(), String> {
    write_brewfile_to(&store.brewfile_path(), manifest)
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
