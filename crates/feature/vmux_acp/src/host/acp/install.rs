use super::*;
use crate::host::acp::registry::{Catalog, CatalogSource, RegistryAgent};
use bevy::ecs::system::SystemParam;
use bevy::prelude::{
    App, ChildOf, Commands, Entity, Has, On, Plugin, Query, Remove, Res, Update, With, Without,
};
use vmux_ecs::profile::CurrentProfile;

pub(super) struct InstallPlugin;

impl Plugin for InstallPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_tool::McpRegistryPlugin>() {
            app.add_plugins(vmux_tool::McpRegistryPlugin);
        }
        app.add_plugins(package::PackagePreparationPlugin)
            .add_message::<super::AcpPackageChanged>()
            .add_observer(cancel)
            .add_systems(
                Update,
                (
                    invalidate,
                    queue,
                    start,
                    poll,
                    prepare,
                    prepared,
                    environment,
                    resolve,
                    prepare_mcp,
                    deliver,
                )
                    .chain(),
            );
    }
}

#[derive(SystemParam)]
struct InstallContext<'w, 's> {
    profile: CurrentProfile<'w, 's>,
    catalogs: Query<'w, 's, (Entity, &'static Catalog, &'static CatalogSource)>,
    policies: Query<'w, 's, &'static crate::policy::AcpRegistryPolicy>,
}

impl InstallContext<'_, '_> {
    fn current(&self) -> Option<(Entity, &vmux_ecs::profile::ProfilePaths, Entity, &Catalog)> {
        let profile = self.profile.entity()?;
        let paths = self.profile.paths()?;
        let (entity, catalog, source) = self.catalogs.single().ok()?;
        let policy = self.policies.single().ok()?;
        if !catalog.ready
            || source.profile != Some(profile)
            || source.cache != paths.agents().join("registry.json")
            || source.url != policy.url
        {
            return None;
        }
        Some((profile, paths, entity, catalog))
    }

    fn accepts(&self, key: &AcpInstallKey) -> bool {
        let Some(expected_profile) = key.profile else {
            return true;
        };
        let Some((profile, paths, entity, catalog)) = self.current() else {
            return false;
        };
        profile == expected_profile
            && paths.agents() == key.agents
            && Some(entity) == key.catalog
            && catalog.revision == key.catalog_revision
    }
}

fn queue(
    mut commands: Commands,
    sessions: Query<
        (Entity, &AgentId),
        (
            With<Session>,
            Without<AcpLaunchStarted>,
            Without<AcpInstallRequest>,
            Without<crate::host::navigation_driver::RetryRequested>,
        ),
    >,
    targets: Query<&EntityTarget<Session>>,
    definitions: Query<(&RegistryAgent, &ChildOf)>,
    context: InstallContext,
    focused: vmux_layout::stack::FocusedStack,
    settings: Option<Res<AppSettings>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let Some(focused) = focused.as_ref() else {
        return;
    };
    let Some((profile_entity, paths, catalog_entity, catalog)) = context.current() else {
        return;
    };
    if catalog.loading {
        return;
    }
    let agents = paths.agents();
    let shell = settings.shell();
    let focused_session = focused
        .stack
        .and_then(|stack| targets.get(stack).ok())
        .map(EntityTarget::entity);
    for (entity, agent_id) in &sessions {
        if focused_session != Some(entity) {
            continue;
        }
        let fallback = settings
            .agent
            .acp
            .iter()
            .find(|config| config.id == agent_id.0)
            .cloned();
        let request = AcpInstallRequest {
            profile: Some(profile_entity),
            agents: agents.clone(),
            catalog: Some(catalog_entity),
            catalog_revision: catalog.revision,
            selected_agent: definitions.iter().find_map(|(definition, parent)| {
                (parent.parent() == catalog_entity && definition.id == agent_id.0)
                    .then(|| definition.clone())
            }),
            agent_id: agent_id.0.clone(),
            fallback,
            shell: shell.clone(),
        };
        commands.entity(entity).insert(request);
    }
}

