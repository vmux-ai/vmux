use bevy::prelude::*;
use vmux_ecs::{IoTask, TaskError};

use super::migration::{MigrationRequest, MigrationResult};
use super::native::{Brewfile, BrewfileWritten, ManifestBytes, Paths};
use super::{
    AcquiredLease, Failure, Idle, Reading, RetainedHere, StoreLease, ToolStore, ToolsManifest,
};

pub(super) struct ReadPlugin;

impl Plugin for ReadPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                cancel, begin, lease, migrate, read, decode, brewfile, reconcile, write, finish,
            )
                .chain()
                .in_set(ReadSet),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ReadSet;

#[derive(Component)]
pub(crate) struct ReadRequest {
    pub(crate) retain_lease: bool,
}

#[derive(Component)]
pub(crate) struct ReadResult(pub(crate) Result<ToolsManifest, String>);

#[derive(Component)]
struct Active;
#[derive(Component)]
struct Acquire;
#[derive(Component)]
struct Migrate;
#[derive(Component)]
struct Read;
#[derive(Component)]
struct ReadBrewfile;
#[derive(Component)]
struct WriteBrewfile;
#[derive(Component)]
struct Done;
#[derive(Component)]
struct Manifest(ToolsManifest);

fn cancel(
    requests: Query<
        (Entity, &vmux_ecs::OperationState, Has<Active>),
        (With<ReadRequest>, Idle, Without<MigrationRequest>),
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
                .remove::<(Acquire, Migrate, Read, ReadBrewfile, WriteBrewfile)>()
                .insert((Failure("tool storage read canceled".into()), Done));
        } else {
            request
                .remove::<ReadRequest>()
                .insert(ReadResult(Err("tool storage read canceled".into())));
        }
    }
}

fn begin(
    requests: Query<
        (Entity, &ToolStore, Has<vmux_ecs::OperationRetained>),
        (
            With<ReadRequest>,
            Without<Active>,
            Without<super::WriteRequest>,
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
            .remove::<(ReadResult, Failure)>()
            .insert((Active, Reading, Acquire, Paths::from_store(store)));
    }
}

fn lease(
    requests: Query<
        (
            Entity,
            Option<&ToolStore>,
            Option<&StoreLease>,
            Option<&TaskError<StoreLease>>,
        ),
        (With<Active>, With<Acquire>, Idle),
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
                .insert((Failure(error.to_string()), Done));
        } else if guard.is_some() {
            commands
                .entity(entity)
                .remove::<Acquire>()
                .insert((Migrate, MigrationRequest));
        } else {
            let store = store.clone();
            commands.entity(entity).insert((
                AcquiredLease,
                IoTask::spawn(async move {
                    Ok::<_, String>(StoreLease {
                        _lease: store.lease_async().await,
                    })
                }),
            ));
        }
    }
}

fn migrate(
    requests: Query<(Entity, &MigrationResult), (With<Active>, With<Migrate>, Idle)>,
    mut commands: Commands,
) {
    for (entity, result) in &requests {
        commands
            .entity(entity)
            .remove::<(Migrate, MigrationResult)>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(Read);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((Failure(error.clone()), Done));
            }
        }
    }
}

fn read(
    requests: Query<
        (Entity, &Paths),
        (
            With<Active>,
            With<Read>,
            Idle,
            Without<ManifestBytes>,
            Without<TaskError<ManifestBytes>>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, paths) in &requests {
        let paths = paths.clone();
        commands
            .entity(entity)
            .insert(IoTask::spawn(async move { paths.read_manifest() }));
    }
}

