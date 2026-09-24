use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::tool::{ToolAction, ToolProvider};

use crate::manifest::{
    ToolStore, add_packages, expand_user_path, load_manifest_from, normalize_names,
    write_manifest_to,
};
use crate::{
    ToolActionCompletion, ToolActionRequest, ToolActionRouteSet, ToolOperation,
    ToolOperationPlugin, ToolStoreAction, ToolStoreTarget,
};

pub(crate) struct NpmToolPlugin;

impl Plugin for NpmToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolOperationPlugin::<ImportNpmManifest>::default())
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
        if request.provider != ToolProvider::Npm || request.action != ToolAction::Import {
            continue;
        }
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreAction, ImportNpmManifest::new(path)));
    }
}

fn complete(
    actions: Query<
        (Entity, &ImportedNpmManifest),
        (With<ToolStoreAction>, Without<ToolActionCompletion>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &actions {
        commands
            .entity(entity)
            .insert(ToolActionCompletion::succeeded(format!(
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
