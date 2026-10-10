use super::*;

pub(super) fn add(app: &mut App) {
    app.add_message::<super::AcpPackageChanged>()
        .add_observer(cancel)
        .add_systems(Update, (start, poll).chain());
}

fn start(
    mut commands: Commands,
    sessions: Query<(Entity, &SessionId, &AgentId), (With<Session>, Without<AcpLaunchStarted>)>,
    targets: Query<&EntityTarget<Session>>,
    jobs: Query<(Entity, &AcpInstallKey)>,
    credentials: Query<&McpCredentials>,
    profile: CurrentProfile,
    focused: vmux_layout::stack::FocusedStack,
    settings: Option<Res<AppSettings>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let Some(focused) = focused.as_ref() else {
        return;
    };
    let Ok(credentials) = credentials.single() else {
        return;
    };
    let Some(agents) = profile.paths().map(|paths| paths.agents()) else {
        return;
    };
    let shell = vmux_terminal::AgentTerminalShell::configured(&settings).into_string();
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let mut active_jobs: Vec<(AcpInstallKey, Entity)> = jobs
        .iter()
        .map(|(entity, key)| (key.clone(), entity))
        .collect();
    let focused_session = focused
        .stack
        .and_then(|stack| targets.get(stack).ok())
        .map(EntityTarget::entity);
    for (entity, session_id, agent_id) in &sessions {
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
            agent_id: agent_id.0.clone(),
            fallback,
            shell: shell.clone(),
        };
        let key = request.key();
        let job = match active_jobs
            .iter()
            .find(|(active, _)| active == &key)
            .map(|(_, entity)| *entity)
        {
            Some(job) => job,
            None => {
                let name = Name::new(format!("ACP install: {}", request.agent_id));
                let job = commands
                    .spawn((
                        name,
                        key.clone(),
                        InstallState::preparing(),
                        start_acp_install_job(
                            request,
                            wake.clone(),
                            (*credentials).clone(),
                            agents.clone(),
                        ),
                    ))
                    .id();
                active_jobs.push((key, job));
                job
            }
        };
        commands.entity(entity).insert((
            AcpLaunchStarted,
            AcpInstallWaiter {
                job,
                sid: session_id.0.clone(),
                agent_id: agent_id.0.clone(),
            },
        ));
    }
}

fn poll(
    mut swaps: MessageReader<vmux_ecs::agent::SwapStackSession>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    settings: Option<Res<AppSettings>>,
    credentials: Query<&McpCredentials>,
    mut jobs: Query<(
        Entity,
        &AcpInstallKey,
        &mut AcpInstallJob,
        &mut InstallState,
        Option<&AcpInstallWaiters>,
    )>,
    targets: Query<&EntityTarget<Session>>,
    mut waiters: Query<
        (
            Entity,
            &SessionId,
            &AgentId,
            &Cwd,
            &ProcessAnchor,
            Option<&AcpSessionId>,
            &AcpInstallWaiter,
            &mut RunState,
        ),
        With<Session>,
    >,
    mut package_changes: MessageWriter<AcpPackageChanged>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(credentials) = credentials.single() else {
        return;
    };
    let swapping: std::collections::HashSet<Entity> = swaps
        .read()
        .filter_map(|request| targets.get(request.stack).ok().map(EntityTarget::entity))
        .collect();
    let mut invalid_waiters = std::collections::HashSet::new();
    for (entity, session_id, agent_id, _, _, _, waiter, _) in &mut waiters {
        if swapping.contains(&entity)
            || !waiter.matches(session_id, agent_id)
            || !jobs.contains(waiter.job)
        {
            invalid_waiters.insert(entity);
            commands
                .entity(entity)
                .remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
        }
    }
    for (job_entity, key, mut job, mut install_state, related_waiters) in &mut jobs {
        let related_waiters = related_waiters
            .map(|related| related.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(progress) = job.take_progress() {
            *install_state = progress;
        }
        if job.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            let thread = job.thread.take().unwrap();
            job.outcome = Some(thread.join().unwrap_or_else(|_| AcpInstallOutcome {
                package_added: false,
                launch: Err("agent installation failed unexpectedly".to_string()),
            }));
        }
        let Some((package_added, launch_ready)) = job
            .outcome
            .as_ref()
            .map(|outcome| (outcome.package_added, outcome.launch.is_ok()))
        else {
            continue;
        };
        if package_added && !job.package_reported {
            package_changes.write(AcpPackageChanged {
                agent_id: key.agent_id.clone(),
            });
            job.package_reported = true;
        }
        let has_waiters = related_waiters.iter().any(|entity| {
            waiters
                .get(*entity)
                .is_ok_and(|(_, session_id, agent_id, _, _, _, waiter, _)| {
                    !invalid_waiters.contains(entity) && waiter.matches(session_id, agent_id)
                })
        });
        if has_waiters && launch_ready && connected.is_none() {
            continue;
        }
        let outcome = job.outcome.take().unwrap();
        for entity in related_waiters {
            let Ok((_, session_id, agent_id, cwd, anchor, resume, waiter, mut state)) =
                waiters.get_mut(entity)
            else {
                continue;
            };
            if invalid_waiters.contains(&entity) || !waiter.matches(session_id, agent_id) {
                continue;
            }
            match &outcome.launch {
                Ok(launch) => {
                    let message = launch.message_for(
                        session_id,
                        agent_id,
                        cwd,
                        anchor,
                        resume,
                        settings.as_deref(),
                    );
                    match credentials.access.with_revision(launch.mcp_revision, || ()) {
                        Ok(Some(())) => {
                            service_requests.write(ServiceRequest(message));
                            *state = RunState::Idle;
                            commands.entity(entity).remove::<AcpInstallWaiter>();
                        }
                        Ok(None) => {
                            *state = RunState::Idle;
                            commands
                                .entity(entity)
                                .remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
                        }
                        Err(error) => {
                            *state = RunState::Errored(error);
                            commands.entity(entity).remove::<AcpInstallWaiter>();
                        }
                    }
                }
                Err(message) => {
                    *state = RunState::Errored(message.clone());
                    commands.entity(entity).remove::<AcpInstallWaiter>();
                }
            }
        }
        commands.entity(job_entity).despawn();
    }
}

fn cancel(trigger: On<Remove, ProcessAnchor>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(trigger.event_target()) {
        entity.remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
    }
}
