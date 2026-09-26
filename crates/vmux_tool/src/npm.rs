use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::IoTaskPool;
use vmux_core::tool::{
    ToolImportRequest, ToolOperationKey, ToolOperationKind, ToolProvider, ToolStatus,
};

use crate::manifest::{ToolStore, ToolsManifest, normalize_names};
use crate::process::ToolProcess;
use crate::{
    ToolInventory, ToolInventoryItem, ToolOperationFailed, ToolOperationFinished,
    ToolOperationRequest, ToolOperationRouteFlush, ToolOperationRouteSet, ToolOperationSucceeded,
    ToolOperationTask, ToolOperator, ToolProviderId, ToolProviderSnapshot, ToolScanner,
    ToolStoreOperation, ToolStoreTarget, finish_tool_operation,
};

pub(crate) struct NpmToolPlugin;

impl Plugin for NpmToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_provider)
            .add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(
                Update,
                (
                    import_npm_manifest_system,
                    finish_tool_operation::<ImportedNpmManifest>,
                )
                    .after(ToolOperationRouteFlush),
            )
            .add_systems(Update, complete);
    }
}

fn spawn_provider(mut commands: Commands) {
    commands.spawn((
        Name::new("NPM tool provider"),
        ToolProviderId(ToolProvider::Npm),
        ToolScanner::new(scan),
        ToolOperator::new(operate),
    ));
}

fn scan(
    _store: &ToolStore,
    manifest: &mut ToolsManifest,
    refresh: bool,
) -> Result<ToolProviderSnapshot, String> {
    Ok(
        ToolInventory::new(ToolProvider::Npm, scan_inventory(refresh)?)
            .reconcile(manifest)
            .into(),
    )
}

fn scan_inventory(refresh: bool) -> Result<Vec<ToolInventoryItem>, String> {
    let Some(npm) = ToolProcess::find("npm") else {
        return Ok(Vec::new());
    };
    let output = npm.output(&["list", "--global", "--depth=0", "--json"], false)?;
    if output.stdout.is_empty() && !output.status.success() {
        return Err(output_error("npm", &output));
    }
    let outdated_output = refresh
        .then(|| npm.output(&["outdated", "--global", "--json"], false).ok())
        .flatten();
    let outdated = outdated_output
        .as_ref()
        .and_then(|output| serde_json::from_slice::<serde_json::Value>(&output.stdout).ok())
        .and_then(|value| {
            value
                .as_object()
                .map(|packages| packages.keys().cloned().collect())
        })
        .unwrap_or_default();
    parse_inventory(&output.stdout, &outdated)
}

fn parse_inventory(
    bytes: &[u8],
    outdated: &BTreeSet<String>,
) -> Result<Vec<ToolInventoryItem>, String> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let dependencies = document
        .get("dependencies")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    Ok(dependencies
        .into_iter()
        .map(|(name, metadata)| {
            let status = if outdated.contains(&name) {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            ToolInventoryItem {
                id: name.clone(),
                name,
                icon: None,
                version: metadata
                    .get("version")
                    .and_then(|version| version.as_str())
                    .map(str::to_string),
                detail: "Global NPM package".to_string(),
                status,
                removable: true,
            }
        })
        .collect())
}