fn decode(
    requests: Query<
        (
            Entity,
            Option<&ManifestBytes>,
            Option<&TaskError<ManifestBytes>>,
        ),
        (With<Active>, With<Read>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, bytes, failure) in &requests {
        let result = match (bytes, failure) {
            (_, Some(failure)) => Err(failure.to_string()),
            (Some(bytes), _) => match &bytes.0 {
                Some(bytes) => {
                    toml::from_str::<ToolsManifest>(bytes).map_err(|error| error.to_string())
                }
                None => Ok(ToolsManifest::default()),
            },
            _ => continue,
        };
        commands
            .entity(entity)
            .remove::<(Read, ManifestBytes, TaskError<ManifestBytes>)>();
        match result {
            Ok(mut manifest) => {
                if manifest.version != ToolsManifest::default().version {
                    commands.entity(entity).insert((
                        Failure(format!(
                            "unsupported tools manifest version: {}",
                            manifest.version
                        )),
                        Done,
                    ));
                    continue;
                }
                manifest.normalize();
                commands
                    .entity(entity)
                    .insert((Manifest(manifest), ReadBrewfile));
            }
            Err(error) => {
                commands.entity(entity).insert((Failure(error), Done));
            }
        }
    }
}

fn brewfile(
    requests: Query<(Entity, &Paths), (With<Active>, With<ReadBrewfile>, Idle, Without<Brewfile>)>,
    mut commands: Commands,
) {
    for (entity, paths) in &requests {
        let paths = paths.clone();
        commands
            .entity(entity)
            .insert(IoTask::spawn(async move { paths.read_brewfile() }));
    }
}

fn reconcile(
    mut requests: Query<
        (Entity, &Brewfile, &mut Manifest),
        (With<Active>, With<ReadBrewfile>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, brewfile, mut manifest) in &mut requests {
        commands.entity(entity).remove::<ReadBrewfile>();
        if brewfile.is_file {
            let source = match &brewfile.source {
                Ok(source) => source,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((Failure(error.clone()), Done));
                    continue;
                }
            };
            let imported = crate::BrewfileImport::parse(source);
            for (provider, packages) in [
                ("homebrew-formula", imported.formulae),
                ("homebrew-cask", imported.casks),
            ] {
                if packages.is_empty() {
                    manifest.0.packages.remove(provider);
                } else {
                    manifest.0.packages.insert(provider.to_string(), packages);
                }
            }
            manifest.0.normalize();
            commands.entity(entity).insert(Done);
        } else {
            commands.entity(entity).insert(WriteBrewfile);
        }
    }
}

fn write(
    requests: Query<
        (
            Entity,
            &Paths,
            &Manifest,
            &Brewfile,
            Option<&BrewfileWritten>,
            Option<&TaskError<BrewfileWritten>>,
        ),
        (With<Active>, With<WriteBrewfile>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, paths, manifest, brewfile, written, failure) in &requests {
        if let Some(failure) = failure {
            commands
                .entity(entity)
                .insert((Failure(failure.to_string()), Done))
                .remove::<WriteBrewfile>();
            continue;
        }
        let formulae = manifest
            .0
            .managed_packages("homebrew-formula")
            .into_iter()
            .collect::<Vec<_>>();
        let casks = manifest
            .0
            .managed_packages("homebrew-cask")
            .into_iter()
            .collect::<Vec<_>>();
        if written.is_some() || formulae.is_empty() && casks.is_empty() && !brewfile.exists {
            commands
                .entity(entity)
                .insert(Done)
                .remove::<WriteBrewfile>();
        } else {
            let source = brewfile.merge(&formulae, &casks);
            let paths = paths.clone();
            commands
                .entity(entity)
                .insert(IoTask::spawn(async move { paths.write_brewfile(source) }));
        }
    }
}

fn finish(
    requests: Query<
        (
            Entity,
            &ReadRequest,
            Option<&Manifest>,
            Option<&Failure>,
            Has<AcquiredLease>,
        ),
        (With<Active>, With<Done>, Idle),
    >,
    mut commands: Commands,
    retained: Query<(), With<RetainedHere>>,
) {
    for (entity, request, manifest, failure, acquired) in &requests {
        let result = match failure {
            Some(failure) => Err(failure.0.clone()),
            None => Ok(manifest.unwrap().0.clone()),
        };
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
            .insert(ReadResult(result))
            .remove::<(
                Active,
                Reading,
                ReadRequest,
                Done,
                Manifest,
                Paths,
                Brewfile,
                AcquiredLease,
                MigrationResult,
                Failure,
            )>();
        commands.entity(entity).remove::<(
            BrewfileWritten,
            TaskError<BrewfileWritten>,
            TaskError<StoreLease>,
        )>();
    }
}