fn start(
    requests: Query<
        (Entity, &SessionId, &AgentId, &AcpInstallRequest),
        (
            Without<AcpLaunchStarted>,
            Without<crate::host::navigation_driver::RetryRequested>,
        ),
    >,
    jobs: Query<(Entity, &AcpInstallKey, Has<Retiring>)>,
    mut commands: Commands,
) {
    let mut active_jobs: Vec<_> = jobs
        .iter()
        .map(|(entity, key, retiring)| (key.clone(), entity, retiring))
        .collect();
    for (entity, session_id, agent_id, request) in &requests {
        if request.agent_id != agent_id.0 {
            commands.entity(entity).remove::<AcpInstallRequest>();
            continue;
        }
        let key = request.key();
        let job = match active_jobs.iter().find(|(active, _, _)| active == &key) {
            Some((_, _, true)) => continue,
            Some((_, job, false)) => *job,
            None => {
                let job = commands
                    .spawn((
                        Name::new(format!("ACP install: {}", request.agent_id)),
                        key.clone(),
                        request.clone(),
                        InstallState::preparing(),
                        AcpInstallJob {
                            package: None,
                            task: None,
                            outcome: None,
                        },
                    ))
                    .id();
                active_jobs.push((key, job, false));
                job
            }
        };
        commands
            .entity(entity)
            .remove::<AcpInstallRequest>()
            .insert((
                AcpLaunchStarted,
                AcpInstallWaiter {
                    job,
                    sid: session_id.0.clone(),
                    agent_id: agent_id.0.clone(),
                },
            ));
    }
}

fn prepare(
    mut jobs: Query<
        (
            Entity,
            &AcpInstallRequest,
            &AcpInstallKey,
            &mut AcpInstallJob,
        ),
        (Without<PackageStarted>, Without<Retiring>),
    >,
    mut commands: Commands,
) {
    for (entity, request, key, mut job) in &mut jobs {
        if let Some(agent) = &request.selected_agent {
            let package = commands
                .spawn((
                    Name::new(format!("Prepare ACP package {}", request.agent_id)),
                    vmux_ecs::Operation::bundle("acp.package.prepare").unwrap(),
                    vmux_ecs::OperationRetained,
                    vmux_ecs::OperationTarget::new(entity),
                    package::Request {
                        agent: agent.clone(),
                        version: key.version.clone(),
                        store: vmux_editor::lsp::store::PackageStore::at(request.agents.clone()),
                    },
                ))
                .id();
            job.package = Some(package);
        } else {
            commands.entity(entity).insert(AgentReady(Err(format!(
                "agent not in ACP registry: {}",
                request.agent_id
            ))));
        }
        commands
            .entity(entity)
            .remove::<AcpInstallRequest>()
            .insert(PackageStarted);
    }
}

fn prepared(
    mut jobs: Query<(Entity, &mut AcpInstallJob), Without<Retiring>>,
    packages: Query<&package::Prepared, With<package::Released>>,
    mut commands: Commands,
) {
    for (entity, mut job) in &mut jobs {
        let Some(package) = job.package else {
            continue;
        };
        let Ok(result) = packages.get(package) else {
            continue;
        };
        commands.entity(entity).insert(AgentReady(result.0.clone()));
        commands.entity(package).despawn();
        job.package = None;
    }
}

fn environment(
    mut jobs: Query<
        (
            Entity,
            &AcpInstallKey,
            &mut AcpInstallJob,
            Option<&AgentReady>,
        ),
        Without<Retiring>,
    >,
    workers: Res<vmux_ecs::BlockingRuntime>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    for (entity, key, mut job, ready) in &mut jobs {
        if job.task.is_some() || job.outcome.is_some() {
            continue;
        }
        let Some(ready) = ready else {
            continue;
        };
        let agent = ready.0.clone();
        let shell = key.shell.clone();
        match workers.spawn_process(
            vmux_ecs::wake::Wake::optional_waker(wake.clone()),
            move || AcpInstallResult {
                agent,
                login_env: vmux_process::LoginShellEnvironment::get(&shell).to_vec(),
            },
        ) {
            Ok(task) => {
                job.task = Some(task);
                commands.entity(entity).remove::<AgentReady>();
            }
            Err(vmux_ecs::blocking::SpawnError::Full) => {}
            Err(error) => {
                job.outcome = Some(AcpInstallOutcome {
                    launch: Err(error.to_string()),
                });
                commands.entity(entity).remove::<AgentReady>();
            }
        }
    }
}

