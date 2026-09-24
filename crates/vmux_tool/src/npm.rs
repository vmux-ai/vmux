use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::tool::{ToolImportRequest, ToolProvider};

use crate::manifest::{
    ToolStore, add_packages, expand_user_path, load_manifest_from, normalize_names,
    write_manifest_to,
};
use crate::{
    ToolOperation, ToolOperationCompletion, ToolOperationPlugin, ToolOperationRequest,
    ToolOperationRouteSet, ToolStoreOperation, ToolStoreTarget,
};

pub(crate) struct NpmToolPlugin;

impl Plugin for NpmToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolOperationPlugin::<ImportNpmManifest>::default())
            .add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(Update, complete);
    }
}

fn route(
    requests: Query<(Entity, &ToolOperationRequest<ToolImportRequest>), Added<ToolStoreTarget>>,
    mut commands: Commands,
) {
    for (entity, operation) in &requests {
        let request = operation.request();
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
        (With<ToolStoreOperation>, Without<ToolOperationCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands
            .entity(entity)
            .insert(ToolOperationCompletion::succeeded(format!(
                "imported {} NPM package(s)",
                output.packages
            )));
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

impl ToolOperation for ImportNpmManifest {
    type Output = ImportedNpmManifest;

    fn execute(&self, store: &ToolStore) -> Result<Self::Output, String> {
        let packages = store.import_npm_manifest(&self.path)?;
        Ok(ImportedNpmManifest { packages })
    }
}

pub fn import_npm_manifest(path: &Path) -> Result<usize, String> {
    ToolStore::current().import_npm_manifest(path)
}

impl ToolStore {
    pub fn import_npm_manifest(&self, path: &Path) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        let path = self.expand_user_path(path)?;
        import_npm_manifest_to(&path, &self.manifest_path())
    }
}

pub fn import_npm_manifest_to(path: &Path, manifest_path: &Path) -> Result<usize, String> {
    let path = expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let packages = parse_npm_manifest(&source)?;
    if packages.is_empty() {
        return Err(format!("no dependencies found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let imported = add_packages(&mut manifest, "npm", &packages);
    write_manifest_to(manifest_path, &manifest)?;
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
