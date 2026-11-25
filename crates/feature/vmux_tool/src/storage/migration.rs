use bevy::prelude::*;
use vmux_ecs::{IoTask, TaskError};

use super::native::{MovePaths, Moved, Paths, Probe};
use super::{Idle, StoreLease};

pub(super) struct MigrationPlugin;

impl Plugin for MigrationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (cancel, begin, probe, decide, rename, advance)
                .chain()
                .after(crate::ToolTaskSet::Start)
                .before(super::read::ReadSet)
                .before(super::write::WriteSet),
        );
    }
}

#[derive(Component)]
pub(super) struct MigrationRequest;

#[derive(Component)]
pub(super) struct MigrationResult(pub(super) Result<(), String>);

#[derive(Component)]
struct Active;

#[derive(Component)]
struct Directory;

#[derive(Component)]
struct Inspect;

#[derive(Component)]
struct Rename;

#[derive(Component)]
struct Done;

fn cancel(
    requests: Query<(Entity, &vmux_ecs::OperationState), (With<MigrationRequest>, Idle)>,
    mut commands: Commands,
) {
    for (entity, state) in &requests {
        if matches!(
            state,
            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
        ) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(
                Active,
                MigrationRequest,
                Inspect,
                Rename,
                MovePaths,
                Probe,
                Moved,
                Directory,
                Done,
                TaskError<Moved>,
                TaskError<Probe>,
            )>()
            .insert(MigrationResult(Err(
                "tool storage migration canceled".into()
            )));
    }
}

fn begin(
    requests: Query<(Entity, &Paths), (With<MigrationRequest>, With<StoreLease>, Without<Active>)>,
    mut commands: Commands,
) {
    for (entity, paths) in &requests {
        let source = paths.root.parent().unwrap().join("registry");
        commands.entity(entity).remove::<MigrationResult>().insert((
            Active,
            Directory,
            Inspect,
            MovePaths {
                source,
                destination: paths.root.clone(),
            },
        ));
    }
}

fn probe(
    requests: Query<(Entity, &MovePaths), (With<Active>, With<Inspect>, Without<Probe>, Idle)>,
    mut commands: Commands,
) {
    for (entity, paths) in &requests {
        let paths = paths.clone();
        commands
            .entity(entity)
            .insert(IoTask::spawn(async move { paths.probe() }));
    }
}

fn decide(
    requests: Query<
        (Entity, &MovePaths, &Probe, Option<&TaskError<Moved>>),
        (With<Active>, With<Inspect>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, paths, probe, failure) in &requests {
        if let Some(failure) = failure {
            if !probe.source && probe.destination {
                commands.entity(entity).insert(Done);
            } else {
                commands
                    .entity(entity)
                    .insert(MigrationResult(Err(failure.to_string())))
                    .remove::<(
                        Active,
                        MigrationRequest,
                        Inspect,
                        MovePaths,
                        Probe,
                        Directory,
                        TaskError<Moved>,
                    )>();
            }
        } else if probe.source && probe.destination {
            commands
                .entity(entity)
                .insert(MigrationResult(Err(format!(
                    "cannot migrate {} because {} already exists",
                    paths.source.display(),
                    paths.destination.display()
                ))))
                .remove::<(
                    Active,
                    MigrationRequest,
                    Inspect,
                    MovePaths,
                    Probe,
                    Directory,
                )>();
        } else if probe.source {
            commands
                .entity(entity)
                .remove::<(Inspect, Probe)>()
                .insert(Rename);
        } else {
            commands.entity(entity).insert(Done);
        }
    }
}

fn rename(
    requests: Query<
        (
            Entity,
            &MovePaths,
            Option<&Moved>,
            Option<&TaskError<Moved>>,
        ),
        (With<Active>, With<Rename>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, paths, moved, error) in &requests {
        if moved.is_some() {
            commands.entity(entity).insert(Done).remove::<Rename>();
        } else if error.is_some() {
            commands
                .entity(entity)
                .insert(Inspect)
                .remove::<(Rename, Probe)>();
        } else {
            let paths = paths.clone();
            commands
                .entity(entity)
                .insert(IoTask::spawn(async move { paths.rename() }));
        }
    }
}

fn advance(
    requests: Query<(Entity, &Paths, Has<Directory>), (With<Active>, With<Done>, Idle)>,
    mut commands: Commands,
) {
    for (entity, paths, directory) in &requests {
        commands.entity(entity).remove::<(
            Done,
            Moved,
            Probe,
            TaskError<Moved>,
            Inspect,
            Rename,
            Directory,
        )>();
        if directory {
            commands.entity(entity).insert((
                Inspect,
                MovePaths {
                    source: paths.root.join("registry.toml"),
                    destination: paths.root.join("tools.toml"),
                },
            ));
        } else {
            commands
                .entity(entity)
                .insert(MigrationResult(Ok(())))
                .remove::<(Active, MigrationRequest, MovePaths)>();
        }
    }
}
