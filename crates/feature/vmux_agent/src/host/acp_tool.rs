use std::collections::BTreeMap;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bevy::prelude::*;
use vmux_api::protocol::{ClientMessage, ManagedMcpServer};
use vmux_core::event::InstallPhase;
use vmux_core::service::ServiceConnected;
use vmux_core::service::ServiceRequest;
use vmux_core::tool::{ToolOperationKey, ToolOperationKind, ToolProvider, ToolStatus};
use vmux_editor::lsp::store::PackageStore;
use vmux_session::AcpSession;
use vmux_setting::{AcpAgentConfig, AppSettings};
use vmux_tool::{
    ToolInventory, ToolInventoryItem, ToolOperator, ToolProviderId, ToolProviderSnapshot,
    ToolScanner, ToolStore, ToolsManifest,
};

use super::acp_environment::AcpEnvironment;
use super::acp_install::{resolve_from_registry, uninstall};
use crate::acp_registry::{self, RegistryAgent};
use crate::host::launch::{AgentLaunchPolicy, AgentLaunchPolicyQuery};
use vmux_session::AgentRunState;

pub(crate) struct AcpToolPlugin;

impl Plugin for AcpToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_message::<AcpPackageChanged>()
            .add_message::<vmux_core::agent::SwapStackSession>()
            .add_observer(cancel_acp_install_on_remove)
            .add_systems(Startup, spawn_tool_provider)
            .add_systems(Update, (start_acp_installs, poll_acp_installs).chain());
    }
}

fn spawn_tool_provider(mut commands: Commands) {
    commands.spawn((
        Name::new("ACP tool provider"),
        ToolProviderId(ToolProvider::Acp),
        ToolScanner::new(scan_tools),
        ToolOperator::new(operate_tool),
    ));
}

