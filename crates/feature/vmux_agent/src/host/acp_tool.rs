use std::collections::BTreeMap;
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

struct AcpEnvironment(Vec<(String, String)>);

impl AcpEnvironment {
    fn build(
        mut base: Vec<(String, String)>,
        login_env: &[(String, String)],
        path_prepend: Option<String>,
    ) -> Self {
        base.extend(login_env.iter().cloned());
        let mut environment = Self(base);
        environment.deduplicate();
        environment.prepend_path(path_prepend);
        environment
    }

    fn for_agent(mut self, agent_id: &str, policy: &AgentLaunchPolicy) -> Self {
        match RegistryAgent::canonical_id(agent_id) {
            "mistral-vibe" => self.apply_vibe(),
            "codex-acp" => self.apply_codex(policy),
            "claude-acp" => self.apply_claude(),
            _ => {}
        }
        self
    }

    fn with_managed_servers(
        mut self,
        agent_id: &str,
        server_names: impl IntoIterator<Item = String>,
    ) -> Self {
        if RegistryAgent::canonical_id(agent_id) != "codex-acp" {
            return self;
        }
        let existing = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| value.as_str());
        let (mut config, warning) = Self::parse_codex_config(existing);
        if let Some(warning) = warning {
            bevy::log::warn!("{warning}");
        }
        let features = config
            .entry("features")
            .or_insert_with(|| serde_json::json!({}));
        if !features.is_object() {
            *features = serde_json::json!({});
        }
        let code_mode = features
            .as_object_mut()
            .unwrap()
            .entry("code_mode")
            .or_insert_with(|| serde_json::json!({}));
        if !code_mode.is_object() {
            *code_mode = serde_json::json!({});
        }
        let namespaces = code_mode
            .as_object_mut()
            .unwrap()
            .entry("direct_only_tool_namespaces")
            .or_insert_with(|| serde_json::json!([]));
        if !namespaces.is_array() {
            *namespaces = serde_json::json!([]);
        }
        let namespaces = namespaces.as_array_mut().unwrap();
        let vmux =
            serde_json::Value::String(crate::host::cli::codex::DIRECT_ONLY_NAMESPACE.to_string());
        if !namespaces.contains(&vmux) {
            namespaces.push(vmux);
        }
        for server_name in server_names {
            let namespace = serde_json::Value::String(format!("mcp__{server_name}"));
            if !namespaces.contains(&namespace) {
                namespaces.push(namespace);
            }
        }
        self.0.retain(|(key, _)| key != "CODEX_CONFIG");
        self.0.push((
            "CODEX_CONFIG".to_string(),
            serde_json::Value::Object(config).to_string(),
        ));
        self
    }

    fn into_inner(self) -> Vec<(String, String)> {
        self.0
    }

    fn prepend_path(&mut self, prepend: Option<String>) {
        let Some(directory) = prepend else {
            return;
        };
        let existing = self
            .0
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var("PATH").ok())
            .filter(|value| !value.is_empty());
        let path = match existing {
            Some(existing) => format!("{directory}:{existing}"),
            None => directory,
        };
        self.0.retain(|(key, _)| key != "PATH");
        self.0.push(("PATH".to_string(), path));
    }

    fn deduplicate(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut environment = Vec::with_capacity(self.0.len());
        for (key, value) in std::mem::take(&mut self.0).into_iter().rev() {
            if seen.insert(key.clone()) {
                environment.push((key, value));
            }
        }
        environment.reverse();
        self.0 = environment;
    }

    fn apply_claude(&mut self) {
        self.0.retain(|(key, _)| key != "MCP_TOOL_TIMEOUT");
        self.0.push((
            "MCP_TOOL_TIMEOUT".to_string(),
            (crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS * 1_000).to_string(),
        ));
    }

    fn apply_vibe(&mut self) {
        let mut disabled = Vec::new();
        if let Some(value) = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
            .map(|(_, value)| value)
        {
            match serde_json::from_str::<Vec<String>>(value) {
                Ok(existing) => Self::extend_unique(&mut disabled, existing),
                Err(error) => bevy::log::warn!(
                    "acp: existing VIBE_DISABLED_TOOLS is invalid JSON ({error}); discarding it"
                ),
            }
        }
        Self::extend_unique(&mut disabled, ["bash".to_string()]);
        self.0.retain(|(key, _)| key != "VIBE_DISABLED_TOOLS");
        self.0.push((
            "VIBE_DISABLED_TOOLS".to_string(),
            serde_json::to_string(&disabled).unwrap(),
        ));
        let mut mcp_servers: Vec<serde_json::Value> = Vec::new();
        if let Some(value) = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "VIBE_MCP_SERVERS")
            .map(|(_, value)| value)
        {
            match serde_json::from_str::<Vec<serde_json::Value>>(value) {
                Ok(existing) => {
                    for server in existing {
                        if let Some(name) = server.get("name").and_then(serde_json::Value::as_str) {
                            mcp_servers.retain(|candidate| {
                                candidate.get("name").and_then(serde_json::Value::as_str)
                                    != Some(name)
                            });
                        }
                        mcp_servers.push(server);
                    }
                }
                Err(error) => bevy::log::warn!(
                    "acp: existing VIBE_MCP_SERVERS is invalid JSON ({error}); discarding it"
                ),
            }
        }
        self.0.retain(|(key, _)| key != "VIBE_MCP_SERVERS");
        if !mcp_servers.is_empty() {
            self.0.push((
                "VIBE_MCP_SERVERS".to_string(),
                serde_json::to_string(&mcp_servers).unwrap(),
            ));
        }
    }

    fn apply_codex(&mut self, policy: &AgentLaunchPolicy) {
        self.0
            .retain(|(key, _)| key != "DISABLE_MCP_CONFIG_FILTERING");
        let existing = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| value.as_str());
        let (mut config, warning) = Self::parse_codex_config(existing);
        if let Some(warning) = warning {
            bevy::log::warn!("{warning}");
        }
        config.insert(
            "approvals_reviewer".to_string(),
            serde_json::Value::String("user".to_string()),
        );
        let features = config
            .entry("features")
            .or_insert_with(|| serde_json::json!({}));
        if !features.is_object() {
            *features = serde_json::json!({});
        }
        let features = features.as_object_mut().unwrap();
        features.insert("shell_tool".to_string(), serde_json::Value::Bool(false));
        features.insert("unified_exec".to_string(), serde_json::Value::Bool(false));
        let code_mode = features
            .entry("code_mode")
            .or_insert_with(|| serde_json::json!({}));
        if !code_mode.is_object() {
            *code_mode = serde_json::json!({});
        }
        code_mode.as_object_mut().unwrap().insert(
            "direct_only_tool_namespaces".to_string(),
            serde_json::json!([crate::host::cli::codex::DIRECT_ONLY_NAMESPACE]),
        );
        let tools = config
            .entry("tools")
            .or_insert_with(|| serde_json::json!({}));
        if !tools.is_object() {
            *tools = serde_json::json!({});
        }
        tools
            .as_object_mut()
            .unwrap()
            .insert("web_search".to_string(), serde_json::Value::Bool(false));
        Self::disable_codex_skills(
            &mut config,
            &crate::host::cli::codex::codex_disabled_skill_files(policy.disabled_skill_roots()),
        );
        let mcp_servers = config
            .entry("mcp_servers")
            .or_insert_with(|| serde_json::json!({}));
        if !mcp_servers.is_object() {
            *mcp_servers = serde_json::json!({});
        }
        let vmux = mcp_servers
            .as_object_mut()
            .unwrap()
            .entry("vmux")
            .or_insert_with(|| serde_json::json!({}));
        if !vmux.is_object() {
            *vmux = serde_json::json!({});
        }
        vmux.as_object_mut().unwrap().insert(
            "tool_timeout_sec".to_string(),
            serde_json::json!(crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS),
        );
        let instructions = config
            .get("developer_instructions")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let instructions = if instructions.contains("mcp__vmux__run") {
            instructions.to_string()
        } else if instructions.is_empty() {
            crate::host::cli::codex::RUN_STEER_PROMPT.to_string()
        } else {
            format!(
                "{instructions}\n\n{}",
                crate::host::cli::codex::RUN_STEER_PROMPT
            )
        };
        let instructions = policy.prompt(&instructions);
        let instructions = if instructions.contains("mcp__vmux__set_conversation_title") {
            instructions
        } else {
            format!("{instructions}\n\n{CONVERSATION_TITLE_STEER_PROMPT}")
        };
        config.insert(
            "developer_instructions".to_string(),
            serde_json::Value::String(instructions),
        );
        self.0.retain(|(key, _)| key != "CODEX_CONFIG");
        self.0.push((
            "CODEX_CONFIG".to_string(),
            serde_json::Value::Object(config).to_string(),
        ));
    }

    fn disable_codex_skills(
        config: &mut serde_json::Map<String, serde_json::Value>,
        skill_files: &[PathBuf],
    ) {
        if skill_files.is_empty() {
            return;
        }
        let skills = config
            .entry("skills")
            .or_insert_with(|| serde_json::json!({}));
        if !skills.is_object() {
            *skills = serde_json::json!({});
        }
        let configured = skills
            .as_object_mut()
            .unwrap()
            .entry("config")
            .or_insert_with(|| serde_json::json!([]));
        if !configured.is_array() {
            *configured = serde_json::json!([]);
        }
        let configured = configured.as_array_mut().unwrap();
        for skill_file in skill_files {
            let path = skill_file.to_string_lossy();
            if let Some(existing) = configured.iter_mut().find(|entry| {
                entry
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|candidate| candidate == path)
            }) {
                existing
                    .as_object_mut()
                    .unwrap()
                    .insert("enabled".to_string(), serde_json::Value::Bool(false));
            } else {
                configured.push(serde_json::json!({
                    "path": path,
                    "enabled": false,
                }));
            }
        }
    }

    fn parse_codex_config(
        value: Option<&str>,
    ) -> (serde_json::Map<String, serde_json::Value>, Option<String>) {
        let Some(value) = value else {
            return (serde_json::Map::new(), None);
        };
        match serde_json::from_str::<serde_json::Value>(value) {
            Ok(serde_json::Value::Object(config)) => (config, None),
            Ok(value) => {
                let kind = match value {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => unreachable!(),
                };
                (
                    serde_json::Map::new(),
                    Some(format!(
                        "acp: existing CODEX_CONFIG is not a JSON object ({kind}); discarding it"
                    )),
                )
            }
            Err(error) => (
                serde_json::Map::new(),
                Some(format!(
                    "acp: existing CODEX_CONFIG is invalid JSON ({error}); discarding it"
                )),
            ),
        }
    }

    fn extend_unique(values: &mut Vec<String>, additions: impl IntoIterator<Item = String>) {
        for value in additions {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
}

impl From<Vec<(String, String)>> for AcpEnvironment {
    fn from(environment: Vec<(String, String)>) -> Self {
        Self(environment)
    }
}

const CONVERSATION_TITLE_STEER_PROMPT: &str = "On the first user message, always call mcp__vmux__set_conversation_title as the first tool of the turn. The host immediately shows the raw first prompt as a provisional title; replace it with a concise 3 to 7 word summary with corrected spelling and grammar. On later user messages, call the tool only when the conversation topic materially changes; keep the current title for same-topic follow-ups. When needed, call it before reading skills, calling any other tool, or answering. Never copy the user's prompt verbatim. This tool never needs user permission.";

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

    fn env(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    #[test]
    fn login_environment_overrides_registry_environment() {
        let base = vec![env("MISTRAL_API_KEY", ""), env("KEEP", "1")];
        let login = vec![
            env("MISTRAL_API_KEY", "real-key"),
            env("PATH", "/login/bin"),
        ];
        let environment = AcpEnvironment::build(base, &login, None).into_inner();

        assert!(environment.contains(&env("MISTRAL_API_KEY", "real-key")));
        assert!(environment.contains(&env("KEEP", "1")));
        assert!(environment.contains(&env("PATH", "/login/bin")));
    }

    #[test]
    fn managed_binary_precedes_login_path() {
        let login = vec![env("PATH", "/login/bin")];
        let environment =
            AcpEnvironment::build(Vec::new(), &login, Some("/managed/node/bin".to_string()))
                .into_inner();
        let path = environment
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.as_str());

        assert_eq!(path, Some("/managed/node/bin:/login/bin"));
    }

    #[test]
    fn managed_binary_uses_environment_path() {
        let environment = AcpEnvironment::build(
            vec![env("PATH", "/from/login")],
            &[],
            Some("/managed".to_string()),
        )
        .into_inner();

        assert_eq!(
            environment
                .iter()
                .find(|(key, _)| key == "PATH")
                .map(|(_, value)| value.as_str()),
            Some("/managed:/from/login")
        );
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

    #[test]
    fn codex_environment_applies_feature_policy_and_routes_shell_commands_through_vmux() {
        let policy = AgentLaunchPolicy::new(
            vec!["Feature-owned agent instruction.".to_string()],
            Vec::new(),
        );
        for agent_id in ["codex", "codex-acp"] {
            let environment = AcpEnvironment::from(Vec::new())
                .for_agent(agent_id, &policy)
                .into_inner();
            let config = environment
                .iter()
                .find(|(key, _)| key == "CODEX_CONFIG")
                .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
                .expect("codex ACP compatibility config");

            assert_eq!(config["features"]["shell_tool"], false);
            assert_eq!(config["features"]["unified_exec"], false);
            assert_eq!(config["tools"]["web_search"], false);
            assert_eq!(config["approvals_reviewer"], "user");
            assert_eq!(config["mcp_servers"]["vmux"]["tool_timeout_sec"], 660);
            assert!(
                environment
                    .iter()
                    .all(|(key, _)| key != "DISABLE_MCP_CONFIG_FILTERING")
            );
            assert_eq!(
                config["features"]["code_mode"]["direct_only_tool_namespaces"],
                serde_json::json!([crate::host::cli::codex::DIRECT_ONLY_NAMESPACE])
            );
            let instructions = config["developer_instructions"].as_str().unwrap();
            assert!(instructions.contains("mcp__vmux__run"));
            assert!(instructions.contains("mcp__vmux__set_conversation_title"));
            assert!(instructions.contains("first tool of the turn"));
            assert!(instructions.contains("raw first prompt as a provisional title"));
            assert!(instructions.contains("topic materially changes"));
            assert!(instructions.contains("same-topic follow-ups"));
            assert!(instructions.contains("never needs user permission"));
            assert!(instructions.contains("Feature-owned agent instruction."));
        }
    }

    #[test]
    fn codex_environment_exposes_managed_namespaces() {
        let environment = AcpEnvironment::from(Vec::new())
            .for_agent("codex-acp", &AgentLaunchPolicy::default())
            .with_managed_servers(
                "codex-acp",
                ["vmux_linear".to_string(), "vmux_notion".to_string()],
            )
            .into_inner();
        let config = environment
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("codex ACP compatibility config");

        assert_eq!(
            config["features"]["code_mode"]["direct_only_tool_namespaces"],
            serde_json::json!(["mcp__vmux", "mcp__vmux_linear", "mcp__vmux_notion"])
        );
    }

    #[test]
    fn codex_environment_disables_session_skills() {
        let mut config = serde_json::json!({
            "skills": {
                "config": [
                    {"path": "/tmp/knowledge/alpha/SKILL.md", "enabled": true},
                    {"path": "/tmp/other", "enabled": true}
                ]
            }
        })
        .as_object()
        .unwrap()
        .clone();
        AcpEnvironment::disable_codex_skills(
            &mut config,
            &[
                PathBuf::from("/tmp/knowledge/alpha/SKILL.md"),
                PathBuf::from("/tmp/knowledge/beta/SKILL.md"),
            ],
        );

        assert_eq!(config["skills"]["config"][0]["enabled"], false);
        assert_eq!(config["skills"]["config"][1]["enabled"], true);
        assert_eq!(
            config["skills"]["config"][2],
            serde_json::json!({"path": "/tmp/knowledge/beta/SKILL.md", "enabled": false})
        );
    }

    #[test]
    fn claude_environment_extends_mcp_timeout() {
        for agent_id in ["claude", "claude-acp"] {
            let environment = AcpEnvironment::from(vec![env("MCP_TOOL_TIMEOUT", "60000")])
                .for_agent(agent_id, &AgentLaunchPolicy::default())
                .into_inner();
            assert_eq!(
                environment
                    .iter()
                    .find(|(key, _)| key == "MCP_TOOL_TIMEOUT")
                    .map(|(_, value)| value.as_str()),
                Some("660000")
            );
        }
    }

    #[test]
    fn vibe_environment_disables_shell_tool() {
        let environment = AcpEnvironment::from(vec![
            env("VIBE_DISABLED_TOOLS", r#"["from-env"]"#),
            env(
                "VIBE_MCP_SERVERS",
                r#"[{"name":"from-env","transport":"stdio","command":"env-command"}]"#,
            ),
        ])
        .for_agent("mistral-vibe", &AgentLaunchPolicy::default())
        .into_inner();
        let disabled = environment
            .iter()
            .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
            .map(|(_, value)| serde_json::from_str::<Vec<String>>(value).unwrap())
            .expect("Vibe ACP disabled tools");

        assert_eq!(disabled, vec!["from-env", "bash"]);
        let mcp_servers = environment
            .iter()
            .find(|(key, _)| key == "VIBE_MCP_SERVERS")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("Vibe ACP MCP servers");
        assert_eq!(mcp_servers[0]["name"], "from-env");
    }

    #[test]
    fn vibe_environment_discards_invalid_mcp_configuration() {
        let environment = AcpEnvironment::from(vec![env("VIBE_MCP_SERVERS", "not-json")])
            .for_agent("mistral-vibe", &AgentLaunchPolicy::default())
            .into_inner();

        assert!(environment.iter().all(|(key, _)| key != "VIBE_MCP_SERVERS"));
    }

    #[test]
    fn codex_environment_preserves_existing_configuration() {
        let environment = AcpEnvironment::from(vec![env(
            "CODEX_CONFIG",
            r#"{"model":"gpt-test","features":{"custom_feature":true,"code_mode":{"custom_setting":"keep"}}}"#,
        )])
        .for_agent("codex", &AgentLaunchPolicy::default())
        .into_inner();
        let config = environment
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .unwrap();

        assert_eq!(config["model"], "gpt-test");
        assert_eq!(config["features"]["custom_feature"], true);
        assert_eq!(config["features"]["code_mode"]["custom_setting"], "keep");
        assert_eq!(config["features"]["shell_tool"], false);
    }

    #[test]
    fn codex_environment_reports_discarded_configuration() {
        let (_, invalid_json) = AcpEnvironment::parse_codex_config(Some("{not-json"));
        assert!(invalid_json.unwrap().contains("invalid JSON"));

        let (_, non_object) = AcpEnvironment::parse_codex_config(Some("[]"));
        assert!(non_object.unwrap().contains("not a JSON object"));
    }
}