fn invalidate(
    mut swaps: MessageReader<vmux_ecs::agent::SwapStackSession>,
    targets: Query<&EntityTarget<Session>>,
    waiters: Query<(Entity, &SessionId, &AgentId, &AcpInstallWaiter)>,
    requests: Query<(Entity, &AcpInstallRequest)>,
    jobs: Query<&AcpInstallKey>,
    context: InstallContext,
    mut commands: Commands,
) {
    let mut swapping = std::collections::HashSet::new();
    for request in swaps.read() {
        if let Ok(target) = targets.get(request.stack) {
            swapping.insert(target.entity());
        }
    }
    for (entity, request) in &requests {
        if swapping.contains(&entity) || !context.accepts(&request.key()) {
            commands.entity(entity).remove::<AcpInstallRequest>();
        }
    }
    for (entity, sid, agent, waiter) in &waiters {
        if swapping.contains(&entity)
            || !waiter.matches(sid, agent)
            || !jobs.get(waiter.job).is_ok_and(|key| context.accepts(key))
        {
            commands.entity(entity).remove::<(
                AcpInstallWaiter,
                AcpLaunchStarted,
                vmux_tool::McpResolveRequest,
                vmux_tool::McpResolveResult,
            )>();
        }
    }
}

fn poll(
    mut jobs: Query<
        (
            Entity,
            &AcpInstallKey,
            &mut AcpInstallJob,
            &mut InstallState,
            Option<&AcpInstallWaiters>,
        ),
        Without<package::Request>,
    >,
    packages: Query<(), With<package::Released>>,
    progress: Query<&InstallState, With<package::Request>>,
    context: InstallContext,
    mut commands: Commands,
) {
    for (entity, key, mut job, mut state, waiters) in &mut jobs {
        if !context.accepts(key) || waiters.is_none_or(|waiters| waiters.iter().next().is_none()) {
            commands.entity(entity).insert(Retiring);
            if let Some(package) = job.package {
                commands
                    .entity(package)
                    .insert(vmux_ecs::OperationState::Canceled);
                if packages.contains(package) {
                    commands.entity(package).despawn();
                    job.package = None;
                }
            }
            if let Some(task) = &mut job.task {
                task.cancel();
                if task.is_finished() {
                    job.task.take();
                }
            }
            if job.package.is_none() && job.task.is_none() {
                job.outcome = Some(AcpInstallOutcome {
                    launch: Err("agent installation canceled".into()),
                });
            }
            continue;
        }
        if let Some(package) = job.package
            && let Ok(progress) = progress.get(package)
            && *state != *progress
        {
            *state = progress.clone();
        }
        if let Some(result) = job.task.as_mut().and_then(|task| task.poll()) {
            job.task.take();
            match result {
                Ok(result) => {
                    commands.entity(entity).insert(result);
                }
                Err(_) => {
                    job.outcome = Some(AcpInstallOutcome {
                        launch: Err("agent installation failed unexpectedly".into()),
                    });
                }
            }
        }
    }
}

fn resolve(
    mut jobs: Query<(
        Entity,
        &AcpInstallKey,
        &AcpInstallResult,
        &mut AcpInstallJob,
    )>,
    context: InstallContext,
    mut package_changes: MessageWriter<AcpPackageChanged>,
    mut commands: Commands,
) {
    for (entity, key, result, mut job) in &mut jobs {
        commands.entity(entity).remove::<AcpInstallResult>();
        if !context.accepts(key) {
            job.outcome = Some(AcpInstallOutcome {
                launch: Err("agent installation canceled".into()),
            });
            continue;
        }
        let package_added = result.agent.as_ref().is_ok_and(|agent| agent.package_added);
        let launch = match &result.agent {
            Ok(agent) => Ok(AcpLaunch {
                command: agent.command.clone(),
                args: agent.args.clone(),
                env: AcpEnvironment::build(
                    agent.env.clone(),
                    &result.login_env,
                    agent.path_prepend.clone(),
                )
                .into_inner(),
            }),
            Err(_) if !key.fallback_command.is_empty() => Ok(AcpLaunch {
                command: key.fallback_command.clone(),
                args: key.fallback_args.clone(),
                env: AcpEnvironment::build(key.fallback_env.clone(), &result.login_env, None)
                    .into_inner(),
            }),
            Err(error) => Err(error.clone()),
        };
        job.outcome = Some(AcpInstallOutcome { launch });
        if package_added {
            package_changes.write(AcpPackageChanged {
                agent_id: key.agent_id.clone(),
            });
        }
    }
}