fn scan_tools(
    _store: &ToolStore,
    manifest: &mut ToolsManifest,
    refresh: bool,
) -> Result<ToolProviderSnapshot, String> {
    let catalog = if refresh {
        acp_registry::fetch_blocking()
            .ok()
            .or_else(acp_registry::load_cached)
    } else {
        acp_registry::load_cached()
    };
    let catalog = catalog
        .map(|registry| {
            registry
                .agents
                .into_iter()
                .map(|agent| (agent.id.clone(), agent))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let package_store = PackageStore::at(vmux_core::profile::ProfilePaths::current().agents());
    let receipts = package_store.installed();
    let inventory = receipts
        .into_values()
        .filter(|receipt| receipt.source_id.starts_with("acp:"))
        .map(|receipt| {
            let agent = catalog.get(receipt.name.as_str());
            let latest = agent.and_then(|agent| agent.version.clone());
            ToolInventoryItem {
                id: receipt.name.as_str().to_string(),
                name: agent
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| receipt.name.as_str().to_string()),
                icon: agent.and_then(|agent| agent.icon.clone()),
                version: receipt.version.clone(),
                detail: agent
                    .and_then(|agent| agent.description.clone())
                    .unwrap_or_else(|| "ACP agent".to_string()),
                status: if receipt.version.is_some()
                    && latest.is_some()
                    && receipt.version != latest
                {
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
                },
                removable: true,
            }
        })
        .collect();
    Ok(ToolInventory::new(ToolProvider::Acp, inventory)
        .reconcile(manifest)
        .into())
}

fn operate_tool(
    store: &ToolStore,
    operation: &ToolOperationKey,
    _value: &str,
) -> Result<String, String> {
    let id = operation.item_id.trim();
    match operation.kind {
        ToolOperationKind::Install | ToolOperationKind::Update => {
            if id.is_empty() {
                return Err("package name is required".to_string());
            }
            resolve_from_registry(id, None, |_, _, _| {})?;
            store.set_managed_package(ToolProvider::Acp, id, true)?;
            let operation = if operation.kind == ToolOperationKind::Install {
                "installed"
            } else {
                "updated"
            };
            Ok(format!("{id} {operation}"))
        }
        ToolOperationKind::Uninstall => {
            if id.is_empty() {
                return Err("package name is required".to_string());
            }
            uninstall(id)?;
            store.set_managed_package(ToolProvider::Acp, id, false)?;
            Ok(format!("{id} removed"))
        }
        ToolOperationKind::Forget => {
            store.set_managed_package(ToolProvider::Acp, id, false)?;
            Ok(format!("{id} removed from tools.toml"))
        }
        ToolOperationKind::Adopt => {
            store.set_managed_package(ToolProvider::Acp, id, true)?;
            Ok(format!("{id} is now managed"))
        }
        ToolOperationKind::Import => {
            let mut manifest = store.load()?;
            let before = manifest.managed_packages(ToolProvider::Acp.id()).len();
            let _ = scan_tools(store, &mut manifest, false)?;
            let imported = manifest
                .managed_packages(ToolProvider::Acp.id())
                .len()
                .saturating_sub(before);
            store.save(&manifest)?;
            Ok(format!("imported {imported} acp item(s)"))
        }
        _ => Err(format!("ACP does not support {:?}", operation.kind)),
    }
}

#[derive(Component)]
pub(crate) struct AcpLaunchStarted;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcpPackageChanged {
    pub(crate) agent_id: String,
}

#[derive(Component)]
struct AcpInstallJob {
    progress: Arc<Mutex<Option<AcpInstallProgress>>>,
    thread: Option<JoinHandle<AcpInstallOutcome>>,
    outcome: Option<AcpInstallOutcome>,
    package_reported: bool,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
struct AcpInstallKey {
    agent_id: String,
    version: Option<String>,
    fallback_command: String,
    fallback_args: Vec<String>,
    fallback_env: Vec<(String, String)>,
    shell: String,
    policy: AgentLaunchPolicy,
}

#[derive(Component)]
#[relationship(relationship_target = AcpInstallWaiters)]
struct AcpInstallWaiter {
    #[relationship]
    job: Entity,
    sid: String,
    agent_id: String,
}

#[derive(Component)]
#[relationship_target(relationship = AcpInstallWaiter)]
struct AcpInstallWaiters(Vec<Entity>);

struct AcpInstallRequest {
    agent_id: String,
    fallback: Option<AcpAgentConfig>,
    shell: String,
    policy: AgentLaunchPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AcpInstallProgress {
    pct: Option<u8>,
    message: String,
    errored: bool,
}

#[derive(Clone)]
struct AcpInstallOutcome {
    package_added: bool,
    launch: Result<AcpLaunch, String>,
}

#[derive(Clone)]
struct AcpLaunch {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    managed_mcp_servers: Vec<ManagedMcpServer>,
    mcp_revision: u64,
}

#[derive(Clone)]
struct AcpInstallProgressSink {
    pending: Arc<Mutex<Option<AcpInstallProgress>>>,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

fn start_acp_installs(
    mut commands: Commands,
    sessions: Query<(Entity, &AcpSession), Without<AcpLaunchStarted>>,
    jobs: Query<(Entity, &AcpInstallKey)>,
    focused: vmux_layout::stack::FocusedStack,
    settings: Option<Res<AppSettings>>,
    policy: AgentLaunchPolicyQuery,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let Some(focused) = focused.as_ref() else {
        return;
    };
    let shell = vmux_terminal::agent_run::AgentTerminalShell::configured(&settings).into_string();
    let policy = policy.snapshot();
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
            .find(|config| RegistryAgent::ids_match(&config.id, &session.agent_id))
            .cloned();
        let request = AcpInstallRequest {
            agent_id: session.agent_id.clone(),
            fallback,
            shell: shell.clone(),
            policy: policy.clone(),
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
                        start_acp_install_job(request, wake.clone()),
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

fn poll_acp_installs(
    mut swaps: MessageReader<vmux_core::agent::SwapStackSession>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    settings: Option<Res<AppSettings>>,
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
                progress.apply(&mut state);
            }
        }
        if job
            .thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
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
                    match vmux_core::profile::mcp_credentials::McpCredentialAccess::with_revision(
                        launch.mcp_revision,
                        || (),
                    ) {
                        Ok(Some(())) => {
                            service_requests.write(ServiceRequest(message));
                            AcpInstallProgress::ready(session.resume.as_deref()).apply(&mut state);
                            commands.entity(entity).remove::<AcpInstallWaiter>();
                        }
                        Ok(None) => {
                            AcpInstallProgress::preparing().apply(&mut state);
                            commands
                                .entity(entity)
                                .remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
                        }
                        Err(error) => {
                            AcpInstallProgress::error(error).apply(&mut state);
                            commands.entity(entity).remove::<AcpInstallWaiter>();
                        }
                    }
                }
                Err(message) => {
                    AcpInstallProgress::error(message.clone()).apply(&mut state);
                    commands.entity(entity).remove::<AcpInstallWaiter>();
                }
            }
        }
        commands.entity(job_entity).despawn();
    }
}