fn operate(store: &ToolStore, operation: &ToolOperationKey, value: &str) -> Result<String, String> {
    let id = operation.item_id.trim();
    match operation.kind {
        ToolOperationKind::Install => {
            package_command("install", id)?;
            store.set_managed_package(ToolProvider::Npm, id, true)?;
            Ok(format!("{id} installed"))
        }
        ToolOperationKind::Update => {
            package_command("update", id)?;
            store.set_managed_package(ToolProvider::Npm, id, true)?;
            Ok(format!("{id} updated"))
        }
        ToolOperationKind::Uninstall => {
            let npm = ToolProcess::find("npm").ok_or_else(|| "npm is not installed".to_string())?;
            if id.is_empty() {
                return Err("package name is required".to_string());
            }
            npm.output(&["uninstall", "--global", id], true)?;
            store.set_managed_package(ToolProvider::Npm, id, false)?;
            Ok(format!("{id} removed"))
        }
        ToolOperationKind::Forget => {
            store.set_managed_package(ToolProvider::Npm, id, false)?;
            Ok(format!("{id} removed from tools.toml"))
        }
        ToolOperationKind::Adopt => {
            store.set_managed_package(ToolProvider::Npm, id, true)?;
            Ok(format!("{id} is now managed"))
        }
        ToolOperationKind::Import if value.trim().is_empty() => {
            let mut manifest = store.load()?;
            let before = manifest.managed_packages(ToolProvider::Npm.id()).len();
            let _ = scan(store, &mut manifest, false)?;
            let imported = manifest
                .managed_packages(ToolProvider::Npm.id())
                .len()
                .saturating_sub(before);
            store.save(&manifest)?;
            Ok(format!("imported {imported} npm item(s)"))
        }
        _ => Err(format!("NPM does not support {:?}", operation.kind)),
    }
}

fn package_command(operation: &str, id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("package name is required".to_string());
    }
    let npm = ToolProcess::find("npm").ok_or_else(|| "npm is not installed".to_string())?;
    npm.output(&[operation, "--global", id], true)?;
    Ok(())
}

fn output_error(program: &str, output: &std::process::Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        format!("{program} exited with {}", output.status)
    } else {
        detail
    }
}

fn route(
    requests: Query<(Entity, &ToolOperationRequest<ToolImportRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = &operation.0;
        if request.provider != ToolProvider::Npm {
            continue;
        }
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreOperation, ImportNpmManifest::new(path)));
    }
}

fn complete(
    operations: Query<
        (Entity, &ImportedNpmManifest),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("imported {} NPM package(s)", output.packages)),
        ));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ImportNpmManifest {
    path: PathBuf,
}

impl ImportNpmManifest {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportedNpmManifest {
    pub packages: usize,
}

fn import_npm_manifest_system(
    operations: Query<
        (Entity, &ImportNpmManifest, &ToolStoreTarget),
        (
            Without<ToolOperationTask<ImportedNpmManifest>>,
            Without<ImportedNpmManifest>,
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
                let packages = import_npm_manifest_in(&store, &path)?;
                Ok(ImportedNpmManifest { packages })
            })));
    }
}

pub fn import_npm_manifest(path: &Path) -> Result<usize, String> {
    import_npm_manifest_in(&ToolStore::current(), path)
}

fn import_npm_manifest_in(store: &ToolStore, path: &Path) -> Result<usize, String> {
    store.migrate_legacy_storage()?;
    let path = store.expand_user_path(path)?;
    import_npm_manifest_to(&path, &store.manifest_path())
}

pub fn import_npm_manifest_to(path: &Path, manifest_path: &Path) -> Result<usize, String> {
    let path = ToolStore::current().expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let packages = parse_npm_manifest(&source)?;
    if packages.is_empty() {
        return Err(format!("no dependencies found in {}", path.display()));
    }
    let mut manifest = ToolsManifest::read(manifest_path)?;
    let imported = manifest.add_packages("npm", &packages);
    manifest.write_to(manifest_path)?;
    Ok(imported)
}

pub fn parse_npm_manifest(source: &str) -> Result<Vec<String>, String> {
    let document: serde_json::Value =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    let mut packages = Vec::new();
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(entries) = document.get(field).and_then(serde_json::Value::as_object) {
            packages.extend(entries.keys().cloned());
        }
    }
    normalize_names(&mut packages);
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scoped_packages_and_outdated_state() {
        let inventory = parse_inventory(
            br#"{"dependencies":{"@scope/tool":{"version":"2.0.0"},"typescript":{"version":"5.9.0"}}}"#,
            &BTreeSet::from(["@scope/tool".to_string()]),
        )
        .unwrap();

        assert_eq!(inventory.len(), 2);
        let scoped = inventory
            .iter()
            .find(|item| item.id == "@scope/tool")
            .unwrap();
        assert_eq!(scoped.version.as_deref(), Some("2.0.0"));
        assert_eq!(scoped.status, ToolStatus::Outdated);
    }
}