fn prepare_mcp(
    profile: CurrentProfile,
    jobs: Query<&AcpInstallJob>,
    waiters: Query<
        (Entity, &SessionId, &AgentId, &Cwd, &AcpInstallWaiter),
        (
            With<Session>,
            Without<crate::host::navigation_driver::RetryRequested>,
            Without<vmux_tool::McpResolveRequest>,
            Without<vmux_tool::McpResolveResult>,
        ),
    >,
    mut commands: Commands,
) {
    let Some(paths) = profile.paths() else {
        return;
    };
    for (entity, sid, agent, cwd, waiter) in &waiters {
        if !waiter.matches(sid, agent) {
            continue;
        }
        let Ok(job) = jobs.get(waiter.job) else {
            continue;
        };
        let Some(outcome) = &job.outcome else {
            continue;
        };
        let Ok(launch) = &outcome.launch else {
            continue;
        };
        commands
            .entity(entity)
            .insert(vmux_tool::McpResolveRequest {
                config_dir: paths.config(),
                profile_dir: paths.profile(),
                cwd: Some(cwd.0.clone()),
                agent_id: Some(agent.0.clone()),
                env: launch.env.clone(),
            });
    }
}

fn deliver(
    profile: CurrentProfile,
    connected: Option<Single<(), With<ServiceConnected>>>,
    settings: Option<Res<AppSettings>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut jobs: Query<(Entity, &mut AcpInstallJob, Option<&AcpInstallWaiters>)>,
    mut waiters: Query<
        (
            Entity,
            &SessionId,
            &AgentId,
            &Cwd,
            &ProcessAnchor,
            Option<&AcpSessionId>,
            &AcpInstallWaiter,
            Option<&vmux_tool::McpResolveResult>,
            &mut RunState,
        ),
        (
            With<Session>,
            Without<crate::host::navigation_driver::RetryRequested>,
        ),
    >,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let has_profile = profile.paths().is_some();
    for (job_entity, mut job, related) in &mut jobs {
        let Some(outcome) = &job.outcome else {
            continue;
        };
        let related = related
            .map(|related| related.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let has_waiters = related.iter().any(|entity| {
            waiters
                .get(*entity)
                .is_ok_and(|(_, sid, agent, _, _, _, waiter, _, _)| waiter.matches(sid, agent))
        });
        if has_waiters && outcome.launch.is_ok() && connected.is_none() {
            continue;
        }
        let waiting_mcp = has_profile
            && outcome.launch.is_ok()
            && related.iter().any(|entity| {
                waiters
                    .get(*entity)
                    .is_ok_and(|(_, sid, agent, _, _, _, waiter, resolved, _)| {
                        waiter.matches(sid, agent) && resolved.is_none()
                    })
            });
        if waiting_mcp {
            continue;
        }
        let outcome = job.outcome.take().unwrap();
        for entity in related {
            let Ok((_, sid, agent, cwd, anchor, resume, waiter, resolved, mut state)) =
                waiters.get_mut(entity)
            else {
                continue;
            };
            if !waiter.matches(sid, agent) {
                continue;
            }
            match &outcome.launch {
                Ok(launch) => {
                    let managed_mcp_servers = resolved
                        .map(|result| result.managed.clone())
                        .unwrap_or_default();
                    service_requests.write(ServiceRequest(launch.message_for(
                        sid,
                        agent,
                        cwd,
                        anchor,
                        resume,
                        settings.as_deref(),
                        managed_mcp_servers,
                    )));
                    *state = RunState::Idle;
                    commands
                        .entity(entity)
                        .insert(AcpSpawnQueued)
                        .remove::<(AcpInstallWaiter, vmux_tool::McpResolveResult)>();
                }
                Err(message) => {
                    *state = RunState::Errored(message.clone());
                    commands.entity(entity).remove::<(
                        AcpInstallWaiter,
                        vmux_tool::McpResolveRequest,
                        vmux_tool::McpResolveResult,
                    )>();
                }
            }
        }
        commands.entity(job_entity).despawn();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn cancel(trigger: On<Remove, ProcessAnchor>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(trigger.event_target()) {
        entity.remove::<(
            AcpInstallWaiter,
            AcpLaunchStarted,
            AcpSpawnQueued,
            AcpInstallRequest,
            vmux_tool::McpResolveRequest,
            vmux_tool::McpResolveResult,
        )>();
    }
}