fn cancel_acp_install_on_remove(trigger: On<Remove, AcpSession>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(trigger.event_target()) {
        entity.remove::<(AcpInstallWaiter, AcpLaunchStarted)>();
    }
}

impl AcpLaunchStarted {
    fn ready_message(resume: Option<&str>) -> &'static str {
        if resume.is_some() {
            "Loading session history…"
        } else {
            "Starting agent…"
        }
    }
}

impl AcpInstallJob {
    fn take_progress(&mut self) -> Option<AcpInstallProgress> {
        match self.progress.lock() {
            Ok(mut pending) => pending.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }
}

fn start_acp_install_job(
    request: AcpInstallRequest,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
) -> AcpInstallJob {
    let progress = Arc::new(Mutex::new(None));
    let sink = AcpInstallProgressSink {
        pending: progress.clone(),
        wake,
    };
    let thread = std::thread::spawn(move || {
        let outcome = resolve_acp_install(request, &sink);
        sink.notify();
        outcome
    });
    AcpInstallJob {
        progress,
        thread: Some(thread),
        outcome: None,
        package_reported: false,
    }
}

fn resolve_acp_install(
    request: AcpInstallRequest,
    progress: &AcpInstallProgressSink,
) -> AcpInstallOutcome {
    let pinned_version = request
        .fallback
        .as_ref()
        .and_then(|config| config.version.as_deref());
    let resolved =
        resolve_from_registry(&request.agent_id, pinned_version, |phase, pct, message| {
            progress.publish(AcpInstallProgress::from_phase(phase, pct, message));
        });
    let package_added = resolved
        .as_ref()
        .map(|resolved| resolved.package_added)
        .unwrap_or(false);
    let login_env = vmux_terminal::shell_env::login_shell_env(&request.shell);
    let managed_mcp =
        match crate::managed_mcp::PreparedManagedMcpServers::for_agent(&request.agent_id) {
            Ok(managed_mcp) => managed_mcp,
            Err(message) => {
                return AcpInstallOutcome {
                    package_added,
                    launch: Err(message),
                };
            }
        };
    let launch = match resolved {
        Ok(resolved) => Ok(AcpLaunch {
            command: resolved.command,
            args: resolved.args,
            env: AcpEnvironment::build(resolved.env, login_env, resolved.path_prepend)
                .for_agent(&request.agent_id, &request.policy)
                .into_inner(),
            managed_mcp_servers: managed_mcp.servers,
            mcp_revision: managed_mcp.revision,
        }),
        Err(registry_error) => match request.fallback {
            Some(config) if !config.command.is_empty() => Ok(AcpLaunch {
                command: config.command,
                args: config.args,
                env: AcpEnvironment::build(config.env, login_env, None)
                    .for_agent(&request.agent_id, &request.policy)
                    .into_inner(),
                managed_mcp_servers: managed_mcp.servers,
                mcp_revision: managed_mcp.revision,
            }),
            _ => Err(registry_error),
        },
    };
    AcpInstallOutcome {
        package_added,
        launch,
    }
}

impl AcpInstallRequest {
    fn key(&self) -> AcpInstallKey {
        let fallback = self.fallback.as_ref();
        AcpInstallKey {
            agent_id: self.agent_id.clone(),
            version: fallback.and_then(|config| config.version.clone()),
            fallback_command: fallback
                .map(|config| config.command.clone())
                .unwrap_or_default(),
            fallback_args: fallback
                .map(|config| config.args.clone())
                .unwrap_or_default(),
            fallback_env: fallback
                .map(|config| config.env.clone())
                .unwrap_or_default(),
            shell: self.shell.clone(),
            policy: self.policy.clone(),
        }
    }
}

impl AcpInstallWaiter {
    fn matches(&self, session: &AcpSession) -> bool {
        self.sid == session.sid && self.agent_id == session.agent_id
    }
}

impl AcpLaunch {
    fn message_for(&self, session: &AcpSession, settings: Option<&AppSettings>) -> ClientMessage {
        let shell = settings
            .map(|settings| {
                vmux_terminal::agent_run::AgentTerminalShell::configured(settings).into_string()
            })
            .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_default());
        let mcp = crate::mcp::resolve_acp(&session.cwd, session.anchor, &session.agent_id, &shell)
            .inspect_err(|error| {
                bevy::log::warn!(
                    "acp: vmux_mcp sidecar unresolved; agent runs without vmux tools: {error}"
                );
            })
            .ok();
        let env = AcpEnvironment::from(self.env.clone())
            .with_managed_servers(
                &session.agent_id,
                self.managed_mcp_servers
                    .iter()
                    .map(|server| server.name.clone()),
            )
            .into_inner();
        ClientMessage::SpawnAcpAgent {
            sid: session.sid.clone(),
            agent_id: session.agent_id.clone(),
            command: self.command.clone(),
            args: self.args.clone(),
            env,
            cwd: session.cwd.to_string_lossy().into_owned(),
            anchor: session.anchor,
            mcp_command: mcp.as_ref().map(|mcp| mcp.command.clone()),
            mcp_args: mcp.map(|mcp| mcp.args).unwrap_or_default(),
            resume_acp_session_id: session.resume.clone(),
            managed_mcp_servers: self.managed_mcp_servers.clone(),
            effort: settings
                .and_then(|settings| settings.agent.effort_for(&session.agent_id))
                .map(str::to_string),
        }
    }
}

