use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bevy::prelude::*;
use vmux_core::event::InstallPhase;
use vmux_editor::lsp::package_path::{PackageName, PackagePath};
use vmux_editor::lsp::{archive, download, store};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ClientMessage, ManagedMcpServer};
use vmux_session::AcpSession;
use vmux_setting::{AcpAgentConfig, AppSettings};

use crate::acp_registry::{self, BinaryTarget, RegistryAgent};
use crate::run_state::AgentRunState;

pub(crate) struct AcpToolPlugin;

impl Plugin for AcpToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AcpPackageChanged>()
            .add_message::<vmux_core::agent::SwapStackSession>()
            .add_observer(cancel_acp_install_on_remove)
            .add_systems(Update, (start_acp_installs, poll_acp_installs).chain());
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
    focused: Option<Res<vmux_layout::stack::FocusedStack>>,
    settings: Option<Res<AppSettings>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let Some(focused) = focused else {
        return;
    };
    let shell = crate::host::agent_terminal_shell(&settings);
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
            .find(|config| agent_ids_match(&config.id, &session.agent_id))
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
    service: Option<Single<&ServiceClient>>,
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
        job.poll();
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
        if has_waiters && launch_ready && service.is_none() {
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
                    let service = service.as_ref().unwrap();
                    let message = launch.message_for(session, settings.as_deref());
                    match vmux_core::profile::mcp_credentials::McpCredentialAccess::with_revision(
                        launch.mcp_revision,
                        || service.0.send(message),
                    ) {
                        Ok(Some(())) => {
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

    fn poll(&mut self) {
        let Some(thread) = self.thread.as_ref() else {
            return;
        };
        if !thread.is_finished() {
            return;
        }
        let thread = self.thread.take().unwrap();
        self.outcome = Some(thread.join().unwrap_or_else(|_| AcpInstallOutcome {
            package_added: false,
            launch: Err("agent installation failed unexpectedly".to_string()),
        }));
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
        let outcome = request.resolve(&sink);
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
        }
    }

    fn resolve(self, progress: &AcpInstallProgressSink) -> AcpInstallOutcome {
        let pinned_version = self
            .fallback
            .as_ref()
            .and_then(|config| config.version.as_deref());
        let resolved =
            resolve_from_registry(&self.agent_id, pinned_version, |phase, pct, message| {
                progress.publish(AcpInstallProgress::from_phase(phase, pct, message));
            });
        let package_added = resolved
            .as_ref()
            .map(|resolved| resolved.package_added)
            .unwrap_or(false);
        let login_env = vmux_terminal::shell_env::login_shell_env(&self.shell);
        let managed_mcp = match crate::managed_mcp::acp_servers(&self.agent_id) {
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
                    .for_agent(&self.agent_id)
                    .into_inner(),
                managed_mcp_servers: managed_mcp.servers,
                mcp_revision: managed_mcp.revision,
            }),
            Err(registry_error) => match self.fallback {
                Some(config) if !config.command.is_empty() => Ok(AcpLaunch {
                    command: config.command,
                    args: config.args,
                    env: AcpEnvironment::build(config.env, login_env, None)
                        .for_agent(&self.agent_id)
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
}

impl AcpInstallWaiter {
    fn matches(&self, session: &AcpSession) -> bool {
        self.sid == session.sid && self.agent_id == session.agent_id
    }
}

impl AcpLaunch {
    fn message_for(&self, session: &AcpSession, settings: Option<&AppSettings>) -> ClientMessage {
        let mcp = crate::mcp::resolve_acp(&session.cwd, session.anchor, &session.agent_id)
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

    fn for_agent(mut self, agent_id: &str) -> Self {
        match registry_id_alias(agent_id) {
            "mistral-vibe" => self.apply_vibe(),
            "codex-acp" => self.apply_codex(),
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
        if registry_id_alias(agent_id) != "codex-acp" {
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
        let vmux = serde_json::Value::String(
            crate::runtime::cli::codex::DIRECT_ONLY_NAMESPACE.to_string(),
        );
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

    fn apply_codex(&mut self) {
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
            serde_json::json!([crate::runtime::cli::codex::DIRECT_ONLY_NAMESPACE]),
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
            &crate::runtime::cli::codex::codex_disabled_skill_files(),
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
            crate::runtime::cli::codex::RUN_STEER_PROMPT.to_string()
        } else {
            format!(
                "{instructions}\n\n{}",
                crate::runtime::cli::codex::RUN_STEER_PROMPT
            )
        };
        let instructions =
            vmux_core::knowledge::AgentPrompt::from(instructions.as_str()).into_string();
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgent {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path_prepend: Option<String>,
    pub package_added: bool,
}

const NODE_VERSION: &str = "22.11.0";
const UV_VERSION: &str = "0.5.11";

fn store_root() -> PathBuf {
    acp_registry::agents_dir()
}

fn write_agent_receipt(
    root: &Path,
    agent: &RegistryAgent,
    version: Option<&str>,
) -> Result<(), String> {
    let name = PackageName::parse(&agent.id)?;
    store::write_receipt(
        root,
        &name,
        &store::Receipt {
            name: name.clone(),
            version: version
                .map(str::to_string)
                .or_else(|| agent.version.clone()),
            source_id: format!("acp:{}", agent.id),
            bin: std::collections::BTreeMap::new(),
        },
    )
    .map_err(|e| e.to_string())
}

fn package_base(package: &str) -> &str {
    match package.rfind('@') {
        Some(at) if at > 0 => &package[..at],
        _ => package,
    }
}

fn package_spec(package: &str, version: Option<&str>) -> String {
    match version.map(str::trim) {
        Some(v) if !v.is_empty() => format!("{}@{v}", package_base(package)),
        _ => package.to_string(),
    }
}

fn cmd_basename(cmd: &str) -> &str {
    let rel = cmd.trim_start_matches("./").trim_start_matches(".\\");
    Path::new(rel)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel)
}

fn archive_filename(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("archive")
}

fn resolved_cmd_path(pkgdir: &Path, target: &BinaryTarget, file: &str) -> Result<PathBuf, String> {
    let rel = target
        .cmd
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .replace('\\', "/");
    let rel = PackagePath::parse(&rel)?;
    match archive::kind_for(file) {
        archive::ArchiveKind::TarGz | archive::ArchiveKind::Zip => Ok(pkgdir.join(rel.as_path())),
        archive::ArchiveKind::Gz | archive::ArchiveKind::Raw => {
            Ok(pkgdir.join(cmd_basename(rel.as_str())))
        }
    }
}

fn ensure_binary_installed(
    agent: &RegistryAgent,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let target = agent
        .binary_for_host()
        .ok_or_else(|| format!("no binary distribution for this platform: {}", agent.id))?;
    let root = store_root();
    let name = PackageName::parse(&agent.id)?;
    let pkgdir = store::package_dir(&root, &name);
    let file = archive_filename(&target.archive).to_string();
    PackageName::parse(&file)?;
    let cmd_path = resolved_cmd_path(&pkgdir, target, &file)?;

    let up_to_date = store::read_receipt(&root, &name)
        .map(|r| r.version == agent.version)
        .unwrap_or(false);
    let package_added = !is_agent_installed_at(&root, agent);
    if !up_to_date || !cmd_path.exists() {
        install_binary(agent, target, &root, &name, &file, &mut emit)?;
    }

    Ok(ResolvedAgent {
        command: cmd_path.to_string_lossy().into_owned(),
        args: target.args.clone(),
        env: target
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: None,
        package_added,
    })
}

fn node_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

fn ensure_node(
    root: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = node_target().ok_or("managed Node not supported on this platform")?;
    let dirname = format!("node-v{NODE_VERSION}-{target}");
    let name = PackageName::parse("node")?;
    let node_parent = store::package_dir(root, &name);
    let bindir = node_parent.join(&dirname).join("bin");
    if bindir.join("node").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://nodejs.org/dist/v{NODE_VERSION}/{file}");
    let checksum_url = format!("https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt");
    let digest =
        download::sha256_from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
    let staging_root = store::staging_dir(root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix("node")
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(&file);

    emit(
        InstallPhase::Downloading,
        Some(0),
        "downloading Node runtime",
    );
    download::download_to(
        &url,
        &dl,
        download::PACKAGE_MAX_BYTES,
        &digest,
        |d, total| {
            let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
            emit(InstallPhase::Downloading, pct, "downloading Node runtime");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting Node runtime");
    archive::extract(&dl, archive::ArchiveKind::TarGz, &staged_package, &dirname)?;
    if !staged_package.join(&dirname).join("bin/node").exists()
        || !staged_package
            .join(&dirname)
            .join("lib/node_modules/npm/bin/npx-cli.js")
            .exists()
    {
        return Err("managed Node missing after extract".to_string());
    }
    store::write_receipt_in(
        &staged_package,
        &store::Receipt {
            name: name.clone(),
            version: Some(NODE_VERSION.to_string()),
            source_id: url,
            bin: Default::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    store::activate_package(root, &name, &staged_package).map_err(|error| error.to_string())?;
    Ok(bindir)
}

fn ensure_npx_installed(
    agent: &RegistryAgent,
    version: Option<&str>,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let dist = agent
        .distribution
        .npx
        .as_ref()
        .ok_or_else(|| format!("no npx distribution: {}", agent.id))?;
    let root = store_root();
    let package_added = !is_agent_installed_at(&root, agent);
    let bindir = ensure_node(&root, &mut emit)?;
    let npx = node_cli(&root, "npx-cli.js")
        .filter(|path| path.is_file())
        .ok_or("managed npx missing after extract")?;
    write_agent_receipt(&root, agent, version)?;
    emit(InstallPhase::Done, Some(100), "ready");

    let mut args = vec![
        npx.to_string_lossy().into_owned(),
        "-y".to_string(),
        package_spec(&dist.package, version),
    ];
    args.extend(dist.args.iter().cloned());
    Ok(ResolvedAgent {
        command: bindir.join("node").to_string_lossy().into_owned(),
        args,
        env: dist
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: Some(bindir.to_string_lossy().into_owned()),
        package_added,
    })
}

fn uv_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

fn ensure_uv(
    root: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = uv_target().ok_or("managed uv not supported on this platform")?;
    let dirname = format!("uv-{target}");
    let name = PackageName::parse("uv")?;
    let uv_parent = store::package_dir(root, &name);
    let bindir = uv_parent.join(&dirname);
    if bindir.join("uvx").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{file}");
    let checksum_url = format!("{url}.sha256");
    let digest =
        download::sha256_from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
    let staging_root = store::staging_dir(root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix("uv")
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(&file);

    emit(InstallPhase::Downloading, Some(0), "downloading uv runtime");
    download::download_to(
        &url,
        &dl,
        download::PACKAGE_MAX_BYTES,
        &digest,
        |d, total| {
            let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
            emit(InstallPhase::Downloading, pct, "downloading uv runtime");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting uv runtime");
    archive::extract(&dl, archive::ArchiveKind::TarGz, &staged_package, &dirname)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for exe in ["uv", "uvx"] {
            let p = staged_package.join(&dirname).join(exe);
            if let Ok(meta) = std::fs::metadata(&p) {
                let mut perm = meta.permissions();
                perm.set_mode(0o755);
                let _ = std::fs::set_permissions(&p, perm);
            }
        }
    }
    if !staged_package.join(&dirname).join("uvx").exists() {
        return Err("managed uv missing after extract".to_string());
    }
    store::write_receipt_in(
        &staged_package,
        &store::Receipt {
            name: name.clone(),
            version: Some(UV_VERSION.to_string()),
            source_id: url,
            bin: Default::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    store::activate_package(root, &name, &staged_package).map_err(|error| error.to_string())?;
    Ok(bindir)
}

fn ensure_uvx_installed(
    agent: &RegistryAgent,
    version: Option<&str>,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let dist = agent
        .distribution
        .uvx
        .as_ref()
        .ok_or_else(|| format!("no uvx distribution: {}", agent.id))?;
    let root = store_root();
    let package_added = !is_agent_installed_at(&root, agent);
    let bindir = ensure_uv(&root, &mut emit)?;
    write_agent_receipt(&root, agent, version)?;
    emit(InstallPhase::Done, Some(100), "ready");

    let mut args = vec![package_spec(&dist.package, version)];
    args.extend(dist.args.iter().cloned());
    Ok(ResolvedAgent {
        command: bindir.join("uvx").to_string_lossy().into_owned(),
        args,
        env: dist
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: Some(bindir.to_string_lossy().into_owned()),
        package_added,
    })
}

fn node_bindir(root: &Path) -> Option<PathBuf> {
    let target = node_target()?;
    Some(
        store::packages_dir(root)
            .join("node")
            .join(format!("node-v{NODE_VERSION}-{target}"))
            .join("bin"),
    )
}

fn node_cli(root: &Path, file: &str) -> Option<PathBuf> {
    Some(
        node_bindir(root)?
            .parent()?
            .join("lib/node_modules/npm/bin")
            .join(file),
    )
}

fn uv_bindir(root: &Path) -> Option<PathBuf> {
    let target = uv_target()?;
    Some(
        store::packages_dir(root)
            .join("uv")
            .join(format!("uv-{target}")),
    )
}

pub fn is_agent_installed(agent: &RegistryAgent) -> bool {
    is_agent_installed_at(&store_root(), agent)
}

fn is_agent_installed_at(root: &Path, agent: &RegistryAgent) -> bool {
    let Ok(name) = PackageName::parse(&agent.id) else {
        return false;
    };
    if !store::is_installed(root, &name) {
        return false;
    }
    match agent.preferred_runtime() {
        acp_registry::Runtime::None => true,
        acp_registry::Runtime::Node => {
            node_bindir(root).is_some_and(|bindir| bindir.join("node").is_file())
                && node_cli(root, "npx-cli.js").is_some_and(|path| path.is_file())
        }
        acp_registry::Runtime::Uv => uv_bindir(root)
            .map(|b| b.join("uvx").exists())
            .unwrap_or(false),
    }
}

pub fn is_update_available(agent: &RegistryAgent) -> bool {
    let Ok(name) = PackageName::parse(&agent.id) else {
        return false;
    };
    matches!(agent.preferred_runtime(), acp_registry::Runtime::None)
        && store::read_receipt(&store_root(), &name)
            .map(|r| r.version != agent.version)
            .unwrap_or(false)
}

pub fn uninstall(id: &str) -> Result<(), String> {
    uninstall_at(&store_root(), id)
}

fn uninstall_at(root: &Path, id: &str) -> Result<(), String> {
    let name = PackageName::parse(id)?;
    store::remove(root, &name).map_err(|e| e.to_string())
}

pub fn registry_id_alias(id: &str) -> &str {
    match id {
        "claude" => "claude-acp",
        "codex" => "codex-acp",
        "vibe" => "mistral-vibe",
        other => other,
    }
}

pub(crate) fn agent_url_id(id: &str) -> &str {
    id.strip_suffix("-acp").unwrap_or(id)
}

pub(crate) fn agent_ids_match(left: &str, right: &str) -> bool {
    let left = registry_id_alias(left);
    let right = registry_id_alias(right);
    left == right || agent_url_id(left) == agent_url_id(right)
}

pub fn resolve_from_registry(
    agent_id: &str,
    version: Option<&str>,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let reg_id = registry_id_alias(agent_id);
    let find = |reg: acp_registry::Registry| {
        reg.agents
            .into_iter()
            .find(|agent| agent_ids_match(&agent.id, agent_id))
    };
    let agent = match acp_registry::load_cached().and_then(find) {
        Some(a) => a,
        None => acp_registry::fetch_blocking()?
            .agents
            .into_iter()
            .find(|agent| agent_ids_match(&agent.id, agent_id))
            .ok_or_else(|| format!("agent not in ACP registry: {agent_id} ({reg_id})"))?,
    };
    ensure_installed(&agent, version, emit)
}

pub fn ensure_installed(
    agent: &RegistryAgent,
    version: Option<&str>,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    use acp_registry::Runtime;
    match agent.preferred_runtime() {
        Runtime::None => ensure_binary_installed(agent, emit),
        Runtime::Node => ensure_npx_installed(agent, version, emit),
        Runtime::Uv => ensure_uvx_installed(agent, version, emit),
    }
}

pub fn fetch_package_versions(agent: &RegistryAgent) -> Vec<String> {
    match agent.preferred_runtime() {
        acp_registry::Runtime::Node => agent
            .distribution
            .npx
            .as_ref()
            .map(|dist| npm_versions(package_base(&dist.package)))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn npm_versions(package: &str) -> Vec<String> {
    let root = store_root();
    let managed = node_bindir(&root).and_then(|bindir| {
        let node = bindir.join("node");
        let npm = node_cli(&root, "npm-cli.js")?;
        (node.is_file() && npm.is_file()).then_some((node, npm))
    });
    let mut command = match managed {
        Some((node, npm)) => {
            let mut command = std::process::Command::new(node);
            command.arg(npm);
            command
        }
        None => std::process::Command::new("npm"),
    };
    let output = match command
        .args(["view", package, "versions", "--json"])
        .output()
    {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };
    let mut versions: Vec<String> = match serde_json::from_slice(&output) {
        Ok(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Ok(serde_json::Value::String(one)) => vec![one],
        _ => return Vec::new(),
    };
    versions.reverse();
    versions.truncate(100);
    versions
}

#[allow(clippy::too_many_arguments)]
fn install_binary(
    agent: &RegistryAgent,
    target: &BinaryTarget,
    root: &Path,
    name: &PackageName,
    file: &str,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<(), String> {
    let digest = target
        .sha256
        .as_ref()
        .ok_or_else(|| format!("ACP registry has no SHA-256 digest for {}", agent.id))?;
    let staging_root = store::staging_dir(root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(name.as_str())
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(file);

    emit(InstallPhase::Downloading, Some(0), &target.archive);
    download::download_to(
        &target.archive,
        &dl,
        download::PACKAGE_MAX_BYTES,
        digest,
        |d, total| {
            let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
            emit(InstallPhase::Downloading, pct, "downloading");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting");
    archive::extract(
        &dl,
        archive::kind_for(file),
        &staged_package,
        cmd_basename(&target.cmd),
    )?;
    let staged_cmd = resolved_cmd_path(&staged_package, target, file)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&staged_cmd) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&staged_cmd, perm);
        }
    }
    if !staged_cmd.exists() {
        return Err(format!(
            "acp install: executable {} missing after extract (cmd={})",
            staged_cmd.display(),
            target.cmd
        ));
    }

    store::write_receipt_in(
        &staged_package,
        &store::Receipt {
            name: name.clone(),
            version: agent.version.clone(),
            source_id: format!("acp:{}", agent.id),
            bin: Default::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    store::activate_package(root, name, &staged_package).map_err(|error| error.to_string())?;
    emit(InstallPhase::Done, Some(100), "installed");
    Ok(())
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

    fn npx_agent(id: &str) -> RegistryAgent {
        RegistryAgent {
            id: id.to_string(),
            name: id.to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            icon: None,
            repository: None,
            distribution: acp_registry::Distribution {
                binary: None,
                npx: Some(acp_registry::PackageDist {
                    package: format!("@example/{id}"),
                    args: vec![],
                    env: Default::default(),
                }),
                uvx: None,
            },
        }
    }

    #[test]
    fn package_spec_pins_version_when_present() {
        assert_eq!(package_spec("@scope/pkg", None), "@scope/pkg");
        assert_eq!(
            package_spec("@scope/pkg", Some("1.2.3")),
            "@scope/pkg@1.2.3"
        );
        assert_eq!(package_spec("pkg", Some("  ")), "pkg");
        assert_eq!(package_spec("pkg", Some("1.0.0")), "pkg@1.0.0");
    }

    #[test]
    fn package_spec_replaces_a_baked_registry_version() {
        assert_eq!(
            package_spec("@scope/pkg@1.1.9", Some("1.1.8")),
            "@scope/pkg@1.1.8"
        );
        assert_eq!(package_spec("pkg@1.1.9", Some("1.1.8")), "pkg@1.1.8");
        assert_eq!(package_spec("@scope/pkg@1.1.9", None), "@scope/pkg@1.1.9");
        assert_eq!(package_base("@scope/pkg"), "@scope/pkg");
    }

    #[test]
    fn cmd_basename_strips_prefix_and_dirs() {
        assert_eq!(cmd_basename("./vibe"), "vibe");
        assert_eq!(cmd_basename("vibe"), "vibe");
        assert_eq!(cmd_basename("./bin/agent"), "agent");
    }

    #[test]
    fn archive_filename_takes_last_segment() {
        assert_eq!(
            archive_filename("https://x/y/vibe-darwin-arm64.tar.gz"),
            "vibe-darwin-arm64.tar.gz"
        );
        assert_eq!(archive_filename("https://x/y/bin.zip?token=1"), "bin.zip");
    }

    #[test]
    fn acp_registry_suffix_is_omitted_from_agent_urls() {
        assert_eq!(agent_url_id("codex-acp"), "codex");
        assert_eq!(agent_url_id("custom-acp"), "custom");
        assert_eq!(agent_url_id("mistral-vibe"), "mistral-vibe");
    }

    #[test]
    fn agent_ids_match_url_and_registry_forms() {
        assert!(agent_ids_match("codex", "codex-acp"));
        assert!(agent_ids_match("custom", "custom-acp"));
        assert!(agent_ids_match("vibe", "mistral-vibe"));
        assert!(!agent_ids_match("codex", "custom-acp"));
    }

    #[test]
    fn resolved_cmd_path_by_archive_kind() {
        let pkg = Path::new("/pkg");
        let tar = BinaryTarget {
            archive: "https://x/a.tar.gz".into(),
            cmd: "./bin/agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert_eq!(
            resolved_cmd_path(pkg, &tar, "a.tar.gz").unwrap(),
            Path::new("/pkg/bin/agent")
        );
        let gz = BinaryTarget {
            archive: "https://x/a.gz".into(),
            cmd: "./agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert_eq!(
            resolved_cmd_path(pkg, &gz, "a.gz").unwrap(),
            Path::new("/pkg/agent")
        );
        let escaping = BinaryTarget {
            archive: "https://x/a.tar.gz".into(),
            cmd: "../agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert!(resolved_cmd_path(pkg, &escaping, "a.tar.gz").is_err());
    }

    #[test]
    fn shared_node_does_not_mark_every_npx_agent_installed() {
        let root = std::env::temp_dir().join(format!(
            "vmux-acp-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let node = node_bindir(&root).unwrap().join("node");
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, b"").unwrap();
        let npx = node_cli(&root, "npx-cli.js").unwrap();
        std::fs::create_dir_all(npx.parent().unwrap()).unwrap();
        std::fs::write(npx, b"").unwrap();
        let installed = npx_agent("installed-agent");
        let available = npx_agent("available-agent");

        assert!(!is_agent_installed_at(&root, &installed));
        assert!(!is_agent_installed_at(&root, &available));

        write_agent_receipt(&root, &installed, None).unwrap();

        assert!(is_agent_installed_at(&root, &installed));
        assert!(!is_agent_installed_at(&root, &available));

        uninstall_at(&root, &installed.id).unwrap();

        assert!(!is_agent_installed_at(&root, &installed));
        assert!(node.exists());
        std::fs::remove_dir_all(root).unwrap();
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
    fn codex_environment_routes_shell_commands_through_vmux() {
        for agent_id in ["codex", "codex-acp"] {
            let environment = AcpEnvironment::from(Vec::new())
                .for_agent(agent_id)
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
                serde_json::json!([crate::runtime::cli::codex::DIRECT_ONLY_NAMESPACE])
            );
            let instructions = config["developer_instructions"].as_str().unwrap();
            assert!(instructions.contains("mcp__vmux__run"));
            assert!(instructions.contains("mcp__vmux__set_conversation_title"));
            assert!(instructions.contains("first tool of the turn"));
            assert!(instructions.contains("raw first prompt as a provisional title"));
            assert!(instructions.contains("topic materially changes"));
            assert!(instructions.contains("same-topic follow-ups"));
            assert!(instructions.contains("never needs user permission"));
            assert!(instructions.contains("mcp__vmux__browser_snapshot"));
            assert!(instructions.contains("page already visible beside you"));
        }
    }

    #[test]
    fn codex_environment_exposes_managed_namespaces() {
        let environment = AcpEnvironment::from(Vec::new())
            .for_agent("codex-acp")
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
                .for_agent(agent_id)
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
        .for_agent("mistral-vibe")
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
            .for_agent("mistral-vibe")
            .into_inner();

        assert!(environment.iter().all(|(key, _)| key != "VIBE_MCP_SERVERS"));
    }

    #[test]
    fn codex_environment_preserves_existing_configuration() {
        let environment = AcpEnvironment::from(vec![env(
            "CODEX_CONFIG",
            r#"{"model":"gpt-test","features":{"custom_feature":true,"code_mode":{"custom_setting":"keep"}}}"#,
        )])
        .for_agent("codex")
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
