use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::migration::{MigrationRequest, MigrationResult};
use super::native::{Brewfile, BrewfileWritten, ManifestWritten, Paths};
use super::{
    AcquiredLease, Busy, Failed, Failure, IoTask, RetainedHere, StoreLease, ToolStore,
    ToolsManifest, Writing,
};

pub(super) struct WritePlugin;

impl Plugin for WritePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                cancel, begin, lease, migrate, manifest, brewfile, write, finish,
            )
                .chain()
                .in_set(WriteSet),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct WriteSet;

#[derive(Component)]
pub(crate) struct WriteRequest {
    pub(crate) manifest: ToolsManifest,
    pub(crate) retain_lease: bool,
}

#[derive(Component)]
pub(crate) struct ManifestOnly;

#[derive(Component, Clone, Default)]
pub(crate) struct ManifestReceipt(Arc<AtomicBool>);

impl ManifestReceipt {
    pub(crate) fn published(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(super) fn record(&self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Component)]
pub(crate) struct WriteResult(pub(crate) Result<(), String>);

#[derive(Component)]
pub(crate) struct BrewfileTemplate(pub(crate) String);

#[derive(Component)]
struct Active;
#[derive(Component)]
struct Acquire;
#[derive(Component)]
struct Migrate;
#[derive(Component)]
struct WriteManifest;
#[derive(Component)]
struct ReadBrewfile;
#[derive(Component)]
struct WriteBrewfile;
#[derive(Component)]
struct Done;

fn cancel(
    requests: Query<
        (Entity, &vmux_ecs::OperationState, Has<Active>),
        (With<WriteRequest>, Without<Busy>, Without<MigrationRequest>),
    >,
    mut commands: Commands,
) {
    for (entity, state, active) in &requests {
        if matches!(
            state,
            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
        ) {
            continue;
        }
        let mut request = commands.entity(entity);
        if active {
            request
                .remove::<(Acquire, Migrate, WriteManifest, ReadBrewfile, WriteBrewfile)>()
                .insert((Failure("tool storage write canceled".into()), Done));
        } else {
            request
                .remove::<WriteRequest>()
                .insert(WriteResult(Err("tool storage write canceled".into())));
        }
    }
}

fn begin(
    requests: Query<
        (Entity, &ToolStore, Has<vmux_ecs::OperationRetained>),
        (
            With<WriteRequest>,
            Without<Active>,
            Without<super::ReadRequest>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, store, retained) in &requests {
        if !retained {
            commands
                .entity(entity)
                .insert((vmux_ecs::OperationRetained, RetainedHere));
        }
        commands
            .entity(entity)
            .remove::<(WriteResult, Failure)>()
            .insert((
                Active,
                Writing,
                Acquire,
                Paths::from_store(store),
                ManifestReceipt::default(),
            ));
    }
}

fn lease(
    requests: Query<
        (
            Entity,
            Option<&ToolStore>,
            Option<&StoreLease>,
            Option<&Failed<StoreLease>>,
        ),
        (With<Active>, With<Acquire>, Without<Busy>),
    >,
    mut commands: Commands,
) {
    for (entity, store, guard, failure) in &requests {
        let Some(store) = store else {
            commands
                .entity(entity)
                .remove::<Acquire>()
                .insert((Failure("tool store is unavailable".into()), Done));
            continue;
        };
        if let Some(error) = failure {
            commands
                .entity(entity)
                .remove::<Acquire>()
                .insert((Failure(error.0.clone()), Done));
        } else if guard.is_some() {
            commands
                .entity(entity)
                .remove::<Acquire>()
                .insert((Migrate, MigrationRequest));
        } else {
            let store = store.clone();
            commands.entity(entity).insert((
                AcquiredLease,
                Busy,
                IoTask(IoTaskPool::get().spawn(async move {
                    Ok(StoreLease {
                        _lease: store.lease_async().await,
                    })
                })),
            ));
        }
    }
}

fn migrate(
    requests: Query<(Entity, &MigrationResult), (With<Active>, With<Migrate>, Without<Busy>)>,
    mut commands: Commands,
) {
    for (entity, result) in &requests {
        commands
            .entity(entity)
            .remove::<(Migrate, MigrationResult)>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(WriteManifest);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((Failure(error.clone()), Done));
            }
        }
    }
}

