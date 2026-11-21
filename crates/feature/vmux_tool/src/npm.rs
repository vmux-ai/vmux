use std::path::PathBuf;

use crate::state::ToolImportRequest;
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;

use vmux_ecs::OperationState;

use crate::manifest::{ToolStore, ToolsManifest};
use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteSet,
    ToolOperationSucceeded, ToolProviderBinding, ToolProviderTarget, ToolStoreOperation,
    ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct NpmToolPlugin;

impl Plugin for NpmToolPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(host)]
        app.add_plugins(crate::npm_inventory::NpmInventoryPlugin);
        app.add_systems(Startup, spawn)
            .add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(
                Update,
                (import, read, commit, save)
                    .chain()
                    .in_set(ToolTaskSet::Start),
            )
            .add_systems(Update, drain.after(ToolTaskSet::Apply))
            .add_systems(Update, finish.after(ToolTaskSet::Apply));
    }
}

#[derive(Component)]
pub(crate) struct NpmProvider;

fn spawn(mut commands: Commands) {
    let provider = commands
        .spawn((ToolProviderBinding::new::<crate::Feature>(2), NpmProvider))
        .id();
    #[cfg(host)]
    commands.entity(provider).insert((
        crate::observation::Handler,
        crate::observation::PackageProvider,
    ));
    #[cfg(not(host))]
    let _ = provider;
}

fn route(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolImportRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<NpmProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreOperation, ImportNpmManifest::new(path)));
    }
}

fn finish(
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
pub(super) struct ImportedNpmManifest {
    pub(super) packages: usize,
}

impl ImportedNpmManifest {
    #[cfg(test)]
    pub(super) fn packages(&self) -> usize {
        self.packages
    }
}

#[derive(Component)]
struct NpmPlan {
    packages: usize,
}

#[derive(Component)]
struct Importing;

fn import(
    operations: Query<
        (
            Entity,
            &ImportNpmManifest,
            &vmux_ecs::EntityTarget<ToolStore>,
            Option<&OperationState>,
        ),
        (Without<Importing>, Without<ToolOperationFinished>),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, _, target, state) in &operations {
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        let Ok(store) = stores.get(target.entity()) else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".into()),
            ));
            continue;
        };
        commands.entity(entity).insert((
            Importing,
            store.clone(),
            vmux_ecs::OperationRetained,
            crate::storage::ReadRequest { retain_lease: true },
        ));
    }
}

fn read(
    operations: Query<
        (
            Entity,
            &ImportNpmManifest,
            &ToolStore,
            &crate::storage::ReadResult,
        ),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, input, store, result) in &operations {
        commands
            .entity(entity)
            .remove::<crate::storage::ReadResult>();
        let manifest = match &result.0 {
            Ok(manifest) => manifest.clone(),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
                continue;
            }
        };
        let path = match store.expand_user_path(&input.path) {
            Ok(path) => path,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error)));
                continue;
            }
        };
        commands
            .entity(entity)
            .insert((manifest, crate::storage::FileReadRequest(path)));
    }
}

fn commit(
    mut operations: Query<
        (
            Entity,
            &ImportNpmManifest,
            &crate::storage::FileReadResult,
            &mut ToolsManifest,
        ),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, input, source, mut manifest) in &mut operations {
        commands
            .entity(entity)
            .remove::<crate::storage::FileReadResult>();
        let source = match &source.0 {
            Ok(source) => source,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
                continue;
            }
        };
        let document = match NpmManifest::parse(source) {
            Ok(document) => document,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error)));
                continue;
            }
        };
        if document.packages.is_empty() {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed(format!("no dependencies found in {}", input.path.display())),
            ));
            continue;
        }
        let packages = manifest.add_packages("npm", &document.packages);
        commands.entity(entity).insert((
            NpmPlan { packages },
            crate::storage::WriteRequest {
                manifest: manifest.clone(),
                retain_lease: true,
            },
        ));
    }
}

fn save(
    operations: Query<
        (Entity, &NpmPlan, &crate::storage::WriteResult),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, plan, result) in &operations {
        commands
            .entity(entity)
            .remove::<crate::storage::WriteResult>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(ImportedNpmManifest {
                    packages: plan.packages,
                });
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
            }
        }
        commands.entity(entity).remove::<(
            Importing,
            NpmPlan,
            ToolStore,
            ToolsManifest,
            crate::storage::StoreLease,
            vmux_ecs::OperationRetained,
        )>();
    }
}

fn drain(
    operations: Query<
        Entity,
        (
            With<Importing>,
            With<ToolOperationFinished>,
            Without<crate::storage::ReadRequest>,
            Without<crate::storage::WriteRequest>,
            Without<crate::storage::FileReadRequest>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &operations {
        commands.entity(entity).remove::<(
            Importing,
            NpmPlan,
            ToolStore,
            ToolsManifest,
            crate::storage::StoreLease,
            vmux_ecs::OperationRetained,
            crate::storage::ReadResult,
            crate::storage::WriteResult,
            crate::storage::FileReadResult,
        )>();
    }
}

pub struct NpmManifest {
    pub packages: Vec<String>,
}

impl NpmManifest {
    pub fn parse(source: &str) -> Result<Self, String> {
        let document: serde_json::Value =
            serde_json::from_str(source).map_err(|error| error.to_string())?;
        let mut packages = Vec::new();
        for field in ["dependencies", "devDependencies", "optionalDependencies"] {
            if let Some(entries) = document.get(field).and_then(serde_json::Value::as_object) {
                packages.extend(entries.keys().cloned());
            }
        }
        ToolsManifest::normalize_names(&mut packages);
        Ok(Self { packages })
    }
}
