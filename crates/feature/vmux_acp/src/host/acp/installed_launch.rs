use bevy::prelude::*;
use vmux_ecs::{Job, JobPlugin, JobRequest, JobResult, OperationState, OperationTarget};
use vmux_editor::lsp::store::Receipt;
use vmux_session::provider::Read as CatalogRead;
use vmux_tool::io::WriteFile;

use super::AcpLaunch;
use super::environment_driver::AcpEnvironment;
use super::installed::{Checked, InstalledAgent, InstalledLaunch, InstalledSet};
use super::package::Lease;

pub(super) struct InstalledLaunchPlugin;

impl Plugin for InstalledLaunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<LoginEnvironment>::default())
            .add_systems(
                Update,
                (invalidate, start, written, environment, completed)
                    .chain()
                    .in_set(PreparationSet)
                    .after(InstalledSet)
                    .before(vmux_ecs::JobSet::Submit),
            )
            .add_systems(PostUpdate, release.after(vmux_ecs::JobSet::Cleanup));
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::host) struct PreparationSet;

#[derive(Component)]
pub(in crate::host) struct Request {
    pub(in crate::host) agent: Entity,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub(in crate::host) source: Checked,
    pub(in crate::host) shell: String,
}

#[derive(Component)]
pub(in crate::host) struct Completed(pub(in crate::host) Result<AcpLaunch, String>);

#[derive(Component)]
#[allow(unknown_lints, vmux_ecs_ownership)]
struct Prepared(InstalledLaunch);
#[derive(Component)]
struct Writing;
#[derive(Component)]
struct Written;
#[derive(Component)]
struct Environment;

#[derive(Clone)]
struct LoginEnvironment {
    shell: String,
}

impl Job for LoginEnvironment {
    type Output = Result<EnvironmentValues, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::BlockingCancellation::canceled() {
            return Err("ACP catalog environment canceled".into());
        }
        Ok(EnvironmentValues(
            vmux_process::LoginShellEnvironment::get(&self.shell).to_vec(),
        ))
    }
}

#[derive(Component)]
#[allow(unknown_lints, vmux_ecs_ownership)]
struct EnvironmentValues(Vec<(String, String)>);

fn invalidate(
    requests: Query<(Entity, &Request, Option<&OperationTarget>, &OperationState)>,
    sources: Query<&Checked, With<InstalledAgent>>,
    reads: Query<&OperationState, With<CatalogRead>>,
    mut commands: Commands,
) {
    for (entity, request, target, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let accepted = sources.get(request.agent).ok() == Some(&request.source)
            && target.is_some_and(|target| {
                reads.get(target.entity()).is_ok_and(|state| {
                    matches!(state, OperationState::Pending | OperationState::Running)
                })
            });
        if !accepted {
            commands.entity(entity).insert((
                OperationState::Canceled,
                Completed(Err("ACP catalog source changed".into())),
            ));
        }
    }
}

fn start(
    requests: Query<(Entity, &Request, &OperationState), (Without<Prepared>, Without<Completed>)>,
    sources: Query<(&InstalledLaunch, &Receipt), With<InstalledAgent>>,
    leases: Query<&Lease>,
    mut commands: Commands,
) {
    let mut occupied = Vec::new();
    for lease in &leases {
        occupied.extend(lease.0.iter().cloned());
    }
    for (entity, request, state) in &requests {
        if *state != OperationState::Pending {
            continue;
        }
        let Ok((launch, receipt)) = sources.get(request.agent) else {
            continue;
        };
        let scopes = request.source.scopes(launch, &receipt.name);
        if scopes.iter().any(|path| occupied.contains(path)) {
            continue;
        }
        occupied.extend(scopes.iter().cloned());
        let mut entity = commands.entity(entity);
        entity.insert((
            Prepared(launch.clone()),
            Lease(scopes),
            OperationState::Running,
        ));
        if let Some(path) = &launch.npmrc {
            entity.insert((
                Writing,
                JobRequest(WriteFile {
                    path: path.clone(),
                    bytes: b"registry=https://registry.npmjs.org/\nignore-scripts=false\n".to_vec(),
                }),
            ));
        } else {
            entity.insert(Written);
        }
    }
}

fn written(
    requests: Query<(Entity, &OperationState, &JobResult<WriteFile>), With<Writing>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Writing, JobResult<WriteFile>)>();
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert(Written);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Completed(Err(error.clone()))));
            }
        }
    }
}

fn environment(
    requests: Query<
        (Entity, &Request, &OperationState),
        (With<Written>, Without<Environment>, Without<Completed>),
    >,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if *state == OperationState::Running {
            commands.entity(entity).insert((
                Environment,
                JobRequest(LoginEnvironment {
                    shell: request.shell.clone(),
                }),
            ));
        }
    }
}

fn completed(
    requests: Query<
        (
            Entity,
            &Prepared,
            &OperationState,
            &JobResult<LoginEnvironment>,
        ),
        Without<Completed>,
    >,
    mut commands: Commands,
) {
    for (entity, prepared, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let launch = match &result.0 {
            Ok(Ok(environment)) => Ok(AcpLaunch {
                command: prepared.0.agent.command.clone(),
                args: prepared.0.agent.args.clone(),
                env: AcpEnvironment::catalog(
                    prepared.0.agent.env.clone(),
                    &environment.0,
                    prepared.0.agent.path_prepend.clone(),
                )
                .into_inner(),
            }),
            Ok(Err(error)) | Err(error) => Err(error.clone()),
        };
        let state = if launch.is_ok() {
            OperationState::Succeeded
        } else {
            OperationState::Failed
        };
        commands
            .entity(entity)
            .insert((state, Completed(launch)))
            .remove::<JobResult<LoginEnvironment>>();
    }
}

fn release(
    requests: Query<
        (
            Entity,
            &OperationState,
            Option<&OperationTarget>,
            Has<Lease>,
        ),
        With<Request>,
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    reads: Query<&OperationState, With<CatalogRead>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, state, target, leased) in &requests {
        if matches!(state, OperationState::Pending | OperationState::Running)
            || captures.iter().any(|target| target.entity() == entity)
        {
            continue;
        }
        if target.is_some_and(|target| {
            reads.get(target.entity()).is_ok_and(|state| {
                matches!(state, OperationState::Pending | OperationState::Running)
            })
        }) {
            if !leased {
                continue;
            }
            commands.entity(entity).remove::<Lease>();
        } else {
            commands.entity(entity).despawn();
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}