fn manifest(
    requests: Query<
        (
            Entity,
            &Paths,
            &WriteRequest,
            &ManifestReceipt,
            Has<ManifestOnly>,
            Option<&ManifestWritten>,
            Option<&Failed<ManifestWritten>>,
        ),
        (With<Active>, With<WriteManifest>, Without<Busy>),
    >,
    mut commands: Commands,
) {
    for (entity, paths, request, receipt, manifest_only, written, failure) in &requests {
        if let Some(failure) = failure {
            commands
                .entity(entity)
                .remove::<WriteManifest>()
                .insert((Failure(failure.0.clone()), Done));
        } else if written.is_some() {
            let mut entity = commands.entity(entity);
            entity.remove::<WriteManifest>();
            if manifest_only {
                entity.insert(Done);
            } else {
                entity.insert(ReadBrewfile);
            }
        } else {
            let mut manifest = request.manifest.clone();
            manifest.version = ToolsManifest::default().version;
            manifest.normalize();
            let source = match toml::to_string_pretty(&manifest) {
                Ok(source) => source,
                Err(error) => {
                    commands
                        .entity(entity)
                        .remove::<WriteManifest>()
                        .insert((Failure(error.to_string()), Done));
                    continue;
                }
            };
            let paths = paths.clone();
            let receipt = receipt.clone();
            commands.entity(entity).insert((
                Busy,
                IoTask(
                    IoTaskPool::get().spawn(async move { paths.write_manifest(source, receipt) }),
                ),
            ));
        }
    }
}

fn brewfile(
    requests: Query<
        (Entity, &Paths),
        (
            With<Active>,
            With<ReadBrewfile>,
            Without<Busy>,
            Without<Brewfile>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, paths) in &requests {
        let paths = paths.clone();
        commands.entity(entity).insert((
            Busy,
            IoTask(IoTaskPool::get().spawn(async move { paths.read_brewfile() })),
        ));
    }
}

fn write(
    requests: Query<
        (
            Entity,
            &Paths,
            &WriteRequest,
            &Brewfile,
            Option<&BrewfileWritten>,
            Option<&Failed<BrewfileWritten>>,
            Option<&BrewfileTemplate>,
        ),
        (
            With<Active>,
            Without<Busy>,
            Or<(With<ReadBrewfile>, With<WriteBrewfile>)>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, paths, request, brewfile, written, failure, template) in &requests {
        commands.entity(entity).remove::<ReadBrewfile>();
        if let Some(failure) = failure {
            commands
                .entity(entity)
                .remove::<WriteBrewfile>()
                .insert((Failure(failure.0.clone()), Done));
            continue;
        }
        let formulae = request
            .manifest
            .managed_packages("homebrew-formula")
            .into_iter()
            .collect::<Vec<_>>();
        let casks = request
            .manifest
            .managed_packages("homebrew-cask")
            .into_iter()
            .collect::<Vec<_>>();
        if written.is_some() || formulae.is_empty() && casks.is_empty() && !brewfile.exists {
            commands
                .entity(entity)
                .remove::<WriteBrewfile>()
                .insert(Done);
        } else {
            let source = if let Some(template) = template {
                Brewfile {
                    exists: brewfile.exists,
                    is_file: brewfile.is_file,
                    source: Ok(template.0.clone()),
                }
                .merge(&formulae, &casks)
            } else {
                brewfile.merge(&formulae, &casks)
            };
            let paths = paths.clone();
            commands.entity(entity).insert((
                WriteBrewfile,
                Busy,
                IoTask(IoTaskPool::get().spawn(async move { paths.write_brewfile(source) })),
            ));
        }
    }
}

fn finish(
    requests: Query<
        (Entity, &WriteRequest, Option<&Failure>, Has<AcquiredLease>),
        (With<Active>, With<Done>, Without<Busy>),
    >,
    mut commands: Commands,
    retained: Query<(), With<RetainedHere>>,
) {
    for (entity, request, failure, acquired) in &requests {
        let result = failure
            .map(|failure| Err(failure.0.clone()))
            .unwrap_or(Ok(()));
        if acquired && !request.retain_lease {
            commands.entity(entity).remove::<StoreLease>();
        }
        if retained.contains(entity) {
            commands
                .entity(entity)
                .remove::<(RetainedHere, vmux_ecs::OperationRetained)>();
        }
        commands
            .entity(entity)
            .insert(WriteResult(result))
            .remove::<(
                Active,
                Writing,
                WriteRequest,
                Done,
                Paths,
                Brewfile,
                AcquiredLease,
                Failure,
                ManifestOnly,
                BrewfileTemplate,
                MigrationResult,
            )>();
        commands.entity(entity).remove::<(
            BrewfileWritten,
            ManifestWritten,
            Failed<BrewfileWritten>,
            Failed<ManifestWritten>,
            Failed<StoreLease>,
        )>();
    }
}
