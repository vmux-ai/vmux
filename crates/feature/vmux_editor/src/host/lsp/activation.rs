use super::activation_io::{Activate, Backup, EnsurePackages, Restore};
use super::package_path::PackageName;
use super::store::PackageStore;
use bevy::prelude::*;
use std::path::PathBuf;
use vmux_ecs::{JobPlugin, JobRequest, JobResult, OperationState};
use vmux_tool::io::TemporaryDirectory as Staging;

pub struct PackageActivationPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActivationSet;

impl Plugin for PackageActivationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<EnsurePackages>::default(),
            JobPlugin::<Backup>::default(),
            JobPlugin::<Restore>::default(),
            JobPlugin::<Activate>::default(),
        ))
        .add_message::<Retry>()
        .add_observer(start)
        .add_systems(
            Update,
            (
                cancel, prepared, backed_up, committed, restored, retry, orphan,
            )
                .chain()
                .in_set(ActivationSet)
                .before(vmux_ecs::JobSet::Submit),
        )
        .add_systems(PostUpdate, wake);
    }
}

#[derive(Message)]
pub struct Retry {
    pub entity: Entity,
}

fn start(trigger: On<Add, Request>, requests: Query<&Request>, mut commands: Commands) {
    let entity = trigger.event_target();
    if let Ok(request) = requests.get(entity) {
        commands.entity(entity).insert((
            PreparingActivation,
            JobRequest(EnsurePackages {
                path: request.store.packages_dir(),
            }),
        ));
    }
}

#[derive(Component)]
pub struct Request {
    pub owner: Entity,
    pub store: PackageStore,
    pub name: PackageName,
    pub backup: PathBuf,
}

#[derive(Component)]
struct PreparingActivation;

#[derive(Component)]
struct BackingUp;

#[derive(Component)]
struct BackupMoved(bool);

#[derive(Component)]
struct CommittingActivation;

#[derive(Component)]
struct RestoringActivation;

#[derive(Component)]
struct ActivationCanceled;

#[derive(Component)]
struct ActivationError(String);

#[derive(Component)]
pub struct RestoreFailed;

#[derive(Component)]
pub struct Completed(pub Result<(), String>);
fn cancel(
    activations: Query<(Entity, &Request), Without<ActivationCanceled>>,
    owners: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, activation) in &activations {
        if !owners
            .get(activation.owner)
            .is_ok_and(|state| *state == OperationState::Running)
        {
            commands.entity(entity).insert(ActivationCanceled);
        }
    }
}

type PreparedActivationActivations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Staging,
        &'static JobResult<EnsurePackages>,
        Has<ActivationCanceled>,
    ),
    With<PreparingActivation>,
>;

fn prepared(activations: PreparedActivationActivations<'_, '_>, mut commands: Commands) {
    for (entity, activation, staging, result, canceled) in &activations {
        commands
            .entity(entity)
            .remove::<(PreparingActivation, JobResult<EnsurePackages>)>();
        match &result.0 {
            Ok(Ok(())) if !canceled => {
                commands.entity(entity).insert((
                    BackingUp,
                    JobRequest(Backup {
                        source: activation.store.package_dir(&activation.name),
                        target: activation.backup.clone(),
                        staging: staging.clone(),
                    }),
                ));
            }
            Ok(Ok(())) => {
                commands
                    .entity(entity)
                    .insert(Completed(Err("ACP activation canceled".into())));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert(Completed(Err(error.clone())));
            }
        }
    }
}

type BackedUpActivations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Staging,
        &'static JobResult<Backup>,
        Has<ActivationCanceled>,
    ),
    With<BackingUp>,
>;

fn backed_up(activations: BackedUpActivations<'_, '_>, mut commands: Commands) {
    for (entity, activation, staging, result, canceled) in &activations {
        commands
            .entity(entity)
            .remove::<(BackingUp, JobResult<Backup>)>();
        let moved = match &result.0 {
            Ok(Ok(moved)) => moved.0,
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert(Completed(Err(error.clone())));
                continue;
            }
        };
        commands.entity(entity).insert(BackupMoved(moved));
        if !canceled {
            commands.entity(entity).insert((
                CommittingActivation,
                JobRequest(Activate {
                    store: activation.store.clone(),
                    name: activation.name.clone(),
                    staging: staging.clone(),
                }),
            ));
        } else if moved {
            commands.entity(entity).insert((
                ActivationError("ACP activation canceled".into()),
                RestoringActivation,
                JobRequest(Restore {
                    source: activation.backup.clone(),
                    target: activation.store.package_dir(&activation.name),
                    staging: staging.clone(),
                }),
            ));
        } else {
            commands
                .entity(entity)
                .insert(Completed(Err("ACP activation canceled".into())));
        }
    }
}

type CommittedActivations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Staging,
        &'static BackupMoved,
        &'static JobResult<Activate>,
    ),
    With<CommittingActivation>,
>;

fn committed(activations: CommittedActivations<'_, '_>, mut commands: Commands) {
    for (entity, activation, staging, backup, result) in &activations {
        commands
            .entity(entity)
            .remove::<(CommittingActivation, JobResult<Activate>)>();
        let error = match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert(Completed(Ok(())));
                continue;
            }
            Ok(Err(error)) | Err(error) => error.clone(),
        };
        if backup.0 {
            commands.entity(entity).insert((
                ActivationError(error),
                RestoringActivation,
                JobRequest(Restore {
                    source: activation.backup.clone(),
                    target: activation.store.package_dir(&activation.name),
                    staging: staging.clone(),
                }),
            ));
        } else {
            commands.entity(entity).insert(Completed(Err(error)));
        }
    }
}

fn restored(
    activations: Query<(Entity, &ActivationError, &JobResult<Restore>), With<RestoringActivation>>,
    mut commands: Commands,
) {
    for (entity, original, result) in &activations {
        commands
            .entity(entity)
            .remove::<(RestoringActivation, JobResult<Restore>)>();
        match &result.0 {
            Ok(Ok(())) => {
                commands
                    .entity(entity)
                    .insert(Completed(Err(original.0.clone())));
            }
            Ok(Err(error)) | Err(error) => {
                let failure = format!("{}; ACP backup restore failed: {error}", original.0);
                commands.entity(entity).insert((
                    RestoreFailed,
                    vmux_ecs::OperationError::new(failure.clone()).unwrap(),
                ));
            }
        }
    }
}
fn retry(
    mut requests: MessageReader<Retry>,
    activations: Query<(&Request, &Staging), With<RestoreFailed>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((activation, staging)) = activations.get(request.entity) else {
            continue;
        };
        commands
            .entity(request.entity)
            .remove::<(RestoreFailed, vmux_ecs::OperationError)>()
            .insert((
                RestoringActivation,
                JobRequest(Restore {
                    source: activation.backup.clone(),
                    target: activation.store.package_dir(&activation.name),
                    staging: staging.clone(),
                }),
            ));
    }
}

fn orphan(
    activations: Query<(Entity, &Request), With<Completed>>,
    owners: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, activation) in &activations {
        if !owners.contains(activation.owner) {
            commands.entity(entity).try_despawn();
        }
    }
}

type WakeReady<'w, 's> = Query<'w, 's, Entity, Or<(Added<Completed>, Added<RestoreFailed>)>>;

fn wake(ready: WakeReady<'_, '_>, wake: Option<Res<vmux_ecs::JobWake>>) {
    if !ready.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}