impl AcpInstallProgressSink {
    fn publish(&self, progress: AcpInstallProgress) {
        match self.pending.lock() {
            Ok(mut pending) => *pending = Some(progress),
            Err(poisoned) => *poisoned.into_inner() = Some(progress),
        }
        self.notify();
    }

    fn notify(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }
}

impl AcpInstallProgress {
    fn from_phase(phase: InstallPhase, pct: Option<u8>, message: &str) -> Self {
        if matches!(phase, InstallPhase::Done) {
            Self {
                pct: None,
                message: "Starting agent…".to_string(),
                errored: false,
            }
        } else {
            Self {
                pct,
                message: message.to_string(),
                errored: false,
            }
        }
    }

    fn ready(resume: Option<&str>) -> Self {
        Self {
            pct: None,
            message: AcpLaunchStarted::ready_message(resume).to_string(),
            errored: false,
        }
    }

    fn preparing() -> Self {
        Self {
            pct: None,
            message: "Preparing agent…".to_string(),
            errored: false,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            pct: None,
            message: message.into(),
            errored: true,
        }
    }

    fn apply(&self, state: &mut AgentRunState) {
        if self.errored {
            *state = AgentRunState::Errored(self.message.clone());
        } else {
            *state = AgentRunState::Installing {
                pct: self.pct,
                message: self.message.clone(),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(AcpToolPlugin);
        app
    }

    fn completed_job(message: &str) -> AcpInstallJob {
        AcpInstallJob {
            progress: Arc::new(Mutex::new(None)),
            thread: None,
            outcome: Some(AcpInstallOutcome {
                package_added: false,
                launch: Err(message.to_string()),
            }),
            package_reported: false,
        }
    }

    fn install_key(agent_id: &str) -> AcpInstallKey {
        AcpInstallRequest {
            agent_id: agent_id.to_string(),
            fallback: None,
            shell: String::new(),
            policy: AgentLaunchPolicy::default(),
        }
        .key()
    }

    fn acp_session(agent_id: &str, sid: &str) -> AcpSession {
        AcpSession {
            agent_id: agent_id.to_string(),
            sid: sid.to_string(),
            cwd: PathBuf::from("/workspace"),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        }
    }

    #[test]
    fn completed_install_progress_describes_agent_startup() {
        let progress = AcpInstallProgress::from_phase(InstallPhase::Done, Some(100), "ready");
        assert_eq!(progress.pct, None);
        assert_eq!(progress.message, "Starting agent…");

        let progress =
            AcpInstallProgress::from_phase(InstallPhase::Downloading, Some(42), "downloading");
        assert_eq!(progress.pct, Some(42));
        assert_eq!(progress.message, "downloading");
        assert_eq!(AcpLaunchStarted::ready_message(None), "Starting agent…");
        assert_eq!(
            AcpLaunchStarted::ready_message(Some("session-1")),
            "Loading session history…"
        );
    }

    #[test]
    fn install_job_tracks_waiting_sessions_through_relationship() {
        let mut app = App::new();
        let job = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn(AcpInstallWaiter {
                job,
                sid: "session".to_string(),
                agent_id: "agent".to_string(),
            })
            .id();

        let waiters = app.world().get::<AcpInstallWaiters>(job).unwrap();
        assert_eq!(waiters.iter().collect::<Vec<_>>(), vec![stack]);
    }

    #[test]
    fn replaced_acp_session_ignores_stale_install_outcome() {
        let mut app = install_test_app();
        let job = app
            .world_mut()
            .spawn((install_key("claude"), completed_job("stale failure")))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                acp_session("codex", "new-session"),
                AcpLaunchStarted,
                AcpInstallWaiter {
                    job,
                    sid: "old-session".to_string(),
                    agent_id: "claude".to_string(),
                },
                AgentRunState::Installing {
                    pct: None,
                    message: "Preparing agent…".to_string(),
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<AcpInstallWaiter>(stack).is_none());
        assert!(app.world().get::<AcpLaunchStarted>(stack).is_none());
        assert!(matches!(
            app.world().get::<AgentRunState>(stack),
            Some(AgentRunState::Installing { .. })
        ));
        assert!(app.world().get_entity(job).is_err());
    }

    #[test]
    fn removing_acp_session_clears_install_waiter() {
        let mut app = install_test_app();
        let job = app
            .world_mut()
            .spawn((install_key("claude"), completed_job("stale failure")))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                acp_session("claude", "old-session"),
                AcpLaunchStarted,
                AcpInstallWaiter {
                    job,
                    sid: "old-session".to_string(),
                    agent_id: "claude".to_string(),
                },
                AgentRunState::Installing {
                    pct: None,
                    message: "Preparing agent…".to_string(),
                },
            ))
            .id();

        app.world_mut().entity_mut(stack).remove::<AcpSession>();
        app.update();

        assert!(app.world().get::<AcpInstallWaiter>(stack).is_none());
        assert!(app.world().get::<AcpLaunchStarted>(stack).is_none());
        assert!(matches!(
            app.world().get::<AgentRunState>(stack),
            Some(AgentRunState::Installing { .. })
        ));
        assert!(app.world().get_entity(job).is_err());
    }
}
