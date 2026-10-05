use super::*;

pub(super) fn add(app: &mut App) {
    app.add_message::<super::AcpPackageChanged>()
        .add_observer(cancel)
        .add_systems(Update, (start, poll).chain());
}

fn start(
    mut commands: Commands,
    sessions: Query<(Entity, &AcpSession), Without<AcpLaunchStarted>>,
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
    for (entity, session) in &sessions {
        if focused.stack != Some(entity) {
            continue;
        }
        let fallback = settings
            .agent
            .acp
            .iter()
            .find(|config| config.id == session.agent_id)
            .cloned();
        let request = AcpInstallRequest {
            agent_id: session.agent_id.clone(),
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
                sid: session.sid.clone(),
                agent_id: session.agent_id.clone(),
            },
            AgentRunState::Installing {
                pct: None,
                message: "Preparing agent…".to_string(),
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
        Option<&AcpInstallWaiters>,
    )>,
    mut waiters: Query<(Entity, &AcpSession, &AcpInstallWaiter, &mut AgentRunState)>,
    mut package_changes: MessageWriter<AcpPackageChanged>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(credentials) = credentials.single() else {
        return;
    };
    let swapping: std::collections::HashSet<Entity> =
        swaps.read().map(|request| request.stack).collect();
    let mut invalid_waiters = std::collections::HashSet::new();
    for (entity, session, waiter, _) in &mut waiters {
        if swapping.contains(&entity) || !waiter.matches(session) || !jobs.contains(waiter.job) {
            invalid_waiters.insert(entity);
            commands
                .entity(entity)
                .remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
        }
    }
    for (job_entity, key, mut job, related_waiters) in &mut jobs {
        let related_waiters = related_waiters
            .map(|related| related.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(progress) = job.take_progress() {
            for entity in related_waiters.iter().copied() {
                let Ok((_, session, waiter, mut state)) = waiters.get_mut(entity) else {
                    continue;
                };
                if invalid_waiters.contains(&entity) || !waiter.matches(session) {
                    continue;
                }
                *state = (&progress).into();
            }
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
            waiters.get(*entity).is_ok_and(|(_, session, waiter, _)| {
                !invalid_waiters.contains(entity) && waiter.matches(session)
            })
        });
        if has_waiters && launch_ready && connected.is_none() {
            continue;
        }
        let outcome = job.outcome.take().unwrap();
        for entity in related_waiters {
            let Ok((_, session, waiter, mut state)) = waiters.get_mut(entity) else {
                continue;
            };
            if invalid_waiters.contains(&entity) || !waiter.matches(session) {
                continue;
            }
            match &outcome.launch {
                Ok(launch) => {
                    let message = launch.message_for(session, settings.as_deref());
                    match credentials.access.with_revision(launch.mcp_revision, || ()) {
                        Ok(Some(())) => {
                            service_requests.write(ServiceRequest(message));
                            *state = AcpInstallProgress::ready(session.resume.as_deref()).into();
                            commands.entity(entity).remove::<AcpInstallWaiter>();
                        }
                        Ok(None) => {
                            *state = AcpInstallProgress::preparing().into();
                            commands
                                .entity(entity)
                                .remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
                        }
                        Err(error) => {
                            *state = AcpInstallProgress::error(error).into();
                            commands.entity(entity).remove::<AcpInstallWaiter>();
                        }
                    }
                }
                Err(message) => {
                    *state = AcpInstallProgress::error(message.clone()).into();
                    commands.entity(entity).remove::<AcpInstallWaiter>();
                }
            }
        }
        commands.entity(job_entity).despawn();
    }
}

fn cancel(trigger: On<Remove, AcpSession>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(trigger.event_target()) {
        entity.remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
    }
}
